//! What carries out the scheduler's effects: the connections runtime's
//! process, the conversation with it, the store the items go into, and
//! what is remembered of each account's syncing.
//!
//! Everything it needs of the running Heyo (the accounts and their
//! connectors, the secrets, how Node.js is started) it asks of a
//! [`Place`], so that a test runs it whole, with the real runtime and a
//! fake connector, and no application.
//!
//! The rules it keeps beside the scheduler's:
//!
//! - The process is started when something is to run in it, and ended
//!   when the last account is removed. When it ends by itself, what ran in
//!   it failed (`crashed`) and the watches start again in a new one; one
//!   that ends within seconds of starting is not started again at once.
//! - Items are written only into the part of an account that is being
//!   synced, and a `host.items.put` is answered once they are written: the
//!   connector's cursor, kept when its sync answers, never runs ahead of
//!   what is in the store. An account removed has its items dropped after
//!   any write in progress, and none can follow.
//! - A sync may take 10 minutes and anything else asked 60 seconds; then
//!   it is stopped and failed as `timeout`.
//! - A sync from no cursor (the first, or after the account was edited)
//!   brings everything: what it did not bring is dropped when it finishes.

use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;
use std::process::Command;
use std::sync::{mpsc, Arc, Condvar, Mutex, Weak};
use std::time::{Duration, Instant};

use chrono::Utc;
use serde::Serialize;
use serde_json::Value;
use ts_rs::TS;

use super::accounts::{Kept, Kind, Synced, Trouble};
use super::items::{Found, Items};
use super::scheduler::{Effect, ErrorKind, Input, Limits, Phase, Plan, Scheduler, Time};
use crate::helper::Helper;
use crate::wire::{ConnectorAccount, ConnectorCall, ConnectorError, ConnectorRequest, ITEMS_AT_ONCE};

/// What the shell asks of the running Heyo.
pub trait Place: Send + Sync + 'static {
    /// The account as its connector is given it, and the connector; none
    /// if either is gone.
    fn account(&self, id: &str) -> Option<(ConnectorAccount, Kind)>;
    /// A secret field of the account.
    fn secret(&self, account: &str, name: &str) -> Option<String>;
    /// Node.js running the connections runtime.
    fn runtime(&self) -> Result<Command, String>;
}

/// The numbers it goes by, and where it remembers the syncing.
#[derive(Debug, Clone)]
pub struct Setup {
    pub limits: Limits,
    pub sync_timeout: Duration,
    pub request_timeout: Duration,
    /// `connections.json`; none for a test that keeps nothing.
    pub kept: Option<PathBuf>,
}

impl Default for Setup {
    fn default() -> Setup {
        Setup { limits: Limits::default(), sync_timeout: Duration::from_secs(600), request_timeout: Duration::from_secs(60), kept: None }
    }
}

/// How long a search of the store may take before the palette is answered
/// without it.
pub const SEARCH_BUDGET: Duration = Duration::from_millis(50);

/// A process that lived less than this is not started again before as long.
const SHORT_LIFE: Duration = Duration::from_secs(5);

/// What a window is told of an account's syncing.
#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../shared/bindings/", rename = "ConnectionState")]
pub struct State {
    /// `idle`, `syncing`, `waiting` (for the user, whose credentials were
    /// refused).
    #[ts(type = "\"idle\" | \"syncing\" | \"waiting\"")]
    pub phase: String,
    /// When the sync running began.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string")]
    pub since: Option<Time>,
    /// When it is synced next.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string")]
    pub next: Option<Time>,
    /// When a sync last finished.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string")]
    pub last_sync: Option<Time>,
    /// What went wrong the last time, if it did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<Trouble>,
    /// How many of its items are in the store.
    pub items: u32,
    /// How far the sync running is, from 0 to 1, and what it is doing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub progress: Option<Progress>,
}

#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize, TS)]
#[ts(export, export_to = "../../shared/bindings/", rename = "ConnectionProgress")]
pub struct Progress {
    pub fraction: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub text: Option<String>,
}

/// What a search found, and whether it is all it would have found: a
/// search that took longer than [`SEARCH_BUDGET`] is answered without.
#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize, TS)]
#[ts(export, export_to = "../../shared/bindings/", rename = "ConnectionsFound")]
pub struct Searched {
    pub found: Vec<Found>,
    pub complete: bool,
}

/// What a request of the host's waits for.
enum Waiting {
    Sync { account: String, full: bool, started: Time },
    Watch { account: String },
    Check(mpsc::Sender<Result<(), ConnectorError>>),
    /// A `connector.stop`, whose answer nothing needs.
    Stop,
}

struct Pending {
    waiting: Waiting,
    deadline: Option<Instant>,
}

struct Running {
    helper: Helper,
    generation: u64,
    started: Instant,
}

struct Inner {
    scheduler: Scheduler,
    kept: Kept,
    process: Option<Running>,
    generations: u64,
    /// When the last process ended, if it lived only a moment.
    died_young: Option<Instant>,
    requests: u64,
    pending: BTreeMap<u64, Pending>,
    wake_at: Option<Time>,
    progress: BTreeMap<String, Progress>,
    over: bool,
}

/// The connections, running.
pub struct Shell {
    place: Box<dyn Place>,
    items: Arc<Items>,
    setup: Setup,
    inner: Mutex<Inner>,
    woken: Condvar,
    /// Held while items are written or dropped: an account removed has
    /// its items dropped after a write in progress, not before.
    writing: Mutex<()>,
}

fn now() -> Time {
    Utc::now()
}

impl Shell {
    /// Starts it, with nothing scheduled yet (`begin`).
    pub fn start(place: impl Place, items: Arc<Items>, setup: Setup) -> Arc<Shell> {
        let inner = Inner {
            scheduler: Scheduler::new(setup.limits.clone()),
            kept: Kept::load(setup.kept.clone()),
            process: None,
            generations: 0,
            died_young: None,
            requests: 0,
            pending: BTreeMap::new(),
            wake_at: None,
            progress: BTreeMap::new(),
            over: false,
        };
        let shell = Arc::new(Shell { place: Box::new(place), items, setup, inner: Mutex::new(inner), woken: Condvar::new(), writing: Mutex::new(()) });
        let weak = Arc::downgrade(&shell);
        std::thread::Builder::new().name("connections".into()).spawn(move || drive(weak)).expect("a thread");
        shell
    }

    /// Schedules the accounts there are when Heyo starts, from where they
    /// were left.
    pub fn begin(self: &Arc<Self>, accounts: &[(String, Plan)]) {
        let mut inner = self.lock();
        for (account, plan) in accounts {
            let past = inner.kept.accounts.get(account).map(Synced::past).unwrap_or_default();
            self.feed(&mut inner, Input::Added { account: account.clone(), plan: *plan, past });
        }
    }

    pub fn added(self: &Arc<Self>, account: &str, plan: Plan) {
        let mut inner = self.lock();
        inner.kept.accounts.remove(account);
        self.feed(&mut inner, Input::Added { account: account.into(), plan, past: Default::default() });
    }

    /// Its settings changed: it syncs everything again from them.
    pub fn edited(self: &Arc<Self>, account: &str, plan: Plan) {
        let mut inner = self.lock();
        if let Some(kept) = inner.kept.accounts.get_mut(account) {
            (kept.cursor, kept.error, kept.failures) = (None, None, 0);
        }
        inner.kept.save();
        self.feed(&mut inner, Input::Edited { account: account.into(), plan });
    }

    /// It is gone: its sync and watch are stopped, its items dropped and
    /// what was remembered of it forgotten. The process ends with the last.
    pub fn removed(self: &Arc<Self>, account: &str) {
        {
            let mut inner = self.lock();
            self.feed(&mut inner, Input::Removed { account: account.into() });
            inner.kept.accounts.remove(account);
            inner.kept.save();
            inner.progress.remove(account);
            if inner.scheduler.is_empty() {
                // Closing its input ends it; what it still says is nobody's.
                inner.process = None;
                inner.pending.clear();
            }
        }
        let _writing = self.writing.lock().unwrap_or_else(|e| e.into_inner());
        if let Err(e) = self.items.drop_account(account) {
            eprintln!("heyo: could not drop the items of {account}: {e}");
        }
    }

    pub fn sync_now(self: &Arc<Self>, account: &str) {
        let mut inner = self.lock();
        self.feed(&mut inner, Input::SyncNow { account: account.into() });
    }

    /// Whether the account works as it is set up, as its connector says.
    /// Blocks until it has said, or for as long as a request may take.
    pub fn check(self: &Arc<Self>, account: &str) -> Result<(), ConnectorError> {
        let (told, answer) = mpsc::channel();
        {
            let mut inner = self.lock();
            let Some((account, _)) = self.place.account(account) else {
                return Err(ConnectorError::new(ErrorKind::Failed, crate::i18n::tf("There is no connection {0}.", &[&account])));
            };
            let deadline = Instant::now() + self.setup.request_timeout;
            self.ask(&mut inner, ConnectorCall::Check { account }, Waiting::Check(told), Some(deadline)).map_err(|why| ConnectorError::new(ErrorKind::Failed, why))?;
        }
        answer.recv_timeout(self.setup.request_timeout + Duration::from_secs(1)).unwrap_or_else(|_| Err(timed_out("check")))
    }

    /// What a window is told of the account's syncing.
    pub fn state(&self, account: &str) -> State {
        let inner = self.lock();
        let shown = inner.scheduler.shown(account);
        let kept = inner.kept.accounts.get(account);
        let phase = match shown.map(|shown| shown.phase) {
            Some(Phase::Syncing { .. } | Phase::Restarting) => "syncing",
            Some(Phase::WaitsForUser) => "waiting",
            _ => "idle",
        };
        State {
            phase: phase.into(),
            since: match shown.map(|shown| shown.phase) {
                Some(Phase::Syncing { since }) => Some(since),
                _ => None,
            },
            next: shown.and_then(|shown| shown.next),
            last_sync: kept.and_then(|kept| kept.last_sync),
            error: kept.and_then(|kept| kept.error.clone()),
            items: self.items.count(account).unwrap_or(0) as u32,
            progress: inner.progress.get(account).cloned(),
        }
    }

    /// The items `query` finds, within [`SEARCH_BUDGET`].
    pub fn search(&self, query: &str, limit: usize) -> Searched {
        let (told, answer) = mpsc::channel();
        let (items, query) = (self.items.clone(), query.to_owned());
        std::thread::spawn(move || {
            let _ = told.send(items.search(&query, limit, chrono::Local::now()));
        });
        match answer.recv_timeout(SEARCH_BUDGET) {
            Ok(Ok(found)) => Searched { found, complete: true },
            Ok(Err(e)) => {
                eprintln!("heyo: searching the connections: {e}");
                Searched { found: Vec::new(), complete: true }
            }
            Err(_) => Searched { found: Vec::new(), complete: false },
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Gives the scheduler `input`, and carries out what it calls for and
    /// what that leads to.
    fn feed(self: &Arc<Self>, inner: &mut Inner, input: Input) {
        let mut inputs = VecDeque::from([input]);
        while let Some(input) = inputs.pop_front() {
            for effect in inner.scheduler.on(input, now()) {
                if let Some(next) = self.carry_out(inner, effect) {
                    inputs.push_back(next);
                }
            }
        }
        self.woken.notify_all();
    }

    fn carry_out(self: &Arc<Self>, inner: &mut Inner, effect: Effect) -> Option<Input> {
        match effect {
            Effect::Sync(account) => {
                let Some((wire, _)) = self.place.account(&account) else {
                    return Some(failed(inner, &account, ConnectorError::new(ErrorKind::Failed, "Its connector is not installed.")));
                };
                let started = now();
                let kept = inner.kept.accounts.entry(account.clone()).or_default();
                kept.last_attempt = Some(started);
                let cursor = kept.cursor.clone();
                inner.kept.save();
                let waiting = Waiting::Sync { account: account.clone(), full: cursor.is_none(), started };
                let deadline = Instant::now() + self.setup.sync_timeout;
                if let Err(why) = self.ask(inner, ConnectorCall::Sync { account: wire, cursor }, waiting, Some(deadline)) {
                    return Some(failed(inner, &account, ConnectorError::new(ErrorKind::Crashed, why)));
                }
                None
            }
            Effect::Stop(account) => {
                let request = inner.pending.iter().find(|(_, pending)| matches!(&pending.waiting, Waiting::Sync { account: of, .. } if *of == account)).map(|(id, _)| *id);
                if let Some(request) = request {
                    self.stop(inner, request);
                }
                None
            }
            Effect::Watch(account) => {
                let Some((wire, _)) = self.place.account(&account) else { return Some(Input::WatchEnded { account }) };
                match self.ask(inner, ConnectorCall::Watch { account: wire }, Waiting::Watch { account: account.clone() }, None) {
                    Ok(_) => None,
                    Err(why) => {
                        eprintln!("heyo: could not watch {account}: {why}");
                        Some(Input::WatchEnded { account })
                    }
                }
            }
            Effect::Unwatch(account) => {
                let request = inner.pending.iter().find(|(_, pending)| matches!(&pending.waiting, Waiting::Watch { account: of } if *of == account)).map(|(id, _)| *id);
                if let Some(request) = request {
                    inner.pending.remove(&request);
                    self.stop(inner, request);
                }
                None
            }
            Effect::WakeAt(at) => {
                inner.wake_at = Some(at);
                None
            }
        }
    }

    /// Tells the runtime to stop the request, if it is running.
    fn stop(self: &Arc<Self>, inner: &mut Inner, request: u64) {
        if inner.process.is_some() {
            let deadline = Some(Instant::now() + self.setup.request_timeout);
            let _ = self.ask(inner, ConnectorCall::Stop { request }, Waiting::Stop, deadline);
        }
    }

    /// Asks the runtime something, starting it if it is not running; says
    /// the request's number.
    fn ask(self: &Arc<Self>, inner: &mut Inner, call: ConnectorCall, waiting: Waiting, deadline: Option<Instant>) -> Result<u64, String> {
        if inner.process.is_none() {
            self.spawn(inner)?;
        }
        inner.requests += 1;
        let id = inner.requests;
        let mut message = serde_json::to_value(&call).expect("plain data");
        message["type"] = "request".into();
        message["id"] = id.into();
        let running = inner.process.as_mut().expect("started");
        if let Err(e) = running.helper.send(&message) {
            inner.process = None;
            return Err(format!("the connections runtime is not running: {e}"));
        }
        inner.pending.insert(id, Pending { waiting, deadline });
        Ok(id)
    }

    fn spawn(self: &Arc<Self>, inner: &mut Inner) -> Result<(), String> {
        if inner.died_young.is_some_and(|at| at.elapsed() < SHORT_LIFE) {
            return Err("the connections runtime ended as it started; not starting it again yet".into());
        }
        inner.generations += 1;
        let generation = inner.generations;
        let command = self.place.runtime()?;
        let (hearing, ending) = (Arc::downgrade(self), Arc::downgrade(self));
        let helper = Helper::start(
            command,
            move |message| {
                if let Some(shell) = hearing.upgrade() {
                    shell.heard(generation, message);
                }
            },
            move || {
                if let Some(shell) = ending.upgrade() {
                    shell.ended(generation);
                }
            },
        )
        .map_err(|e| format!("failed to start the connections runtime: {e}"))?;
        inner.process = Some(Running { helper, generation, started: Instant::now() });
        Ok(())
    }

    /// The process of that generation ended.
    fn ended(self: &Arc<Self>, generation: u64) {
        let mut inner = self.lock();
        let Some(running) = inner.process.take_if(|running| running.generation == generation) else { return };
        if inner.over {
            return;
        }
        if running.started.elapsed() < SHORT_LIFE {
            inner.died_young = Some(Instant::now());
        }
        eprintln!("heyo: the connections runtime ended");
        for (_, pending) in std::mem::take(&mut inner.pending) {
            match pending.waiting {
                Waiting::Check(told) => drop(told.send(Err(ConnectorError::new(ErrorKind::Crashed, "The connections runtime ended.")))),
                Waiting::Sync { account, .. } => drop(record(&mut inner, &account, &ConnectorError::new(ErrorKind::Crashed, "The connections runtime ended."))),
                Waiting::Watch { .. } | Waiting::Stop => {}
            }
        }
        inner.progress.clear();
        inner.kept.save();
        self.feed(&mut inner, Input::Crashed);
    }

    /// A message of the process of that generation.
    fn heard(self: &Arc<Self>, generation: u64, message: Value) {
        match message["type"].as_str() {
            Some("reply") => self.answered(generation, &message),
            Some("request") => {
                let id = message["id"].clone();
                let answer = match serde_json::from_value::<ConnectorRequest>(message.clone()) {
                    Ok(request) => self.answer(generation, request),
                    Err(e) => Err(format!("the host cannot answer {}: {e}", message["method"])),
                };
                let reply = crate::runtime::reply(id, answer);
                let mut inner = self.lock();
                if let Some(running) = inner.process.as_mut().filter(|running| running.generation == generation) {
                    let _ = running.helper.send(&reply);
                }
            }
            _ => {}
        }
    }

    /// The runtime answered a request of the host's.
    fn answered(self: &Arc<Self>, generation: u64, message: &Value) {
        let mut inner = self.lock();
        if inner.process.as_ref().is_none_or(|running| running.generation != generation) {
            return;
        }
        let Some(pending) = message["id"].as_u64().and_then(|id| inner.pending.remove(&id)) else { return };
        let outcome: Result<Value, ConnectorError> = match message.get("error").filter(|error| !error.is_null()) {
            Some(error) => Err(serde_json::from_value(error.clone()).unwrap_or_else(|_| ConnectorError::new(ErrorKind::Failed, error.to_string()))),
            None => Ok(message["result"].clone()),
        };
        let next = match pending.waiting {
            Waiting::Stop => None,
            Waiting::Check(told) => {
                let _ = told.send(outcome.map(drop));
                None
            }
            Waiting::Watch { account } => Some(Input::WatchEnded { account }),
            Waiting::Sync { account, full, started } => match outcome.and_then(|result| {
                result["cursor"].as_str().map(str::to_owned).ok_or_else(|| ConnectorError::new(ErrorKind::Failed, "The connector's sync did not answer with a cursor."))
            }) {
                Ok(cursor) => Some(self.synced(&mut inner, &account, cursor, full, started)),
                Err(error) => Some(failed(&mut inner, &account, error)),
            },
        };
        if let Some(next) = next {
            self.feed(&mut inner, next);
        }
    }

    fn synced(&self, inner: &mut Inner, account: &str, cursor: String, full: bool, started: Time) -> Input {
        inner.progress.remove(account);
        // Stopped meanwhile (edited, removed): what it came to is not kept.
        if inner.scheduler.syncing(account) {
            if full {
                let _writing = self.writing.lock().unwrap_or_else(|e| e.into_inner());
                if let Err(e) = self.items.prune(account, started) {
                    eprintln!("heyo: could not prune the items of {account}: {e}");
                }
            }
            let kept = inner.kept.accounts.entry(account.into()).or_default();
            (kept.cursor, kept.last_sync, kept.failures, kept.error) = (Some(cursor), Some(now()), 0, None);
            inner.kept.save();
        }
        Input::Synced { account: account.into() }
    }

    /// Answers a request of the runtime's.
    fn answer(self: &Arc<Self>, generation: u64, request: ConnectorRequest) -> Result<Value, String> {
        match request {
            ConnectorRequest::ItemsPut { account, items } => self.write(generation, &account, items.len(), || self.items.put(&account, &items)),
            ConnectorRequest::ItemsRemove { account, ids } => self.write(generation, &account, ids.len(), || self.items.remove(&account, &ids)),
            ConnectorRequest::Secret { account, name } => {
                let (_, kind) = self.place.account(&account).ok_or_else(|| format!("there is no connection {account}"))?;
                if !kind.secret(&name) {
                    return Err(format!("{name} is not a secret field of {}", kind.title));
                }
                Ok(self.place.secret(&account, &name).map_or(Value::Null, Value::from))
            }
            ConnectorRequest::Changed { account } => {
                let mut inner = self.lock();
                self.feed(&mut inner, Input::Changed { account });
                Ok(Value::Null)
            }
            ConnectorRequest::Progress { account, fraction, text } => {
                let mut inner = self.lock();
                if inner.scheduler.syncing(&account) {
                    inner.progress.insert(account, Progress { fraction: fraction.clamp(0.0, 1.0), text });
                }
                Ok(Value::Null)
            }
        }
    }

    /// Writes to the store for an account that is being synced, and says
    /// so once it is written.
    fn write(&self, generation: u64, account: &str, count: usize, write: impl FnOnce() -> Result<(), String>) -> Result<Value, String> {
        if count > ITEMS_AT_ONCE {
            return Err(format!("at most {ITEMS_AT_ONCE} items are written at once, not {count}"));
        }
        let _writing = self.writing.lock().unwrap_or_else(|e| e.into_inner());
        {
            let inner = self.lock();
            let ours = inner.process.as_ref().is_some_and(|running| running.generation == generation);
            if !ours || !inner.scheduler.syncing(account) {
                return Err(format!("{account} is not being synced"));
            }
        }
        write().map(|()| Value::Null)
    }

    /// Whatever is due: the scheduler's next time, and the requests that
    /// took too long.
    fn wake(self: &Arc<Self>) {
        let mut inner = self.lock();
        let late: Vec<u64> = inner.pending.iter().filter(|(_, pending)| pending.deadline.is_some_and(|at| at <= Instant::now())).map(|(id, _)| *id).collect();
        for id in late {
            let Some(pending) = inner.pending.remove(&id) else { continue };
            match pending.waiting {
                Waiting::Stop => {}
                Waiting::Watch { .. } => {}
                Waiting::Check(told) => drop(told.send(Err(timed_out("check")))),
                Waiting::Sync { account, .. } => {
                    self.stop(&mut inner, id);
                    let input = failed(&mut inner, &account, timed_out("sync"));
                    self.feed(&mut inner, input);
                }
            }
        }
        if inner.wake_at.is_some_and(|at| at <= now()) {
            inner.wake_at = None;
            self.feed(&mut inner, Input::Tick);
        }
    }
}

impl Drop for Shell {
    fn drop(&mut self) {
        self.lock().over = true;
    }
}

fn timed_out(what: &str) -> ConnectorError {
    ConnectorError::new(ErrorKind::Timeout, format!("The connector's {what} took too long."))
}

/// Notes what went wrong, and says it to the scheduler.
fn failed(inner: &mut Inner, account: &str, error: ConnectorError) -> Input {
    inner.progress.remove(account);
    record(inner, account, &error);
    inner.kept.save();
    Input::Failed { account: account.into(), error: error.kind }
}

fn record(inner: &mut Inner, account: &str, error: &ConnectorError) -> Input {
    // Being stopped is nothing that went wrong; nor is anything of an
    // account that is gone.
    if error.kind != ErrorKind::Cancelled && inner.scheduler.syncing(account) {
        let kept = inner.kept.accounts.entry(account.into()).or_default();
        kept.failures += 1;
        kept.error = Some(Trouble { kind: error.kind, message: error.message.clone() });
    }
    Input::Failed { account: account.into(), error: error.kind }
}

/// The shell's own thread: it sleeps until the next thing is due.
fn drive(shell: Weak<Shell>) {
    loop {
        let Some(strong) = shell.upgrade() else { return };
        strong.wake();
        let inner = strong.lock();
        let until_wake = inner.wake_at.map(|at| (at - now()).to_std().unwrap_or_default());
        let until_deadline = inner.pending.values().filter_map(|pending| pending.deadline).min().map(|at| at.saturating_duration_since(Instant::now()));
        // At most a second: the clock may jump, and the shell may be gone.
        let wait = [until_wake, until_deadline].into_iter().flatten().min().unwrap_or(Duration::MAX).min(Duration::from_secs(1));
        let _ = strong.woken.wait_timeout(inner, wait);
    }
}

#[cfg(test)]
mod tests;

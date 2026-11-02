//! When each connection's account is synced and watched.
//!
//! The [`Scheduler`] is plain data with the time passed in, in the manner
//! of `activities::Table` and `dictation::session`: it takes an [`Input`]
//! (time has passed, an account was added, its watch saw a change, the
//! user asked for a sync, a sync finished or failed...) and answers with
//! the [`Effect`]s it calls for (start a sync, stop one, start or stop a
//! watch, wake me at this time). The shell in `connections` carries them
//! out and feeds back what came of them. So the rules are tested here in
//! tables, without a process or a clock:
//!
//! - an account is synced every 15 minutes, or as often as its connector
//!   says (`heyo.sync.every`);
//! - what its watch reports starts a sync, at most once a minute;
//! - a sync that failed is tried again after 1, 5, 15 and then every 60
//!   minutes; one whose credentials were refused waits for the user (a
//!   sync asked for, or the account edited);
//! - at most three accounts sync at once, and an account only one sync at
//!   a time: what asks for another meanwhile has it run after;
//! - editing an account stops its sync and starts it again from the new
//!   settings, and restarts its watch.

use std::collections::BTreeMap;

use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub type Time = DateTime<Utc>;

/// The numbers the rules go by. Tests of the whole shrink them.
#[derive(Debug, Clone, PartialEq)]
pub struct Limits {
    /// How often an account is synced when its connector does not say.
    pub every: TimeDelta,
    /// The least time between the starts of two syncs a watch asked for.
    pub change_gap: TimeDelta,
    /// How long after the first, second, third and every later failure in
    /// a row a sync is tried again.
    pub backoff: [TimeDelta; 4],
    /// How many accounts sync at once.
    pub parallel: usize,
    /// How long after a watch ended by itself it is started again.
    pub watch_retry: TimeDelta,
}

impl Default for Limits {
    fn default() -> Limits {
        let minutes = TimeDelta::minutes;
        Limits { every: minutes(15), change_gap: minutes(1), backoff: [minutes(1), minutes(5), minutes(15), minutes(60)], parallel: 3, watch_retry: minutes(1) }
    }
}

/// What a connector says of how its accounts are synced (`heyo.sync`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Plan {
    /// How often; the default (`Limits::every`) when it does not say.
    pub every: Option<TimeDelta>,
    /// It has a `watch` that reports changes as they happen.
    pub watch: bool,
}

/// Why a sync did not finish.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export, export_to = "../../shared/bindings/", rename = "ConnectorErrorKind")]
pub enum ErrorKind {
    /// The service refused the credentials: the user has to see to them.
    Auth,
    /// The service could not be reached, or asked to be left alone for now.
    Unavailable,
    /// Anything else that went wrong in the connector.
    Failed,
    /// It took longer than a sync may.
    Timeout,
    /// The process that ran it ended.
    Crashed,
    /// Heyo stopped it (the account was edited or removed).
    Cancelled,
}

/// What is remembered of an account's syncs from before Heyo started.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Past {
    /// When the last sync started.
    pub last_attempt: Option<Time>,
    /// How many failed in a row since the last that finished.
    pub failures: u32,
    /// What the last failure was, if the last sync failed.
    pub error: Option<ErrorKind>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Input {
    /// Time has passed: whatever is due starts.
    Tick,
    Added { account: String, plan: Plan, past: Past },
    /// Its settings or its secrets changed.
    Edited { account: String, plan: Plan },
    Removed { account: String },
    /// Its watch saw something change.
    Changed { account: String },
    /// The user asked for a sync now.
    SyncNow { account: String },
    /// Its sync finished; the cursor it came to is the shell's to keep.
    Synced { account: String },
    Failed { account: String, error: ErrorKind },
    /// Its watch ended by itself (returned or threw).
    WatchEnded { account: String },
    /// The process syncs and watches ran in ended: none of them is running.
    Crashed,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// Start syncing the account from the cursor kept for it: `Synced` or
    /// `Failed` comes.
    Sync(String),
    /// Stop its sync: `Failed` with `Cancelled` comes.
    Stop(String),
    /// Start its watch: `Changed` comes as often as it sees a change, and
    /// `WatchEnded` if it ends.
    Watch(String),
    Unwatch(String),
    /// Send `Tick` at this time; it replaces any time asked for before.
    WakeAt(Time),
}

/// How far an account is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Waiting for its next sync (`due`).
    Idle,
    Syncing { since: Time },
    /// Told to stop, and to start again when it has (it was edited).
    Restarting,
    /// Its credentials were refused: nothing happens until the user acts.
    WaitsForUser,
}

#[derive(Debug, Clone)]
struct Account {
    plan: Plan,
    phase: Phase,
    last_attempt: Option<Time>,
    failures: u32,
    /// When it is synced next, while it is `Idle`.
    due: Option<Time>,
    /// Asked for another sync while one ran: it runs after.
    again: bool,
    watching: bool,
    /// When its watch is started again.
    watch_due: Option<Time>,
}

/// What a window is told of an account's syncing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shown {
    pub phase: Phase,
    /// When it is synced next, if it is waiting for that.
    pub next: Option<Time>,
    pub failures: u32,
}

/// Every account's syncing.
#[derive(Debug, Default)]
pub struct Scheduler {
    limits: Limits,
    accounts: BTreeMap<String, Account>,
    /// The wake-up last asked for, so it is asked for once.
    wake: Option<Time>,
}

impl Scheduler {
    pub fn new(limits: Limits) -> Scheduler {
        Scheduler { limits, ..Scheduler::default() }
    }

    /// Takes `input` at `now`.
    pub fn on(&mut self, input: Input, now: Time) -> Vec<Effect> {
        let mut effects = Vec::new();
        match input {
            Input::Tick => {}
            Input::Added { account, plan, past } => self.add(account, plan, past, now, &mut effects),
            Input::Edited { account, plan } => self.edit(&account, plan, now, &mut effects),
            Input::Removed { account } => {
                if let Some(gone) = self.accounts.remove(&account) {
                    if matches!(gone.phase, Phase::Syncing { .. }) {
                        effects.push(Effect::Stop(account.clone()));
                    }
                    if gone.watching {
                        effects.push(Effect::Unwatch(account));
                    }
                }
            }
            Input::Changed { account } => self.changed(&account, now),
            Input::SyncNow { account } => {
                if let Some(it) = self.accounts.get_mut(&account) {
                    match it.phase {
                        Phase::Syncing { .. } | Phase::Restarting => it.again = true,
                        Phase::Idle | Phase::WaitsForUser => (it.phase, it.due) = (Phase::Idle, Some(now)),
                    }
                }
            }
            Input::Synced { account } => self.synced(&account, now, &mut effects),
            Input::Failed { account, error } => self.failed(&account, error, now, &mut effects),
            Input::WatchEnded { account } => {
                let retry = self.limits.watch_retry;
                if let Some(it) = self.accounts.get_mut(&account) {
                    it.watching = false;
                    if it.plan.watch && it.phase != Phase::WaitsForUser {
                        it.watch_due = Some(now + retry);
                    }
                }
            }
            Input::Crashed => {
                let names: Vec<String> = self.accounts.keys().cloned().collect();
                for account in names {
                    let it = self.accounts.get_mut(&account).expect("listed");
                    if it.watching {
                        it.watching = false;
                        it.watch_due = Some(now);
                    }
                    match it.phase {
                        Phase::Syncing { .. } => self.failed(&account, ErrorKind::Crashed, now, &mut effects),
                        Phase::Restarting => self.failed(&account, ErrorKind::Cancelled, now, &mut effects),
                        _ => {}
                    }
                }
            }
        }
        self.start_due(now, &mut effects);
        self.ask_wake(now, &mut effects);
        effects
    }

    /// What a window is told of an account.
    pub fn shown(&self, account: &str) -> Option<Shown> {
        self.accounts.get(account).map(|it| Shown { phase: it.phase, next: if it.phase == Phase::Idle { it.due } else { None }, failures: it.failures })
    }

    /// Whether the account is being synced: only then may items be put
    /// into its part of the store.
    pub fn syncing(&self, account: &str) -> bool {
        self.accounts.get(account).is_some_and(|it| matches!(it.phase, Phase::Syncing { .. }))
    }

    pub fn is_empty(&self) -> bool {
        self.accounts.is_empty()
    }

    fn every(&self, plan: Plan) -> TimeDelta {
        plan.every.unwrap_or(self.limits.every)
    }

    fn backoff(&self, failures: u32) -> TimeDelta {
        self.limits.backoff[(failures.max(1) as usize - 1).min(self.limits.backoff.len() - 1)]
    }

    fn add(&mut self, account: String, plan: Plan, past: Past, now: Time, effects: &mut Vec<Effect>) {
        if self.accounts.contains_key(&account) {
            return self.edit(&account, plan, now, effects);
        }
        let waits = past.error == Some(ErrorKind::Auth);
        let due = match past.last_attempt {
            _ if waits => None,
            None => Some(now),
            Some(last) if past.failures > 0 => Some(last + self.backoff(past.failures)),
            Some(last) => Some(last + self.every(plan)),
        };
        let watching = plan.watch && !waits;
        if watching {
            effects.push(Effect::Watch(account.clone()));
        }
        let phase = if waits { Phase::WaitsForUser } else { Phase::Idle };
        self.accounts.insert(account, Account { plan, phase, last_attempt: past.last_attempt, failures: past.failures, due, again: false, watching, watch_due: None });
    }

    fn edit(&mut self, account: &str, plan: Plan, now: Time, effects: &mut Vec<Effect>) {
        let Some(it) = self.accounts.get_mut(account) else { return };
        it.plan = plan;
        it.failures = 0;
        match it.phase {
            Phase::Syncing { .. } => {
                effects.push(Effect::Stop(account.into()));
                it.phase = Phase::Restarting;
            }
            Phase::Restarting => {}
            Phase::Idle | Phase::WaitsForUser => (it.phase, it.due) = (Phase::Idle, Some(now)),
        }
        // Its watch was started with what it was before.
        if it.watching {
            effects.push(Effect::Unwatch(account.into()));
            it.watching = false;
        }
        it.watch_due = None;
        if plan.watch {
            effects.push(Effect::Watch(account.into()));
            it.watching = true;
        }
    }

    fn changed(&mut self, account: &str, now: Time) {
        let gap = self.limits.change_gap;
        let Some(it) = self.accounts.get_mut(account) else { return };
        match it.phase {
            Phase::Syncing { .. } | Phase::Restarting => it.again = true,
            // A change does not hurry a sync that failed: it waits as it was.
            Phase::WaitsForUser => {}
            Phase::Idle if it.failures > 0 => {}
            Phase::Idle => {
                let when = it.last_attempt.map_or(now, |last| (last + gap).max(now));
                it.due = Some(it.due.map_or(when, |due| due.min(when)));
            }
        }
    }

    fn synced(&mut self, account: &str, now: Time, effects: &mut Vec<Effect>) {
        let (gap, every) = (self.limits.change_gap, self.accounts.get(account).map(|it| self.every(it.plan)));
        let Some(it) = self.accounts.get_mut(account) else { return };
        if !matches!(it.phase, Phase::Syncing { .. }) {
            return;
        }
        it.failures = 0;
        it.phase = Phase::Idle;
        let last = it.last_attempt.unwrap_or(now);
        it.due = Some(if std::mem::take(&mut it.again) { (last + gap).max(now) } else { now + every.unwrap_or(gap) });
        // A watch stopped while the credentials were refused starts again.
        if it.plan.watch && !it.watching && it.watch_due.is_none() {
            it.watching = true;
            effects.push(Effect::Watch(account.into()));
        }
    }

    fn failed(&mut self, account: &str, error: ErrorKind, now: Time, effects: &mut Vec<Effect>) {
        let failures = self.accounts.get(account).map_or(0, |it| it.failures + 1);
        let (backoff, every) = (self.backoff(failures), self.accounts.get(account).map(|it| self.every(it.plan)));
        let Some(it) = self.accounts.get_mut(account) else { return };
        let restarting = it.phase == Phase::Restarting;
        if !matches!(it.phase, Phase::Syncing { .. } | Phase::Restarting) {
            return;
        }
        let again = std::mem::take(&mut it.again);
        match error {
            // Stopped to start again: no failure of its own.
            _ if restarting => (it.phase, it.due) = (Phase::Idle, Some(now)),
            ErrorKind::Cancelled => (it.phase, it.due) = (Phase::Idle, Some(if again { now } else { now + every.unwrap_or(backoff) })),
            ErrorKind::Auth => {
                (it.phase, it.due, it.failures) = (Phase::WaitsForUser, None, failures);
                if it.watching {
                    it.watching = false;
                    effects.push(Effect::Unwatch(account.into()));
                }
                it.watch_due = None;
            }
            _ => (it.phase, it.due, it.failures) = (Phase::Idle, Some(now + backoff), failures),
        }
    }

    /// Starts what is due, the longest waiting first, as far as there is room.
    fn start_due(&mut self, now: Time, effects: &mut Vec<Effect>) {
        for (account, it) in self.accounts.iter_mut() {
            if it.watch_due.is_some_and(|due| due <= now) {
                it.watch_due = None;
                it.watching = true;
                effects.push(Effect::Watch(account.clone()));
            }
        }
        let running = self.accounts.values().filter(|it| matches!(it.phase, Phase::Syncing { .. } | Phase::Restarting)).count();
        let mut due: Vec<(Time, String)> = self
            .accounts
            .iter()
            .filter(|(_, it)| it.phase == Phase::Idle)
            .filter_map(|(account, it)| Some((it.due.filter(|due| *due <= now)?, account.clone())))
            .collect();
        due.sort();
        for (_, account) in due.into_iter().take(self.limits.parallel.saturating_sub(running)) {
            let it = self.accounts.get_mut(&account).expect("listed");
            (it.phase, it.due, it.last_attempt) = (Phase::Syncing { since: now }, None, Some(now));
            effects.push(Effect::Sync(account));
        }
    }

    /// Asks to be woken when the next thing is due, if that changed. What
    /// is due already waits for a sync to finish, not for the time.
    fn ask_wake(&mut self, now: Time, effects: &mut Vec<Effect>) {
        let next = self.accounts.values().flat_map(|it| [if it.phase == Phase::Idle { it.due } else { None }, it.watch_due]).flatten().filter(|at| *at > now).min();
        if next != self.wake {
            self.wake = next;
            if let Some(at) = next {
                effects.push(Effect::WakeAt(at));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    /// A scheduler at a time of the test's.
    struct Run {
        scheduler: Scheduler,
        start: Time,
        now: Time,
    }

    impl Run {
        fn new() -> Run {
            let start = Utc.with_ymd_and_hms(2026, 11, 3, 9, 0, 0).unwrap();
            Run { scheduler: Scheduler::new(Limits::default()), start, now: start }
        }

        fn pass(&mut self, minutes: i64) {
            self.now += TimeDelta::minutes(minutes);
        }

        fn pass_seconds(&mut self, seconds: i64) {
            self.now += TimeDelta::seconds(seconds);
        }

        /// What the scheduler called for, a line each; times in minutes
        /// from the start.
        fn on(&mut self, input: Input) -> Vec<String> {
            let start = self.start;
            self.scheduler
                .on(input, self.now)
                .into_iter()
                .map(|effect| match effect {
                    Effect::Sync(a) => format!("sync {a}"),
                    Effect::Stop(a) => format!("stop {a}"),
                    Effect::Watch(a) => format!("watch {a}"),
                    Effect::Unwatch(a) => format!("unwatch {a}"),
                    Effect::WakeAt(at) => format!("wake at {}", (at - start).num_seconds() as f64 / 60.0),
                })
                .collect()
        }

        fn tick(&mut self) -> Vec<String> {
            self.on(Input::Tick)
        }

        fn add(&mut self, account: &str, watch: bool) -> Vec<String> {
            self.on(Input::Added { account: account.into(), plan: Plan { every: None, watch }, past: Past::default() })
        }
    }

    fn synced(account: &str) -> Input {
        Input::Synced { account: account.into() }
    }

    fn failed(account: &str, error: ErrorKind) -> Input {
        Input::Failed { account: account.into(), error }
    }

    fn changed(account: &str) -> Input {
        Input::Changed { account: account.into() }
    }

    #[test]
    fn a_new_account_is_synced_at_once_and_then_every_fifteen_minutes() {
        let mut run = Run::new();
        assert_eq!(run.add("mail", false), ["sync mail"]);
        assert!(run.scheduler.syncing("mail"));
        // A sync asked for while one runs runs after it.
        assert_eq!(run.on(Input::SyncNow { account: "mail".into() }), Vec::<String>::new());
        run.pass(2);
        assert_eq!(run.on(synced("mail")), ["sync mail"]);
        assert_eq!(run.on(synced("mail")), ["wake at 17"]);
        assert_eq!(run.scheduler.shown("mail").unwrap().next, Some(run.start + TimeDelta::minutes(17)));
        run.pass(14);
        assert_eq!(run.tick(), Vec::<String>::new());
        run.pass(1);
        assert_eq!(run.tick(), ["sync mail"]);
        // A connector may say how often.
        let mut run = Run::new();
        run.on(Input::Added { account: "cal".into(), plan: Plan { every: Some(TimeDelta::minutes(60)), watch: false }, past: Past::default() });
        assert_eq!(run.on(synced("cal")), ["wake at 60"]);
    }

    #[test]
    fn what_was_synced_before_heyo_started_is_not_synced_again_too_soon() {
        let mut run = Run::new();
        let past = Past { last_attempt: Some(run.now - TimeDelta::minutes(10)), failures: 0, error: None };
        assert_eq!(run.on(Input::Added { account: "mail".into(), plan: Plan::default(), past }), ["wake at 5"]);
        let failing = Past { last_attempt: Some(run.now), failures: 2, error: Some(ErrorKind::Unavailable) };
        assert_eq!(run.on(Input::Added { account: "cal".into(), plan: Plan::default(), past: failing }), Vec::<String>::new());
        assert_eq!(run.scheduler.shown("cal").unwrap().next, Some(run.now + TimeDelta::minutes(5)));
        // Refused credentials still wait for the user, and its watch with them.
        let refused = Past { last_attempt: Some(run.now), failures: 1, error: Some(ErrorKind::Auth) };
        assert_eq!(run.on(Input::Added { account: "chat".into(), plan: Plan { every: None, watch: true }, past: refused }), Vec::<String>::new());
        assert_eq!(run.scheduler.shown("chat").unwrap().phase, Phase::WaitsForUser);
    }

    #[test]
    fn a_change_starts_a_sync_at_most_once_a_minute() {
        let mut run = Run::new();
        assert_eq!(run.add("mail", true), ["watch mail", "sync mail"]);
        run.pass_seconds(10);
        assert_eq!(run.on(synced("mail")), ["wake at 15.166666666666666"]);
        // Twenty seconds after the last sync began: it waits for the minute.
        run.pass_seconds(10);
        assert_eq!(run.on(changed("mail")), ["wake at 1"]);
        assert_eq!(run.on(changed("mail")), Vec::<String>::new());
        run.pass_seconds(40);
        assert_eq!(run.tick(), ["sync mail"]);
        // A change while it syncs has another sync after, a minute after this one began.
        run.pass_seconds(5);
        assert_eq!(run.on(changed("mail")), Vec::<String>::new());
        run.pass_seconds(5);
        assert_eq!(run.on(synced("mail")), ["wake at 2"]);
        run.pass(1);
        assert_eq!(run.tick(), ["sync mail"]);
        // Long after the last, at once.
        run.on(synced("mail"));
        run.pass(5);
        assert_eq!(run.on(changed("mail")), ["sync mail"]);
    }

    #[test]
    fn a_failed_sync_is_tried_again_after_one_five_fifteen_and_sixty_minutes() {
        let mut run = Run::new();
        run.add("mail", false);
        let mut waits = Vec::new();
        for _ in 0..5 {
            let said = run.on(failed("mail", ErrorKind::Unavailable));
            let next = run.scheduler.shown("mail").unwrap().next.unwrap();
            waits.push((next - run.now).num_minutes());
            assert_eq!(said.len(), 1, "{said:?}");
            run.now = next;
            assert_eq!(run.tick(), ["sync mail"]);
        }
        assert_eq!(waits, [1, 5, 15, 60, 60]);
        assert_eq!(run.scheduler.shown("mail").unwrap().failures, 5);
        // A change does not hurry it, and one that finishes ends the counting.
        run.on(failed("mail", ErrorKind::Timeout));
        run.pass(2);
        run.on(changed("mail"));
        assert_eq!(run.scheduler.shown("mail").unwrap().next, Some(run.now + TimeDelta::minutes(58)));
        run.pass(58);
        assert_eq!(run.tick(), ["sync mail"]);
        run.on(synced("mail"));
        assert_eq!(run.scheduler.shown("mail").unwrap().failures, 0);
    }

    #[test]
    fn refused_credentials_wait_for_the_user() {
        let mut run = Run::new();
        run.add("mail", true);
        assert_eq!(run.on(failed("mail", ErrorKind::Auth)), ["unwatch mail"]);
        let shown = run.scheduler.shown("mail").unwrap();
        assert_eq!((shown.phase, shown.next), (Phase::WaitsForUser, None));
        // Neither time nor a change does anything.
        run.pass(600);
        assert_eq!(run.tick(), Vec::<String>::new());
        assert_eq!(run.on(changed("mail")), Vec::<String>::new());
        // The user asking does, and the watch comes back once it works.
        assert_eq!(run.on(Input::SyncNow { account: "mail".into() }), ["sync mail"]);
        assert_eq!(run.on(synced("mail")), ["watch mail", "wake at 615"]);
        // As does editing the account.
        run.pass(15);
        run.tick();
        run.on(failed("mail", ErrorKind::Auth));
        assert_eq!(run.on(Input::Edited { account: "mail".into(), plan: Plan { every: None, watch: true } }), ["watch mail", "sync mail"]);
    }

    #[test]
    fn at_most_three_sync_at_once_the_longest_waiting_first() {
        let mut run = Run::new();
        for account in ["a", "b", "c"] {
            assert_eq!(run.add(account, false), [format!("sync {account}")]);
        }
        run.pass(1);
        assert_eq!(run.add("d", false), Vec::<String>::new());
        run.pass(1);
        assert_eq!(run.add("e", false), Vec::<String>::new());
        assert_eq!(run.tick(), Vec::<String>::new());
        // A slot that frees goes to the one waiting longest.
        assert_eq!(run.on(synced("b")), ["sync d", "wake at 17"]);
        assert_eq!(run.on(failed("a", ErrorKind::Failed)), ["sync e", "wake at 3"]);
        assert!(["c", "d", "e"].iter().all(|account| run.scheduler.syncing(account)));
    }

    #[test]
    fn editing_an_account_stops_its_sync_and_starts_again_with_its_watch() {
        let mut run = Run::new();
        run.add("mail", true);
        let plan = Plan { every: None, watch: true };
        assert_eq!(run.on(Input::Edited { account: "mail".into(), plan }), ["stop mail", "unwatch mail", "watch mail"]);
        assert_eq!(run.scheduler.shown("mail").unwrap().phase, Phase::Restarting);
        assert!(!run.scheduler.syncing("mail"));
        // Its being stopped is no failure: it starts again at once.
        assert_eq!(run.on(failed("mail", ErrorKind::Cancelled)), ["sync mail"]);
        assert_eq!(run.scheduler.shown("mail").unwrap().failures, 0);
        // Edited while it waits, it syncs now; a watch it no longer has is stopped.
        run.on(synced("mail"));
        assert_eq!(run.on(Input::Edited { account: "mail".into(), plan: Plan::default() }), ["unwatch mail", "sync mail"]);
    }

    #[test]
    fn a_removed_account_is_stopped_and_forgotten() {
        let mut run = Run::new();
        run.add("mail", true);
        assert_eq!(run.on(Input::Removed { account: "mail".into() }), ["stop mail", "unwatch mail"]);
        assert!(run.scheduler.is_empty() && run.scheduler.shown("mail").is_none());
        // What it said after is nobody's.
        assert_eq!(run.on(failed("mail", ErrorKind::Cancelled)), Vec::<String>::new());
        assert_eq!(run.on(synced("mail")), Vec::<String>::new());
        assert_eq!(run.on(changed("mail")), Vec::<String>::new());
    }

    #[test]
    fn a_crash_fails_what_ran_and_resumes_the_watches() {
        let mut run = Run::new();
        run.add("mail", true);
        run.add("cal", false);
        run.on(synced("cal"));
        assert_eq!(run.on(Input::Crashed), ["watch mail", "wake at 1"]);
        let shown = run.scheduler.shown("mail").unwrap();
        assert_eq!((shown.phase, shown.failures), (Phase::Idle, 1));
        // A watch that ends by itself is started again a minute later.
        assert_eq!(run.on(Input::WatchEnded { account: "mail".into() }), Vec::<String>::new());
        run.pass(1);
        assert_eq!(run.tick(), ["watch mail", "sync mail", "wake at 15"]);
    }
}

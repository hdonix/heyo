//! The shell whole: the built connections runtime (`pnpm build:runtime`)
//! in Node.js, with the fake connector of `runtime/src/connections/
//! fixtures/fake`, which syncs what a JSON file of the test's says.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::TimeDelta;
use serde_json::{json, Map, Value};

use super::*;
use crate::connections::accounts::{self, kinds_in};
use crate::secrets::Owner;
use crate::settings::Settings;

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../runtime/src/connections/fixtures/fake");
const SCRIPT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../runtime/dist/heyo-connections.cjs");

/// Heyo as the shell sees it: settings in memory, and the fake connector.
struct Bench {
    settings: Arc<Mutex<Settings>>,
    kind: Kind,
}

impl Place for Bench {
    fn account(&self, id: &str) -> Option<(ConnectorAccount, Kind)> {
        let account = self.settings.lock().unwrap().connections.iter().find(|account| account.id == id).cloned()?;
        (account.connector == self.kind.id).then(|| (crate::connections::given(&account, &self.kind), self.kind.clone()))
    }

    fn secret(&self, account: &str, name: &str) -> Option<String> {
        self.settings.lock().unwrap().secrets.read(Owner::Connection(account), name).map(str::to_owned)
    }

    fn runtime(&self) -> Result<Command, String> {
        assert!(Path::new(SCRIPT).is_file(), "{SCRIPT} is not built: run pnpm build:runtime");
        let mut command = Command::new(crate::settings::node_given().unwrap_or_else(|| "node".into()));
        command.arg(SCRIPT);
        Ok(command)
    }
}

struct Test {
    shell: Arc<Shell>,
    items: Arc<Items>,
    settings: Arc<Mutex<Settings>>,
    kind: Kind,
    dir: PathBuf,
}

fn limits() -> Limits {
    let quick = TimeDelta::milliseconds(150);
    Limits { change_gap: quick, backoff: [quick; 4], watch_retry: quick, ..Limits::default() }
}

impl Test {
    fn new(name: &str, setup: Setup) -> Test {
        let dir = std::env::temp_dir().join(format!("heyo-connections-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let kind = kinds_in(Path::new(FIXTURE)).pop().expect("the fake connector");
        let settings = Arc::new(Mutex::new(Settings::default()));
        let items = Arc::new(Items::in_memory());
        let shell = Shell::start(Bench { settings: settings.clone(), kind: kind.clone() }, items.clone(), setup);
        Test { shell, items, settings, kind, dir }
    }

    fn quick(name: &str) -> Test {
        Test::new(name, Setup { limits: limits(), ..Setup::default() })
    }

    fn source(&self) -> PathBuf {
        self.dir.join("source.json")
    }

    /// What the fake connector syncs from now on.
    fn says(&self, what: Value) {
        // Written beside and renamed, as an editor would: the watch sees one change.
        let temporary = self.dir.join("source.tmp");
        std::fs::write(&temporary, what.to_string()).unwrap();
        std::fs::rename(&temporary, self.source()).unwrap();
    }

    /// Adds an account as the settings would, and tells the shell.
    fn add(&self, password: &str) -> String {
        let values: Map<String, Value> = json!({ "source": self.source().to_string_lossy(), "password": password }).as_object().unwrap().clone();
        let id = accounts::add(&mut self.settings.lock().unwrap(), &self.kind, "Fake", &values).unwrap();
        self.shell.added(&id, self.kind.plan());
        id
    }

    fn titles(&self, query: &str) -> Vec<String> {
        let mut titles: Vec<String> = self.items.search(query, 50, chrono::Local::now()).unwrap().into_iter().map(|found| found.item.title).collect();
        titles.sort();
        titles
    }
}

impl Drop for Test {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Waits for `what` to hold, for up to ten seconds.
fn until(what: &str, mut holds: impl FnMut() -> bool) {
    let start = Instant::now();
    while !holds() {
        assert!(start.elapsed() < Duration::from_secs(10), "waited in vain for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn item(id: &str, title: &str) -> crate::index::items::Item {
    serde_json::from_value(json!({ "id": id, "kind": "email", "title": title, "time": "2026-11-03T09:00:00Z" })).unwrap()
}

fn mail(id: &str, title: &str) -> Value {
    json!({ "id": id, "kind": "email", "title": title, "time": "2026-11-03T09:00:00Z", "people": [{ "name": "Ada", "role": "from" }] })
}

#[test]
fn an_account_is_checked_synced_watched_synced_again_and_removed_with_everything_of_it() {
    let test = Test::quick("life");
    test.says(json!({ "version": 1, "password": "pw", "items": [mail("1", "Invoice 1"), mail("2", "Lunch")] }));
    let id = test.add("pw");
    assert_eq!(test.shell.check(&id), Ok(()));
    // The first sync brings everything.
    until("the first sync", || test.shell.state(&id).last_sync.is_some());
    let state = test.shell.state(&id);
    assert_eq!((state.phase.as_str(), state.items, state.error), ("idle", 2, None));
    assert_eq!(test.titles("ada"), ["Invoice 1", "Lunch"]);
    // The watch sees the file change, and the sync after brings what changed.
    test.says(json!({ "version": 2, "password": "pw", "items": [mail("3", "Invoice 3")], "removed": ["2"] }));
    until("the change to be synced", || test.titles("ada") == ["Invoice 1", "Invoice 3"]);
    until("its cursor", || test.shell.state(&id).items == 2);
    // Removed, nothing of it is left: items, secrets, what was remembered.
    accounts::remove(&mut test.settings.lock().unwrap(), &id).unwrap();
    test.shell.removed(&id);
    assert_eq!(test.items.count(&id), Ok(0));
    assert!(test.settings.lock().unwrap().secrets.names(Owner::Connection(&id)).is_empty());
    assert_eq!(test.shell.state(&id).last_sync, None);
    // And the process is ended with the last account.
    assert!(test.shell.lock().process.is_none());
}

#[test]
fn a_sync_that_brings_everything_keeps_what_it_brought_however_old_and_drops_the_rest() {
    let test = Test::quick("full");
    let old = |id: &str, title: &str| json!({ "id": id, "kind": "email", "title": title, "time": "2020-01-02T09:00:00Z" });
    test.says(json!({ "version": 1, "password": "pw", "items": [old("1", "Old letter"), old("2", "Older letter")] }));
    let id = test.add("pw");
    until("the first sync", || test.shell.state(&id).last_sync.is_some());
    assert_eq!(test.shell.state(&id).items, 2);
    // Edited, it brings everything again: what it no longer brings goes.
    test.says(json!({ "version": 1, "password": "pw", "items": [old("2", "Older letter")] }));
    let values = json!({ "password": "pw" }).as_object().unwrap().clone();
    accounts::edit(&mut test.settings.lock().unwrap(), &test.kind, &id, None, &values).unwrap();
    test.shell.edited(&id, test.kind.plan());
    until("the sync after the edit", || test.shell.state(&id).items == 1);
    assert_eq!(test.titles("letter"), ["Older letter"]);
}

#[test]
fn refused_credentials_are_said_and_wait_for_the_user() {
    let test = Test::quick("auth");
    test.says(json!({ "version": 1, "password": "right", "items": [mail("1", "Invoice")] }));
    let id = test.add("wrong");
    let refused = test.shell.check(&id).unwrap_err();
    assert_eq!((refused.kind, refused.message.as_str()), (ErrorKind::Auth, "The password was refused."));
    until("the sync to fail", || test.shell.state(&id).error.is_some());
    let state = test.shell.state(&id);
    assert_eq!((state.phase.as_str(), state.error.unwrap().kind, state.items), ("waiting", ErrorKind::Auth, 0));
    // Nothing is tried again by itself, and the watch is stopped.
    std::thread::sleep(Duration::from_millis(400));
    assert_eq!(test.shell.state(&id).phase, "waiting");
    // The user sets the password: the account is edited, and syncs.
    let values = json!({ "password": "right" }).as_object().unwrap().clone();
    accounts::edit(&mut test.settings.lock().unwrap(), &test.kind, &id, None, &values).unwrap();
    test.shell.edited(&id, test.kind.plan());
    until("the sync after the edit", || test.shell.state(&id).items == 1);
    assert_eq!(test.shell.state(&id).error, None);
}

#[test]
fn a_sync_that_is_stopped_is_no_failure_and_writes_nothing_after() {
    let test = Test::quick("stop");
    test.says(json!({ "version": 1, "password": "pw", "hang": true, "items": [mail("1", "Invoice")] }));
    let id = test.add("pw");
    until("the sync to start", || test.shell.state(&id).phase == "syncing" && test.shell.lock().pending.len() >= 2);
    // Edited, it is stopped and starts again from the new settings.
    test.says(json!({ "version": 1, "password": "pw", "items": [mail("1", "Invoice")] }));
    test.shell.edited(&id, test.kind.plan());
    until("the sync after the stop", || test.shell.state(&id).items == 1);
    assert_eq!(test.shell.state(&id).error, None);
    // A put for an account that is not being synced is refused.
    let generation = test.shell.lock().generations;
    let put = ConnectorRequest::ItemsPut { account: id.clone(), items: vec![item("9", "Late")] };
    assert!(test.shell.answer(generation, put).unwrap_err().contains("not being synced"));
    let many = ConnectorRequest::ItemsRemove { account: id, ids: vec!["x".into(); ITEMS_AT_ONCE + 1] };
    assert!(test.shell.answer(generation, many).unwrap_err().contains("at most 200"));
}

#[test]
fn a_sync_that_takes_too_long_is_stopped_and_failed() {
    let setup = Setup { limits: limits(), sync_timeout: Duration::from_millis(500), ..Setup::default() };
    let test = Test::new("timeout", setup);
    // It does not even hear that it is stopped.
    test.says(json!({ "version": 1, "password": "pw", "hang": "deaf" }));
    let id = test.add("pw");
    until("the timeout", || test.shell.state(&id).error.is_some());
    let state = test.shell.state(&id);
    assert_eq!((state.error.unwrap().kind, state.items), (ErrorKind::Timeout, 0));
}

#[test]
fn a_failing_sync_is_tried_again_and_a_crash_restarts_the_watch() {
    let test = Test::quick("again");
    test.says(json!({ "version": 1, "password": "pw", "fail": "unavailable" }));
    let id = test.add("pw");
    until("the failure", || test.shell.state(&id).error.is_some());
    assert_eq!(test.shell.state(&id).error.unwrap().kind, ErrorKind::Unavailable);
    test.says(json!({ "version": 1, "password": "pw", "items": [mail("1", "Invoice")] }));
    until("the sync tried again", || test.shell.state(&id).items == 1 && test.shell.state(&id).error.is_none());
    // The process is gone: the watch is started in a new one.
    let first = test.shell.lock().generations;
    // As if it had ended: its input is closed, and the shell hears of it.
    test.shell.ended(first);
    until("a new process", || test.shell.lock().generations > first && test.shell.lock().process.is_some());
    // Changed until it is seen: the new process may still be starting its watch.
    let mut polls = 0;
    until("the change seen by the new watch", || {
        if polls % 25 == 0 {
            test.says(json!({ "version": 2 + polls, "password": "pw", "items": [mail("1", "Invoice"), mail("2", "Receipt")] }));
        }
        polls += 1;
        test.shell.state(&id).items == 2
    });
}

#[test]
fn a_search_answers_within_its_budget_or_without() {
    // A test machine busy with the others is given time; Heyo gives 50 ms.
    let test = Test::new("search", Setup { search_budget: Duration::from_secs(5), ..Setup::default() });
    test.items.put("work", &[item("1", "Quarterly invoice")]).unwrap();
    let searched = test.shell.search("invoice", 10);
    assert!(searched.complete);
    assert_eq!(searched.found[0].item.title, "Quarterly invoice");
    let hurried = Test::new("hurried", Setup { search_budget: Duration::ZERO, ..Setup::default() });
    hurried.items.put("work", &[item("1", "Quarterly invoice")]).unwrap();
    assert_eq!(hurried.shell.search("invoice", 10), Searched { found: Vec::new(), complete: false });
}

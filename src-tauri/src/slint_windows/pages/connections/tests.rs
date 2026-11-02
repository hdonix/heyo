//! The Connections page through the window model, with answers written out.

use chrono::TimeDelta;
use serde_json::json;

use super::*;
use crate::connections::accounts::tests::mail;
use crate::connections::accounts::{Preset, Trouble};
use crate::connections::shell::{Progress, State};
use crate::slint_windows::model::Model;

fn imap() -> Kind {
    let mut kind = mail();
    kind.presets = vec![Preset { title: "Gmail".into(), values: json!({ "host": "imap.gmail.com" }).as_object().unwrap().clone(), web: Some("https://mail.google.com".into()) }];
    kind
}

fn idle() -> State {
    State { phase: "idle".into(), since: None, next: None, last_sync: None, error: None, items: 0, progress: None }
}

fn view(id: &str, name: &str, state: State) -> View {
    View { id: id.into(), connector: "mail:imap".into(), name: name.into(), config: json!({ "host": "imap.example.com", "port": 993 }).as_object().unwrap().clone(), secrets_set: vec!["password".into()], orphaned: false, state }
}

fn told(connections: Vec<View>) -> Model {
    let mut model = Model::new();
    model.known.connectors = vec![imap()];
    model.known.connections = connections;
    model
}

fn row<'a>(lines: &'a [Line], label: &str) -> &'a Line {
    lines.iter().find(|line| line.label == label).unwrap_or_else(|| panic!("no line {label:?} among {:?}", lines.iter().map(|line| &line.label).collect::<Vec<_>>()))
}

fn asked(asks: &[Ask]) -> Command {
    assert_eq!(asks.len(), 1, "{asks:?}");
    asks[0].command.clone()
}

#[test]
fn the_list_says_how_each_connection_syncs_and_how_much_it_holds() {
    let synced = State { last_sync: Some(Utc::now() - TimeDelta::minutes(3)), items: 1234, ..idle() };
    let syncing = State { phase: "syncing".into(), progress: Some(Progress { fraction: 0.42, text: None }), ..idle() };
    let refused = State { phase: "waiting".into(), error: Some(Trouble { kind: ErrorKind::Auth, message: "The password was refused.".into() }), ..idle() };
    let mut model = told(vec![view("imap", "Work", synced), view("imap-2", "IMAP", syncing), view("imap-3", "Home", refused)]);
    let asks = model.show(PAGE);
    assert_eq!(asks.iter().map(|ask| ask.command.name()).collect::<Vec<_>>(), ["connections", "connector_kinds"]);
    assert_eq!(model.poll().iter().map(|ask| ask.command.name()).collect::<Vec<_>>(), ["connections"]);
    let lines = model.lines();
    let work = row(&lines, "Work");
    assert_eq!((work.description.as_str(), work.chips.as_slice(), work.id.as_str(), work.danger), ("IMAP · Synced · 3 min ago", &["1,234 items".to_string()][..], "go:connection:imap", false));
    // Its kind is not said twice.
    assert_eq!(row(&lines, "IMAP").description, "Syncing 42%");
    assert!(row(&lines, "Home").danger && row(&lines, "Home").description == "IMAP · Wrong password");
    assert_eq!(row(&lines, "Add Connection…").id, "go:connection:add");
}

#[test]
fn a_connection_is_added_from_a_preset_only_once_it_was_tried_and_works() {
    let mut model = told(Vec::new());
    model.show(PAGE);
    model.acted("go:connection:add", "");
    let lines = model.lines();
    let kind = row(&lines, "IMAP");
    assert_eq!((kind.description.as_str(), kind.icon.as_deref()), ("Mail · Mail", Some("envelope-16")));
    model.acted(&kind.id, "");
    assert_eq!(model.page, PageId::NewConnection);
    let lines = model.lines();
    assert_eq!(lines.iter().find(|line| line.kind == "head").unwrap().label, "New IMAP");
    // The defaults are filled in; a preset fills in the rest, and names it.
    assert_eq!(row(&lines, "Port").value, "993");
    assert_eq!(row(&lines, "Folders").value, "INBOX");
    assert!(model.acted("connection-preset", "0").is_empty());
    let lines = model.lines();
    assert_eq!((row(&lines, "Server").value.as_str(), row(&lines, "Name").value.as_str(), row(&lines, "Service").value.as_str()), ("imap.gmail.com", "Gmail", "0"));
    // Not added before it is tried.
    assert!(row(&lines, "Add This Connection").busy);
    assert!(model.acted("act:save-connection", "").is_empty());
    model.acted(&row(&lines, "Port").id, "143");
    model.acted(&row(&lines, "Folders").id, "INBOX, Sent");
    model.acted(&row(&lines, "Password").id, "app-password");
    assert_eq!(row(&model.lines(), "Password").placeholder, "Typed — type again to replace it");
    let trying = model.acted("act:try-connection", "");
    let values = json!({ "host": "imap.gmail.com", "port": 143, "folders": ["INBOX", "Sent"], "security": "tls" }).as_object().unwrap().clone();
    let secrets = BTreeMap::from([("password".to_string(), "app-password".to_string())]);
    assert_eq!(asked(&trying), Command::TryConnection { connector: "mail:imap".into(), of: None, config: values.clone(), secrets: secrets.clone() });
    assert!(Command::TryConnection { connector: String::new(), of: None, config: Map::new(), secrets: BTreeMap::new() }.slow());
    assert!(row(&model.lines(), "Check Connection").busy);
    // Refused: said in the form, not as the window's error.
    model.answered(trying[0].then.clone(), Err("The password was refused.".into()));
    let lines = model.lines();
    assert!(row(&lines, "Connection Failed").danger && model.error.is_empty() && row(&lines, "Add This Connection").busy);
    // Works: it can be added, and is shown on its own page once it is.
    let trying = model.acted("act:try-connection", "");
    model.answered(trying[0].then.clone(), Ok(Answer::Done));
    assert!(!row(&model.lines(), "Add This Connection").busy);
    let adding = model.acted("act:save-connection", "");
    assert_eq!(asked(&adding), Command::AddConnection { connector: "mail:imap".into(), name: "Gmail".into(), config: values, secrets });
    model.answered(adding[0].then.clone(), Ok(Answer::Connections(vec![view("imap", "Gmail", idle())])));
    assert_eq!(model.page, PageId::Connection("imap".into()));
}

#[test]
fn a_change_to_the_form_is_tried_again() {
    let mut model = told(Vec::new());
    model.show("connection:add");
    model.acted("new-connection:mail:imap", "");
    model.acted("connection-draft:host", "imap.example.com");
    let trying = model.acted("act:try-connection", "");
    model.answered(trying[0].then.clone(), Ok(Answer::Done));
    model.acted("connection-draft:host", "imap.example.org");
    assert!(row(&model.lines(), "Add This Connection").busy);
    assert!(!model.lines().iter().any(|line| line.label == "Connection Works"));
}

#[test]
fn a_connections_page_syncs_checks_edits_and_removes_it() {
    let failed = State { last_sync: Some(Utc::now() - TimeDelta::hours(2)), error: Some(Trouble { kind: ErrorKind::Unavailable, message: "imap.example.com did not answer.".into() }), items: 1, ..idle() };
    let mut model = told(vec![view("imap", "Work", failed)]);
    model.show("connection:imap");
    let lines = model.lines();
    let status = row(&lines, "Status");
    assert_eq!((status.description.as_str(), status.danger, status.value.as_str()), ("Not reachable", true, "Sync Now"));
    assert_eq!(row(&lines, "Last Error").description, "imap.example.com did not answer.");
    assert_eq!(row(&lines, "Items").value, "1 item");
    assert_eq!(model.poll().iter().map(|ask| ask.command.name()).collect::<Vec<_>>(), ["connections"]);
    assert_eq!(model.acted("sync-connection:imap", "")[0].command, Command::SyncConnection("imap".into()));
    // A secret is never shown, only that it is saved; left empty it stays.
    let password = row(&lines, "Password");
    assert_eq!((password.control, password.placeholder.as_str()), ("secret", "Saved — type a new one to replace it"));
    assert!(model.acted(&password.id, "").is_empty());
    let edit = |id: &str, config: Value, secrets: &[(&str, &str)], name: Option<&str>| Command::EditConnection {
        id: id.into(),
        name: name.map(str::to_owned),
        config: config.as_object().unwrap().clone(),
        secrets: secrets.iter().map(|(name, value)| (name.to_string(), value.to_string())).collect(),
    };
    assert_eq!(asked(&model.acted(&password.id, "new")), edit("imap", json!({}), &[("password", "new")], None));
    assert_eq!(asked(&model.acted(&row(&lines, "Port").id, "143")), edit("imap", json!({ "port": 143 }), &[], None));
    assert!(model.acted(&row(&lines, "Port").id, "993").is_empty());
    assert_eq!(asked(&model.acted(&row(&lines, "Name").id, " Office ")), edit("imap", json!({}), &[], Some("Office")));
    // Checked: what it came to is said on the page.
    let checking = model.acted("check-connection:imap", "");
    assert_eq!(asked(&checking), Command::CheckConnection("imap".into()));
    assert!(model.acted("check-connection:imap", "").is_empty());
    model.answered(checking[0].then.clone(), Ok(Answer::Done));
    assert!(!row(&model.lines(), "Connection Works").danger);
    let remove = row(&lines, "Remove Connection");
    assert_eq!(remove.control, "confirm");
    assert_eq!(asked(&model.acted(&remove.id, "")), Command::RemoveConnection("imap".into()));
    assert_eq!(model.page, PageId::own(PAGE));
}

#[test]
fn a_connection_whose_connector_is_gone_says_so() {
    let mut model = told(vec![View { orphaned: true, connector: "gone:x".into(), ..view("x", "Old", idle()) }]);
    model.show("connection:x");
    let lines = model.lines();
    assert!(row(&lines, "Status").danger && row(&lines, "Status").control.is_empty());
    assert!(!lines.iter().any(|line| line.label == "Check Connection"));
    assert!(lines.iter().any(|line| line.label == "Remove Connection"));
}

#[test]
fn a_time_ago_is_said_in_a_few_words() {
    let now = Utc::now();
    assert_eq!(ago(now - TimeDelta::seconds(20), now), "just now");
    assert_eq!(ago(now - TimeDelta::minutes(1), now), "1 min ago");
    assert_eq!(ago(now - TimeDelta::hours(5), now), "5 hours ago");
    assert_eq!(ago(now - TimeDelta::days(1), now), "1 day ago");
}

#[test]
fn a_preset_says_where_its_service_is_read_in_a_browser() {
    let kind = imap();
    assert_eq!(kind.web(json!({ "host": "imap.gmail.com", "port": 993 }).as_object().unwrap()), Some("https://mail.google.com"));
    assert_eq!(kind.web(json!({ "host": "imap.example.com" }).as_object().unwrap()), None);
}

//! Connections: the accounts of mail, calendars and other services whose
//! items are synced to this computer to be found from the root search,
//! each on a page of its own (`connection:<id>`), and adding one: its
//! connector (`connection:add`), then a preset or the fields filled in by
//! hand, checked before it can be added (`connection:new`).

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde_json::{Map, Value};

use super::{tile, Known, Page, Reaction, Sheet, To};
use crate::connections::accounts::{Field, FieldType, Kind};
use crate::connections::commands::View;
use crate::connections::scheduler::ErrorKind;
use crate::i18n::{marked, t, tc, tf, tn};
use crate::index::items::ItemKind;
use crate::slint_windows::answer::Answer;
use crate::slint_windows::host::Command;
use crate::slint_windows::ids::{Act, Id, PageId};
use crate::slint_windows::model::{ask, Ask, Entry, Line, Then};

pub(crate) const PAGE: &str = "connections";

/// Checking a connection, or trying one before it is added.
#[derive(Clone, Debug, PartialEq)]
enum Check {
    Checking,
    /// What it came to: none, or what went wrong in the connector's words.
    Done(Result<(), String>),
}

/// A connection being added: its connector, and its form as it is filled in.
#[derive(Clone, Debug)]
struct Draft {
    connector: String,
    name: String,
    /// The preset it was filled in from, by its place among the connector's.
    preset: Option<usize>,
    /// The fields that are not secrets, as the connector takes them.
    values: Map<String, Value>,
    secrets: BTreeMap<String, String>,
    /// What trying it as it is came to; any change of it is tried again.
    tried: Option<Check>,
}

#[derive(Debug, Default)]
pub(crate) struct Connections {
    /// What checking each connection came to, by its id.
    checks: BTreeMap<String, Check>,
    draft: Option<Draft>,
    /// The ids there were when the draft was sent to be added.
    adding: Vec<String>,
}

// --- What a connection is said as ---------------------------------------------

/// What a kind of item is called, many of them.
pub(crate) fn kind_word(kind: ItemKind) -> &'static str {
    match kind {
        ItemKind::Email => t("Mail"),
        ItemKind::Event => t("Events"),
        ItemKind::Message => t("Messages"),
        ItemKind::Document => t("Documents"),
        ItemKind::Task => t("Tasks"),
    }
}

/// The symbol of a connector: its own, or that of what it brings.
fn icon(kind: Option<&Kind>) -> String {
    if let Some(own) = kind.and_then(|kind| kind.icon.clone()) {
        return own;
    }
    let symbol = match kind.and_then(|kind| kind.items.first()) {
        Some(ItemKind::Email) => "envelope-16",
        Some(ItemKind::Event) => "calendar-16",
        Some(ItemKind::Message) => "speech-bubble-16",
        Some(ItemKind::Document) => "blank-document-16",
        Some(ItemKind::Task) => "check-list-16",
        None => "plug-16",
    };
    symbol.into()
}

/// What a connector is, besides its title: whose it is (an extension's)
/// and what it brings.
fn about(kind: &Kind) -> String {
    let brings = kind.items.iter().map(|item| kind_word(*item)).collect::<Vec<_>>().join(tc("between the items of a list", ", "));
    match (kind.builtin(), brings.is_empty()) {
        (true, _) => brings,
        (false, true) => kind.extension_title.clone(),
        (false, false) => format!("{} · {brings}", kind.extension_title),
    }
}

/// How long ago `then` was, in a few words.
fn ago(then: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let seconds = (now - then).num_seconds().max(0) as u64;
    match seconds {
        0..60 => t("just now").into(),
        60..3600 => tn("{n} min ago", "{n} min ago", seconds / 60, &[]),
        3600..86400 => tn("{n} hour ago", "{n} hours ago", seconds / 3600, &[]),
        _ => tn("{n} day ago", "{n} days ago", seconds / 86400, &[]),
    }
}

/// How a connection's syncing goes: in a few words for the list, in a
/// sentence for its page, and whether it needs the user.
fn status(view: &View, now: DateTime<Utc>) -> (String, String, bool) {
    let state = &view.state;
    if view.orphaned {
        return (t("Connector missing").into(), t("Its connector is not installed any more: install its extension again, or remove the connection.").into(), true);
    }
    if state.phase == "syncing" {
        let short = match &state.progress {
            Some(progress) => tf("Syncing {0}%", &[&((progress.fraction * 100.0).round() as u32)]),
            None => t("Syncing…").into(),
        };
        let long = state.progress.as_ref().and_then(|progress| progress.text.clone()).map_or_else(|| short.clone(), |doing| format!("{short} · {doing}"));
        return (short, long, false);
    }
    if let Some(error) = &state.error {
        let short = match error.kind {
            ErrorKind::Auth => t("Wrong password"),
            ErrorKind::Unavailable => t("Not reachable"),
            ErrorKind::Timeout => t("Timed out"),
            _ => t("Sync failed"),
        };
        // What the connector said is under it (Last Error).
        let long = if error.kind == ErrorKind::Auth { t("Wrong password. Type it again below, and it is synced at once.") } else { short };
        return (short.into(), long.into(), true);
    }
    match state.last_sync {
        Some(last) => {
            let short = tf("Synced · {0}", &[&ago(last, now)]);
            (short.clone(), short, false)
        }
        None => (t("Not synced yet").into(), t("It is synced in a moment.").into(), false),
    }
}

/// How many items it has, as a chip says it.
fn count(view: &View) -> String {
    let items = u64::from(view.state.items);
    tn("{0} item", "{0} items", items, &[&crate::i18n::number(items)])
}

// --- The fields --------------------------------------------------------------

/// A value as its field shows it.
fn shown(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(all)) => all.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(", "),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    }
}

/// What is typed into a field, as the connector takes it: a number, a
/// list apart by commas, on or off. What is not a number is sent as it is,
/// and the host says why it is not taken.
fn taken(field: &Field, typed: &str) -> Value {
    let typed = typed.trim();
    match field.kind {
        FieldType::Number => typed.parse::<i64>().map(Value::from).or_else(|_| typed.parse::<f64>().map(Value::from)).unwrap_or_else(|_| typed.into()),
        FieldType::List => Value::Array(typed.split([',', '\n']).map(str::trim).filter(|part| !part.is_empty()).map(Value::from).collect()),
        FieldType::Switch => Value::Bool(typed == "true"),
        _ => typed.into(),
    }
}

/// The row of a field. `id` is what it is named by; `secret_set` says a
/// secret is set, which is never shown.
fn field_row(field: &Field, id: Id, value: Option<&Value>, secret_set: Option<&str>) -> Line {
    let row = Line::row(id, field.title.clone()).says(field.description.clone().unwrap_or_default());
    let placeholder = field.placeholder.as_deref().unwrap_or_default();
    match field.kind {
        FieldType::Secret => row.secret(secret_set.unwrap_or(placeholder)),
        FieldType::Switch => row.switch(value.and_then(Value::as_bool).unwrap_or(false)),
        FieldType::Choice => {
            let chosen = value.and_then(Value::as_str).or_else(|| field.choices.first().map(|choice| choice.value.as_str())).unwrap_or_default().to_owned();
            row.select(&chosen, field.choices.iter().map(|choice| (choice.value.as_str(), choice.title.as_str())))
        }
        _ => row.field(shown(value), placeholder),
    }
}

/// The values a new connection of `kind` starts with: its fields' defaults.
fn defaults(kind: &Kind) -> Map<String, Value> {
    kind.fields.iter().filter(|field| field.kind != FieldType::Secret).filter_map(|field| Some((field.name.clone(), field.default.clone()?))).collect()
}

fn connection<'a>(known: &'a Known, id: &str) -> Option<&'a View> {
    known.connections.iter().find(|view| view.id == id)
}

fn connector<'a>(known: &'a Known, id: &str) -> Option<&'a Kind> {
    known.connectors.iter().find(|kind| kind.id == id)
}

fn asked_again() -> Ask {
    ask(Command::Connections, Then::Known)
}

/// The row that says what checking came to, if it came to anything.
fn checked_row(check: Option<&Check>) -> Option<Line> {
    match check? {
        Check::Checking => None,
        Check::Done(Ok(())) => Some(Line::row("", t("Connection Works")).says(t("It signed in and can be read."))),
        Check::Done(Err(why)) => Some(Line::row("", t("Connection Failed")).says(why.clone()).warns(true)),
    }
}

// --- The pages ---------------------------------------------------------------

impl Connections {
    /// The connections, and the way to add one.
    fn list(known: &Known) -> Vec<Line> {
        let now = Utc::now();
        let mut page = Sheet::default();
        page.note(t("Connect mail, calendars and other services. What they hold is synced to this computer and found from the root search; nothing is sent anywhere else."));
        let rows = known
            .connections
            .iter()
            .map(|view| {
                let kind = connector(known, &view.connector);
                let (short, _, warning) = status(view, now);
                // Its kind, where its name does not say it already.
                let said = match kind {
                    Some(kind) if kind.title != view.name => format!("{} · {short}", kind.title),
                    _ => short,
                };
                Line::row(Id::Go(PageId::Connection(view.id.clone())), view.name.clone()).says(said).warns(warning).pictured(Some(icon(kind))).chips(vec![count(view)]).link()
            })
            .collect();
        page.card(t("Connections"), rows);
        page.card("", vec![Line::row(Id::Go(PageId::AddConnection), t("Add Connection…")).says(t("Several of one kind can be added, two mailboxes for instance.")).pictured(Some("plus".into())).link()]);
        page.done()
    }

    /// Choosing the connector of a new connection.
    fn adding(known: &Known) -> Vec<Line> {
        let mut page = Sheet::default();
        page.top(PageId::own(PAGE), t("Connections"), "plus", t("Add Connection").into(), t("Choose what to connect.").into());
        if known.connectors.is_empty() {
            page.note(t("No connector is installed. Extensions that connect a service bring one."));
        }
        let rows = known.connectors.iter().map(|kind| Line::row(Id::NewConnection(kind.id.clone()), kind.title.clone()).says(about(kind)).pictured(Some(icon(Some(kind)))).link()).collect();
        page.card("", rows);
        page.done()
    }

    /// A new connection's form, until it is added or left: a preset, its
    /// fields, checking it, and adding it once it works.
    fn new_one(&self, known: &Known) -> Vec<Line> {
        let Some((draft, kind)) = self.draft.as_ref().and_then(|draft| Some((draft, connector(known, &draft.connector)?))) else { return Self::adding(known) };
        let mut page = Sheet::default();
        page.top(PageId::AddConnection, t("Add Connection"), &icon(Some(kind)), tf("New {0}", &[&kind.title]), about(kind));
        if !kind.presets.is_empty() {
            let mut choices = vec![(String::new(), t("Other (Fill In Yourself)").to_string())];
            choices.extend(kind.presets.iter().enumerate().map(|(at, preset)| (at.to_string(), preset.title.clone())));
            let mut service = Line::row(Id::ConnectionPreset, t("Service")).says(t("Fills in the server and the rest for a service it knows."));
            (service.control, service.value, service.choices) = ("select", draft.preset.map(|at| at.to_string()).unwrap_or_default(), choices);
            page.card("", vec![service]);
        }
        let mut rows = vec![Line::row(Id::ConnectionDraft(String::new()), t("Name")).field(draft.name.clone(), &kind.title)];
        rows.extend(kind.fields.iter().map(|field| {
            let typed = draft.secrets.get(&field.name).is_some_and(|secret| !secret.is_empty()).then(|| t("Typed — type again to replace it"));
            field_row(field, Id::ConnectionDraft(field.name.clone()), draft.values.get(&field.name), typed)
        }));
        page.card(t("Settings"), rows);
        let checking = draft.tried == Some(Check::Checking);
        let works = draft.tried == Some(Check::Done(Ok(())));
        let mut check = Line::row(Id::Act(Act::TryConnection), t("Check Connection")).says(t("Signs in as it would once added, and reads nothing yet.")).button(if checking { t("Checking…") } else { t("Check") });
        check.busy = checking;
        let mut rows = vec![check];
        rows.extend(checked_row(draft.tried.as_ref()));
        page.card("", rows);
        let mut add = Line::row(Id::Act(Act::SaveConnection), t("Add This Connection"))
            .says(if works { t("It is synced at once; what it holds is then found from the root search.") } else { t("It can be added once the check works.") })
            .button(t("Add"))
            .more(t("Cancel"), Id::Go(PageId::own(PAGE)));
        add.busy = !works;
        page.card("", vec![add]);
        page.done()
    }

    /// A connection's page: how its syncing goes (and syncing it now,
    /// checking it), its settings, and removing it.
    fn connection_page(&self, known: &Known, view: &View) -> Vec<Line> {
        let kind = connector(known, &view.connector);
        let mut page = Sheet::default();
        let title = kind.map(|kind| kind.title.clone()).unwrap_or_default();
        page.top(PageId::own(PAGE), t("Connections"), &icon(kind), view.name.clone(), if view.name == title { String::new() } else { title });
        let (_, long, warning) = status(view, Utc::now());
        let syncing = view.state.phase == "syncing";
        let mut sync = Line::row(Id::SyncConnection(view.id.clone()), t("Status")).says(long).warns(warning);
        if !view.orphaned {
            sync = sync.button(if syncing { t("Syncing…") } else { t("Sync Now") });
            sync.busy = syncing;
        }
        let mut rows = vec![sync];
        if let Some(error) = view.state.error.as_ref().filter(|_| !syncing) {
            rows.push(Line::row("", t("Last Error")).says(error.message.clone()).warns(true));
        }
        rows.push(Line::row("", t("Items")).says(t("Found from the root search, and kept on this computer only.")).text(count(view)));
        if !view.orphaned {
            let checking = self.checks.get(&view.id) == Some(&Check::Checking);
            let mut check = Line::row(Id::CheckConnection(view.id.clone()), t("Check Connection")).says(t("Signs in as a sync would, and reads nothing.")).button(if checking { t("Checking…") } else { t("Check") });
            check.busy = checking;
            rows.push(check);
            rows.extend(checked_row(self.checks.get(&view.id)));
        }
        page.card("", rows);
        if let Some(kind) = kind {
            let id = |field: &str| Id::ConnectionField { account: view.id.clone(), field: field.into() };
            let mut rows = vec![Line::row(id(""), t("Name")).field(view.name.clone(), &kind.title)];
            rows.extend(kind.fields.iter().map(|field| {
                let set = view.secrets_set.contains(&field.name).then(|| t("Saved — type a new one to replace it"));
                field_row(field, id(&field.name), view.config.get(&field.name), set)
            }));
            page.card(t("Settings"), rows);
        }
        page.card("", vec![Line::row(Id::RemoveConnection(view.id.clone()), t("Remove Connection")).says(t("Its items and its password are deleted from this computer.")).confirm(t("Remove…"), t("Click Again to Remove"))]);
        page.done()
    }

    /// A field of the form was changed: what trying it came to no longer holds.
    fn fill(&mut self, known: &Known, field: &str, value: &str) {
        let Some(draft) = &mut self.draft else { return };
        let Some(kind) = connector(known, &draft.connector) else { return };
        draft.tried = None;
        if field.is_empty() {
            draft.name = value.trim().into();
            return;
        }
        let Some(field) = kind.fields.iter().find(|known| known.name == field) else { return };
        if field.kind == FieldType::Secret {
            // An empty field keeps what was typed before.
            if !value.is_empty() {
                draft.secrets.insert(field.name.clone(), value.into());
            }
        } else if value.trim().is_empty() && field.kind != FieldType::Switch {
            draft.values.remove(&field.name);
        } else {
            draft.values.insert(field.name.clone(), taken(field, value));
        }
    }

    /// The form filled in from the connector's preset at `at` (none: by hand).
    fn preset(&mut self, known: &Known, at: &str) {
        let Some(draft) = &mut self.draft else { return };
        let Some(kind) = connector(known, &draft.connector) else { return };
        draft.preset = at.parse::<usize>().ok().filter(|at| *at < kind.presets.len());
        draft.values = defaults(kind);
        if let Some(preset) = draft.preset.map(|at| &kind.presets[at]) {
            draft.values.extend(preset.values.clone());
            if draft.name.is_empty() {
                draft.name = preset.title.clone();
            }
        }
        draft.tried = None;
    }

    /// A field of a connection there is was changed: sent at once, unless
    /// it says what it said (a secret left empty keeps the one saved).
    fn edit(known: &Known, account: &str, field: &str, value: &str) -> Vec<Ask> {
        let Some(view) = connection(known, account) else { return Vec::new() };
        let (mut config, mut secrets, mut name) = (Map::new(), BTreeMap::new(), None);
        if field.is_empty() {
            if value.trim().is_empty() || value.trim() == view.name {
                return Vec::new();
            }
            name = Some(value.trim().to_owned());
        } else {
            let Some(field) = connector(known, &view.connector).and_then(|kind| kind.fields.iter().find(|known| known.name == field)) else { return Vec::new() };
            if field.kind == FieldType::Secret {
                if value.is_empty() {
                    return Vec::new();
                }
                secrets.insert(field.name.clone(), value.to_owned());
            } else {
                let value = taken(field, value);
                if view.config.get(&field.name) == Some(&value) {
                    return Vec::new();
                }
                config.insert(field.name.clone(), value);
            }
        }
        vec![ask(Command::EditConnection { id: account.into(), name, config, secrets }, Then::Known)]
    }
}

impl Page for Connections {
    fn entries(&self, _: &Known) -> Vec<Entry> {
        vec![tile(PAGE, marked("Connections"), "cable-16")]
    }

    fn shows(&self, at: &PageId) -> bool {
        matches!(at, PageId::AddConnection | PageId::NewConnection | PageId::Connection(_)) || *at == PageId::own(PAGE)
    }

    fn lines(&self, at: &PageId, known: &Known) -> Vec<Line> {
        match at {
            PageId::AddConnection => Self::adding(known),
            PageId::NewConnection => self.new_one(known),
            PageId::Connection(id) => connection(known, id).map_or_else(|| Self::list(known), |view| self.connection_page(known, view)),
            _ => Self::list(known),
        }
    }

    fn acted(&mut self, _: &PageId, known: &mut Known, id: &Id, value: &str) -> Option<Reaction> {
        let asks = match id {
            Id::NewConnection(kind) => {
                let Some(kind) = connector(known, kind) else { return Some(Reaction::default()) };
                self.draft = Some(Draft { connector: kind.id.clone(), name: String::new(), preset: None, values: defaults(kind), secrets: BTreeMap::new(), tried: None });
                return Some(Reaction::showing(PageId::NewConnection));
            }
            Id::ConnectionPreset => {
                self.preset(known, value);
                Vec::new()
            }
            Id::ConnectionDraft(field) => {
                self.fill(known, field, value);
                Vec::new()
            }
            Id::Act(Act::TryConnection) => {
                let Some(draft) = self.draft.as_mut().filter(|draft| draft.tried != Some(Check::Checking)) else { return Some(Reaction::default()) };
                draft.tried = Some(Check::Checking);
                vec![ask(Command::TryConnection { connector: draft.connector.clone(), of: None, config: draft.values.clone(), secrets: draft.secrets.clone() }, Then::Acted(id.clone()))]
            }
            Id::Act(Act::SaveConnection) => {
                // Only what was tried, and worked, is added.
                let Some(draft) = self.draft.as_ref().filter(|draft| draft.tried == Some(Check::Done(Ok(())))) else { return Some(Reaction::default()) };
                self.adding = known.connections.iter().map(|view| view.id.clone()).collect();
                vec![ask(Command::AddConnection { connector: draft.connector.clone(), name: draft.name.clone(), config: draft.values.clone(), secrets: draft.secrets.clone() }, Then::Acted(id.clone()))]
            }
            Id::ConnectionField { account, field } => {
                self.checks.remove(account);
                Self::edit(known, account, field, value)
            }
            Id::CheckConnection(account) => {
                if self.checks.get(account) == Some(&Check::Checking) {
                    return Some(Reaction::default());
                }
                self.checks.insert(account.clone(), Check::Checking);
                vec![ask(Command::CheckConnection(account.clone()), Then::Acted(id.clone()))]
            }
            Id::SyncConnection(account) => vec![ask(Command::SyncConnection(account.clone()), Then::Known), asked_again()],
            Id::RemoveConnection(account) => {
                self.checks.remove(account);
                return Some(Reaction { asks: vec![ask(Command::RemoveConnection(account.clone()), Then::Known)], to: Some(To::Move(PageId::own(PAGE))), said: false });
            }
            _ => return None,
        };
        Some(Reaction::asking(asks))
    }

    fn answered(&mut self, known: &mut Known, id: &Id, answer: &Result<Answer, String>) -> Reaction {
        let outcome = answer.as_ref().map(drop).map_err(Clone::clone);
        match id {
            Id::Act(Act::TryConnection) => {
                // Changed while it was tried: what it came to is not of it.
                if let Some(draft) = self.draft.as_mut().filter(|draft| draft.tried == Some(Check::Checking)) {
                    draft.tried = Some(Check::Done(outcome.clone()));
                }
                // Said in the form.
                return Reaction { said: outcome.is_err(), ..Reaction::default() };
            }
            Id::CheckConnection(account) => {
                self.checks.insert(account.clone(), Check::Done(outcome.clone()));
                return Reaction { said: outcome.is_err(), ..Reaction::default() };
            }
            // Added: its own page, where it is being synced.
            Id::Act(Act::SaveConnection) if outcome.is_ok() => {
                self.draft = None;
                let added = known.connections.iter().find(|view| !self.adding.contains(&view.id));
                return Reaction::showing(added.map_or(PageId::own(PAGE), |view| PageId::Connection(view.id.clone())));
            }
            _ => {}
        }
        Reaction::default()
    }

    fn entered(&self, _: &PageId, _: &Known) -> Vec<Ask> {
        vec![asked_again(), ask(Command::ConnectorKinds, Then::Known)]
    }

    fn poll(&self, at: &PageId, _: &Known) -> Vec<Ask> {
        match at {
            PageId::Own(_) | PageId::Connection(_) => vec![asked_again()],
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests;

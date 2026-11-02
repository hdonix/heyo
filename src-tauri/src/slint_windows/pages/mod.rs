//! The pages of the settings window, each a module of its own.
//!
//! A page is what it shows (its lines, from what the window knows), what
//! its controls do (what to ask of the host, what to show next), and what
//! it makes of the answers to what it asked: all behind [`Page`]. The
//! window model (`model`) only routes: it keeps which page is shown, the
//! sidebar and its search, and what the host told, which every page reads
//! ([`Known`]). Adding a page is a module here and a line in [`all`].

pub(crate) mod about;
pub(crate) mod advanced;
pub(crate) mod connections;
pub(crate) mod dictation;
pub(crate) mod extensions;
pub(crate) mod general;
pub(crate) mod models;
pub(crate) mod screenshots;
pub(crate) mod shortcuts;

use std::collections::BTreeMap;

use serde::{de::DeserializeOwned, Serialize};
use serde_json::{Map, Value};

use super::answer::{Answer, Application, DictationStatus, SpeechModel};
use super::host::Command;
use super::ids::{Id, PageId};
use super::model::{ask, Ask, Entry, Line, Then};
use crate::clipboard::{meaning, sources::Source};
use crate::commands::media::OcrStatus;
use crate::commands::settings::{Hotkeys, SettingsView};
use crate::extensions::ExtensionInfo;
use crate::permissions::Row;
use crate::platform::Capabilities;

/// A page of the window, or a family of them (a provider's pages are the
/// Models page's).
pub(crate) trait Page {
    /// Its entries in the sidebar: what each is called, its symbol, and
    /// what the sidebar's search finds it by.
    fn entries(&self, known: &Known) -> Vec<Entry>;
    /// Whether `at` is one of its pages.
    fn shows(&self, at: &PageId) -> bool;
    /// The lines of `at`, from what is known.
    fn lines(&self, at: &PageId, known: &Known) -> Vec<Line>;
    /// A control of `at` was used and now says `value`. None when the
    /// control is not one of its.
    fn acted(&mut self, at: &PageId, known: &mut Known, id: &Id, value: &str) -> Option<Reaction>;
    /// What `at` needs to know besides, when it is shown.
    fn entered(&self, _at: &PageId, _known: &Known) -> Vec<Ask> {
        Vec::new()
    }
    /// The host answered what the control `id` asked (`Then::Acted`); every
    /// page is told, and the one whose control it was acts on it. What it
    /// answered is already known.
    fn answered(&mut self, _known: &mut Known, _id: &Id, _answer: &Result<Answer, String>) -> Reaction {
        Reaction::default()
    }
    /// The field and the menu above the lines of `at`.
    fn header(&self, _at: &PageId) -> Header {
        Header::default()
    }
    /// What is asked again every two seconds while `at` is shown.
    fn poll(&self, _at: &PageId, _known: &Known) -> Vec<Ask> {
        Vec::new()
    }
}

/// Every page, in the order the sidebar lists them.
pub(crate) fn all() -> Vec<Box<dyn Page>> {
    vec![
        Box::new(general::General),
        Box::new(shortcuts::Shortcuts::default()),
        Box::new(dictation::Dictation),
        Box::new(screenshots::Screenshots),
        Box::new(models::Models::default()),
        Box::new(connections::Connections::default()),
        Box::new(advanced::Advanced),
        Box::new(about::About::default()),
        Box::new(extensions::Extensions),
    ]
}

/// What a control, or an answer, comes to besides what is known.
#[derive(Debug, Default)]
pub(crate) struct Reaction {
    pub asks: Vec<Ask>,
    /// Another page.
    pub to: Option<To>,
    /// A failure the page says itself, not as the window's error.
    pub said: bool,
}

impl Reaction {
    pub(crate) fn asking(asks: Vec<Ask>) -> Reaction {
        Reaction { asks, ..Reaction::default() }
    }

    pub(crate) fn showing(page: PageId) -> Reaction {
        Reaction { to: Some(To::Show(page)), ..Reaction::default() }
    }
}

/// Where a page sends the window.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum To {
    /// Shown as the sidebar shows it: what it needs is asked.
    Show(PageId),
    /// Gone to, nothing asked: what follows asks.
    Move(PageId),
}

/// The field above a page's lines and the menu beside it.
#[derive(Debug, Default)]
pub(crate) struct Header {
    /// The field's words; none, no field.
    pub placeholder: &'static str,
    /// What the menu's button says, how many things are ticked that
    /// narrow the page, and its entries (id, title, whether it is a
    /// heading, whether it is ticked).
    pub filter: Option<(String, usize, Vec<(String, String, bool, bool)>)>,
}

/// Everything the window was told, which every page reads.
#[derive(Debug, Default)]
pub(crate) struct Known {
    pub settings: SettingsView,
    pub hotkeys: Hotkeys,
    pub capabilities: Option<Capabilities>,
    pub dictation: Option<DictationStatus>,
    pub ocr: OcrStatus,
    pub meaning: Option<meaning::Status>,
    /// `clipboard_sources`: the applications one can be told to leave out.
    pub sources: Vec<Source>,
    /// The extension whose permissions are known, and they.
    pub permissions: (String, Vec<Row>),
    /// The models each OpenAI-compatible server listed, by the provider's
    /// id, or why it could not be asked.
    pub listed: BTreeMap<String, Result<Vec<String>, String>>,
    pub extensions: Vec<ExtensionInfo>,
    pub applications: Vec<Application>,
    pub item_hotkeys: BTreeMap<String, String>,
    /// The speech models of this computer, as `dictation::models` says them.
    pub speech: Vec<SpeechModel>,
    /// Downloads started here, until the host says how they are.
    pub ocr_installing: bool,
    pub meaning_installing: bool,
    /// The diagnostics were copied: said for a moment.
    pub copied: bool,
    /// What is typed into the field above a page's lines.
    pub find: String,
    /// The connected accounts, with how their syncing goes; and the
    /// connectors accounts can be added of.
    pub connections: Vec<crate::connections::commands::View>,
    pub connectors: Vec<crate::connections::accounts::Kind>,
}

impl Known {
    /// What the host answered, where it is kept. An answer that is about
    /// something only its asker knows (`Then`) is put by the model.
    pub(crate) fn learn(&mut self, answer: Answer) {
        match answer {
            Answer::Settings(settings) => self.settings = *settings,
            Answer::Hotkeys(hotkeys) => self.hotkeys = hotkeys,
            Answer::Capabilities(capabilities) => self.capabilities = Some(capabilities),
            Answer::Extensions(extensions) => self.extensions = extensions,
            Answer::Applications(applications) => self.applications = applications,
            Answer::ItemHotkeys(bound) => self.item_hotkeys = bound,
            Answer::Dictation(dictation) => self.dictation = Some(dictation),
            Answer::Speech(speech) => self.speech = speech,
            Answer::Ocr(ocr) => self.ocr = ocr,
            Answer::Meaning(meaning) => self.meaning = Some(meaning),
            Answer::Sources(sources) => self.sources = sources,
            Answer::Connections(connections) => self.connections = connections,
            Answer::ConnectorKinds(connectors) => self.connectors = connectors,
            Answer::Permissions(_) | Answer::Models(_) | Answer::Checked(_) | Answer::Update(_) | Answer::Found(_) | Answer::Done => {}
        }
    }

    /// Changes the settings as `edit` does: shown at once, before the
    /// host has answered, and asked of it as the settings that differ.
    pub(crate) fn change(&mut self, edit: impl FnOnce(&mut SettingsView)) -> Vec<Ask> {
        let mut next = self.settings.clone();
        edit(&mut next);
        let changes = differing(&self.settings, &next);
        self.settings = next;
        if changes.is_empty() { Vec::new() } else { vec![ask(Command::ChangeSettings(changes), Then::Settings)] }
    }

    pub(crate) fn hotkeys_supported(&self) -> bool {
        self.hotkeys.supported
    }
}

/// The settings of `is` that are not as in `was`, by the names the host
/// reads them by: the host's own, as serde writes them.
fn differing(was: &SettingsView, is: &SettingsView) -> Map<String, Value> {
    let (Ok(Value::Object(was)), Ok(Value::Object(is))) = (serde_json::to_value(was), serde_json::to_value(is)) else { return Map::new() };
    is.into_iter().filter(|(name, value)| was.get(name) != Some(value)).collect()
}

/// What a switch says: true or false.
pub(crate) fn on(value: &str) -> bool {
    value == "true"
}

/// A value of the settings as a choice says it: the word it is stored as.
pub(crate) fn word<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value).ok().and_then(|value| value.as_str().map(str::to_string)).unwrap_or_default()
}

/// The value a choice's word says; none for a word it is not.
pub(crate) fn from_word<T: DeserializeOwned>(word: &str) -> Option<T> {
    serde_json::from_value(Value::String(word.to_string())).ok()
}

/// A list with `item` added to it or taken out.
pub(crate) fn listed(all: &[String], item: &str, there: bool) -> Vec<String> {
    let mut all: Vec<String> = all.iter().filter(|other| *other != item).cloned().collect();
    if there {
        all.push(item.to_string());
    }
    all
}

/// Everything about the extensions and the applications, read again.
pub(crate) fn reload() -> Vec<Ask> {
    vec![ask(Command::ListExtensions, Then::Known), ask(Command::ListApplications, Then::Known), ask(Command::ItemHotkeys, Then::Known)]
}

/// Whether the command `command` is enabled, from Shortcuts or its
/// extension's page.
pub(crate) fn enable(known: &mut Known, command: &str, on: bool) -> Vec<Ask> {
    known.change(|view| view.settings.disabled_commands = listed(&view.settings.disabled_commands, command, !on))
}

/// A page being put together.
#[derive(Default)]
pub(crate) struct Sheet(pub Vec<Line>);

impl Sheet {
    pub(crate) fn note(&mut self, words: impl Into<String>) {
        self.0.push(Line { kind: "note", label: words.into(), ..Line::default() });
    }

    /// A card: its title, if it has one, and its rows.
    pub(crate) fn card(&mut self, title: &str, mut rows: Vec<Line>) {
        if rows.is_empty() {
            return;
        }
        let space = if self.0.last().is_some_and(|line| line.kind != "note" && line.kind != "head") { 24.0 } else { 0.0 };
        if title.is_empty() {
            rows[0].space = space;
        } else {
            self.0.push(Line { kind: "heading", label: title.to_string(), space, ..Line::default() });
        }
        rows[0].first = true;
        rows.last_mut().expect("a card has rows").last = true;
        self.0.extend(rows);
    }

    /// The top of a page that is gone to from another: the way back, and
    /// what it is.
    pub(crate) fn top(&mut self, back: PageId, back_title: &str, icon: &str, title: String, about: String) {
        self.0.push(Line { kind: "back", id: Id::Go(back).into(), label: back_title.into(), ..Line::default() });
        self.0.push(Line { kind: "head", label: title, description: about, icon: Some(icon.into()), ..Line::default() });
    }

    pub(crate) fn done(mut self) -> Vec<Line> {
        // Room under the last line, as a page has.
        self.0.push(Line { kind: "end", space: 24.0, ..Line::default() });
        self.0
    }
}

/// The sidebar's entry for a page of its own: id, title (marked for
/// translation) and symbol. It is found by its name as it is shown, and by
/// its English.
pub(crate) fn tile(page: &str, title: &'static str, icon: &str) -> Entry {
    let shown = crate::i18n::translate(title).to_string();
    Entry { id: page.to_string(), found_by: vec![shown.clone(), title.to_string()], title: shown, icon: Some(icon.to_string()), extension: false }
}

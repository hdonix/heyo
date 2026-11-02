//! What the settings window shows and what its controls do, worked out
//! from what the host's commands answer.
//!
//! The window knows nothing of settings: it draws lines (`Line`: a
//! heading, a row with a control, a row of a table) and says which control
//! was used. This is everything in between. A page is its lines, made from
//! the answers of `get_settings`, `get_hotkeys`, `list_extensions` and the
//! rest (JSON, by the names of `shared/bindings/`); a control that was used
//! is what to ask of the host (`Ask`); the host's answer changes what is
//! known and so the lines. None of it needs a window, so all of it is
//! tested with answers written out.
//!
//! What the host decides is not decided here: every change goes through a
//! command, and is shown as the host then says it is.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Map, Value};

use super::answer::Answer;
use super::host::Command;
use crate::i18n::{self, marked, t, tf, tn, translate};
use crate::models::kinds::{self, Known, Source};
use crate::models::registry::{self, Entry as Field, ModelRef, Provider, Use};
use super::ids::{self, Act, Bound, Id};
use super::keys;

/// The pages: id, title, and the symbol on its tile in the sidebar.
pub(crate) const PAGES: &[(&str, &str, &str)] = &[
    ("general", marked("General"), "cog-16"),
    ("shortcuts", marked("Shortcuts"), "keyboard-16"),
    ("dictation", marked("Dictation"), "microphone-16"),
    ("screenshots", marked("Screenshots"), "camera-16"),
    ("models", marked("Models"), "computer-chip-16"),
    ("advanced", marked("Advanced"), "wrench-screwdriver-16"),
    ("about", marked("About"), "info-01-16"),
];

/// The pages of the Models page's providers: `provider:<id>` one's own,
/// `provider:add` choosing the kind of a new one, `provider:new` its form.
const PROVIDER: &str = "provider:";
const ADDING: &str = "add";
const NEW: &str = "new";

/// The id the applications have among the extensions of the sidebar.
pub(crate) const APPLICATIONS: &str = "@applications";
/// Heyo's own commands.
pub(crate) const BUILTIN: &str = "@heyo";
/// The command whose hotkey is one of Heyo's own, set on its own row.
const CLIPBOARD_COMMAND: &str = "cmd:@heyo:clipboard-history";
const DICTATION_LANGUAGES: &[(&str, &str)] = &[("auto", marked("Detect Automatically")), ("zh", marked("Chinese (Mandarin)")), ("yue", marked("Cantonese")), ("en", marked("English")), ("ja", marked("Japanese")), ("ko", marked("Korean"))];

const POLISH_MODES: &[(&str, &str, &str)] = &[
    ("off", marked("Off"), marked("The text is left as it was recognized.")),
    ("clean", marked("Clean Up"), marked("When you finish, the model removes filler words, applies your corrections and fixes punctuation, then the result is pasted.")),
    ("translate", marked("Translate to English"), marked("When you finish, the model cleans up what you said and pastes it in English.")),
];

const RETENTION: &[(&str, &str)] = &[("1", marked("1 Day")), ("7", marked("7 Days")), ("30", marked("1 Month")), ("90", marked("3 Months")), ("180", marked("6 Months")), ("365", marked("1 Year"))];

const RENDERERS: &[(&str, &str)] = &[("auto", marked("Automatic")), ("cpu", "CPU"), ("gpu-shared-memory", marked("GPU, shared memory")), ("gpu-dma-buf", marked("GPU, DMA-BUF (WebKit's default)"))];

/// What the Shortcuts table can be narrowed to: id, title.
const MODIFICATIONS: &[(&str, &str)] = &[("hotkey", marked("Hotkey Set")), ("alias", marked("Alias Set")), ("enabled", marked("Enabled")), ("disabled", marked("Disabled"))];
const CATEGORIES: &[(&str, &str)] = &[("all", marked("All categories")), ("applications", marked("Applications")), ("extensions", marked("Extensions"))];

/// One line of a page, as `ui/settings.slint` draws it (see `Line` there).
/// `icon` is a file, or the name of a symbol.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Line {
    pub kind: &'static str,
    pub id: String,
    pub first: bool,
    pub last: bool,
    pub space: f32,
    pub indent: bool,
    pub label: String,
    pub description: String,
    pub danger: bool,
    pub icon: Option<String>,
    pub control: &'static str,
    pub on: bool,
    pub mixed: bool,
    pub value: String,
    pub placeholder: String,
    pub choices: Vec<(String, String)>,
    pub hotkey: String,
    pub keys: Vec<String>,
    pub more: String,
    /// What its further button acts on, its box to tick, and the field
    /// of its alias: a line with more than one control has an id for each.
    pub more_id: String,
    pub alias: String,
    pub alias_id: String,
    pub checkable: bool,
    pub check_id: String,
    pub open: bool,
    pub busy: bool,
    /// Words shown as chips before its control: what a provider can do.
    pub chips: Vec<String>,
}

impl Line {
    fn row(id: impl Into<String>, label: impl Into<String>) -> Line {
        Line { kind: "row", id: id.into(), label: label.into(), ..Line::default() }
    }

    fn says(mut self, description: impl Into<String>) -> Line {
        self.description = description.into();
        self
    }

    fn warns(mut self, danger: bool) -> Line {
        self.danger = danger;
        self
    }

    fn pictured(mut self, icon: Option<String>) -> Line {
        self.icon = icon;
        self
    }

    fn switch(mut self, on: bool) -> Line {
        (self.control, self.on) = ("switch", on);
        self
    }

    fn select<'a>(mut self, value: &str, choices: impl IntoIterator<Item = (&'a str, &'a str)>) -> Line {
        (self.control, self.value) = ("select", value.to_string());
        self.choices = choices.into_iter().map(|(value, title)| (value.to_string(), title.to_string())).collect();
        self
    }

    fn field(mut self, value: impl Into<String>, placeholder: &str) -> Line {
        (self.control, self.value, self.placeholder) = ("field", value.into(), placeholder.to_string());
        self
    }

    /// A field for something that is never shown again once it is said:
    /// what was typed is taken out of it. (A "password" field is another
    /// thing: it shows, as dots, what is stored.)
    fn secret(mut self, placeholder: &str) -> Line {
        (self.control, self.placeholder) = ("secret", placeholder.to_string());
        self
    }

    fn button(mut self, words: &str) -> Line {
        (self.control, self.value) = ("button", words.to_string());
        self
    }

    /// A button that asks for a second click, saying `again` in between.
    fn confirm(mut self, words: &str, again: &str) -> Line {
        (self.control, self.value, self.placeholder) = ("confirm", words.to_string(), again.to_string());
        self
    }

    /// The whole row is pressed to go where its id says, which a chevron shows.
    fn link(mut self) -> Line {
        self.control = "link";
        self
    }

    fn chips(mut self, chips: Vec<String>) -> Line {
        self.chips = chips;
        self
    }

    fn badge(mut self, words: &str) -> Line {
        (self.control, self.value) = ("badge", words.to_string());
        self
    }

    fn text(mut self, words: impl Into<String>) -> Line {
        (self.control, self.value) = ("text", words.into());
        self
    }

    /// A further button, and what it acts on.
    fn more(mut self, words: &str, id: Id) -> Line {
        (self.more, self.more_id) = (words.to_string(), id.into());
        self
    }

    /// A field that records the hotkey `name`, showing the one bound.
    fn hotkey(mut self, name: &str, accelerator: Option<&Value>) -> Line {
        self.hotkey = name.to_string();
        self.keys = accelerator.and_then(Value::as_str).filter(|a| !a.is_empty()).map(keys::parts).unwrap_or_default();
        self
    }
}

/// What is different between the lines a page showed and the lines it
/// is to show, as Slint's list is told it.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Changes {
    /// How many lines at the beginning stay.
    pub head: usize,
    /// After them: how many of the old go, and how many new ones come in
    /// their place.
    pub out: usize,
    pub come: usize,
    /// Which of the lines that stay are not as they were, by their place
    /// among the new.
    pub changed: Vec<usize>,
}

/// What Slint's list is told when a page's lines become others: the
/// lines that begin and end both stay (changed, where they changed), and
/// what is between them is taken out and put in. A line stays if it is
/// the same line of the same thing, whatever it now says (`kind`, `id`
/// and `label`).
///
/// So a control that is being used stays the control it was (a switch
/// moves, a field keeps what is typed into it), a list that is opened
/// further down stays where it was scrolled to, and another page is not
/// left scrolled to where the last one was: Slint's list keeps its place
/// by the number of its first row, and forgets it only when that row is
/// taken out.
pub(crate) fn changes(was: &[Line], is: &[Line]) -> Changes {
    let alike = |was: &Line, is: &Line| was.kind == is.kind && was.id == is.id && was.label == is.label;
    let head = was.iter().zip(is).take_while(|(was, is)| alike(was, is)).count();
    let most = was.len().min(is.len()) - head;
    let tail = was.iter().rev().zip(is.iter().rev()).take(most).take_while(|(was, is)| alike(was, is)).count();
    let at_head = (0..head).filter(|at| was[*at] != is[*at]);
    let at_tail = (0..tail).filter(|back| was[was.len() - 1 - back] != is[is.len() - 1 - back]).map(|back| is.len() - 1 - back);
    Changes { head, out: was.len() - head - tail, come: is.len() - head - tail, changed: at_head.chain(at_tail).collect() }
}

/// A page being put together.
#[derive(Default)]
struct Page(Vec<Line>);

impl Page {
    fn note(&mut self, words: impl Into<String>) {
        self.0.push(Line { kind: "note", label: words.into(), ..Line::default() });
    }

    /// A card: its title, if it has one, and its rows.
    fn card(&mut self, title: &str, mut rows: Vec<Line>) {
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

    fn done(mut self) -> Vec<Line> {
        // Room under the last line, as a page has.
        self.0.push(Line { kind: "end", space: 24.0, ..Line::default() });
        self.0
    }
}

/// What the update button of the About page is doing.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) enum Update {
    /// Not asked yet.
    #[default]
    Unknown,
    Checking,
    /// This is the newest.
    Newest,
    /// A newer version, by its number.
    Newer(String),
    Installing(String),
}

/// One entry of the sidebar. `icon` is a file or the name of a symbol.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Entry {
    pub id: String,
    pub title: String,
    pub icon: Option<String>,
    pub extension: bool,
}

/// Something to ask of the host, and what its answer is for.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Ask {
    pub command: Command,
    pub then: Then,
}

/// What an answer is for: where `Model::answered` puts it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Then {
    /// The settings as they now are.
    Settings,
    /// The same, read again after the host refused a change: what it said
    /// stays on screen.
    Reread,
    Hotkeys,
    Capabilities,
    Extensions,
    Applications,
    ItemHotkeys,
    Dictation,
    /// The speech models of this computer (`dictation::models`).
    Speech,
    Ocr,
    Meaning,
    Sources,
    /// What the extension with this id may ask Heyo for.
    Permissions(String),
    /// The models the service that is this provider lists, by its id.
    Models(String),
    /// What checking this provider came to, by its id.
    ProviderChecked(String),
    /// A provider was added: the settings, with the ids there were before.
    Added(Vec<String>),
    Copied,
    Checked,
    Installed,
    /// The item's hotkey was set to this accelerator.
    Bound(String, String),
    /// A download was started; an answer only if it could not be.
    Started(&'static str),
    /// Something of the extensions changed: read them again.
    Reload,
    Uninstalled,
    Nothing,
}

fn ask(command: Command, then: Then) -> Ask {
    Ask { command, then }
}

/// Checking a provider's connection.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Check {
    Checking,
    /// What it came to, in words.
    Done(Result<String, String>),
}

/// Everything the window was told, and where it is.
#[derive(Debug, Default)]
pub(crate) struct Model {
    pub settings: Map<String, Value>,
    pub hotkeys: Value,
    pub capabilities: Value,
    pub dictation: Value,
    pub ocr: Value,
    pub meaning: Value,
    /// `clipboard_sources`: the applications one can be told to leave out.
    pub sources: Vec<Value>,
    /// The extension whose permissions are known, and they.
    pub permissions: (String, Vec<Value>),
    /// The models each OpenAI-compatible server listed, by the provider's
    /// id, or why it could not be asked.
    pub listed: BTreeMap<String, Result<Vec<String>, String>>,
    pub extensions: Vec<Value>,
    pub applications: Vec<Value>,
    pub item_hotkeys: Map<String, Value>,
    pub page: String,
    pub nav_search: String,
    /// What is typed into the field above a page's lines.
    pub find: String,
    /// The Shortcuts table: what it is narrowed to, and what is open.
    pub modifications: BTreeSet<String>,
    pub category: String,
    pub expanded: BTreeSet<String>,
    /// The speech models of this computer, as `dictation::models` says them.
    pub speech: Vec<Value>,
    /// What checking each provider came to, by its id.
    pub checks: BTreeMap<String, Check>,
    /// A provider being added, until it is.
    pub draft: Option<Provider>,
    pub ocr_installing: bool,
    pub meaning_installing: bool,
    pub update: Update,
    pub copied: bool,
    pub error: String,
}

fn text(value: &Value, name: &str) -> String {
    value.get(name).and_then(Value::as_str).unwrap_or_default().to_string()
}

fn flag(value: &Value, name: &str) -> bool {
    value.get(name).and_then(Value::as_bool).unwrap_or(false)
}

fn list<'a>(value: &'a Value, name: &str) -> &'a [Value] {
    value.get(name).and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

impl Model {
    pub(crate) fn new() -> Model {
        Model { page: "general".into(), category: "all".into(), expanded: BTreeSet::from(["extensions".to_string()]), ..Model::default() }
    }

    fn setting(&self, name: &str) -> &Value {
        self.settings.get(name).unwrap_or(&Value::Null)
    }

    fn set(&self, name: &str) -> bool {
        self.setting(name).as_bool().unwrap_or(false)
    }

    fn chosen(&self, name: &str) -> String {
        self.setting(name).as_str().unwrap_or_default().to_string()
    }

    fn names(&self, name: &str) -> Vec<&str> {
        self.setting(name).as_array().map(|all| all.iter().filter_map(Value::as_str).collect()).unwrap_or_default()
    }

    pub(crate) fn hotkeys_supported(&self) -> bool {
        flag(&self.hotkeys, "supported")
    }

    /// The applications as the extension the sidebar lists them as.
    fn applications_extension(&self) -> Value {
        let commands: Vec<Value> = self.applications.iter().map(|item| json!({ "id": item["id"], "name": item["id"], "title": item["title"], "description": item["subtitle"], "icon": item["icon"], "preferences": [] })).collect();
        json!({ "id": APPLICATIONS, "name": APPLICATIONS, "title": t("Applications"), "description": t("Give an application a hotkey to launch it from anywhere."), "icon": "app-window-grid-3x3-16", "builtin": true, "commands": commands, "preferences": [], "values": { "extension": {}, "commands": {} } })
    }

    /// Applications first, then Heyo's own commands, then the installed
    /// extensions by name.
    fn sidebar_extensions(&self) -> Vec<Value> {
        let rank = |extension: &Value| match extension["id"].as_str() {
            Some(APPLICATIONS) => 0,
            Some(BUILTIN) => 1,
            _ => 2,
        };
        let mut extensions: Vec<Value> = self.extensions.clone();
        extensions.push(self.applications_extension());
        extensions.sort_by(|a, b| rank(a).cmp(&rank(b)).then_with(|| text(a, "title").to_lowercase().cmp(&text(b, "title").to_lowercase())));
        extensions
    }

    fn extension_title(extension: &Value) -> String {
        if extension["id"] == BUILTIN { t("Heyo Commands").into() } else { text(extension, "title") }
    }

    /// What to show for an icon the host named: a file as it is, a file of
    /// the extension's own, or a symbol by its name.
    fn icon(extension: &Value, icon: &Value) -> Option<String> {
        let icon = icon.as_str().filter(|icon| !icon.is_empty())?;
        if std::path::Path::new(icon).is_absolute() || !icon.contains('.') {
            return Some(icon.to_string());
        }
        Some(format!("{}/assets/{icon}", text(extension, "id")))
    }

    /// What the sidebar lists: the pages, then the extensions, each only
    /// if the search finds it (an extension also by its commands, the
    /// applications by their name alone).
    pub(crate) fn nav(&self) -> (Vec<Entry>, usize) {
        let query = self.nav_search.to_lowercase();
        let found = |titles: Vec<String>| query.is_empty() || titles.iter().any(|title| title.to_lowercase().contains(&query));
        // A page is found by its name as it is shown, and by its English.
        let mut entries: Vec<Entry> = PAGES.iter().filter(|(_, title, _)| found(vec![translate(title).to_string(), title.to_string()])).map(|(id, title, icon)| Entry { id: id.to_string(), title: translate(title).to_string(), icon: Some(icon.to_string()), extension: false }).collect();
        let pages = entries.len();
        for extension in self.sidebar_extensions() {
            let mut titles = vec![text(&extension, "title")];
            if extension["id"] != APPLICATIONS {
                titles.extend(list(&extension, "commands").iter().map(|command| text(command, "title")));
            }
            if found(titles) {
                entries.push(Entry { id: text(&extension, "id"), title: Self::extension_title(&extension), icon: Self::icon(&extension, &extension["icon"]), extension: true });
            }
        }
        (entries, pages)
    }

    fn extension(&self) -> Option<Value> {
        if self.page == APPLICATIONS {
            return Some(self.applications_extension());
        }
        self.extensions.iter().find(|extension| extension["id"] == self.page.as_str()).cloned()
    }

    /// The page of the sidebar the one shown belongs to: a provider's
    /// belongs to Models.
    pub(crate) fn section(&self) -> &str {
        if self.page.starts_with(PROVIDER) { "models" } else { &self.page }
    }

    pub(crate) fn title(&self) -> String {
        match PAGES.iter().find(|(id, ..)| *id == self.section()) {
            Some((_, title, _)) => translate(title).to_string(),
            None => self.extension().as_ref().map(Self::extension_title).unwrap_or_else(|| t("Settings").into()),
        }
    }

    /// The words in the field above the page's lines, if it has one.
    pub(crate) fn find_placeholder(&self) -> &'static str {
        match self.page.as_str() {
            "shortcuts" => t("Search…"),
            APPLICATIONS => t("Search applications…"),
            _ => "",
        }
    }

    /// The menu beside that field: what its button says, how many things
    /// are ticked that narrow the table, and its entries (id, title,
    /// whether it is a heading, whether it is ticked).
    pub(crate) fn filter(&self) -> Option<(String, usize, Vec<(String, String, bool, bool)>)> {
        if self.page != "shortcuts" {
            return None;
        }
        let title = CATEGORIES.iter().find(|(id, _)| *id == self.category).map_or(t("All categories"), |(_, title)| translate(title));
        let mut picks = vec![(String::new(), t("Modifications").to_string(), true, false)];
        picks.extend(MODIFICATIONS.iter().map(|(id, title)| (Id::Modification(id.to_string()).into(), translate(title).to_string(), false, self.modifications.contains(*id))));
        picks.push((String::new(), t("Categories").to_string(), true, false));
        picks.extend(CATEGORIES.iter().map(|(id, title)| (Id::Category(id.to_string()).into(), translate(title).to_string(), false, self.category == *id)));
        Some((title.to_string(), self.modifications.len(), picks))
    }

    /// An entry of that menu was ticked.
    pub(crate) fn pick(&mut self, id: &str) {
        match Id::parse(id) {
            Id::Modification(modification) => {
                if !self.modifications.remove(&modification) {
                    self.modifications.insert(modification);
                }
            }
            Id::Category(category) => self.category = category,
            _ => {}
        }
    }

    /// The lines of the page that is shown.
    pub(crate) fn lines(&self) -> Vec<Line> {
        match self.page.as_str() {
            "general" => self.general(),
            "shortcuts" => self.shortcuts(),
            "dictation" => self.dictation_page(),
            "screenshots" => self.screenshots(),
            "models" => self.models_page(),
            "advanced" => self.advanced(),
            "about" => self.about(),
            page if page == format!("{PROVIDER}{ADDING}") => self.adding_page(),
            page if page == format!("{PROVIDER}{NEW}") => self.new_page(),
            _ if self.provider_page().is_some() => self.provider_page().and_then(|id| self.provider(id)).map(|provider| self.provider_page_lines(&provider)).unwrap_or_else(|| self.models_page()),
            _ => self.extension().map(|extension| self.extension_page(&extension)).unwrap_or_default(),
        }
    }

    // --- General -----------------------------------------------------------

    fn general(&self) -> Vec<Line> {
        let mut page = Page::default();
        // GNOME's Shell extension, where there is a use for it and it is
        // not running as it should.
        let extension = match self.capabilities["extension"].as_str() {
            Some("missing") => Some((t("On Wayland, switching and arranging windows, pasting, clipboard history, screenshots and the hold-to-talk key go through a small GNOME Shell extension that only Heyo can use."), t("Install"))),
            Some("outdated") => Some((t("This version of Heyo comes with a newer extension than the one installed. Update it, then log out and back in. An extension that is already running keeps working until then, without what the new one adds."), t("Update"))),
            Some("pending") => Some((t("In place. Log out and back in to finish: GNOME loads a new or updated extension only when a session starts."), "")),
            _ => None,
        };
        if let Some((description, action)) = extension {
            let row = Line::row(Id::Act(Act::InstallShellExtension), t("GNOME Shell Extension")).says(description);
            page.card(t("Wayland"), vec![if action.is_empty() { row } else { row.button(action) }]);
        }
        let hotkey = Line::row("", t("Heyo Hotkey"));
        page.card(
            "",
            vec![
                Line::row(Id::Set("launchAtLogin".into()), t("Open at Login")).switch(self.set("launchAtLogin")),
                if self.hotkeys_supported() { hotkey.hotkey(&Bound::Own("toggle".into()).to_string(), self.hotkeys.get("toggle")) } else { hotkey.says(t("Bind a desktop keyboard shortcut to heyo.")) },
            ],
        );
        // Each language by its own name, as `i18n::languages` has them,
        // English too: whoever cannot read the one in use finds theirs.
        let languages = [(i18n::SYSTEM, t("System")), (i18n::SOURCE, "English")].into_iter().chain(i18n::languages());
        page.card(
            t("Appearance"),
            vec![
                Line::row(Id::Set("appearance".into()), t("Theme")).says(t("Light, dark, or whichever the system is using")).select(&self.chosen("appearance"), [("system", t("System")), ("light", t("Light")), ("dark", t("Dark"))]),
                Line::row(Id::Set("language".into()), t("Language")).says(t("What Heyo itself says. Extensions speak their own.")).select(&self.chosen("language"), languages),
            ],
        );
        page.card(
            t("Window"),
            vec![
                Line::row(Id::Set("compactMode".into()), t("Compact Mode")).says(t("Show only the search field until you type")).switch(self.set("compactMode")),
                Line::row(Id::Set("hideOnBlur".into()), t("Hide When Focus Is Lost")).says(t("Close the window when you click elsewhere")).switch(self.set("hideOnBlur")),
                Line::row(Id::Set("keepCommand".into()), t("Keep the Running Command")).says(t("Reopening Heyo returns to the command you were using instead of the root search")).switch(self.set("keepCommand")),
            ],
        );
        page.card(t("Live Activities"), vec![Line::row(Id::Set("liveActivities".into()), t("Show What Is in Progress")).says(t("Timers, downloads and what extensions report as under way, at the top of the root search, with a notification when one ends or needs you")).switch(self.set("liveActivities"))]);
        page.done()
    }

    // --- Shortcuts ---------------------------------------------------------

    fn disabled(&self) -> BTreeSet<&str> {
        self.names("disabledCommands").into_iter().collect()
    }

    /// The table: the applications and the extensions with their commands,
    /// each with an alias, a hotkey and (for a command) whether it is on.
    fn shortcuts(&self) -> Vec<Line> {
        let query = self.find.to_lowercase();
        let filtering = !query.is_empty() || !self.modifications.is_empty();
        let (disabled, aliases) = (self.disabled(), self.setting("aliases"));
        let open = |id: &str| filtering || self.expanded.contains(id);
        let visible = |command: &Value, group: &str, application: bool| {
            let id = text(command, "id");
            if !query.is_empty() && !text(command, "title").to_lowercase().contains(&query) && !group.to_lowercase().contains(&query) {
                return false;
            }
            if self.modifications.contains("hotkey") && !self.item_hotkeys.contains_key(&id) {
                return false;
            }
            if self.modifications.contains("alias") && aliases.get(&id).is_none() {
                return false;
            }
            let (enabled, off) = (self.modifications.contains("enabled"), self.modifications.contains("disabled"));
            // Applications are always enabled.
            enabled == off || (application || !disabled.contains(id.as_str())) == enabled
        };
        let entry = |extension: &Value, command: &Value, indent: bool, application: bool| {
            let id = text(command, "id");
            Line {
                kind: "entry",
                label: text(command, "title"),
                icon: Self::icon(extension, &command["icon"]),
                alias: text(aliases, &id),
                alias_id: Id::Alias(id.clone()).into(),
                check_id: Id::Enable(id.clone()).into(),
                hotkey: if id == CLIPBOARD_COMMAND { String::new() } else { id.clone() },
                keys: self.item_hotkeys.get(&id).and_then(Value::as_str).map(keys::parts).unwrap_or_default(),
                checkable: !application,
                on: !disabled.contains(id.as_str()),
                indent,
                id,
                ..Line::default()
            }
        };
        let group = |id: &str, label: &str, icon: &str| Line { kind: "group", id: Id::Open(id.to_string()).into(), label: label.to_string(), icon: Some(icon.to_string()), open: open(id), ..Line::default() };
        let mut page = Page::default();

        let applications = self.applications_extension();
        let apps: Vec<&Value> = list(&applications, "commands").iter().filter(|command| visible(command, t("Applications"), true)).collect();
        if self.category != "extensions" && (!apps.is_empty() || !filtering) {
            let mut rows = vec![group("applications", t("Applications"), "app-window-grid-3x3-16")];
            if open("applications") {
                rows.push(Line { kind: "columns", ..Line::default() });
                rows.extend(apps.iter().map(|command| entry(&applications, command, false, true)));
            }
            page.card("", rows);
        }

        let groups: Vec<(&Value, Vec<&Value>)> = self.extensions.iter().map(|extension| (extension, list(extension, "commands").iter().filter(|command| visible(command, &text(extension, "title"), false)).collect::<Vec<_>>())).filter(|(_, commands)| !commands.is_empty()).collect();
        if self.category != "applications" && (!groups.is_empty() || !filtering) {
            let mut rows = vec![group("extensions", t("Extensions"), "plug-16")];
            if open("extensions") {
                rows.push(Line { kind: "columns", ..Line::default() });
                for (extension, commands) in &groups {
                    let all = list(extension, "commands");
                    // An extension with one command is shown as that command.
                    if all.len() == 1 {
                        rows.push(entry(extension, commands[0], false, false));
                        continue;
                    }
                    let id = text(extension, "id");
                    let enabled = all.iter().filter(|command| !disabled.contains(text(command, "id").as_str())).count();
                    rows.push(Line { kind: "group", id: Id::Open(id.to_string()).into(), label: Self::extension_title(extension), icon: Self::icon(extension, &extension["icon"]), indent: true, open: open(&id), checkable: true, check_id: Id::EnableAll(id.to_string()).into(), on: enabled > 0, mixed: enabled > 0 && enabled < all.len(), ..Line::default() });
                    if open(&id) {
                        rows.extend(commands.iter().map(|command| entry(extension, command, true, false)));
                    }
                }
            }
            page.card("", rows);
        }
        if filtering && apps.is_empty() && groups.is_empty() {
            page.note(t("Nothing matches."));
        }
        page.done()
    }

    // --- Dictation ---------------------------------------------------------

    /// What "Type as You Speak" says of this session, by the ways dictated
    /// words can reach an application in it.
    fn live_description(&self) -> String {
        let ways: Vec<&str> = list(&self.capabilities, "dictationWords").iter().filter_map(Value::as_str).collect();
        match ways.first() {
            None => String::new(),
            Some(&"composed") => tf(
                "Your words go into the window as you speak, through the input method: the sentence you are speaking is shown there as text being composed, and becomes the window's text when it is finished. Your own input method (Pinyin, say) is set aside while you dictate and comes back when you finish. {} When this is off, nothing is shown before a sentence is finished.",
                &[&if ways.contains(&"rewritten") {
                    t("A window that takes no input method gets the words by pasting, corrected where they stand until you press a key or click.")
                } else {
                    t("A window that takes no input method gets each sentence pasted when it is finished.")
                }],
            ),
            Some(&"rewritten") => t("Your words go into the window as they are recognized, and are corrected there when a later word changes an earlier one. Heyo only ever erases what it wrote itself: once you press a key or click while dictating, it stops correcting and adds each finished sentence where the cursor is. In terminals, password managers and the applications excluded from snippet expansion, and when this is off, each sentence is pasted when it is finished.").into(),
            Some(_) if self.capabilities["session"] == "wayland" => t("Not available on Wayland, where Heyo cannot tell whether you typed in between, and so must not erase anything. Each sentence is pasted when it is finished.").into(),
            Some(_) => t("Not available in this desktop session. Each sentence is pasted when it is finished.").into(),
        }
    }

    fn dictation_page(&self) -> Vec<Line> {
        let mut page = Page::default();
        page.note(t("Hold the hotkey while you speak and let go, or tap it to start and tap again to finish: what you said is typed into the window you are working in."));
        // Speech models are downloaded on the Models page.
        let engine = vec![self.picker(Use::DictationRecognition, t("Speech Recognition"), t("Models on this computer keep your audio here; a service is sent it. Download speech models in Models."), None)];
        let hotkey = Line::row("", t("Hotkey"));
        let hotkey = if self.hotkeys_supported() { hotkey.says(t("Tap to start and tap again to finish, or hold it while you speak")).hotkey(&Bound::Own("dictation".into()).to_string(), self.hotkeys.get("dictation")) } else { hotkey.says(t("Bind a desktop keyboard shortcut to heyo --dictate.")) };
        let known = !self.capabilities.is_null();
        let hold = Line::row(Id::Set("dictationHoldKey".into()), t("Hold-to-Talk Key"));
        let hold = if self.capabilities["holdKey"] == "none" {
            hold.says(t("A single key cannot be watched in this desktop session; use the hotkey instead."))
        } else {
            let hold = hold.says(if self.chosen("dictationHoldKey").is_empty() { t("A single key to hold while you speak. It keeps working as usual in shortcuts.") } else { t("Hold it and speak; let go to finish. A quick tap, or using it in a shortcut, does nothing.") });
            if known { hold.select(&self.chosen("dictationHoldKey"), std::iter::once(("", t("None"))).chain(list(&self.capabilities, "holdKeys").iter().filter_map(|key| Some((key["value"].as_str()?, key["title"].as_str()?))))) } else { hold }
        };
        let mut rows = engine;
        rows.extend([hotkey, hold]);
        let live = self.live_description();
        if !live.is_empty() {
            let row = Line::row(Id::Set("dictationLive".into()), t("Type as You Speak")).says(live);
            rows.push(if list(&self.capabilities, "dictationWords").first().is_some_and(|way| way != "pasted") { row.switch(self.set("dictationLive")) } else { row });
        }
        page.card("", rows);

        let microphones = list(&self.dictation, "microphones");
        let none = !self.dictation.is_null() && microphones.is_empty();
        page.card(
            t("Input"),
            vec![
                Line::row(Id::Set("dictationSource".into()), t("Microphone")).says(if none { t("No microphone found. Connect one to use dictation.") } else { "" }).warns(none).select(&self.chosen("dictationSource"), std::iter::once(("", t("System Default"))).chain(microphones.iter().filter_map(|m| Some((m["name"].as_str()?, m["description"].as_str()?))))),
                Line::row(Id::Set("dictationLanguage".into()), t("Language")).says(t("Automatic detection handles mixed Chinese and English")).select(&self.chosen("dictationLanguage"), DICTATION_LANGUAGES.iter().map(|(value, title)| (*value, translate(title)))),
            ],
        );
        page.card(
            t("Vocabulary"),
            vec![
                Line::row("", t("Your Words")).says(t("Names, jargon and product names, one to a line, written the way you want them: “type script” then comes out as TypeScript. For a word that is heard as something else, write heard as -> written as. The recognizer cannot be taught new words, so these are put right in its text; a Chinese name heard as other characters of the same sound is only fixed if you list those characters as “heard as”. Polishing with AI is given the list too.")),
                Line { kind: "area", id: Id::Words.into(), label: t("Your Words").into(), value: self.names("dictationVocabulary").join("\n"), placeholder: "TypeScript\nKubernetes\njason -> JSON\n杰森 -> JSON".into(), ..Line::default() },
            ],
        );
        let polish = self.chosen("dictationPolish");
        let mode = POLISH_MODES.iter().find(|(value, ..)| *value == polish).unwrap_or(&POLISH_MODES[0]);
        let mut rows = vec![Line::row(Id::Set("dictationPolish".into()), t("Mode")).says(translate(mode.2)).select(mode.0, POLISH_MODES.iter().map(|(value, title, _)| (*value, translate(title))))];
        if mode.0 != "off" {
            rows.push(self.picker(Use::DictationPolish, t("Model"), "", None));
        }
        page.card(t("Polish with AI"), rows);
        page.card("", vec![Self::to_models()]);
        page.done()
    }

    // --- Screenshots -------------------------------------------------------

    fn screenshots(&self) -> Vec<Line> {
        let mut page = Page::default();
        page.note(t("Drag to select part of the screen, or click a window to select exactly that. Mark it up with rectangles, arrows or the pen, then copy it, save it to Pictures/Screenshots, pin it on top of other windows, or copy the text in it."));
        let hotkey = |id: &str, title: &str, description: &str| {
            let row = Line::row("", title).says(description);
            if self.hotkeys_supported() { row.hotkey(id, self.item_hotkeys.get(id)) } else { row }
        };
        page.card(t("Hotkeys"), vec![hotkey("heyo:screenshot", t("Capture Screenshot"), t("Enter copies, Ctrl+S saves, Esc cancels")), hotkey("heyo:screenshot-text", t("Capture Text from Screen"), t("Drag over text and it is copied"))]);
        let installed = flag(&self.ocr, "installed");
        let engine = Line::row(Id::Act(Act::InstallOcr), t("Recognition Engine")).says(if installed { t("PaddleOCR is installed. Chinese and English; images never leave this computer.") } else { t("A one-time download of about 340 MB (PaddleOCR, Chinese and English).") });
        page.card(t("Text Recognition"), vec![if installed { engine.badge(t("Installed")) } else if self.ocr_installing || flag(&self.ocr, "installing") { engine.text("Downloading…") } else { engine.button(t("Download")) }]);
        page.done()
    }

    // --- Advanced ----------------------------------------------------------

    fn advanced(&self) -> Vec<Line> {
        let mut page = Page::default();
        let node = self.setting("nodeVersion");
        let found = node.get("Ok").and_then(Value::as_str);
        let version = match found {
            Some(version) => tf("Using {0}.", &[&version]),
            None => text(node, "Err"),
        };
        page.card(
            t("Extensions"),
            vec![
                Line::row(Id::Set("nodePath".into()), t("Node.js")).says(format!("{version}{} {}", if self.set("nodeOverridden") { format!(" {}", t("HEYO_NODE overrides this setting.")) } else { String::new() }, t("Extensions need Node.js 22 or newer."))).warns(found.is_none()).field(self.chosen("nodePath"), "node (from PATH)"),
                Line::row(Id::Act(Act::OpenExtensions), t("Extensions Folder")).says(self.chosen("extensionsDir")).button(t("Open")),
            ],
        );
        page.card(
            t("Rendering"),
            vec![Line::row(Id::Set("renderer".into()), t("Renderer")).says(t("How the palette and what else the web view draws is drawn. Automatic keeps to the CPU on an NVIDIA driver without modesetting (the GPU cannot share frames there) and lets WebKit choose elsewhere; GPU with shared memory draws on the GPU and copies each frame, which is faster at 200% scale and keeps every CSS effect. Takes effect when Heyo is next started.")).select(&self.chosen("renderer"), RENDERERS.iter().map(|(value, title)| (*value, translate(title))))],
        );
        page.card(
            "AI",
            vec![
                self.picker(Use::Answers, t("Model"), t("Answers Ask AI, AI Chat, Selected Text Actions and extensions."), None),
                self.picker(Use::ClipboardCases, t("Clipboard Search Cases"), t("Writes the cases the clipboard search is measured by, from what you copied: only a model that keeps it on this computer is offered."), Some(t("None"))),
                Self::to_models(),
            ],
        );
        page.card(t("Pasting"), vec![Line::row(Id::Set("confirmTerminalPaste".into()), t("Ask Before Pasting Several Lines into a Terminal")).says(t("A terminal runs each pasted line as a command. Dictation never asks: there, line breaks become spaces.")).switch(self.set("confirmTerminalPaste"))]);
        page.card(t("Data"), vec![Line::row("", t("Settings File")).says(self.chosen("settingsPath"))]);
        page.done()
    }

    // --- Providers, and the models each use refers to ---------------------

    /// The providers, as the host told them (their secrets taken out).
    /// A secret that is set stands as something in its field, so that
    /// what needs it is not taken for missing.
    fn providers(&self) -> Vec<Provider> {
        let mut providers: Vec<Provider> = serde_json::from_value(self.setting("providers").clone()).unwrap_or_default();
        for provider in &mut providers {
            for name in list(self.provider_state(&provider.id), "secretsSet").iter().filter_map(Value::as_str) {
                provider.config.insert(name.to_string(), "(set)".into());
            }
        }
        providers
    }

    fn provider(&self, id: &str) -> Option<Provider> {
        self.providers().into_iter().find(|provider| provider.id == id)
    }

    /// The model `need` refers to.
    fn reference(&self, need: Use) -> ModelRef {
        serde_json::from_value(self.setting(need.setting()).clone()).unwrap_or_default()
    }

    /// What the host said of a provider besides its settings.
    fn provider_state(&self, id: &str) -> &Value {
        self.setting("providerStates").get(id).unwrap_or(&Value::Null)
    }

    fn secret_set(&self, id: &str, field: &str) -> bool {
        list(self.provider_state(id), "secretsSet").iter().any(|name| name == field)
    }

    /// The models each service listed, of those that answered.
    fn listed_models(&self) -> BTreeMap<String, Vec<String>> {
        self.listed.iter().filter_map(|(id, models)| Some((id.clone(), models.as_ref().ok()?.clone()))).collect()
    }

    /// Whether the speech model `id` is downloaded; for none, the one in use.
    fn speech_installed(&self, id: &str) -> bool {
        self.speech.iter().any(|entry| (if id.is_empty() { flag(entry, "inUse") } else { entry["id"] == id }) && flag(entry, "installed"))
    }

    /// What is known of a provider besides its settings, as its kind's
    /// status is said from.
    fn known(&self, provider: &Provider) -> Known {
        let found = &self.provider_state(&provider.id)["cliFound"];
        let found = (!found.is_null()).then(|| match (found.get("Ok"), found.get("Err").and_then(Value::as_str)) {
            (Some(found), _) => Ok((text(found, "version"), text(found, "path"))),
            (_, why) => Err(why.unwrap_or_default().to_string()),
        });
        let source = provider.kind().source();
        let listed = if source == Source::Listed && provider.unset().is_none() { self.listed.get(&provider.id).cloned() } else { None };
        let downloaded = (source == Source::Catalogue && !self.speech.is_empty()).then(|| self.speech.iter().filter(|entry| flag(entry, "installed")).count());
        Known { found, listed, downloaded }
    }

    /// What keeps a provider from being used as it is now, as far as is
    /// known; none if nothing does.
    fn trouble(&self, provider: &Provider) -> Option<String> {
        if let Some(missing) = provider.unset() {
            return Some(translate(missing).into());
        }
        let known = self.known(provider);
        match (known.found, known.listed) {
            (Some(Err(why)), _) => Some(why),
            (_, Some(Err(e))) => Some(tf("Not reachable: {0}.", &[&e])),
            _ => None,
        }
    }

    /// The chips of what a provider can do.
    fn abilities(provider: &Provider) -> Vec<String> {
        kinds::ABILITIES.iter().filter(|(can, _)| provider.can(*can)).map(|(_, word)| translate(word).to_string()).collect()
    }

    /// A picker of the model `need` refers to: each model of each provider
    /// that can be used for it, as "provider · model", and `off` first where
    /// using none is a choice. What it refers to that cannot be used, or is
    /// gone, is said as a warning.
    fn picker(&self, need: Use, label: &str, description: &str, off: Option<&str>) -> Line {
        let providers = self.providers();
        let reference = self.reference(need);
        let mut choices: Vec<(String, String)> = off.map(|off| (String::new(), off.to_string())).into_iter().collect();
        choices.extend(registry::offered(&providers, need, &self.listed_models()).into_iter().map(|(offered, title)| (ids::choice(&offered.provider, &offered.model), title)));
        let value = if reference.is_none() { String::new() } else { ids::choice(&reference.provider, &reference.model) };
        let problem = match registry::resolve(&providers, &reference, need) {
            Err(registry::Unusable::NoneChosen) if off.is_some() => None,
            Err(why) => Some(crate::models::ModelError::Unusable(why).message()),
            Ok((provider, model)) => self.trouble(provider).or_else(|| (provider.kind().source() == Source::Catalogue && !self.speech.is_empty() && !self.speech_installed(model)).then(|| tf("{0} is not downloaded yet. Download it in Models.", &[&provider.model_title(model)]))),
        };
        if !choices.iter().any(|(choice, _)| *choice == value) {
            let name = providers.iter().find(|provider| provider.id == reference.provider).map_or(reference.provider.clone(), |provider| provider.name.clone());
            let title = if reference.is_none() { t("None").to_string() } else { format!("{name} · {}", if reference.model.is_empty() { t("Default") } else { &reference.model }) };
            choices.push((value.clone(), title));
        }
        let said = match &problem {
            Some(problem) if !description.is_empty() => format!("{problem} {description}"),
            Some(problem) => problem.clone(),
            None => description.to_string(),
        };
        let mut line = Line::row(Id::Pick(need.setting().into()), label).says(said).warns(problem.is_some());
        (line.control, line.value, line.choices) = ("select", value, choices);
        line
    }

    /// The row that goes to the Models page, under the pickers.
    fn to_models() -> Line {
        Line::row(Id::Act(Act::OpenModels), t("Providers")).says(t("Connect Claude, servers such as Ollama, Claude Code, Codex and speech services, and download speech models.")).button(t("Open Models"))
    }

    /// What is asked to know the models of every service that lists them
    /// and is set up.
    fn listings(&self) -> Vec<Ask> {
        self.providers().into_iter().filter(|provider| provider.kind().source() == Source::Listed && provider.unset().is_none()).map(|provider| ask(Command::ProviderModels(provider.id.clone()), Then::Models(provider.id))).collect()
    }

    // --- Models: the providers, each on a page of its own --------------------

    /// The provider whose page is shown, by its id; none on another page.
    fn provider_page(&self) -> Option<&str> {
        self.page.strip_prefix(PROVIDER).filter(|id| *id != ADDING && *id != NEW)
    }

    /// The list of the providers, and the way to add one.
    fn models_page(&self) -> Vec<Line> {
        let mut page = Page::default();
        page.note(t("Providers run the models Heyo's features use. Add one, then choose its models where each feature is set up: AI in Advanced, speech recognition and polishing in Dictation."));
        let rows = self
            .providers()
            .iter()
            .map(|provider| {
                let kind = provider.kind();
                let status = kind.status(provider, &self.known(provider));
                let title = translate(kind.title());
                // Its kind, where its name does not say it already.
                let said = if provider.name == title { status.short } else { format!("{title} · {}", status.short) };
                Line::row(Id::Go(format!("{PROVIDER}{}", provider.id)), provider.name.clone()).says(said).warns(status.warning).pictured(Some(kind.icon().into())).chips(Self::abilities(provider)).link()
            })
            .collect();
        page.card(t("Providers"), rows);
        page.card("", vec![Line::row(Id::Go(format!("{PROVIDER}{ADDING}")), t("Add Provider…")).says(t("Several of one kind can be added, two servers for instance.")).pictured(Some("plus".into())).link()]);
        page.done()
    }

    /// The top of a provider's page: the way back, and what it is.
    fn top(page: &mut Page, back: &str, back_title: &str, icon: &str, title: String, about: String) {
        page.0.push(Line { kind: "back", id: Id::Go(back.into()).into(), label: back_title.into(), ..Line::default() });
        page.0.push(Line { kind: "head", label: title, description: about, icon: Some(icon.into()), ..Line::default() });
    }

    /// Choosing the kind of a new provider.
    fn adding_page(&self) -> Vec<Line> {
        let mut page = Page::default();
        Self::top(&mut page, "models", t("Models"), "plus", t("Add Provider").into(), t("Choose what to connect.").into());
        let rows = kinds::addable().map(|kind| Line::row(Id::NewProvider(kind.word().into()), translate(kind.title())).says(translate(kind.about())).pictured(Some(kind.icon().into())).chips(Self::abilities(&Provider::new("", kind.word()))).link()).collect();
        page.card("", rows);
        page.done()
    }

    /// The fields of a provider, to edit: its name (unless it is always
    /// there) and its kind's fields that apply. A new one's (`draft`) are
    /// kept here until it is added.
    fn form(&self, provider: &Provider, draft: bool) -> Vec<Line> {
        let id = |field: &str| if draft { Id::DraftField(field.into()) } else { Id::ProviderField { provider: provider.id.clone(), field: field.into() } };
        let kind = provider.kind();
        let mut rows = Vec::new();
        if !kind.builtin() {
            rows.push(Line::row(id("name"), t("Name")).field(provider.name.clone(), translate(kind.title())));
        }
        for field in kind.fields().iter().filter(|field| provider.applies(field)) {
            let row = Line::row(id(field.name), translate(field.title)).says(translate(field.description));
            let set = if draft { !provider.get(field.name).is_empty() } else { self.secret_set(&provider.id, field.name) };
            rows.push(match field.entry {
                Field::Secret if set && draft => row.secret(t("Typed — type again to replace it")),
                Field::Secret if set => row.secret(t("Saved — type a new key to replace it")).more(t("Remove"), Id::ForgetSecret { provider: provider.id.clone(), field: field.name.into() }),
                Field::Secret => row.secret(field.placeholder),
                Field::Choice(choices) => {
                    let chosen = provider.get(field.name);
                    row.select(if chosen.is_empty() { choices[0].0 } else { chosen }, choices.iter().map(|(value, title)| (*value, translate(title))))
                }
                Field::Switch => row.switch(provider.get(field.name) == "true"),
                _ => row.field(provider.config.get(field.name).cloned().unwrap_or_default(), field.placeholder),
            });
        }
        rows
    }

    /// A new provider's form, until it is added or left.
    fn new_page(&self) -> Vec<Line> {
        let Some(draft) = &self.draft else { return self.adding_page() };
        let kind = draft.kind();
        let mut page = Page::default();
        Self::top(&mut page, &format!("{PROVIDER}{ADDING}"), t("Add Provider"), kind.icon(), tf("New {0}", &[&translate(kind.title())]), translate(kind.about()).into());
        page.card(t("Settings"), self.form(draft, true));
        page.card("", vec![Line::row(Id::Act(Act::SaveProvider), t("Add This Provider")).says(t("Its models can then be chosen where each feature is set up. It can be changed or removed here later.")).button(t("Add")).more(t("Cancel"), Id::Go("models".into()))]);
        page.done()
    }

    /// Which uses refer to `model` of `provider`, in a word each.
    fn used_for(&self, provider: &Provider, model: &str) -> Vec<String> {
        const USES: &[(Use, &str)] = &[(Use::Answers, marked("AI")), (Use::DictationRecognition, marked("Dictation")), (Use::DictationPolish, marked("Polish")), (Use::ClipboardCases, marked("Clipboard Search"))];
        USES.iter().filter(|(need, _)| need.suits(provider) && self.reference(*need) == ModelRef::to(&provider.id, model)).map(|(_, word)| translate(word).to_string()).collect()
    }

    /// A provider's models, as its kind has them: a catalogue to download
    /// from, what the service lists, or what is known of it. Each says what
    /// uses it.
    fn model_rows(&self, provider: &Provider) -> Vec<Line> {
        let kind = provider.kind();
        if kind.source() == Source::Catalogue {
            return self.speech_rows();
        }
        let mut models = provider.models();
        let mut rows = Vec::new();
        if kind.source() == Source::Listed {
            match self.listed.get(&provider.id) {
                Some(Ok(listed)) => models.extend(listed.iter().filter(|name| !models.contains(name)).cloned().collect::<Vec<_>>()),
                Some(Err(e)) => rows.push(Line::row("", t("Not reachable")).says(e.clone()).warns(true)),
                None if provider.unset().is_none() => rows.push(Line::row("", t("Checking…"))),
                None => {}
            }
        }
        rows.extend(models.iter().map(|model| {
            let title = provider.model_title(model);
            let row = Line::row("", title.clone()).says(if title == *model || model.is_empty() { String::new() } else { model.clone() });
            let used = self.used_for(provider, model);
            if used.is_empty() { row } else { row.badge(&tf("Used for {0}", &[&used.join(", ")])) }
        }));
        rows
    }

    /// A provider's page: how it is (and checking it), its settings, what
    /// it can do, its models, and removing it.
    fn provider_page_lines(&self, provider: &Provider) -> Vec<Line> {
        let kind = provider.kind();
        let mut page = Page::default();
        // Its kind, where its name does not say it already.
        let title = translate(kind.title());
        Self::top(&mut page, "models", t("Models"), kind.icon(), provider.name.clone(), if provider.name == title { String::new() } else { title.into() });
        let status = kind.status(provider, &self.known(provider));
        let mut rows = vec![Line::row(Id::CheckProvider(provider.id.clone()), t("Status")).says(status.long).warns(status.warning)];
        rows[0] = match self.checks.get(&provider.id) {
            Some(Check::Checking) => rows[0].clone().button(t("Checking…")),
            _ => rows[0].clone().button(t("Check Connection")),
        };
        rows[0].busy = self.checks.get(&provider.id) == Some(&Check::Checking);
        if let Some(Check::Done(result)) = self.checks.get(&provider.id) {
            let (said, failed) = match result {
                Ok(said) => (said.clone(), false),
                Err(why) => (why.clone(), true),
            };
            rows.push(Line::row("", if failed { t("Connection Failed") } else { t("Connection Works") }).says(said).warns(failed));
        }
        rows.push(Line::row("", t("What It Can Do")).says(translate(kind.goes(provider))).chips(Self::abilities(provider)));
        page.card("", rows);
        page.card(t("Settings"), self.form(provider, false));
        page.card(t("Models"), self.model_rows(provider));
        if !kind.builtin() {
            page.card("", vec![Line::row(Id::RemoveProvider(provider.id.clone()), t("Remove Provider")).says(t("What uses its models says so until another is chosen.")).confirm(t("Remove…"), t("Click Again to Remove"))]);
        }
        page.done()
    }

    /// The speech models of this computer: each with what it hears, how
    /// much it is, and what can be done with it (download it, or call
    /// the download off; use it, or delete it).
    fn speech_rows(&self) -> Vec<Line> {
        self.speech
            .iter()
            .map(|entry| {
                let id = text(entry, "id");
                let size = tf("{0} MB", &[&entry["megabytes"].as_u64().unwrap_or(0)]);
                let row = Line::row(Id::SpeechDownload(id.clone()), text(entry, "name")).says(tf("{0} · {1}. {2}", &[&text(entry, "languages"), &size, &text(entry, "note")]));
                let going = &entry["downloading"];
                if !going.is_null() {
                    let percent = match (going["received"].as_f64(), going["total"].as_f64()) {
                        (Some(received), Some(total)) if total > 0.0 => format!(" {}%", (received / total * 100.0).round()),
                        _ => "…".into(),
                    };
                    row.text(format!("{}{percent}", crate::helper::step(&text(going, "step")))).more(t("Cancel"), Id::SpeechCancel(id))
                } else if flag(entry, "installed") && flag(entry, "inUse") {
                    row.badge(t("In Use"))
                } else if flag(entry, "installed") {
                    Line { id: Id::SpeechDelete(id.clone()).into(), ..row }.confirm(t("Delete…"), t("Click Again to Delete")).more(t("Use"), Id::SpeechUse(id))
                } else {
                    row.button(t("Download"))
                }
            })
            .collect()
    }

    // --- About -------------------------------------------------------------

    fn about(&self) -> Vec<Line> {
        let update = match &self.update {
            _ if !self.set("canUpdate") => String::new(),
            Update::Unknown => t("Check for Updates").into(),
            Update::Checking => t("Checking…").into(),
            Update::Newest => t("Heyo Is Up to Date").into(),
            Update::Newer(version) => tf("Install Heyo {0} and Restart", &[version]),
            Update::Installing(_) => t("Installing…").into(),
        };
        vec![Line {
            kind: "about",
            // Its two buttons: the diagnostics, and the update.
            id: Id::Act(Act::CopyDiagnostics).into(),
            more_id: Id::Act(Act::Update).into(),
            value: self.chosen("version"),
            description: t("A command palette launcher for Linux that runs Raycast extensions.").into(),
            placeholder: t("For a bug report: versions, the desktop session and the end of Heyo's log, with keys and tokens removed. Read it before you share it.").into(),
            more: update,
            busy: matches!(self.update, Update::Checking | Update::Installing(_)),
            on: self.copied,
            ..Line::default()
        }]
    }

    // --- An extension's page -----------------------------------------------

    /// One preference of an extension or of one of its commands, as a row.
    fn preference(extension: &Value, command: &str, preference: &Value, stored: &Value) -> Line {
        let name = text(preference, "name");
        let kind = text(preference, "type");
        let value = if stored.is_null() { &preference["default"] } else { stored };
        let (title, label) = (preference["title"].as_str(), preference["label"].as_str());
        let row = Line::row(Id::Preference { extension: text(extension, "name"), command: command.to_string(), name: name.to_string(), kind: kind.to_string() }, format!("{}{}", title.or(label).unwrap_or(&name), if flag(preference, "required") { " *" } else { "" }));
        // A checkbox's label says what ticking it does, where it has a title too.
        let row = row.says(if kind == "checkbox" && label.is_some() && title.is_some() { label.unwrap_or_default().to_string() } else { text(preference, "description") });
        match kind.as_str() {
            "checkbox" => row.switch(value == true),
            "dropdown" => {
                let choices: Vec<(&str, &str)> = list(preference, "data").iter().filter_map(|choice| Some((choice["value"].as_str()?, choice["title"].as_str()?))).collect();
                let chosen = value.as_str().map(str::to_string).or_else(|| choices.first().map(|(value, _)| value.to_string())).unwrap_or_default();
                row.select(&chosen, choices)
            }
            _ => {
                let shown = match value {
                    Value::Null => String::new(),
                    Value::String(text) => text.clone(),
                    other => other.to_string(),
                };
                let placeholder = preference["placeholder"].as_str().unwrap_or(if kind == "directory" || kind == "file" { "/path" } else { "" });
                let mut row = row.field(shown, placeholder);
                if kind == "password" {
                    row.control = "password";
                }
                row
            }
        }
    }

    /// An application as a row of a card: one that is left out of
    /// something, with the button that takes it off the list.
    fn left_out(&self, id: &str, remove: fn(String) -> Id, missing: &str) -> Line {
        let source = self.sources.iter().find(|source| source["id"] == id);
        Line::row(remove(id.to_string()), source.map_or(id.to_string(), |source| text(source, "name")))
            .says(if source.is_some() { "" } else { missing })
            .pictured(Some(source.and_then(|source| source["icon"].as_str()).unwrap_or("app-window-16").to_string()))
            .button(t("Remove"))
    }

    /// The cards about the clipboard history, on the page of Heyo's commands.
    fn clipboard(&self, page: &mut Page) {
        let recording = self.set("clipboardHistory");
        let mut rows = Vec::new();
        if self.hotkeys_supported() {
            rows.push(Line::row("", t("Hotkey")).says(t("Opens Clipboard History from anywhere")).hotkey(&Bound::Own("clipboard".into()).to_string(), self.hotkeys.get("clipboard")));
        }
        rows.push(Line::row(Id::Set("clipboardHistory".into()), t("Record Clipboard History")).says(t("Copies marked as secret by password managers are never recorded")).switch(recording));
        if recording {
            rows.push(Line::row(Id::Set("clipboardPaused".into()), t("Pause Recording")).says(t("Nothing you copy is recorded while this is on. “Pause or Resume Clipboard History” in the root search does the same, and can have a hotkey.")).switch(self.set("clipboardPaused")));
        }
        rows.push(Line::row(Id::Num("clipboardRetentionDays".into()), t("Keep History For")).says(t("Pinned entries are kept forever")).select(&self.setting("clipboardRetentionDays").to_string(), RETENTION.iter().map(|(value, title)| (*value, translate(title)))));
        rows.push(Line::row(Id::Act(Act::ClearClipboard), t("Clear History")).says(t("Pinned entries are not deleted")).confirm(t("Clear…"), t("Click Again to Clear")));
        if recording {
            // Finding entries by what they mean: the switch, the download
            // it needs, and how far reading the history has got.
            let (installed, on) = (flag(&self.meaning, "installed"), self.set("clipboardMeaning"));
            let (waiting, indexed) = (self.meaning["waiting"].as_u64().unwrap_or(0), self.meaning["indexed"].as_u64().unwrap_or(0));
            let state = if !installed {
                t("A one-time download of about 135 MB, or 250 MB if text recognition is not installed. It takes about 350 MB of memory while you search, and none a few minutes after.").to_string()
            } else if !on {
                String::new()
            } else if let Some(problem) = self.meaning["problem"].as_str() {
                tf("It is not working at present: {0}", &[&problem])
            } else if waiting > 0 {
                tn("Reading the history: {0} entry to go; until then some are found by their words only.", "Reading the history: {0} entries to go; until then some are found by their words only.", waiting, &[&i18n::number(waiting)])
            } else {
                tn("{0} entry can be found this way.", "{0} entries can be found this way.", indexed, &[&i18n::number(indexed)])
            };
            let row = Line::row(Id::Set("clipboardMeaning".into()), t("Find by Meaning")).says(format!("{} {state}", t("“服务器登录命令” or “server login command” finds an ssh line: entries are found by what they are about as well as by their words, in Chinese and English alike. A language model on this computer reads what you copy; nothing is sent anywhere.")));
            rows.push(if self.meaning.is_null() || installed {
                row.switch(on)
            } else if self.meaning_installing || flag(&self.meaning, "installing") {
                row.text("Downloading…")
            } else {
                Line { id: Id::Act(Act::InstallMeaning).into(), ..row }.button(t("Download"))
            });
        }
        page.card(t("Clipboard History"), rows);
        if recording {
            let ignored = self.names("clipboardIgnoredApps");
            let mut rows: Vec<Line> = ignored.iter().map(|id| self.left_out(id, Id::IgnoreRemove, t("Not installed"))).collect();
            // Those copied from first, then the rest of the installed.
            let offered = |seen: bool| -> Vec<(&str, &str)> { self.sources.iter().filter(|source| flag(source, "seen") == seen && !ignored.contains(&source["id"].as_str().unwrap_or_default())).filter_map(|source| Some((source["id"].as_str()?, source["name"].as_str()?))).collect() };
            rows.push(Line::row(Id::IgnoreAdd, t("Never Record Copies From")).says(t("What you copy while a window of one of these applications is in front stays out of the history. Password managers are always left out.")).select("", std::iter::once(("", t("Add Application…"))).chain(offered(true)).chain(offered(false))));
            page.card(t("Ignored Applications"), rows);
        }
    }

    /// Snippets that expand as their keyword is typed: the switch, and the
    /// applications it leaves alone.
    fn snippet_expansion(&self, page: &mut Page) {
        if self.capabilities.is_null() {
            return;
        }
        let available = self.capabilities["snippetExpansion"] != "none";
        let on = self.set("snippetExpansion");
        let row = Line::row(Id::Set("snippetExpansion".into()), t("Expand Snippets as You Type"));
        let mut rows = vec![if available {
            row.says(t("Typing a snippet's keyword in any application replaces it with the snippet's text. While this is on, Heyo watches what you type to find the keywords; none of it is stored. “Pause or Resume Snippet Expansion” in the root search switches it too.")).switch(on)
        } else if self.capabilities["session"] == "wayland" {
            row.says(t("Not available on Wayland, where an application cannot see what is typed in another. Snippets can still be pasted from the root search."))
        } else {
            row.says(t("Not available in this desktop session. Snippets can still be pasted from the root search."))
        }];
        if available && on {
            let excluded = self.names("snippetExpansionIgnoredApps");
            rows.extend(excluded.iter().map(|id| self.left_out(id, Id::ExpandRemove, "")));
            rows.push(Line::row(Id::ExpandAdd, t("Never Expand In")).says(t("Password managers are always left out. A password field in any other application cannot be told from an ordinary one. Nothing expands while an input method (Chinese, Japanese, …) is in use.")).select("", std::iter::once(("", t("Add Application…"))).chain(self.sources.iter().filter(|source| !excluded.contains(&source["id"].as_str().unwrap_or_default())).filter_map(|source| Some((source["id"].as_str()?, source["name"].as_str()?))))));
        }
        page.card(t("Snippet Expansion"), rows);
    }

    fn extension_page(&self, extension: &Value) -> Vec<Line> {
        let id = text(extension, "id");
        let (applications, builtin) = (id == APPLICATIONS, flag(extension, "builtin"));
        let disabled = self.disabled();
        let mut page = Page::default();
        let about = match (extension["development"].as_str(), extension["author"].as_str()) {
            (Some(directory), _) => tf("In development from {0}", &[&directory]),
            (None, Some(author)) => tf("by {0}", &[&author]),
            (None, None) if builtin && !applications => t("Built into Heyo").into(),
            _ => String::new(),
        };
        page.0.push(Line { kind: "head", label: Self::extension_title(extension), description: about, icon: Self::icon(extension, &extension["icon"]), ..Line::default() });
        if let Some(description) = extension["description"].as_str().filter(|description| !description.is_empty()) {
            page.note(description);
        }
        page.card(t("Preferences"), list(extension, "preferences").iter().map(|preference| Self::preference(extension, "", preference, &extension["values"]["extension"][text(preference, "name")])).collect());

        let query = if applications { self.find.to_lowercase() } else { String::new() };
        let mut commands: Vec<Line> = list(extension, "commands")
            .iter()
            .filter(|command| query.is_empty() || text(command, "title").to_lowercase().contains(&query))
            .map(|command| {
                let command_id = text(command, "id");
                let row = Line::row(Id::Enable(command_id.to_string()), text(command, "title")).says(text(command, "description")).pictured(Self::icon(extension, &command["icon"]));
                let row = if command_id == CLIPBOARD_COMMAND { row } else { row.hotkey(&command_id, self.item_hotkeys.get(&command_id)) };
                if applications { row } else { row.switch(!disabled.contains(command_id.as_str())) }
            })
            .collect();
        if commands.is_empty() {
            commands.push(Line::row("", t("No matches")));
        }
        page.card(if applications { "" } else { t("Commands") }, commands);

        if id == BUILTIN {
            self.clipboard(&mut page);
            self.snippet_expansion(&mut page);
        }
        for command in list(extension, "commands") {
            let name = text(command, "name");
            page.card(&tf("{} Preferences", &[&text(command, "title")]), list(command, "preferences").iter().map(|preference| Self::preference(extension, &name, preference, &extension["values"]["commands"][&name][text(preference, "name")])).collect());
        }
        if !builtin && self.permissions.0 == id && !self.permissions.1.is_empty() {
            // What it may ask Heyo for: what its manifest lists and what
            // the user has allowed, which can be taken back here. Live
            // activities are not asked about; they have a switch instead.
            let name = text(extension, "name");
            let activities = self.set("liveActivities");
            let rows = self.permissions.1.iter().map(|row| {
                let (permission, allows) = (text(row, "permission"), text(row, "allows"));
                let mut letters = allows.chars();
                let label = letters.next().map(|first| first.to_uppercase().chain(letters).collect::<String>()).unwrap_or_default();
                if permission == "activity" {
                    return Line::row(Id::Activities(name.to_string()), label).says(if activities { t("Declared; turn it off here") } else { t("Live activities are off in General") }).switch(activities && !self.names("activitiesOff").contains(&name.as_str()));
                }
                let line = Line::row(Id::Revoke { extension: name.to_string(), permission: permission.to_string() }, label).says(match row["state"].as_str() {
                    Some("allowed") => t("Allowed by you"),
                    Some("asks") => t("Asks you the first time it is used"),
                    _ => t("Needs no asking"),
                });
                if row["state"] == "allowed" { line.button(t("Revoke")) } else { line }
            });
            page.card(t("Permissions"), rows.collect());
            page.note(t("Permissions are about what Heyo hands over or does for an extension. They do not confine it: an extension is a program on this computer, and can read your files and start other programs by itself."));
        }
        if !builtin {
            let developed = extension["development"].is_string();
            page.card("", vec![Line::row(Id::Uninstall(id.to_string()), if developed { t("Stop Developing") } else { t("Uninstall Extension") }).says(if developed { t("Takes it out of Heyo; its sources are not touched") } else { t("Its preferences are kept") }).confirm(if developed { t("Remove") } else { t("Uninstall") }, if developed { t("Click Again to Remove") } else { t("Click Again to Uninstall") })]);
        }
        page.done()
    }

    // --- What a control does -------------------------------------------------

    fn change(&mut self, name: &str, value: Value) -> Vec<Ask> {
        let changes = Map::from_iter([(name.to_string(), value)]);
        // Shown at once, before the host has answered.
        self.settings.extend(changes.clone());
        vec![ask(Command::ChangeSettings(changes), Then::Settings)]
    }

    /// A list of the settings with `item` added to it or taken out.
    fn listed(&self, name: &str, item: &str, there: bool) -> Value {
        let mut all: Vec<&str> = self.names(name).into_iter().filter(|other| *other != item).collect();
        if there {
            all.push(item);
        }
        json!(all)
    }

    /// The title of the item `id`, which binding a hotkey to it names.
    fn item_title(&self, id: &str) -> Option<String> {
        match id {
            "heyo:screenshot" => return Some(t("Capture Screenshot").into()),
            "heyo:screenshot-text" => return Some(t("Capture Text from Screen").into()),
            _ => {}
        }
        self.applications.iter().chain(self.extensions.iter().flat_map(|extension| list(extension, "commands"))).find(|item| item["id"] == id).map(|item| text(item, "title"))
    }

    /// Binds `accelerator` (none, when it is empty) to one of Heyo's own
    /// hotkeys or to an item.
    pub(crate) fn bind(&mut self, id: &str, accelerator: &str) -> Vec<Ask> {
        self.error.clear();
        if let Bound::Own(name) = Bound::parse(id) {
            // One of the host's own, by the name the host has for it.
            let Ok(name) = serde_json::from_value(json!(name)) else { return Vec::new() };
            return vec![ask(Command::SetHotkey { name, accelerator: accelerator.to_string() }, Then::Hotkeys)];
        }
        let Some(title) = self.item_title(id) else { return Vec::new() };
        vec![ask(Command::SetItemHotkey { id: id.to_string(), title, accelerator: accelerator.to_string() }, Then::Bound(id.to_string(), accelerator.to_string()))]
    }

    /// The control `id` was used and now says `value`: what to ask of the
    /// host. What only changes what is shown is changed here.
    pub(crate) fn acted(&mut self, id: &str, value: &str) -> Vec<Ask> {
        // What went wrong before stays said until something else is done:
        // not until the host next answers, which it also does unasked (a
        // download that failed is followed by a look at what is installed).
        self.error.clear();
        let on = value == "true";
        match Id::parse(id) {
            Id::Set(name) => {
                // A switch says true or false; everything else its words.
                let value = if value == "true" || value == "false" { json!(on) } else { json!(value) };
                self.change(&name, value)
            }
            Id::Num(name) => value.parse::<u64>().map(|number| self.change(&name, json!(number))).unwrap_or_default(),
            Id::Alias(item) => {
                let mut aliases = self.setting("aliases").as_object().cloned().unwrap_or_default();
                let alias = value.trim();
                if aliases.get(&item).and_then(Value::as_str).unwrap_or_default() == alias {
                    return Vec::new();
                }
                if alias.is_empty() {
                    aliases.remove(&item);
                } else {
                    aliases.insert(item, json!(alias));
                }
                self.change("aliases", Value::Object(aliases))
            }
            Id::Enable(command) => {
                let disabled = self.listed("disabledCommands", &command, !on);
                self.change("disabledCommands", disabled)
            }
            Id::EnableAll(extension) => {
                let Some(extension) = self.extensions.iter().find(|other| other["id"] == extension.as_str()) else { return Vec::new() };
                let ids: Vec<String> = list(extension, "commands").iter().map(|command| text(command, "id")).collect();
                let mut disabled: Vec<String> = self.names("disabledCommands").into_iter().filter(|other| !ids.iter().any(|id| id == other)).map(str::to_string).collect();
                if !on {
                    disabled.extend(ids);
                }
                self.change("disabledCommands", json!(disabled))
            }
            Id::Open(group) => {
                if !self.expanded.remove(&group) {
                    self.expanded.insert(group);
                }
                Vec::new()
            }
            Id::Preference { extension, command, name, kind } => {
                let Some(found) = self.extensions.iter().find(|other| other["name"] == extension.as_str()) else { return Vec::new() };
                let mut values = found["values"].clone();
                let value = if kind == "checkbox" { json!(on) } else { json!(value) };
                if command.is_empty() {
                    values["extension"][name.as_str()] = value;
                } else {
                    values["commands"][command.as_str()][name.as_str()] = value;
                }
                let Ok(values) = serde_json::from_value(values) else { return Vec::new() };
                vec![ask(Command::SetExtensionPreferences { extension, values }, Then::Reload)]
            }
            Id::Revoke { extension, permission } => {
                // By the name the host has for it, as the host said it.
                let Ok(permission) = serde_json::from_value(json!(permission)) else { return Vec::new() };
                vec![ask(Command::RevokeExtensionPermission { extension, permission }, Then::Permissions(self.page.clone())), ask(Command::ExtensionPermissions(self.page.clone()), Then::Permissions(self.page.clone()))]
            }
            Id::Activities(extension) => {
                let off = self.listed("activitiesOff", &extension, !on);
                self.change("activitiesOff", off)
            }
            Id::Uninstall(extension) => vec![ask(Command::UninstallExtension(extension), Then::Uninstalled)],
            Id::Words => {
                let lines: Vec<&str> = value.lines().map(str::trim).filter(|line| !line.is_empty()).collect();
                if lines == self.names("dictationVocabulary") { Vec::new() } else { self.change("dictationVocabulary", json!(lines)) }
            }
            Id::IgnoreAdd | Id::ExpandAdd if value.is_empty() => Vec::new(),
            Id::IgnoreAdd => self.change("clipboardIgnoredApps", self.listed("clipboardIgnoredApps", value, true)),
            Id::ExpandAdd => self.change("snippetExpansionIgnoredApps", self.listed("snippetExpansionIgnoredApps", value, true)),
            Id::IgnoreRemove(application) => self.change("clipboardIgnoredApps", self.listed("clipboardIgnoredApps", &application, false)),
            Id::ExpandRemove(application) => self.change("snippetExpansionIgnoredApps", self.listed("snippetExpansionIgnoredApps", &application, false)),
            Id::Pick(setting) => {
                let (provider, model) = ids::chosen(value).unwrap_or_default();
                self.change(&setting, json!(ModelRef::to(provider, model)))
            }
            // A provider has commands of its own: the window never has its
            // secrets to send back with the rest of the settings.
            Id::ProviderField { provider, field } => {
                let Some(known) = self.provider(&provider) else { return Vec::new() };
                let secret = known.kind().fields().iter().any(|known| known.name == field && known.entry == Field::Secret);
                let was = if field == "name" { known.name.as_str() } else { known.get(&field) };
                if (secret && value.trim().is_empty()) || (!secret && was == value.trim()) {
                    return Vec::new();
                }
                // What a check said may no longer be so.
                self.checks.remove(&provider);
                let mut asks = vec![ask(Command::SetProviderField { provider: provider.clone(), field: field.clone(), value: value.to_string() }, Then::Settings)];
                // Where the service is, or what it asks for, changed: asked again.
                if known.kind().source() == Source::Listed && known.kind().fields().iter().any(|known| known.name == field && matches!(known.entry, Field::Url | Field::Secret)) {
                    self.listed.remove(&provider);
                    asks.push(ask(Command::ProviderModels(provider.clone()), Then::Models(provider)));
                }
                asks
            }
            Id::ForgetSecret { provider, field } => {
                self.checks.remove(&provider);
                vec![ask(Command::SetProviderField { provider, field, value: String::new() }, Then::Settings)]
            }
            Id::RemoveProvider(provider) => {
                self.listed.remove(&provider);
                self.checks.remove(&provider);
                self.page = "models".into();
                vec![ask(Command::RemoveProvider(provider), Then::Settings)]
            }
            Id::Go(page) => self.show(&page),
            Id::NewProvider(kind) => {
                self.draft = Some(Provider::new("", &kind));
                self.show(&format!("{PROVIDER}{NEW}"))
            }
            // A new one's fields are kept here until it is added.
            Id::DraftField(field) => {
                if let Some(draft) = &mut self.draft {
                    let value = value.trim();
                    match field.as_str() {
                        "name" if !value.is_empty() => draft.name = value.into(),
                        "name" => {}
                        _ if value.is_empty() => {
                            draft.config.remove(&field);
                        }
                        _ => {
                            draft.config.insert(field, value.into());
                        }
                    }
                }
                Vec::new()
            }
            Id::Act(Act::SaveProvider) => {
                let Some(draft) = self.draft.clone() else { return Vec::new() };
                let before = self.providers().into_iter().map(|provider| provider.id).collect();
                vec![ask(Command::AddProvider { kind: draft.kind, name: draft.name, config: draft.config }, Then::Added(before))]
            }
            Id::CheckProvider(provider) => {
                if self.checks.get(&provider) == Some(&Check::Checking) {
                    return Vec::new();
                }
                self.checks.insert(provider.clone(), Check::Checking);
                vec![ask(Command::CheckProvider(provider.clone()), Then::ProviderChecked(provider))]
            }
            Id::Act(Act::OpenModels) => self.show("models"),
            Id::Act(Act::InstallShellExtension) => vec![ask(Command::InstallShellExtension, Then::Capabilities)],
            // The speech models of this computer: each a command of the host's,
            // and then what they are is asked again.
            Id::SpeechDownload(id) => vec![ask(Command::DownloadSpeechModel(id), Then::Nothing), ask(Command::SpeechModels, Then::Speech)],
            Id::SpeechCancel(id) => vec![ask(Command::CancelSpeechDownload(id), Then::Nothing), ask(Command::SpeechModels, Then::Speech)],
            Id::SpeechDelete(id) => vec![ask(Command::DeleteSpeechModel(id), Then::Nothing), ask(Command::SpeechModels, Then::Speech)],
            Id::SpeechUse(id) => {
                let mut asks = self.change(Use::DictationRecognition.setting(), json!(ModelRef::to(registry::LOCAL_SPEECH, &id)));
                asks.push(ask(Command::SpeechModels, Then::Speech));
                asks
            }
            Id::Act(Act::InstallOcr) => {
                self.ocr_installing = true;
                vec![ask(Command::OcrInstall, Then::Started("ocr"))]
            }
            Id::Act(Act::InstallMeaning) => {
                self.meaning_installing = true;
                vec![ask(Command::ClipboardMeaningInstall, Then::Started("meaning"))]
            }
            Id::Act(Act::OpenExtensions) => vec![ask(Command::OpenExtensionsFolder, Then::Nothing)],
            Id::Act(Act::ClearClipboard) => vec![ask(Command::ClearClipboardHistory, Then::Nothing)],
            Id::Act(Act::CopyDiagnostics) => vec![ask(Command::CopyDiagnostics, Then::Copied)],
            Id::Act(Act::Update) => match self.update.clone() {
                Update::Newer(version) => {
                    self.update = Update::Installing(version);
                    vec![ask(Command::InstallUpdate, Then::Installed)]
                }
                Update::Unknown | Update::Newest => {
                    self.update = Update::Checking;
                    vec![ask(Command::CheckForUpdate, Then::Checked)]
                }
                Update::Checking | Update::Installing(_) => Vec::new(),
            },
            // The menu's entries are `pick`'s; a line that only has a name does nothing.
            Id::Modification(_) | Id::Category(_) | Id::Named(_) => Vec::new(),
        }
    }

    // --- What is asked without a control -------------------------------------

    /// Everything about the extensions and the applications, read again.
    fn reload() -> Vec<Ask> {
        vec![ask(Command::ListExtensions, Then::Extensions), ask(Command::ListApplications, Then::Applications), ask(Command::ItemHotkeys, Then::ItemHotkeys)]
    }

    /// What is read when the window is shown: it may have changed while
    /// the window was away.
    pub(crate) fn opened(&self) -> Vec<Ask> {
        let mut asks = vec![ask(Command::GetSettings, Then::Settings), ask(Command::GetHotkeys, Then::Hotkeys), ask(Command::GetCapabilities, Then::Capabilities)];
        asks.extend(Self::reload());
        asks.extend(self.entered());
        asks
    }

    /// What the page that is shown needs to know besides.
    pub(crate) fn entered(&self) -> Vec<Ask> {
        match self.page.as_str() {
            "dictation" | "models" => [vec![ask(Command::DictationStatus, Then::Dictation), ask(Command::SpeechModels, Then::Speech)], self.listings()].concat(),
            page if page.starts_with(PROVIDER) => [vec![ask(Command::SpeechModels, Then::Speech)], self.listings()].concat(),
            "screenshots" => vec![ask(Command::OcrStatus, Then::Ocr)],
            "advanced" => self.listings(),
            BUILTIN => vec![ask(Command::ClipboardSources, Then::Sources), ask(Command::ClipboardMeaningStatus, Then::Meaning)],
            page if self.extensions.iter().any(|extension| extension["id"] == page && !flag(extension, "builtin")) => vec![ask(Command::ExtensionPermissions(page.to_string()), Then::Permissions(page.to_string()))],
            _ => Vec::new(),
        }
    }

    /// Asked again every two seconds while the history is being read for
    /// finding by meaning, so that the page says how much is left.
    pub(crate) fn poll(&self) -> Vec<Ask> {
        let reading = self.page == BUILTIN && self.set("clipboardMeaning") && flag(&self.meaning, "installed") && self.meaning["waiting"].as_u64().unwrap_or(0) > 0;
        if reading { vec![ask(Command::ClipboardMeaningStatus, Then::Meaning)] } else { Vec::new() }
    }

    /// Another page is shown.
    pub(crate) fn show(&mut self, page: &str) -> Vec<Ask> {
        if self.page == page {
            return Vec::new();
        }
        self.page = page.to_string();
        self.find.clear();
        self.error.clear();
        self.entered()
    }

    /// The host answered: what is known changes, and more may be asked.
    pub(crate) fn answered(&mut self, then: Then, answer: Result<Answer, String>) -> Vec<Ask> {
        // Read as it is written for now.
        let answer = answer.and_then(|answer| Ok(serde_json::to_value(answer).map_err(|e| e.to_string())?["data"].take()));
        self.answered_value(then, answer)
    }

    fn answered_value(&mut self, then: Then, answer: Result<Value, String>) -> Vec<Ask> {
        let answer = match answer {
            Ok(answer) => answer,
            Err(message) => {
                match then {
                    Then::Models(provider) => {
                        self.listed.insert(provider, Err(message));
                        return Vec::new();
                    }
                    // Said on the provider's page.
                    Then::ProviderChecked(provider) => {
                        self.checks.insert(provider, Check::Done(Err(message)));
                        return Vec::new();
                    }
                    Then::Started("ocr") => self.ocr_installing = false,
                    Then::Started("meaning") => self.meaning_installing = false,
                    Then::Checked => self.update = Update::Unknown,
                    Then::Installed => {
                        if let Update::Installing(version) = &self.update {
                            self.update = Update::Newer(version.clone());
                        }
                    }
                    _ => {}
                }
                self.error = message;
                // What was shown before the host answered may not be so.
                return if then == Then::Settings { vec![ask(Command::GetSettings, Then::Reread)] } else { Vec::new() };
            }
        };
        match then {
            Then::Settings | Then::Reread => {
                if let Value::Object(settings) = answer {
                    self.settings = settings;
                }
            }
            Then::Hotkeys => self.hotkeys = answer,
            Then::Capabilities => self.capabilities = answer,
            Then::Extensions => {
                if let Value::Array(extensions) = answer {
                    self.extensions = extensions;
                }
            }
            Then::Applications => {
                if let Value::Array(applications) = answer {
                    self.applications = applications;
                }
            }
            Then::ItemHotkeys => {
                if let Value::Object(bound) = answer {
                    self.item_hotkeys = bound;
                }
            }
            Then::Dictation => self.dictation = answer,
            Then::Speech => {
                if let Value::Array(speech) = answer {
                    self.speech = speech;
                }
            }
            Then::Ocr => self.ocr = answer,
            Then::Meaning => self.meaning = answer,
            Then::Sources => {
                if let Value::Array(sources) = answer {
                    self.sources = sources;
                }
            }
            Then::Permissions(extension) => {
                if let Value::Array(rows) = answer {
                    self.permissions = (extension, rows);
                }
            }
            Then::Models(provider) => {
                self.listed.insert(provider, Ok(answer.as_array().map(|models| models.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default()));
            }
            Then::ProviderChecked(provider) => {
                let checked: kinds::Checked = serde_json::from_value(answer).unwrap_or_default();
                if let Some(models) = checked.models {
                    self.listed.insert(provider.clone(), Ok(models));
                }
                self.checks.insert(provider, Check::Done(Ok(checked.said)));
            }
            Then::Added(before) => {
                if let Value::Object(settings) = answer {
                    self.settings = settings;
                }
                self.draft = None;
                // Its own page, where it can be checked.
                let added = self.providers().into_iter().find(|provider| !before.contains(&provider.id));
                return self.show(&added.map_or("models".into(), |provider| format!("{PROVIDER}{}", provider.id)));
            }
            Then::Copied => self.copied = true,
            Then::Checked => self.update = answer.as_str().map_or(Update::Newest, |version| Update::Newer(version.to_string())),
            Then::Bound(id, accelerator) => {
                if accelerator.is_empty() {
                    self.item_hotkeys.remove(&id);
                } else {
                    self.item_hotkeys.insert(id, json!(accelerator));
                }
            }
            Then::Reload => return Self::reload(),
            Then::Uninstalled => {
                self.page = "general".into();
                return Self::reload();
            }
            Then::Installed | Then::Started(_) | Then::Nothing => {}
        }
        Vec::new()
    }

    /// The host emitted something to its windows.
    pub(crate) fn heard(&mut self, event: &str, told: &Value) -> Vec<Ask> {
        use crate::wire::event;
        // A download's progress, or its end: true when it is over.
        let over = told["type"] != "progress";
        if over && told["type"] == "error" && event != event::SETTINGS_CHANGED && event != event::INDEX_READY {
            self.error = text(told, "message");
        }
        match event {
            // Favorites and aliases also change from the palette. The event
            // leaves out what only this window may know: ask for the whole.
            event::SETTINGS_CHANGED => vec![ask(Command::GetSettings, Then::Settings)],
            event::INDEX_READY => Self::reload(),
            // How far each download is, the host keeps: asked for again.
            event::DICTATION_INSTALL if over => vec![ask(Command::DictationStatus, Then::Dictation), ask(Command::SpeechModels, Then::Speech)],
            event::DICTATION_INSTALL => vec![ask(Command::SpeechModels, Then::Speech)],
            event::OCR_INSTALL => {
                self.ocr_installing = !over;
                if over { vec![ask(Command::OcrStatus, Then::Ocr)] } else { Vec::new() }
            }
            event::CLIPBOARD_MEANING_INSTALL => {
                self.meaning_installing = !over;
                if over { vec![ask(Command::ClipboardMeaningStatus, Then::Meaning)] } else { Vec::new() }
            }
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn told(settings: Value, capabilities: Value) -> Model {
        let mut model = Model::new();
        model.settings = settings.as_object().unwrap().clone();
        model.capabilities = capabilities;
        model
    }

    fn row<'a>(lines: &'a [Line], label: &str) -> &'a Line {
        lines.iter().find(|line| line.label == label).unwrap_or_else(|| panic!("no line {label:?} among {:?}", lines.iter().map(|line| &line.label).collect::<Vec<_>>()))
    }

    fn has(lines: &[Line], label: &str) -> bool {
        lines.iter().any(|line| line.label == label)
    }

    fn titles(line: &Line) -> Vec<&str> {
        line.choices.iter().map(|(_, title)| title.as_str()).collect()
    }

    /// The one thing asked of the host.
    fn asked(asks: &[Ask]) -> Command {
        assert_eq!(asks.len(), 1, "{asks:?}");
        asks[0].command.clone()
    }

    /// The settings the one thing asked of the host changes, and what to.
    fn patch(asks: &[Ask]) -> Value {
        match asked(asks) {
            Command::ChangeSettings(changes) => Value::Object(changes),
            other => panic!("{other:?} changes no settings"),
        }
    }

    /// The preferences the one thing asked of the host saves, and whose.
    fn saved(asks: &[Ask]) -> (String, Value) {
        match asked(asks) {
            Command::SetExtensionPreferences { extension, values } => (extension, serde_json::to_value(values).unwrap()),
            other => panic!("{other:?} saves no preferences"),
        }
    }

    /// A page of rows, by their ids.
    fn page(ids: &[&str]) -> Vec<Line> {
        ids.iter().map(|id| Line::row(*id, *id)).collect()
    }

    #[test]
    fn a_control_that_was_used_stays_the_line_it_was() {
        // A switch that was flipped: the line is told to be drawn again,
        // not taken out and put back, which would make it another switch.
        let was = page(&["a", "b", "c"]);
        let mut is = was.clone();
        is[1].on = true;
        assert_eq!(changes(&was, &is), Changes { head: 3, out: 0, come: 0, changed: vec![1] });
        // And nothing at all is said when nothing is different.
        assert_eq!(changes(&was, &was), Changes { head: 3, ..Changes::default() });
    }

    #[test]
    fn a_group_opened_further_down_leaves_the_lines_round_it() {
        // The first line stays, so the list stays where it was scrolled to.
        let (was, is) = (page(&["a", "group", "z"]), page(&["a", "group", "one", "two", "z"]));
        assert_eq!(changes(&was, &is), Changes { head: 2, out: 0, come: 2, changed: vec![] });
        // Closed again; and the group's own line says it is closed.
        let mut closed = was.clone();
        closed[1].open = true;
        assert_eq!(changes(&is, &closed), Changes { head: 2, out: 2, come: 0, changed: vec![1] });
        // What changed after what was put in is told by its place among the new lines.
        let mut last = is.clone();
        last[4].value = "now".into();
        assert_eq!(changes(&was, &last), Changes { head: 2, out: 0, come: 2, changed: vec![4] });
    }

    #[test]
    fn another_page_takes_the_first_line_out() {
        // So that it is not left scrolled to where the last one was.
        let (was, is) = (page(&["a", "b", "c"]), page(&["x", "y"]));
        assert_eq!(changes(&was, &is), Changes { head: 0, out: 3, come: 2, changed: vec![] });
        // Even where the two end alike: the end stays, the beginning goes.
        let (was, is) = (page(&["a", "b", "end"]), page(&["x", "end"]));
        assert_eq!(changes(&was, &is), Changes { head: 0, out: 2, come: 1, changed: vec![] });
        // From nothing, and to nothing.
        assert_eq!(changes(&[], &is), Changes { head: 0, out: 0, come: 2, changed: vec![] });
        assert_eq!(changes(&was, &[]), Changes { head: 0, out: 3, come: 0, changed: vec![] });
    }

    #[test]
    fn a_line_is_counted_once_where_the_pages_begin_and_end_alike() {
        // One line taken out of a run of lines that are alike: no line is
        // both of the beginning that stays and of the end that stays.
        let same = |n: usize| vec![Line::row("x", "x"); n];
        assert_eq!(changes(&same(3), &same(2)), Changes { head: 2, out: 1, come: 0, changed: vec![] });
        assert_eq!(changes(&same(2), &same(3)), Changes { head: 2, out: 0, come: 1, changed: vec![] });
    }

    #[test]
    fn general_says_what_the_settings_are() {
        let mut model = told(json!({ "launchAtLogin": true, "appearance": "dark", "language": "system", "compactMode": false, "hideOnBlur": true, "keepCommand": false, "liveActivities": true }), json!({ "extension": "none" }));
        model.hotkeys = json!({ "supported": true, "toggle": "<Super>space", "clipboard": null, "dictation": null });
        let lines = model.lines();
        assert!(row(&lines, "Open at Login").on && row(&lines, "Hide When Focus Is Lost").on && row(&lines, "Show What Is in Progress").on);
        assert!(!row(&lines, "Compact Mode").on && !row(&lines, "Keep the Running Command").on);
        assert_eq!((row(&lines, "Theme").value.as_str(), titles(row(&lines, "Theme"))), ("dark", vec!["System", "Light", "Dark"]));
        assert_eq!((row(&lines, "Heyo Hotkey").hotkey.as_str(), row(&lines, "Heyo Hotkey").keys.clone()), ("@toggle", vec!["Super".to_string(), "Space".to_string()]));
        assert!(!has(&lines, "GNOME Shell Extension"));
        // The cards: the first row of each is its first, the last its last.
        assert!(row(&lines, "Open at Login").first && row(&lines, "Heyo Hotkey").last);
        assert_eq!(lines.iter().filter(|line| line.kind == "heading").map(|line| line.label.as_str()).collect::<Vec<_>>(), ["Appearance", "Window", "Live Activities"]);
        // The language: the system's, the sources', and every translation
        // by its own name for itself.
        assert_eq!((row(&lines, "Language").value.as_str(), titles(row(&lines, "Language"))), ("system", vec!["System", "English", "简体中文"]));
        assert!(row(&lines, "Theme").first && !row(&lines, "Theme").last && row(&lines, "Language").last);
    }

    #[test]
    fn a_switch_is_shown_at_once_and_asked_of_the_host() {
        let mut model = told(json!({ "compactMode": false }), json!({}));
        assert_eq!(patch(&model.acted("set:compactMode", "true")), json!({ "compactMode": true }));
        assert!(row(&model.lines(), "Compact Mode").on);
        assert_eq!(patch(&model.acted("set:appearance", "light")), json!({ "appearance": "light" }));
        // The host's answer is what is then so; its refusal is said, and
        // the settings are read again.
        assert!(model.answered_value(Then::Settings, Ok(json!({ "compactMode": false, "appearance": "dark" }))).is_empty());
        assert!(!row(&model.lines(), "Compact Mode").on);
        assert_eq!(asked(&model.answered_value(Then::Settings, Err("no".into()))).name(), "get_settings");
        assert_eq!(model.error, "no");
    }

    #[test]
    fn general_offers_the_shell_extension_only_where_it_is_wanted() {
        let of = |state: &str| {
            let lines = told(json!({}), json!({ "extension": state })).lines();
            lines.iter().find(|line| line.label == "GNOME Shell Extension").map(|line| (line.control, line.value.clone()))
        };
        assert_eq!(of("none"), None);
        assert_eq!(of("active"), None);
        assert_eq!(of("missing"), Some(("button", "Install".to_string())));
        assert_eq!(of("outdated"), Some(("button", "Update".to_string())));
        assert_eq!(of("pending"), Some(("", String::new())));
    }

    #[test]
    fn a_session_without_hotkeys_has_no_recorder() {
        let mut model = told(json!({}), json!({}));
        model.hotkeys = json!({ "supported": false, "toggle": null });
        let lines = model.lines();
        assert_eq!((row(&lines, "Heyo Hotkey").hotkey.as_str(), row(&lines, "Heyo Hotkey").description.as_str()), ("", "Bind a desktop keyboard shortcut to heyo."));
    }

    #[test]
    fn a_hotkey_is_bound_as_one_of_heyos_own_or_as_an_items() {
        let mut model = told(json!({}), json!({}));
        model.applications = vec![json!({ "id": "app:files.desktop", "title": "Files" })];
        assert_eq!(asked(&model.bind("@toggle", "<Super>space")), Command::SetHotkey { name: crate::commands::settings::HotkeyName::Toggle, accelerator: "<Super>space".into() });
        assert_eq!(asked(&model.bind("app:files.desktop", "<Super>e")), Command::SetItemHotkey { id: "app:files.desktop".into(), title: "Files".into(), accelerator: "<Super>e".into() });
        assert_eq!(asked(&model.bind("heyo:screenshot", "")), Command::SetItemHotkey { id: "heyo:screenshot".into(), title: "Capture Screenshot".into(), accelerator: String::new() });
        assert!(model.bind("app:gone.desktop", "<Super>g").is_empty());
        model.answered_value(Then::Bound("app:files.desktop".into(), "<Super>e".into()), Ok(Value::Null));
        assert_eq!(model.item_hotkeys["app:files.desktop"], "<Super>e");
        model.answered_value(Then::Bound("app:files.desktop".into(), String::new()), Ok(Value::Null));
        assert!(model.item_hotkeys.is_empty());
    }

    /// The providers as the host tells them: Claude with its key taken out,
    /// a server on this computer and one elsewhere, Claude Code found and
    /// Codex not, and the speech models of this computer.
    fn providers() -> Value {
        json!({
            "providers": [
                { "id": "local-speech", "kind": "local-speech", "name": "Speech Models on This Computer" },
                { "id": "claude", "kind": "anthropic", "name": "Claude", "config": { "key": "" } },
                { "id": "ollama", "kind": "openai-compatible", "name": "Ollama", "config": { "url": "http://127.0.0.1:11434/v1" } },
                { "id": "lan", "kind": "openai-compatible", "name": "Workstation", "config": { "url": "http://10.0.0.5:8000/v1" } },
                { "id": "claude-code", "kind": "claude-code", "name": "Claude Code" },
                { "id": "codex", "kind": "codex", "name": "Codex", "config": { "path": "/opt/codex" } },
            ],
            "providerStates": {
                "claude": { "secretsSet": ["key"], "cliFound": null },
                "claude-code": { "secretsSet": [], "cliFound": { "Ok": { "path": "/home/ada/.local/bin/claude", "version": "2.1.302 (Claude Code)" } } },
                "codex": { "secretsSet": [], "cliFound": { "Err": "There is no executable at /opt/codex." } },
            },
            "aiModel": { "provider": "ollama", "model": "qwen3" },
            "dictationModel": { "provider": "local-speech", "model": "x-asr-2026-06-05" },
            "polishModel": { "provider": "claude", "model": "claude-opus-5-5" },
            "casesModel": { "provider": "", "model": "" },
        })
    }

    /// The speech models of this computer as the host says them: one in
    /// use, one downloading, one not downloaded.
    fn speech() -> Vec<Value> {
        let entry = |id: &str, name: &str, installed: bool, in_use: bool, downloading: Value| json!({ "id": id, "name": name, "languages": "Chinese, English", "note": "Fast.", "megabytes": 240, "installed": installed, "inUse": in_use, "takesLanguage": true, "downloading": downloading });
        vec![
            entry("sense-voice-2024-07-17", "SenseVoice Small", true, false, Value::Null),
            entry("x-asr-2026-06-05", "X-ASR Streaming", false, true, json!({ "step": "speech-model", "received": 67, "total": 134 })),
            entry("whisper-turbo", "Whisper Turbo", false, false, Value::Null),
        ]
    }

    /// `providers()` with these settings besides.
    fn with_providers(more: Value) -> Map<String, Value> {
        let mut settings = providers().as_object().unwrap().clone();
        settings.extend(more.as_object().unwrap().clone());
        settings
    }

    #[test]
    fn dictation_says_what_this_session_can_do() {
        let mut model = told(
            Value::Object(with_providers(json!({ "dictationHoldKey": "", "dictationLive": true, "dictationSource": "", "dictationLanguage": "zh", "dictationVocabulary": ["TypeScript", "杰森 -> JSON"], "dictationPolish": "clean" }))),
            json!({ "session": "x11", "holdKey": "full", "holdKeys": [{ "value": "Control_R", "title": "Right Ctrl" }], "dictationWords": ["composed", "rewritten", "pasted"] }),
        );
        model.page = "dictation".into();
        model.dictation = json!({ "installed": false, "installing": false, "microphones": [{ "name": "alsa_input.usb", "description": "USB Microphone" }] });
        model.speech = speech();
        let lines = model.lines();
        // Recognition: the speech models that can be used, the one chosen not downloaded yet; no download here.
        let recognition = row(&lines, "Speech Recognition");
        assert_eq!(&titles(recognition)[..3], ["This Computer · Default", "This Computer · SenseVoice Small", "This Computer · SenseVoice (Cantonese)"]);
        assert!(recognition.danger && recognition.description.starts_with("X-ASR Streaming is not downloaded yet. Download it in Models."), "{}", recognition.description);
        assert!(lines.iter().filter(|line| line.control == "button").all(|line| line.id == "act:open-models"));
        assert_eq!(titles(row(&lines, "Hold-to-Talk Key")), ["None", "Right Ctrl"]);
        assert!(row(&lines, "Hold-to-Talk Key").description.starts_with("A single key to hold"));
        let live = row(&lines, "Type as You Speak");
        assert!(live.control == "switch" && live.on && live.description.contains("through the input method") && live.description.contains("corrected where they stand"));
        assert_eq!(titles(row(&lines, "Microphone")), ["System Default", "USB Microphone"]);
        assert!(!row(&lines, "Microphone").danger);
        assert_eq!(row(&lines, "Language").value, "zh");
        assert_eq!(lines.iter().find(|line| line.kind == "area").unwrap().value, "TypeScript\n杰森 -> JSON");
        assert!(row(&lines, "Mode").value == "clean" && row(&lines, "Mode").description.contains("filler words"));
        // Polishing: the text models, of every provider that answers in text.
        let polishing = row(&lines, "Model");
        assert_eq!(polishing.value, ids::choice("claude", "claude-opus-5-5"));
        assert!(titles(polishing).contains(&"Claude · claude-opus-5-5") && titles(polishing).contains(&"Codex · Default") && !titles(polishing).iter().any(|title| title.contains("This Computer")));
        assert!(!polishing.danger);
        assert_eq!(model.entered().iter().map(|ask| ask.command.name()).collect::<Vec<_>>(), ["dictation_status", "speech_models", "provider_models", "provider_models"]);
        // One that is downloaded: nothing to say.
        model.settings.insert("dictationModel".into(), json!({ "provider": "local-speech", "model": "sense-voice-2024-07-17" }));
        assert!(!row(&model.lines(), "Speech Recognition").danger);
    }

    #[test]
    fn dictation_explains_what_a_session_cannot_do() {
        let mut model = told(Value::Object(with_providers(json!({ "dictationHoldKey": "Control_R", "dictationPolish": "nonsense" }))), json!({ "session": "wayland", "holdKey": "none", "holdKeys": [], "dictationWords": ["pasted"] }));
        model.page = "dictation".into();
        model.dictation = json!({ "installed": true, "installing": false, "microphones": [] });
        let lines = model.lines();
        assert!(row(&lines, "Microphone").danger);
        assert!(row(&lines, "Hold-to-Talk Key").control.is_empty() && row(&lines, "Hold-to-Talk Key").description.contains("cannot be watched"));
        assert!(row(&lines, "Type as You Speak").control.is_empty() && row(&lines, "Type as You Speak").description.starts_with("Not available on Wayland"));
        // A mode it does not know is shown as none, and then there is no model to choose.
        assert_eq!(row(&lines, "Mode").value, "off");
        assert!(!has(&lines, "Model"));
    }

    #[test]
    fn a_speech_model_is_downloaded_used_and_deleted_on_the_models_page() {
        let mut model = told(providers(), json!({}));
        model.show("provider:local-speech");
        model.answered_value(Then::Speech, Ok(json!(speech())));
        let lines = model.lines();
        // Always there: no name to change, nothing to remove.
        assert!(!has(&lines, "Name") && !has(&lines, "Remove Provider"));
        assert!(row(&lines, "What It Can Do").description.contains("stays here"));
        // One row a model: downloaded but not used, downloading, not downloaded.
        let speech = card(&lines, "Models");
        let downloaded = row(speech, "SenseVoice Small");
        assert_eq!((downloaded.control, downloaded.value.as_str(), downloaded.more.as_str()), ("confirm", "Delete…", "Use"));
        assert_eq!(downloaded.description, "Chinese, English · 240 MB. Fast.");
        let downloading = row(speech, "X-ASR Streaming");
        assert_eq!((downloading.control, downloading.value.as_str(), downloading.more.as_str()), ("text", "Speech model 50%", "Cancel"));
        assert_eq!((row(speech, "Whisper Turbo").control, row(speech, "Whisper Turbo").value.as_str()), ("button", "Download"));
        let names = |asks: Vec<Ask>| asks.iter().map(|ask| ask.command.clone()).collect::<Vec<_>>();
        assert_eq!(names(model.acted(&row(speech, "Whisper Turbo").id, "")), [Command::DownloadSpeechModel("whisper-turbo".into()), Command::SpeechModels]);
        assert_eq!(names(model.acted(&downloading.more_id, "")), [Command::CancelSpeechDownload("x-asr-2026-06-05".into()), Command::SpeechModels]);
        assert_eq!(names(model.acted(&downloaded.id, "")), [Command::DeleteSpeechModel("sense-voice-2024-07-17".into()), Command::SpeechModels]);
        // Used: dictation refers to it.
        let asks = model.acted(&downloaded.more_id, "");
        assert_eq!(asks[0].command, Command::ChangeSettings(json!({ "dictationModel": { "provider": "local-speech", "model": "sense-voice-2024-07-17" } }).as_object().unwrap().clone()));
        // A download's progress, and its end, are read again from the host.
        use crate::wire::event;
        assert_eq!(names(model.heard(event::DICTATION_INSTALL, &json!({ "type": "progress", "step": "speech-model", "received": 1, "total": 2 }))), [Command::SpeechModels]);
        assert_eq!(names(model.heard(event::DICTATION_INSTALL, &json!({ "type": "error", "message": "no space left" }))), [Command::DictationStatus, Command::SpeechModels]);
        assert_eq!(model.error, "no space left");
        // In use: said, and no delete.
        let mut used = self::speech();
        used[0]["inUse"] = json!(true);
        model.answered_value(Then::Speech, Ok(json!(used)));
        let lines = model.lines();
        assert_eq!(row(card(&lines, "Models"), "SenseVoice Small").control, "badge");
    }

    #[test]
    fn the_vocabulary_is_saved_as_its_lines() {
        let mut model = told(json!({ "dictationVocabulary": ["TypeScript", "jason -> JSON"] }), json!({}));
        assert!(model.acted("words", "  TypeScript \n\n jason -> JSON\n").is_empty());
        assert_eq!(patch(&model.acted("words", "TypeScript\n杰森 -> JSON\n\n")), json!({ "dictationVocabulary": ["TypeScript", "杰森 -> JSON"] }));
    }

    #[test]
    fn screenshots_have_their_hotkeys_and_the_engine() {
        let mut model = told(json!({}), json!({}));
        model.page = "screenshots".into();
        model.hotkeys = json!({ "supported": true });
        model.item_hotkeys = json!({ "heyo:screenshot": "<Control><Shift>4" }).as_object().unwrap().clone();
        let lines = model.lines();
        assert_eq!((row(&lines, "Capture Screenshot").hotkey.as_str(), row(&lines, "Capture Screenshot").keys.len()), ("heyo:screenshot", 3));
        assert_eq!(row(&lines, "Recognition Engine").control, "button");
        assert_eq!(asked(&model.acted("act:install-ocr", "")).name(), "ocr_install");
        assert_eq!(row(&model.lines(), "Recognition Engine").value, "Downloading…");
        assert_eq!(asked(&model.heard(crate::wire::event::OCR_INSTALL, &json!({ "type": "installed" }))).name(), "ocr_status");
        model.answered_value(Then::Ocr, Ok(json!({ "installed": true, "installing": false })));
        assert_eq!(row(&model.lines(), "Recognition Engine").control, "badge");
    }


    /// A card of a page: its lines, from its heading to the next.
    fn card<'a>(lines: &'a [Line], title: &str) -> &'a [Line] {
        let at = lines.iter().position(|line| line.kind == "heading" && line.label == title).unwrap_or_else(|| panic!("no card {title}"));
        let end = lines[at + 2..].iter().position(|line| line.kind == "heading" || line.kind == "end" || line.first).map_or(lines.len(), |end| at + 2 + end);
        &lines[at + 1..end]
    }

    fn head(lines: &[Line]) -> &Line {
        lines.iter().find(|line| line.kind == "head").expect("a head")
    }

    #[test]
    fn the_models_page_lists_each_provider_with_how_it_is_and_what_it_can_do() {
        let mut model = told(providers(), json!({}));
        assert_eq!(model.show("models").iter().map(|ask| ask.command.name()).collect::<Vec<_>>(), ["dictation_status", "speech_models", "provider_models", "provider_models"]);
        let lines = model.lines();
        let listed = card(&lines, "Providers");
        assert_eq!(listed.iter().map(|line| line.label.as_str()).collect::<Vec<_>>(), ["Speech Models on This Computer", "Claude", "Ollama", "Workstation", "Claude Code", "Codex"]);
        // Each a row to its page: its kind (where its name does not say it), how it is, what it can do.
        let claude = row(listed, "Claude");
        assert_eq!((claude.control, claude.id.as_str(), claude.description.as_str()), ("link", "go:provider:claude", "Claude API · Set up"));
        assert_eq!(claude.chips, ["Text", "Images", "Streaming"]);
        assert_eq!(row(listed, "Ollama").description, "OpenAI-Compatible Server · Checking…");
        assert_eq!(row(listed, "Ollama").chips, ["Text", "Streaming", "Private"]);
        assert_eq!(row(listed, "Claude Code").description, "Set up");
        assert!(row(listed, "Codex").danger && row(listed, "Codex").description == "Not found");
        model.answered_value(Then::Models("ollama".into()), Ok(json!(["qwen3", "llama3"])));
        model.answered_value(Then::Models("lan".into()), Err("connection refused".into()));
        model.answered_value(Then::Speech, Ok(json!(speech())));
        let lines = model.lines();
        let listed = card(&lines, "Providers");
        assert_eq!(row(listed, "Ollama").description, "OpenAI-Compatible Server · 2 models");
        assert!(row(listed, "Workstation").danger && row(listed, "Workstation").description.ends_with("Not reachable"));
        assert_eq!(row(listed, "Speech Models on This Computer").description, "1 downloaded");
        assert!(model.error.is_empty(), "a server that does not answer is said on its row");
        // No provider's details here: those are on its page.
        assert!(!has(&lines, "API Key") && !has(&lines, "Server"));
        assert_eq!(row(&lines, "Add Provider…").id, "go:provider:add");
        assert!(model.acted("go:provider:ollama", "").iter().any(|ask| ask.command == Command::ProviderModels("ollama".into())));
        assert_eq!((model.page.as_str(), model.section(), model.title().as_str()), ("provider:ollama", "models", "Models"));
    }

    #[test]
    fn a_provider_is_added_by_its_kind_and_then_its_form() {
        let mut model = told(providers(), json!({}));
        model.show("models");
        model.acted("go:provider:add", "");
        let lines = model.lines();
        assert_eq!((lines[0].kind, lines[0].id.as_str(), lines[0].label.as_str()), ("back", "go:models", "Models"));
        assert_eq!(head(&lines).label, "Add Provider");
        // Every kind there is but the one that is always there.
        let kinds: Vec<&str> = lines.iter().filter(|line| line.control == "link").map(|line| line.label.as_str()).collect();
        assert_eq!(kinds, ["Claude API", "OpenAI-Compatible Server", "Claude Code", "Codex", "Doubao (Volcengine)"]);
        assert!(model.acted(&row(&lines, "OpenAI-Compatible Server").id, "").iter().all(|ask| ask.command != Command::GetSettings));
        assert_eq!(model.page, "provider:new");
        let lines = model.lines();
        assert_eq!(head(&lines).label, "New OpenAI-Compatible Server");
        let form = card(&lines, "Settings");
        assert_eq!(form.iter().map(|line| line.label.as_str()).collect::<Vec<_>>(), ["Name", "Server", "API Key", "More Models"]);
        // Its fields are kept here until it is added.
        assert!(model.acted(&row(form, "Server").id, " http://127.0.0.1:8080/v1 ").is_empty());
        assert!(model.acted(&row(form, "Name").id, "Lab").is_empty());
        assert!(model.acted(&row(form, "API Key").id, "sk-1").is_empty());
        let lines = model.lines();
        assert_eq!(row(card(&lines, "Settings"), "API Key").placeholder, "Typed — type again to replace it");
        assert_eq!(row(&lines, "Add This Provider").more_id, "go:models");
        let asks = model.acted("act:save-provider", "");
        let config = BTreeMap::from([("url".to_string(), "http://127.0.0.1:8080/v1".to_string()), ("key".to_string(), "sk-1".to_string())]);
        assert_eq!(asks[0].command, Command::AddProvider { kind: "openai-compatible".into(), name: "Lab".into(), config });
        // Added: its own page, where it can be checked.
        let mut added = providers();
        added["providers"].as_array_mut().unwrap().push(json!({ "id": "openai-compatible", "kind": "openai-compatible", "name": "Lab", "config": { "url": "http://127.0.0.1:8080/v1" } }));
        let asks = model.answered_value(asks[0].then.clone(), Ok(added));
        assert_eq!((model.page.as_str(), model.draft.is_none()), ("provider:openai-compatible", true));
        assert!(asks.iter().any(|ask| ask.command == Command::ProviderModels("openai-compatible".into())));
        assert_eq!(head(&model.lines()).label, "Lab");
    }

    #[test]
    fn a_providers_page_edits_checks_and_removes_it() {
        let mut model = told(providers(), json!({}));
        model.show("provider:claude");
        let lines = model.lines();
        assert_eq!((lines[0].id.as_str(), head(&lines).label.as_str(), head(&lines).description.as_str()), ("go:models", "Claude", "Claude API"));
        let status = row(&lines, "Status").clone();
        assert!(status.description.starts_with("Connected with an API key.") && status.value == "Check Connection");
        assert_eq!(row(&lines, "What It Can Do").chips, ["Text", "Images", "Streaming"]);
        assert_eq!(row(&lines, "What It Can Do").description, "What it is given is sent to Anthropic.");
        // The key is never shown, only that it is saved, and it can be forgotten.
        let key = row(card(&lines, "Settings"), "API Key").clone();
        assert_eq!((key.control, key.value.as_str(), key.placeholder.as_str(), key.more.as_str()), ("secret", "", "Saved — type a new key to replace it", "Remove"));
        assert_eq!(asked(&model.acted(&key.id, "sk-ant-2")), Command::SetProviderField { provider: "claude".into(), field: "key".into(), value: "sk-ant-2".into() });
        assert!(model.acted(&key.id, "  ").is_empty());
        assert_eq!(asked(&model.acted(&key.more_id, "")), Command::SetProviderField { provider: "claude".into(), field: "key".into(), value: String::new() });
        assert_eq!(asked(&model.acted(&row(&lines, "Name").id, "Work")), Command::SetProviderField { provider: "claude".into(), field: "name".into(), value: "Work".into() });
        // Checked on a thread of its own; once at a time; what it came to is said.
        assert_eq!(asked(&model.acted(&status.id, "")), Command::CheckProvider("claude".into()));
        assert!(Command::CheckProvider("claude".into()).slow());
        assert!(model.acted(&status.id, "").is_empty());
        assert!(row(&model.lines(), "Status").busy);
        model.answered_value(Then::ProviderChecked("claude".into()), Err("The Claude API key is invalid.".into()));
        let failed = row(&model.lines(), "Connection Failed").clone();
        assert!(failed.danger && failed.description == "The Claude API key is invalid." && model.error.is_empty());
        model.acted(&status.id, "");
        model.answered_value(Then::ProviderChecked("claude".into()), Ok(json!({ "said": "Claude's API takes the key.", "models": null })));
        assert!(!row(&model.lines(), "Connection Works").danger);
        // The model it has, and what uses it.
        assert_eq!(row(card(&model.lines(), "Models"), "claude-opus-5-5").value, "Used for Polish");
        // Removed: back to the list.
        let remove = row(&lines, "Remove Provider").clone();
        assert_eq!(remove.control, "confirm");
        assert_eq!(asked(&model.acted(&remove.id, "")), Command::RemoveProvider("claude".into()));
        assert_eq!(model.page, "models");
    }

    #[test]
    fn a_server_lists_its_models_and_says_where_what_it_is_given_goes() {
        let mut model = told(providers(), json!({}));
        model.show("provider:ollama");
        assert!(row(&model.lines(), "Status").description.starts_with("Checking… Text stays on this computer."));
        // A check lists its models too.
        model.acted("check-provider:ollama", "");
        model.answered_value(Then::ProviderChecked("ollama".into()), Ok(json!({ "said": "The server answers and lists 2 models.", "models": ["qwen3", "llama3"] })));
        let lines = model.lines();
        assert!(row(&lines, "Status").description.starts_with("Connected: qwen3, llama3. Text stays on this computer.") && !row(&lines, "Status").danger);
        assert_eq!(row(&lines, "Connection Works").description, "The server answers and lists 2 models.");
        let models = card(&lines, "Models");
        assert_eq!(models.iter().map(|line| line.label.as_str()).collect::<Vec<_>>(), ["First Listed", "qwen3", "llama3"]);
        assert_eq!(row(models, "qwen3").value, "Used for AI");
        // Its address changed: asked again, and the check no longer said.
        let url = row(card(&lines, "Settings"), "Server").id.clone();
        assert_eq!(model.acted(&url, "http://127.0.0.1:11434/v1").len(), 0, "the same address is no change");
        let asks = model.acted(&url, "http://127.0.0.1:8080/v1");
        assert_eq!(asks.iter().map(|ask| ask.command.clone()).collect::<Vec<_>>(), [Command::SetProviderField { provider: "ollama".into(), field: "url".into(), value: "http://127.0.0.1:8080/v1".into() }, Command::ProviderModels("ollama".into())]);
        assert!(!has(&model.lines(), "Connection Works"));
        // One elsewhere, over plain http, that does not answer.
        model.show("provider:lan");
        model.answered_value(Then::Models("lan".into()), Err("connection refused".into()));
        let status = row(&model.lines(), "Status").clone();
        assert!(status.danger && status.description.starts_with("Not reachable: connection refused.") && status.description.contains("sent to 10.0.0.5:8000 over plain http"));
        assert_eq!(row(card(&model.lines(), "Models"), "Not reachable").description, "connection refused");
    }

    #[test]
    fn a_tools_page_says_whether_it_is_found() {
        let mut model = told(providers(), json!({}));
        model.show("provider:claude-code");
        let lines = model.lines();
        let status = row(&lines, "Status");
        assert!(!status.danger && status.description.starts_with("Using 2.1.302 (Claude Code) at /home/ada/.local/bin/claude."), "{}", status.description);
        assert_eq!(row(&lines, "What It Can Do").description, "Runs on this computer, and sends what it is given to Anthropic.");
        assert_eq!(card(&lines, "Models").iter().map(|line| line.label.as_str()).collect::<Vec<_>>(), ["Default", "opus", "sonnet", "haiku"]);
        model.show("provider:codex");
        let lines = model.lines();
        assert!(row(&lines, "Status").danger && row(&lines, "Status").description.starts_with("There is no executable at /opt/codex."));
        assert_eq!(row(card(&lines, "Settings"), "Executable").value, "/opt/codex");
    }

    #[test]
    fn doubao_asks_for_the_key_of_its_console_and_its_resources_are_speech_models() {
        let mut settings = with_providers(json!({ "dictationModel": { "provider": "doubao", "model": "" } }));
        settings["providers"].as_array_mut().unwrap().push(json!({ "id": "doubao", "kind": "doubao", "name": "Doubao", "config": {} }));
        let mut model = told(Value::Object(settings), json!({}));
        model.page = "models".into();
        assert_eq!(row(&model.lines(), "Doubao").description, "Doubao (Volcengine) · Key missing");
        model.show("provider:doubao");
        let lines = model.lines();
        assert!(row(&lines, "Status").danger && row(&lines, "Status").description.starts_with("The key is missing."));
        let form = card(&lines, "Settings");
        assert_eq!(row(form, "Console").value, "new");
        assert!(has(form, "API Key") && !has(form, "App ID") && row(form, "Remove Filler Words").control == "switch");
        assert!(has(card(&lines, "Models"), "Doubao Streaming ASR 2.0, Hourly"));
        // The old console: an App ID and an Access Token instead.
        model.settings["providers"][6]["config"] = json!({ "console": "old", "appId": "id" });
        model.settings["providerStates"]["doubao"] = json!({ "secretsSet": ["accessToken"] });
        let lines = model.lines();
        let form = card(&lines, "Settings");
        assert!(!row(&lines, "Status").danger && !has(form, "API Key") && row(form, "App ID").value == "id" && row(form, "Access Token").more == "Remove");
        assert_eq!(asked(&model.acted(&row(form, "Remove Filler Words").id, "true")), Command::SetProviderField { provider: "doubao".into(), field: "removeFillers".into(), value: "true".into() });
        // Recognition offers its resources beside the models of this computer.
        model.page = "dictation".into();
        let lines = model.lines();
        let recognition = row(&lines, "Speech Recognition");
        assert!(titles(recognition).contains(&"Doubao · Doubao Streaming ASR 2.0, Hourly"));
        assert_eq!(recognition.value, ids::choice("doubao", ""));
    }

    #[test]
    fn a_picker_offers_what_can_be_used_and_says_what_cannot() {
        let mut model = told(providers(), json!({}));
        model.page = "advanced".into();
        model.answered_value(Then::Models("ollama".into()), Ok(json!(["qwen3", "llama3"])));
        let lines = model.lines();
        let answers = row(&lines, "Model");
        assert_eq!(answers.value, ids::choice("ollama", "qwen3"));
        assert_eq!(&titles(answers)[..4], ["Claude · claude-opus-5-5", "Ollama · First Listed", "Ollama · qwen3", "Ollama · llama3"]);
        assert!(!answers.danger);
        // Cases: none, or a model that keeps what it is given here; never Claude, a tool, or the server elsewhere.
        let cases = row(&lines, "Clipboard Search Cases");
        assert_eq!(titles(cases), ["None", "Ollama · First Listed", "Ollama · qwen3", "Ollama · llama3"]);
        assert!(cases.value.is_empty() && !cases.danger);
        // Choosing is a change of the reference.
        assert_eq!(patch(&model.acted(&answers.id, &ids::choice("claude-code", "opus"))), json!({ "aiModel": { "provider": "claude-code", "model": "opus" } }));
        assert_eq!(patch(&model.acted(&cases.id, "")), json!({ "casesModel": { "provider": "", "model": "" } }));
        // A tool that is not found: said where it is chosen.
        model.settings.insert("aiModel".into(), json!({ "provider": "codex", "model": "" }));
        let answers = row(&model.lines(), "Model").clone();
        assert!(answers.danger && answers.description.starts_with("There is no executable at /opt/codex."));
        // A provider that is gone: still shown, and said.
        model.settings.insert("aiModel".into(), json!({ "provider": "gone", "model": "big" }));
        let answers = row(&model.lines(), "Model").clone();
        assert!(answers.danger && answers.value == ids::choice("gone", "big") && titles(&answers).contains(&"gone · big") && answers.description.contains("no longer connected"));
        // Models is a button away.
        assert_eq!(model.acted("act:open-models", "").iter().map(|ask| ask.command.name()).collect::<Vec<_>>(), ["dictation_status", "speech_models", "provider_models", "provider_models"]);
        assert_eq!(model.page, "models");
    }

    #[test]
    fn about_says_the_version_and_what_updating_is_doing() {
        let mut model = told(json!({ "version": "0.1.0", "canUpdate": false }), json!({}));
        model.page = "about".into();
        let about = |model: &Model| model.lines()[0].clone();
        assert_eq!((about(&model).value.as_str(), about(&model).more.as_str()), ("0.1.0", ""));
        model.settings.insert("canUpdate".into(), json!(true));
        assert_eq!(about(&model).more, "Check for Updates");
        assert_eq!(asked(&model.acted("act:update", "")).name(), "check_for_update");
        assert!(about(&model).busy && model.acted("act:update", "").is_empty());
        model.answered_value(Then::Checked, Ok(Value::Null));
        assert_eq!(about(&model).more, "Heyo Is Up to Date");
        model.acted("act:update", "");
        model.answered_value(Then::Checked, Ok(json!("0.2.0")));
        assert_eq!(about(&model).more, "Install Heyo 0.2.0 and Restart");
        assert_eq!(asked(&model.acted("act:update", "")).name(), "install_update");
        model.answered_value(Then::Installed, Err("no network".into()));
        assert_eq!((about(&model).more.as_str(), model.error.as_str()), ("Install Heyo 0.2.0 and Restart", "no network"));
        model.answered_value(Then::Copied, Ok(Value::Null));
        assert!(about(&model).on);
    }

    fn with_extensions() -> Model {
        let mut model = told(json!({ "aliases": { "app:firefox.desktop": "ff" }, "disabledCommands": ["cmd:zeta:two"], "clipboardHistory": true, "clipboardRetentionDays": 90, "clipboardIgnoredApps": ["org.keepassxc.KeePassXC.desktop", "gone.desktop"], "liveActivities": true, "activitiesOff": [] }), json!({ "snippetExpansion": "full", "session": "x11" }));
        model.hotkeys = json!({ "supported": true, "clipboard": "<Super>v" });
        model.extensions = vec![
            json!({ "id": "/data/zeta", "name": "zeta", "title": "Zeta Tools", "author": "someone", "icon": "icon.png", "builtin": false, "description": "Tools.",
                "preferences": [{ "name": "token", "type": "password", "title": "Token", "required": true }, { "name": "mode", "type": "dropdown", "title": "Mode", "data": [{ "title": "Fast", "value": "fast" }, { "title": "Careful", "value": "careful" }] }],
                "values": { "extension": { "mode": "careful", "token": "tok_1" }, "commands": {} },
                "commands": [{ "id": "cmd:zeta:one", "name": "one", "title": "First Thing", "description": null, "icon": "star-16", "preferences": [{ "name": "loud", "type": "checkbox", "title": "Sound", "label": "Play a sound", "default": true }] }, { "id": "cmd:zeta:two", "name": "two", "title": "Second Thing", "description": "Does more", "icon": null, "preferences": [] }] }),
            json!({ "id": BUILTIN, "name": BUILTIN, "title": "Heyo", "builtin": true, "icon": "store-16", "preferences": [], "values": { "extension": {}, "commands": {} }, "commands": [{ "id": "cmd:@heyo:clipboard-history", "name": "clipboard-history", "title": "Clipboard History", "description": "What was copied", "preferences": [] }, { "id": "cmd:@heyo:dictate", "name": "dictate", "title": "Dictate", "description": null, "preferences": [] }] }),
            json!({ "id": "/src/alpha", "name": "alpha", "title": "alpha Notes", "builtin": false, "development": "/src/alpha", "preferences": [], "values": { "extension": {}, "commands": {} }, "commands": [{ "id": "cmd:alpha:only", "name": "only", "title": "Only Command", "preferences": [] }] }),
        ];
        model.applications = vec![json!({ "id": "app:firefox.desktop", "title": "Firefox", "subtitle": "Web Browser", "icon": "/usr/share/icons/firefox.png" }), json!({ "id": "app:files.desktop", "title": "Files", "subtitle": null, "icon": "folder" })];
        model.item_hotkeys = json!({ "app:files.desktop": "<Super>e" }).as_object().unwrap().clone();
        model.sources = vec![json!({ "id": "org.keepassxc.KeePassXC.desktop", "name": "KeePassXC", "icon": "/icons/keepassxc.png", "seen": false }), json!({ "id": "firefox.desktop", "name": "Firefox", "icon": null, "seen": true }), json!({ "id": "files.desktop", "name": "Files", "icon": null, "seen": false })];
        model
    }

    #[test]
    fn the_sidebar_lists_pages_then_extensions_in_their_order() {
        let mut model = with_extensions();
        let (entries, pages) = model.nav();
        assert_eq!(pages, PAGES.len());
        assert_eq!(entries.iter().map(|entry| entry.title.as_str()).collect::<Vec<_>>(), ["General", "Shortcuts", "Dictation", "Screenshots", "Models", "Advanced", "About", "Applications", "Heyo Commands", "alpha Notes", "Zeta Tools"]);
        // A page's symbol, a symbol the host named, a file of the extension's own.
        assert_eq!((entries[0].icon.as_deref(), entries[8].icon.as_deref(), entries[10].icon.as_deref()), (Some("cog-16"), Some("store-16"), Some("/data/zeta/assets/icon.png")));
        assert!(entries[10].extension && !entries[0].extension);
        // An extension is found by a command of its own; the applications
        // only by their name.
        model.nav_search = "dict".into();
        let (entries, pages) = model.nav();
        assert_eq!((entries.iter().map(|entry| entry.title.as_str()).collect::<Vec<_>>(), pages), (vec!["Dictation", "Heyo Commands"], 1));
        model.nav_search = "firefox".into();
        assert_eq!(model.nav().0, []);
    }

    #[test]
    fn the_applications_are_a_list_one_can_search() {
        let mut model = with_extensions();
        assert_eq!(model.show(APPLICATIONS), []);
        assert_eq!((model.title().as_str(), model.find_placeholder()), ("Applications", "Search applications…"));
        let lines = model.lines();
        assert_eq!((lines[0].kind, lines[0].label.as_str(), lines[0].icon.as_deref()), ("head", "Applications", Some("app-window-grid-3x3-16")));
        let firefox = row(&lines, "Firefox");
        assert_eq!((firefox.description.as_str(), firefox.icon.as_deref(), firefox.hotkey.as_str(), firefox.control), ("Web Browser", Some("/usr/share/icons/firefox.png"), "app:firefox.desktop", ""));
        assert_eq!(row(&lines, "Files").keys, ["Super", "E"]);
        model.find = "FIRE".into();
        assert!(has(&model.lines(), "Firefox") && !has(&model.lines(), "Files"));
        model.find = "nothing".into();
        assert!(has(&model.lines(), "No matches"));
        // Another page starts with nothing searched for.
        model.show("general");
        assert!(model.find.is_empty());
    }

    #[test]
    fn an_extension_has_its_preferences_commands_permissions_and_removal() {
        let mut model = with_extensions();
        assert_eq!(asked(&model.show("/data/zeta")), Command::ExtensionPermissions("/data/zeta".into()));
        model.answered_value(Then::Permissions("/data/zeta".into()), Ok(json!([{ "permission": "windows", "allows": "see and move windows", "state": "allowed" }, { "permission": "ai", "allows": "ask the model", "state": "asks" }, { "permission": "activity", "allows": "show live activities", "state": "free" }])));
        let lines = model.lines();
        assert_eq!((lines[0].label.as_str(), lines[0].description.as_str(), lines[1].kind, lines[1].label.as_str()), ("Zeta Tools", "by someone", "note", "Tools."));
        // Preferences, of each kind the API has; a value stored, else the default.
        assert_eq!((row(&lines, "Token *").control, row(&lines, "Mode").control, row(&lines, "Mode").value.as_str(), titles(row(&lines, "Mode"))), ("password", "select", "careful", vec!["Fast", "Careful"]));
        // A password that is stored stays in its field (as dots): the field
        // is not one that forgets what was typed, or leaving it would save
        // the nothing it then held.
        assert_eq!(row(&lines, "Token *").value, "tok_1");
        let sound = row(&lines, "Sound");
        assert_eq!((sound.control, sound.on, sound.description.as_str()), ("switch", true, "Play a sound"));
        assert!(has(&lines, "First Thing Preferences"));
        // Commands: on unless disabled, each with its hotkey.
        assert!(row(&lines, "First Thing").on && !row(&lines, "Second Thing").on && row(&lines, "First Thing").icon.as_deref() == Some("star-16"));
        assert_eq!(patch(&model.acted("enable:cmd:zeta:two", "true")), json!({ "disabledCommands": [] }));
        // Permissions: what was allowed can be taken back; activities have a switch.
        let lines = model.lines();
        assert_eq!((row(&lines, "See and move windows").control, row(&lines, "See and move windows").description.as_str()), ("button", "Allowed by you"));
        assert_eq!((row(&lines, "Ask the model").control, row(&lines, "Show live activities").control, row(&lines, "Show live activities").on), ("", "switch", true));
        assert_eq!(model.acted(&row(&lines, "See and move windows").id, "")[0], ask(Command::RevokeExtensionPermission { extension: "zeta".into(), permission: crate::permissions::Permission::Windows }, Then::Permissions("/data/zeta".into())));
        assert_eq!(patch(&model.acted("activities:zeta", "false")), json!({ "activitiesOff": ["zeta"] }));
        // A preference is saved among the others of the extension.
        assert_eq!(saved(&model.acted(&sound.id, "false")), ("zeta".to_string(), json!({ "extension": { "mode": "careful", "token": "tok_1" }, "commands": { "one": { "loud": false } } })));
        assert_eq!(saved(&model.acted(&row(&lines, "Token *").id, "secret")).1["extension"], json!({ "mode": "careful", "token": "secret" }));
        // Removal, after which the window is elsewhere.
        assert_eq!((row(&lines, "Uninstall Extension").control, row(&lines, "Uninstall Extension").placeholder.as_str()), ("confirm", "Click Again to Uninstall"));
        assert_eq!(asked(&model.acted("uninstall:/data/zeta", "")), Command::UninstallExtension("/data/zeta".into()));
        assert_eq!(model.answered_value(Then::Uninstalled, Ok(Value::Null)).len(), 3);
        assert_eq!(model.page, "general");
        model.page = "/src/alpha".into();
        assert!(has(&model.lines(), "Stop Developing") && model.lines()[0].description == "In development from /src/alpha");
    }

    #[test]
    fn heyos_commands_have_the_clipboard_and_snippets() {
        let mut model = with_extensions();
        assert_eq!(model.show(BUILTIN).iter().map(|ask| ask.command.name()).collect::<Vec<_>>(), ["clipboard_sources", "clipboard_meaning_status"]);
        model.meaning = json!({ "installed": true, "installing": false, "indexed": 1200, "waiting": 3400, "problem": null });
        model.settings.insert("clipboardMeaning".into(), json!(true));
        let lines = model.lines();
        assert_eq!((lines[0].description.as_str(), model.title().as_str()), ("Built into Heyo", "Heyo Commands"));
        // The clipboard history's hotkey is Heyo's own, on a row of its own.
        assert!(row(&lines, "Clipboard History").hotkey.is_empty() && row(&lines, "Dictate").hotkey == "cmd:@heyo:dictate");
        assert_eq!((row(&lines, "Hotkey").hotkey.as_str(), row(&lines, "Hotkey").keys.clone()), ("@clipboard", vec!["Super".to_string(), "V".to_string()]));
        assert_eq!((row(&lines, "Keep History For").value.as_str(), titles(row(&lines, "Keep History For"))[3]), ("90", "3 Months"));
        assert_eq!(patch(&model.acted("num:clipboardRetentionDays", "7")), json!({ "clipboardRetentionDays": 7 }));
        assert_eq!(row(&lines, "Clear History").control, "confirm");
        assert!(row(&lines, "Find by Meaning").description.ends_with("Reading the history: 3,400 entries to go; until then some are found by their words only."));
        assert_eq!(model.poll().len(), 1);
        // Left out of the history: by name where the application is known.
        assert_eq!((row(&lines, "KeePassXC").icon.as_deref(), row(&lines, "gone.desktop").description.as_str(), row(&lines, "gone.desktop").icon.as_deref()), (Some("/icons/keepassxc.png"), "Not installed", Some("app-window-16")));
        assert_eq!(titles(row(&lines, "Never Record Copies From")), ["Add Application…", "Firefox", "Files"]);
        assert_eq!(patch(&model.acted("ignore:add", "firefox.desktop")), json!({ "clipboardIgnoredApps": ["org.keepassxc.KeePassXC.desktop", "gone.desktop", "firefox.desktop"] }));
        assert_eq!(patch(&model.acted("ignore:remove:gone.desktop", ""))["clipboardIgnoredApps"], json!(["org.keepassxc.KeePassXC.desktop", "firefox.desktop"]));
        assert!(model.acted("ignore:add", "").is_empty());
        // With recording off, what only matters while it is on is not shown.
        model.settings.insert("clipboardHistory".into(), json!(false));
        let lines = model.lines();
        assert!(!has(&lines, "Pause Recording") && !has(&lines, "Find by Meaning") && !has(&lines, "Never Record Copies From"));
        // Snippets: the switch, and where it is not on offer, why.
        assert_eq!(row(&lines, "Expand Snippets as You Type").control, "switch");
        model.capabilities = json!({ "snippetExpansion": "none", "session": "wayland" });
        let lines = model.lines();
        assert!(row(&lines, "Expand Snippets as You Type").control.is_empty() && row(&lines, "Expand Snippets as You Type").description.starts_with("Not available on Wayland"));
    }

    #[test]
    fn find_by_meaning_offers_its_download_until_it_is_there() {
        let mut model = with_extensions();
        model.page = BUILTIN.into();
        model.meaning = json!({ "installed": false, "installing": false, "indexed": 0, "waiting": 0, "problem": null });
        let lines = model.lines();
        assert_eq!((row(&lines, "Find by Meaning").control, row(&lines, "Find by Meaning").id.as_str()), ("button", "act:install-meaning"));
        assert_eq!(asked(&model.acted("act:install-meaning", "")).name(), "clipboard_meaning_install");
        assert_eq!(row(&model.lines(), "Find by Meaning").value, "Downloading…");
        model.heard(crate::wire::event::CLIPBOARD_MEANING_INSTALL, &json!({ "type": "installed" }));
        model.meaning = json!({ "installed": true, "installing": false, "indexed": 1, "waiting": 0, "problem": "the model would not load" });
        model.settings.insert("clipboardMeaning".into(), json!(true));
        let lines = model.lines();
        assert!(row(&lines, "Find by Meaning").control == "switch" && row(&lines, "Find by Meaning").description.ends_with("It is not working at present: the model would not load"));
        assert!(model.poll().is_empty());
    }

    #[test]
    fn the_shortcuts_table_opens_narrows_and_changes() {
        let mut model = with_extensions();
        model.show("shortcuts");
        let labels = |model: &Model| model.lines().iter().filter(|line| line.kind == "group" || line.kind == "entry").map(|line| line.label.clone()).collect::<Vec<_>>();
        // Everything starts closed but the extensions; one with a single
        // command is shown as that command.
        assert_eq!(labels(&model), ["Applications", "Extensions", "Zeta Tools", "Heyo Commands", "Only Command"]);
        assert!(model.acted("open:applications", "toggle").is_empty());
        model.acted("open:/data/zeta", "toggle");
        assert_eq!(labels(&model), ["Applications", "Firefox", "Files", "Extensions", "Zeta Tools", "First Thing", "Second Thing", "Heyo Commands", "Only Command"]);
        let lines = model.lines();
        let (firefox, second, zeta) = (row(&lines, "Firefox"), row(&lines, "Second Thing"), row(&lines, "Zeta Tools"));
        assert_eq!((firefox.alias.as_str(), firefox.checkable, firefox.hotkey.as_str()), ("ff", false, "app:firefox.desktop"));
        assert!(second.checkable && !second.on && second.indent);
        // Some of its commands are on: neither ticked nor empty.
        assert!(zeta.checkable && zeta.on && zeta.mixed && zeta.open);
        assert_eq!(patch(&model.acted(&zeta.check_id, "false"))["disabledCommands"], json!(["cmd:zeta:one", "cmd:zeta:two"]));
        assert_eq!(patch(&model.acted("alias:app:files.desktop", " fm ")), json!({ "aliases": { "app:firefox.desktop": "ff", "app:files.desktop": "fm" } }));
        assert!(model.acted("alias:app:files.desktop", "fm").is_empty());
        assert_eq!(patch(&model.acted("alias:app:firefox.desktop", ""))["aliases"], json!({ "app:files.desktop": "fm" }));
        // Searching opens everything it finds something in, by a command's
        // title or its group's.
        model.find = "zeta".into();
        assert_eq!(labels(&model), ["Extensions", "Zeta Tools", "First Thing", "Second Thing"]);
        model.find.clear();
        // The menu: what the table is narrowed to.
        let (title, ticked, picks) = model.filter().unwrap();
        assert_eq!((title.as_str(), ticked, picks.len()), ("All categories", 0, 9));
        model.pick("mod:hotkey");
        assert_eq!(labels(&model), ["Applications", "Files"]);
        model.pick("mod:hotkey");
        model.pick("mod:disabled");
        assert!(labels(&model).contains(&"First Thing".to_string()) && !labels(&model).contains(&"Firefox".to_string()));
        model.pick("mod:disabled");
        model.pick("cat:applications");
        assert_eq!((labels(&model)[0].as_str(), model.filter().unwrap().0.as_str(), labels(&model).contains(&"Extensions".to_string())), ("Applications", "Applications", false));
        model.pick("cat:all");
        model.find = "no such thing".into();
        assert_eq!(model.lines().iter().map(|line| (line.kind, line.label.as_str())).collect::<Vec<_>>(), [("note", "Nothing matches."), ("end", "")]);
        assert!(model.filter().is_some());
        model.show("general");
        assert!(model.filter().is_none());
    }

    #[test]
    fn what_changes_elsewhere_is_read_again() {
        let mut model = with_extensions();
        use crate::wire::event;
        assert_eq!(asked(&model.heard(event::SETTINGS_CHANGED, &Value::Null)).name(), "get_settings");
        assert_eq!(model.heard(event::INDEX_READY, &Value::Null).iter().map(|ask| ask.command.name()).collect::<Vec<_>>(), ["list_extensions", "list_applications", "item_hotkeys"]);
        assert_eq!(model.opened().iter().map(|ask| ask.command.name()).collect::<Vec<_>>(), ["get_settings", "get_hotkeys", "get_capabilities", "list_extensions", "list_applications", "item_hotkeys"]);
        // A preference that was saved: the extensions are read again.
        assert_eq!(model.answered_value(Then::Reload, Ok(Value::Null)).len(), 3);
        model.answered_value(Then::Extensions, Ok(json!([])));
        assert_eq!(model.nav().0.len(), PAGES.len() + 1);
    }
}

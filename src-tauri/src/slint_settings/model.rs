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

use std::collections::BTreeSet;

use serde_json::{json, Map, Value};

use super::keys;

/// The pages: id, title, and the symbol on its tile in the sidebar.
pub(crate) const PAGES: &[(&str, &str, &str)] = &[
    ("general", "General", "cog-16"),
    ("shortcuts", "Shortcuts", "keyboard-16"),
    ("dictation", "Dictation", "microphone-16"),
    ("screenshots", "Screenshots", "camera-16"),
    ("advanced", "Advanced", "wrench-screwdriver-16"),
    ("about", "About", "info-01-16"),
];

/// The id the applications have among the extensions of the sidebar.
pub(crate) const APPLICATIONS: &str = "@applications";
/// Heyo's own commands.
pub(crate) const BUILTIN: &str = "@heyo";
/// The command whose hotkey is one of Heyo's own, set on its own row.
const CLIPBOARD_COMMAND: &str = "cmd:@heyo:clipboard-history";
/// Between the parts of an id that names several things.
const SEP: char = '\u{1f}';

const DICTATION_LANGUAGES: &[(&str, &str)] = &[("auto", "Detect Automatically"), ("zh", "Chinese (Mandarin)"), ("yue", "Cantonese"), ("en", "English"), ("ja", "Japanese"), ("ko", "Korean")];

const POLISH_MODES: &[(&str, &str, &str)] = &[
    ("off", "Off", "The text is left as it was recognized."),
    ("clean", "Clean Up", "When you finish, Claude removes filler words, applies your corrections and fixes punctuation, then the result is pasted."),
    ("translate", "Translate to English", "When you finish, Claude cleans up what you said and pastes it in English."),
];

const RETENTION: &[(&str, &str)] = &[("1", "1 Day"), ("7", "7 Days"), ("30", "1 Month"), ("90", "3 Months"), ("180", "6 Months"), ("365", "1 Year")];

const RENDERERS: &[(&str, &str)] = &[("auto", "Automatic"), ("cpu", "CPU"), ("gpu-shared-memory", "GPU, shared memory"), ("gpu-dma-buf", "GPU, DMA-BUF (WebKit's default)")];

/// What the Shortcuts table can be narrowed to: id, title.
const MODIFICATIONS: &[(&str, &str)] = &[("hotkey", "Hotkey Set"), ("alias", "Alias Set"), ("enabled", "Enabled"), ("disabled", "Disabled")];
const CATEGORIES: &[(&str, &str)] = &[("all", "All categories"), ("applications", "Applications"), ("extensions", "Extensions")];

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
    pub alias: String,
    pub checkable: bool,
    pub open: bool,
    pub busy: bool,
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

    fn badge(mut self, words: &str) -> Line {
        (self.control, self.value) = ("badge", words.to_string());
        self
    }

    fn text(mut self, words: impl Into<String>) -> Line {
        (self.control, self.value) = ("text", words.into());
        self
    }

    fn more(mut self, words: &str) -> Line {
        self.more = words.to_string();
        self
    }

    /// A field that records the hotkey `name`, showing the one bound.
    fn hotkey(mut self, name: &str, accelerator: Option<&Value>) -> Line {
        self.hotkey = name.to_string();
        self.keys = accelerator.and_then(Value::as_str).filter(|a| !a.is_empty()).map(keys::parts).unwrap_or_default();
        self
    }
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
    pub command: &'static str,
    pub args: Value,
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
    Ocr,
    Meaning,
    Sources,
    /// What the extension with this id may ask Heyo for.
    Permissions(String),
    /// The models the server at this address lists.
    Models(String),
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

fn ask(command: &'static str, args: Value, then: Then) -> Ask {
    Ask { command, args, then }
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
    /// The address the local model's server was asked at, and its models.
    pub models: Option<(String, Result<Vec<String>, String>)>,
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
    /// How far the speech engine's download is, while one runs.
    pub progress: Option<String>,
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

fn count(n: u64, one: &str, many: &str) -> String {
    let mut digits = n.to_string();
    let mut at = digits.len();
    while at > 3 {
        at -= 3;
        digits.insert(at, ',');
    }
    format!("{digits} {}", if n == 1 { one } else { many })
}

/// Where the "local model" setting really points: this computer, or
/// another (by name, and whether what is sent there is encrypted), or
/// nowhere a model could be. Only loopback is this computer: an address in
/// a private range is another machine, however near.
#[derive(Debug, PartialEq)]
pub(crate) enum Address {
    ThisComputer,
    Elsewhere { host: String, encrypted: bool },
    Unknown,
}

pub(crate) fn address(url: &str) -> Address {
    use url::{Host, Url};
    let Ok(url) = Url::parse(url.trim()) else { return Address::Unknown };
    if url.scheme() != "http" && url.scheme() != "https" {
        return Address::Unknown;
    }
    let local = match url.host() {
        None => return Address::Unknown,
        Some(Host::Domain(name)) => {
            let name = name.trim_end_matches('.');
            name == "localhost" || name.ends_with(".localhost")
        }
        Some(Host::Ipv4(ip)) => ip.is_loopback(),
        Some(Host::Ipv6(ip)) => ip.is_loopback() || ip.to_ipv4_mapped().is_some_and(|ip| ip.is_loopback()),
    };
    if local {
        return Address::ThisComputer;
    }
    let host = match url.port() {
        Some(port) => format!("{}:{port}", url.host_str().unwrap_or_default()),
        None => url.host_str().unwrap_or_default().to_string(),
    };
    Address::Elsewhere { host, encrypted: url.scheme() == "https" }
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
        json!({ "id": APPLICATIONS, "name": APPLICATIONS, "title": "Applications", "description": "Give an application a hotkey to launch it from anywhere.", "icon": "app-window-grid-3x3-16", "builtin": true, "commands": commands, "preferences": [], "values": { "extension": {}, "commands": {} } })
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
        if extension["id"] == BUILTIN { "Heyo Commands".into() } else { text(extension, "title") }
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
        let mut entries: Vec<Entry> = PAGES.iter().filter(|(_, title, _)| found(vec![title.to_string()])).map(|(id, title, icon)| Entry { id: id.to_string(), title: title.to_string(), icon: Some(icon.to_string()), extension: false }).collect();
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

    pub(crate) fn title(&self) -> String {
        match PAGES.iter().find(|(id, ..)| *id == self.page) {
            Some((_, title, _)) => title.to_string(),
            None => self.extension().as_ref().map(Self::extension_title).unwrap_or_else(|| "Settings".into()),
        }
    }

    /// The words in the field above the page's lines, if it has one.
    pub(crate) fn find_placeholder(&self) -> &'static str {
        match self.page.as_str() {
            "shortcuts" => "Search…",
            APPLICATIONS => "Search applications…",
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
        let title = CATEGORIES.iter().find(|(id, _)| *id == self.category).map_or("All categories", |(_, title)| title);
        let mut picks = vec![(String::new(), "Modifications".to_string(), true, false)];
        picks.extend(MODIFICATIONS.iter().map(|(id, title)| (format!("mod:{id}"), title.to_string(), false, self.modifications.contains(*id))));
        picks.push((String::new(), "Categories".to_string(), true, false));
        picks.extend(CATEGORIES.iter().map(|(id, title)| (format!("cat:{id}"), title.to_string(), false, self.category == *id)));
        Some((title.to_string(), self.modifications.len(), picks))
    }

    /// An entry of that menu was ticked.
    pub(crate) fn pick(&mut self, id: &str) {
        if let Some(modification) = id.strip_prefix("mod:") {
            if !self.modifications.remove(modification) {
                self.modifications.insert(modification.to_string());
            }
        } else if let Some(category) = id.strip_prefix("cat:") {
            self.category = category.to_string();
        }
    }

    /// The lines of the page that is shown.
    pub(crate) fn lines(&self) -> Vec<Line> {
        match self.page.as_str() {
            "general" => self.general(),
            "shortcuts" => self.shortcuts(),
            "dictation" => self.dictation_page(),
            "screenshots" => self.screenshots(),
            "advanced" => self.advanced(),
            "about" => self.about(),
            _ => self.extension().map(|extension| self.extension_page(&extension)).unwrap_or_default(),
        }
    }

    // --- General -----------------------------------------------------------

    fn general(&self) -> Vec<Line> {
        let mut page = Page::default();
        // GNOME's Shell extension, where there is a use for it and it is
        // not running as it should.
        let extension = match self.capabilities["extension"].as_str() {
            Some("missing") => Some(("On Wayland, switching and arranging windows, pasting, clipboard history, screenshots and the hold-to-talk key go through a small GNOME Shell extension that only Heyo can use.", "Install")),
            Some("outdated") => Some(("This version of Heyo comes with a newer extension than the one installed. Update it, then log out and back in. An extension that is already running keeps working until then, without what the new one adds.", "Update")),
            Some("pending") => Some(("In place. Log out and back in to finish: GNOME loads a new or updated extension only when a session starts.", "")),
            _ => None,
        };
        if let Some((description, action)) = extension {
            let row = Line::row("act:install-shell-extension", "GNOME Shell Extension").says(description);
            page.card("Wayland", vec![if action.is_empty() { row } else { row.button(action) }]);
        }
        let hotkey = Line::row("", "Heyo Hotkey");
        page.card(
            "",
            vec![
                Line::row("set:launchAtLogin", "Open at Login").switch(self.set("launchAtLogin")),
                if self.hotkeys_supported() { hotkey.hotkey("@toggle", self.hotkeys.get("toggle")) } else { hotkey.says("Bind a desktop keyboard shortcut to heyo.") },
            ],
        );
        page.card("Appearance", vec![Line::row("set:appearance", "Theme").says("Light, dark, or whichever the system is using").select(&self.chosen("appearance"), [("system", "System"), ("light", "Light"), ("dark", "Dark")])]);
        page.card(
            "Window",
            vec![
                Line::row("set:compactMode", "Compact Mode").says("Show only the search field until you type").switch(self.set("compactMode")),
                Line::row("set:hideOnBlur", "Hide When Focus Is Lost").says("Close the window when you click elsewhere").switch(self.set("hideOnBlur")),
                Line::row("set:keepCommand", "Keep the Running Command").says("Reopening Heyo returns to the command you were using instead of the root search").switch(self.set("keepCommand")),
            ],
        );
        page.card("Live Activities", vec![Line::row("set:liveActivities", "Show What Is in Progress").says("Timers, downloads and what extensions report as under way, at the top of the root search, with a notification when one ends or needs you").switch(self.set("liveActivities"))]);
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
                hotkey: if id == CLIPBOARD_COMMAND { String::new() } else { id.clone() },
                keys: self.item_hotkeys.get(&id).and_then(Value::as_str).map(keys::parts).unwrap_or_default(),
                checkable: !application,
                on: !disabled.contains(id.as_str()),
                indent,
                id,
                ..Line::default()
            }
        };
        let group = |id: &str, label: &str, icon: &str| Line { kind: "group", id: format!("open:{id}"), label: label.to_string(), icon: Some(icon.to_string()), open: open(id), ..Line::default() };
        let mut page = Page::default();

        let applications = self.applications_extension();
        let apps: Vec<&Value> = list(&applications, "commands").iter().filter(|command| visible(command, "Applications", true)).collect();
        if self.category != "extensions" && (!apps.is_empty() || !filtering) {
            let mut rows = vec![group("applications", "Applications", "app-window-grid-3x3-16")];
            if open("applications") {
                rows.push(Line { kind: "columns", ..Line::default() });
                rows.extend(apps.iter().map(|command| entry(&applications, command, false, true)));
            }
            page.card("", rows);
        }

        let groups: Vec<(&Value, Vec<&Value>)> = self.extensions.iter().map(|extension| (extension, list(extension, "commands").iter().filter(|command| visible(command, &text(extension, "title"), false)).collect::<Vec<_>>())).filter(|(_, commands)| !commands.is_empty()).collect();
        if self.category != "applications" && (!groups.is_empty() || !filtering) {
            let mut rows = vec![group("extensions", "Extensions", "plug-16")];
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
                    rows.push(Line { kind: "group", id: format!("open:{id}"), label: Self::extension_title(extension), icon: Self::icon(extension, &extension["icon"]), indent: true, open: open(&id), checkable: true, on: enabled > 0, mixed: enabled > 0 && enabled < all.len(), ..Line::default() });
                    if open(&id) {
                        rows.extend(commands.iter().map(|command| entry(extension, command, true, false)));
                    }
                }
            }
            page.card("", rows);
        }
        if filtering && apps.is_empty() && groups.is_empty() {
            page.note("Nothing matches.");
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
            Some(&"composed") => format!(
                "Your words go into the window as you speak, through the input method: the sentence you are speaking is shown there as text being composed, and becomes the window's text when it is finished. Your own input method (Pinyin, say) is set aside while you dictate and comes back when you finish. {} When this is off, nothing is shown before a sentence is finished.",
                if ways.contains(&"rewritten") {
                    "A window that takes no input method gets the words by pasting, corrected where they stand until you press a key or click."
                } else {
                    "A window that takes no input method gets each sentence pasted when it is finished."
                }
            ),
            Some(&"rewritten") => "Your words go into the window as they are recognized, and are corrected there when a later word changes an earlier one. Heyo only ever erases what it wrote itself: once you press a key or click while dictating, it stops correcting and adds each finished sentence where the cursor is. In terminals, password managers and the applications excluded from snippet expansion, and when this is off, each sentence is pasted when it is finished.".into(),
            Some(_) if self.capabilities["session"] == "wayland" => "Not available on Wayland, where Heyo cannot tell whether you typed in between, and so must not erase anything. Each sentence is pasted when it is finished.".into(),
            Some(_) => "Not available in this desktop session. Each sentence is pasted when it is finished.".into(),
        }
    }

    fn dictation_page(&self) -> Vec<Line> {
        let mut page = Page::default();
        page.note("Hold the hotkey while you speak and let go, or tap it to start and tap again to finish: what you said is typed into the window you are working in. Speech is recognized on this computer.");
        let installed = flag(&self.dictation, "installed");
        let engine = Line::row("act:install-dictation", "Speech Recognition").says(if installed { "SenseVoice is installed. Audio never leaves this computer." } else { "A one-time download of about 270 MB (SenseVoice speech model)." });
        let engine = if installed {
            engine.badge("Installed")
        } else if self.progress.is_some() || flag(&self.dictation, "installing") {
            engine.text(self.progress.clone().unwrap_or_else(|| "Downloading…".into()))
        } else {
            engine.button("Download")
        };
        let hotkey = Line::row("", "Hotkey");
        let hotkey = if self.hotkeys_supported() { hotkey.says("Tap to start and tap again to finish, or hold it while you speak").hotkey("@dictation", self.hotkeys.get("dictation")) } else { hotkey.says("Bind a desktop keyboard shortcut to heyo --dictate.") };
        let known = !self.capabilities.is_null();
        let hold = Line::row("set:dictationHoldKey", "Hold-to-Talk Key");
        let hold = if self.capabilities["holdKey"] == "none" {
            hold.says("A single key cannot be watched in this desktop session; use the hotkey instead.")
        } else {
            let hold = hold.says(if self.chosen("dictationHoldKey").is_empty() { "A single key to hold while you speak. It keeps working as usual in shortcuts." } else { "Hold it and speak; let go to finish. A quick tap, or using it in a shortcut, does nothing." });
            if known { hold.select(&self.chosen("dictationHoldKey"), std::iter::once(("", "None")).chain(list(&self.capabilities, "holdKeys").iter().filter_map(|key| Some((key["value"].as_str()?, key["title"].as_str()?))))) } else { hold }
        };
        let mut rows = vec![engine, hotkey, hold];
        let live = self.live_description();
        if !live.is_empty() {
            let row = Line::row("set:dictationLive", "Type as You Speak").says(live);
            rows.push(if list(&self.capabilities, "dictationWords").first().is_some_and(|way| way != "pasted") { row.switch(self.set("dictationLive")) } else { row });
        }
        page.card("", rows);

        let microphones = list(&self.dictation, "microphones");
        let none = !self.dictation.is_null() && microphones.is_empty();
        page.card(
            "Input",
            vec![
                Line::row("set:dictationSource", "Microphone").says(if none { "No microphone found. Connect one to use dictation." } else { "" }).warns(none).select(&self.chosen("dictationSource"), std::iter::once(("", "System Default")).chain(microphones.iter().filter_map(|m| Some((m["name"].as_str()?, m["description"].as_str()?))))),
                Line::row("set:dictationLanguage", "Language").says("Automatic detection handles mixed Chinese and English").select(&self.chosen("dictationLanguage"), DICTATION_LANGUAGES.iter().copied()),
            ],
        );
        page.card(
            "Vocabulary",
            vec![
                Line::row("", "Your Words").says("Names, jargon and product names, one to a line, written the way you want them: “type script” then comes out as TypeScript. For a word that is heard as something else, write heard as -> written as. The recognizer cannot be taught new words, so these are put right in its text; a Chinese name heard as other characters of the same sound is only fixed if you list those characters as “heard as”. Polishing with AI is given the list too."),
                Line { kind: "area", id: "words".into(), label: "Your Words".into(), value: self.names("dictationVocabulary").join("\n"), placeholder: "TypeScript\nKubernetes\njason -> JSON\n杰森 -> JSON".into(), ..Line::default() },
            ],
        );
        let polish = self.chosen("dictationPolish");
        let mode = POLISH_MODES.iter().find(|(value, ..)| *value == polish).unwrap_or(&POLISH_MODES[0]);
        let mut rows = vec![Line::row("set:dictationPolish", "Mode").says(mode.2).select(mode.0, POLISH_MODES.iter().map(|(value, title, _)| (*value, *title)))];
        if mode.0 != "off" && self.chosen("aiProvider") != "local" && !self.set("anthropicApiKeySet") {
            rows.push(Line::row("", "Claude API Key").says("Polishing needs a Claude API key or a local model. Set either in Advanced."));
        }
        page.card("Polish with AI", rows);
        page.done()
    }

    // --- Screenshots -------------------------------------------------------

    fn screenshots(&self) -> Vec<Line> {
        let mut page = Page::default();
        page.note("Drag to select part of the screen, or click a window to select exactly that. Mark it up with rectangles, arrows or the pen, then copy it, save it to Pictures/Screenshots, pin it on top of other windows, or copy the text in it.");
        let hotkey = |id: &str, title: &str, description: &str| {
            let row = Line::row("", title).says(description);
            if self.hotkeys_supported() { row.hotkey(id, self.item_hotkeys.get(id)) } else { row }
        };
        page.card("Hotkeys", vec![hotkey("heyo:screenshot", "Capture Screenshot", "Enter copies, Ctrl+S saves, Esc cancels"), hotkey("heyo:screenshot-text", "Capture Text from Screen", "Drag over text and it is copied")]);
        let installed = flag(&self.ocr, "installed");
        let engine = Line::row("act:install-ocr", "Recognition Engine").says(if installed { "PaddleOCR is installed. Chinese and English; images never leave this computer." } else { "A one-time download of about 340 MB (PaddleOCR, Chinese and English)." });
        page.card("Text Recognition", vec![if installed { engine.badge("Installed") } else if self.ocr_installing || flag(&self.ocr, "installing") { engine.text("Downloading…") } else { engine.button("Download") }]);
        page.done()
    }

    // --- Advanced ----------------------------------------------------------

    /// Whether the local model's server answers, and where what is sent to
    /// it goes; the second is whether that is a warning.
    fn server_description(&self) -> (String, bool) {
        let url = self.chosen("localModelUrl");
        let status = match &self.models {
            Some((asked, Ok(models))) if *asked == url && models.is_empty() => "Connected, but the server lists no models.".to_string(),
            Some((asked, Ok(models))) if *asked == url => format!("Connected: {}", models.join(", ")),
            Some((asked, Err(e))) if *asked == url => format!("Not reachable: {e}. Any OpenAI-compatible server works (vLLM, LM Studio, Ollama)."),
            _ => "Checking…".to_string(),
        };
        match address(&url) {
            Address::ThisComputer => (format!("{status} Text stays on this computer."), false),
            Address::Unknown => (format!("{status} This is not an http or https address."), true),
            Address::Elsewhere { host, encrypted: true } => (format!("{status} This address is not this computer: the text you select, ask about and dictate is sent to {host}."), true),
            Address::Elsewhere { host, encrypted: false } => (format!("{status} This address is not this computer, and the connection is not encrypted: the text you select, ask about and dictate is sent to {host} over plain http, readable by anyone on the network in between. Use https, or a server on this computer."), true),
        }
    }

    fn advanced(&self) -> Vec<Line> {
        let mut page = Page::default();
        let node = self.setting("nodeVersion");
        let found = node.get("Ok").and_then(Value::as_str);
        let version = match found {
            Some(version) => format!("Using {version}."),
            None => text(node, "Err"),
        };
        page.card(
            "Extensions",
            vec![
                Line::row("set:nodePath", "Node.js").says(format!("{version}{} Extensions need Node.js 22 or newer.", if self.set("nodeOverridden") { " HEYO_NODE overrides this setting." } else { "" })).warns(found.is_none()).field(self.chosen("nodePath"), "node (from PATH)"),
                Line::row("act:open-extensions", "Extensions Folder").says(self.chosen("extensionsDir")).button("Open"),
            ],
        );
        page.card(
            "Rendering",
            vec![Line::row("set:renderer", "Renderer").says("How the palette and what else the web view draws is drawn. Automatic keeps to the CPU on an NVIDIA driver without modesetting (the GPU cannot share frames there) and lets WebKit choose elsewhere; GPU with shared memory draws on the GPU and copies each frame, which is faster at 200% scale and keeps every CSS effect. Takes effect when Heyo is next started.").select(&self.chosen("renderer"), RENDERERS.iter().copied())],
        );
        let local = self.chosen("aiProvider") == "local";
        let mut rows = vec![Line::row("set:aiProvider", "Model").says("Answers Selected Text Actions and polishes dictation.").select(if local { "local" } else { "claude" }, [("claude", "Claude"), ("local", "Local Model")])];
        if local {
            let (description, warning) = self.server_description();
            rows.push(Line::row("set:localModelUrl", "Server").says(description).warns(warning).field(self.chosen("localModelUrl"), "http://127.0.0.1:8000/v1"));
            rows.push(Line::row("set:localModelName", "Model Name").says("Leave empty to use the first model the server offers.").field(self.chosen("localModelName"), "automatic"));
        } else {
            let saved = self.set("anthropicApiKeySet");
            let key = Line::row("apikey", "API Key").says("Leave empty to use ANTHROPIC_API_KEY or an `ant auth login` profile.").secret(if saved { "Saved — type a new key to replace it" } else { "sk-ant-…" });
            rows.push(if saved { key.more("Remove") } else { key });
        }
        page.card("AI", rows);
        page.card("Pasting", vec![Line::row("set:confirmTerminalPaste", "Ask Before Pasting Several Lines into a Terminal").says("A terminal runs each pasted line as a command. Dictation never asks: there, line breaks become spaces.").switch(self.set("confirmTerminalPaste"))]);
        page.card("Data", vec![Line::row("", "Settings File").says(self.chosen("settingsPath"))]);
        page.done()
    }

    // --- About -------------------------------------------------------------

    fn about(&self) -> Vec<Line> {
        let update = match &self.update {
            _ if !self.set("canUpdate") => String::new(),
            Update::Unknown => "Check for Updates".into(),
            Update::Checking => "Checking…".into(),
            Update::Newest => "Heyo Is Up to Date".into(),
            Update::Newer(version) => format!("Install Heyo {version} and Restart"),
            Update::Installing(_) => "Installing…".into(),
        };
        vec![Line {
            kind: "about",
            value: self.chosen("version"),
            description: "A command palette launcher for Linux that runs Raycast extensions.".into(),
            placeholder: "For a bug report: versions, the desktop session and the end of Heyo's log, with keys and tokens removed. Read it before you share it.".into(),
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
        let row = Line::row(format!("pref:{}{SEP}{command}{SEP}{name}{SEP}{kind}", text(extension, "name")), format!("{}{}", title.or(label).unwrap_or(&name), if flag(preference, "required") { " *" } else { "" }));
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
    fn left_out(&self, id: &str, remove: &str, missing: &str) -> Line {
        let source = self.sources.iter().find(|source| source["id"] == id);
        Line::row(format!("{remove}{id}"), source.map_or(id.to_string(), |source| text(source, "name")))
            .says(if source.is_some() { "" } else { missing })
            .pictured(Some(source.and_then(|source| source["icon"].as_str()).unwrap_or("app-window-16").to_string()))
            .button("Remove")
    }

    /// The cards about the clipboard history, on the page of Heyo's commands.
    fn clipboard(&self, page: &mut Page) {
        let recording = self.set("clipboardHistory");
        let mut rows = Vec::new();
        if self.hotkeys_supported() {
            rows.push(Line::row("", "Hotkey").says("Opens Clipboard History from anywhere").hotkey("@clipboard", self.hotkeys.get("clipboard")));
        }
        rows.push(Line::row("set:clipboardHistory", "Record Clipboard History").says("Copies marked as secret by password managers are never recorded").switch(recording));
        if recording {
            rows.push(Line::row("set:clipboardPaused", "Pause Recording").says("Nothing you copy is recorded while this is on. “Pause or Resume Clipboard History” in the root search does the same, and can have a hotkey.").switch(self.set("clipboardPaused")));
        }
        rows.push(Line::row("num:clipboardRetentionDays", "Keep History For").says("Pinned entries are kept forever").select(&self.setting("clipboardRetentionDays").to_string(), RETENTION.iter().copied()));
        rows.push(Line::row("act:clear-clipboard", "Clear History").says("Pinned entries are not deleted").confirm("Clear…", "Click Again to Clear"));
        if recording {
            // Finding entries by what they mean: the switch, the download
            // it needs, and how far reading the history has got.
            let (installed, on) = (flag(&self.meaning, "installed"), self.set("clipboardMeaning"));
            let (waiting, indexed) = (self.meaning["waiting"].as_u64().unwrap_or(0), self.meaning["indexed"].as_u64().unwrap_or(0));
            let state = if !installed {
                "A one-time download of about 135 MB, or 250 MB if text recognition is not installed. It takes about 350 MB of memory while you search, and none a few minutes after.".to_string()
            } else if !on {
                String::new()
            } else if let Some(problem) = self.meaning["problem"].as_str() {
                format!("It is not working at present: {problem}")
            } else if waiting > 0 {
                format!("Reading the history: {} to go; until then some are found by their words only.", count(waiting, "entry", "entries"))
            } else {
                format!("{} can be found this way.", count(indexed, "entry", "entries"))
            };
            let row = Line::row("set:clipboardMeaning", "Find by Meaning").says(format!("“服务器登录命令” or “server login command” finds an ssh line: entries are found by what they are about as well as by their words, in Chinese and English alike. A language model on this computer reads what you copy; nothing is sent anywhere. {state}"));
            rows.push(if self.meaning.is_null() || installed {
                row.switch(on)
            } else if self.meaning_installing || flag(&self.meaning, "installing") {
                row.text("Downloading…")
            } else {
                Line { id: "act:install-meaning".into(), ..row }.button("Download")
            });
        }
        page.card("Clipboard History", rows);
        if recording {
            let ignored = self.names("clipboardIgnoredApps");
            let mut rows: Vec<Line> = ignored.iter().map(|id| self.left_out(id, "ignore:remove:", "Not installed")).collect();
            // Those copied from first, then the rest of the installed.
            let offered = |seen: bool| -> Vec<(&str, &str)> { self.sources.iter().filter(|source| flag(source, "seen") == seen && !ignored.contains(&source["id"].as_str().unwrap_or_default())).filter_map(|source| Some((source["id"].as_str()?, source["name"].as_str()?))).collect() };
            rows.push(Line::row("ignore:add", "Never Record Copies From").says("What you copy while a window of one of these applications is in front stays out of the history. Password managers are always left out.").select("", std::iter::once(("", "Add Application…")).chain(offered(true)).chain(offered(false))));
            page.card("Ignored Applications", rows);
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
        let row = Line::row("set:snippetExpansion", "Expand Snippets as You Type");
        let mut rows = vec![if available {
            row.says("Typing a snippet's keyword in any application replaces it with the snippet's text. While this is on, Heyo watches what you type to find the keywords; none of it is stored. “Pause or Resume Snippet Expansion” in the root search switches it too.").switch(on)
        } else if self.capabilities["session"] == "wayland" {
            row.says("Not available on Wayland, where an application cannot see what is typed in another. Snippets can still be pasted from the root search.")
        } else {
            row.says("Not available in this desktop session. Snippets can still be pasted from the root search.")
        }];
        if available && on {
            let excluded = self.names("snippetExpansionIgnoredApps");
            rows.extend(excluded.iter().map(|id| self.left_out(id, "expand:remove:", "")));
            rows.push(Line::row("expand:add", "Never Expand In").says("Password managers are always left out. A password field in any other application cannot be told from an ordinary one. Nothing expands while an input method (Chinese, Japanese, …) is in use.").select("", std::iter::once(("", "Add Application…")).chain(self.sources.iter().filter(|source| !excluded.contains(&source["id"].as_str().unwrap_or_default())).filter_map(|source| Some((source["id"].as_str()?, source["name"].as_str()?))))));
        }
        page.card("Snippet Expansion", rows);
    }

    fn extension_page(&self, extension: &Value) -> Vec<Line> {
        let id = text(extension, "id");
        let (applications, builtin) = (id == APPLICATIONS, flag(extension, "builtin"));
        let disabled = self.disabled();
        let mut page = Page::default();
        let about = match (extension["development"].as_str(), extension["author"].as_str()) {
            (Some(directory), _) => format!("In development from {directory}"),
            (None, Some(author)) => format!("by {author}"),
            (None, None) if builtin && !applications => "Built into Heyo".into(),
            _ => String::new(),
        };
        page.0.push(Line { kind: "head", label: Self::extension_title(extension), description: about, icon: Self::icon(extension, &extension["icon"]), ..Line::default() });
        if let Some(description) = extension["description"].as_str().filter(|description| !description.is_empty()) {
            page.note(description);
        }
        page.card("Preferences", list(extension, "preferences").iter().map(|preference| Self::preference(extension, "", preference, &extension["values"]["extension"][text(preference, "name")])).collect());

        let query = if applications { self.find.to_lowercase() } else { String::new() };
        let mut commands: Vec<Line> = list(extension, "commands")
            .iter()
            .filter(|command| query.is_empty() || text(command, "title").to_lowercase().contains(&query))
            .map(|command| {
                let command_id = text(command, "id");
                let row = Line::row(format!("enable:{command_id}"), text(command, "title")).says(text(command, "description")).pictured(Self::icon(extension, &command["icon"]));
                let row = if command_id == CLIPBOARD_COMMAND { row } else { row.hotkey(&command_id, self.item_hotkeys.get(&command_id)) };
                if applications { row } else { row.switch(!disabled.contains(command_id.as_str())) }
            })
            .collect();
        if commands.is_empty() {
            commands.push(Line::row("", "No matches"));
        }
        page.card(if applications { "" } else { "Commands" }, commands);

        if id == BUILTIN {
            self.clipboard(&mut page);
            self.snippet_expansion(&mut page);
        }
        for command in list(extension, "commands") {
            let name = text(command, "name");
            page.card(&format!("{} Preferences", text(command, "title")), list(command, "preferences").iter().map(|preference| Self::preference(extension, &name, preference, &extension["values"]["commands"][&name][text(preference, "name")])).collect());
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
                    return Line::row(format!("activities:{name}"), label).says(if activities { "Declared; turn it off here" } else { "Live activities are off in General" }).switch(activities && !self.names("activitiesOff").contains(&name.as_str()));
                }
                let line = Line::row(format!("revoke:{name}{SEP}{permission}"), label).says(match row["state"].as_str() {
                    Some("allowed") => "Allowed by you",
                    Some("asks") => "Asks you the first time it is used",
                    _ => "Needs no asking",
                });
                if row["state"] == "allowed" { line.button("Revoke") } else { line }
            });
            page.card("Permissions", rows.collect());
            page.note("Permissions are about what Heyo hands over or does for an extension. They do not confine it: an extension is a program on this computer, and can read your files and start other programs by itself.");
        }
        if !builtin {
            let developed = extension["development"].is_string();
            page.card("", vec![Line::row(format!("uninstall:{id}"), if developed { "Stop Developing" } else { "Uninstall Extension" }).says(if developed { "Takes it out of Heyo; its sources are not touched" } else { "Its preferences are kept" }).confirm(if developed { "Remove" } else { "Uninstall" }, if developed { "Click Again to Remove" } else { "Click Again to Uninstall" })]);
        }
        page.done()
    }

    // --- What a control does -------------------------------------------------

    fn change(&mut self, name: &str, value: Value) -> Vec<Ask> {
        let changes = Map::from_iter([(name.to_string(), value)]);
        // Shown at once, before the host has answered.
        self.settings.extend(changes.clone());
        vec![ask("change_settings", json!({ "changes": changes }), Then::Settings)]
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
            "heyo:screenshot" => return Some("Capture Screenshot".into()),
            "heyo:screenshot-text" => return Some("Capture Text from Screen".into()),
            _ => {}
        }
        self.applications.iter().chain(self.extensions.iter().flat_map(|extension| list(extension, "commands"))).find(|item| item["id"] == id).map(|item| text(item, "title"))
    }

    /// Binds `accelerator` (none, when it is empty) to one of Heyo's own
    /// hotkeys or to an item.
    pub(crate) fn bind(&self, id: &str, accelerator: &str) -> Vec<Ask> {
        if let Some(name) = id.strip_prefix('@') {
            return vec![ask("set_hotkey", json!({ "name": name, "accelerator": accelerator }), Then::Hotkeys)];
        }
        let Some(title) = self.item_title(id) else { return Vec::new() };
        vec![ask("set_item_hotkey", json!({ "id": id, "title": title, "accelerator": accelerator }), Then::Bound(id.to_string(), accelerator.to_string()))]
    }

    /// The control `id` was used and now says `value`: what to ask of the
    /// host. What only changes what is shown is changed here.
    pub(crate) fn acted(&mut self, id: &str, value: &str) -> Vec<Ask> {
        let on = value == "true";
        if let Some(name) = id.strip_prefix("set:") {
            // A switch says true or false; everything else its words.
            let value = if value == "true" || value == "false" { json!(on) } else { json!(value) };
            let mut asks = self.change(name, value);
            if name == "localModelUrl" || name == "aiProvider" {
                asks.extend(self.entered());
            }
            return asks;
        }
        if let Some(name) = id.strip_prefix("num:") {
            return value.parse::<u64>().map(|number| self.change(name, json!(number))).unwrap_or_default();
        }
        if let Some(item) = id.strip_prefix("alias:") {
            let mut aliases = self.setting("aliases").as_object().cloned().unwrap_or_default();
            let alias = value.trim();
            if aliases.get(item).and_then(Value::as_str).unwrap_or_default() == alias {
                return Vec::new();
            }
            if alias.is_empty() {
                aliases.remove(item);
            } else {
                aliases.insert(item.to_string(), json!(alias));
            }
            return self.change("aliases", Value::Object(aliases));
        }
        if let Some(command) = id.strip_prefix("enable:") {
            let disabled = self.listed("disabledCommands", command, !on);
            return self.change("disabledCommands", disabled);
        }
        if let Some(extension) = id.strip_prefix("enable-all:open:") {
            let Some(extension) = self.extensions.iter().find(|other| other["id"] == extension) else { return Vec::new() };
            let ids: Vec<String> = list(extension, "commands").iter().map(|command| text(command, "id")).collect();
            let mut disabled: Vec<String> = self.names("disabledCommands").into_iter().filter(|other| !ids.iter().any(|id| id == other)).map(str::to_string).collect();
            if !on {
                disabled.extend(ids);
            }
            return self.change("disabledCommands", json!(disabled));
        }
        if let Some(group) = id.strip_prefix("open:") {
            if !self.expanded.remove(group) {
                self.expanded.insert(group.to_string());
            }
            return Vec::new();
        }
        if let Some(rest) = id.strip_prefix("pref:") {
            let &[extension, command, name, kind] = rest.split(SEP).collect::<Vec<_>>().as_slice() else { return Vec::new() };
            let Some(found) = self.extensions.iter().find(|other| other["name"] == extension) else { return Vec::new() };
            let mut values = found["values"].clone();
            let value = if kind == "checkbox" { json!(on) } else { json!(value) };
            if command.is_empty() {
                values["extension"][name] = value;
            } else {
                values["commands"][command][name] = value;
            }
            return vec![ask("set_extension_preferences", json!({ "extension": extension, "values": values }), Then::Reload)];
        }
        if let Some(rest) = id.strip_prefix("revoke:") {
            let Some((extension, permission)) = rest.split_once(SEP) else { return Vec::new() };
            return vec![ask("revoke_extension_permission", json!({ "extension": extension, "permission": permission }), Then::Permissions(self.page.clone())), ask("extension_permissions", json!({ "id": self.page }), Then::Permissions(self.page.clone()))];
        }
        if let Some(extension) = id.strip_prefix("activities:") {
            let off = self.listed("activitiesOff", extension, !on);
            return self.change("activitiesOff", off);
        }
        if let Some(extension) = id.strip_prefix("uninstall:") {
            return vec![ask("uninstall_extension", json!({ "id": extension }), Then::Uninstalled)];
        }
        match id {
            "words" => {
                let lines: Vec<&str> = value.lines().map(str::trim).filter(|line| !line.is_empty()).collect();
                if lines == self.names("dictationVocabulary") { Vec::new() } else { self.change("dictationVocabulary", json!(lines)) }
            }
            "ignore:add" | "expand:add" if value.is_empty() => Vec::new(),
            "ignore:add" => self.change("clipboardIgnoredApps", self.listed("clipboardIgnoredApps", value, true)),
            "expand:add" => self.change("snippetExpansionIgnoredApps", self.listed("snippetExpansionIgnoredApps", value, true)),
            // The key has a command of its own, which is the only way it is set.
            "apikey" if value.trim().is_empty() => Vec::new(),
            "apikey" => vec![ask("set_anthropic_api_key", json!({ "key": value }), Then::Settings)],
            "more:apikey" => vec![ask("set_anthropic_api_key", json!({ "key": "" }), Then::Settings)],
            "act:install-shell-extension" => vec![ask("install_shell_extension", Value::Null, Then::Capabilities)],
            "act:install-dictation" => {
                self.progress = Some("Starting…".into());
                vec![ask("dictation_install", Value::Null, Then::Started("dictation"))]
            }
            "act:install-ocr" => {
                self.ocr_installing = true;
                vec![ask("ocr_install", Value::Null, Then::Started("ocr"))]
            }
            "act:install-meaning" => {
                self.meaning_installing = true;
                vec![ask("clipboard_meaning_install", Value::Null, Then::Started("meaning"))]
            }
            "act:open-extensions" => vec![ask("open_extensions_folder", Value::Null, Then::Nothing)],
            "act:clear-clipboard" => vec![ask("clear_clipboard_history", Value::Null, Then::Nothing)],
            "act:copy-diagnostics" => vec![ask("copy_diagnostics", Value::Null, Then::Copied)],
            "act:update" => match self.update.clone() {
                Update::Newer(version) => {
                    self.update = Update::Installing(version);
                    vec![ask("install_update", Value::Null, Then::Installed)]
                }
                Update::Unknown | Update::Newest => {
                    self.update = Update::Checking;
                    vec![ask("check_for_update", Value::Null, Then::Checked)]
                }
                Update::Checking | Update::Installing(_) => Vec::new(),
            },
            _ => {
                if let Some(app) = id.strip_prefix("ignore:remove:") {
                    return self.change("clipboardIgnoredApps", self.listed("clipboardIgnoredApps", app, false));
                }
                if let Some(app) = id.strip_prefix("expand:remove:") {
                    return self.change("snippetExpansionIgnoredApps", self.listed("snippetExpansionIgnoredApps", app, false));
                }
                Vec::new()
            }
        }
    }

    // --- What is asked without a control -------------------------------------

    /// Everything about the extensions and the applications, read again.
    fn reload() -> Vec<Ask> {
        vec![ask("list_extensions", Value::Null, Then::Extensions), ask("list_applications", Value::Null, Then::Applications), ask("item_hotkeys", Value::Null, Then::ItemHotkeys)]
    }

    /// What is read when the window is shown: it may have changed while
    /// the window was away.
    pub(crate) fn opened(&self) -> Vec<Ask> {
        let mut asks = vec![ask("get_settings", Value::Null, Then::Settings), ask("get_hotkeys", Value::Null, Then::Hotkeys), ask("get_capabilities", Value::Null, Then::Capabilities)];
        asks.extend(Self::reload());
        asks.extend(self.entered());
        asks
    }

    /// What the page that is shown needs to know besides.
    pub(crate) fn entered(&self) -> Vec<Ask> {
        match self.page.as_str() {
            "dictation" => vec![ask("dictation_status", Value::Null, Then::Dictation)],
            "screenshots" => vec![ask("ocr_status", Value::Null, Then::Ocr)],
            "advanced" if self.chosen("aiProvider") == "local" => {
                let url = self.chosen("localModelUrl");
                vec![ask("local_models", json!({ "url": url }), Then::Models(url))]
            }
            BUILTIN => vec![ask("clipboard_sources", Value::Null, Then::Sources), ask("clipboard_meaning_status", Value::Null, Then::Meaning)],
            page if self.extensions.iter().any(|extension| extension["id"] == page && !flag(extension, "builtin")) => vec![ask("extension_permissions", json!({ "id": page }), Then::Permissions(page.to_string()))],
            _ => Vec::new(),
        }
    }

    /// Asked again every two seconds while the history is being read for
    /// finding by meaning, so that the page says how much is left.
    pub(crate) fn poll(&self) -> Vec<Ask> {
        let reading = self.page == BUILTIN && self.set("clipboardMeaning") && flag(&self.meaning, "installed") && self.meaning["waiting"].as_u64().unwrap_or(0) > 0;
        if reading { vec![ask("clipboard_meaning_status", Value::Null, Then::Meaning)] } else { Vec::new() }
    }

    /// Another page is shown.
    pub(crate) fn show(&mut self, page: &str) -> Vec<Ask> {
        if self.page == page {
            return Vec::new();
        }
        self.page = page.to_string();
        self.find.clear();
        self.entered()
    }

    /// The host answered: what is known changes, and more may be asked.
    pub(crate) fn answered(&mut self, then: Then, answer: Result<Value, String>) -> Vec<Ask> {
        let answer = match answer {
            Ok(answer) => answer,
            Err(message) => {
                match then {
                    Then::Models(url) => {
                        self.models = Some((url, Err(message)));
                        return Vec::new();
                    }
                    Then::Started("dictation") => self.progress = None,
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
                return if then == Then::Settings { vec![ask("get_settings", Value::Null, Then::Reread)] } else { Vec::new() };
            }
        };
        if !matches!(then, Then::Started(_) | Then::Models(_) | Then::Reread) {
            self.error.clear();
        }
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
            Then::Models(url) => self.models = Some((url, Ok(answer.as_array().map(|models| models.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default()))),
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
            event::SETTINGS_CHANGED => vec![ask("get_settings", Value::Null, Then::Settings)],
            event::INDEX_READY => Self::reload(),
            event::DICTATION_INSTALL => {
                if over {
                    self.progress = None;
                    return vec![ask("dictation_status", Value::Null, Then::Dictation)];
                }
                let percent = match (told["received"].as_f64(), told["total"].as_f64()) {
                    (received, Some(total)) if total > 0.0 => format!(" {}%", (received.unwrap_or(0.0) / total * 100.0).round()),
                    _ => "…".into(),
                };
                self.progress = Some(format!("{}{percent}", text(told, "step")));
                Vec::new()
            }
            event::OCR_INSTALL => {
                self.ocr_installing = !over;
                if over { vec![ask("ocr_status", Value::Null, Then::Ocr)] } else { Vec::new() }
            }
            event::CLIPBOARD_MEANING_INSTALL => {
                self.meaning_installing = !over;
                if over { vec![ask("clipboard_meaning_status", Value::Null, Then::Meaning)] } else { Vec::new() }
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

    /// The one thing asked of the host, as command and arguments.
    fn asked(asks: &[Ask]) -> (&'static str, Value) {
        assert_eq!(asks.len(), 1, "{asks:?}");
        (asks[0].command, asks[0].args.clone())
    }

    #[test]
    fn general_says_what_the_settings_are() {
        let mut model = told(json!({ "launchAtLogin": true, "appearance": "dark", "compactMode": false, "hideOnBlur": true, "keepCommand": false, "liveActivities": true }), json!({ "extension": "none" }));
        model.hotkeys = json!({ "supported": true, "toggle": "<Super>space", "clipboard": null, "dictation": null });
        let lines = model.lines();
        assert!(row(&lines, "Open at Login").on && row(&lines, "Hide When Focus Is Lost").on && row(&lines, "Show What Is in Progress").on);
        assert!(!row(&lines, "Compact Mode").on && !row(&lines, "Keep the Running Command").on);
        assert_eq!((row(&lines, "Theme").value.as_str(), titles(row(&lines, "Theme"))), ("dark", vec!["System", "Light", "Dark"]));
        assert_eq!((row(&lines, "Heyo Hotkey").hotkey.as_str(), row(&lines, "Heyo Hotkey").keys.clone()), ("@toggle", vec!["Super".to_string(), "Space".to_string()]));
        assert!(!has(&lines, "GNOME Shell Extension"));
        // The cards: the first row of each is its first, the last its last.
        assert!(row(&lines, "Open at Login").first && row(&lines, "Heyo Hotkey").last && row(&lines, "Theme").first && row(&lines, "Theme").last);
        assert_eq!(lines.iter().filter(|line| line.kind == "heading").map(|line| line.label.as_str()).collect::<Vec<_>>(), ["Appearance", "Window", "Live Activities"]);
    }

    #[test]
    fn a_switch_is_shown_at_once_and_asked_of_the_host() {
        let mut model = told(json!({ "compactMode": false }), json!({}));
        assert_eq!(asked(&model.acted("set:compactMode", "true")), ("change_settings", json!({ "changes": { "compactMode": true } })));
        assert!(row(&model.lines(), "Compact Mode").on);
        assert_eq!(asked(&model.acted("set:appearance", "light")), ("change_settings", json!({ "changes": { "appearance": "light" } })));
        // The host's answer is what is then so; its refusal is said, and
        // the settings are read again.
        assert!(model.answered(Then::Settings, Ok(json!({ "compactMode": false, "appearance": "dark" }))).is_empty());
        assert!(!row(&model.lines(), "Compact Mode").on);
        assert_eq!(asked(&model.answered(Then::Settings, Err("no".into()))).0, "get_settings");
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
        assert_eq!(asked(&model.bind("@toggle", "<Super>space")), ("set_hotkey", json!({ "name": "toggle", "accelerator": "<Super>space" })));
        assert_eq!(asked(&model.bind("app:files.desktop", "<Super>e")), ("set_item_hotkey", json!({ "id": "app:files.desktop", "title": "Files", "accelerator": "<Super>e" })));
        assert_eq!(asked(&model.bind("heyo:screenshot", "")).1["title"], "Capture Screenshot");
        assert!(model.bind("app:gone.desktop", "<Super>g").is_empty());
        model.answered(Then::Bound("app:files.desktop".into(), "<Super>e".into()), Ok(Value::Null));
        assert_eq!(model.item_hotkeys["app:files.desktop"], "<Super>e");
        model.answered(Then::Bound("app:files.desktop".into(), String::new()), Ok(Value::Null));
        assert!(model.item_hotkeys.is_empty());
    }

    #[test]
    fn dictation_says_what_this_session_can_do() {
        let mut model = told(
            json!({ "dictationHoldKey": "", "dictationLive": true, "dictationSource": "", "dictationLanguage": "zh", "dictationVocabulary": ["TypeScript", "杰森 -> JSON"], "dictationPolish": "clean", "aiProvider": "claude", "anthropicApiKeySet": false }),
            json!({ "session": "x11", "holdKey": "full", "holdKeys": [{ "value": "Control_R", "title": "Right Ctrl" }], "dictationWords": ["composed", "rewritten", "pasted"] }),
        );
        model.page = "dictation".into();
        model.dictation = json!({ "installed": false, "installing": false, "microphones": [{ "name": "alsa_input.usb", "description": "USB Microphone" }] });
        let lines = model.lines();
        assert_eq!((row(&lines, "Speech Recognition").control, row(&lines, "Speech Recognition").value.as_str()), ("button", "Download"));
        assert_eq!(titles(row(&lines, "Hold-to-Talk Key")), ["None", "Right Ctrl"]);
        assert!(row(&lines, "Hold-to-Talk Key").description.starts_with("A single key to hold"));
        let live = row(&lines, "Type as You Speak");
        assert!(live.control == "switch" && live.on && live.description.contains("through the input method") && live.description.contains("corrected where they stand"));
        assert_eq!(titles(row(&lines, "Microphone")), ["System Default", "USB Microphone"]);
        assert!(!row(&lines, "Microphone").danger);
        assert_eq!(row(&lines, "Language").value, "zh");
        assert_eq!(lines.iter().find(|line| line.kind == "area").unwrap().value, "TypeScript\n杰森 -> JSON");
        assert!(row(&lines, "Mode").value == "clean" && row(&lines, "Mode").description.contains("filler words"));
        assert!(has(&lines, "Claude API Key"));
        assert_eq!(model.entered()[0].command, "dictation_status");
    }

    #[test]
    fn dictation_explains_what_a_session_cannot_do() {
        let mut model = told(json!({ "dictationHoldKey": "Control_R", "dictationPolish": "nonsense" }), json!({ "session": "wayland", "holdKey": "none", "holdKeys": [], "dictationWords": ["pasted"] }));
        model.page = "dictation".into();
        model.dictation = json!({ "installed": true, "installing": false, "microphones": [] });
        let lines = model.lines();
        assert_eq!(row(&lines, "Speech Recognition").control, "badge");
        assert!(row(&lines, "Microphone").danger);
        assert!(row(&lines, "Hold-to-Talk Key").control.is_empty() && row(&lines, "Hold-to-Talk Key").description.contains("cannot be watched"));
        assert!(row(&lines, "Type as You Speak").control.is_empty() && row(&lines, "Type as You Speak").description.starts_with("Not available on Wayland"));
        // A mode it does not know is shown as none.
        assert_eq!(row(&lines, "Mode").value, "off");
        assert!(!has(&lines, "Claude API Key"));
    }

    #[test]
    fn a_download_is_followed() {
        let mut model = told(json!({}), json!({}));
        model.page = "dictation".into();
        use crate::wire::event;
        assert_eq!(asked(&model.acted("act:install-dictation", "")).0, "dictation_install");
        assert_eq!(row(&model.lines(), "Speech Recognition").value, "Starting…");
        assert!(model.heard(event::DICTATION_INSTALL, &json!({ "type": "progress", "step": "Downloading model", "received": 135, "total": 270 })).is_empty());
        assert_eq!(row(&model.lines(), "Speech Recognition").value, "Downloading model 50%");
        model.heard(event::DICTATION_INSTALL, &json!({ "type": "progress", "step": "Unpacking" }));
        assert_eq!(row(&model.lines(), "Speech Recognition").value, "Unpacking…");
        assert_eq!(asked(&model.heard(event::DICTATION_INSTALL, &json!({ "type": "error", "message": "no space left" }))).0, "dictation_status");
        assert_eq!((row(&model.lines(), "Speech Recognition").control, model.error.as_str()), ("button", "no space left"));
        // It could not even be started.
        model.acted("act:install-dictation", "");
        model.answered(Then::Started("dictation"), Err("no Node.js".into()));
        assert_eq!((row(&model.lines(), "Speech Recognition").control, model.error.as_str()), ("button", "no Node.js"));
    }

    #[test]
    fn the_vocabulary_is_saved_as_its_lines() {
        let mut model = told(json!({ "dictationVocabulary": ["TypeScript", "jason -> JSON"] }), json!({}));
        assert!(model.acted("words", "  TypeScript \n\n jason -> JSON\n").is_empty());
        assert_eq!(asked(&model.acted("words", "TypeScript\n杰森 -> JSON\n\n")).1, json!({ "changes": { "dictationVocabulary": ["TypeScript", "杰森 -> JSON"] } }));
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
        assert_eq!(asked(&model.acted("act:install-ocr", "")).0, "ocr_install");
        assert_eq!(row(&model.lines(), "Recognition Engine").value, "Downloading…");
        assert_eq!(asked(&model.heard(crate::wire::event::OCR_INSTALL, &json!({ "type": "installed" }))).0, "ocr_status");
        model.answered(Then::Ocr, Ok(json!({ "installed": true, "installing": false })));
        assert_eq!(row(&model.lines(), "Recognition Engine").control, "badge");
    }

    #[test]
    fn an_address_is_this_computer_only_when_it_is_loopback() {
        for local in ["http://127.0.0.1:8000/v1", "http://localhost:1234", "https://LOCALHOST./v1", "http://api.localhost", "http://127.1/", "http://0x7f.0.0.1", "http://2130706433", "http://[::1]:8080", "http://[::ffff:127.0.0.1]"] {
            assert_eq!(address(local), Address::ThisComputer, "{local}");
        }
        assert_eq!(address("http://192.168.1.20:8000/v1"), Address::Elsewhere { host: "192.168.1.20:8000".into(), encrypted: false });
        assert_eq!(address(" https://models.example.com/v1 "), Address::Elsewhere { host: "models.example.com".into(), encrypted: true });
        assert_eq!(address("http://localhost.example.com"), Address::Elsewhere { host: "localhost.example.com".into(), encrypted: false });
        for unknown in ["", "localhost:8000", "ftp://127.0.0.1", "file:///etc/passwd", "not a url"] {
            assert_eq!(address(unknown), Address::Unknown, "{unknown}");
        }
    }

    /// The runtime has the same rule, for the same warning where it asks a
    /// model (`src/modelAddress.ts`): what its tests say of an address,
    /// this says too.
    #[test]
    fn an_address_is_judged_as_the_runtime_judges_it() {
        let theirs = include_str!("../../../src/modelAddress.test.ts");
        let local = theirs.split_once("for (const address of [").and_then(|(_, rest)| rest.split_once("]) {")).expect("the loopback addresses are no longer listed in src/modelAddress.test.ts").0;
        let local: Vec<&str> = local.lines().filter_map(|line| line.trim().strip_prefix('"')?.strip_suffix("\",")).collect();
        assert!(local.len() > 10, "{local:?}");
        for address_ in local {
            assert_eq!(address(address_), Address::ThisComputer, "{address_}");
        }
        let mut elsewhere = 0;
        for line in theirs.lines() {
            let Some(rest) = line.trim().strip_prefix("expect(classifyModelAddress(\"") else { continue };
            let Some((url, rest)) = rest.split_once("\")).toEqual(elsewhere(\"") else { continue };
            let Some((host, encrypted)) = rest.trim_end_matches(");").trim_end_matches(')').split_once("\", ") else { continue };
            assert_eq!(address(url), Address::Elsewhere { host: host.to_string(), encrypted: encrypted == "true" }, "{url}");
            elsewhere += 1;
        }
        assert!(elsewhere >= 7, "only {elsewhere} addresses elsewhere were read out of src/modelAddress.test.ts");
    }

    #[test]
    fn advanced_says_where_text_goes_and_keeps_the_key_to_its_command() {
        let mut model = told(json!({ "nodeVersion": { "Err": "node was not found" }, "nodeOverridden": true, "nodePath": "", "extensionsDir": "/data/extensions", "settingsPath": "/config/settings.json", "renderer": "auto", "aiProvider": "claude", "anthropicApiKeySet": true, "localModelUrl": "http://10.0.0.5:8000/v1", "localModelName": "" }), json!({}));
        model.page = "advanced".into();
        let lines = model.lines();
        assert!(row(&lines, "Node.js").danger && row(&lines, "Node.js").description == "node was not found HEYO_NODE overrides this setting. Extensions need Node.js 22 or newer.");
        assert_eq!((row(&lines, "API Key").control, row(&lines, "API Key").more.as_str(), row(&lines, "API Key").value.as_str()), ("secret", "Remove", ""));
        assert!(!has(&lines, "Server"));
        assert_eq!(asked(&model.acted("apikey", "sk-ant-1")), ("set_anthropic_api_key", json!({ "key": "sk-ant-1" })));
        assert!(model.acted("apikey", "  ").is_empty());
        assert_eq!(asked(&model.acted("more:apikey", "")).1, json!({ "key": "" }));
        // A local model: its server is asked, and what is not this computer is a warning.
        let asks = model.acted("set:aiProvider", "local");
        assert_eq!(asks.iter().map(|ask| ask.command).collect::<Vec<_>>(), ["change_settings", "local_models"]);
        let server = row(&model.lines(), "Server").clone();
        assert!(server.danger && server.description.starts_with("Checking…") && server.description.contains("sent to 10.0.0.5:8000 over plain http"));
        model.answered(Then::Models("http://10.0.0.5:8000/v1".into()), Ok(json!(["qwen", "llama"])));
        assert!(row(&model.lines(), "Server").description.starts_with("Connected: qwen, llama This address is not this computer"));
        // An answer for an address that is no longer the one set is not shown.
        model.settings.insert("localModelUrl".into(), json!("http://127.0.0.1:1/v1"));
        let server = row(&model.lines(), "Server").clone();
        assert!(!server.danger && server.description == "Checking… Text stays on this computer.");
        model.answered(Then::Models("http://127.0.0.1:1/v1".into()), Err("connection refused".into()));
        assert!(row(&model.lines(), "Server").description.starts_with("Not reachable: connection refused."));
        assert!(model.error.is_empty(), "a server that does not answer is said on its row");
    }

    #[test]
    fn about_says_the_version_and_what_updating_is_doing() {
        let mut model = told(json!({ "version": "0.1.0", "canUpdate": false }), json!({}));
        model.page = "about".into();
        let about = |model: &Model| model.lines()[0].clone();
        assert_eq!((about(&model).value.as_str(), about(&model).more.as_str()), ("0.1.0", ""));
        model.settings.insert("canUpdate".into(), json!(true));
        assert_eq!(about(&model).more, "Check for Updates");
        assert_eq!(asked(&model.acted("act:update", "")).0, "check_for_update");
        assert!(about(&model).busy && model.acted("act:update", "").is_empty());
        model.answered(Then::Checked, Ok(Value::Null));
        assert_eq!(about(&model).more, "Heyo Is Up to Date");
        model.acted("act:update", "");
        model.answered(Then::Checked, Ok(json!("0.2.0")));
        assert_eq!(about(&model).more, "Install Heyo 0.2.0 and Restart");
        assert_eq!(asked(&model.acted("act:update", "")).0, "install_update");
        model.answered(Then::Installed, Err("no network".into()));
        assert_eq!((about(&model).more.as_str(), model.error.as_str()), ("Install Heyo 0.2.0 and Restart", "no network"));
        model.answered(Then::Copied, Ok(Value::Null));
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
        assert_eq!(entries.iter().map(|entry| entry.title.as_str()).collect::<Vec<_>>(), ["General", "Shortcuts", "Dictation", "Screenshots", "Advanced", "About", "Applications", "Heyo Commands", "alpha Notes", "Zeta Tools"]);
        // A page's symbol, a symbol the host named, a file of the extension's own.
        assert_eq!((entries[0].icon.as_deref(), entries[7].icon.as_deref(), entries[9].icon.as_deref()), (Some("cog-16"), Some("store-16"), Some("/data/zeta/assets/icon.png")));
        assert!(entries[9].extension && !entries[0].extension);
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
        assert_eq!(asked(&model.show("/data/zeta")), ("extension_permissions", json!({ "id": "/data/zeta" })));
        model.answered(Then::Permissions("/data/zeta".into()), Ok(json!([{ "permission": "windows", "allows": "see and move windows", "state": "allowed" }, { "permission": "ai", "allows": "ask the model", "state": "asks" }, { "permission": "activity", "allows": "show live activities", "state": "free" }])));
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
        assert_eq!(asked(&model.acted("enable:cmd:zeta:two", "true")).1, json!({ "changes": { "disabledCommands": [] } }));
        // Permissions: what was allowed can be taken back; activities have a switch.
        let lines = model.lines();
        assert_eq!((row(&lines, "See and move windows").control, row(&lines, "See and move windows").description.as_str()), ("button", "Allowed by you"));
        assert_eq!((row(&lines, "Ask the model").control, row(&lines, "Show live activities").control, row(&lines, "Show live activities").on), ("", "switch", true));
        assert_eq!(model.acted(&row(&lines, "See and move windows").id, "")[0], ask("revoke_extension_permission", json!({ "extension": "zeta", "permission": "windows" }), Then::Permissions("/data/zeta".into())));
        assert_eq!(asked(&model.acted("activities:zeta", "false")).1, json!({ "changes": { "activitiesOff": ["zeta"] } }));
        // A preference is saved among the others of the extension.
        let saved = asked(&model.acted(&sound.id, "false"));
        assert_eq!(saved, ("set_extension_preferences", json!({ "extension": "zeta", "values": { "extension": { "mode": "careful", "token": "tok_1" }, "commands": { "one": { "loud": false } } } })));
        assert_eq!(asked(&model.acted(&row(&lines, "Token *").id, "secret")).1["values"]["extension"], json!({ "mode": "careful", "token": "secret" }));
        // Removal, after which the window is elsewhere.
        assert_eq!((row(&lines, "Uninstall Extension").control, row(&lines, "Uninstall Extension").placeholder.as_str()), ("confirm", "Click Again to Uninstall"));
        assert_eq!(asked(&model.acted("uninstall:/data/zeta", "")), ("uninstall_extension", json!({ "id": "/data/zeta" })));
        assert_eq!(model.answered(Then::Uninstalled, Ok(Value::Null)).len(), 3);
        assert_eq!(model.page, "general");
        model.page = "/src/alpha".into();
        assert!(has(&model.lines(), "Stop Developing") && model.lines()[0].description == "In development from /src/alpha");
    }

    #[test]
    fn heyos_commands_have_the_clipboard_and_snippets() {
        let mut model = with_extensions();
        assert_eq!(model.show(BUILTIN).iter().map(|ask| ask.command).collect::<Vec<_>>(), ["clipboard_sources", "clipboard_meaning_status"]);
        model.meaning = json!({ "installed": true, "installing": false, "indexed": 1200, "waiting": 3400, "problem": null });
        model.settings.insert("clipboardMeaning".into(), json!(true));
        let lines = model.lines();
        assert_eq!((lines[0].description.as_str(), model.title().as_str()), ("Built into Heyo", "Heyo Commands"));
        // The clipboard history's hotkey is Heyo's own, on a row of its own.
        assert!(row(&lines, "Clipboard History").hotkey.is_empty() && row(&lines, "Dictate").hotkey == "cmd:@heyo:dictate");
        assert_eq!((row(&lines, "Hotkey").hotkey.as_str(), row(&lines, "Hotkey").keys.clone()), ("@clipboard", vec!["Super".to_string(), "V".to_string()]));
        assert_eq!((row(&lines, "Keep History For").value.as_str(), titles(row(&lines, "Keep History For"))[3]), ("90", "3 Months"));
        assert_eq!(asked(&model.acted("num:clipboardRetentionDays", "7")).1, json!({ "changes": { "clipboardRetentionDays": 7 } }));
        assert_eq!(row(&lines, "Clear History").control, "confirm");
        assert!(row(&lines, "Find by Meaning").description.ends_with("Reading the history: 3,400 entries to go; until then some are found by their words only."));
        assert_eq!(model.poll().len(), 1);
        // Left out of the history: by name where the application is known.
        assert_eq!((row(&lines, "KeePassXC").icon.as_deref(), row(&lines, "gone.desktop").description.as_str(), row(&lines, "gone.desktop").icon.as_deref()), (Some("/icons/keepassxc.png"), "Not installed", Some("app-window-16")));
        assert_eq!(titles(row(&lines, "Never Record Copies From")), ["Add Application…", "Firefox", "Files"]);
        assert_eq!(asked(&model.acted("ignore:add", "firefox.desktop")).1, json!({ "changes": { "clipboardIgnoredApps": ["org.keepassxc.KeePassXC.desktop", "gone.desktop", "firefox.desktop"] } }));
        assert_eq!(asked(&model.acted("ignore:remove:gone.desktop", "")).1["changes"]["clipboardIgnoredApps"], json!(["org.keepassxc.KeePassXC.desktop", "firefox.desktop"]));
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
        assert_eq!(asked(&model.acted("act:install-meaning", "")).0, "clipboard_meaning_install");
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
        assert_eq!(asked(&model.acted(&format!("enable-all:{}", zeta.id), "false")).1["changes"]["disabledCommands"], json!(["cmd:zeta:one", "cmd:zeta:two"]));
        assert_eq!(asked(&model.acted("alias:app:files.desktop", " fm ")).1, json!({ "changes": { "aliases": { "app:firefox.desktop": "ff", "app:files.desktop": "fm" } } }));
        assert!(model.acted("alias:app:files.desktop", "fm").is_empty());
        assert_eq!(asked(&model.acted("alias:app:firefox.desktop", "")).1["changes"]["aliases"], json!({ "app:files.desktop": "fm" }));
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
        assert_eq!(asked(&model.heard(event::SETTINGS_CHANGED, &Value::Null)).0, "get_settings");
        assert_eq!(model.heard(event::INDEX_READY, &Value::Null).iter().map(|ask| ask.command).collect::<Vec<_>>(), ["list_extensions", "list_applications", "item_hotkeys"]);
        assert_eq!(model.opened().iter().map(|ask| ask.command).collect::<Vec<_>>(), ["get_settings", "get_hotkeys", "get_capabilities", "list_extensions", "list_applications", "item_hotkeys"]);
        // A preference that was saved: the extensions are read again.
        assert_eq!(model.answered(Then::Reload, Ok(Value::Null)).len(), 3);
        model.answered(Then::Extensions, Ok(json!([])));
        assert_eq!(model.nav().0.len(), PAGES.len() + 1);
    }
}

//! The root search: applications, commands, quicklinks and snippets, ranked
//! by fuzzy match, usage, aliases and favorites.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::LazyLock;

use serde::Serialize;

use crate::apps::App;
use crate::extensions::{Argument, Command};
use crate::library::{Quicklink, Snippet};
use crate::search;
use crate::settings::Settings;
use crate::usage::UsageStore;

const MAX_RESULTS: usize = 50;
const SUGGESTIONS: usize = 8;

/// Root search item that opens the settings window.
pub const SETTINGS_ID: &str = "heyo:settings";
pub const SCREENSHOT_ID: &str = "heyo:screenshot";
pub const SCREENSHOT_TEXT_ID: &str = "heyo:screenshot-text";

pub const KEEP_AWAKE_ID: &str = "heyo:keep-awake";
pub const PASTE_PLAIN_ID: &str = "heyo:paste-plain";
pub const CLIPBOARD_PAUSE_ID: &str = "heyo:clipboard-pause";
pub const SNIPPET_EXPANSION_ID: &str = "heyo:snippet-expansion";
/// Prefix of the window layout commands; the rest is a `windows::LAYOUTS` id.
pub const WINDOW_PREFIX: &str = "heyo:window-";

/// Something Heyo does itself, outside the extension runtime.
pub struct Native {
    pub id: String,
    pub title: String,
    pub icon: &'static str,
    pub description: String,
    /// Shown as the subtitle, e.g. "Window Management".
    pub group: &'static str,
    keywords: Vec<String>,
}

pub static NATIVE: LazyLock<Vec<Native>> = LazyLock::new(|| {
    let native = |id: &str, title: &str, icon, group, description: &str, keywords: &[&str]| Native {
        id: id.into(),
        title: title.into(),
        icon,
        group,
        description: description.into(),
        keywords: keywords.iter().map(|k| k.to_string()).collect(),
    };
    let mut all = vec![
        native(SETTINGS_ID, "Heyo Settings", "cog-16", "Heyo", "Open Heyo's settings window.", &["preferences", "configuration", "node"]),
        native(
            SCREENSHOT_ID,
            "Capture Screenshot",
            "camera-16",
            "Heyo",
            "Select part of the screen, annotate it, then copy, save or pin it.",
            &["screen", "snip", "capture", "pin", "annotate"],
        ),
        native(
            SCREENSHOT_TEXT_ID,
            "Capture Text from Screen",
            "text-selection-16",
            "Heyo",
            "Select part of the screen and copy the text in it.",
            &["ocr", "extract", "recognize", "screen"],
        ),
        native(
            KEEP_AWAKE_ID,
            "Toggle Keep Awake",
            "mug-steam-16",
            "System",
            "Stop the computer from sleeping or dimming the screen until you turn it off again.",
            &["caffeine", "sleep", "idle", "inhibit", "amphetamine"],
        ),
    ];
    all.push(native(
        PASTE_PLAIN_ID,
        "Paste as Plain Text",
        "clipboard-16",
        "Heyo",
        "Paste what you copied without its formatting (fonts, colors, links). Give it a hotkey.",
        &["unformatted", "strip", "format", "clipboard", "text only"],
    ));
    all.push(native(
        CLIPBOARD_PAUSE_ID,
        "Pause or Resume Clipboard History",
        "clipboard-16",
        "Heyo",
        "Stop recording what you copy until you run this again.",
        &["private", "incognito", "stop", "record", "ignore"],
    ));
    all.push(native(
        SNIPPET_EXPANSION_ID,
        "Pause or Resume Snippet Expansion",
        "snippets-16",
        "Heyo",
        "Stop replacing the keywords of snippets as you type them, and stop following what is typed, until you run this again.",
        &["keyword", "typing", "text expander", "stop", "private"],
    ));
    all.extend(crate::windows::LAYOUTS.iter().map(|(id, title, _)| {
        native(
            &format!("{WINDOW_PREFIX}{id}"),
            title,
            "app-window-sidebar-left-16",
            "Window Management",
            "Move and resize the focused window.",
            &["window", "resize", "tile", "snap", "layout"],
        )
    }));
    all
});

enum Target {
    App(usize),
    Command(usize),
    Quicklink(usize),
    Snippet(usize),
    /// Index into [`NATIVE`].
    Native(usize),
}

/// What ranking and display depend on besides the index itself.
pub struct Context<'a> {
    pub settings: &'a Settings,
    pub usage: &'a UsageStore,
    /// Item id → hotkey.
    pub hotkeys: &'a HashMap<String, String>,
}

#[derive(Default)]
pub struct Index {
    pub apps: Vec<App>,
    pub commands: Vec<Command>,
    quicklinks: Vec<Quicklink>,
    snippets: Vec<Snippet>,
    /// Parallel to `targets`, sorted by title.
    entries: Vec<search::Entry>,
    targets: Vec<Target>,
}

#[derive(Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../shared/bindings/")]
pub struct Item {
    id: String,
    title: String,
    subtitle: Option<String>,
    /// A file path, URL, or Raycast icon name.
    icon: Option<String>,
    #[ts(type = "\"Application\" | \"Command\" | \"Quicklink\" | \"Snippet\"")]
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    alias: Option<String>,
    favorite: bool,
    /// Global hotkey (GTK accelerator) bound to this item.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    hotkey: Option<String>,
    /// Heading for the empty-query view ("Favorites", "Suggestions").
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string")]
    section: Option<&'static str>,
    /// Quicklink URL template.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    link: Option<String>,
    /// Snippet text.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    text: Option<String>,
    /// What a command asks for before it runs: fields in the search bar.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, as = "Option<Vec<Argument>>")]
    arguments: Vec<Argument>,
}

impl Index {
    pub fn new(apps: Vec<App>, commands: Vec<Command>, quicklinks: Vec<Quicklink>, snippets: Vec<Snippet>) -> Self {
        let entry = |title: &str, keywords: Vec<String>| search::Entry { title: title.to_owned(), keywords };
        let mut items: Vec<(search::Entry, Target)> = Vec::new();
        for (i, app) in apps.iter().enumerate() {
            let keywords = app.generic_name.iter().chain(&app.keywords).cloned().collect();
            items.push((entry(&app.name, keywords), Target::App(i)));
        }
        for (i, cmd) in commands.iter().enumerate() {
            items.push((entry(&cmd.title, cmd.keywords.clone()), Target::Command(i)));
        }
        for (i, link) in quicklinks.iter().enumerate() {
            items.push((entry(&link.name, vec!["quicklink".into()]), Target::Quicklink(i)));
        }
        for (i, snippet) in snippets.iter().enumerate() {
            let keywords = snippet.keyword.iter().cloned().chain(["snippet".into()]).collect();
            items.push((entry(&snippet.name, keywords), Target::Snippet(i)));
        }
        // Only what can work in this session.
        for (i, native) in NATIVE.iter().enumerate().filter(|(_, n)| crate::platform::item_available(&n.id)) {
            items.push((entry(&native.title, native.keywords.clone()), Target::Native(i)));
        }
        items.sort_by_cached_key(|(entry, _)| entry.title.to_lowercase());
        let (entries, targets) = items.into_iter().unzip();
        Index { apps, commands, quicklinks, snippets, entries, targets }
    }

    fn id(&self, target: &Target) -> String {
        match *target {
            Target::App(i) => format!("app:{}", self.apps[i].id),
            Target::Command(i) => self.commands[i].id(),
            Target::Quicklink(i) => format!("quicklink:{}", self.quicklinks[i].id),
            Target::Snippet(i) => format!("snippet:{}", self.snippets[i].id),
            Target::Native(i) => NATIVE[i].id.clone(),
        }
    }

    /// The alias set in the settings, else a snippet's keyword.
    fn alias(&self, target: &Target, id: &str, aliases: &BTreeMap<String, String>) -> Option<String> {
        aliases.get(id).cloned().or_else(|| match *target {
            Target::Snippet(i) => self.snippets[i].keyword.clone(),
            _ => None,
        })
    }

    fn item(&self, target: &Target, ctx: &Context, section: Option<&'static str>) -> Item {
        let settings = ctx.settings;
        let id = self.id(target);
        let hotkey = ctx.hotkeys.get(&id).cloned();
        let alias = self.alias(target, &id, &settings.aliases);
        let favorite = settings.favorites.contains(&id);
        let base = |title: &str, subtitle: Option<String>, icon: Option<String>, kind| Item {
            id: id.clone(),
            title: title.to_owned(),
            subtitle,
            icon,
            kind,
            alias: alias.clone(),
            favorite,
            hotkey: hotkey.clone(),
            section,
            link: None,
            text: None,
            arguments: Vec::new(),
        };
        match *target {
            Target::App(i) => {
                let app = &self.apps[i];
                base(
                    &app.name,
                    app.generic_name.clone().or_else(|| app.comment.clone()),
                    app.icon.as_ref().map(|p| p.to_string_lossy().into_owned()),
                    "Application",
                )
            }
            Target::Command(i) => {
                let cmd = &self.commands[i];
                Item {
                    arguments: cmd.arguments.clone(),
                    ..base(&cmd.title, cmd.subtitle.clone(), cmd.icon.clone(), "Command")
                }
            }
            Target::Quicklink(i) => {
                let link = &self.quicklinks[i];
                Item {
                    link: Some(link.link.clone()),
                    ..base(&link.name, Some(link.link.clone()), Some(link.icon()), "Quicklink")
                }
            }
            Target::Snippet(i) => {
                let snippet = &self.snippets[i];
                let preview = snippet.text.lines().next().unwrap_or_default().chars().take(60).collect();
                Item {
                    text: Some(snippet.text.clone()),
                    ..base(&snippet.name, Some(preview), Some("snippets-16".into()), "Snippet")
                }
            }
            Target::Native(i) => base(&NATIVE[i].title, Some(NATIVE[i].group.into()), Some(NATIVE[i].icon.into()), "Command"),
        }
    }

    pub fn search(&self, query: &str, ctx: &Context) -> Vec<Item> {
        let (settings, usage) = (ctx.settings, ctx.usage);
        let query = query.trim();
        if query.is_empty() {
            return self.home(ctx);
        }
        let lower = query.to_lowercase();
        // "gh heyo": the alias of a quicklink, or of a command that takes
        // arguments, followed by its (first) argument.
        let first_word = lower.split_once(char::is_whitespace).map(|(first, _)| first);
        let takes_argument = |target: &Target| match *target {
            Target::Quicklink(_) => true,
            Target::Command(i) => !self.commands[i].arguments.is_empty(),
            _ => false,
        };
        let mut scored: Vec<(f32, usize)> = Vec::new();
        let matched: std::collections::HashMap<usize, u32> = search::scores(&self.entries, query).into_iter().collect();
        for (i, target) in self.targets.iter().enumerate() {
            let id = self.id(target);
            // Extension commands are filtered when the index is built; Heyo's own here.
            if matches!(target, Target::Native(_)) && settings.disabled_commands.contains(&id) {
                continue;
            }
            let alias = self.alias(target, &id, &settings.aliases).map(|a| a.to_lowercase());
            let alias_bonus = match &alias {
                Some(a) if *a == lower => 10_000.0,
                Some(a) if first_word == Some(a.as_str()) && takes_argument(target) => 10_000.0,
                Some(a) if a.starts_with(&lower) => 500.0,
                _ => 0.0,
            };
            let Some(fuzzy) = matched.get(&i).map(|&s| s as f32).or((alias_bonus > 0.0).then_some(0.0)) else {
                continue;
            };
            let frecency = (usage.score(&id) * 25.0).min(80.0);
            let favorite = if settings.favorites.contains(&id) { 15.0 } else { 0.0 };
            scored.push((fuzzy + alias_bonus + frecency + favorite, i));
        }
        scored.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
        scored
            .into_iter()
            .take(MAX_RESULTS)
            .map(|(_, i)| self.item(&self.targets[i], ctx, None))
            .collect()
    }

    /// The empty-query view: favorites, then the most used items; or, before
    /// there are any, everything alphabetically.
    fn home(&self, ctx: &Context) -> Vec<Item> {
        let (settings, usage) = (ctx.settings, ctx.usage);
        let by_id: std::collections::HashMap<String, &Target> =
            self.targets.iter().map(|t| (self.id(t), t)).collect();
        let mut items: Vec<Item> = settings
            .favorites
            .iter()
            .filter_map(|id| by_id.get(id))
            .map(|t| self.item(t, ctx, Some("Favorites")))
            .collect();
        let favorites: HashSet<&String> = settings.favorites.iter().collect();
        let mut used: Vec<(f32, &Target)> = self
            .targets
            .iter()
            .map(|t| (self.id(t), t))
            .filter(|(id, _)| !favorites.contains(id))
            .map(|(id, t)| (usage.score(&id), t))
            .filter(|(score, _)| *score > 0.0)
            .collect();
        used.sort_by(|a, b| b.0.total_cmp(&a.0));
        items.extend(used.into_iter().take(SUGGESTIONS).map(|(_, t)| self.item(t, ctx, Some("Suggestions"))));
        if items.is_empty() {
            items = self.targets.iter().take(MAX_RESULTS).map(|t| self.item(t, ctx, None)).collect();
        }
        items
    }

    /// One item by id, e.g. for `heyo --open <id>`.
    pub fn get(&self, id: &str, ctx: &Context) -> Option<Item> {
        self.targets.iter().find(|t| self.id(t) == id).map(|t| self.item(t, ctx, None))
    }

    /// All applications, for the settings window's hotkey table.
    pub fn applications(&self, ctx: &Context) -> Vec<Item> {
        let mut apps: Vec<Item> = self
            .targets
            .iter()
            .filter(|t| matches!(t, Target::App(_)))
            .map(|t| self.item(t, ctx, None))
            .collect();
        apps.sort_by_cached_key(|a| a.title.to_lowercase());
        apps
    }

    pub fn quicklink_items(&self, ctx: &Context) -> Vec<Item> {
        self.targets.iter().filter(|t| matches!(t, Target::Quicklink(_))).map(|t| self.item(t, ctx, None)).collect()
    }

    pub fn app(&self, desktop_id: &str) -> Option<&App> {
        self.apps.iter().find(|a| a.id == desktop_id)
    }

    pub fn commands(&self) -> &[Command] {
        &self.commands
    }

    pub fn command(&self, id: &str) -> Option<&Command> {
        self.commands.iter().find(|c| c.id() == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command(name: &str, title: &str) -> Command {
        Command {
            extension: "/ext".into(),
            name: name.into(),
            title: title.into(),
            subtitle: None,
            keywords: Vec::new(),
            icon: None,
            mode: "view".into(),
            interval: None,
            arguments: Vec::new(),
            handles_activities: false,
        }
    }

    fn index() -> Index {
        let city = Argument { name: "city".into(), kind: "text".into(), placeholder: Some("City".into()), required: true, data: Vec::new() };
        let weather = Command { arguments: vec![city], ..command("weather", "Weather Forecast") };
        let commands = vec![command("notes", "Search Notes"), command("notion", "Search Notion"), command("timer", "Start Timer"), weather];
        let quicklinks = vec![Quicklink { id: "gh".into(), name: "GitHub Search".into(), link: "https://github.com/search?q={query}".into(), icon: None }];
        let snippets = vec![Snippet { id: "addr".into(), name: "Home Address".into(), text: "1 Main St\nSpringfield".into(), keyword: Some("addr".into()) }];
        Index::new(Vec::new(), commands, quicklinks, snippets)
    }

    /// The ids a query finds, best first.
    fn found(index: &Index, query: &str, settings: &Settings, usage: &UsageStore) -> Vec<String> {
        let hotkeys = HashMap::new();
        index.search(query, &Context { settings, usage, hotkeys: &hotkeys }).into_iter().map(|item| item.id).collect()
    }

    const NOTES: &str = "cmd:/ext:notes";
    const NOTION: &str = "cmd:/ext:notion";
    const TIMER: &str = "cmd:/ext:timer";
    const WEATHER: &str = "cmd:/ext:weather";

    fn position(ids: &[String], id: &str) -> usize {
        ids.iter().position(|i| i == id).unwrap_or_else(|| panic!("{id} was not found among {ids:?}"))
    }

    #[test]
    fn what_is_used_comes_before_what_matches_as_well() {
        let (index, settings) = (index(), Settings::default());
        let fresh = found(&index, "search not", &settings, &UsageStore::default());
        assert!(position(&fresh, NOTES) < position(&fresh, NOTION), "alphabetical when nothing tells them apart");
        let used = found(&index, "search not", &settings, &UsageStore::with(&[(NOTION, 5, 0.0)]));
        assert!(position(&used, NOTION) < position(&used, NOTES));
    }

    #[test]
    fn use_does_not_outweigh_a_far_better_match() {
        let (index, settings) = (index(), Settings::default());
        // "Start Timer" is the only thing called a timer, however much Notion is used.
        let ids = found(&index, "timer", &settings, &UsageStore::with(&[(NOTION, 10_000, 0.0)]));
        assert_eq!(ids.first().map(String::as_str), Some(TIMER));
    }

    #[test]
    fn a_favorite_is_preferred_among_equals() {
        let index = index();
        let settings = Settings { favorites: vec![NOTION.into()], ..Settings::default() };
        let ids = found(&index, "search not", &settings, &UsageStore::default());
        assert!(position(&ids, NOTION) < position(&ids, NOTES));
    }

    #[test]
    fn an_alias_typed_exactly_wins_over_everything() {
        let index = index();
        let settings = Settings { aliases: [(TIMER.to_owned(), "n".to_owned())].into(), ..Settings::default() };
        let usage = UsageStore::with(&[(NOTES, 10_000, 0.0)]);
        assert_eq!(found(&index, "n", &settings, &usage).first().map(String::as_str), Some(TIMER));
        // A snippet's keyword is its alias.
        assert_eq!(found(&index, "addr", &settings, &usage).first().map(String::as_str), Some("snippet:addr"));
    }

    #[test]
    fn a_quicklinks_alias_takes_the_rest_as_its_argument() {
        let index = index();
        let settings = Settings { aliases: [("quicklink:gh".to_owned(), "gh".to_owned())].into(), ..Settings::default() };
        let ids = found(&index, "gh heyo launcher", &settings, &UsageStore::default());
        assert_eq!(ids.first().map(String::as_str), Some("quicklink:gh"));
        // So does a command that takes arguments; the alias of one that takes
        // none must be the whole query.
        let settings = Settings { aliases: [(TIMER.to_owned(), "t".to_owned()), (WEATHER.to_owned(), "w".to_owned())].into(), ..Settings::default() };
        assert!(!found(&index, "t heyo launcher", &settings, &UsageStore::default()).contains(&TIMER.to_owned()));
        assert_eq!(found(&index, "w oslo", &settings, &UsageStore::default()).first().map(String::as_str), Some(WEATHER));
    }

    #[test]
    fn a_command_brings_the_arguments_it_asks_for() {
        let index = index();
        let hotkeys = HashMap::new();
        let ctx = Context { settings: &Settings::default(), usage: &UsageStore::default(), hotkeys: &hotkeys };
        let weather = serde_json::to_value(index.get(WEATHER, &ctx).unwrap()).unwrap();
        assert_eq!(weather["arguments"], serde_json::json!([{ "name": "city", "type": "text", "placeholder": "City", "required": true }]));
        // One that asks for nothing says nothing.
        assert!(serde_json::to_value(index.get(TIMER, &ctx).unwrap()).unwrap().get("arguments").is_none());
    }

    #[test]
    fn a_command_of_heyos_that_is_switched_off_is_not_found() {
        let index = index();
        let off = Settings { disabled_commands: vec![SCREENSHOT_ID.into()], ..Settings::default() };
        assert!(found(&index, "capture screenshot", &Settings::default(), &UsageStore::default()).contains(&SCREENSHOT_ID.to_owned()));
        assert!(!found(&index, "capture screenshot", &off, &UsageStore::default()).contains(&SCREENSHOT_ID.to_owned()));
    }

    #[test]
    fn with_nothing_typed_favorites_come_first_then_what_is_used_most() {
        let index = index();
        let settings = Settings { favorites: vec![TIMER.into()], ..Settings::default() };
        let usage = UsageStore::with(&[(NOTES, 2, 0.0), (NOTION, 9, 0.0), (TIMER, 1, 0.0)]);
        let hotkeys = HashMap::new();
        let home = index.search("", &Context { settings: &settings, usage: &usage, hotkeys: &hotkeys });
        let shown: Vec<(&str, Option<&str>)> = home.iter().map(|item| (item.id.as_str(), item.section)).collect();
        assert_eq!(shown, [(TIMER, Some("Favorites")), (NOTION, Some("Suggestions")), (NOTES, Some("Suggestions"))]);
    }

    #[test]
    fn before_anything_is_used_everything_is_listed() {
        let index = index();
        let hotkeys = HashMap::new();
        let home = index.search("", &Context { settings: &Settings::default(), usage: &UsageStore::default(), hotkeys: &hotkeys });
        assert!(home.len() > 5 && home.iter().all(|item| item.section.is_none()));
    }
}

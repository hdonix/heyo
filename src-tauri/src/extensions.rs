//! Discovers installed Raycast extensions and their commands.
//!
//! Extensions live in `$XDG_DATA_HOME/heyo/extensions/<name>/`, each a built
//! extension as downloaded from the Raycast Store (or produced by
//! `ray build`): a `package.json` manifest, one `<command>.js` per command and
//! an `assets/` directory. The Node runtime executes them.
//!
//! An extension being written is in the same form somewhere else: `heyo
//! dev` builds it into `development/<name>/` in the data directory and
//! names it in `development.json` (`runtime/src/dev/registry.ts`). Those are
//! listed after the installed ones and say so.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::i18n::{marked, t, tc, translate};

/// Extension name the runtime uses for Heyo's own commands.
pub const BUILTIN: &str = "@heyo";

#[derive(Debug, Clone)]
pub struct Command {
    /// `@heyo`, or the extension's directory; what the runtime's `launch` expects.
    pub extension: String,
    pub name: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub keywords: Vec<String>,
    /// A file path, or a Raycast icon name such as `box-16`.
    pub icon: Option<String>,
    /// `view`, `no-view` or `menu-bar`.
    pub mode: String,
    /// How often the command runs by itself, from the manifest's `interval`.
    pub interval: Option<std::time::Duration>,
    /// What the command asks for before it runs, from the manifest's `arguments`.
    pub arguments: Vec<Argument>,
    /// The command the host runs for the events of the extension's live
    /// activities (`"heyo": { "handles": "activity" }` on the command).
    pub handles_activities: bool,
}

/// One thing a command asks for before it runs. The root search shows each
/// as a field in the search bar.
#[derive(Debug, Clone, PartialEq, serde::Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../shared/bindings/")]
pub struct Argument {
    pub name: String,
    #[serde(rename = "type", default = "text_argument")]
    #[ts(type = "\"text\" | \"password\" | \"dropdown\"")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub placeholder: Option<String>,
    #[serde(default)]
    pub required: bool,
    /// The choices of a `dropdown`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, as = "Option<Vec<Choice>>")]
    pub data: Vec<Choice>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../shared/bindings/")]
pub struct Choice {
    pub title: String,
    pub value: String,
}

fn text_argument() -> String {
    "text".into()
}

/// A manifest interval such as `30s`, `10m`, `1h` or `1d`.
fn parse_interval(text: &str) -> Option<std::time::Duration> {
    let text = text.trim();
    let split = text.find(|c: char| !c.is_ascii_digit())?;
    let amount: u64 = text[..split].parse().ok()?;
    let seconds = match &text[split..] {
        "s" => 1,
        "m" => 60,
        "h" => 3600,
        "d" => 86_400,
        _ => return None,
    };
    // Raycast's minimum is 10 seconds.
    Some(std::time::Duration::from_secs((amount * seconds).max(10)))
}

impl Command {
    pub fn id(&self) -> String {
        format!("cmd:{}:{}", self.extension, self.name)
    }
}

#[derive(Deserialize)]
struct Manifest {
    title: String,
    icon: Option<String>,
    #[serde(default)]
    commands: Vec<CommandManifest>,
    #[serde(default)]
    heyo: Option<Heyo>,
}

/// The `heyo` field of a manifest: what an extension written for Heyo says
/// of itself (docs/extension-api.md, 3.2).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Heyo {
    /// The version of the API it was written against.
    #[serde(default = "first_version")]
    pub api_version: u32,
    /// What it may come to ask for (`permissions::Permission`, by name).
    #[serde(default)]
    pub permissions: Vec<String>,
    /// Where it runs (`linux`, `windows`); everywhere when left out.
    #[serde(default)]
    pub platforms: Option<Vec<String>>,
}

fn first_version() -> u32 {
    1
}

impl Heyo {
    /// Whether the extension is for a platform of that name.
    fn runs_on(&self, platform: &str) -> bool {
        self.platforms.as_ref().is_none_or(|platforms| platforms.iter().any(|p| p == platform))
    }
}

/// This platform, as manifests name it.
pub const PLATFORM: &str = if cfg!(windows) { "windows" } else { "linux" };

/// What the host needs to know of an extension that asks it for something.
#[derive(Debug, Clone, PartialEq)]
pub struct Declared {
    pub name: String,
    pub title: String,
    pub heyo: Option<Heyo>,
}

/// Reads that from the manifest of the extension in `dir`.
pub fn declared(dir: &Path) -> Option<Declared> {
    declared_in(&fs::read_to_string(dir.join("package.json")).ok()?)
}

fn declared_in(manifest: &str) -> Option<Declared> {
    let manifest: serde_json::Value = serde_json::from_str(manifest).ok()?;
    let name = manifest["name"].as_str()?.to_owned();
    Some(Declared {
        title: manifest["title"].as_str().unwrap_or(&name).to_owned(),
        // A `heyo` field that cannot be read declares nothing.
        heyo: manifest.get("heyo").map(|heyo| serde_json::from_value(heyo.clone()).unwrap_or_default()),
        name,
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommandManifest {
    name: String,
    title: String,
    subtitle: Option<String>,
    mode: String,
    icon: Option<String>,
    #[serde(default)]
    keywords: Vec<String>,
    #[serde(default)]
    disabled_by_default: bool,
    interval: Option<String>,
    #[serde(default)]
    arguments: Vec<Argument>,
    /// What the command does for Heyo beside being run: `handles`.
    #[serde(default)]
    heyo: Option<CommandHeyo>,
}

/// The `heyo` field of a command's manifest entry.
#[derive(Default, Deserialize)]
struct CommandHeyo {
    /// `activity`: run for the events of the extension's live activities.
    #[serde(default)]
    handles: Option<String>,
}

/// One of Heyo's own commands that the runtime runs like an extension's.
pub struct Builtin {
    pub name: &'static str,
    pub title: &'static str,
    pub icon: &'static str,
    /// False for the commands that act on the system and show nothing.
    pub view: bool,
    /// For the settings.
    pub description: &'static str,
    /// What else it is found by, apart by commas. Its translation is the
    /// words someone typing that language would look for it by; the
    /// English ones find it whatever the language.
    pub keywords: &'static str,
    /// One text argument typed in the search bar: its name and placeholder.
    pub argument: Option<(&'static str, &'static str)>,
    /// Run for the events of Heyo's own live activities (`activities.rs`).
    pub handles_activities: bool,
}

impl Builtin {
    fn mode(&self) -> &'static str {
        if self.view { "view" } else { "no-view" }
    }

    /// Its description in the language spoken. `{0}` in one is the
    /// placeholder a quicklink is written with, which no message may spell
    /// out: braces are a message's own placeholders.
    fn description(&self) -> String {
        translate(self.description).replace("{0}", "{query}")
    }
}

/// A command's keywords, as a table has them (`Builtin::keywords`): the
/// English ones, and after them the ones of the language spoken.
pub(crate) fn keywords(marked: &'static str) -> Vec<String> {
    let english = marked.split(", ");
    let spoken = Some(translate(marked)).filter(|spoken| *spoken != marked).into_iter().flat_map(|spoken| spoken.split(", "));
    english.chain(spoken).map(str::to_owned).collect()
}

/// The subtitle of the commands that act on the system.
pub(crate) fn system_group() -> &'static str {
    tc("command group", "System")
}

/// Heyo's built-in commands. This is the one list of them: the runtime's
/// copy (`shared/bindings/builtins.ts`) is written from it by `cargo test`,
/// and the runtime will not build without an implementation for each.
pub const BUILTINS: &[Builtin] = &[
    Builtin {
        name: "store", title: marked("Search Raycast Store"), icon: "store-16", view: true,
        description: marked("Browse the Raycast Store and install extensions."),
        keywords: marked("extensions, install, plugins"),
        argument: None, handles_activities: false,
    },
    Builtin {
        name: "extensions", title: marked("Manage Extensions"), icon: "box-16", view: true,
        description: marked("Configure and uninstall installed extensions."),
        keywords: marked("uninstall, preferences, plugins"),
        argument: None, handles_activities: false,
    },
    Builtin {
        name: "clipboard-history", title: marked("Clipboard History"), icon: "clipboard-16", view: true,
        description: marked("Search, preview and paste everything you have copied."),
        keywords: marked("paste, copy, pasteboard"),
        argument: None, handles_activities: false,
    },
    Builtin {
        name: "selection", title: marked("Selected Text Actions"), icon: "text-selection-16", view: true,
        description: marked("Translate, improve, explain or search the text selected in any application. Give it a hotkey."),
        keywords: marked("translate, improve, explain, ai"),
        argument: None, handles_activities: false,
    },
    Builtin {
        name: "ask-ai", title: marked("Ask AI"), icon: "stars-16", view: true,
        description: marked("Ask the AI model a question. From the root search, type the question and press Tab."),
        keywords: marked("chat, question, assistant, llm, claude"),
        argument: None, handles_activities: false,
    },
    Builtin {
        name: "ai-chat", title: marked("AI Chat"), icon: "speech-bubble-16", view: true,
        description: marked("Go back to a conversation with the AI model and continue it. Conversations are kept on this computer."),
        keywords: marked("conversations, history, assistant, llm, claude, ask"),
        argument: None, handles_activities: false,
    },
    Builtin {
        name: "create-quicklink", title: marked("Create Quicklink"), icon: "link-16", view: true,
        description: marked("Save a link, optionally with a {0} placeholder."),
        keywords: marked("bookmark, url, add"),
        argument: None, handles_activities: false,
    },
    Builtin {
        name: "search-quicklinks", title: marked("Search Quicklinks"), icon: "link-16", view: true,
        description: marked("Open, edit and delete quicklinks."),
        keywords: marked("bookmarks, urls"),
        argument: None, handles_activities: false,
    },
    Builtin {
        name: "create-snippet", title: marked("Create Snippet"), icon: "snippets-16", view: true,
        description: marked("Save text to paste anywhere, with dynamic placeholders."),
        keywords: marked("text, template, add"),
        argument: None, handles_activities: false,
    },
    Builtin {
        name: "search-snippets", title: marked("Search Snippets"), icon: "snippets-16", view: true,
        description: marked("Paste, edit and delete snippets."),
        keywords: marked("text, templates"),
        argument: None, handles_activities: false,
    },
    Builtin {
        name: "switch-windows", title: marked("Switch Windows"), icon: "app-window-list-16", view: true,
        description: marked("Search the open windows and jump to one."),
        keywords: marked("window, alt tab, focus, open"),
        argument: None, handles_activities: false,
    },
    Builtin {
        name: "emoji", title: marked("Search Emoji & Symbols"), icon: "emoji-16", view: true,
        description: marked("Find an emoji or symbol in English or Chinese and paste it."),
        keywords: marked("emoticon, smiley, symbol, character, 表情"),
        argument: None, handles_activities: false,
    },
    Builtin {
        name: "search-files", title: marked("Search Files"), icon: "magnifying-glass-16", view: true,
        description: marked("Find files and folders under your home folder by name."),
        keywords: marked("find, documents, folder, locate"),
        argument: None, handles_activities: false,
    },
    Builtin {
        name: "kill-process", title: marked("Kill Process"), icon: "bolt-16", view: true,
        description: marked("Stop a running process; search by name, command or listening port."),
        keywords: marked("quit, port, task, pid, terminate"),
        argument: None, handles_activities: false,
    },
    Builtin {
        name: "audio-devices", title: marked("Switch Audio Device"), icon: "speaker-on-16", view: true,
        description: marked("Choose the default speaker and microphone."),
        keywords: marked("sound, output, input, microphone, speaker, headphones"),
        argument: None, handles_activities: false,
    },
    Builtin {
        name: "search-bookmarks", title: marked("Search Browser Bookmarks"), icon: "bookmark-16", view: true,
        description: marked("Search bookmarks from Firefox, Chrome, Chromium, Brave and Edge."),
        keywords: marked("firefox, chrome, chromium, favorites"),
        argument: None, handles_activities: false,
    },
    Builtin {
        name: "search-history", title: marked("Search Browser History"), icon: "clock-16", view: true,
        description: marked("Search pages you visited in Firefox, Chrome, Chromium, Brave and Edge."),
        keywords: marked("firefox, chrome, chromium, visited"),
        argument: None, handles_activities: false,
    },
    Builtin {
        name: "lock-screen", title: marked("Lock Screen"), icon: "lock-16", view: false,
        description: marked("Lock the session."),
        keywords: marked("system, lock"),
        argument: None, handles_activities: false,
    },
    Builtin {
        name: "sleep", title: marked("Sleep"), icon: "moon-16", view: false,
        description: marked("Suspend the computer."),
        keywords: marked("system, suspend"),
        argument: None, handles_activities: false,
    },
    Builtin {
        name: "restart", title: marked("Restart"), icon: "arrow-clockwise-16", view: false,
        description: marked("Restart the computer, after confirming."),
        keywords: marked("system, reboot"),
        argument: None, handles_activities: false,
    },
    Builtin {
        name: "shut-down", title: marked("Shut Down"), icon: "power-16", view: false,
        description: marked("Shut the computer down, after confirming."),
        keywords: marked("system, poweroff, power off"),
        argument: None, handles_activities: false,
    },
    Builtin {
        name: "log-out", title: marked("Log Out"), icon: "logout-16", view: false,
        description: marked("End the desktop session, after confirming."),
        keywords: marked("system, sign out"),
        argument: None, handles_activities: false,
    },
    Builtin {
        name: "empty-trash", title: marked("Empty Trash"), icon: "trash-16", view: false,
        description: marked("Permanently delete everything in the trash, after confirming."),
        keywords: marked("system, bin, delete"),
        argument: None, handles_activities: false,
    },
    Builtin {
        name: "toggle-dark-mode", title: marked("Toggle Dark Mode"), icon: "moon-16", view: false,
        description: marked("Switch the desktop between light and dark appearance."),
        keywords: marked("system, appearance, light, theme"),
        argument: None, handles_activities: false,
    },
    Builtin {
        name: "volume-up", title: marked("Volume Up"), icon: "speaker-up-16", view: false,
        description: marked("Raise the volume by 5%."),
        keywords: marked("system, sound, louder"),
        argument: None, handles_activities: false,
    },
    Builtin {
        name: "volume-down", title: marked("Volume Down"), icon: "speaker-down-16", view: false,
        description: marked("Lower the volume by 5%."),
        keywords: marked("system, sound, quieter"),
        argument: None, handles_activities: false,
    },
    Builtin {
        name: "toggle-mute", title: marked("Toggle Mute"), icon: "speaker-off-16", view: false,
        description: marked("Mute or unmute the speakers."),
        keywords: marked("system, sound, silence"),
        argument: None, handles_activities: false,
    },
    Builtin {
        name: "timer", title: marked("Timer"), icon: "clock-16", view: false,
        description: marked("Count down from a length of time typed in the search bar (25m, 1h 10m, 90s): shown in the In Progress section with Pause and +1 min, with a notification at the end."),
        keywords: marked("countdown, pomodoro, alarm, minutes"),
        argument: Some(("duration", "25m, 1h 10m, 90s")), handles_activities: true,
    },
];

fn builtins() -> Vec<Command> {
    BUILTINS
        .iter()
        .map(|b| Command {
            extension: BUILTIN.into(),
            name: b.name.into(),
            title: translate(b.title).into(),
            mode: b.mode().into(),
            interval: None,
            arguments: b
                .argument
                .map(|(name, placeholder)| Argument { name: name.into(), kind: "text".into(), placeholder: Some(placeholder.into()), required: true, data: Vec::new() })
                .into_iter()
                .collect(),
            handles_activities: b.handles_activities,
            subtitle: Some(if b.view || b.handles_activities { "Heyo" } else { system_group() }.into()),
            keywords: keywords(b.keywords),
            icon: Some(b.icon.into()),
        })
        .collect()
}

/// Prefers the `@dark` variant of an asset, matching Heyo's dark theme.
fn asset(dir: &Path, name: &str) -> Option<String> {
    let path = dir.join("assets").join(name);
    let dark = match (path.file_stem(), path.extension()) {
        (Some(stem), Some(ext)) => {
            path.with_file_name(format!("{}@dark.{}", stem.to_string_lossy(), ext.to_string_lossy()))
        }
        _ => path.clone(),
    };
    [dark, path].into_iter().find(|p| p.is_file()).map(|p| p.to_string_lossy().into_owned())
}

fn load(dir: &Path) -> Option<Vec<Command>> {
    let text = fs::read_to_string(dir.join("package.json")).ok()?;
    let manifest: Manifest = match serde_json::from_str(&text) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("heyo: skipping {}: {e}", dir.display());
            return None;
        }
    };
    // An extension that says it is for other platforms offers nothing here.
    if manifest.heyo.as_ref().is_some_and(|heyo| !heyo.runs_on(PLATFORM)) {
        return Some(Vec::new());
    }
    let extension_icon = manifest.icon.as_deref().and_then(|i| asset(dir, i));
    let commands = manifest
        .commands
        .into_iter()
        // Commands disabled by default need enabling in Raycast too.
        .filter(|c| !c.disabled_by_default)
        .filter(|c| dir.join(format!("{}.js", c.name)).is_file())
        .map(|c| Command {
            extension: dir.to_string_lossy().into_owned(),
            icon: c.icon.as_deref().and_then(|i| asset(dir, i)).or_else(|| extension_icon.clone()),
            subtitle: c.subtitle.or_else(|| Some(manifest.title.clone())),
            keywords: c.keywords.into_iter().chain([manifest.title.clone()]).collect(),
            interval: c.interval.as_deref().and_then(parse_interval),
            arguments: c.arguments,
            handles_activities: c.heyo.as_ref().and_then(|heyo| heyo.handles.as_deref()) == Some("activity"),
            mode: c.mode,
            name: c.name,
            title: c.title,
        })
        .collect();
    Some(commands)
}

/// The directories of the installed extensions, in order.
fn installed() -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(crate::paths::extensions()) else { return Vec::new() };
    let mut dirs: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        // Dot-directories are in-progress installs.
        .filter(|p| p.is_dir() && !p.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.')))
        .collect();
    dirs.sort();
    dirs
}

pub fn discover() -> Vec<Command> {
    let mut commands = builtins();
    for dir in installed() {
        commands.extend(load(&dir).unwrap_or_default());
    }
    for entry in development() {
        commands.extend(load_development(&entry));
    }
    commands
}

/// An extension being developed, as `heyo dev` registered it.
#[derive(Debug, Clone, PartialEq, serde::Serialize, Deserialize)]
pub struct Development {
    /// The manifest's name.
    pub name: String,
    /// The source directory `heyo dev` was run on.
    pub source: String,
    /// Where it is built: what is loaded, and the extension's id.
    pub dir: String,
}

#[derive(Default, serde::Serialize, Deserialize)]
struct Registry {
    #[serde(default)]
    extensions: Vec<Development>,
}

/// What is said of a development extension's commands wherever they are listed.
const IN_DEVELOPMENT: &str = marked("Development");

/// The registry's entries. An entry that is not one is passed over, so a
/// file written by a newer `heyo dev` still gives what can be read of it.
fn parse_development(text: &str) -> Vec<Development> {
    let Ok(registry) = serde_json::from_str::<serde_json::Value>(text) else { return Vec::new() };
    let entries = registry["extensions"].as_array().cloned().unwrap_or_default();
    entries.into_iter().filter_map(|entry| serde_json::from_value(entry).ok()).collect()
}

/// The extensions being developed. They stay until taken out (`heyo dev
/// --remove`, or uninstalling them), across restarts of Heyo.
pub fn development() -> Vec<Development> {
    fs::read_to_string(crate::paths::development()).map(|text| parse_development(&text)).unwrap_or_default()
}

/// A development extension's commands, each marked as one.
fn load_development(entry: &Development) -> Vec<Command> {
    let mut commands = load(Path::new(&entry.dir)).unwrap_or_default();
    for command in &mut commands {
        let said = translate(IN_DEVELOPMENT);
        command.subtitle = Some(match command.subtitle.take() {
            Some(subtitle) => format!("{subtitle} · {said}"),
            None => said.to_owned(),
        });
        command.keywords.push(IN_DEVELOPMENT.to_lowercase());
        if said != IN_DEVELOPMENT {
            command.keywords.push(said.to_owned());
        }
    }
    commands
}

/// Takes a development extension out: off the list, and what was built of
/// it deleted. Its sources are not touched.
fn stop_developing(entry: &Development) -> Result<(), String> {
    let file = crate::paths::development();
    let registry = Registry { extensions: development().into_iter().filter(|other| other != entry).collect() };
    let text = serde_json::to_string_pretty(&registry).map_err(|e| e.to_string())?;
    let partial = file.with_extension("json.tmp");
    fs::write(&partial, text + "\n").and_then(|()| fs::rename(&partial, &file)).map_err(|e| crate::i18n::tf("failed to write {0}: {1}", &[&file.display(), &e]))?;
    // Only ever what `heyo dev` built, whatever the file was made to say.
    let dir = Path::new(&entry.dir);
    if dir.parent() == Some(crate::paths::data().join("development").as_path()) {
        let _ = fs::remove_dir_all(dir);
        let _ = fs::remove_file(format!("{}.log", entry.dir));
    }
    Ok(())
}

/// An extension as the settings window shows it: the manifest's metadata,
/// preference definitions and the saved preference values.
#[derive(serde::Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../shared/bindings/")]
pub struct ExtensionInfo {
    /// `@heyo`, or the extension's directory.
    pub id: String,
    pub name: String,
    pub title: String,
    pub description: Option<String>,
    pub author: Option<String>,
    pub icon: Option<String>,
    pub builtin: bool,
    /// The source directory, for an extension being developed (`heyo dev`).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub development: Option<String>,
    pub commands: Vec<CommandInfo>,
    /// Extension-level preference definitions from the manifest.
    pub preferences: Vec<Preference>,
    pub values: PreferenceValues,
}

#[derive(serde::Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../shared/bindings/")]
pub struct CommandInfo {
    pub id: String,
    pub name: String,
    pub title: String,
    pub description: Option<String>,
    pub mode: String,
    pub icon: Option<String>,
    pub preferences: Vec<Preference>,
}

/// A preference a manifest declares, as much of it as the settings window
/// shows.
#[derive(Debug, Clone, PartialEq, serde::Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../shared/bindings/")]
pub struct Preference {
    pub name: String,
    /// `textfield`, `password`, `checkbox`, `dropdown`, `appPicker`, `file`
    /// or `directory`.
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub title: Option<String>,
    /// What stands beside a checkbox.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub placeholder: Option<String>,
    #[serde(default)]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "unknown")]
    pub default: Option<serde_json::Value>,
    /// The choices of a `dropdown`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, as = "Option<Vec<Choice>>")]
    pub data: Vec<Choice>,
}

/// The preferences a manifest, or one of its commands, declares. One that
/// is not well formed is left out.
fn preferences(declaring: &serde_json::Value) -> Vec<Preference> {
    declaring["preferences"].as_array().into_iter().flatten().filter_map(|preference| serde_json::from_value(preference.clone()).ok()).collect()
}

/// The preferences the user has set, as the runtime stores them
/// (`runtime/src/context.ts`): the extension's own, and each command's
/// under its name.
#[derive(Debug, Default, Clone, PartialEq, serde::Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../shared/bindings/")]
pub struct PreferenceValues {
    #[serde(default)]
    #[ts(type = "Record<string, unknown>")]
    pub extension: serde_json::Map<String, serde_json::Value>,
    #[serde(default)]
    #[ts(type = "Record<string, Record<string, unknown>>")]
    pub commands: std::collections::BTreeMap<String, serde_json::Map<String, serde_json::Value>>,
}

pub fn preferences_path(extension: &str) -> PathBuf {
    crate::paths::preferences(extension)
}

fn stored_values(extension: &str) -> PreferenceValues {
    fs::read_to_string(preferences_path(extension)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// Every extension with all of its commands, including Heyo's own.
pub fn list() -> Vec<ExtensionInfo> {
    let builtin = ExtensionInfo {
        id: BUILTIN.into(),
        name: BUILTIN.into(),
        title: "Heyo".into(),
        description: Some(t("Commands built into Heyo.").into()),
        author: None,
        icon: Some("store-16".into()),
        builtin: true,
        development: None,
        commands: builtins()
            .into_iter()
            .map(|c| CommandInfo {
                id: c.id(),
                description: BUILTINS.iter().find(|b| b.name == c.name).map(Builtin::description),
                mode: c.mode.clone(),
                name: c.name,
                title: c.title,
                icon: c.icon,
                preferences: Vec::new(),
            })
            .collect(),
        preferences: Vec::new(),
        values: PreferenceValues::default(),
    };
    let mut builtin = builtin;
    // Heyo's native commands, so they get aliases, hotkeys and a switch too.
    builtin.commands.extend(crate::root::NATIVE.iter().filter(|n| n.id != crate::root::SETTINGS_ID).map(|n| CommandInfo {
        id: n.id.clone(),
        name: n.id.clone(),
        title: n.title().into(),
        description: Some(n.description().into()),
        mode: "native".into(),
        icon: Some(n.icon.into()),
        preferences: Vec::new(),
    }));
    let mut out = vec![builtin];
    let developed = development().into_iter().map(|entry| (PathBuf::from(entry.dir), Some(entry.source)));
    for (dir, source) in installed().into_iter().map(|dir| (dir, None)).chain(developed) {
        let Some(manifest) = fs::read_to_string(dir.join("package.json"))
            .ok()
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        else {
            continue;
        };
        let str_of = |v: &serde_json::Value, key: &str| v[key].as_str().map(str::to_owned);
        let name = str_of(&manifest, "name").unwrap_or_default();
        let id = dir.to_string_lossy().into_owned();
        let extension_icon = str_of(&manifest, "icon").and_then(|i| asset(&dir, &i));
        let commands = manifest["commands"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|c| {
                let command = str_of(c, "name").unwrap_or_default();
                CommandInfo {
                    id: format!("cmd:{id}:{command}"),
                    title: str_of(c, "title").unwrap_or_else(|| command.clone()),
                    name: command,
                    description: str_of(c, "description"),
                    mode: str_of(c, "mode").unwrap_or_default(),
                    icon: str_of(c, "icon").and_then(|i| asset(&dir, &i)).or_else(|| extension_icon.clone()),
                    preferences: preferences(c),
                }
            })
            .collect();
        out.push(ExtensionInfo {
            title: str_of(&manifest, "title").unwrap_or_else(|| name.clone()),
            description: str_of(&manifest, "description"),
            author: str_of(&manifest, "author"),
            icon: extension_icon,
            builtin: false,
            development: source,
            commands,
            preferences: preferences(&manifest),
            values: stored_values(&name),
            name,
            id,
        });
    }
    out
}

pub fn save_preferences(extension: &str, values: &PreferenceValues) -> Result<(), String> {
    if extension.contains(['/', '\\']) || extension.starts_with('.') {
        return Err(format!("invalid extension name {extension}"));
    }
    let path = preferences_path(extension);
    fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(values).map_err(|e| e.to_string())?;
    fs::write(&path, text).map_err(|e| e.to_string())
}

/// Deletes an installed extension's directory, or takes a development
/// extension out of Heyo.
pub fn uninstall(dir: &str) -> Result<(), String> {
    if let Some(entry) = development().into_iter().find(|entry| entry.dir == dir) {
        return stop_developing(&entry);
    }
    let root = crate::paths::extensions().canonicalize().map_err(|e| e.to_string())?;
    let dir = Path::new(dir).canonicalize().map_err(|e| e.to_string())?;
    if dir.parent() != Some(root.as_path()) {
        return Err(format!("{} is not an installed extension", dir.display()));
    }
    fs::remove_dir_all(&dir).map_err(|e| e.to_string())
}

#[cfg(test)]
mod catalogue {
    use super::*;

    #[test]
    fn every_builtin_is_named_once_and_described() {
        let mut names: Vec<&str> = BUILTINS.iter().map(|b| b.name).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), BUILTINS.len());
        assert!(BUILTINS.iter().all(|b| !b.description.is_empty() && !b.title.is_empty()));
    }

    #[test]
    fn keywords_are_a_list_apart_by_commas() {
        // In English, as the tests run: the list and nothing else.
        assert_eq!(keywords("window, alt tab, focus"), ["window", "alt tab", "focus"]);
        assert!(BUILTINS.iter().all(|b| !b.keywords.is_empty() && b.keywords.split(", ").all(|keyword| !keyword.is_empty() && keyword.trim() == keyword && !keyword.contains(','))));
    }

    #[test]
    fn the_commands_that_show_nothing_are_the_system_actions() {
        // Each is an action the desktop knows (`platform::system::Action`),
        // but for the timer, which shows itself as a live activity.
        for builtin in BUILTINS.iter().filter(|b| !b.view && b.name != "timer") {
            let action = serde_json::from_value::<crate::platform::system::Action>(builtin.name.into());
            assert!(action.is_ok(), "{} is not a system action", builtin.name);
        }
    }

    #[test]
    fn a_builtin_may_take_an_argument_and_handle_activities() {
        let timer = builtins().into_iter().find(|c| c.name == "timer").unwrap();
        assert_eq!(timer.arguments.len(), 1);
        assert!(timer.arguments[0].name == "duration" && timer.arguments[0].required && timer.handles_activities);
        assert_eq!(timer.subtitle.as_deref(), Some("Heyo"));
        assert!(builtins().iter().filter(|c| c.name != "timer").all(|c| c.arguments.is_empty() && !c.handles_activities));
    }

    #[test]
    fn a_command_may_handle_its_extensions_activities() {
        let manifest = serde_json::json!({ "name": "events", "title": "Events", "mode": "no-view", "heyo": { "handles": "activity" } });
        let command: CommandManifest = serde_json::from_value(manifest).unwrap();
        assert_eq!(command.heyo.unwrap().handles.as_deref(), Some("activity"));
        let plain: CommandManifest = serde_json::from_value(serde_json::json!({ "name": "a", "title": "A", "mode": "view" })).unwrap();
        assert!(plain.heyo.is_none());
    }

    #[test]
    fn a_manifest_says_what_a_command_asks_for() {
        let manifest = serde_json::json!({
            "name": "at", "title": "Weather At", "mode": "view",
            "arguments": [
                { "name": "city", "type": "text", "placeholder": "City", "required": true },
                { "name": "unit", "type": "dropdown", "data": [{ "title": "Celsius", "value": "c" }] },
                { "name": "note" },
            ],
        });
        let command: CommandManifest = serde_json::from_value(manifest).unwrap();
        let [city, unit, note] = &command.arguments[..] else { panic!("three arguments") };
        assert!(city.required && city.kind == "text" && city.placeholder.as_deref() == Some("City"));
        assert!(!unit.required && unit.kind == "dropdown" && unit.data[0].value == "c");
        // What is left out: a text field that may stay empty.
        assert!(!note.required && note.kind == "text" && note.data.is_empty());
    }

    #[test]
    fn a_manifests_preferences_reach_the_settings_as_declared() {
        let manifest = serde_json::json!({
            "preferences": [
                { "name": "token", "type": "password", "title": "API Token", "description": "From your account page.", "required": true, "placeholder": "tok_…" },
                { "name": "unit", "type": "dropdown", "required": false, "default": "c", "data": [{ "title": "Celsius", "value": "c" }, { "title": "Fahrenheit", "value": "f" }] },
                { "name": "notify", "type": "checkbox", "label": "Notify me", "default": true },
                { "title": "No name" },
                "not a preference",
            ],
        });
        let read = preferences(&manifest);
        // One that is not well formed is left out; the rest stay.
        let [token, unit, notify] = &read[..] else { panic!("three preferences: {read:?}") };
        assert!(token.required && token.kind == "password" && token.title.as_deref() == Some("API Token") && token.placeholder.as_deref() == Some("tok_…"));
        assert!(!unit.required && unit.data[1].value == "f" && unit.default == Some("c".into()));
        assert!(notify.label.as_deref() == Some("Notify me") && notify.default == Some(true.into()));
        // As the window is sent them: under the manifest's own names, without what was left out.
        assert_eq!(serde_json::to_value(notify).unwrap(), serde_json::json!({ "name": "notify", "type": "checkbox", "label": "Notify me", "required": false, "default": true }));
        assert!(preferences(&serde_json::json!({ "name": "none" })).is_empty());
        assert!(preferences(&serde_json::json!({ "preferences": "many" })).is_empty());
    }

    #[test]
    fn the_values_set_are_kept_as_the_runtime_stores_them() {
        let stored = serde_json::json!({ "extension": { "token": "tok_1" }, "commands": { "now": { "unit": "f" } } });
        let values: PreferenceValues = serde_json::from_value(stored.clone()).unwrap();
        assert_eq!(values.commands["now"]["unit"], "f");
        assert_eq!(serde_json::to_value(&values).unwrap(), stored);
        // Nothing set yet, or only some of it.
        let none: PreferenceValues = serde_json::from_value(serde_json::json!({})).unwrap();
        assert_eq!(serde_json::to_value(none).unwrap(), serde_json::json!({ "extension": {}, "commands": {} }));
        let some: PreferenceValues = serde_json::from_value(serde_json::json!({ "extension": { "a": 1 } })).unwrap();
        assert!(some.commands.is_empty() && some.extension["a"] == 1);
    }

    #[test]
    fn the_development_list_survives_what_it_cannot_read() {
        let text = r#"{ "extensions": [
            { "name": "a", "source": "/src/a", "dir": "/data/development/a" },
            { "name": "broken" },
            { "name": "b", "source": "/src/b", "dir": "/data/development/b", "later": true }
        ] }"#;
        let names: Vec<String> = parse_development(text).into_iter().map(|entry| entry.name).collect();
        assert_eq!(names, ["a", "b"]);
        assert!(parse_development("").is_empty());
        assert!(parse_development("{}").is_empty());
        assert!(parse_development(r#"{ "extensions": 3 }"#).is_empty());
    }

    #[test]
    fn a_development_extensions_commands_say_so() {
        let dir = std::env::temp_dir().join(format!("heyo-development-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let manifest = serde_json::json!({
            "name": "draft", "title": "Draft",
            "commands": [
                { "name": "one", "title": "One", "mode": "view" },
                { "name": "two", "title": "Two", "mode": "no-view", "subtitle": "Second" },
                { "name": "unbuilt", "title": "Unbuilt", "mode": "view" },
            ],
        });
        fs::write(dir.join("package.json"), manifest.to_string()).unwrap();
        fs::write(dir.join("one.js"), "").unwrap();
        fs::write(dir.join("two.js"), "").unwrap();
        let entry = Development { name: "draft".into(), source: "/src/draft".into(), dir: dir.to_string_lossy().into_owned() };
        let commands = load_development(&entry);
        let _ = fs::remove_dir_all(&dir);
        // Only what has been built, under the built directory as its extension.
        let subtitles: Vec<_> = commands.iter().map(|c| (c.name.as_str(), c.subtitle.as_deref().unwrap())).collect();
        assert_eq!(subtitles, [("one", "Draft · Development"), ("two", "Second · Development")]);
        assert!(commands.iter().all(|c| c.extension == entry.dir && c.keywords.contains(&"development".to_owned())));
        // Nothing built yet: nothing listed, and no complaint.
        assert!(load_development(&Development { dir: "/nonexistent/heyo".into(), ..entry }).is_empty());
    }

    #[test]
    fn a_manifest_says_what_an_extension_for_heyo_declares() {
        let manifest = r#"{ "name": "window-sessions", "title": "Window Sessions",
            "heyo": { "apiVersion": 1, "permissions": ["windows", "memory.read"], "platforms": ["linux", "windows"] } }"#;
        let declared = declared_in(manifest).unwrap();
        assert_eq!((declared.name.as_str(), declared.title.as_str()), ("window-sessions", "Window Sessions"));
        let heyo = declared.heyo.unwrap();
        assert_eq!((heyo.api_version, heyo.permissions.len()), (1, 2));
        assert!(heyo.runs_on("linux") && heyo.runs_on("windows") && !heyo.runs_on("macos"));

        // A store extension has no such field, which is not the same as an empty one.
        assert_eq!(declared_in(r#"{ "name": "weather" }"#).unwrap().heyo, None);
        let empty = declared_in(r#"{ "name": "weather", "heyo": {} }"#).unwrap().heyo.unwrap();
        assert_eq!((empty.api_version, empty.permissions.len()), (1, 0));
        assert!(empty.runs_on("linux"));
        // One that cannot be read declares nothing, rather than everything.
        assert_eq!(declared_in(r#"{ "name": "odd", "heyo": { "permissions": "all" } }"#).unwrap().heyo, Some(Heyo::default()));
        assert_eq!(declared_in("{}"), None);

        // An extension for other platforms keeps its commands to itself here.
        let elsewhere: Manifest = serde_json::from_str(r#"{ "title": "Mac Only", "heyo": { "platforms": ["macos"] } }"#).unwrap();
        assert!(!elsewhere.heyo.unwrap().runs_on(PLATFORM));
    }

    /// Writes the runtime's copy of the list, next to the generated types.
    #[test]
    fn export_builtins() {
        let mut out = String::from("// Generated from src-tauri/src/extensions.rs by `cargo test`. Do not edit.\n\n");
        out.push_str("/** Heyo's built-in commands, as the runtime's manifest of them. */\nexport const builtins = [\n");
        for b in BUILTINS {
            out.push_str(&format!("  {{ name: {:?}, title: {:?}, mode: {:?}, icon: {:?} }},\n", b.name, b.title, b.mode(), b.icon));
        }
        out.push_str("] as const;\n\nexport type BuiltinName = (typeof builtins)[number][\"name\"];\n");
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../shared/bindings/builtins.ts");
        if std::fs::read_to_string(path).ok().as_deref() != Some(&out) {
            std::fs::write(path, out).unwrap();
        }
    }
}

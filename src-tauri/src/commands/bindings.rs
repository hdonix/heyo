//! The host as its windows may call it: every command with its arguments
//! and what it answers, and every event with what it carries. `cargo test`
//! writes this to `shared/bindings/commands.ts`, which the web view calls
//! and listens through (`src/host.ts`): a command it names is one the host
//! has, taking those arguments.
//!
//! A row repeats a command's signature as a window sees it: the arguments
//! Tauri takes from the call (not the `AppHandle`, the state or the window
//! it adds itself), and what comes back when the command works. The types
//! are the host's own, so their TypeScript is ts-rs's. A test reads every
//! `#[tauri::command]` out of the sources and holds the rows to them.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use regex::Regex;
use ts_rs::{Config, Dependency, TypeVisitor, TS};

use crate::activities::ActivityView;
use crate::clipboard::{meaning, sources::Source};
use crate::commands::media::OcrStatus;
use crate::commands::settings::{HotkeyName, Hotkeys, SettingsView};
use crate::extensions::{ExtensionInfo, PreferenceValues};
use crate::helper::InstallProgress;
use crate::open::Arguments;
use crate::permissions::{Permission, Row};
use crate::platform::Capabilities;
use crate::root::Item;
use crate::screenshot::Finish;
use crate::settings::Settings;
use crate::wire::WebviewMessage;
use crate::{dictation, expand, screenshot};

commands! {
    search(query: String) -> Vec<Item>;
    launch(id: String, fallback_text: Option<String>, arguments: Option<Arguments>);
    runtime_send(message: WebviewMessage);
    hide();
    get_settings() -> SettingsView;
    change_settings(changes: serde_json::Map<String, Value> as SettingsChanges) -> SettingsView;
    set_anthropic_api_key(key: String) -> SettingsView;
    open_settings();
    open_extensions_folder();
    list_extensions() -> Vec<ExtensionInfo>;
    set_extension_preferences(extension: String, values: PreferenceValues);
    uninstall_extension(id: String);
    clear_clipboard_history();
    get_hotkeys() -> Hotkeys;
    set_hotkey(name: HotkeyName, accelerator: String) -> Hotkeys;
    set_item_hotkey(id: String, title: String, accelerator: String);
    item_hotkeys() -> HashMap<String, String>;
    screenshot_finish(request: Finish) -> String;
    ocr_status() -> OcrStatus;
    ocr_install();
    dictation_status() -> dictation::Status;
    dictation_install();
    dictation_toggle();
    dictation_cancel();
    show_window();
    get_item(id: String) -> Option<Item>;
    list_applications() -> Vec<Item>;
    list_quicklinks() -> Vec<Item>;
    get_capabilities() -> Capabilities;
    install_shell_extension() -> Capabilities;
    local_models(url: String) -> Vec<String>;
    record_usage(id: String);
    set_favorite(id: String, favorite: bool);
    move_favorite(id: String, offset: i32);
    set_alias(id: String, alias: String);
    reset_ranking(id: String);
    read_clipboard() -> String;
    copy_text(text: String, paste: bool);
    open_target(target: String);
    clipboard_sources() -> Vec<Source>;
    clipboard_meaning_status() -> meaning::Status;
    clipboard_meaning_install();
    check_for_update() -> Option<String>;
    install_update();
    copy_diagnostics();
    extension_permissions(id: String) -> Vec<Row>;
    revoke_extension_permission(extension: String, permission: Permission);
    snippet_expanded(request: u64, text: String);
    activities() -> Vec<ActivityView>;
    activity_action(key: String, button: Option<usize>);
    activity_dismiss(key: String);
}

events! {
    SHOWN: ();
    INDEX_READY: ();
    // As every window may see them: where the files are is left empty, and
    // the settings window asks for the whole (`get_settings`).
    SETTINGS_CHANGED: SettingsView;
    OPEN_ITEM: String;
    RUNTIME: RuntimeMessage;
    DICTATION: dictation::Told;
    DICTATION_INSTALL: InstallProgress;
    OCR_INSTALL: InstallProgress;
    SCREENSHOT: screenshot::Capture;
    EXPAND_SNIPPET: expand::ExpandSnippet;
    CLIPBOARD_MEANING_INSTALL: InstallProgress;
    ACTIVITIES: Vec<ActivityView>;
}

/// The settings that change, under the names they are stored by, and
/// whether Heyo opens at login. The API key is not among them: it has a
/// command of its own.
struct SettingsChanges;

impl TS for SettingsChanges {
    type WithoutGenerics = Self;
    type OptionInnerType = Self;

    fn name(cfg: &Config) -> String {
        format!("Partial<Omit<{}, \"anthropicApiKey\">> & {{ launchAtLogin?: boolean }}", Settings::name(cfg))
    }

    fn inline(cfg: &Config) -> String {
        Self::name(cfg)
    }

    fn visit_dependencies(visitor: &mut impl TypeVisitor) {
        visitor.visit::<Settings>();
    }
}

/// What the runtime tells the webview. The host passes it on unread, so it
/// is declared where both ends see it: in `shared/wire.ts`.
struct RuntimeMessage;

impl TS for RuntimeMessage {
    type WithoutGenerics = Self;
    type OptionInnerType = Self;

    fn name(_: &Config) -> String {
        "RuntimeMessage".into()
    }

    fn inline(cfg: &Config) -> String {
        Self::name(cfg)
    }

    fn output_path() -> Option<PathBuf> {
        Some(format!("{SHARED}wire.ts").into())
    }
}

/// Where the generated types go, as ts-rs names it (`export_to`), and the
/// directory above it.
const BINDINGS: &str = "../../shared/bindings/";
const SHARED: &str = "../../shared/";

/// A type of the host's, as TypeScript writes it.
struct Type {
    /// As the row has it.
    rust: &'static str,
    ts: String,
    /// It may be left out of a call's arguments.
    optional: bool,
    /// The declared types it may mention, by name, and the files they are in.
    uses: Vec<Dependency>,
}

impl Type {
    fn of<T: TS + 'static>(rust: &'static str) -> Type {
        struct Uses<'a>(&'a Config, Vec<Dependency>);
        impl TypeVisitor for Uses<'_> {
            fn visit<U: TS + 'static + ?Sized>(&mut self) {
                self.1.extend(Dependency::from_ty::<U>(self.0));
                // What it is made of, too: `Array<Item>` mentions `Item`.
                U::visit_generics(self);
            }
        }
        // A window counts in JavaScript's numbers, which hold what Heyo counts.
        let cfg = Config::new().with_large_int("number");
        let mut uses = Uses(&cfg, Vec::new());
        uses.visit::<T>();
        T::visit_dependencies(&mut uses);
        Type { rust, ts: T::name(&cfg), optional: T::IS_OPTION, uses: uses.1 }
    }
}

struct Command {
    name: &'static str,
    arguments: Vec<(&'static str, Type)>,
    /// Of `()` when the command answers nothing.
    result: Type,
}

struct Event {
    /// As `wire::event` names it.
    name: &'static str,
    carries: Type,
}

/// The commands, each as `name(argument: Type, …) -> Result;`. An argument
/// the host takes as plain JSON says what that JSON is with `as`.
macro_rules! commands {
    (@ts $ty:ty) => { $ty };
    (@ts $ty:ty, $ts:ty) => { $ts };
    (@result) => { Type::of::<()>("()") };
    (@result $result:ty) => { Type::of::<$result>(stringify!($result)) };
    ($($name:ident($($argument:ident: $ty:ty $(as $ts:ty)?),*) $(-> $result:ty)?;)*) => {
        fn command_rows() -> Vec<Command> {
            vec![$(Command {
                name: stringify!($name),
                arguments: vec![$((stringify!($argument), Type::of::<commands!(@ts $ty $(, $ts)?)>(stringify!($ty)))),*],
                result: commands!(@result $($result)?),
            }),*]
        }
    };
}
use commands;

/// The events, each as `NAME: Payload;`, named as in `wire::event`.
macro_rules! events {
    ($($name:ident: $carries:ty;)*) => {
        fn event_rows() -> Vec<Event> {
            // Naming the constant keeps a row from outliving its event.
            vec![$({ let _ = crate::wire::event::$name; Event { name: stringify!($name), carries: Type::of::<$carries>(stringify!($carries)) } }),*]
        }
    };
}
use events;

// --- The commands as the sources have them -----------------------------------

/// A `#[tauri::command]` function, read out of its source.
pub(crate) struct Defined {
    pub name: String,
    /// Its doc comment.
    doc: String,
    /// What a window passes: name and type, as written.
    arguments: Vec<(String, String)>,
    /// What it answers when it works, as written.
    result: String,
}

/// A type as written, without the spaces and the modules it is reached
/// through: `root::Item` in one file is `crate::root::Item` in another.
fn plain(written: &str) -> String {
    let compact: String = written.split_whitespace().collect();
    Regex::new(r"\b[a-z_][a-z0-9_]*::").unwrap().replace_all(&compact, "").into_owned()
}

/// The parts of a list between its commas, leaving alone those inside `<>`.
fn listed(list: &str) -> Vec<&str> {
    let (mut parts, mut depth, mut start) = (Vec::new(), 0, 0);
    for (at, c) in list.char_indices() {
        match c {
            '<' => depth += 1,
            '>' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(&list[start..at]);
                start = at + 1;
            }
            _ => {}
        }
    }
    parts.push(&list[start..]);
    parts.into_iter().map(str::trim).filter(|part| !part.is_empty()).collect()
}

/// The commands in one source file.
fn defined_in(source: &str) -> Vec<Defined> {
    let signature = Regex::new(concat!(
        r"(?m)^((?:[ \t]*///.*\n)*)(?:[ \t]*//.*\n)*",
        r"[ \t]*#\[tauri::command(?:\([a-z]*\))?\]\n(?:[ \t]*//.*\n)*",
        r"[ \t]*pub(?:\(crate\))? (?:async )?fn (\w+)\(([^{;]*?)\)(?: -> ([^{;]+?))? \{",
    ))
    .unwrap();
    // What Tauri hands a command itself is not an argument of the call.
    let given = |kind: &str| ["AppHandle", "WebviewWindow", "Window"].contains(&kind) || kind.starts_with("State<");
    signature
        .captures_iter(source)
        .map(|found| {
            let part = |n: usize| found.get(n).map_or("", |m| m.as_str());
            let result = plain(part(4));
            Defined {
                name: part(2).to_owned(),
                doc: part(1).lines().map(|line| line.trim().trim_start_matches("///").trim()).collect::<Vec<_>>().join("\n"),
                arguments: listed(part(3))
                    .into_iter()
                    .filter_map(|argument| argument.split_once(':'))
                    .map(|(name, kind)| (name.trim().trim_start_matches("mut ").to_owned(), plain(kind)))
                    .filter(|(_, kind)| !given(kind))
                    .collect(),
                // What goes wrong is a sentence the call is rejected with.
                result: match result.strip_prefix("Result<").and_then(|rest| rest.strip_suffix(",String>")) {
                    Some(answer) => answer.to_owned(),
                    None if result.is_empty() => "()".to_owned(),
                    None => result,
                },
            }
        })
        .collect()
}

/// Every command defined anywhere in the crate.
pub(crate) fn defined() -> Vec<Defined> {
    let mut found = Vec::new();
    for entry in walkdir::WalkDir::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src")).into_iter().flatten().filter(|entry| entry.path().extension().is_some_and(|e| e == "rs")) {
        let source = std::fs::read_to_string(entry.path()).unwrap();
        let commands = defined_in(&source);
        let marked = source.lines().filter(|line| line.trim_start().starts_with("#[tauri::command")).count();
        assert_eq!(commands.len(), marked, "{}: a #[tauri::command] is written in a way that cannot be read here (its signature on one line, under its comments)", entry.path().display());
        found.extend(commands);
    }
    found
}

/// The names between the brackets of `generate_handler![…]` in `lib.rs`.
pub(crate) fn registered() -> Vec<String> {
    let source = include_str!("../lib.rs");
    let list = source.split("generate_handler![").nth(1).and_then(|rest| rest.split(']').next()).expect("lib.rs registers its commands with generate_handler!");
    // A command is called by its own name, whatever module it is registered from.
    list.split(',').map(|name| name.trim().rsplit("::").next().unwrap_or_default().to_owned()).filter(|name| !name.is_empty()).collect()
}

// --- The TypeScript ------------------------------------------------------------

/// `fallback_text` and `SETTINGS_CHANGED` as JavaScript names them.
fn camel(name: &str) -> String {
    let mut parts = name.split('_').map(str::to_lowercase);
    let first = parts.next().unwrap_or_default();
    parts.fold(first, |name, part| name + &part[..1].to_uppercase() + &part[1..])
}

/// A command's doc comment, as a member's in TypeScript.
fn comment(doc: &str) -> String {
    match doc.lines().collect::<Vec<_>>().as_slice() {
        [] => String::new(),
        [line] => format!("  /** {line} */\n"),
        lines => {
            let lines: String = lines.iter().map(|line| format!("   * {line}").trim_end().to_owned() + "\n").collect();
            format!("  /**\n{lines}   */\n")
        }
    }
}

/// `shared/bindings/commands.ts`.
fn typescript(commands: &[Command], events: &[Event], defined: &[Defined]) -> String {
    let mut body = String::from("/** The commands the host takes from its windows: the arguments of each, and what it answers. */\nexport type Commands = {\n");
    for command in commands {
        if let Some(source) = defined.iter().find(|defined| defined.name == command.name) {
            body.push_str(&comment(&source.doc));
        }
        let arguments: Vec<String> = command.arguments.iter().map(|(name, kind)| format!("{}{}: {}", camel(name), if kind.optional { "?" } else { "" }, kind.ts)).collect();
        let arguments = if arguments.is_empty() { "null".to_owned() } else { format!("{{ {} }}", arguments.join("; ")) };
        body.push_str(&format!("  {}: {{ args: {arguments}; result: {} }};\n", command.name, command.result.ts));
    }
    body.push_str("};\n\n/** The commands that answer nothing: a host played in a test answers them unasked. */\nexport const answersNothing = [\n");
    for command in commands.iter().filter(|command| command.result.ts == "null") {
        body.push_str(&format!("  \"{}\",\n", command.name));
    }
    body.push_str("] as const satisfies ReadonlyArray<keyof Commands>;\n\n/** What each of the host's events (`event` in ./events) carries. */\nexport type Events = {\n");
    for event in events {
        body.push_str(&format!("  {}: {};\n", camel(event.name), event.carries.ts));
    }
    body.push_str("};\n");

    // Only the types the table mentions are imported: an unused one is an error to the web view's compiler.
    let kinds: Vec<&Type> = commands.iter().flat_map(|command| command.arguments.iter().map(|(_, kind)| kind).chain([&command.result])).chain(events.iter().map(|event| &event.carries)).collect();
    let mentioned = kinds.iter().map(|kind| kind.ts.as_str()).collect::<Vec<_>>().join(" ");
    let mut imports = BTreeMap::new();
    for used in kinds.iter().flat_map(|kind| &kind.uses) {
        if Regex::new(&format!(r"\b{}\b", regex::escape(&used.ts_name))).unwrap().is_match(&mentioned) {
            let file = used.output_path.to_string_lossy().replace('\\', "/");
            let from = match file.strip_prefix(BINDINGS) {
                Some(beside) => format!("./{beside}"),
                None => format!("../{}", file.strip_prefix(SHARED).unwrap_or_else(|| panic!("{} is declared outside shared/: {file}", used.ts_name))),
            };
            imports.insert(used.ts_name.clone(), from.trim_end_matches(".ts").to_owned());
        }
    }
    let mut out = String::from("// Generated from src-tauri/src/commands/bindings.rs by `cargo test`. Do not edit.\n");
    for (name, from) in imports {
        out.push_str(&format!("import type {{ {name} }} from \"{from}\";\n"));
    }
    out + "\n" + &body
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A command registered and left out here could not be called from the
    /// web view; a row for one that is gone, or that has changed, would
    /// promise what the host does not do.
    #[test]
    fn the_rows_are_the_commands_as_they_are_defined() {
        let (rows, defined, registered) = (command_rows(), defined(), registered());
        assert!(registered.len() > 30, "could not read the commands out of lib.rs: {registered:?}");
        for name in &registered {
            assert_eq!(rows.iter().filter(|row| row.name == name).count(), 1, "{name} is registered in lib.rs: give it a row in commands/bindings.rs (once)");
            assert_eq!(defined.iter().filter(|command| &command.name == name).count(), 1, "{name} is registered in lib.rs but not defined once as a #[tauri::command]");
        }
        for row in &rows {
            let Some(command) = defined.iter().find(|command| command.name == row.name) else { panic!("{} has a row but is not a command", row.name) };
            assert!(registered.iter().any(|name| name == row.name), "{} has a row but is not registered in lib.rs", row.name);
            let written: Vec<(String, String)> = row.arguments.iter().map(|(name, kind)| ((*name).to_owned(), plain(kind.rust))).collect();
            assert_eq!(written, command.arguments, "the arguments of {} are not those of its row", row.name);
            assert_eq!(plain(row.result.rust), command.result, "what {} answers is not what its row says", row.name);
        }
    }

    #[test]
    fn a_signature_is_read_as_a_window_sees_it() {
        // `%` for the attribute, which no line here may begin with: this file is read too.
        let source = "
/// Opens it.
/// Twice.
// Async: it waits.
%(async)]
pub(crate) fn launch(id: String, fallback_text: Option<String>, arguments: Option<open::Arguments>, app: AppHandle) -> Result<(), String> {
}

%]
pub(crate) async fn change(mut changes: serde_json::Map<String, Value>, usage: State<UsageStore>, window: tauri::WebviewWindow) -> Result<Vec<root::Item>, String> {
}

%]
pub(crate) fn hide(window: WebviewWindow) {
}
";
        let read = defined_in(&source.replace('%', "#[tauri::command"));
        let shape = |command: &Defined| (command.name.clone(), command.arguments.iter().map(|(name, kind)| format!("{name}: {kind}")).collect::<Vec<_>>().join(", "), command.result.clone());
        assert_eq!(shape(&read[0]), ("launch".into(), "id: String, fallback_text: Option<String>, arguments: Option<Arguments>".into(), "()".into()));
        assert_eq!(read[0].doc, "Opens it.\nTwice.");
        assert_eq!(shape(&read[1]), ("change".into(), "changes: Map<String,Value>".into(), "Vec<Item>".into()));
        assert_eq!(shape(&read[2]), ("hide".into(), String::new(), "()".into()));
        assert_eq!(read.len(), 3);
        assert_eq!((camel("fallback_text"), camel("SETTINGS_CHANGED"), camel("id")), ("fallbackText".into(), "settingsChanged".into(), "id".into()));
    }

    /// The events `wire.rs` declares, by the names of their constants.
    fn declared() -> Vec<String> {
        let source = include_str!("../wire.rs");
        let list = source.split("\nevents! {").nth(1).and_then(|rest| rest.split("\n}").next()).expect("wire.rs declares its events with events!");
        list.lines().filter_map(|line| line.trim().split_once(" = \"")).map(|(name, _)| name.to_owned()).collect()
    }

    #[test]
    fn every_event_says_what_it_carries() {
        let (rows, declared) = (event_rows(), declared());
        assert!(declared.len() > 10, "could not read the events out of wire.rs: {declared:?}");
        for name in &declared {
            assert_eq!(rows.iter().filter(|row| row.name == name).count(), 1, "{name} is an event in wire.rs: say in commands/bindings.rs what it carries (once)");
        }
        assert_eq!(rows.len(), declared.len());
    }

    #[test]
    fn the_typescript_says_what_the_rows_say() {
        let text = typescript(&command_rows(), &event_rows(), &defined());
        for line in [
            "  search: { args: { query: string }; result: Array<Item> };",
            "  launch: { args: { id: string; fallbackText?: string | null; arguments?: { [key in string]: string } | null }; result: null };",
            "  hide: { args: null; result: null };",
            "  change_settings: { args: { changes: Partial<Omit<Settings, \"anthropicApiKey\">> & { launchAtLogin?: boolean } }; result: SettingsView };",
            "  snippet_expanded: { args: { request: number; text: string }; result: null };",
            "  check_for_update: { args: null; result: string | null };",
            "  settingsChanged: SettingsView;",
            "  shown: null;",
            "  runtime: RuntimeMessage;",
            "  activities: Array<ActivityView>;",
            "import type { Item } from \"./Item\";",
            "import type { Settings } from \"./Settings\";",
            "import type { DictationEvent } from \"./DictationEvent\";",
            "import type { RuntimeMessage } from \"../wire\";",
            "  /** Counts a use of a root search item, for ranking. */",
        ] {
            assert!(text.lines().any(|written| written == line), "missing: {line}\n{text}");
        }
        // Only what is mentioned is imported, and what answers nothing is listed.
        assert!(!text.contains("import type { Argument }"), "{text}");
        assert!(text.contains("  \"hide\",\n") && !text.contains("  \"search\",\n"), "{text}");
    }

    /// Writes the table for the web view, next to the generated types.
    #[test]
    fn export_commands() {
        let out = typescript(&command_rows(), &event_rows(), &defined());
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../shared/bindings/commands.ts");
        if std::fs::read_to_string(path).ok().as_deref() != Some(&out) {
            std::fs::write(path, out).unwrap();
        }
    }
}

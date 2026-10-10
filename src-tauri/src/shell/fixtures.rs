//! Keeps the Flutter interface's idea of the host's types true.
//!
//! The web view's TypeScript is generated from the Rust types (`ts-rs`).
//! Dart's models (`ui/lib/host/models.dart`) are written by hand, so they
//! are held to the Rust types by example instead: `cargo test` writes one
//! of everything a window is sent or sends, as JSON, into
//! `shared/fixtures/`, and `flutter test` must read each file into its
//! model and write the same JSON back. A field added, renamed or removed
//! here changes a file there, and a model that has not followed fails.
//!
//! The same run writes what Dart needs to know by name: the events
//! (`ui/lib/host/events.g.dart`) and the commands with the windows that may
//! invoke each (`ui/lib/host/commands.g.dart`).

use serde_json::{json, Value};

use super::dispatch::COMMANDS;

fn value(of: impl serde::Serialize) -> Value {
    serde_json::to_value(of).unwrap()
}

/// One of everything, by the name of its file.
fn fixtures() -> Vec<(&'static str, Value)> {
    let (settings, hotkeys) = crate::commands::settings::fixture();
    let (capture, finish) = crate::screenshot::fixture();
    let extension = crate::extensions::ExtensionInfo {
        id: "/home/ada/.local/share/heyo/extensions/todo".into(),
        name: "todo".into(),
        title: "Todo".into(),
        description: Some("Things to do".into()),
        author: Some("ada".into()),
        icon: Some("/home/ada/.local/share/heyo/extensions/todo/assets/icon.png".into()),
        builtin: false,
        development: Some("/home/ada/src/todo".into()),
        commands: vec![crate::extensions::CommandInfo {
            id: "command:/home/ada/.local/share/heyo/extensions/todo/add".into(),
            name: "add".into(),
            title: "Add Todo".into(),
            description: Some("Adds one".into()),
            mode: "view".into(),
            icon: Some("plus-16".into()),
            preferences: json!([{ "name": "list", "type": "textfield", "title": "List", "required": false }]),
        }],
        preferences: json!([{ "name": "token", "type": "password", "title": "Token", "required": true }]),
        values: json!({ "extension": { "token": "abc" }, "commands": { "add": { "list": "Home" } } }),
    };
    let source = crate::clipboard::sources::Source { id: "firefox.desktop".into(), name: "Firefox".into(), icon: Some("/usr/share/icons/firefox.png".into()), seen: true };
    let meaning = crate::clipboard::meaning::Status { enabled: true, installed: true, installing: false, indexed: 120, waiting: 3, problem: Some("The model could not be loaded".into()) };
    let permission = crate::permissions::Row { permission: crate::permissions::Permission::ScreenCapture, allows: "take pictures of the screen", state: "allowed" };
    // What a window tells the runtime. Read into the type and written back:
    // the interface is held to sending exactly these.
    let messages: Vec<Value> = [
        json!({ "type": "dispatch", "session": 3, "node": 12, "prop": "onAction", "args": [1, "two"] }),
        json!({ "type": "pop", "session": 3 }),
        json!({ "type": "close", "session": 3 }),
        json!({ "type": "reply", "id": 7, "result": true }),
        json!({ "type": "reply", "id": 8, "error": "no" }),
        json!({ "type": "toast-action", "id": 2, "action": "primary" }),
        json!({ "type": "open", "target": "https://example.com" }),
    ]
    .into_iter()
    .map(|message| value(serde_json::from_value::<crate::wire::WebviewMessage>(message).expect("a message a window sends")))
    .collect();
    vec![
        ("Item", value(crate::root::fixture())),
        ("SettingsView", value(settings)),
        ("Hotkeys", value(hotkeys)),
        ("Capabilities", value(crate::platform::fixture())),
        ("OcrStatus", value(crate::commands::media::fixture())),
        ("DictationStatus", value(crate::dictation::fixture())),
        ("ExtensionInfo", value(extension)),
        ("ClipboardSource", value(source)),
        ("ClipboardMeaningStatus", value(meaning)),
        ("PermissionRow", value(permission)),
        ("ScreenshotCapture", value(capture)),
        ("ScreenshotFinish", finish),
        ("WebviewMessage", Value::Array(messages)),
    ]
}

/// The types exported for TypeScript that a fixture above holds (itself or
/// as part of another), and those that never reach a window: they are what
/// the runtime and the host say to each other.
const IN_A_FIXTURE: &[&str] = &[
    "Item", "Argument", "Choice", "SettingsView", "Settings", "Appearance", "Hotkeys", "Capabilities", "Support", "HoldKey", "ClipboardSource",
    "ClipboardMeaningStatus", "Permission", "Rect", "WebviewMessage",
];
const NOT_FOR_A_WINDOW: &[&str] = &[
    "Action", "AiProvider", "AudioDevice", "AudioKind", "Capture", "CaptureTarget", "CaptureWhole", "ErrorCode", "ExtCaller", "ExtError", "ExtRequest", "ExtWindow",
    "HostMessage", "HostRequest", "Kind", "MemoryItem", "MemoryResult", "Module", "ProcessInfo", "RecognizedText", "Route", "Subscription", "Understood",
    "WindowLayout", "builtins", "events",
];

fn repository(path: &str) -> std::path::PathBuf {
    std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).join(path)
}

/// Writes a generated file, leaving one that is already right alone.
fn write(path: &std::path::Path, text: &str) {
    if std::fs::read_to_string(path).ok().as_deref() != Some(text) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
}

#[test]
fn the_fixtures_are_written() {
    let dir = repository("shared/fixtures");
    let fixtures = fixtures();
    for (name, value) in &fixtures {
        write(&dir.join(format!("{name}.json")), &format!("{}\n", serde_json::to_string_pretty(value).unwrap()));
    }
    // One left from a type that is gone would be read by nobody, and mislead.
    for entry in std::fs::read_dir(&dir).unwrap().flatten() {
        let name = entry.file_name().to_string_lossy().trim_end_matches(".json").to_owned();
        assert!(fixtures.iter().any(|(known, _)| *known == name), "shared/fixtures/{name}.json is no longer written: delete it");
    }
}

/// A type newly exported to TypeScript is one the web view is about to
/// use: say here whether a window is sent it (and give it a fixture, and
/// Dart a model) or not.
#[test]
fn every_exported_type_is_accounted_for() {
    let mut seen = 0;
    for entry in std::fs::read_dir(repository("shared/bindings")).unwrap().flatten() {
        let name = entry.file_name().to_string_lossy().trim_end_matches(".ts").to_owned();
        let places = IN_A_FIXTURE.contains(&name.as_str()) as u8 + NOT_FOR_A_WINDOW.contains(&name.as_str()) as u8;
        assert_eq!(places, 1, "shared/bindings/{name}.ts: list it in shell/fixtures.rs, as in a fixture or as not for a window");
        seen += 1;
    }
    assert!(seen > 30, "could not read shared/bindings");
}

/// `SHOWN` → `shown`, `INDEX_READY` → `indexReady`.
fn camel(name: &str) -> String {
    let mut out = String::new();
    let mut upper = false;
    for c in name.chars() {
        match c {
            '_' => upper = true,
            c if upper => {
                out.push(c);
                upper = false;
            }
            c => out.push(c.to_ascii_lowercase()),
        }
    }
    out
}

#[test]
fn the_names_are_written_for_dart() {
    let mut events = String::from("// Generated from src-tauri/src/wire.rs by `cargo test`. Do not edit.\n\n/// The events the host emits to its windows.\nabstract final class HeyoEvent {\n");
    for (name, value) in crate::wire::EVENTS {
        events.push_str(&format!("  static const {} = '{value}';\n", camel(name)));
    }
    events.push_str("}\n");
    write(&repository("ui/lib/host/events.g.dart"), &events);

    // The windows by the label of one of each kind.
    let windows = [("main", "main"), ("settings", "settings"), ("dictation", "dictation"), ("screenshot", "screenshot"), ("pin", "pin-1")];
    let mut commands = String::from(
        "// Generated from src-tauri/src/shell/dispatch.rs and commands/access.rs by\n// `cargo test`. Do not edit.\n\n/// Every command of the host's, and the kinds of window that may invoke it.\nconst hostCommands = <String, List<String>>{\n",
    );
    for (command, _) in COMMANDS {
        let allowed: Vec<String> = windows.iter().filter(|(_, label)| crate::commands::access::allowed(label, command)).map(|(kind, _)| format!("'{kind}'")).collect();
        commands.push_str(&format!("  '{command}': [{}],\n", allowed.join(", ")));
    }
    commands.push_str("};\n");
    write(&repository("ui/lib/host/commands.g.dart"), &commands);
}

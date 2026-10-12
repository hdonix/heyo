//! Which window may invoke which command.
//!
//! Every window loads the same page, and the palette draws what extensions
//! hand it. So a window is held to the commands its own view calls: a
//! palette that was talked into something still cannot change the settings,
//! and the screenshot overlay cannot open anything. A command that is not
//! in the table is refused to every window; a test keeps the table complete.

use tauri::ipc::Invoke;
use tauri::Runtime;

/// The windows, by what they show (`src/main.tsx` picks the view the same way).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Caller {
    /// `main`: the root search and the commands of extensions.
    Palette,
    /// The overlay a capture is selected on.
    Screenshot,
    /// `pin-N`: a capture kept on screen. It asks nothing of the host.
    Pin,
}

impl Caller {
    fn of(label: &str) -> Option<Caller> {
        match label {
            crate::palette::WINDOW => Some(Caller::Palette),
            crate::screenshot::WINDOW => Some(Caller::Screenshot),
            _ => label.strip_prefix("pin-").filter(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())).map(|_| Caller::Pin),
        }
    }
}

use Caller::{Palette, Screenshot};

/// Each command and the windows whose view calls it (`call(` in `src/`).
const TABLE: &[(&str, &[Caller])] = &[
    // The palette: searching, opening, what it remembers about its items.
    ("search", &[Palette]),
    ("launch", &[Palette]),
    ("runtime_send", &[Palette]),
    ("hide", &[Palette]),
    ("show_window", &[Palette]),
    ("get_item", &[Palette]),
    ("list_quicklinks", &[Palette]),
    ("record_usage", &[Palette]),
    ("set_favorite", &[Palette]),
    ("move_favorite", &[Palette]),
    ("set_alias", &[Palette]),
    ("reset_ranking", &[Palette]),
    ("read_clipboard", &[Palette]),
    ("copy_text", &[Palette]),
    ("open_target", &[Palette]),
    ("open_settings", &[Palette]),
    ("snippet_expanded", &[Palette]),
    // The live activities in progress, and acting on one.
    ("activities", &[Palette]),
    ("activity_action", &[Palette]),
    ("activity_dismiss", &[Palette]),
    // Both read the settings (the palette for its appearance and layout),
    // and both bind a hotkey to an item.
    ("get_settings", &[Palette]),
    ("set_item_hotkey", &[Palette]),
    // Changing things is the settings window's alone.
    ("screenshot_finish", &[Screenshot]),
];

/// Whether the window of that label may invoke that command.
pub(crate) fn allowed(window: &str, command: &str) -> bool {
    let Some(caller) = Caller::of(window) else { return false };
    TABLE.iter().any(|(name, callers)| *name == command && callers.contains(&caller))
}

/// Puts the table in front of the commands: `handler` only ever sees an
/// invocation its window is allowed to make.
pub(crate) fn guarded<R: Runtime>(handler: impl Fn(Invoke<R>) -> bool + Send + Sync + 'static) -> impl Fn(Invoke<R>) -> bool + Send + Sync + 'static {
    move |invoke| {
        let (window, command) = (invoke.message.webview().label().to_owned(), invoke.message.command().to_owned());
        if allowed(&window, &command) {
            return handler(invoke);
        }
        eprintln!("heyo: refused {command} to the window {window}");
        invoke.resolver.reject(format!("{command} is not available to this window"));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::super::bindings::{defined, registered};
    use super::*;

    #[test]
    fn a_window_is_known_by_its_label() {
        assert_eq!(Caller::of("main"), Some(Palette));
        assert_eq!(Caller::of("screenshot"), Some(Screenshot));
        assert_eq!(Caller::of("pin-3"), Some(Caller::Pin));
        for stranger in ["", "Main", "pin-", "pin-x", "pin-3-settings", "settings", "dictation", "other"] {
            assert_eq!(Caller::of(stranger), None, "{stranger}");
        }
    }

    #[test]
    fn each_window_is_held_to_its_own_commands() {
        let cases = [
            // The palette does its work…
            ("main", "search", true),
            ("main", "launch", true),
            ("main", "runtime_send", true),
            ("main", "copy_text", true),
            ("main", "set_favorite", true),
            ("main", "get_settings", true),
            // …but what it draws comes from extensions: it changes no settings.
            ("main", "change_settings", false),
            ("main", "set_anthropic_api_key", false),
            ("main", "set_extension_preferences", false),
            ("main", "uninstall_extension", false),
            ("main", "install_shell_extension", false),
            ("main", "clear_clipboard_history", false),
            ("main", "local_models", false),
            ("main", "copy_diagnostics", false),
            ("main", "screenshot_finish", false),
            // What an extension was allowed is the user's to take back, in the
            // settings; the palette, which extensions draw in, cannot.
            ("main", "revoke_extension_permission", false),
            ("main", "extension_permissions", false),
            // The settings window runs nothing and pastes nothing.
            ("screenshot", "screenshot_finish", true),
            ("screenshot", "launch", false),
            ("screenshot", "change_settings", false),
            ("pin-1", "screenshot_finish", false),
            ("pin-1", "hide", false),
            // Nothing for a window Heyo does not open, or a command it does not have.
            ("devtools", "search", false),
            ("main", "format_disk", false),
            ("main", "", false),
        ];
        for (window, command, expected) in cases {
            assert_eq!(allowed(window, command), expected, "{window} calling {command}");
        }
        // A pinned capture may call nothing at all.
        assert!(TABLE.iter().all(|(command, _)| !allowed("pin-1", command)));
    }

    /// A command added to `lib.rs` and forgotten here would be refused to
    /// every window; one left here after it is gone would mislead.
    #[test]
    fn the_table_holds_exactly_the_registered_commands() {
        let registered = registered();
        assert!(registered.len() > 20, "could not read the commands out of lib.rs: {registered:?}");
        for command in &registered {
            let rows = TABLE.iter().filter(|(name, _)| name == command).count();
            assert_eq!(rows, 1, "{command} is registered in lib.rs: say in commands/access.rs which windows may call it (once)");
        }
        for (command, callers) in TABLE {
            assert!(registered.iter().any(|name| name == command), "{command} is in the table but not registered in lib.rs");
            assert!(!callers.is_empty(), "{command} may be called by no window: remove it");
        }
    }

    /// Every `#[tauri::command]` in the crate, whichever module defines it,
    /// is registered, so the check above sees it.
    #[test]
    fn every_command_defined_is_registered() {
        let (registered, defined) = (registered(), defined());
        assert!(defined.len() > 20, "could not read the commands out of the sources");
        for command in defined {
            assert!(registered.contains(&command.name), "{} is a command but lib.rs does not register it", command.name);
        }
    }
}

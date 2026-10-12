//! What the settings window asks of Heyo.
//!
//! The window is in the host's own process, but asks as a web view would:
//! a command by its name, arguments and an answer as JSON, and the events
//! the host emits. So what it may do is a list one can read (`COMMANDS`),
//! the model that decides what to ask needs no host to be tested, and
//! nothing the host decides is decided a second time here. Most of these
//! are functions only this window calls; `get_settings` and
//! `set_item_hotkey` are commands of the palette's web view too.

use std::sync::Arc;

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use tauri::async_runtime::block_on;
use tauri::{AppHandle, Listener, Manager};

use crate::commands::{media, root, settings};
use crate::{clipboard, permissions, updates};
use crate::wire::event;

/// The host, as a settings window that is not a web view sees it.
pub(crate) trait Host: Send + Sync + 'static {
    /// Calls a command of the host by the name `invoke` knows it by.
    fn call(&self, command: &str, args: Value) -> Result<Value, String>;
    /// Hears the events the host emits to its windows, by their names.
    fn listen(&self, hear: Box<dyn Fn(&str, Value) + Send + Sync>);
    /// Whether the desktop shows applications dark.
    fn system_dark(&self) -> bool;
    /// Has the desktop bring Heyo's window of that title to the front,
    /// where a window cannot do so itself (Wayland). It does not wait.
    fn forward(&self, title: String);
    /// Something was done to the dictation pill.
    fn pill(&self, did: super::pill::Did);
}

/// The commands this window calls: each is answered in `call` below.
pub(crate) const COMMANDS: &[&str] = &[
    "get_settings",
    "change_settings",
    "get_hotkeys",
    "set_hotkey",
    "set_item_hotkey",
    "item_hotkeys",
    "list_extensions",
    "list_applications",
    "get_capabilities",
    "install_shell_extension",
    "dictation_status",
    "dictation_install",
    "copy_diagnostics",
    "check_for_update",
    "install_update",
    "set_anthropic_api_key",
    "open_extensions_folder",
    "set_extension_preferences",
    "uninstall_extension",
    "clear_clipboard_history",
    "local_models",
    "ocr_status",
    "ocr_install",
    "clipboard_sources",
    "clipboard_meaning_status",
    "clipboard_meaning_install",
    "extension_permissions",
    "revoke_extension_permission",
];

/// The events it listens to.
const EVENTS: &[&str] = &[event::SETTINGS_CHANGED, event::INDEX_READY, event::DICTATION_INSTALL, event::OCR_INSTALL, event::CLIPBOARD_MEANING_INSTALL];

/// Heyo itself.
pub(crate) struct Running(pub AppHandle);

/// The argument `name` of a command, as the window sent it.
fn arg<T: DeserializeOwned>(args: &Value, name: &str) -> Result<T, String> {
    serde_json::from_value(args.get(name).cloned().unwrap_or(Value::Null)).map_err(|e| format!("{name}: {e}"))
}

/// A command's answer, as a window sees it.
fn answer(value: impl Serialize) -> Result<Value, String> {
    serde_json::to_value(value).map_err(|e| e.to_string())
}

impl Host for Running {
    fn call(&self, command: &str, args: Value) -> Result<Value, String> {
        if !COMMANDS.contains(&command) {
            return Err(format!("{command} is not available to this window"));
        }
        let app = self.0.clone();
        match command {
            "get_settings" => answer(settings::settings_for_its_window(&app)),
            "change_settings" => answer(settings::change_settings(arg(&args, "changes")?, app)?),
            "get_hotkeys" => answer(block_on(settings::get_hotkeys())),
            "set_hotkey" => answer(block_on(settings::set_hotkey(arg(&args, "name")?, arg(&args, "accelerator")?))?),
            "set_item_hotkey" => answer(block_on(settings::set_item_hotkey(arg(&args, "id")?, arg(&args, "title")?, arg(&args, "accelerator")?, app))?),
            "item_hotkeys" => answer(settings::item_hotkeys(app.state())),
            "list_extensions" => answer(settings::list_extensions()),
            "list_applications" => answer(root::list_applications(app)),
            "get_capabilities" => answer(settings::get_capabilities()),
            "install_shell_extension" => answer(settings::install_shell_extension()?),
            "dictation_status" => answer(media::dictation_status(app)),
            "dictation_install" => answer(media::dictation_install(app)?),
            "copy_diagnostics" => answer(settings::copy_diagnostics(app)?),
            "check_for_update" => answer(block_on(updates::check_for_update(app))?),
            "install_update" => answer(block_on(updates::install_update(app))?),
            "set_anthropic_api_key" => answer(settings::set_anthropic_api_key(arg(&args, "key")?, app)?),
            "open_extensions_folder" => answer(settings::open_extensions_folder()?),
            "set_extension_preferences" => answer(settings::set_extension_preferences(arg(&args, "extension")?, arg(&args, "values")?)?),
            "uninstall_extension" => answer(settings::uninstall_extension(arg(&args, "id")?, app)?),
            "clear_clipboard_history" => answer(settings::clear_clipboard_history(app.state())),
            "local_models" => answer(block_on(settings::local_models(arg(&args, "url")?))?),
            "ocr_status" => answer(media::ocr_status()),
            "ocr_install" => answer(media::ocr_install(app)?),
            "clipboard_sources" => answer(clipboard::clipboard_sources(app)),
            "clipboard_meaning_status" => answer(clipboard::clipboard_meaning_status(app)),
            "clipboard_meaning_install" => answer(clipboard::clipboard_meaning_install(app)?),
            "extension_permissions" => answer(permissions::extension_permissions(arg(&args, "id")?)),
            "revoke_extension_permission" => answer(permissions::revoke_extension_permission(arg(&args, "extension")?, arg(&args, "permission")?)?),
            _ => Err(format!("{command} is not available to this window")),
        }
    }

    fn listen(&self, hear: Box<dyn Fn(&str, Value) + Send + Sync>) {
        let hear = Arc::new(hear);
        for name in EVENTS {
            let hear = hear.clone();
            self.0.listen_any(*name, move |told| hear(name, serde_json::from_str(told.payload()).unwrap_or(Value::Null)));
        }
    }

    fn system_dark(&self) -> bool {
        crate::platform::system::dark().unwrap_or(false)
    }

    fn forward(&self, title: String) {
        // The desktop knows the window a moment after it was asked to be
        // shown, not at once: it is asked until it does, for half a second.
        #[cfg(target_os = "linux")]
        std::thread::spawn(move || {
            let desktop = crate::platform::desktop::desktop();
            for _ in 0..10 {
                if desktop.activate_own(&title) {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        });
        #[cfg(not(target_os = "linux"))]
        let _ = title;
    }

    fn pill(&self, did: super::pill::Did) {
        use super::pill::Did;
        match did {
            Did::Pressed => crate::dictation::pressed(&self.0),
            Did::Finish => crate::dictation::toggle(&self.0),
            Did::Cancel => crate::dictation::cancel(&self.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every command the window may call is answered here.
    #[test]
    fn every_command_the_window_calls_is_answered() {
        let source = include_str!("host.rs");
        for command in COMMANDS {
            assert!(source.contains(&format!("\"{command}\" => ")), "{command} is not answered in host.rs");
        }
    }

    /// No web view may change the settings: what does is not a command of
    /// theirs at all.
    #[test]
    fn what_changes_settings_is_no_command_of_a_web_view() {
        let registered = crate::commands::bindings::registered();
        for command in ["change_settings", "set_anthropic_api_key", "set_extension_preferences", "uninstall_extension", "revoke_extension_permission", "install_update"] {
            assert!(COMMANDS.contains(&command) && !registered.iter().any(|known| known == command), "{command}");
        }
    }

    /// Nothing is answered that the window was not said to call: the list
    /// is the whole of what this window can do.
    #[test]
    fn nothing_else_is_answered() {
        let source = include_str!("host.rs");
        let answered = source.lines().filter(|line| line.trim_start().starts_with('"') && line.contains("\" => answer(")).count();
        assert_eq!(answered, COMMANDS.len());
    }
}

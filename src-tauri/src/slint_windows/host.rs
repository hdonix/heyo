//! What the settings window asks of Heyo.
//!
//! The window is in the host's own process, but asks as a web view would:
//! a command by its name, arguments and an answer as JSON, and the events
//! the host emits. So what it may do is a list one can read (`Command`:
//! each thing it may ask, with what it says in asking, written once),
//! the model that decides what to ask needs no host to be tested, and
//! nothing the host decides is decided a second time here. Most of these
//! are functions only this window calls; `get_settings` and
//! `set_item_hotkey` are commands of the palette's web view too.

use std::sync::Arc;

use serde::Serialize;
use serde_json::{Map, Value};
use tauri::async_runtime::block_on;
use tauri::{AppHandle, Listener, Manager};

use crate::commands::{media, root, settings};
use crate::{clipboard, permissions, updates};
use crate::wire::event;

/// The host, as a settings window that is not a web view sees it.
pub(crate) trait Host: Send + Sync + 'static {
    /// Asks the host for one of the things this window may ask.
    fn call(&self, command: Command) -> Result<Value, String>;
    /// Hears the events the host emits to its windows, by their names.
    fn listen(&self, hear: Box<dyn Fn(&str, Value) + Send + Sync>);
    /// Whether the desktop shows applications dark.
    fn system_dark(&self) -> bool;
    /// Has the desktop bring Heyo's window of that title to the front,
    /// where a window cannot do so itself (Wayland). It does not wait.
    fn forward(&self, title: String);
    /// Something was done to the dictation pill.
    fn pill(&self, did: super::pill::Did);
    /// What is to become of the capture in the screenshot overlay; and
    /// the selected part as a picture, where something was drawn on it.
    fn screenshot(&self, outcome: crate::screenshot::selecting::Outcome, drawn: Option<image::RgbaImage>);
}

/// What this window may ask of the host, each with what it says in
/// asking. This is the one place a command is written: the model makes
/// these, and `Running::call` answers every one of them, or does not
/// compile.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Command {
    GetSettings,
    /// The settings to change, by their names, and what each is to be.
    ChangeSettings(Map<String, Value>),
    GetHotkeys,
    /// One of Heyo's own hotkeys, by its name; none, when the accelerator is empty.
    SetHotkey { name: settings::HotkeyName, accelerator: String },
    /// The hotkey of an item of the palette, which is called `title`.
    SetItemHotkey { id: String, title: String, accelerator: String },
    ItemHotkeys,
    ListExtensions,
    ListApplications,
    GetCapabilities,
    InstallShellExtension,
    DictationStatus,
    DictationInstall,
    CopyDiagnostics,
    CheckForUpdate,
    InstallUpdate,
    /// The key; none, when it is empty.
    SetAnthropicApiKey(String),
    OpenExtensionsFolder,
    /// All the preferences of the extension of that name, as they are to be.
    SetExtensionPreferences { extension: String, values: Value },
    /// By the extension's id.
    UninstallExtension(String),
    ClearClipboardHistory,
    /// The models the server at that address lists.
    LocalModels(String),
    OcrStatus,
    OcrInstall,
    ClipboardSources,
    ClipboardMeaningStatus,
    ClipboardMeaningInstall,
    /// By the extension's id.
    ExtensionPermissions(String),
    RevokeExtensionPermission { extension: String, permission: permissions::Permission },
}

impl Command {
    /// What the host calls it: the name `invoke` knows it by where a web
    /// view may call it too, and the function's name where none may. For
    /// the tests, which hold which of these a web view may call.
    #[cfg(test)]
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Command::GetSettings => "get_settings",
            Command::ChangeSettings(_) => "change_settings",
            Command::GetHotkeys => "get_hotkeys",
            Command::SetHotkey { .. } => "set_hotkey",
            Command::SetItemHotkey { .. } => "set_item_hotkey",
            Command::ItemHotkeys => "item_hotkeys",
            Command::ListExtensions => "list_extensions",
            Command::ListApplications => "list_applications",
            Command::GetCapabilities => "get_capabilities",
            Command::InstallShellExtension => "install_shell_extension",
            Command::DictationStatus => "dictation_status",
            Command::DictationInstall => "dictation_install",
            Command::CopyDiagnostics => "copy_diagnostics",
            Command::CheckForUpdate => "check_for_update",
            Command::InstallUpdate => "install_update",
            Command::SetAnthropicApiKey(_) => "set_anthropic_api_key",
            Command::OpenExtensionsFolder => "open_extensions_folder",
            Command::SetExtensionPreferences { .. } => "set_extension_preferences",
            Command::UninstallExtension(_) => "uninstall_extension",
            Command::ClearClipboardHistory => "clear_clipboard_history",
            Command::LocalModels(_) => "local_models",
            Command::OcrStatus => "ocr_status",
            Command::OcrInstall => "ocr_install",
            Command::ClipboardSources => "clipboard_sources",
            Command::ClipboardMeaningStatus => "clipboard_meaning_status",
            Command::ClipboardMeaningInstall => "clipboard_meaning_install",
            Command::ExtensionPermissions(_) => "extension_permissions",
            Command::RevokeExtensionPermission { .. } => "revoke_extension_permission",
        }
    }
}

/// The events it listens to.
const EVENTS: &[&str] = &[event::SETTINGS_CHANGED, event::INDEX_READY, event::DICTATION_INSTALL, event::OCR_INSTALL, event::CLIPBOARD_MEANING_INSTALL];

/// Heyo itself.
pub(crate) struct Running(pub AppHandle);

/// A command's answer, as a window sees it.
fn answer(value: impl Serialize) -> Result<Value, String> {
    serde_json::to_value(value).map_err(|e| e.to_string())
}

impl Host for Running {
    fn call(&self, command: Command) -> Result<Value, String> {
        let app = self.0.clone();
        match command {
            Command::GetSettings => answer(settings::settings_for_its_window(&app)),
            Command::ChangeSettings(changes) => answer(settings::change_settings(changes, app)?),
            Command::GetHotkeys => answer(block_on(settings::get_hotkeys())),
            Command::SetHotkey { name, accelerator } => answer(block_on(settings::set_hotkey(name, accelerator))?),
            Command::SetItemHotkey { id, title, accelerator } => answer(block_on(settings::set_item_hotkey(id, title, accelerator, app))?),
            Command::ItemHotkeys => answer(settings::item_hotkeys(app.state())),
            Command::ListExtensions => answer(settings::list_extensions()),
            Command::ListApplications => answer(root::list_applications(app)),
            Command::GetCapabilities => answer(settings::get_capabilities()),
            Command::InstallShellExtension => answer(settings::install_shell_extension()?),
            Command::DictationStatus => answer(media::dictation_status(app)),
            Command::DictationInstall => answer(media::dictation_install(app)?),
            Command::CopyDiagnostics => answer(settings::copy_diagnostics(app)?),
            Command::CheckForUpdate => answer(block_on(updates::check_for_update(app))?),
            Command::InstallUpdate => answer(block_on(updates::install_update(app))?),
            Command::SetAnthropicApiKey(key) => answer(settings::set_anthropic_api_key(key, app)?),
            Command::OpenExtensionsFolder => answer(settings::open_extensions_folder()?),
            // The preferences are the host's own shape, which the model
            // was given and hands back with one of them changed.
            Command::SetExtensionPreferences { extension, values } => answer(settings::set_extension_preferences(extension, serde_json::from_value(values).map_err(|e| format!("values: {e}"))?)?),
            Command::UninstallExtension(id) => answer(settings::uninstall_extension(id, app)?),
            Command::ClearClipboardHistory => answer(settings::clear_clipboard_history(app.state())),
            Command::LocalModels(url) => answer(block_on(settings::local_models(url))?),
            Command::OcrStatus => answer(media::ocr_status()),
            Command::OcrInstall => answer(media::ocr_install(app)?),
            Command::ClipboardSources => answer(clipboard::clipboard_sources(app)),
            Command::ClipboardMeaningStatus => answer(clipboard::clipboard_meaning_status(app)),
            Command::ClipboardMeaningInstall => answer(clipboard::clipboard_meaning_install(app)?),
            Command::ExtensionPermissions(id) => answer(permissions::extension_permissions(id)),
            Command::RevokeExtensionPermission { extension, permission } => answer(permissions::revoke_extension_permission(extension, permission)?),
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

    fn screenshot(&self, outcome: crate::screenshot::selecting::Outcome, drawn: Option<image::RgbaImage>) {
        crate::screenshot::decided(&self.0, outcome, drawn);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// No web view may change the settings: what does is not a command of
    /// theirs at all.
    #[test]
    fn what_changes_settings_is_no_command_of_a_web_view() {
        let registered = crate::commands::bindings::registered();
        let (none, nothing) = (String::new, Value::Null);
        let changing = [
            Command::ChangeSettings(Map::new()),
            Command::SetAnthropicApiKey(none()),
            Command::SetExtensionPreferences { extension: none(), values: nothing },
            Command::UninstallExtension(none()),
            Command::RevokeExtensionPermission { extension: none(), permission: permissions::Permission::Windows },
            Command::InstallUpdate,
        ];
        for command in changing {
            assert!(!registered.iter().any(|known| known == command.name()), "{}", command.name());
        }
    }

    /// The two that a web view may call too are called here by the names
    /// it calls them by.
    #[test]
    fn what_a_web_view_may_call_too_has_its_name() {
        let registered = crate::commands::bindings::registered();
        for command in [Command::GetSettings, Command::SetItemHotkey { id: String::new(), title: String::new(), accelerator: String::new() }] {
            assert!(registered.iter().any(|known| known == command.name()), "{}", command.name());
        }
    }
}

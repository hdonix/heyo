//! What the settings window asks of Heyo.
//!
//! The window is in the host's own process, but asks as a web view would:
//! a command with its arguments, an answer (`answer::Answer`, typed; JSON
//! only where it crosses to the macOS window's process), and the events
//! the host emits. So what it may do is a list one can read (`Command`:
//! each thing it may ask, with what it says in asking, written once),
//! the model that decides what to ask needs no host to be tested, and
//! nothing the host decides is decided a second time here. Most of these
//! are functions only this window calls; `get_settings` and
//! `set_item_hotkey` are commands of the palette's web view too.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tauri::async_runtime::block_on;
use tauri::{AppHandle, Listener, Manager};

use super::answer::{self, Answer};
use crate::commands::{media, root, settings};
use crate::connections::commands as connections;
use crate::extensions::PreferenceValues;
use crate::{clipboard, permissions, updates};
use crate::wire::event;

/// The host, as a settings window that is not a web view sees it.
pub(crate) trait Host: Send + Sync + 'static {
    /// Asks the host for one of the things this window may ask.
    fn call(&self, command: Command) -> Result<Answer, String>;
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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
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
    /// The speech models of this computer, and what each is to this installation.
    SpeechModels,
    /// By its id.
    DownloadSpeechModel(String),
    CancelSpeechDownload(String),
    DeleteSpeechModel(String),
    CopyDiagnostics,
    CheckForUpdate,
    InstallUpdate,
    /// A provider of this kind (`ProviderKind::word`), connected: named
    /// so, and set up with these fields (by name), as its form says.
    AddProvider { kind: String, name: String, config: std::collections::BTreeMap<String, String> },
    /// A field of a provider's configuration, or its `name`, as it is to be.
    SetProviderField { provider: String, field: String, value: String },
    /// By its id.
    RemoveProvider(String),
    OpenExtensionsFolder,
    /// All the preferences of the extension of that name, as they are to be.
    SetExtensionPreferences { extension: String, values: PreferenceValues },
    /// By the extension's id.
    UninstallExtension(String),
    ClearClipboardHistory,
    /// The models the service that is this provider lists, by its id.
    ProviderModels(String),
    /// Whether this provider answers as it is set up, by its id. Slow:
    /// asked on a thread of its own (`slow`).
    CheckProvider(String),
    OcrStatus,
    OcrInstall,
    ClipboardSources,
    ClipboardMeaningStatus,
    ClipboardMeaningInstall,
    /// By the extension's id.
    ExtensionPermissions(String),
    RevokeExtensionPermission { extension: String, permission: permissions::Permission },
    /// The connected accounts, with how their syncing goes.
    Connections,
    /// The connectors there are, with their fields and presets.
    ConnectorKinds,
    /// An account of this connector (`<extension>:<command>`), named so,
    /// with the values of its fields and its secrets, by name.
    AddConnection { connector: String, name: String, config: Map<String, Value>, secrets: std::collections::BTreeMap<String, String> },
    /// What changes of an account: its name if given, the fields and the
    /// secrets given (an empty secret is one removed).
    EditConnection { id: String, name: Option<String>, config: Map<String, Value>, secrets: std::collections::BTreeMap<String, String> },
    /// By its id: it goes with its secrets and its items.
    RemoveConnection(String),
    /// Whether the account works as it is set up, by its id. Slow.
    CheckConnection(String),
    /// Whether an account of this connector would work set up so, before
    /// it is added; or, `of` an account there is, as it would be changed. Slow.
    TryConnection { connector: String, of: Option<String>, config: Map<String, Value>, secrets: std::collections::BTreeMap<String, String> },
    /// Syncs the account now, by its id.
    SyncConnection(String),
    /// What connectors synced that this finds.
    SearchConnections(String),
}

impl Command {
    /// Whether it may take seconds: a service asked over the network.
    pub(crate) fn slow(&self) -> bool {
        matches!(self, Command::CheckProvider(_) | Command::ProviderModels(_) | Command::CheckConnection(_) | Command::TryConnection { .. })
    }

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
            Command::SpeechModels => "speech_models",
            Command::DownloadSpeechModel(_) => "download_speech_model",
            Command::CancelSpeechDownload(_) => "cancel_speech_download",
            Command::DeleteSpeechModel(_) => "delete_speech_model",
            Command::CopyDiagnostics => "copy_diagnostics",
            Command::CheckForUpdate => "check_for_update",
            Command::InstallUpdate => "install_update",
            Command::AddProvider { .. } => "add_provider",
            Command::SetProviderField { .. } => "set_provider_field",
            Command::RemoveProvider(_) => "remove_provider",
            Command::OpenExtensionsFolder => "open_extensions_folder",
            Command::SetExtensionPreferences { .. } => "set_extension_preferences",
            Command::UninstallExtension(_) => "uninstall_extension",
            Command::ClearClipboardHistory => "clear_clipboard_history",
            Command::ProviderModels(_) => "provider_models",
            Command::CheckProvider(_) => "check_provider",
            Command::OcrStatus => "ocr_status",
            Command::OcrInstall => "ocr_install",
            Command::ClipboardSources => "clipboard_sources",
            Command::ClipboardMeaningStatus => "clipboard_meaning_status",
            Command::ClipboardMeaningInstall => "clipboard_meaning_install",
            Command::ExtensionPermissions(_) => "extension_permissions",
            Command::RevokeExtensionPermission { .. } => "revoke_extension_permission",
            Command::Connections => "connections",
            Command::ConnectorKinds => "connector_kinds",
            Command::AddConnection { .. } => "add_connection",
            Command::EditConnection { .. } => "edit_connection",
            Command::RemoveConnection(_) => "remove_connection",
            Command::CheckConnection(_) => "check_connection",
            Command::TryConnection { .. } => "try_connection",
            Command::SyncConnection(_) => "sync_connection",
            Command::SearchConnections(_) => "search_connections",
        }
    }
}

/// The events it listens to.
const EVENTS: &[&str] = &[event::SETTINGS_CHANGED, event::INDEX_READY, event::DICTATION_INSTALL, event::OCR_INSTALL, event::CLIPBOARD_MEANING_INSTALL];

/// Heyo itself.
pub(crate) struct Running(pub AppHandle);

impl Host for Running {
    fn call(&self, command: Command) -> Result<Answer, String> {
        let app = self.0.clone();
        let settings = |view: settings::SettingsView| Answer::Settings(Box::new(view));
        Ok(match command {
            Command::GetSettings => settings(settings::settings_for_its_window(&app)),
            Command::ChangeSettings(changes) => settings(settings::change_settings(changes, app)?),
            Command::GetHotkeys => Answer::Hotkeys(block_on(settings::get_hotkeys())),
            Command::SetHotkey { name, accelerator } => Answer::Hotkeys(block_on(settings::set_hotkey(name, accelerator))?),
            Command::SetItemHotkey { id, title, accelerator } => block_on(settings::set_item_hotkey(id, title, accelerator, app)).map(|()| Answer::Done)?,
            Command::ItemHotkeys => Answer::ItemHotkeys(settings::item_hotkeys(app.state()).into_iter().collect()),
            Command::ListExtensions => Answer::Extensions(settings::list_extensions()),
            Command::ListApplications => Answer::Applications(root::list_applications(app).into_iter().map(Into::into).collect()),
            Command::GetCapabilities => Answer::Capabilities(settings::get_capabilities()),
            Command::InstallShellExtension => Answer::Capabilities(settings::install_shell_extension()?),
            Command::DictationStatus => Answer::Dictation(answer::read(media::dictation_status(app))?),
            Command::SpeechModels => Answer::Speech(answer::read(crate::dictation::models(&app))?),
            Command::DownloadSpeechModel(id) => crate::dictation::download(&app, &id).map(|()| Answer::Done)?,
            Command::CancelSpeechDownload(id) => crate::dictation::cancel_download(&app, &id).map(|()| Answer::Done)?,
            Command::DeleteSpeechModel(id) => crate::dictation::delete(&app, &id).map(|()| Answer::Done)?,
            Command::CopyDiagnostics => settings::copy_diagnostics(app).map(|()| Answer::Done)?,
            Command::CheckForUpdate => Answer::Update(block_on(updates::check_for_update(app))?),
            Command::InstallUpdate => block_on(updates::install_update(app)).map(|()| Answer::Done)?,
            Command::AddProvider { kind, name, config } => settings(settings::add_provider(kind, name, config, app)?),
            Command::SetProviderField { provider, field, value } => settings(settings::set_provider_field(provider, field, value, app)?),
            Command::RemoveProvider(id) => settings(settings::remove_provider(id, app)?),
            Command::OpenExtensionsFolder => settings::open_extensions_folder().map(|()| Answer::Done)?,
            Command::SetExtensionPreferences { extension, values } => settings::set_extension_preferences(extension, values).map(|()| Answer::Done)?,
            Command::UninstallExtension(id) => settings::uninstall_extension(id, app).map(|()| Answer::Done)?,
            Command::ClearClipboardHistory => {
                settings::clear_clipboard_history(app.state());
                Answer::Done
            }
            Command::ProviderModels(id) => Answer::Models(settings::provider_models(id, app)?),
            Command::CheckProvider(id) => Answer::Checked(settings::check_provider(id, app)?),
            Command::OcrStatus => Answer::Ocr(media::ocr_status()),
            Command::OcrInstall => media::ocr_install(app).map(|()| Answer::Done)?,
            Command::ClipboardSources => Answer::Sources(clipboard::clipboard_sources(app)),
            Command::ClipboardMeaningStatus => Answer::Meaning(clipboard::clipboard_meaning_status(app)),
            Command::ClipboardMeaningInstall => clipboard::clipboard_meaning_install(app).map(|()| Answer::Done)?,
            Command::ExtensionPermissions(id) => Answer::Permissions(permissions::extension_permissions(id)),
            Command::RevokeExtensionPermission { extension, permission } => permissions::revoke_extension_permission(extension, permission).map(|()| Answer::Done)?,
            Command::Connections => Answer::Connections(connections::list(&app)),
            Command::ConnectorKinds => Answer::ConnectorKinds(connections::kinds()),
            Command::AddConnection { connector, name, config, secrets } => Answer::Connections(connections::add(&app, connector, name, config, secrets)?),
            Command::EditConnection { id, name, config, secrets } => Answer::Connections(connections::edit(&app, id, name, config, secrets)?),
            Command::RemoveConnection(id) => Answer::Connections(connections::remove(&app, id)?),
            Command::CheckConnection(id) => connections::check(&app, id).map(|()| Answer::Done)?,
            Command::TryConnection { connector, of, config, secrets } => connections::try_out(&app, connector, of, config, secrets).map(|()| Answer::Done)?,
            Command::SyncConnection(id) => connections::sync_now(&app, id).map(|()| Answer::Done)?,
            Command::SearchConnections(query) => Answer::Found(connections::search(&app, &query)),
        })
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
        let (none, nothing) = (String::new, PreferenceValues::default());
        let changing = [
            Command::ChangeSettings(Map::new()),
            Command::AddProvider { kind: none(), name: none(), config: Default::default() },
            Command::SetProviderField { provider: none(), field: none(), value: none() },
            Command::RemoveProvider(none()),
            Command::SetExtensionPreferences { extension: none(), values: nothing },
            Command::UninstallExtension(none()),
            Command::RevokeExtensionPermission { extension: none(), permission: permissions::Permission::Windows },
            Command::InstallUpdate,
            Command::AddConnection { connector: none(), name: none(), config: Map::new(), secrets: Default::default() },
            Command::EditConnection { id: none(), name: None, config: Map::new(), secrets: Default::default() },
            Command::RemoveConnection(none()),
            Command::SyncConnection(none()),
            Command::CheckConnection(none()),
            Command::TryConnection { connector: none(), of: None, config: Map::new(), secrets: Default::default() },
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

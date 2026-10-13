//! Commands for the settings window: settings, hotkeys, extensions.

use std::collections::HashMap;

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::*;

pub(crate) const SETTINGS_WINDOW: &str = "settings";

/// The models an OpenAI-compatible server on this computer offers; an error
/// says why it could not be asked.
pub(crate) async fn local_models(url: String) -> Result<Vec<String>, String> {
    let endpoint = format!("{}/models", url.trim_end_matches('/'));
    let output = tauri::async_runtime::spawn_blocking(move || {
        std::process::Command::new("curl").args(["-fsS", "--max-time", "3", "--proto", "=http,https", "--", &endpoint]).output()
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| format!("could not run curl: {e}"))?;
    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        // "curl: (7) Failed to connect to …" → the part after the code.
        return Err(error.trim().split_once(") ").map_or(error.trim(), |(_, reason)| reason).to_owned());
    }
    let body: Value = serde_json::from_slice(&output.stdout).map_err(|_| "the server did not answer with a model list".to_owned())?;
    Ok(body["data"].as_array().into_iter().flatten().filter_map(|m| m["id"].as_str().map(str::to_owned)).collect())
}

/// Makes `next` the settings: saves them, does what the change calls for
/// (`settings::consequences`), and tells every window. The one way settings
/// change.
fn commit(app: &AppHandle, next: Settings) -> Result<SettingsView, String> {
    let state = app.state::<SettingsState>();
    let previous = state.0.read().unwrap().clone();
    next.save()?;
    *state.0.write().unwrap() = next.clone();
    for consequence in settings::consequences(&previous, &next, settings::node_given().is_some()) {
        carry_out(app, &next, consequence);
    }
    let view = view(app);
    // Every window hears this, so it carries what every window may know;
    // the settings window asks for the rest (`get_settings`).
    let _ = app.emit(crate::wire::event::SETTINGS_CHANGED, view.clone().for_window("*"));
    Ok(view)
}

fn carry_out(app: &AppHandle, next: &Settings, consequence: settings::Consequence) {
    use settings::Consequence::*;
    match consequence {
        RestartRuntime => app.state::<Runtime>().stop(),
        RestartBackground => menubar::restart(app),
        ReloadCommands => reload_commands(app),
        RefreshIndex => {
            let _ = app.emit(crate::wire::event::INDEX_READY, ());
        }
        TellAppearance => {
            // Neither running: nobody to tell.
            let _ = crate::runtime::tell_running(app, &HostMessage::Appearance { dark: next.dark() }.json());
        }
        TellLanguage => {
            if crate::i18n::choose(&next.language) {
                tell_language(app);
            }
        }
        PruneClipboard { days } => app.state::<ClipboardHistory>().prune(days),
        WakeMeaning => app.state::<ClipboardHistory>().wake_meaning(),
        AtNextStart => eprintln!("heyo: the renderer changes when Heyo is next started"),
    }
}

/// Has everything that shows text say it in the language now spoken: the
/// windows Slint draws, the palette, and whichever runtime is running (one
/// started later is told as it starts).
fn tell_language(app: &AppHandle) {
    slint_windows::speak();
    let translations = crate::i18n::translations();
    let _ = crate::runtime::tell_running(app, &HostMessage::Language { translations: translations.clone() }.json());
    let _ = app.emit(crate::wire::event::LANGUAGE_CHANGED, translations);
}

/// The language Heyo speaks, for a web view to say its own text in.
#[tauri::command]
pub(crate) fn translations() -> crate::i18n::Translations {
    crate::i18n::translations()
}

/// Held from reading the settings to committing the change made to them:
/// changes come from several threads, and none may undo another.
static CHANGING: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn changing() -> std::sync::MutexGuard<'static, ()> {
    CHANGING.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Changes the settings from within Heyo (favorites and aliases, from the
/// palette).
pub(crate) fn update_settings(app: &AppHandle, change: impl FnOnce(&mut Settings)) -> Result<(), String> {
    let _changing = changing();
    let mut next = app.state::<SettingsState>().0.read().unwrap().clone();
    change(&mut next);
    commit(app, next).map(|_| ())
}

/// Settings plus derived, read-only state for the settings page.
#[derive(Clone, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../shared/bindings/")]
pub(crate) struct SettingsView {
    #[serde(flatten)]
    settings: Settings,
    /// Whether a Claude API key is stored. The key itself never goes to a
    /// window (`anthropicApiKey` is always empty here).
    anthropic_api_key_set: bool,
    launch_at_login: bool,
    node_version: Result<String, String>,
    node_overridden: bool,
    extensions_dir: String,
    settings_path: String,
    version: String,
    /// Whether this copy of Heyo can look for and install a newer version.
    can_update: bool,
}

impl SettingsView {
    /// The view as the window of that label may see it. Where Heyo keeps
    /// its files (the settings file holds the API key) is for the settings
    /// window alone; the palette draws what extensions give it.
    fn for_window(mut self, label: &str) -> SettingsView {
        if label != SETTINGS_WINDOW {
            self.extensions_dir.clear();
            self.settings_path.clear();
        }
        self
    }
}

fn view(app: &AppHandle) -> SettingsView {
    let mut settings = app.state::<SettingsState>().0.read().unwrap().clone();
    let anthropic_api_key_set = !settings.anthropic_api_key.is_empty();
    settings.anthropic_api_key.clear();
    SettingsView {
        anthropic_api_key_set,
        node_version: settings::node_version(&settings.node()),
        node_overridden: std::env::var_os("HEYO_NODE").is_some(),
        launch_at_login: settings::launch_at_login(),
        extensions_dir: paths::extensions().to_string_lossy().into_owned(),
        settings_path: settings::settings_path().to_string_lossy().into_owned(),
        version: app.package_info().version.to_string(),
        can_update: crate::updates::available(&app),
        settings,
    }
}

/// What `get_settings` answers the settings window, for one that is not a
/// web view.
pub(crate) fn settings_for_its_window(app: &AppHandle) -> SettingsView {
    view(app).for_window(SETTINGS_WINDOW)
}

// Async: the Node.js version is asked of Node.js.
#[tauri::command(async)]
pub(crate) fn get_settings(app: AppHandle, window: tauri::WebviewWindow) -> SettingsView {
    view(&app).for_window(window.label())
}

/// Changes some settings: `changes` holds the new values of the ones that
/// change, by name, and nothing else. `launchAtLogin` may be among them.
pub(crate) fn change_settings(mut changes: serde_json::Map<String, Value>, app: AppHandle) -> Result<SettingsView, String> {
    // The key has a command of its own, which is the only way it is set.
    if changes.contains_key("anthropicApiKey") {
        return Err("the API key is not set this way".into());
    }
    if let Some(enabled) = changes.remove("launchAtLogin") {
        let enabled = enabled.as_bool().ok_or("launchAtLogin is on or off")?;
        if enabled != settings::launch_at_login() {
            settings::set_launch_at_login(enabled)?;
        }
    }
    let _changing = changing();
    let next = app.state::<SettingsState>().0.read().unwrap().changed(&changes)?;
    commit(&app, next)
}

/// What the running session can do, for explaining what is unavailable.
pub(crate) fn get_capabilities() -> platform::Capabilities {
    platform::capabilities()
}

/// Installs the GNOME Shell extension that Wayland sessions need.
pub(crate) fn install_shell_extension() -> Result<platform::Capabilities, String> {
    platform::install_extension()?;
    Ok(platform::capabilities())
}

/// Stores the Claude API key; an empty one removes it.
pub(crate) fn set_anthropic_api_key(key: String, app: AppHandle) -> Result<SettingsView, String> {
    let _changing = changing();
    let mut next = app.state::<SettingsState>().0.read().unwrap().clone();
    next.anthropic_api_key = key.trim().to_owned();
    commit(&app, next)
}

/// Opens the settings window, or brings it back if it is already open.
// Async: creating a window from a synchronous command can deadlock.
#[tauri::command]
pub(crate) async fn open_settings(app: AppHandle) -> Result<(), String> {
    if crate::slint_windows::open(&app) {
        palette::hide(&app);
        return Ok(());
    }
    // What draws the settings is gone for this run, and cannot be started
    // twice in one. Heyo itself goes on; the palette says so, and offers
    // the one thing that brings the settings back.
    palette::bring_back(&app);
    let restarting = app.clone();
    let consent = crate::confirm::Consent {
        title: "Settings need Heyo to be restarted".into(),
        message: format!("The settings window stopped working ({}). Everything else goes on as it was.", crate::slint_windows::failure()),
        yes: "Restart Heyo",
        no: "Not Now",
    };
    crate::confirm::ask_consent(&app, consent, move |yes| {
        if yes {
            restarting.restart();
        }
    });
    Ok(())
}

#[derive(Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../shared/bindings/")]
pub(crate) struct Hotkeys {
    supported: bool,
    toggle: Option<String>,
    clipboard: Option<String>,
    dictation: Option<String>,
}

// Async so the `gsettings` calls stay off the main thread.
pub(crate) async fn get_hotkeys() -> Hotkeys {
    let supported = hotkeys::supported();
    Hotkeys {
        supported,
        toggle: supported.then(|| hotkeys::get(&hotkeys::Target::toggle())).flatten(),
        clipboard: supported.then(|| hotkeys::get(&hotkeys::Target::clipboard_history())).flatten(),
        dictation: supported.then(|| hotkeys::get(&hotkeys::Target::dictation())).flatten(),
    }
}

/// The hotkeys that open Heyo itself, named as in `Hotkeys`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../shared/bindings/")]
pub(crate) enum HotkeyName {
    Toggle,
    Clipboard,
    Dictation,
}

pub(crate) async fn set_hotkey(name: HotkeyName, accelerator: String) -> Result<Hotkeys, String> {
    let target = match name {
        HotkeyName::Toggle => hotkeys::Target::toggle(),
        HotkeyName::Clipboard => hotkeys::Target::clipboard_history(),
        HotkeyName::Dictation => hotkeys::Target::dictation(),
    };
    hotkeys::set(&target, &accelerator)?;
    Ok(get_hotkeys().await)
}

/// Binds a global hotkey to a root search item; an empty accelerator removes it.
#[tauri::command]
pub(crate) async fn set_item_hotkey(id: String, title: String, accelerator: String, app: AppHandle) -> Result<(), String> {
    if !hotkeys::supported() {
        return Err("Hotkeys need GNOME. Bind a desktop shortcut to `heyo --open <id>` instead.".into());
    }
    hotkeys::set(&hotkeys::Target::item(&id, &title), &accelerator)?;
    let state = app.state::<ItemHotkeys>();
    let mut map = state.0.write().unwrap();
    if accelerator.is_empty() {
        map.remove(&id);
    } else {
        map.insert(id, accelerator);
    }
    drop(map);
    let _ = app.emit(crate::wire::event::INDEX_READY, ());
    Ok(())
}

pub(crate) fn item_hotkeys(hotkeys: State<ItemHotkeys>) -> HashMap<String, String> {
    hotkeys.0.read().unwrap().clone()
}

pub(crate) fn list_extensions() -> Vec<extensions::ExtensionInfo> {
    extensions::list()
}

pub(crate) fn set_extension_preferences(extension: String, values: extensions::PreferenceValues) -> Result<(), String> {
    extensions::save_preferences(&extension, &values)
}

pub(crate) fn uninstall_extension(id: String, app: AppHandle) -> Result<(), String> {
    let name = extensions::declared(std::path::Path::new(&id)).map(|declared| declared.name);
    extensions::uninstall(&id)?;
    // What it was allowed is not for another extension of its name.
    if let Some(name) = name {
        permissions::forget(&name);
    }
    reload_commands(&app);
    Ok(())
}

pub(crate) fn clear_clipboard_history(history: State<ClipboardHistory>) {
    history.clear();
}

pub(crate) fn open_extensions_folder() -> Result<(), String> {
    let dir = paths::extensions();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    crate::platform::common::open(&dir)
}

/// Puts a description of this Heyo and where it runs on the clipboard, for
/// a bug report: versions, the session, the end of the log, secrets removed.
pub(crate) fn copy_diagnostics(app: AppHandle) -> Result<(), String> {
    crate::platform::clipboard_watch::write_text(&diagnostics::report(&app))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_settings_window_learns_where_the_files_are() {
        let view = SettingsView {
            settings: Settings { anthropic_api_key: String::new(), compact_mode: true, ..Settings::default() },
            anthropic_api_key_set: true,
            launch_at_login: false,
            node_version: Ok("v24.0.0".into()),
            node_overridden: false,
            extensions_dir: "/home/ada/.local/share/heyo/extensions".into(),
            settings_path: "/home/ada/.config/heyo/settings.json".into(),
            version: "0.1.0".into(),
            can_update: false,
        };
        let whole = view.clone().for_window(SETTINGS_WINDOW);
        assert_eq!(whole.settings_path, "/home/ada/.config/heyo/settings.json");
        assert_eq!(whole.extensions_dir, "/home/ada/.local/share/heyo/extensions");
        for label in ["main", "screenshot", "pin-1", "*", ""] {
            let seen = view.clone().for_window(label);
            assert_eq!((seen.settings_path.as_str(), seen.extensions_dir.as_str()), ("", ""), "{label}");
            // What the palette lays itself out by is still there.
            assert!(seen.settings.compact_mode && seen.anthropic_api_key_set);
        }
    }
}

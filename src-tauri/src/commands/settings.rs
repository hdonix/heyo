//! Commands for the settings window: settings, hotkeys, extensions.

use crate::i18n::{t, tf};
use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::*;
use crate::models::registry::{self, Provider};

pub(crate) const SETTINGS_WINDOW: &str = "settings";

/// The provider `id` as it is used, secrets and all: for the host alone.
fn saved_provider(app: &AppHandle, id: &str) -> Result<Provider, String> {
    let provider = app.state::<SettingsState>().0.read().unwrap().providers_in_use().into_iter().find(|provider| provider.id == id);
    provider.ok_or_else(|| tf("There is no provider {0}.", &[&id]))
}

/// The models the service that is the provider `id` lists; an error says
/// why it could not be asked. Blocks.
pub(crate) fn provider_models(id: String, app: AppHandle) -> Result<Vec<String>, String> {
    let provider = saved_provider(&app, &id)?;
    provider.kind().list(&provider)
}

/// Whether the provider `id` answers as it is set up, in words, and the
/// models it listed if it lists them. Blocks, up to some seconds.
pub(crate) fn check_provider(id: String, app: AppHandle) -> Result<crate::models::kinds::Checked, String> {
    let provider = saved_provider(&app, &id)?;
    provider.kind().check(&provider)
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
                // The root search says its commands in the language spoken.
                reload_commands(app);
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

/// Changes the settings as `change` does, if it can, and says what it
/// answered: one change of several of the settings (a connection, with
/// its secrets).
pub(crate) fn change_with<T>(app: &AppHandle, change: impl FnOnce(&mut Settings) -> Result<T, String>) -> Result<T, String> {
    let _changing = changing();
    let mut next = app.state::<SettingsState>().0.read().unwrap().clone();
    let answer = change(&mut next)?;
    commit(app, next)?;
    Ok(answer)
}

/// Changes the settings from within Heyo (favorites and aliases, from the
/// palette).
pub(crate) fn update_settings(app: &AppHandle, change: impl FnOnce(&mut Settings)) -> Result<(), String> {
    let _changing = changing();
    let mut next = app.state::<SettingsState>().0.read().unwrap().clone();
    change(&mut next);
    commit(app, next).map(|_| ())
}

/// What the settings window is told of a provider besides its settings.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../shared/bindings/")]
pub(crate) struct ProviderState {
    /// Which of its secret fields hold something. What they hold never
    /// goes to a window: there they are empty.
    pub secrets_set: Vec<String>,
    /// For a command-line tool: where it is and its version, or why there
    /// is none to run.
    #[serde(default, with = "found")]
    #[ts(as = "Option<Result<crate::models::cli::Found, String>>")]
    pub cli_found: Option<Result<crate::models::cli::Found, String>>,
}

/// A tool's `cli_found` as it is written, and as a window reads it back
/// (the macOS settings window, a process of its own, is sent the view as
/// JSON).
mod found {
    use crate::models::cli::Found;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub(super) fn serialize<S: Serializer>(found: &Option<Result<Found, String>>, to: S) -> Result<S::Ok, S::Error> {
        found.serialize(to)
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(from: D) -> Result<Option<Result<Found, String>>, D::Error> {
        #[derive(Deserialize)]
        struct Read {
            path: String,
            version: String,
        }
        let found: Option<Result<Read, String>> = Deserialize::deserialize(from)?;
        Ok(found.map(|found| found.map(|Read { path, version }| Found { path, version })))
    }
}

/// Settings plus derived, read-only state for the settings page.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../shared/bindings/")]
pub(crate) struct SettingsView {
    #[serde(flatten)]
    pub settings: Settings,
    /// Each provider's, by its id.
    pub provider_states: std::collections::BTreeMap<String, ProviderState>,
    pub launch_at_login: bool,
    pub node_version: Result<String, String>,
    pub node_overridden: bool,
    pub extensions_dir: String,
    pub settings_path: String,
    pub version: String,
    /// Whether this copy of Heyo can look for and install a newer version.
    pub can_update: bool,
}

/// Nothing told yet: the settings as they are when none are saved.
impl Default for SettingsView {
    fn default() -> SettingsView {
        SettingsView {
            settings: Settings::default(),
            provider_states: Default::default(),
            launch_at_login: false,
            node_version: Err(String::new()),
            node_overridden: false,
            extensions_dir: String::new(),
            settings_path: String::new(),
            version: String::new(),
            can_update: false,
        }
    }
}

impl SettingsView {
    /// The view as the window of that label may see it. Where Heyo keeps
    /// its files (the settings file holds the API key), and where the
    /// tools it runs are, is for the settings window alone; the palette
    /// draws what extensions give it.
    fn for_window(mut self, label: &str) -> SettingsView {
        if label != SETTINGS_WINDOW {
            self.extensions_dir.clear();
            self.settings_path.clear();
            for state in self.provider_states.values_mut() {
                state.cli_found = None;
            }
        }
        self
    }
}

/// The settings as a window may see them: without the secrets, and with
/// which of each provider's are set said instead.
fn without_secrets(settings: &mut Settings) -> std::collections::BTreeMap<String, ProviderState> {
    let secrets = std::mem::take(&mut settings.secrets);
    settings
        .providers
        .iter_mut()
        .map(|provider| {
            // Nothing of a secret field is shown, whatever put it there.
            for field in provider.kind().fields().iter().filter(|field| field.entry == registry::Entry::Secret) {
                provider.config.remove(field.name);
            }
            let state = ProviderState { secrets_set: secrets.names(crate::secrets::Owner::Provider(&provider.id)), cli_found: provider.kind().found(provider) };
            (provider.id.clone(), state)
        })
        .collect()
}

fn view(app: &AppHandle) -> SettingsView {
    let mut settings = app.state::<SettingsState>().0.read().unwrap().clone();
    SettingsView {
        provider_states: without_secrets(&mut settings),
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
    // The providers have commands of their own: a window never has their
    // secrets to send back.
    for own in ["providers", "connections", "secrets"] {
        if changes.contains_key(own) {
            return Err(format!("the {own} are not changed this way"));
        }
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

/// Changes the providers and their secrets as `change` does, if it can.
fn change_providers(app: &AppHandle, change: impl FnOnce(&mut Vec<Provider>, &mut crate::secrets::Secrets) -> Result<(), String>) -> Result<SettingsView, String> {
    let _changing = changing();
    let mut next = app.state::<SettingsState>().0.read().unwrap().clone();
    change(&mut next.providers, &mut next.secrets)?;
    commit(app, next)
}

/// Connects a new provider of `kind` (by its word), named `name`, set up
/// with `config` (its fields by name), as the settings window's form for a
/// new one says. Its id is made here.
pub(crate) fn add_provider(kind: String, name: String, config: std::collections::BTreeMap<String, String>, app: AppHandle) -> Result<SettingsView, String> {
    let known = crate::models::kinds::addable().find(|known| known.word() == kind).ok_or_else(|| tf("{0} is not a provider that can be added.", &[&kind]))?;
    change_providers(&app, |providers, secrets| {
        let id = registry::new_id(providers, known.word());
        providers.push(Provider::new(&id, known.word()));
        if !name.trim().is_empty() {
            registry::set_field(providers, secrets, &id, "name", &name)?;
        }
        for (field, value) in &config {
            registry::set_field(providers, secrets, &id, field, value)?;
        }
        Ok(())
    })
}

/// Sets a field of a provider's configuration, or its name (`name`). A
/// secret is never shown again; an empty one is removed.
pub(crate) fn set_provider_field(provider: String, field: String, value: String, app: AppHandle) -> Result<SettingsView, String> {
    change_providers(&app, |providers, secrets| registry::set_field(providers, secrets, &provider, &field, &value))
}

/// Disconnects a provider. What refers to it says it is gone.
pub(crate) fn remove_provider(id: String, app: AppHandle) -> Result<SettingsView, String> {
    change_providers(&app, |providers, secrets| registry::remove(providers, secrets, &id))
}

/// Opens the settings window, or brings it back if it is already open.
// Async: creating a window from a synchronous command can deadlock.
#[tauri::command]
pub(crate) async fn open_settings(app: AppHandle) -> Result<(), String> {
    if crate::slint_windows::open(&app) {
        #[cfg(not(target_os = "macos"))]
        palette::hide(&app);
        return Ok(());
    }
    // What draws the settings is gone for this run, and cannot be started
    // twice in one. Heyo itself goes on; the palette says so, and offers
    // the one thing that brings the settings back.
    palette::bring_back(&app);
    let restarting = app.clone();
    let consent = crate::confirm::Consent {
        title: t("Settings need Heyo to be restarted").into(),
        message: tf("The settings window stopped working ({0}). Everything else goes on as it was.", &[&crate::slint_windows::failure()]),
        yes: t("Restart Heyo"),
        no: t("Not Now"),
    };
    crate::confirm::ask_consent(&app, consent, move |yes| {
        if yes {
            restarting.restart();
        }
    });
    Ok(())
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../shared/bindings/")]
pub(crate) struct Hotkeys {
    pub supported: bool,
    pub toggle: Option<String>,
    pub clipboard: Option<String>,
    pub dictation: Option<String>,
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
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
        return Err(t("Hotkeys need GNOME. Bind a desktop shortcut to `heyo --open <id>` instead.").into());
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
        let found = Some(Ok(crate::models::cli::Found { path: "/home/ada/.local/bin/claude".into(), version: "2.1.302 (Claude Code)".into() }));
        let view = SettingsView {
            settings: Settings { compact_mode: true, ..Settings::default() },
            provider_states: [("cc".to_string(), ProviderState { secrets_set: vec!["key".into()], cli_found: found })].into(),
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
        assert!(whole.provider_states["cc"].cli_found.is_some());
        for label in ["main", "screenshot", "pin-1", "*", ""] {
            let seen = view.clone().for_window(label);
            assert_eq!((seen.settings_path.as_str(), seen.extensions_dir.as_str(), &seen.provider_states["cc"].cli_found), ("", "", &None), "{label}");
            // What the palette lays itself out by is still there.
            assert!(seen.settings.compact_mode && seen.provider_states["cc"].secrets_set == ["key"]);
        }
    }

    #[test]
    fn no_window_is_told_a_secret_only_that_it_is_set() {
        use crate::secrets::Owner;
        let mut settings = Settings::default();
        settings.secrets.store(Owner::Provider("claude"), registry::KEY, "sk-ant-secret");
        settings.secrets.store(Owner::Connection("work"), "password", "hunter2");
        let mut server = Provider::new("ollama", crate::models::kinds::OPENAI_COMPATIBLE);
        server.config.insert(registry::URL.to_string(), "http://127.0.0.1:11434/v1".to_string());
        settings.providers.push(server);
        // What is used has the key with it.
        assert_eq!(settings.providers_in_use()[1].get(registry::KEY), "sk-ant-secret");
        // A secret field in a configuration, however it got there, is not shown either.
        settings.providers[1].config.insert(registry::KEY.into(), "sk-ant-stray".into());
        let states = without_secrets(&mut settings);
        assert_eq!(settings.providers[1].get(registry::KEY), "");
        assert_eq!(settings.providers[2].get(registry::URL), "http://127.0.0.1:11434/v1");
        assert_eq!((states["claude"].secrets_set.as_slice(), states["ollama"].secrets_set.as_slice()), (&["key".to_string()][..], &[][..]));
        let shown = serde_json::to_string(&settings).unwrap();
        assert!(!shown.contains("sk-ant") && !shown.contains("hunter2") && !shown.contains("secrets"), "{shown}");
    }
}

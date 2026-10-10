//! User settings, stored in `$XDG_CONFIG_HOME/heyo/settings.json`.
//!
//! `Settings` is the one declaration of what a setting is. The windows'
//! TypeScript type is generated from it (`shared/bindings/`, written by
//! `cargo test`), and a change arrives as the fields that changed
//! (`Settings::changed`), so a field need be named nowhere else to be saved.
//!
//! "Launch at login" is not stored: it is whether the autostart entry exists.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The colour scheme of the interface.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../shared/bindings/")]
pub enum Appearance {
    /// Whichever the system is using.
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
#[ts(export, export_to = "../../shared/bindings/")]
pub struct Settings {
    pub appearance: Appearance,
    /// Hide the window when it loses focus.
    pub hide_on_blur: bool,
    /// Show only the search field until something is typed.
    pub compact_mode: bool,
    /// Reopening the window returns to the running command instead of the
    /// root search.
    pub keep_command: bool,
    /// Node.js executable for extensions; empty means `node` from `PATH`.
    pub node_path: String,
    /// Record clipboard history.
    pub clipboard_history: bool,
    /// Days unpinned clipboard entries are kept.
    pub clipboard_retention_days: u32,
    /// Command ids (`Command::id`) hidden from the root search.
    pub disabled_commands: Vec<String>,
    /// Root search item ids pinned to the top, in order.
    pub favorites: Vec<String>,
    /// Root search item id → alias.
    pub aliases: BTreeMap<String, String>,
    /// Dictation language: `auto`, `zh`, `en`, `yue`, `ja` or `ko`.
    pub dictation_language: String,
    /// PulseAudio source to record from; empty means the default.
    pub dictation_source: String,
    /// What Claude does with the transcript: `off`, `clean` or `translate`.
    pub dictation_polish: String,
    /// A key that dictates while it is held (`talkkey::KEYS`); empty for none.
    pub dictation_hold_key: String,
    /// Claude API key for polishing; empty uses the environment's credentials.
    pub anthropic_api_key: String,
    /// Which model Heyo's AI features use: `claude` or `local`.
    pub ai_provider: String,
    /// Base URL of an OpenAI-compatible server on this computer.
    pub local_model_url: String,
    /// Model to ask that server for; empty means the first one it lists.
    pub local_model_name: String,
    /// Clipboard history is not recorded for now (`clipboard_history` is
    /// whether it is recorded at all).
    pub clipboard_paused: bool,
    /// Applications whose copies are never recorded: the id of an installed
    /// one (its desktop file), or a window class.
    pub clipboard_ignored_apps: Vec<String>,
    /// Words dictation should write as given, one each: `TypeScript`, or
    /// `heard as -> written as`.
    pub dictation_vocabulary: Vec<String>,
    /// Ask before pasting several lines into a terminal, where each line
    /// runs as a command.
    pub confirm_terminal_paste: bool,
    /// Replace a snippet's keyword with its text as it is typed in any
    /// application. While on, Heyo follows what is typed (`expand`).
    pub snippet_expansion: bool,
    /// Applications snippets are never expanded in, named as in
    /// `clipboard_ignored_apps`. Password managers need not be listed.
    pub snippet_expansion_ignored_apps: Vec<String>,
    /// Find clipboard entries by what they mean as well as by their words,
    /// with a language model on this computer (downloaded when asked for).
    pub clipboard_meaning: bool,
}

impl Settings {
    /// What the helpers need to reach the configured model.
    pub fn ai(&self) -> serde_json::Value {
        serde_json::json!({
            "provider": if self.ai_provider == "local" { "local" } else { "claude" },
            "apiKey": self.anthropic_api_key,
            "localUrl": self.local_model_url,
            "localModel": self.local_model_name,
        })
    }
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            appearance: Appearance::System,
            hide_on_blur: true,
            compact_mode: false,
            keep_command: false,
            node_path: String::new(),
            clipboard_history: true,
            clipboard_retention_days: 90,
            disabled_commands: Vec::new(),
            favorites: Vec::new(),
            aliases: BTreeMap::new(),
            dictation_language: "auto".into(),
            dictation_source: String::new(),
            dictation_polish: "off".into(),
            dictation_hold_key: String::new(),
            anthropic_api_key: String::new(),
            ai_provider: "claude".into(),
            local_model_url: "http://127.0.0.1:8000/v1".into(),
            local_model_name: String::new(),
            clipboard_paused: false,
            clipboard_ignored_apps: Vec::new(),
            dictation_vocabulary: Vec::new(),
            confirm_terminal_paste: true,
            snippet_expansion: false,
            snippet_expansion_ignored_apps: Vec::new(),
            clipboard_meaning: false,
        }
    }
}

pub fn settings_path() -> PathBuf {
    crate::paths::settings()
}

#[cfg(not(windows))]
fn autostart_path() -> PathBuf {
    crate::paths::config_home().join("autostart/heyo.desktop")
}

impl Settings {
    /// These settings with some fields changed: `changes` holds the new
    /// values under the names the settings are stored by. Every other field
    /// stays as it is. A name that is not a setting, or a value of the wrong
    /// kind, is an error and changes nothing.
    pub fn changed(&self, changes: &serde_json::Map<String, serde_json::Value>) -> Result<Settings, String> {
        let serde_json::Value::Object(mut fields) = serde_json::to_value(self).map_err(|e| e.to_string())? else {
            return Err("settings are not an object".into());
        };
        for (name, value) in changes {
            match fields.get_mut(name) {
                Some(field) => *field = value.clone(),
                None => return Err(format!("there is no setting called {name}")),
            }
        }
        serde_json::from_value(serde_json::Value::Object(fields)).map_err(|e| format!("not a valid value for a setting: {e}"))
    }

    pub fn load() -> Self {
        let (settings, kept) = Settings::load_from(&settings_path());
        if let Some(kept) = kept {
            crate::notify("Heyo's settings could not be read", &format!("Using defaults. The old file is kept as {}.", kept.display()));
        }
        settings
    }

    /// The settings in a file; the defaults if there is none. A file that
    /// cannot be read as settings is moved aside (and its new place
    /// returned): starting from defaults would overwrite it on the next
    /// save, and nothing should be lost.
    pub fn load_from(path: &Path) -> (Settings, Option<PathBuf>) {
        let Ok(text) = fs::read_to_string(path) else { return (Settings::default(), None) };
        match serde_json::from_str(&text) {
            Ok(settings) => (settings, None),
            Err(e) => {
                let kept = path.with_extension("json.bad");
                eprintln!("heyo: {} is not valid ({e}); kept as {}", path.display(), kept.display());
                let _ = fs::rename(path, &kept);
                (Settings::default(), Some(kept))
            }
        }
    }

    pub fn save(&self) -> Result<(), String> {
        self.save_to(&settings_path())
    }

    pub fn save_to(&self, path: &Path) -> Result<(), String> {
        fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        // Written beside the file and renamed over it, so a crash never leaves
        // half a file; private from the start, as it may hold an API key.
        let temporary = path.with_extension("json.tmp");
        let write = || -> std::io::Result<()> {
            use std::io::Write;
            let mut options = fs::OpenOptions::new();
            options.write(true).create(true).truncate(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&temporary)?;
            file.write_all(text.as_bytes())?;
            file.sync_all()?;
            fs::rename(&temporary, path)
        };
        write().map_err(|e| format!("failed to write {}: {e}", path.display()))
    }

    /// The Node.js executable; `HEYO_NODE` overrides the setting.
    pub fn node(&self) -> String {
        env::var("HEYO_NODE")
            .ok()
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| match self.node_path.trim() {
                // The one that came with Heyo, if one did; else whichever is installed.
                "" => bundled_node().unwrap_or_else(|| "node".into()),
                path => path.into(),
            })
    }
}

/// What Windows runs at sign-in to start Heyo without showing it. The path
/// is quoted: it usually has spaces in it.
#[cfg(any(windows, test))]
pub(crate) fn login_command(exe: &str) -> String {
    format!("\"{exe}\" --hidden")
}

pub fn launch_at_login() -> bool {
    #[cfg(windows)]
    return crate::platform::win::autostart::enabled();
    #[cfg(not(windows))]
    autostart_path().is_file()
}

pub fn set_launch_at_login(enabled: bool) -> Result<(), String> {
    // Windows has no autostart directory: it keeps a list in the registry.
    #[cfg(windows)]
    return crate::platform::win::autostart::set(enabled, &env::current_exe().map_err(|e| e.to_string())?);
    #[cfg(not(windows))]
    set_autostart_entry(enabled)
}

#[cfg(not(windows))]
fn set_autostart_entry(enabled: bool) -> Result<(), String> {
    let path = autostart_path();
    if !enabled {
        return match fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
            _ => Ok(()),
        };
    }
    let exe = env::current_exe().map_err(|e| e.to_string())?;
    let entry = format!(
        "[Desktop Entry]\nType=Application\nName=Heyo\nComment=Command palette launcher\n\
         Exec={} --hidden\nX-GNOME-Autostart-enabled=true\nNoDisplay=true\n",
        exe.display()
    );
    fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    fs::write(&path, entry).map_err(|e| format!("failed to write {}: {e}", path.display()))
}

/// `node --version`, or why it could not run.
/// The version of a Node.js executable, or why it cannot be used. Asked for
/// on every settings change, so the answer for the last path is remembered
/// for a while instead of starting Node each time.
/// The Node.js that sits beside Heyo's own executable.
fn bundled_node() -> Option<String> {
    let beside = env::current_exe().ok()?.parent()?.join(if cfg!(windows) { "node.exe" } else { "node" });
    beside.is_file().then(|| beside.to_string_lossy().into_owned())
}

pub fn node_version(node: &str) -> Result<String, String> {
    type Cached = (String, std::time::Instant, Result<String, String>);
    static LAST: std::sync::Mutex<Option<Cached>> = std::sync::Mutex::new(None);
    if let Some((path, at, result)) = &*LAST.lock().unwrap() {
        if path == node && at.elapsed() < std::time::Duration::from_secs(60) {
            return result.clone();
        }
    }
    let result = check_node(node);
    *LAST.lock().unwrap() = Some((node.to_owned(), std::time::Instant::now(), result.clone()));
    result
}

fn check_node(node: &str) -> Result<String, String> {
    let output = crate::platform::common::quiet(node).arg("--version").output().map_err(|e| format!("{node}: {e}"))?;
    if !output.status.success() {
        return Err(format!("{node} --version failed"));
    }
    let version = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let major: u32 = version.trim_start_matches('v').split('.').next().and_then(|m| m.parse().ok()).unwrap_or(0);
    if major < 22 {
        return Err(format!("{version} is too old; extensions need Node.js 22 or newer"));
    }
    Ok(version)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn changes(value: serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
        value.as_object().unwrap().clone()
    }

    #[test]
    fn a_change_leaves_every_other_setting_alone() {
        let before = Settings { compact_mode: true, favorites: vec!["app:a".into()], anthropic_api_key: "sk-secret".into(), ..Settings::default() };
        let after = before.changed(&changes(json!({ "appearance": "dark" }))).unwrap();
        assert_eq!(after, Settings { appearance: Appearance::Dark, ..before.clone() });
        // Several at once, by the names they are stored under.
        let after = before.changed(&changes(json!({ "hideOnBlur": false, "clipboardRetentionDays": 7 }))).unwrap();
        assert_eq!(after, Settings { hide_on_blur: false, clipboard_retention_days: 7, ..before.clone() });
        assert_eq!(before.changed(&changes(json!({}))).unwrap(), before);
    }

    #[test]
    fn what_is_not_a_setting_or_not_a_value_for_one_is_refused() {
        let settings = Settings::default();
        assert!(settings.changed(&changes(json!({ "compactmode": true }))).unwrap_err().contains("compactmode"));
        assert!(settings.changed(&changes(json!({ "appearance": "sepia" }))).is_err());
        assert!(settings.changed(&changes(json!({ "hideOnBlur": "yes" }))).is_err());
    }

    /// Every setting can be changed by its name: none is left off a list.
    #[test]
    fn every_setting_can_be_changed() {
        let defaults = serde_json::to_value(Settings::default()).unwrap();
        for (name, value) in defaults.as_object().unwrap() {
            let different = match value {
                serde_json::Value::Bool(b) => json!(!b),
                serde_json::Value::Number(n) => json!(n.as_u64().unwrap() + 1),
                serde_json::Value::String(s) if name == "appearance" => json!(if s == "dark" { "light" } else { "dark" }),
                serde_json::Value::String(s) => json!(format!("{s}x")),
                serde_json::Value::Array(_) => json!(["x"]),
                serde_json::Value::Object(_) => json!({ "x": "y" }),
                serde_json::Value::Null => continue,
            };
            let changed = Settings::default().changed(&changes(json!({ name: different.clone() }))).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(serde_json::to_value(&changed).unwrap()[name], different, "{name}");
        }
    }

    #[test]
    fn the_command_run_at_sign_in_quotes_the_path() {
        assert_eq!(login_command(r"C:\Program Files\Heyo\heyo.exe"), r#""C:\Program Files\Heyo\heyo.exe" --hidden"#);
    }

    #[test]
    fn saved_settings_are_loaded_again_and_a_broken_file_is_kept() {
        let dir = env::temp_dir().join(format!("heyo-settings-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("heyo/settings.json");
        assert_eq!(Settings::load_from(&path), (Settings::default(), None));

        let settings = Settings { appearance: Appearance::Light, aliases: BTreeMap::from([("app:a".into(), "a".into())]), ..Settings::default() };
        settings.save_to(&path).unwrap();
        assert_eq!(Settings::load_from(&path), (settings, None));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        // A file from an older version, with fields missing and one unknown.
        fs::write(&path, r#"{"hideOnBlur": false, "somethingOld": 1}"#).unwrap();
        assert_eq!(Settings::load_from(&path).0, Settings { hide_on_blur: false, ..Settings::default() });

        fs::write(&path, "{ not json").unwrap();
        let (loaded, kept) = Settings::load_from(&path);
        assert_eq!(loaded, Settings::default());
        assert_eq!(fs::read_to_string(kept.unwrap()).unwrap(), "{ not json");
        assert!(!path.exists());
        let _ = fs::remove_dir_all(&dir);
    }
}

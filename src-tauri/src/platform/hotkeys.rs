//! Global hotkeys as GNOME custom keyboard shortcuts.
//!
//! Wayland gives applications no global hotkeys, so Heyo's hotkeys are
//! desktop shortcuts that run `heyo` (toggling the running instance),
//! `heyo --clipboard`, or `heyo --open <item id>` for a hotkey bound to a
//! root search item (an application, command, quicklink or snippet). On GNOME
//! they are managed here through `gsettings`; an existing shortcut for the
//! same command (e.g. one made by hand) is reused rather than duplicated.

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
#[cfg(any(target_os = "linux", test))]
use std::env;
use std::hash::{Hash, Hasher};
#[cfg(any(target_os = "linux", test))]
use std::process::Command;

use super::desktop::desktop;
#[cfg(any(target_os = "linux", test))]
use super::desktop::Hotkeys;

#[cfg(any(target_os = "linux", test))]
const LIST_SCHEMA: &str = "org.gnome.settings-daemon.plugins.media-keys";
#[cfg(any(target_os = "linux", test))]
const ENTRY_SCHEMA: &str = "org.gnome.settings-daemon.plugins.media-keys.custom-keybinding";
#[cfg(any(target_os = "linux", test))]
const BASE_PATH: &str = "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings";

/// What a hotkey does: run Heyo with these arguments.
// The label and the path are for GNOME's settings; a desktop where Heyo
// keeps the hotkeys itself reads the arguments alone.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub struct Target {
    pub(super) args: Vec<String>,
    /// Name shown in GNOME's keyboard settings.
    label: String,
    /// Last component of the dconf path for a new shortcut.
    path_id: String,
}

impl Target {
    /// Shows or hides the palette.
    pub fn toggle() -> Self {
        Target { args: vec![], label: "Heyo".into(), path_id: "heyo-toggle".into() }
    }

    pub fn clipboard_history() -> Self {
        Target {
            args: vec!["--clipboard".into()],
            label: "Heyo Clipboard History".into(),
            path_id: "heyo-clipboard".into(),
        }
    }

    /// Starts or finishes voice dictation.
    pub fn dictation() -> Self {
        Target { args: vec!["--dictate".into()], label: "Heyo Dictation".into(), path_id: "heyo-dictation".into() }
    }

    /// Opens a root search item.
    pub fn item(id: &str, title: &str) -> Self {
        let mut hasher = DefaultHasher::new();
        id.hash(&mut hasher);
        Target {
            args: vec!["--open".into(), id.into()],
            label: format!("Heyo: {title}"),
            path_id: format!("heyo-item-{:016x}", hasher.finish()),
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "toggle" => Some(Self::toggle()),
            "clipboard" => Some(Self::clipboard_history()),
            "dictation" => Some(Self::dictation()),
            _ => None,
        }
    }

    #[cfg(any(target_os = "linux", test))]
    fn command(&self) -> String {
        let exe = env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|_| "heyo".into());
        self.command_for(&exe)
    }

    /// The shell command running `exe` with this target's arguments.
    #[cfg(any(target_os = "linux", test))]
    fn command_for(&self, exe: &str) -> String {
        shlex::try_join(std::iter::once(exe).chain(self.args.iter().map(String::as_str))).unwrap_or_else(|_| exe.into())
    }

    /// Whether a shortcut's command runs this hotkey's action: any `heyo`
    /// executable with exactly these arguments.
    #[cfg(any(target_os = "linux", test))]
    fn matches(&self, command: &str) -> bool {
        heyo_args(command).is_some_and(|args| args == self.args)
    }
}

/// The arguments of a shortcut command that runs a `heyo` executable.
#[cfg(any(target_os = "linux", test))]
fn heyo_args(command: &str) -> Option<Vec<String>> {
    let mut parts = shlex::split(command)?;
    if parts.is_empty() || std::path::Path::new(&parts[0]).file_name().is_none_or(|n| n != "heyo") {
        return None;
    }
    parts.remove(0);
    Some(parts)
}

// --- What the rest of Heyo asks ---------------------------------------------------

/// Whether hotkeys can be set from Heyo on this desktop.
pub fn supported() -> bool {
    desktop().hotkeys().is_some()
}

/// The hotkey's accelerator, e.g. `<Control>space`, if one is set.
pub fn get(target: &Target) -> Option<String> {
    desktop().hotkeys()?.get(target)
}

/// Accelerators of all `heyo --open <id>` hotkeys, by item id.
pub fn item_hotkeys() -> HashMap<String, String> {
    desktop().hotkeys().map(|hotkeys| hotkeys.items()).unwrap_or_default()
}

/// Sets the accelerator; an empty one removes the hotkey.
pub fn set(target: &Target, accelerator: &str) -> Result<(), String> {
    desktop().hotkeys().ok_or_else(|| super::unsupported("Setting hotkeys"))?.set(target, accelerator)
}

/// Starts acting on the hotkeys, where Heyo itself has to (elsewhere the
/// desktop runs `heyo` for them).
pub fn start(app: &tauri::AppHandle) {
    if let Some(hotkeys) = desktop().hotkeys() {
        hotkeys.start(app);
    }
}

// --- GNOME's custom keyboard shortcuts, through gsettings ---------------------------

/// GNOME's custom shortcuts: the same on X11 and Wayland.
#[cfg(any(target_os = "linux", test))]
pub struct GSettings;

#[cfg(any(target_os = "linux", test))]
impl Hotkeys for GSettings {
    fn get(&self, target: &Target) -> Option<String> {
        gsettings_get(target)
    }

    fn set(&self, target: &Target, accelerator: &str) -> Result<(), String> {
        gsettings_set(target, accelerator)
    }

    fn items(&self) -> HashMap<String, String> {
        gsettings_items()
    }
}

/// The adapter, where GNOME's settings are there to hold shortcuts.
#[cfg(any(target_os = "linux", test))]
pub(super) fn gsettings() -> Option<&'static dyn Hotkeys> {
    static THERE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    THERE.get_or_init(gsettings_supported).then_some(&GSettings as &dyn Hotkeys)
}

#[cfg(any(target_os = "linux", test))]
fn run_gsettings(args: &[&str]) -> Result<String, String> {
    let output = Command::new("gsettings").args(args).output().map_err(|e| format!("gsettings: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

#[cfg(any(target_os = "linux", test))]
/// Parses a GVariant string or string array as printed by `gsettings get`.
fn strings(value: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != '\'' && c != '"' {
            continue;
        }
        let mut s = String::new();
        while let Some(d) = chars.next() {
            match d {
                '\\' => s.extend(chars.next()),
                d if d == c => break,
                d => s.push(d),
            }
        }
        out.push(s);
    }
    out
}

#[cfg(any(target_os = "linux", test))]
fn entry_get(path: &str, key: &str) -> String {
    run_gsettings(&["get", &format!("{ENTRY_SCHEMA}:{path}"), key])
        .map(|v| strings(&v).into_iter().next().unwrap_or_default())
        .unwrap_or_default()
}

#[cfg(any(target_os = "linux", test))]
fn entry_set(path: &str, key: &str, value: &str) -> Result<(), String> {
    run_gsettings(&["set", &format!("{ENTRY_SCHEMA}:{path}"), key, value]).map(|_| ())
}

#[cfg(any(target_os = "linux", test))]
fn paths() -> Result<Vec<String>, String> {
    Ok(strings(&run_gsettings(&["get", LIST_SCHEMA, "custom-keybindings"])?))
}

#[cfg(any(target_os = "linux", test))]
fn set_paths(paths: &[String]) -> Result<(), String> {
    let list = format!("[{}]", paths.iter().map(|p| format!("'{p}'")).collect::<Vec<_>>().join(", "));
    run_gsettings(&["set", LIST_SCHEMA, "custom-keybindings", &list]).map(|_| ())
}

/// Whether hotkeys can be managed (GNOME's settings schema is present).
#[cfg(any(target_os = "linux", test))]
fn gsettings_supported() -> bool {
    run_gsettings(&["list-keys", LIST_SCHEMA]).is_ok_and(|keys| keys.lines().any(|k| k == "custom-keybindings"))
}

#[cfg(any(target_os = "linux", test))]
fn find(target: &Target) -> Result<Option<String>, String> {
    Ok(paths()?.into_iter().find(|p| target.matches(&entry_get(p, "command"))))
}

/// The hotkey's accelerator, e.g. `<Control>space`, if one is set.
#[cfg(any(target_os = "linux", test))]
fn gsettings_get(target: &Target) -> Option<String> {
    let path = find(target).ok()??;
    Some(entry_get(&path, "binding")).filter(|b| !b.is_empty())
}

/// Accelerators of all `heyo --open <id>` shortcuts, by item id.
#[cfg(any(target_os = "linux", test))]
fn gsettings_items() -> HashMap<String, String> {
    let Ok(paths) = paths() else { return HashMap::new() };
    paths
        .iter()
        .filter_map(|path| {
            let args = heyo_args(&entry_get(path, "command"))?;
            let [flag, id] = <[String; 2]>::try_from(args).ok()?;
            let binding = entry_get(path, "binding");
            (flag == "--open" && !binding.is_empty()).then_some((id, binding))
        })
        .collect()
}

/// Accelerators compare equal regardless of modifier order and spelling.
#[cfg(any(target_os = "linux", test))]
fn normalize(accelerator: &str) -> (Vec<String>, String) {
    let mut mods: Vec<String> = Vec::new();
    let mut rest = accelerator;
    while let Some(start) = rest.strip_prefix('<') {
        let Some(end) = start.find('>') else { break };
        let m = start[..end].to_lowercase();
        mods.push(match m.as_str() {
            "primary" | "ctrl" | "control" => "control".into(),
            "mod4" | "meta" => "super".into(),
            _ => m,
        });
        rest = &start[end + 1..];
    }
    mods.sort();
    mods.dedup();
    (mods, rest.to_lowercase())
}

/// Sets the accelerator; an empty one removes the hotkey. Fails if another
/// custom shortcut already uses the accelerator.
#[cfg(any(target_os = "linux", test))]
fn gsettings_set(target: &Target, accelerator: &str) -> Result<(), String> {
    let all = paths()?;
    let existing = all.iter().find(|p| target.matches(&entry_get(p, "command"))).cloned();

    if !accelerator.is_empty() {
        let wanted = normalize(accelerator);
        for path in all.iter().filter(|p| Some(*p) != existing.as_ref()) {
            let binding = entry_get(path, "binding");
            if !binding.is_empty() && normalize(&binding) == wanted {
                let name = entry_get(path, "name");
                return Err(format!("This hotkey is already used by “{name}”."));
            }
        }
    }

    match existing {
        // Item shortcuts are removed outright; the main ones may have been
        // made by hand, so they are only unbound.
        Some(path) if accelerator.is_empty() && target.args.first().is_some_and(|a| a == "--open") => {
            set_paths(&all.into_iter().filter(|p| *p != path).collect::<Vec<_>>())?;
            run_gsettings(&["reset-recursively", &format!("{ENTRY_SCHEMA}:{path}")]).map(|_| ())
        }
        Some(path) => entry_set(&path, "binding", accelerator),
        None if accelerator.is_empty() => Ok(()),
        None => {
            let path = format!("{BASE_PATH}/{}/", target.path_id);
            let mut all = all;
            all.push(path.clone());
            set_paths(&all)?;
            entry_set(&path, "name", &target.label)?;
            entry_set(&path, "command", &target.command())?;
            entry_set(&path, "binding", accelerator)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_gvariant_strings() {
        assert_eq!(strings("['/a/', '/b/']"), ["/a/", "/b/"]);
        assert_eq!(strings("@as []"), Vec::<String>::new());
        assert_eq!(strings(r"'it\'s'"), ["it's"]);
    }

    #[test]
    fn matches_heyo_commands() {
        assert!(Target::toggle().matches("/home/me/.local/bin/heyo"));
        assert!(!Target::toggle().matches("/home/me/.local/bin/heyo --clipboard"));
        assert!(Target::clipboard_history().matches("heyo --clipboard"));
        assert!(!Target::toggle().matches("heyoo"));
        let item = Target::item("cmd:/a b/ext:run", "Run");
        assert!(item.matches(&item.command_for("/home/me/.local/bin/heyo")));
    }

    #[test]
    fn normalizes_accelerators() {
        assert_eq!(normalize("<Control><Shift>v"), normalize("<Shift><Primary>V"));
        assert_ne!(normalize("<Control>v"), normalize("<Alt>v"));
    }
}

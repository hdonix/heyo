//! Installing the Shell extension, which is carried inside Heyo, and telling
//! where it stands: absent, older than the one carried, waiting for the next
//! login, or running.
//!
//! GNOME Shell reads a user's extensions when a session starts and at no
//! other time, and once it has loaded an extension's code it cannot load
//! another version of it (GNOME 45 to 50: the code is an ES module, which
//! stays cached; the Shell refuses with "A different version was loaded
//! previously. You need to log out for changes to take effect."). On Wayland
//! the Shell cannot be restarted without ending the session either. So both
//! a first installation and an update take effect at the next login, and
//! until then an older extension that is running keeps serving.

use std::path::PathBuf;
use std::process::Command;

use super::API;

const UUID: &str = "heyo@heyo.app";
const FILES: &[(&str, &str)] = &[
    ("metadata.json", include_str!("../../../../gnome-extension/heyo@heyo.app/metadata.json")),
    ("extension.js", include_str!("../../../../gnome-extension/heyo@heyo.app/extension.js")),
];

fn directory() -> PathBuf {
    crate::paths::data_home().join("gnome-shell/extensions").join(UUID)
}

/// Whether the session is GNOME's, where the extension can run.
fn gnome() -> bool {
    std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|d| d.to_lowercase().contains("gnome"))
}

/// The version an extension's code says it is: its `const API = N;` line.
fn version_of(source: &str) -> Option<u32> {
    source.lines().find_map(|line| line.trim().strip_prefix("const API =")?.trim().strip_suffix(';')?.trim().parse().ok())
}

/// The version of the extension in the user's extensions, if one is there.
/// (One installed from extensions.gnome.org is the same code, but its
/// `metadata.json` is rewritten there: only the code is looked at.)
fn installed() -> Option<u32> {
    version_of(&std::fs::read_to_string(directory().join("extension.js")).ok()?)
}

/// Where the extension stands.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum State {
    /// It has no use in this session.
    None,
    /// Not installed, or installed and switched off.
    Missing,
    /// An older one than Heyo carries is installed. If it is running it
    /// still serves, without what was added since.
    Outdated,
    /// The one Heyo carries (or a newer one) is in place and switched on;
    /// the Shell loads it at the next login. An older one may be serving
    /// meanwhile.
    Pending,
    /// Running, and not older than the one Heyo carries.
    Active,
}

impl State {
    pub fn name(self) -> &'static str {
        match self {
            State::None => "none",
            State::Missing => "missing",
            State::Outdated => "outdated",
            State::Pending => "pending",
            State::Active => "active",
        }
    }
}

/// The state, from the version that answers in the Shell, the version in
/// the user's extensions, and whether the Shell is told to load it.
fn standing(running: Option<u32>, installed: Option<u32>, enabled: bool) -> State {
    let current = |version: u32| version >= API;
    match (running, installed) {
        (Some(version), _) if current(version) => State::Active,
        (_, Some(version)) if current(version) && enabled => State::Pending,
        (Some(_), _) => State::Outdated,
        (None, Some(version)) if !current(version) => State::Outdated,
        _ => State::Missing,
    }
}

pub fn state() -> State {
    if crate::platform::session() != crate::platform::Session::Wayland || !gnome() {
        State::None
    } else {
        standing(super::running(), installed(), enabled())
    }
}

fn enabled_extensions() -> Option<String> {
    let output = Command::new("gsettings").args(["get", "org.gnome.shell", "enabled-extensions"]).output().ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn enabled() -> bool {
    enabled_extensions().is_some_and(|list| list.contains(&format!("'{UUID}'")))
}

/// The list (as gsettings prints it) with the extension added.
fn with_extension(list: &str) -> String {
    match list.strip_suffix(']') {
        Some(rest) if rest.ends_with('[') => format!("['{UUID}']"),
        Some(rest) => format!("{rest}, '{UUID}']"),
        None => format!("['{UUID}']"),
    }
}

/// Puts the extension in the user's extensions, over an older one, and
/// enables it. It takes effect at the next login (see the module's header).
pub fn install() -> Result<(), String> {
    // A newer one than Heyo carries (from extensions.gnome.org) is left alone.
    if installed().is_none_or(|version| version < API) {
        let dir = directory();
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        for (name, content) in FILES {
            std::fs::write(dir.join(name), content).map_err(|e| format!("{}: {e}", dir.join(name).display()))?;
        }
    }
    if !enabled() {
        let list = enabled_extensions().ok_or("could not read the enabled extensions (is gsettings installed?)")?;
        let status = Command::new("gsettings")
            .args(["set", "org.gnome.shell", "enabled-extensions", &with_extension(&list)])
            .status()
            .map_err(|e| e.to_string())?;
        if !status.success() {
            return Err("could not enable the extension".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_to_the_enabled_list() {
        assert_eq!(with_extension("@as []"), "['heyo@heyo.app']");
        assert_eq!(with_extension("[]"), "['heyo@heyo.app']");
        assert_eq!(with_extension("['a@b', 'c@d']"), "['a@b', 'c@d', 'heyo@heyo.app']");
    }

    #[test]
    fn the_carried_extension_is_the_version_this_code_expects() {
        let source = FILES.iter().find(|(name, _)| *name == "extension.js").unwrap().1;
        assert_eq!(version_of(source), Some(API));
        // The first one was written the same way.
        assert_eq!(version_of("const HEYO_NAME = 'x';\nconst API = 1;\n"), Some(1));
        assert_eq!(version_of("// nothing of the kind"), None);
    }

    #[test]
    fn where_the_extension_stands() {
        let (old, new, newer) = (API - 1, API, API + 1);
        // Nothing there, or there and switched off.
        assert_eq!(standing(None, None, false), State::Missing);
        assert_eq!(standing(None, None, true), State::Missing);
        assert_eq!(standing(None, Some(new), false), State::Missing);
        // Installed and switched on, but the session started without it.
        assert_eq!(standing(None, Some(new), true), State::Pending);
        // Running.
        assert_eq!(standing(Some(new), Some(new), true), State::Active);
        // Heyo was updated: the old one runs, and is on disk.
        assert_eq!(standing(Some(old), Some(old), true), State::Outdated);
        // ... and after "Update": the new one is on disk, the old still runs.
        assert_eq!(standing(Some(old), Some(new), true), State::Pending);
        // An old one on disk that is not even running.
        assert_eq!(standing(None, Some(old), true), State::Outdated);
        assert_eq!(standing(None, Some(old), false), State::Outdated);
        // A newer extension than this Heyo knows (updated on its own) is fine.
        assert_eq!(standing(Some(newer), Some(newer), true), State::Active);
        assert_eq!(standing(Some(old), Some(newer), true), State::Pending);
        // What runs is what counts, whatever has been done to the files since.
        assert_eq!(standing(Some(new), None, false), State::Active);
        assert_eq!(standing(Some(old), None, false), State::Outdated);
    }
}

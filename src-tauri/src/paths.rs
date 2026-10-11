//! Where Heyo keeps things. Every location is named here once; the
//! extension runtime is told the same roots when it starts (`for_runtime`).
//!
//! The roots follow the XDG base directories. On Windows `lib.rs` points
//! those at `%APPDATA%` and `%LOCALAPPDATA%` before anything asks.

use std::env;
use std::path::PathBuf;

/// A base directory: the variable when it holds an absolute path (the
/// specification says to ignore anything else), else `fallback` under home.
fn base(variable: &str, fallback: &str) -> PathBuf {
    resolve(env::var_os(variable).map(PathBuf::from), fallback)
}

fn resolve(given: Option<PathBuf>, fallback: &str) -> PathBuf {
    given.filter(|p| p.is_absolute()).unwrap_or_else(|| home().join(fallback))
}

pub fn home() -> PathBuf {
    PathBuf::from(env::var_os("HOME").unwrap_or_default())
}

/// The user's data directory, which other programs share (`~/.local/share`).
pub fn data_home() -> PathBuf {
    base("XDG_DATA_HOME", ".local/share")
}

/// The user's configuration directory (`~/.config`).
pub fn config_home() -> PathBuf {
    base("XDG_CONFIG_HOME", ".config")
}

/// A place for sockets and short-lived files only this user can read.
#[cfg(target_os = "linux")]
pub fn runtime_dir() -> PathBuf {
    env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).filter(|p| p.is_absolute()).unwrap_or_else(env::temp_dir)
}

/// The input method Heyo set aside to compose text itself, noted for as
/// long as it does: read at the next start if Heyo did not live to give it
/// back. It means something only while the session lasts, as this
/// directory does.
#[cfg(target_os = "linux")]
pub fn input_method_set_aside() -> PathBuf {
    runtime_dir().join("heyo-input-method")
}

/// Heyo's data: extensions, clipboard history, quicklinks, usage, engines.
pub fn data() -> PathBuf {
    data_home().join("heyo")
}

/// Heyo's configuration: settings and extension preferences.
pub fn config() -> PathBuf {
    config_home().join("heyo")
}

/// What Heyo can make again: captures, the log.
pub fn cache() -> PathBuf {
    base("XDG_CACHE_HOME", ".cache").join("heyo")
}

/// Installed extensions, one directory each.
pub fn extensions() -> PathBuf {
    data().join("extensions")
}

pub fn settings() -> PathBuf {
    config().join("settings.json")
}

/// The values a user gave an extension's preferences.
pub fn preferences(extension: &str) -> PathBuf {
    config().join("preferences").join(format!("{extension}.json"))
}

/// The extensions being developed (`heyo dev`): which, and from where.
pub fn development() -> PathBuf {
    data().join("development.json")
}

/// What the user has allowed each extension to ask of Heyo.
pub fn permissions() -> PathBuf {
    config().join("permissions.json")
}

/// The roots, for the extension runtime's environment.
pub fn for_runtime() -> [(&'static str, PathBuf); 4] {
    [("HEYO_DATA_DIR", data()), ("HEYO_CONFIG_DIR", config()), ("HEYO_CACHE_DIR", cache()), ("HEYO_EXTENSIONS_DIR", extensions())]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_base_directory_must_be_absolute() {
        let home = home();
        assert_eq!(resolve(Some("/srv/data".into()), ".local/share"), PathBuf::from("/srv/data"));
        assert_eq!(resolve(Some("relative".into()), ".local/share"), home.join(".local/share"));
        assert_eq!(resolve(None, ".config"), home.join(".config"));
    }

    #[test]
    fn everything_of_heyos_sits_under_its_three_roots() {
        assert!(extensions().starts_with(data()));
        assert!(settings().starts_with(config()));
        assert!(preferences("x").ends_with("heyo/preferences/x.json"));
        for (_, path) in for_runtime() {
            assert!([data(), config(), cache()].iter().any(|root| path.starts_with(root)));
        }
    }
}

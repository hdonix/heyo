//! The installed applications: what they are, however the system lists
//! them (freedesktop `.desktop` entries, or Start menu shortcuts).

use std::path::PathBuf;

#[cfg_attr(windows, path = "apps/windows.rs")]
#[cfg_attr(not(windows), path = "apps/desktop_entries.rs")]
mod system;
#[cfg(any(windows, test))]
mod shortcut;

#[cfg(windows)]
pub use system::icon_dir;

pub use system::{discover, fingerprint, launch};

#[derive(Debug, Clone)]
pub struct App {
    /// Desktop file ID, e.g. `org.gnome.Nautilus.desktop`.
    pub id: String,
    pub name: String,
    pub generic_name: Option<String>,
    pub comment: Option<String>,
    pub keywords: Vec<String>,
    pub exec: String,
    pub icon: Option<PathBuf>,
    pub terminal: bool,
    // Windows' shortcuts are opened by the shell, which sees to theirs.
    #[cfg_attr(windows, allow(dead_code))]
    pub working_dir: Option<PathBuf>,
    pub path: PathBuf,
    /// `StartupWMClass`: the window class the application says its windows have.
    pub startup_wm_class: Option<String>,
}

impl App {
    /// Names its windows may carry as their class, lowercase: what the
    /// desktop file declares, the desktop file's own name, and the program's.
    /// Used to find the application's windows when it is already running.
    pub fn window_classes(&self) -> Vec<String> {
        // A terminal application's window belongs to the terminal.
        if self.terminal {
            return Vec::new();
        }
        let stem = self.id.strip_suffix(".desktop").unwrap_or(&self.id);
        let program = shlex::split(&self.exec)
            .unwrap_or_default()
            .into_iter()
            // `env VAR=1 program`, and the like.
            .find(|word| !word.contains('=') && word != "env")
            .and_then(|word| word.rsplit('/').next().map(str::to_owned))
            // Launchers that many applications share say nothing about which one.
            .filter(|name| !["sh", "bash", "flatpak", "snap", "python", "python3", "java", "electron", "gtk-launch", "xdg-open"].contains(&name.as_str()));
        let mut names: Vec<String> = Vec::new();
        for name in [self.startup_wm_class.clone(), Some(stem.to_owned()), stem.rsplit('.').next().map(str::to_owned), program].into_iter().flatten() {
            let name = name.to_lowercase();
            if !name.is_empty() && !names.contains(&name) {
                names.push(name);
            }
        }
        names
    }
}

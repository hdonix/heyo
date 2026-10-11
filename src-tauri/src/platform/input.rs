//! Input into other applications: the paste keystroke and the selected text.
//!
//! What is pasted, when, and what a terminal may be given is `pasting`'s
//! business; here is the keystroke itself, and which windows are terminals
//! (where paste is another shortcut).

use super::desktop::desktop;
use super::windows;
#[cfg(target_os = "linux")]
use super::{session, Session};

/// Window classes of terminal emulators, where paste is Ctrl+Shift+V.
const TERMINALS: &[&str] = &["terminal", "konsole", "kitty", "alacritty", "xterm", "wezterm", "tilix", "ghostty", "foot", "kgx", "ptyxis", "warp"];

/// Whether a window class (lowercase) is a terminal emulator's.
pub(crate) fn is_terminal(class: &str) -> bool {
    TERMINALS.iter().any(|name| class.contains(name))
}

/// Whether the focused window is a terminal. Unknown counts as not.
pub(crate) fn terminal_focused() -> bool {
    windows::active_class().is_some_and(|class| is_terminal(&class.to_lowercase()))
}

/// Presses the paste shortcut in the focused window: a terminal's, if it
/// is one.
pub(crate) fn press_paste(terminal: bool) -> Result<(), String> {
    desktop().paste(terminal)
}

/// The text currently selected in the application in front.
pub(crate) fn selected_text() -> String {
    desktop().selected_text()
}

/// Presses the paste shortcut with whichever keystroke tool is installed:
/// what a Linux session without an adapter of its own is left with, and how
/// X11 does it anyway.
#[cfg(target_os = "linux")]
pub(super) fn paste_with_tools(terminal: bool) -> Result<(), String> {
    let xdotool: (&str, &[&str]) = ("xdotool", if terminal { &["key", "--clearmodifiers", "ctrl+shift+v"] } else { &["key", "--clearmodifiers", "ctrl+v"] });
    let wtype: (&str, &[&str]) =
        ("wtype", if terminal { &["-M", "ctrl", "-M", "shift", "v", "-m", "shift", "-m", "ctrl"] } else { &["-M", "ctrl", "v", "-m", "ctrl"] });
    // Key codes: 29 Ctrl, 42 Shift, 47 V.
    let ydotool: (&str, &[&str]) =
        ("ydotool", if terminal { &["key", "29:1", "42:1", "47:1", "47:0", "42:0", "29:0"] } else { &["key", "29:1", "47:1", "47:0", "29:0"] });
    // The tool that belongs to the session first; the others in case it is missing.
    let attempts = match session() {
        Session::Wayland => [wtype, ydotool, xdotool],
        _ => [xdotool, wtype, ydotool],
    };
    for (program, args) in attempts {
        if std::process::Command::new(program).args(args).status().is_ok_and(|s| s.success()) {
            return Ok(());
        }
    }
    Err("install xdotool (X11), or wtype or ydotool (Wayland)".into())
}

/// X11's PRIMARY selection (which Wayland sessions keep too): it holds what
/// is selected without the user having to copy.
#[cfg(target_os = "linux")]
pub(super) fn primary_selection() -> String {
    use arboard::{Clipboard, GetExtLinux, LinuxClipboardKind};
    Clipboard::new().and_then(|mut clipboard| clipboard.get().clipboard(LinuxClipboardKind::Primary).text()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminals_are_recognised_by_window_class() {
        assert!(is_terminal("gnome-terminal-server"));
        assert!(is_terminal("dev.warp.warp"));
        assert!(is_terminal("org.gnome.ptyxis"));
        assert!(!is_terminal("firefox"));
    }
}

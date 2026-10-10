//! Input into other applications: the paste keystroke and the selected text.
//!
//! A terminal runs what is pasted into it line by line as the lines land.
//! So text of several lines only goes into one as it is when the user has
//! said so ([`paste_as_is`], after `pasting` asked); pasted any other way
//! ([`paste_keystroke`]: dictation, which has nowhere to ask) it goes in as
//! one line.

use std::sync::OnceLock;
use std::time::Duration;

use tauri::AppHandle;

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

/// Whether a terminal would run (part of) this text as it is pasted: any
/// line end does it, also a single one at the very end.
pub(crate) fn runs_as_commands(text: &str) -> bool {
    text.contains(['\n', '\r'])
}

/// The text with its lines joined by spaces: nothing in it presses Enter.
pub(crate) fn on_one_line(text: &str) -> String {
    text.split(['\n', '\r']).map(str::trim).filter(|line| !line.is_empty()).collect::<Vec<_>>().join(" ")
}

/// The application, for reaching the clipboard as the rest of Heyo does.
static APP: OnceLock<AppHandle> = OnceLock::new();

/// Tells this module the application once it exists. Until then (and in
/// tests) the clipboard is left as it is.
pub(crate) fn attach(app: &AppHandle) {
    let _ = APP.set(app.clone());
}

/// Rewrites the clipboard's text onto one line if a terminal would run it.
fn keep_on_one_line() {
    let Some(app) = APP.get() else { return };
    let Some(text) = super::clipboard_watch::read_text(app).filter(|text| runs_as_commands(text)) else { return };
    // Plumbing, not something the user copied.
    crate::clipboard::skip_next();
    match super::clipboard_watch::write_text(app, &on_one_line(&text)) {
        // Let the new contents be the clipboard's before they are asked for.
        Ok(()) => std::thread::sleep(Duration::from_millis(40)),
        Err(e) => eprintln!("heyo: could not put the text on one line: {e}"),
    }
}

/// Sends the paste keystroke to the focused window after `delay`. Into a
/// terminal, text of several lines goes as one line.
pub(crate) fn paste_keystroke(delay: Duration) {
    std::thread::sleep(delay);
    let terminal = terminal_focused();
    if terminal {
        keep_on_one_line();
    }
    if let Err(e) = desktop().paste(terminal) {
        eprintln!("heyo: could not paste: {e}");
    }
}

/// Sends the paste keystroke to the focused window after `delay`, whatever
/// is on the clipboard: for when the user has seen what will be pasted.
pub(crate) fn paste_as_is(delay: Duration) {
    std::thread::sleep(delay);
    if let Err(e) = desktop().paste(terminal_focused()) {
        eprintln!("heyo: could not paste: {e}");
    }
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

    #[test]
    fn any_line_end_would_run_in_a_terminal() {
        assert!(runs_as_commands("ls\nrm -rf x"));
        assert!(runs_as_commands("ls\n"));
        assert!(runs_as_commands("ls\r"));
        assert!(runs_as_commands("a\r\nb"));
        assert!(!runs_as_commands("echo one; echo two"));
        assert!(!runs_as_commands(""));
    }

    #[test]
    fn text_put_on_one_line_presses_no_enter() {
        assert_eq!(on_one_line("First paragraph.\n\nSecond one.\n"), "First paragraph. Second one.");
        assert_eq!(on_one_line("a\r\nb\rc"), "a b c");
        assert_eq!(on_one_line("  ls -la  \n"), "ls -la");
        assert_eq!(on_one_line("one line"), "one line");
        assert_eq!(on_one_line("\n\n"), "");
        for text in ["curl x | sh\n", "a\nb\r\nc\r", "\n"] {
            assert!(!runs_as_commands(&on_one_line(text)));
        }
    }
}

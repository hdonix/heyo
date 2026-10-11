//! The palette window: showing, hiding and toggling it.

use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

use crate::*;

pub(crate) fn main_window(app: &AppHandle) -> WebviewWindow {
    app.get_webview_window("main").expect("main window")
}

/// The window the user came from when the palette came up.
pub(crate) fn came_from(app: &AppHandle) -> &windows::LastWindow {
    app.state::<windows::LastWindow>().inner()
}

/// Takes the window in front as the one the user came from: the palette is
/// about to come up over it.
pub(crate) fn remember_window(app: &AppHandle) {
    came_from(app).remember();
}

/// The window the user is working in: the one in front, or the one that
/// was before the palette came up.
pub(crate) fn working_window(app: &AppHandle) -> Option<windows::Window> {
    came_from(app).working()
}

pub(crate) fn show(app: &AppHandle) {
    // For commands that act on that window.
    remember_window(app);
    let window = main_window(app);
    let _ = window.center();
    let _ = window.show();
    focus(&window);
    let _ = window.emit(event::SHOWN, ());
}

/// How long the palette takes to be gone from the screen once it is hidden.
const GONE: Duration = Duration::from_millis(180);

/// Puts the palette away and returns when it is off the screen, for what
/// is about to look at the screen. Says whether it was up. (It waits: not
/// for the main thread.)
pub(crate) fn hide_until_gone(app: &AppHandle) -> bool {
    let window = main_window(app);
    let up = window.is_visible().unwrap_or(false);
    if up {
        let _ = window.hide();
        std::thread::sleep(GONE);
    }
    up
}

/// Runs `f` with the palette off the screen, and brings it back as it was:
/// the command that asked is still in it.
pub(crate) fn away_during<T>(app: &AppHandle, f: impl FnOnce() -> T) -> T {
    let up = hide_until_gone(app);
    let result = f();
    if up {
        let window = main_window(app);
        let _ = window.show();
        focus(&window);
    }
    result
}

/// Runs `f` with the palette up, bringing it forward for as long as `f`
/// takes if it was away: a command in the background has something to ask.
pub(crate) fn up_during<T>(app: &AppHandle, f: impl FnOnce() -> T) -> T {
    let window = main_window(app);
    let away = matches!(window.is_visible(), Ok(false));
    if away {
        let _ = window.show();
        focus(&window);
    }
    let result = f();
    if away {
        let _ = window.hide();
    }
    result
}

/// `heyo --clipboard`: shows the palette with Clipboard History open.
pub(crate) fn open_clipboard_history(app: &AppHandle) {
    show(app);
    let launch = HostMessage::launch(extensions::BUILTIN, "clipboard-history", None);
    if let Err(e) = app.state::<Runtime>().send(app, &launch) {
        eprintln!("heyo: {e}");
    }
}

pub(crate) fn toggle(app: &AppHandle) {
    if main_window(app).is_visible().unwrap_or(false) {
        let _ = main_window(app).hide();
    } else {
        show(app);
    }
}

//! The palette window: showing, hiding and toggling it.

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

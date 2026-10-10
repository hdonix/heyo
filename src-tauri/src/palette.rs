//! The palette window: showing, hiding and toggling it.

use tauri::{AppHandle, Manager};

use crate::*;

/// The palette, in whichever interface draws it (`shell`).
pub(crate) fn main_window(app: &AppHandle) -> shell::Window {
    shell::window(app, "main").expect("main window")
}

pub(crate) fn show(app: &AppHandle) {
    // Remember where the user came from, for commands that act on that window.
    if let Some(active) = windows::active_external() {
        *app.state::<windows::LastWindow>().0.lock().unwrap() = Some(active);
    }
    let window = main_window(app);
    let _ = window.center();
    let _ = window.show();
    window.focus();
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

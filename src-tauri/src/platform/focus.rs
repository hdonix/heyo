//! Giving one of Heyo's own windows the keyboard focus.

use tauri::{WebviewWindow, Window};

/// Gives the window keyboard focus.
///
/// Heyo is usually shown from a hotkey command, not from input to our own
/// window, so mutter's focus-stealing prevention rejects a plain focus request
/// and only marks the window as demanding attention. On X11, presenting with
/// the server's current time makes the request newer than the user's last
/// interaction, which mutter accepts.
#[cfg(target_os = "linux")]
pub(crate) fn focus(window: &WebviewWindow) {
    use gtk::prelude::*;

    let window = window.clone();
    let _ = window.clone().run_on_main_thread(move || {
        let Ok(gtk_window) = window.gtk_window() else { return };
        let server_time = gtk_window
            .window()
            .and_then(|w| w.downcast::<gdkx11::X11Window>().ok())
            .map(|w| gdkx11::functions::x11_get_server_time(&w));
        match server_time {
            Some(time) => gtk_window.present_with_time(time),
            None => gtk_window.present(),
        }
        // On Wayland an application cannot bring itself to the front; the
        // Shell extension does it.
        if let Some(title) = gtk_window.title() {
            let title = title.to_string();
            std::thread::spawn(move || super::desktop::desktop().activate_own(&title));
        }
    });
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn focus(window: &WebviewWindow) {
    let _ = window.set_focus();
}

/// Runs `then` if the window, which was just told it lost the focus, has
/// really lost it. (Call on the main thread, from the event.)
///
/// On Wayland the toolkit is told so whenever a keyboard appears or its
/// layout changes, with the window still in front and still being typed
/// into. So ask again a little later, and ask the Shell where that is
/// possible: it is the one that decides.
#[cfg(target_os = "linux")]
pub(crate) fn when_lost(window: &Window, then: impl FnOnce(&Window) + Send + 'static) {
    use gtk::prelude::*;

    let window = window.clone();
    gtk::glib::timeout_add_local_once(std::time::Duration::from_millis(150), move || {
        if window.gtk_window().map(|w| w.is_active()).unwrap_or(false) {
            return;
        }
        // The toolkit says it is gone; the desktop may know better.
        std::thread::spawn(move || {
            if super::desktop::desktop().own_focused() != Some(true) {
                let _ = window.clone().run_on_main_thread(move || then(&window));
            }
        });
    });
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn when_lost(window: &Window, then: impl FnOnce(&Window) + Send + 'static) {
    // The focus also "leaves" for a moment as it moves between the window
    // and the web view inside it: look again shortly, and at whose window
    // is in front.
    let window = window.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(150));
        if super::desktop::desktop().own_focused() == Some(true) {
            return;
        }
        let _ = window.clone().run_on_main_thread(move || {
            if !window.is_focused().unwrap_or(false) {
                then(&window);
            }
        });
    });
}

//! The display changing while Heyo runs: a monitor connected or taken
//! away, another resolution, another scale, another size or theme of the
//! pointer. A laptop is carried from one monitor to another with Heyo
//! running, and what Heyo knew of the display when it started is then of
//! another display.
//!
//! `watch` has the desktop say so. On Linux the toolkit of the palette is
//! asked, which hears it from the session on X11 and on Wayland alike.
//! Elsewhere the palette's window is told of its scale by the toolkit
//! itself (`palette::on_window_event`), and that is passed on.

/// Has `changed` called, on the main thread, each time the desktop says
/// something of the display changed. It says several things for one
/// change, each a moment after the other. (Call on the main thread.)
#[cfg(target_os = "linux")]
pub(crate) fn watch(changed: impl Fn() + Clone + 'static) {
    use gtk::prelude::*;
    if let Some(screen) = gtk::gdk::Screen::default() {
        // Monitors, their sizes, and the scale windows are drawn at.
        let say = changed.clone();
        screen.connect_monitors_changed(move |_| say());
        let say = changed.clone();
        screen.connect_size_changed(move |_| say());
    }
    if let Some(settings) = gtk::Settings::default() {
        // What text and the pointer are sized by.
        for name in ["gtk-xft-dpi", "gtk-cursor-theme-size", "gtk-cursor-theme-name"] {
            let say = changed.clone();
            settings.connect_notify_local(Some(name), move |_, _| say());
        }
    }
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn watch(_changed: impl Fn() + Clone + 'static) {}

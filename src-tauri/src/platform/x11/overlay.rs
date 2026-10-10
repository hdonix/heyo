//! The screenshot overlay on X11: a window that bypasses the window manager.

use std::time::Duration;

use gtk::prelude::*;

/// Puts the window exactly over the given rectangle (logical pixels), above
/// everything, and gives it the keyboard.
///
/// It bypasses the window manager ("override redirect", like a menu): above
/// the desktop's own panels, with no opening animation, and nothing may
/// move it. A fullscreen window would be animated by GNOME, and one of a
/// kind that is not animated is not allowed to cover the top bar.
pub fn place(gtk_window: &gtk::ApplicationWindow, x: i32, y: i32, width: i32, height: i32) {
    grab_keyboard_when_mapped(gtk_window);
    gtk_window.realize();
    if let Some(gdk_window) = gtk_window.window() {
        gdk_window.set_override_redirect(true);
    }
    gtk_window.move_(x, y);
    gtk_window.resize(width, height);
    // Its contents too: the window was created hidden, so nothing in it has
    // been shown yet the first time.
    gtk_window.show_all();
    if let Some(gdk_window) = gtk_window.window() {
        gdk_window.move_resize(x, y, width, height);
        gdk_window.raise();
    }
}

thread_local! {
    /// The window showing the frozen capture under the overlay. Main thread only.
    static BACKDROP: std::cell::RefCell<Option<gtk::Window>> = const { std::cell::RefCell::new(None) };
}

/// Shows the capture in a window of its own over the given rectangle, for
/// the (transparent) overlay to be placed on top of. False where that
/// cannot work, and the overlay has to paint the capture itself.
///
/// The desktop's compositor then blends the two, on the graphics card. The
/// overlay painting the capture under its own contents means blending a
/// whole screen in software for every frame, which at 4K is most of what a
/// frame costs, and makes dragging lag behind the pointer.
pub fn backdrop(surface: gtk::cairo::ImageSurface, x: i32, y: i32, width: i32, height: i32) -> bool {
    remove_backdrop();
    let window = gtk::Window::new(gtk::WindowType::Popup);
    // Without a compositor a transparent window shows nothing behind it.
    if !WidgetExt::screen(&window).is_some_and(|screen| screen.is_composited()) {
        return false;
    }
    window.set_app_paintable(true);
    window.set_accept_focus(false);
    window.connect_draw(move |_, cr| {
        cr.set_operator(gtk::cairo::Operator::Source);
        let _ = cr.set_source_surface(&surface, 0.0, 0.0);
        let _ = cr.paint();
        gtk::glib::Propagation::Stop
    });
    window.move_(x, y);
    window.resize(width, height);
    window.show();
    if let Some(gdk_window) = window.window() {
        gdk_window.move_resize(x, y, width, height);
        // The pointer is the overlay's.
        gdk_window.input_shape_combine_region(&gtk::cairo::Region::create(), 0, 0);
    }
    BACKDROP.with(|backdrop| *backdrop.borrow_mut() = Some(window));
    true
}

fn remove_backdrop() {
    if let Some(window) = BACKDROP.with(|backdrop| backdrop.borrow_mut().take()) {
        // SAFETY: the window is ours alone and nothing else refers to it.
        unsafe { window.destroy() };
    }
}

/// Gives the keyboard back, before the window is hidden.
pub fn release(gtk_window: &gtk::ApplicationWindow) {
    if let Some(seat) = gtk_window.display().default_seat() {
        seat.ungrab();
    }
}

/// After the window is hidden.
pub fn close() {
    remove_backdrop();
}

/// No window manager hands the overlay the keyboard, so once it is on screen
/// it grabs it: every key goes to the overlay until it closes. The input
/// focus stays where it was, so there is nothing to give back afterwards and
/// nothing for the window manager to object to. (Input methods work under
/// the grab; taking the real focus instead was tried and window managers
/// take it back.)
fn grab_keyboard_when_mapped(gtk_window: &gtk::ApplicationWindow) {
    thread_local!(static CONNECTED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) });
    if CONNECTED.replace(true) {
        return;
    }
    gtk_window.connect_map_event(|gtk_window, _| {
        // Usually the hotkey that started the capture is still held and the
        // desktop's own grab on it is still active, so the first attempts
        // can fail. Retry until it is ours.
        fn grab(gtk_window: gtk::ApplicationWindow, attempts: u32) {
            if !gtk_window.is_visible() {
                return;
            }
            let Some(gdk_window) = gtk_window.window() else { return };
            let Some(seat) = gdk_window.display().default_seat() else { return };
            let status = seat.grab(&gdk_window, gtk::gdk::SeatCapabilities::KEYBOARD, true, None, None, None);
            if status != gtk::gdk::GrabStatus::Success && attempts > 0 {
                gtk::glib::timeout_add_local_once(Duration::from_millis(50), move || grab(gtk_window, attempts - 1));
            }
        }
        grab(gtk_window.clone(), 60);
        gtk::glib::Propagation::Proceed
    });
}

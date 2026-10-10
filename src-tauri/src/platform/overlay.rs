//! A window that covers a whole monitor with a frozen picture of it, for
//! selecting part of the screen.
//!
//! The picture is painted natively, under the (transparent) webview, so the
//! overlay can appear the moment the screen has been read: by a window of
//! its own underneath where the desktop composites windows (X11), else by
//! the overlay window itself.
//! Getting the window over everything, panels included, and getting it the
//! keyboard is the session's business.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::AppHandle;

use super::capture::Area;
use crate::shell::Window;
#[cfg(target_os = "linux")]
use super::desktop::desktop;

/// How bright the capture is around the selection, out of 256.
#[cfg(target_os = "linux")]
const DIM: u16 = 154;

/// The overlay is on screen.
static OPEN: AtomicBool = AtomicBool::new(false);
/// Counts answers from the main thread, for `watch`.
#[cfg(target_os = "linux")]
static BEATS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// A safety net for sessions where the overlay holds the keyboard by a grab
/// (X11). If Heyo dies, the server releases the grab by itself. If it were
/// to hang with the overlay up, the grab would stay and the keyboard would
/// be Heyo's alone. So while the overlay is open, the main thread is asked
/// to answer every second; when it has not for a few seconds, Heyo exits,
/// which frees the keyboard.
#[cfg(target_os = "linux")]
fn watch(app: &AppHandle) {
    use std::time::Duration;
    static WATCHING: AtomicBool = AtomicBool::new(false);
    if WATCHING.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let mut silent = 0;
        while OPEN.load(Ordering::SeqCst) {
            let before = BEATS.load(Ordering::SeqCst);
            let _ = app.run_on_main_thread(|| {
                BEATS.fetch_add(1, Ordering::SeqCst);
            });
            std::thread::sleep(Duration::from_secs(1));
            silent = if BEATS.load(Ordering::SeqCst) == before { silent + 1 } else { 0 };
            if silent >= 5 && OPEN.load(Ordering::SeqCst) {
                eprintln!("heyo: not responding while the screenshot overlay holds the keyboard; exiting to release it");
                // Not `exit`: that runs cleanup which could wait on the very
                // thread that is stuck.
                std::process::abort();
            }
        }
        WATCHING.store(false, Ordering::SeqCst);
    });
}

#[cfg(target_os = "linux")]
thread_local! {
    /// What the overlay window paints behind the webview. Main thread only.
    static FROZEN: std::cell::RefCell<Option<gtk::cairo::ImageSurface>> = const { std::cell::RefCell::new(None) };
}

/// Connects the painting of the frozen capture, once.
#[cfg(target_os = "linux")]
fn paint_frozen(gtk_window: &gtk::ApplicationWindow) {
    use gtk::prelude::*;
    thread_local!(static CONNECTED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) });
    if CONNECTED.replace(true) {
        return;
    }
    // The webview clears its area to transparent as it draws, so nothing
    // painted before it survives. The capture is therefore painted after it,
    // but underneath: the window's buffer then holds the tools over
    // transparency, and "destination over" fills in behind them.
    gtk_window.connect_local("draw", true, |values| {
        let Ok(cr) = values[1].get::<gtk::cairo::Context>() else { return Some(false.to_value()) };
        // For measuring how well the overlay keeps up with the pointer.
        static COUNT_FRAMES: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        if *COUNT_FRAMES.get_or_init(|| std::env::var_os("HEYO_OVERLAY_DEBUG").is_some()) {
            eprintln!("heyo: overlay frame");
        }
        FROZEN.with(|frozen| {
            if let Some(surface) = &*frozen.borrow() {
                let _ = cr.save();
                cr.set_operator(gtk::cairo::Operator::DestOver);
                let _ = cr.set_source_surface(surface, 0.0, 0.0);
                let _ = cr.paint();
                let _ = cr.restore();
            }
        });
        Some(false.to_value())
    });
}

/// Whether a window of another process (a Flutter overlay) put over the
/// host's backdrop is drawn over it. Xfwm4's compositor draws the backdrop
/// on top whatever the order of the two windows (seen with xfwm4 4.18; GNOME
/// Shell keeps the order): there the overlay paints the capture itself.
#[cfg(target_os = "linux")]
fn stacks_other_processes(display: &gtk::gdk::Display) -> bool {
    use gtk::prelude::*;
    let manager = display.default_screen().downcast::<gdkx11::X11Screen>().ok().and_then(|screen| screen.window_manager_name()).map(|name| name.to_string());
    !manager.is_some_and(|name| name.eq_ignore_ascii_case("xfwm4"))
}

/// Shows the window `label` over `area`, painting `pixels` (the capture of
/// that area, as `capture::Shot::bgrx`), dimmed, behind its contents.
#[cfg(target_os = "linux")]
pub fn show(app: &AppHandle, label: &'static str, area: Area, mut pixels: Vec<u8>) {
    use gtk::prelude::*;
    // Dimmed: the page shows the selected part undimmed, over it. (Dimming
    // from the page would have it repaint the whole screen as the selection
    // changes.)
    for value in &mut pixels {
        *value = (*value as u16 * DIM / 256) as u8;
    }
    OPEN.store(true, Ordering::SeqCst);
    watch(app);
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(window) = crate::shell::window(&handle, label) else { return };
        // The web view's window is the host's own, to place and to paint
        // in. A Flutter window belongs to its process, which is told where
        // to put it and does what `place_overlay` does there.
        let gtk_window = match &window {
            Window::Web(window) => match window.gtk_window() {
                Ok(gtk_window) => Some(gtk_window),
                Err(_) => return,
            },
            Window::Flutter(_) => None,
        };
        let Some(display) = gtk::gdk::Display::default() else { return };
        if let Some(gtk_window) = &gtk_window {
            paint_frozen(gtk_window);
        }
        let stride = area.width as i32 * 4;
        let surface = gtk::cairo::ImageSurface::create_for_data(pixels, gtk::cairo::Format::Rgb24, area.width as i32, area.height as i32, stride);
        let wayland = super::session() == super::Session::Wayland;
        // On Wayland each monitor has its own scale, and the window still
        // has that of the monitor it was on last.
        let toolkit = || match &gtk_window {
            Some(gtk_window) => gtk_window.scale_factor(),
            None => display.primary_monitor().or_else(|| display.monitor(0)).map_or(1, |monitor| monitor.scale_factor()),
        };
        let scale = if wayland && area.scale >= 1.0 { area.scale } else { toolkit() as f64 };
        // In logical pixels, as GTK places windows.
        let (x, y) = ((area.x as f64 / scale) as i32, (area.y as f64 / scale) as i32);
        let (width, height) = ((area.width as f64 / scale).ceil() as i32, (area.height as f64 / scale).ceil() as i32);
        // On Wayland the capture has the monitor's own pixels, which under
        // fractional scaling are fewer than the toolkit counts: there it is
        // stretched over the monitor it is of.
        let monitor = wayland
            .then(|| display.monitor_at_point(x, y))
            .flatten()
            .map(|monitor| monitor.geometry())
            .filter(|geometry| (geometry.x(), geometry.y()) == (x, y) && geometry.width() > 0);
        let (width, height) = monitor.map_or((width, height), |geometry| (geometry.width(), geometry.height()));
        let density = monitor.map_or(scale, |geometry| area.width as f64 / geometry.width() as f64);
        let mut separate = false;
        if let Ok(surface) = surface {
            // One capture pixel per device pixel, whatever the display scale.
            surface.set_device_scale(density, density);
            // Under the overlay in a window of its own where that works;
            // else painted by the overlay, behind its contents.
            separate = (gtk_window.is_some() || stacks_other_processes(&display)) && desktop().overlay_backdrop(surface.clone(), x, y, width, height);
            // (A Flutter overlay paints it from the file it is sent.)
            FROZEN.with(|frozen| *frozen.borrow_mut() = (!separate && gtk_window.is_some()).then_some(surface));
        }
        match (&window, &gtk_window) {
            (Window::Flutter(window), _) => window.overlay(x, y, width, height, separate),
            (_, Some(gtk_window)) => {
                desktop().place_overlay(gtk_window, x, y, width, height);
                gtk_window.queue_draw();
            }
            _ => {}
        }
    });
}

#[cfg(not(target_os = "linux"))]
pub fn show(app: &AppHandle, label: &'static str, area: Area, _pixels: Vec<u8>) {
    match crate::shell::window(app, label) {
        Some(Window::Web(window)) => {
            let _ = window.set_position(tauri::PhysicalPosition::new(area.x, area.y));
            let _ = window.set_size(tauri::PhysicalSize::new(area.width, area.height));
            let _ = window.set_fullscreen(true);
            let _ = window.show();
            // Escape must reach it: it covers everything.
            let _ = window.set_focus();
        }
        Some(Window::Flutter(window)) => window.overlay(area.x, area.y, area.width as i32, area.height as i32, false),
        None => {}
    }
}

/// Takes the overlay off the screen and frees the capture.
pub fn hide(app: &AppHandle, label: &'static str) {
    OPEN.store(false, Ordering::SeqCst);
    let Some(window) = crate::shell::window(app, label) else { return };
    #[cfg(not(target_os = "linux"))]
    let _ = window.hide();
    #[cfg(target_os = "linux")]
    let _ = app.run_on_main_thread(move || {
        use gtk::prelude::*;
        match &window {
            Window::Web(window) => {
                let Ok(gtk_window) = window.gtk_window() else { return };
                desktop().release_overlay(&gtk_window);
                gtk_window.hide();
            }
            // Its process gives the keyboard back as it hides the window.
            Window::Flutter(window) => window.hide(),
        }
        desktop().close_overlay();
        // Tens of megabytes.
        FROZEN.with(|frozen| *frozen.borrow_mut() = None);
    });
}

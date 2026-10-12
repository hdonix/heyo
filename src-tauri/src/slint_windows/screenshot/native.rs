//! What a desktop must be told for the overlay to cover a monitor exactly
//! and have every key. This is all of the overlay that differs by
//! platform: the window and what it shows are the same everywhere.
//!
//! - X11: the window goes past the window manager (override-redirect): it
//!   is above the desktop's panels, nothing animates it and nothing moves
//!   it. No window manager hands such a window the keyboard, so it grabs
//!   it, on the connection the window was made on (winit's): every key
//!   goes to the overlay until it lets go, and the focus stays where it
//!   was, so there is nothing to give back. The server lets the grab go by
//!   itself when the window leaves the screen or its connection ends,
//!   which is what makes the overlay safe to lose (`take_down`, `guard`).
//!   Like the pill, it is hidden and shown by winit, which keeps the
//!   window: Slint would make it anew without what is said here.
//! - Windows: a window over the monitor, not in the taskbar, given the
//!   keyboard when it is shown and not when it is made.
//!
//! Everywhere the window is said to hide all that is behind it, which it
//! does: a desktop that composites windows need not blend this one, and
//! one that can show a window that covers a monitor directly may.

use slint::winit_030::winit::window::{Window, WindowAttributes};

/// What winit is told of the overlay's window before it makes it.
#[cfg(target_os = "linux")]
pub(in super::super) fn attributes(attributes: WindowAttributes) -> WindowAttributes {
    use slint::winit_030::winit::platform::x11::WindowAttributesExtX11;
    attributes.with_override_redirect(true).with_transparent(false)
}

#[cfg(windows)]
pub(in super::super) fn attributes(attributes: WindowAttributes) -> WindowAttributes {
    use slint::winit_030::winit::platform::windows::WindowAttributesExtWindows;
    // Made ahead, when Heyo starts: it must not take the keyboard then.
    attributes.with_skip_taskbar(true).with_active(false).with_transparent(false)
}

#[cfg(not(any(target_os = "linux", windows)))]
pub(in super::super) fn attributes(attributes: WindowAttributes) -> WindowAttributes {
    attributes.with_transparent(false)
}

/// The window on winit's own connection to the X server.
#[cfg(target_os = "linux")]
struct OnX11 {
    xlib: &'static x11_dl::xlib::Xlib,
    display: *mut x11_dl::xlib::Display,
    window: std::ffi::c_ulong,
}

#[cfg(target_os = "linux")]
impl OnX11 {
    /// None on Wayland.
    fn of(window: &Window) -> Option<Self> {
        use slint::winit_030::winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
        thread_local! {
            /// libX11 as winit loaded it: the same library, so its display is ours to call with.
            static XLIB: Option<&'static x11_dl::xlib::Xlib> = x11_dl::xlib::Xlib::open().ok().map(|xlib| &*Box::leak(Box::new(xlib)));
        }
        let RawDisplayHandle::Xlib(display) = window.display_handle().ok()?.as_raw() else { return None };
        let RawWindowHandle::Xlib(handle) = window.window_handle().ok()?.as_raw() else { return None };
        Some(Self { xlib: XLIB.with(|xlib| *xlib)?, display: display.display?.as_ptr().cast(), window: handle.window })
    }
}

/// What the window is known by outside this thread, for `take_down`. 0
/// where there is no such thing.
pub(super) fn id(window: &Window) -> u64 {
    #[cfg(target_os = "linux")]
    if let Some(x11) = OnX11::of(window) {
        return x11.window as u64;
    }
    let _ = window;
    0
}

/// Puts the window on screen, above everything.
pub(super) fn show(window: &Window) {
    window.set_visible(true);
    #[cfg(target_os = "linux")]
    if let Some(x11) = OnX11::of(window) {
        // SAFETY: winit's display, on the thread winit runs on.
        unsafe {
            (x11.xlib.XRaiseWindow)(x11.display, x11.window);
            (x11.xlib.XFlush)(x11.display);
        }
    }
}

/// Asks for every key, whoever has the focus. False when it was not given
/// them yet, which is so while the hotkey that began the capture is held
/// (the desktop's own grab on it is still there): it is asked again.
pub(super) fn hold(window: &Window) -> bool {
    #[cfg(target_os = "linux")]
    if let Some(x11) = OnX11::of(window) {
        use x11_dl::xlib;
        // SAFETY: as above.
        return unsafe {
            let status = (x11.xlib.XGrabKeyboard)(x11.display, x11.window, xlib::False, xlib::GrabModeAsync, xlib::GrabModeAsync, xlib::CurrentTime);
            (x11.xlib.XFlush)(x11.display);
            status == xlib::GrabSuccess
        };
    }
    window.focus_window();
    true
}

/// Lets the keys go, and takes the window off the screen; it stays, for
/// the next time.
pub(super) fn hide(window: &Window) {
    #[cfg(target_os = "linux")]
    if let Some(x11) = OnX11::of(window) {
        // SAFETY: as above.
        unsafe {
            (x11.xlib.XUngrabKeyboard)(x11.display, x11_dl::xlib::CurrentTime);
        }
    }
    window.set_visible(false);
    #[cfg(target_os = "linux")]
    if let Some(x11) = OnX11::of(window) {
        // Now, not when winit next writes to the server.
        unsafe {
            (x11.xlib.XFlush)(x11.display);
        }
    }
}

/// Takes the overlay off the screen from another thread, when Slint's does
/// not answer: by a connection of its own, since winit's is that thread's.
/// The server lets the keyboard go when the window that holds it is no
/// longer on the screen.
pub(super) fn take_down(id: u64) {
    #[cfg(target_os = "linux")]
    {
        use x11rb::protocol::xproto::ConnectionExt;
        if id == 0 {
            return;
        }
        let taken = x11rb::connect(None).ok().and_then(|(x, _)| {
            x.unmap_window(id as u32).ok()?.check().ok()?;
            // Asked back, so that the server has done it before this
            // connection ends.
            let now = x.get_window_attributes(id as u32).ok()?.reply().ok()?;
            Some(now.map_state == x11rb::protocol::xproto::MapState::UNMAPPED)
        });
        if taken != Some(true) {
            eprintln!("heyo: the screenshot overlay (window {id:#x}) could not be taken off the screen from outside");
        }
    }
    #[cfg(not(target_os = "linux"))]
    let _ = id;
}

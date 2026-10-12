//! What a desktop is told of a window: that it floats over everything,
//! that it never has the keyboard, that it is in no taskbar, where in
//! itself it takes the pointer. A window says its standing once, as data
//! ([`Standing`]), and nothing of how any desktop is told.
//!
//! [`told`] says everything that is done about a standing on a desktop, as
//! a value: that is what the tests are of, and it is the one place to
//! read what a window comes to on X11, on Wayland and on Windows. The
//! rest of this file carries it out, once per desktop: before winit makes
//! the window (`attributes`), once it is there (`made`), each time it goes
//! on or off the screen (`on`, `off`), and for the pointer (`reach`).
//!
//! A window that has every key while it is on the screen has its ways out
//! from here too, not by its own care: `off` gives the keys back before
//! the window leaves the screen, and [`take_down`] takes whichever window
//! has them off the screen from any thread, for when Slint's does not
//! answer (the server lets the keys go with the window).
//!
//! A window never asks which desktop this is, never takes its native
//! handle, and tells winit nothing of its standing itself.

use std::sync::atomic::{AtomicU64, Ordering};

use slint::winit_030::winit::window::{Window, WindowAttributes, WindowLevel};

use crate::platform::place::Rect;

/// Where a window is among the others.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Layer {
    /// Among them, as the desktop arranges windows.
    Among,
    /// Above them, and still one the desktop arranges: it can be moved
    /// and given the keyboard like any other.
    Above,
    /// Over everything, a fullscreen window too, on every workspace, and
    /// not the desktop's to place, to animate or to move.
    Over,
}

/// What a window has of the keyboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Keyboard {
    /// As any window: when the desktop gives it the focus.
    Ordinary,
    /// Never: not by appearing, not by being clicked.
    Never,
    /// Every key while it is on the screen, whoever has the focus.
    All,
}

/// A window's standing on the desktop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Standing {
    pub layer: Layer,
    pub keyboard: Keyboard,
    /// In a taskbar and in the lists windows are switched by.
    pub listed: bool,
    /// What is behind it shows where it draws nothing.
    pub clear: bool,
}

/// The desktops a window can be on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Desktop {
    X11,
    Wayland,
    Windows,
    /// One nothing is known of: the window is as winit makes it.
    Other,
}

/// How a window comes to have every key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Keys {
    /// It does not.
    Left,
    /// It grabs the keyboard, on the connection its window was made on:
    /// every key goes to it until it lets go, and the focus stays where
    /// it was, so there is nothing to give back. (X11: no window manager
    /// hands a window it does not manage the keyboard.)
    Grabbed,
    /// It asks for the focus, which the desktop gives or does not.
    Focused,
}

/// Everything that is done about a standing on a desktop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Told {
    /// X11: the window goes past the window manager (override-redirect).
    /// It is then above every window the manager stacks, on every
    /// workspace, nothing animates or moves it, and it is never given the
    /// focus, by appearing or by a click.
    pub unmanaged: bool,
    /// X11: it is said to be a utility window, which a taskbar leaves out.
    pub utility: bool,
    /// Windows: it is kept out of the taskbar; and out of the window
    /// switcher too, as a tool window, when it never has the keyboard.
    pub unlisted: bool,
    /// Windows: it is not activated when it is made, which for a window
    /// made ahead is when Heyo starts: the window in front stays in front.
    pub made_inactive: bool,
    /// Windows: it is never activated. Not by a click (the style
    /// WS_EX_NOACTIVATE, set once the window is there) and not by being
    /// shown again, which winit would activate it by: it is shown and
    /// hidden by its handle (SW_SHOWNOACTIVATE, topmost without
    /// activation).
    pub never_active: bool,
    /// winit is asked to keep it above the other windows, once it is
    /// there.
    pub above: bool,
    /// X11: it is raised each time it goes on the screen, above the other
    /// windows that go past the window manager.
    pub raised: bool,
    pub keys: Keys,
    /// winit is told it hides all that is behind it: a desktop that
    /// composites windows need not blend it.
    pub opaque: bool,
    /// winit keeps the window and puts it on and off the screen. Slint,
    /// hiding a window, may make it anew when it shows it again, without
    /// what is said here. Where this is false, Slint shows and hides it.
    pub kept: bool,
    /// The desktop cannot be told where the window goes, nor that it is
    /// over everything or without the keyboard: all of that is the
    /// compositor's to decide. (Wayland; and winit cannot take a window
    /// off the screen there, which is why none is kept.)
    pub unplaced: bool,
}

/// What is done about a standing on a desktop.
pub(super) fn told(standing: Standing, desktop: Desktop) -> Told {
    let (over, all) = (standing.layer == Layer::Over, standing.keyboard == Keyboard::All);
    let nothing = Told { unmanaged: false, utility: false, unlisted: false, made_inactive: false, never_active: false, above: standing.layer == Layer::Above, raised: false, keys: Keys::Left, opaque: !standing.clear, kept: false, unplaced: false };
    match desktop {
        Desktop::X11 => Told { unmanaged: over, utility: !over && !standing.listed, raised: over && all, keys: if all { Keys::Grabbed } else { Keys::Left }, kept: over, ..nothing },
        Desktop::Wayland => Told { keys: if all { Keys::Focused } else { Keys::Left }, unplaced: over, ..nothing },
        Desktop::Windows => Told { unlisted: !standing.listed, made_inactive: over, never_active: standing.keyboard == Keyboard::Never, keys: if all { Keys::Focused } else { Keys::Left }, kept: over, ..nothing },
        Desktop::Other => Told { keys: if all { Keys::Focused } else { Keys::Left }, kept: over, ..nothing },
    }
}

/// The desktop Heyo's windows are on.
pub(super) fn desktop() -> Desktop {
    if cfg!(windows) {
        Desktop::Windows
    } else if super::thread::wayland() {
        Desktop::Wayland
    } else if cfg!(target_os = "linux") {
        Desktop::X11
    } else {
        Desktop::Other
    }
}

/// What is done about a standing here.
pub(super) fn here(standing: Standing) -> Told {
    told(standing, desktop())
}

// --- Carrying it out ---------------------------------------------------------------

/// What winit is told of a window before it makes it.
pub(super) fn attributes(told: Told, attributes: WindowAttributes) -> WindowAttributes {
    let attributes = if told.opaque { attributes.with_transparent(false) } else { attributes };
    #[cfg(target_os = "linux")]
    let attributes = {
        use slint::winit_030::winit::platform::x11::{WindowAttributesExtX11, WindowType};
        let attributes = if told.unmanaged { attributes.with_override_redirect(true) } else { attributes };
        if told.utility {
            attributes.with_x11_window_type(vec![WindowType::Utility])
        } else {
            attributes
        }
    };
    #[cfg(windows)]
    let attributes = {
        use slint::winit_030::winit::platform::windows::WindowAttributesExtWindows;
        let attributes = if told.unlisted { attributes.with_skip_taskbar(true) } else { attributes };
        if told.made_inactive {
            attributes.with_active(false)
        } else {
            attributes
        }
    };
    attributes
}

/// What can only be said of a window once winit has made it.
pub(super) fn made(told: Told, window: &Window) {
    if told.above {
        window.set_window_level(WindowLevel::AlwaysOnTop);
    }
    #[cfg(windows)]
    if told.never_active {
        if let Some(window) = windows::handle(window) {
            use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW};
            // SAFETY: a window of this thread's that is there.
            unsafe {
                let style = GetWindowLongPtrW(window, GWL_EXSTYLE);
                SetWindowLongPtrW(window, GWL_EXSTYLE, style | (WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW) as isize);
            }
        }
    }
}

/// The window that has every key, as it is known outside Slint's thread:
/// 0 when none has.
static HOLDER: AtomicU64 = AtomicU64::new(0);

/// Puts a kept window on the screen, as its standing has it there.
pub(super) fn on(told: Told, window: &Window) {
    if told.keys != Keys::Left {
        HOLDER.store(id(window), Ordering::SeqCst);
    }
    #[cfg(windows)]
    if told.never_active {
        if let Some(window) = windows::handle(window) {
            use windows_sys::Win32::UI::WindowsAndMessaging::{SetWindowPos, ShowWindow, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SW_SHOWNOACTIVATE};
            // SAFETY: as above.
            unsafe {
                ShowWindow(window, SW_SHOWNOACTIVATE);
                SetWindowPos(window, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            }
        }
        return;
    }
    window.set_visible(true);
    #[cfg(target_os = "linux")]
    if told.raised {
        if let Some(x11) = x11::of(window) {
            // SAFETY: winit's display, on the thread winit runs on.
            unsafe {
                (x11.xlib.XRaiseWindow)(x11.display, x11.window);
                (x11.xlib.XFlush)(x11.display);
            }
        }
    }
}

/// Takes every key for a window that is to have them, now that it is on
/// the screen. False when it was not given them yet, which is so while
/// the hotkey that brought the window up is held (the desktop's own grab
/// on it is still there): it is to be asked again.
pub(super) fn hold(told: Told, window: &Window) -> bool {
    match told.keys {
        Keys::Left => true,
        Keys::Focused => {
            window.focus_window();
            true
        }
        Keys::Grabbed => {
            #[cfg(target_os = "linux")]
            if let Some(x11) = x11::of(window) {
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
    }
}

/// Takes a kept window off the screen; it stays, for the next time. A
/// window that had every key gives them back first.
pub(super) fn off(told: Told, window: &Window) {
    if told.keys != Keys::Left {
        HOLDER.store(0, Ordering::SeqCst);
    }
    #[cfg(target_os = "linux")]
    if told.keys == Keys::Grabbed {
        if let Some(x11) = x11::of(window) {
            // SAFETY: as above.
            unsafe {
                (x11.xlib.XUngrabKeyboard)(x11.display, x11_dl::xlib::CurrentTime);
            }
        }
    }
    #[cfg(windows)]
    if told.never_active {
        if let Some(window) = windows::handle(window) {
            use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};
            // SAFETY: as above.
            unsafe {
                ShowWindow(window, SW_HIDE);
            }
        }
        return;
    }
    window.set_visible(false);
    #[cfg(target_os = "linux")]
    if let Some(x11) = x11::of(window) {
        // Now, not when winit next writes to the server.
        unsafe {
            (x11.xlib.XFlush)(x11.display);
        }
    }
}

/// Takes the window that has every key off the screen, from any thread,
/// when Slint's does not answer: by a connection of its own, since
/// winit's is that thread's. The server lets the keyboard go when the
/// window that holds it is no longer on the screen. (X11, where a window
/// can hold the keyboard against everything; elsewhere the desktop still
/// has its own ways of taking the focus away.)
pub(super) fn take_down() {
    let id = HOLDER.swap(0, Ordering::SeqCst);
    #[cfg(target_os = "linux")]
    {
        use x11rb::protocol::xproto::ConnectionExt;
        if id == 0 || desktop() != Desktop::X11 {
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
            eprintln!("heyo: the window that holds the keyboard ({id:#x}) could not be taken off the screen from outside");
        }
    }
    #[cfg(not(target_os = "linux"))]
    let _ = id;
}

/// What a window is known by outside Slint's thread. 0 where there is no
/// such thing.
fn id(window: &Window) -> u64 {
    #[cfg(target_os = "linux")]
    if let Some(x11) = x11::of(window) {
        return x11.window as u64;
    }
    let _ = window;
    0
}

/// Has the window take the pointer in `within` only, in its own device
/// pixels, or everywhere: what is seen through of a window is not the
/// window to whoever looks at the screen, and a press there belongs to
/// what is seen there.
///
/// - X11: the window is given an input shape (XFixes, on winit's own
///   connection). To the server and to the window manager the rest of it
///   is then not there for the pointer.
/// - Wayland has the same thing (a surface's input region), but winit
///   offers it only as all or nothing. Not done.
/// - Windows has none for a window drawn as these are: a window's region
///   cuts what is drawn along with what is pressed, a shadow too, and only
///   a layered window lets the pointer through where it is clear. Not
///   done.
///
/// Where it is not done, a press on what is seen through comes to Slint,
/// which has nothing there, and is lost.
pub(super) fn reach(window: &Window, within: Option<Rect>) {
    #[cfg(target_os = "linux")]
    if let (Some(x11), Ok(xfixes)) = (x11::of(window), x11_dl::xfixes::Xlib::open()) {
        /// Of a window's shapes, the one the pointer goes by (the shape extension's ShapeInput).
        const INPUT: std::ffi::c_int = 2;
        // SAFETY: winit's display and a window of it that is there, on
        // the thread winit runs on.
        unsafe {
            match within {
                Some((x, y, width, height)) => {
                    let mut only = x11_dl::xlib::XRectangle { x: x as i16, y: y as i16, width: width.min(u16::MAX as u32) as u16, height: height.min(u16::MAX as u32) as u16 };
                    let region = (xfixes.XFixesCreateRegion)(x11.display, &mut only, 1);
                    (xfixes.XFixesSetWindowShapeRegion)(x11.display, x11.window, INPUT, 0, 0, region);
                    (xfixes.XFixesDestroyRegion)(x11.display, region);
                }
                // No region: the window's own bounds again.
                None => (xfixes.XFixesSetWindowShapeRegion)(x11.display, x11.window, INPUT, 0, 0, 0),
            }
            (x11.xlib.XFlush)(x11.display);
        }
    }
    #[cfg(not(target_os = "linux"))]
    let _ = (window, within);
}

/// A window on winit's own connection to the X server: the one place its
/// handles are taken.
#[cfg(target_os = "linux")]
pub(super) mod x11 {
    use slint::winit_030::winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
    use slint::winit_030::winit::window::Window;

    pub(in super::super) struct OnX11 {
        pub xlib: &'static x11_dl::xlib::Xlib,
        pub display: *mut x11_dl::xlib::Display,
        pub window: std::ffi::c_ulong,
    }

    /// None on Wayland.
    pub(in super::super) fn of(window: &Window) -> Option<OnX11> {
        thread_local! {
            /// libX11 as winit loaded it: the same library, so its display is ours to call with.
            static XLIB: Option<&'static x11_dl::xlib::Xlib> = x11_dl::xlib::Xlib::open().ok().map(|xlib| &*Box::leak(Box::new(xlib)));
        }
        let RawDisplayHandle::Xlib(display) = window.display_handle().ok()?.as_raw() else { return None };
        let RawWindowHandle::Xlib(handle) = window.window_handle().ok()?.as_raw() else { return None };
        Some(OnX11 { xlib: XLIB.with(|xlib| *xlib)?, display: display.display?.as_ptr().cast(), window: handle.window })
    }
}

#[cfg(windows)]
pub(super) mod windows {
    use slint::winit_030::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use slint::winit_030::winit::window::Window;

    pub(in super::super) fn handle(window: &Window) -> Option<windows_sys::Win32::Foundation::HWND> {
        match window.window_handle().ok()?.as_raw() {
            RawWindowHandle::Win32(handle) => Some(handle.hwnd.get() as windows_sys::Win32::Foundation::HWND),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SETTINGS: Standing = Standing { layer: Layer::Among, keyboard: Keyboard::Ordinary, listed: true, clear: true };
    const PINNED: Standing = Standing { layer: Layer::Above, keyboard: Keyboard::Ordinary, listed: false, clear: false };
    const PILL: Standing = Standing { layer: Layer::Over, keyboard: Keyboard::Never, listed: false, clear: true };
    const OVERLAY: Standing = Standing { layer: Layer::Over, keyboard: Keyboard::All, listed: false, clear: false };

    const NOTHING: Told = Told { unmanaged: false, utility: false, unlisted: false, made_inactive: false, never_active: false, above: false, raised: false, keys: Keys::Left, opaque: false, kept: false, unplaced: false };

    #[test]
    fn an_ordinary_window_is_told_nothing_of_anywhere() {
        for desktop in [Desktop::X11, Desktop::Wayland, Desktop::Windows, Desktop::Other] {
            assert_eq!(told(SETTINGS, desktop), NOTHING, "{desktop:?}");
        }
    }

    #[test]
    fn on_x11_a_window_over_everything_goes_past_the_window_manager() {
        // Which is also what keeps the keyboard from it.
        assert_eq!(told(PILL, Desktop::X11), Told { unmanaged: true, kept: true, ..NOTHING });
        // And one that is to have every key grabs it, and is raised.
        assert_eq!(told(OVERLAY, Desktop::X11), Told { unmanaged: true, kept: true, raised: true, keys: Keys::Grabbed, opaque: true, ..NOTHING });
        // One above the others is still the window manager's.
        assert_eq!(told(PINNED, Desktop::X11), Told { utility: true, above: true, opaque: true, ..NOTHING });
    }

    #[test]
    fn on_windows_a_window_without_the_keyboard_is_never_activated() {
        // Not when it is made at start, not by a click, not by being shown.
        assert_eq!(told(PILL, Desktop::Windows), Told { unlisted: true, made_inactive: true, never_active: true, kept: true, ..NOTHING });
        // The overlay is not activated when it is made at start either,
        // and asks for the focus when it is shown.
        assert_eq!(told(OVERLAY, Desktop::Windows), Told { unlisted: true, made_inactive: true, keys: Keys::Focused, opaque: true, kept: true, ..NOTHING });
        assert_eq!(told(PINNED, Desktop::Windows), Told { unlisted: true, above: true, opaque: true, ..NOTHING });
    }

    #[test]
    fn on_wayland_nothing_of_it_can_be_said_and_no_window_is_kept() {
        assert_eq!(told(PILL, Desktop::Wayland), Told { unplaced: true, ..NOTHING });
        assert_eq!(told(OVERLAY, Desktop::Wayland), Told { unplaced: true, keys: Keys::Focused, opaque: true, ..NOTHING });
        assert!(![SETTINGS, PINNED, PILL, OVERLAY].iter().any(|standing| told(*standing, Desktop::Wayland).kept));
    }

    #[test]
    fn a_window_has_every_key_only_where_it_is_given_a_way_to_let_go() {
        // Only one that is kept is put on and taken off the screen here,
        // which is where the keys are taken and given back; and the grab
        // is only ever of a window that goes past the window manager.
        for desktop in [Desktop::X11, Desktop::Windows, Desktop::Other] {
            for standing in [SETTINGS, PINNED, PILL, OVERLAY] {
                let told = told(standing, desktop);
                assert!(told.keys != Keys::Grabbed || (told.kept && told.unmanaged), "{desktop:?} {standing:?}");
            }
        }
    }
}

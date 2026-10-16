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


use slint::winit_030::winit::window::{Window, WindowAttributes, WindowLevel};

use crate::platform::desktop::{OwnPlace, OwnStanding};
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
    /// A Wayland compositor without the layer shell (GNOME).
    Wayland,
    /// A Wayland compositor that offers the layer shell (Sway and the
    /// other wlroots compositors, KWin, COSMIC).
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Layers,
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
    /// The window cannot say where it goes, nor that it is over everything
    /// or without the keyboard: on Wayland all of that is the compositor's
    /// to decide. So the desktop itself is asked (`ask_desktop`): under
    /// GNOME, Heyo's Shell extension gives the window of that title its
    /// standing (above the others, on every workspace, the keyboard left
    /// with or returned to the window that had it) and its place. Under
    /// another compositor nobody answers, and the window is an ordinary
    /// one where the compositor puts it. (And winit cannot take a window
    /// off the screen there, which is why none is kept, and none is made
    /// before it is wanted: a window that exists is on the screen.)
    pub unplaced: bool,
    /// The window is a layer surface of Heyo's own (`layer`), where the
    /// compositor offers that: on the overlay layer, so over everything on
    /// every workspace; anchored where its window says; never with the
    /// keyboard; taking the pointer where it is told. It is not one of
    /// winit's windows, and nothing else need be asked of anybody. (The
    /// pill, never with the keyboard; a pinned capture, which there has
    /// the pointer only; and the overlay, over all of an output with every
    /// key while it is there.)
    pub layered: bool,
}

/// What is done about a standing on a desktop.
pub(super) fn told(standing: Standing, desktop: Desktop) -> Told {
    let (over, all) = (standing.layer == Layer::Over, standing.keyboard == Keyboard::All);
    let nothing = Told { unmanaged: false, utility: false, unlisted: false, made_inactive: false, never_active: false, above: standing.layer == Layer::Above, raised: false, keys: Keys::Left, opaque: !standing.clear, kept: false, unplaced: false, layered: false };
    let above = standing.layer == Layer::Above;
    let layered = over || above;
    match desktop {
        Desktop::X11 => Told { unmanaged: over, utility: !over && !standing.listed, raised: over && all, keys: if all { Keys::Grabbed } else { Keys::Left }, kept: over, ..nothing },
        Desktop::Wayland => Told { keys: if all { Keys::Focused } else { Keys::Left }, unplaced: over || above, ..nothing },
        Desktop::Layers => Told { keys: if all { Keys::Focused } else { Keys::Left }, unplaced: over && !layered, layered, ..nothing },
        Desktop::Windows => Told { unlisted: !standing.listed, made_inactive: over, never_active: standing.keyboard == Keyboard::Never, keys: if all { Keys::Focused } else { Keys::Left }, kept: over, ..nothing },
        Desktop::Other => Told { keys: if all { Keys::Focused } else { Keys::Left }, kept: over, ..nothing },
    }
}

/// Asks the desktop itself for the standing of a window that cannot say
/// it there (`Told::unplaced`), before the window is shown: the window is
/// known to the desktop by its title, and `at` says where it goes. It
/// does not wait: the desktop gives the standing to the window when it
/// comes, or to the one that is there.
pub(super) fn ask_desktop(standing: Standing, told: Told, window: &slint::Window, title: &'static str, at: Option<OwnPlace>) {
    // A layer surface is put where it says by the compositor itself.
    #[cfg(target_os = "linux")]
    if told.layered {
        return super::layer::place(window, at);
    }
    let _ = window;
    if !told.unplaced {
        return;
    }
    let own = OwnStanding { above: true, everywhere: standing.layer == Layer::Over, keyboard: standing.keyboard != Keyboard::Never, at };
    let _ = std::thread::Builder::new().name("heyo-slint-standing".into()).spawn(move || {
        crate::platform::desktop::desktop().stand_own(title, &own);
    });
}

/// The desktop Heyo's windows are on.
pub(super) fn desktop() -> Desktop {
    if cfg!(windows) {
        Desktop::Windows
    } else if super::thread::wayland() {
        #[cfg(target_os = "linux")]
        if super::layer::offered() {
            return Desktop::Layers;
        }
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
    #[cfg(target_os = "macos")]
    let attributes = {
        use slint::winit_030::winit::platform::macos::WindowAttributesExtMacOS;

        // Every Slint window draws its own shadow into the transparent
        // margin. AppKit's additional NSWindow shadow otherwise sits outside
        // that margin and makes the window look like a dark rectangular box.
        attributes.with_has_shadow(false)
    };
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

/// Puts a kept window on the screen, as its standing has it there.
pub(super) fn on(told: Told, window: &Window) {
    // One that was taken off the screen from outside is drawn again.
    #[cfg(windows)]
    if let Some(window) = windows::handle(window) {
        windows::cloak(window, false);
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

/// What a window on the screen is known by outside Slint's thread, for
/// whoever has to take it off the screen when that thread does not
/// answer (`watch`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Outside {
    /// Nothing: it cannot be taken off from outside (a window of winit's
    /// on Wayland, which is on winit's connection and known to nobody
    /// else).
    None,
    /// The X server's number for it.
    #[cfg(target_os = "linux")]
    X11(u32),
    /// A layer surface, by the compositor's number for it.
    #[cfg(target_os = "linux")]
    Layer(u64),
    /// Its handle on Windows.
    #[cfg(windows)]
    Windows(isize),
}

/// What a window that is on the screen is known by outside.
pub(super) fn outside(told: Told, window: &slint::Window) -> Outside {
    use slint::winit_030::WinitWindowAccessor;
    #[cfg(target_os = "linux")]
    {
        if told.layered {
            return super::layer::outside(window).map_or(Outside::None, Outside::Layer);
        }
        window.with_winit_window(|made| x11::of(made).map(|x11| x11.window as u32)).flatten().map_or(Outside::None, Outside::X11)
    }
    #[cfg(windows)]
    {
        let _ = told;
        window.with_winit_window(|made| windows::handle(made).map(|handle| handle as isize)).flatten().map_or(Outside::None, Outside::Windows)
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        let _ = (told, window.has_winit_window());
        Outside::None
    }
}

/// Takes a window off the screen, from any thread, when Slint's does not
/// answer. False when it could not be.
///
/// - X11: by a connection of its own, since winit's is that thread's. The
///   server lets the keyboard go when the window that holds it is no
///   longer on the screen.
/// - A layer surface: by its own name, on the connection Slint's thread
///   has (`layer::take_down`).
/// - Windows: a window is its thread's, and one whose thread does not
///   answer cannot be hidden or moved by another. What another thread
///   may do is have the desktop stop drawing it (it is cloaked), after
///   which it is not there for the pointer either; `on` has it drawn
///   again. Written from the documentation and not tried on Windows yet.
pub(super) fn take_down(outside: Outside) -> bool {
    match outside {
        Outside::None => false,
        #[cfg(target_os = "linux")]
        Outside::Layer(surface) => super::layer::take_down(surface),
        #[cfg(target_os = "linux")]
        Outside::X11(id) => {
            use x11rb::protocol::xproto::ConnectionExt;
            x11rb::connect(None).ok().and_then(|(x, _)| {
                x.unmap_window(id).ok()?.check().ok()?;
                // Asked back, so that the server has done it before this
                // connection ends.
                let now = x.get_window_attributes(id).ok()?.reply().ok()?;
                Some(now.map_state == x11rb::protocol::xproto::MapState::UNMAPPED)
            }) == Some(true)
        }
        #[cfg(windows)]
        Outside::Windows(handle) => windows::cloak(handle as windows_sys::Win32::Foundation::HWND, true),
    }
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

/// What the pointer did in a window, for a window that hears of it
/// itself (a pinned capture, which is moved by it): places are in the
/// window's device pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Pointed {
    Moved((f64, f64)),
    Pressed(Button),
    Released(Button),
    /// It left the window.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Left,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Button {
    Left,
    Right,
}

/// What the keys did in a window that is not winit's and has the
/// keyboard, as whoever hears of it for the window is told.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(super) enum Keyed {
    /// A key went down or up; `again` when it is pressed once more
    /// because it stays down.
    Key { key: KeyOf, down: bool, again: bool },
    /// Which of the keys held with others are down, as the desktop says.
    Held { ctrl: bool, alt: bool },
}

/// Which key, as far as a window of Heyo's own has a rule about it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(super) enum KeyOf {
    Escape,
    Enter,
    F4,
    Control,
    Alt,
    /// One that makes a character: the one it makes without the keys held
    /// with it, and the letter at its place on the layout the letters are
    /// named after, if a letter is there.
    Character { made: char, placed: Option<char> },
    Other,
}

/// How the pointer is to look over a window, whoever's it is.
pub(super) fn cursor_of(told: Told, window: &slint::Window, icon: slint::winit_030::winit::window::CursorIcon) {
    #[cfg(target_os = "linux")]
    if told.layered {
        return super::layer::cursor(window, icon);
    }
    let _ = told;
    use slint::winit_030::WinitWindowAccessor;
    window.with_winit_window(|made| made.set_cursor(icon));
}

/// Has a window that is not winit's and has the keyboard hear of the keys
/// (winit's hear of them from winit), before Slint does: `hear` says
/// whether Slint is to have a key too.
pub(super) fn keyed_of(told: Told, window: &slint::Window, hear: impl Fn(Keyed) -> bool + 'static) {
    #[cfg(target_os = "linux")]
    if told.layered {
        return super::layer::hear_keys(window, hear);
    }
    let _ = (told, window, hear);
}

/// Has a window that is not winit's hear what the pointer does in it
/// (winit's hear of it from winit), before Slint does: `hear` says
/// whether Slint is to have it too.
pub(super) fn pointed_of(told: Told, window: &slint::Window, hear: impl Fn(Pointed) -> bool + 'static) {
    #[cfg(target_os = "linux")]
    if told.layered {
        return super::layer::hear(window, hear);
    }
    let _ = (told, window, hear);
}

/// Begins moving a window by the pointer that is pressed in it. True
/// where the desktop moves it from here on and has the pointer until the
/// button is let go; false where nobody does, and the window is to be
/// moved by as much as the pointer goes (`move_by`).
pub(super) fn drag(told: Told, window: &slint::Window) -> bool {
    if told.layered {
        return false;
    }
    use slint::winit_030::WinitWindowAccessor;
    window.with_winit_window(|made| {
        let _ = made.drag_window();
    });
    true
}

/// Moves a window that nobody else moves, by so many of its device
/// pixels, and says by how many it went.
pub(super) fn move_by(told: Told, window: &slint::Window, by: (f64, f64)) -> (f64, f64) {
    #[cfg(target_os = "linux")]
    if told.layered {
        return super::layer::move_by(window, by);
    }
    let _ = (told, window, by);
    (0.0, 0.0)
}

/// How far a window that moves itself under a pressed pointer has still
/// to go, when the pointer is `gone` from where it was pressed (as the
/// window is told, in its own pixels) and the window has `moved` so far.
///
/// A compositor says where the pointer is in the window, and compositors
/// differ on a window that moves while the button is down: by the window
/// as it is now (then `gone` is what is left to go), or by the window as
/// it was when the button went down (Sway: then `gone` is all the pointer
/// went, of which `moved` is done). Whichever leaves less to go is the
/// one this compositor means: a pointer goes a little between two
/// tellings, and a window that follows it has gone nearly all the way.
pub(super) fn left_to_go(gone: (f64, f64), moved: (f64, f64)) -> (f64, f64) {
    let since = (gone.0 - moved.0, gone.1 - moved.1);
    if since.0.hypot(since.1) < gone.0.hypot(gone.1) {
        since
    } else {
        gone
    }
}

/// Whether what is behind a window shows where it draws nothing, for a
/// window that is not winit's and so not drawn by the renderer the others
/// are: a layer surface always is seen through. None for winit's windows,
/// where that depends on what draws them (`thread::seen_through`).
pub(super) fn drawn_clear(window: &slint::Window) -> Option<bool> {
    #[cfg(target_os = "linux")]
    if super::layer::is(window) {
        return Some(true);
    }
    let _ = window;
    None
}

/// The same for a window that may not be winit's: a layer surface says
/// it to the compositor.
pub(super) fn reach_of(told: Told, window: &slint::Window, within: Option<Rect>) {
    #[cfg(target_os = "linux")]
    if told.layered {
        return super::layer::reach(window, within);
    }
    let _ = told;
    use slint::winit_030::WinitWindowAccessor;
    window.with_winit_window(|made| reach(made, within));
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

    /// Has the desktop stop drawing a window, or draw it again. From any
    /// thread. False when the desktop would not.
    #[cfg(windows)]
    pub(in super::super) fn cloak(window: windows_sys::Win32::Foundation::HWND, cloaked: bool) -> bool {
        use windows_sys::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_CLOAK};
        let value: i32 = cloaked.into();
        // SAFETY: a window of this process's, and a value of the size said.
        unsafe { DwmSetWindowAttribute(window, DWMWA_CLOAK as u32, std::ptr::addr_of!(value).cast(), std::mem::size_of::<i32>() as u32) >= 0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SETTINGS: Standing = Standing { layer: Layer::Among, keyboard: Keyboard::Ordinary, listed: true, clear: true };
    const PINNED: Standing = Standing { layer: Layer::Above, keyboard: Keyboard::Ordinary, listed: false, clear: false };
    const PILL: Standing = Standing { layer: Layer::Over, keyboard: Keyboard::Never, listed: false, clear: true };
    const OVERLAY: Standing = Standing { layer: Layer::Over, keyboard: Keyboard::All, listed: false, clear: false };

    const NOTHING: Told = Told { unmanaged: false, utility: false, unlisted: false, made_inactive: false, never_active: false, above: false, raised: false, keys: Keys::Left, opaque: false, kept: false, unplaced: false, layered: false };

    #[test]
    fn where_wayland_has_the_layer_shell_the_pill_is_a_layer_surface_and_nobody_is_asked() {
        assert_eq!(told(PILL, Desktop::Layers), Told { layered: true, ..NOTHING });
        // The overlay is one that covers an output and has every key.
        assert_eq!(told(OVERLAY, Desktop::Layers), Told { layered: true, keys: Keys::Focused, opaque: true, ..NOTHING });
        // A pinned capture is one too, with the pointer only.
        assert_eq!(told(PINNED, Desktop::Layers), Told { above: true, opaque: true, layered: true, ..NOTHING });
        // And where it has not (GNOME), the desktop itself is asked.
        assert!(!told(PILL, Desktop::Wayland).layered && told(PILL, Desktop::Wayland).unplaced);
        // A layer surface is never one of those winit keeps.
        assert!(![SETTINGS, PINNED, PILL, OVERLAY].iter().any(|standing| told(*standing, Desktop::Layers).kept));
    }

    #[test]
    fn a_window_that_moves_itself_follows_the_pointer_whichever_way_it_is_told() {
        // Told by the window as it was when the button went down: the
        // pointer has gone 100,50 in all, of which 90,45 is done.
        assert_eq!(left_to_go((100.0, 50.0), (90.0, 45.0)), (10.0, 5.0));
        // Told by the window as it is now: 10,5 is what is left.
        assert_eq!(left_to_go((10.0, 5.0), (90.0, 45.0)), (10.0, 5.0));
        // At the start the two are one.
        assert_eq!(left_to_go((6.0, 0.0), (0.0, 0.0)), (6.0, 0.0));
        // And back the way it came, past where it was pressed.
        assert_eq!(left_to_go((-20.0, 0.0), (-16.0, 0.0)), (-4.0, 0.0));
    }

    #[test]
    fn an_ordinary_window_is_told_nothing_of_anywhere() {
        for desktop in [Desktop::X11, Desktop::Wayland, Desktop::Layers, Desktop::Windows, Desktop::Other] {
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
        // A pinned capture cannot say where it was read either.
        assert_eq!(told(PINNED, Desktop::Wayland), Told { unplaced: true, above: true, opaque: true, ..NOTHING });
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

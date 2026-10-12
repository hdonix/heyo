//! The pointer over Slint's windows, after the desktop changed its size or
//! its theme.
//!
//! On X11 every program draws the pointer over its own windows, from
//! pictures it loads itself at the size the desktop publishes. winit loads
//! a shape the first time a window wants it and keeps it for as long as it
//! runs; and libXcursor, which it loads them with, takes the size and the
//! theme from what its connection was told when it was opened. So when a
//! monitor of another density was connected, the pointer over Heyo's
//! windows stayed the size it had been: half that over every other window.
//!
//! winit cannot be told. What is done when the display changes (`follow`)
//! is what GTK does for its own pointers:
//!
//! - a connection is opened anew, on which the library finds the size and
//!   the theme as a program started now would;
//! - winit's connection is told that size and theme, for the shapes it has
//!   yet to load;
//! - every pointer winit has loaded is replaced in the server by the same
//!   shape loaded now (XFixes' ChangeCursor: the pointer winit knows by a
//!   number is the new one from then on, in every window that shows it).
//!   winit does not say which pointers are its own, so they are found: the
//!   server is asked, of every number winit's connection has given out,
//!   whether it is a pointer with a name.
//!
//! Nothing of this is needed on Windows, where the system draws the
//! pointer, or on Wayland, where winit is told the scale of each window's
//! surface and loads for it.

use slint::winit_030::winit::window::Window;

/// Notes winit's connection, which is known through a window of it. On
/// Slint's thread.
pub(super) fn remember(window: &Window) {
    #[cfg(target_os = "linux")]
    x11::remember(window);
    #[cfg(not(target_os = "linux"))]
    let _ = window;
}

/// Has the pointer over Slint's windows be what the desktop now says. On
/// Slint's thread.
pub(super) fn follow() {
    #[cfg(target_os = "linux")]
    x11::follow();
}

/// The numbers a connection has given out so far: `known` is one of them,
/// `last` the latest, and `mask` the bits a server leaves to a connection
/// to count in. None when `last` is not of the same connection.
#[cfg(any(target_os = "linux", test))]
fn given_out(known: u32, last: u32, mask: u32) -> impl Iterator<Item = u32> {
    let base = known & !mask;
    let step = mask.trailing_zeros().min(31);
    let count = if mask != 0 && last & !mask == base { ((last & mask) >> step) + 1 } else { 0 };
    (0..count).map(move |at| base | (at << step))
}

#[cfg(target_os = "linux")]
mod x11 {
    use std::cell::Cell;
    use std::ffi::{c_ulong, CString};

    use slint::winit_030::winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
    use slint::winit_030::winit::window::Window;
    use x11_dl::xcursor::Xcursor;
    use x11_dl::xlib::{self, Xlib};
    use x11rb::connection::Connection;
    use x11rb::protocol::xfixes::ConnectionExt;

    thread_local! {
        /// winit's connection, and a number it gave out.
        static WINIT: Cell<Option<(*mut xlib::Display, c_ulong)>> = const { Cell::new(None) };
    }

    pub(super) fn remember(window: &Window) {
        let Some(RawDisplayHandle::Xlib(display)) = window.display_handle().ok().map(|handle| handle.as_raw()) else { return };
        let Some(RawWindowHandle::Xlib(handle)) = window.window_handle().ok().map(|handle| handle.as_raw()) else { return };
        if let Some(display) = display.display {
            WINIT.set(Some((display.as_ptr().cast(), handle.window)));
        }
    }

    pub(super) fn follow() {
        let Some((display, known)) = WINIT.get() else { return };
        match replace(display, known as u32) {
            Ok((size, replaced)) => eprintln!("heyo: the pointer over Slint's windows is of size {size} now ({replaced} loaded again)"),
            Err(reason) => eprintln!("heyo: the pointer over Slint's windows keeps the size it had: {reason}"),
        }
    }

    /// Says the size they are now, and how many were loaded again.
    fn replace(theirs: *mut xlib::Display, known: u32) -> Result<(i32, usize), String> {
        let xlib = Xlib::open().map_err(|e| e.to_string())?;
        let xcursor = Xcursor::open().map_err(|e| e.to_string())?;
        // SAFETY: `theirs` is winit's display, which lives as long as
        // Slint's thread, and this is that thread; `fresh` is opened and
        // closed here and used nowhere else.
        unsafe {
            let fresh = (xlib.XOpenDisplay)(std::ptr::null());
            if fresh.is_null() {
                return Err("the display could not be opened again".into());
            }
            let done = (|| {
                let size = (xcursor.XcursorGetDefaultSize)(fresh);
                (xcursor.XcursorSetDefaultSize)(theirs, size);
                (xcursor.XcursorSetTheme)(theirs, (xcursor.XcursorGetTheme)(fresh));
                // How far winit's connection has counted: the number it gives next.
                let last = (xlib.XCreatePixmap)(theirs, (xlib.XDefaultRootWindow)(theirs), 1, 1, 1);
                (xlib.XFreePixmap)(theirs, last);
                (xlib.XFlush)(theirs);

                // Asked on a connection of its own: most of the numbers are
                // not pointers, and the server's saying so must not reach winit.
                let (asking, _) = x11rb::connect(None).map_err(|e| e.to_string())?;
                asking.xfixes_query_version(4, 0).map_err(|e| e.to_string())?.reply().map_err(|e| e.to_string())?;
                let numbers: Vec<u32> = super::given_out(known, last as u32, asking.setup().resource_id_mask).collect();
                let mut replaced = 0;
                for some in numbers.chunks(1024) {
                    let asked: Vec<_> = some.iter().filter_map(|number| Some((*number, asking.xfixes_get_cursor_name(*number).ok()?))).collect();
                    for (old, answer) in asked {
                        let Some(name) = answer.reply().ok().and_then(|answer| CString::new(answer.name).ok()).filter(|name| !name.as_bytes().is_empty()) else { continue };
                        let new = (xcursor.XcursorLibraryLoadCursor)(fresh, name.as_ptr());
                        if new == 0 {
                            continue;
                        }
                        // The server has it before it is named to it on the other connection.
                        (xlib.XSync)(fresh, xlib::False);
                        if asking.xfixes_change_cursor(new as u32, old).ok().and_then(|changed| changed.check().ok()).is_some() {
                            replaced += 1;
                        }
                    }
                }
                Ok((size, replaced))
            })();
            // The new pointers stay: winit's numbers are theirs now.
            (xlib.XCloseDisplay)(fresh);
            done
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_numbers_a_connection_gave_out_are_those_from_its_first_to_its_last() {
        // As an X server hands them out: 21 bits to count in.
        let given: Vec<u32> = given_out(0x0460_0007, 0x0460_0003, 0x001f_ffff).collect();
        assert_eq!(given, [0x0460_0000, 0x0460_0001, 0x0460_0002, 0x0460_0003]);
        // Counted in other bits than the lowest.
        let given: Vec<u32> = given_out(0x0460_0010, 0x0460_0020, 0x001f_fff0).collect();
        assert_eq!(given, [0x0460_0000, 0x0460_0010, 0x0460_0020]);
    }

    #[test]
    fn a_number_of_another_connection_is_the_end_of_none() {
        assert_eq!(given_out(0x0460_0007, 0x0480_0003, 0x001f_ffff).count(), 0);
        assert_eq!(given_out(0x0460_0007, 0x0460_0003, 0).count(), 0);
    }
}

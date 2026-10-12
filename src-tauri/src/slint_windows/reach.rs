//! Where in its window a window takes the pointer.
//!
//! The settings window is larger than the panel it draws: the margin
//! round the panel is there for the shadow to fall in, and to whoever
//! looks at the screen it is not the window at all. A press there belongs
//! to what is seen there, which is whatever is underneath.
//!
//! - X11: the window is given an input shape, the panel's rectangle
//!   (XFixes, on winit's own connection). To the server and to the window
//!   manager the margin is then not there for the pointer.
//! - Wayland has the same thing (a surface's input region), but winit
//!   offers it only as all or nothing. Not done.
//! - Windows has none for a window drawn as these are: a window's region
//!   cuts what is drawn along with what is pressed, the shadow too, and
//!   only a layered window lets the pointer through where it is clear.
//!   Not done.
//!
//! Where it is not done, a press in the margin comes to Slint, which has
//! nothing there, and is lost.

use slint::winit_030::winit::window::Window;

use super::area::Rect;

/// The panel in a window of `size` device pixels that keeps `margin`
/// logical pixels round it, drawn at `scale`. None when the window keeps
/// no margin, or is all margin: it is then all of it that takes the
/// pointer.
pub(super) fn panel(size: (u32, u32), margin: f32, scale: f32) -> Option<Rect> {
    let edge = (margin * scale).round().max(0.0) as u32;
    (edge > 0 && size.0 > 2 * edge && size.1 > 2 * edge).then(|| (edge as i32, edge as i32, size.0 - 2 * edge, size.1 - 2 * edge))
}

/// Has the window take the pointer in `within` only, in its own device
/// pixels, or everywhere. On Slint's thread.
pub(super) fn limit(window: &Window, within: Option<Rect>) {
    #[cfg(target_os = "linux")]
    {
        use slint::winit_030::winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
        use x11_dl::xlib;
        /// Of a window's shapes, the one the pointer goes by (the shape extension's ShapeInput).
        const INPUT: std::ffi::c_int = 2;
        // Neither on Wayland.
        let Some(RawDisplayHandle::Xlib(display)) = window.display_handle().ok().map(|handle| handle.as_raw()) else { return };
        let Some(RawWindowHandle::Xlib(handle)) = window.window_handle().ok().map(|handle| handle.as_raw()) else { return };
        let (Some(display), Ok(xfixes), Ok(xlib)) = (display.display, x11_dl::xfixes::Xlib::open(), xlib::Xlib::open()) else { return };
        let display: *mut xlib::Display = display.as_ptr().cast();
        // SAFETY: winit's display and a window of it that is there, on the
        // thread winit runs on.
        unsafe {
            match within {
                Some((x, y, width, height)) => {
                    let mut panel = xlib::XRectangle { x: x as i16, y: y as i16, width: width.min(u16::MAX as u32) as u16, height: height.min(u16::MAX as u32) as u16 };
                    let region = (xfixes.XFixesCreateRegion)(display, &mut panel, 1);
                    (xfixes.XFixesSetWindowShapeRegion)(display, handle.window, INPUT, 0, 0, region);
                    (xfixes.XFixesDestroyRegion)(display, region);
                }
                // No region: the window's own bounds again.
                None => (xfixes.XFixesSetWindowShapeRegion)(display, handle.window, INPUT, 0, 0, 0),
            }
            (xlib.XFlush)(display);
        }
    }
    #[cfg(not(target_os = "linux"))]
    let _ = (window, within);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_panel_is_the_window_without_its_margin() {
        // The settings window as it is at scale 1 and at scale 2.
        assert_eq!(panel((924, 704), 32.0, 1.0), Some((32, 32, 860, 640)));
        assert_eq!(panel((1848, 1408), 32.0, 2.0), Some((64, 64, 1720, 1280)));
        // At a scale between, the margin is whole pixels each side.
        assert_eq!(panel((1155, 880), 32.0, 1.25), Some((40, 40, 1075, 800)));
    }

    #[test]
    fn a_window_without_a_margin_takes_the_pointer_everywhere() {
        // On a screen too small for the shadow.
        assert_eq!(panel((860, 640), 0.0, 1.0), None);
        // And one that would be nothing but margin.
        assert_eq!(panel((60, 60), 32.0, 1.0), None);
    }
}

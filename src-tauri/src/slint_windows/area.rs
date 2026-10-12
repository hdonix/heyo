//! The room a screen has for a window: the screen without the desktop's
//! panels and docks. The settings window is made no larger than that, and
//! is put in the middle of it.
//!
//! winit knows a monitor's size but not what the desktop keeps of it, so
//! the desktop is asked: on X11 the window manager says (`_NET_WORKAREA`),
//! on Windows the system does. Elsewhere (Wayland, which tells no client)
//! the room is the monitor.

/// A rectangle of the screen, in its pixels: x, y, width, height.
pub(super) type Rect = (i32, i32, u32, u32);

/// What two rectangles share, if anything.
fn shared(a: Rect, b: Rect) -> Option<Rect> {
    let (left, top) = (a.0.max(b.0), a.1.max(b.1));
    let (right, bottom) = ((a.0 + a.2 as i32).min(b.0 + b.2 as i32), (a.1 + a.3 as i32).min(b.1 + b.3 as i32));
    (right > left && bottom > top).then(|| (left, top, (right - left) as u32, (bottom - top) as u32))
}

/// The room on the monitor the window is on (`monitor`, as winit has it):
/// what the desktop leaves of it, or all of it where the desktop does not
/// say. `None` when not even the monitor is known.
pub(super) fn work(monitor: Option<Rect>) -> Option<Rect> {
    match (monitor, desktop()) {
        (Some(monitor), Some(left)) => Some(shared(monitor, left).unwrap_or(monitor)),
        (monitor, left) => monitor.or(left),
    }
}

#[cfg(target_os = "linux")]
fn desktop() -> Option<Rect> {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt};
    // A Wayland session has no such thing to ask, though it may have an X
    // server for the programs that want one.
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        return None;
    }
    let (x, screen) = x11rb::connect(None).ok()?;
    let root = x.setup().roots.get(screen)?.root;
    let name = x.intern_atom(false, b"_NET_WORKAREA").ok()?.reply().ok()?.atom;
    let answer = x.get_property(false, root, name, AtomEnum::CARDINAL, 0, 4).ok()?.reply().ok()?;
    let area: Vec<u32> = answer.value32()?.collect();
    match area[..] {
        [x, y, width, height] if width > 0 && height > 0 => Some((x as i32, y as i32, width, height)),
        _ => None,
    }
}

#[cfg(windows)]
fn desktop() -> Option<Rect> {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::WindowsAndMessaging::{SystemParametersInfoW, SPI_GETWORKAREA};
    let mut area = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    let asked = unsafe { SystemParametersInfoW(SPI_GETWORKAREA, 0, (&mut area as *mut RECT).cast(), 0) };
    (asked != 0 && area.right > area.left && area.bottom > area.top).then(|| (area.left, area.top, (area.right - area.left) as u32, (area.bottom - area.top) as u32))
}

#[cfg(not(any(target_os = "linux", windows)))]
fn desktop() -> Option<Rect> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_room_is_what_the_monitor_and_the_desktop_share() {
        // A panel along the top, a dock on the left.
        assert_eq!(shared((0, 0, 1920, 1080), (72, 32, 1848, 1048)), Some((72, 32, 1848, 1048)));
        // The second monitor of two: the desktop's area spans both.
        assert_eq!(shared((1920, 0, 1920, 1080), (0, 32, 3840, 1048)), Some((1920, 32, 1920, 1048)));
        assert_eq!(shared((0, 0, 800, 600), (2000, 0, 800, 600)), None);
    }
}

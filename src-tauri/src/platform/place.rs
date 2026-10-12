//! Where a window is: which monitor, its scale and the room it has there
//! now, where a window of a size goes on it, and being told when any of
//! that changes. Heyo's windows are drawn by two toolkits (the palette by
//! Tauri's, the rest by Slint's), and each of them asked for itself,
//! differently, and kept what it was told: a laptop carried from one
//! monitor to another had its windows drawn and placed for the first.
//!
//! The interface:
//!
//! - A window asks for the display as it is now ([`Display`], which its
//!   toolkit's adapter fills in at that moment and nobody keeps), picks
//!   its monitor from it ([`Display::monitor`], [`Display::at`]), and asks
//!   the monitor where it goes ([`Monitor::middle`],
//!   [`Monitor::middle_of_room`], [`Monitor::bottom_centre`]). All of it
//!   is arithmetic on values, in the screen's device pixels, which is
//!   what the tests are of: a stand-in display that changes.
//! - What is to be done when the display changes is said once
//!   ([`when_changed`]); the desktop is asked to say when it does
//!   ([`watch`]), and what it says piece by piece is heard as one change
//!   ([`changed`]).
//!
//! Nothing here is remembered between two askings, and no window is to
//! remember a monitor, a scale or a room either.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

/// A rectangle of the screen, in its device pixels: x, y, width, height.
pub(crate) type Rect = (i32, i32, u32, u32);

/// What two rectangles share, if anything.
fn shared(a: Rect, b: Rect) -> Option<Rect> {
    let (left, top) = (a.0.max(b.0), a.1.max(b.1));
    let (right, bottom) = ((a.0 + a.2 as i32).min(b.0 + b.2 as i32), (a.1 + a.3 as i32).min(b.1 + b.3 as i32));
    (right > left && bottom > top).then(|| (left, top, (right - left) as u32, (bottom - top) as u32))
}

/// A monitor as it is now.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Monitor {
    /// All of it.
    pub whole: Rect,
    /// What the desktop leaves of it for a window: without its panels and
    /// docks. All of it where the desktop does not say.
    pub room: Rect,
    /// How many device pixels a logical one is on it.
    pub scale: f64,
    pub primary: bool,
}

impl Monitor {
    /// A monitor as a toolkit says it, and what the desktop keeps free of
    /// the screen (`kept_free`), which may span several monitors.
    pub(crate) fn new(whole: Rect, scale: f64, primary: bool, free: Option<Rect>) -> Self {
        Self { whole, room: free.and_then(|free| shared(whole, free)).unwrap_or(whole), scale: if scale > 0.0 { scale } else { 1.0 }, primary }
    }

    /// Where a window of a size in logical pixels goes to be in the middle
    /// of the monitor.
    pub(crate) fn middle(&self, size: (f64, f64)) -> (i32, i32) {
        let (width, height) = ((size.0 * self.scale).round() as i32, (size.1 * self.scale).round() as i32);
        (self.whole.0 + (self.whole.2 as i32 - width) / 2, self.whole.1 + (self.whole.3 as i32 - height) / 2)
    }

    /// Where a window of a size in device pixels goes to be in the middle
    /// of the room the monitor has for it.
    pub(crate) fn middle_of_room(&self, size: (f64, f64)) -> (i32, i32) {
        (self.room.0 + ((self.room.2 as f64 - size.0) / 2.0) as i32, self.room.1 + ((self.room.3 as f64 - size.1) / 2.0) as i32)
    }

    /// The room in logical pixels: a window is made no larger.
    pub(crate) fn room_size(&self) -> (f64, f64) {
        (self.room.2 as f64 / self.scale, self.room.3 as f64 / self.scale)
    }

    /// Where a window of a size in device pixels goes to be at the bottom
    /// centre of the monitor, a tenth of the monitor's height above its
    /// bottom.
    pub(crate) fn bottom_centre(&self, size: (u32, u32)) -> (i32, i32) {
        let (x, y, width, height) = (self.whole.0, self.whole.1, self.whole.2 as i32, self.whole.3 as i32);
        (x + (width - size.0 as i32) / 2, y + height - size.1 as i32 - height / 10)
    }
}

/// Which monitor a window means.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Which {
    /// The one it is on, or comes up on, as its toolkit knows; else the
    /// primary one.
    Its,
    /// The primary one, wherever the window was; else the one it is on.
    Primary,
}

/// The display as it is at the moment of asking.
#[derive(Clone, Debug, PartialEq, Default)]
pub(crate) struct Display {
    pub monitors: Vec<Monitor>,
    /// The monitor the window that asks is on, where its toolkit knows.
    pub its: Option<usize>,
}

impl Display {
    /// The monitor a window means: in the order `which` says, and the
    /// first there is when neither is known. None when there is none.
    pub(crate) fn monitor(&self, which: Which) -> Option<Monitor> {
        let its = self.its.and_then(|at| self.monitors.get(at));
        let primary = self.monitors.iter().find(|monitor| monitor.primary);
        match which {
            Which::Its => its.or(primary),
            Which::Primary => primary.or(its),
        }
        .or(self.monitors.first())
        .copied()
    }

    /// The monitor a point of the screen is on, where the pointer is say;
    /// else the primary one.
    pub(crate) fn at(&self, point: (i32, i32)) -> Option<Monitor> {
        let on = |monitor: &&Monitor| {
            let (x, y, width, height) = monitor.whole;
            point.0 >= x && point.0 < x + width as i32 && point.1 >= y && point.1 < y + height as i32
        };
        self.monitors.iter().find(on).copied().or_else(|| self.monitor(Which::Primary))
    }
}

/// What the desktop keeps free of the screen for windows: the screen
/// without its panels and docks. On X11 the window manager says
/// (`_NET_WORKAREA`), on Windows the system does. Elsewhere (Wayland,
/// which tells no client) nothing is known, and a monitor's room is the
/// monitor.
#[cfg(target_os = "linux")]
pub(crate) fn kept_free() -> Option<Rect> {
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
pub(crate) fn kept_free() -> Option<Rect> {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::WindowsAndMessaging::{SystemParametersInfoW, SPI_GETWORKAREA};
    let mut area = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    let asked = unsafe { SystemParametersInfoW(SPI_GETWORKAREA, 0, (&mut area as *mut RECT).cast(), 0) };
    (asked != 0 && area.right > area.left && area.bottom > area.top).then(|| (area.left, area.top, (area.right - area.left) as u32, (area.bottom - area.top) as u32))
}

#[cfg(not(any(target_os = "linux", windows)))]
pub(crate) fn kept_free() -> Option<Rect> {
    None
}

// --- When it changes -----------------------------------------------------------------

/// How long the desktop is given to finish saying how the display
/// changed: it says so piece by piece (the monitors, the scale, the
/// pointer), each a moment after the other.
const SETTLED: Duration = Duration::from_millis(400);

/// What was said of the display, counted: what is to be done about a
/// change is done by whoever said the last of it.
#[derive(Default)]
struct Said(AtomicU64);

impl Said {
    /// Something was said: the ticket of having said it.
    fn say(&self) -> u64 {
        self.0.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Whether nothing has been said since.
    fn last(&self, ticket: u64) -> bool {
        self.0.load(Ordering::SeqCst) == ticket
    }
}

type Heard = Box<dyn Fn() + Send + Sync>;

fn hearers() -> &'static Mutex<Vec<Heard>> {
    static HEARERS: OnceLock<Mutex<Vec<Heard>>> = OnceLock::new();
    HEARERS.get_or_init(Mutex::default)
}

/// Has `heard` called, on a thread of its own, each time the display has
/// changed: once for a change, when the desktop has finished saying it.
pub(crate) fn when_changed(heard: impl Fn() + Send + Sync + 'static) {
    hearers().lock().unwrap().push(Box::new(heard));
}

/// Something of the display changed. From any thread; it does not wait.
pub(crate) fn changed() {
    static SAID: Said = Said(AtomicU64::new(0));
    let ticket = SAID.say();
    let _ = std::thread::Builder::new().name("heyo-display".into()).spawn(move || {
        std::thread::sleep(SETTLED);
        if SAID.last(ticket) {
            for heard in hearers().lock().unwrap().iter() {
                heard();
            }
        }
    });
}

/// Has the desktop say when the display changes: a monitor connected or
/// taken away, another resolution, another scale, another size or theme
/// of the pointer. (Call on the main thread.)
///
/// On Linux the toolkit of the palette is asked, which hears it from the
/// session on X11 and on Wayland alike. Elsewhere the palette's window is
/// told of its scale by the toolkit itself (`palette::on_window_event`),
/// and that is passed on to `changed`.
#[cfg(target_os = "linux")]
pub(crate) fn watch() {
    use gtk::prelude::*;
    if let Some(screen) = gtk::gdk::Screen::default() {
        // Monitors, their sizes, and the scale windows are drawn at.
        screen.connect_monitors_changed(|_| changed());
        screen.connect_size_changed(|_| changed());
    }
    if let Some(settings) = gtk::Settings::default() {
        // What text and the pointer are sized by.
        for name in ["gtk-xft-dpi", "gtk-cursor-theme-size", "gtk-cursor-theme-name"] {
            settings.connect_notify_local(Some(name), |_, _| changed());
        }
    }
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn watch() {}

#[cfg(test)]
mod tests {
    use super::*;

    /// A laptop's own screen at scale 2, and a monitor to its right at
    /// scale 1 with a panel along its top.
    fn two() -> Display {
        let free = Some((0, 0, 3840 + 2560, 2160));
        Display { monitors: vec![Monitor::new((0, 0, 3840, 2160), 2.0, true, free), Monitor::new((3840, 0, 2560, 1440), 1.0, false, Some((3840, 32, 2560, 1408)))], its: None }
    }

    #[test]
    fn the_room_is_what_the_monitor_and_the_desktop_share() {
        // A panel along the top, a dock on the left.
        assert_eq!(Monitor::new((0, 0, 1920, 1080), 1.0, true, Some((72, 32, 1848, 1048))).room, (72, 32, 1848, 1048));
        // The second monitor of two: the desktop's area spans both.
        assert_eq!(Monitor::new((1920, 0, 1920, 1080), 1.0, false, Some((0, 32, 3840, 1048))).room, (1920, 32, 1920, 1048));
        // What the desktop says is of elsewhere, or it says nothing: all of it.
        assert_eq!(Monitor::new((0, 0, 800, 600), 1.0, true, Some((2000, 0, 800, 600))).room, (0, 0, 800, 600));
        assert_eq!(Monitor::new((0, 0, 800, 600), 1.0, true, None).room, (0, 0, 800, 600));
    }

    #[test]
    fn a_window_means_the_monitor_it_is_on_or_the_primary_one() {
        let mut display = two();
        // Never anywhere yet: the primary one, whichever is asked for.
        assert_eq!(display.monitor(Which::Its).map(|m| m.whole.0), Some(0));
        assert_eq!(display.monitor(Which::Primary).map(|m| m.whole.0), Some(0));
        // On the second: its own, unless it is the primary that is meant.
        display.its = Some(1);
        assert_eq!(display.monitor(Which::Its).map(|m| m.whole.0), Some(3840));
        assert_eq!(display.monitor(Which::Primary).map(|m| m.whole.0), Some(0));
        // None is primary: the one it is on, else the first.
        display.monitors[0].primary = false;
        assert_eq!(display.monitor(Which::Primary).map(|m| m.whole.0), Some(3840));
        display.its = None;
        assert_eq!(display.monitor(Which::Its).map(|m| m.whole.0), Some(0));
        assert_eq!(Display::default().monitor(Which::Its), None);
    }

    #[test]
    fn a_point_is_on_the_monitor_that_holds_it() {
        let display = two();
        assert_eq!(display.at((100, 100)).map(|m| m.scale), Some(2.0));
        assert_eq!(display.at((3840, 0)).map(|m| m.scale), Some(1.0));
        // Off every monitor: the primary one.
        assert_eq!(display.at((-5, 9000)).map(|m| m.scale), Some(2.0));
    }

    #[test]
    fn the_middle_of_a_monitor_is_reckoned_in_its_own_pixels() {
        // The palette as declared, on a 4K monitor at a scale of 2 and of 1.
        assert_eq!(Monitor::new((0, 0, 3840, 2160), 2.0, true, None).middle((814.0, 534.0)), (1106, 546));
        assert_eq!(Monitor::new((0, 0, 3840, 2160), 1.0, true, None).middle((814.0, 534.0)), (1513, 813));
        // A second monitor to the right of the first, at a scale and a half.
        assert_eq!(Monitor::new((3840, 0, 2560, 1440), 1.5, false, None).middle((814.0, 534.0)), (3840 + 669, 319));
    }

    #[test]
    fn a_window_goes_in_the_middle_of_the_room_or_at_the_bottom_centre() {
        // The settings with their margin at scale 2, beside a dock and under a panel.
        let monitor = Monitor::new((0, 0, 3840, 2160), 2.0, true, Some((132, 64, 3708, 2096)));
        assert_eq!(monitor.middle_of_room((1848.0, 1408.0)), (132 + 930, 64 + 344));
        assert_eq!(monitor.room_size(), (1854.0, 1048.0));
        // The pill, a tenth of the monitor's height above its bottom.
        assert_eq!(monitor.bottom_centre((720, 88)), (1560, 1856));
        assert_eq!(Monitor::new((0, 0, 3840, 2160), 1.0, true, None).bottom_centre((360, 44)), (1740, 1900));
    }

    #[test]
    fn what_was_asked_before_the_display_changed_is_not_what_is_answered_after() {
        // The laptop's screen alone, at scale 1; then a monitor of twice
        // the density takes its place, under another name, with a dock.
        let before = Display { monitors: vec![Monitor::new((0, 0, 1920, 1080), 1.0, true, None)], its: Some(0) };
        let after = Display { monitors: vec![Monitor::new((0, 0, 3840, 2160), 2.0, true, Some((132, 64, 3708, 2096)))], its: None };
        let (was, is) = (before.monitor(Which::Its).unwrap(), after.monitor(Which::Its).unwrap());
        assert_eq!((was.scale, was.middle((814.0, 534.0))), (1.0, (553, 273)));
        assert_eq!((is.scale, is.middle((814.0, 534.0))), (2.0, (1106, 546)));
        assert_eq!((was.room_size(), is.room_size()), ((1920.0, 1080.0), (1854.0, 1048.0)));
        assert_eq!((was.bottom_centre((360, 44)), is.bottom_centre((720, 88))), ((780, 928), (1560, 1856)));
        // And a monitor taken away while a window was on it: the window
        // means the one that is left.
        let mut display = two();
        display.its = Some(1);
        display.monitors.pop();
        assert_eq!(display.monitor(Which::Its).map(|m| m.whole), Some((0, 0, 3840, 2160)));
    }

    #[test]
    fn what_the_desktop_says_piece_by_piece_is_one_change() {
        let said = Said::default();
        let (monitors, scale, pointer) = (said.say(), said.say(), said.say());
        assert_eq!([said.last(monitors), said.last(scale), said.last(pointer)], [false, false, true]);
        // And the next change is another.
        let next = said.say();
        assert!(!said.last(pointer) && said.last(next));
    }
}

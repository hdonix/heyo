//! Other applications' windows: listing and switching, and moving a window
//! into a layout (halves, quarters, thirds, …).
//!
//! What a layout means and which window is "the previous one" are the same
//! everywhere; reaching the windows is the session's business (`x11`).

use std::sync::Mutex;

use serde::Serialize;

use super::desktop::desktop;

/// A window of another application, as the session identifies it.
pub type Window = u32;

/// The window that was active before the palette took focus, so commands
/// run from the palette act on the application the user came from.
#[derive(Default)]
pub struct LastWindow(Mutex<Option<Window>>);

impl LastWindow {
    /// Notes the window in front as the one the user comes from. With one
    /// of Heyo's own in front there is none to note, and the one noted
    /// before stays.
    pub fn remember(&self) {
        if let Some(active) = active_external() {
            *self.0.lock().unwrap() = Some(active);
        }
    }

    /// The window the user is working in: the one in front, or, with one of
    /// Heyo's own in front, the one they came from.
    pub fn working(&self) -> Option<Window> {
        active_external().or(*self.0.lock().unwrap())
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct WindowInfo {
    pub id: Window,
    pub title: String,
    /// The application's window class, e.g. "firefox".
    pub class: String,
    pub active: bool,
    /// Where its frame is, when the desktop says.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bounds: Option<Rect>,
}

/// A rectangle in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../shared/bindings/")]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Rect {
    #[cfg(any(target_os = "linux", test))]
    pub(super) fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }

    pub(super) fn intersect(&self, other: &Rect) -> Rect {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = (self.x + self.width).min(other.x + other.width);
        let bottom = (self.y + self.height).min(other.y + other.height);
        Rect { x, y, width: (right - x).max(0), height: (bottom - y).max(0) }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Layout {
    /// A fraction of the work area: x, y, width, height in twelfths.
    Fraction(i32, i32, i32, i32),
    Maximize,
    /// Keeps the size, centered on its monitor.
    Center,
    NextDisplay,
    /// Exactly there, wherever its monitor.
    Exact(Rect),
}

/// The layouts offered as commands: id suffix, title, layout.
pub const LAYOUTS: &[(&str, &str, Layout)] = &[
    ("left-half", "Left Half", Layout::Fraction(0, 0, 6, 12)),
    ("right-half", "Right Half", Layout::Fraction(6, 0, 6, 12)),
    ("top-half", "Top Half", Layout::Fraction(0, 0, 12, 6)),
    ("bottom-half", "Bottom Half", Layout::Fraction(0, 6, 12, 6)),
    ("top-left", "Top Left Quarter", Layout::Fraction(0, 0, 6, 6)),
    ("top-right", "Top Right Quarter", Layout::Fraction(6, 0, 6, 6)),
    ("bottom-left", "Bottom Left Quarter", Layout::Fraction(0, 6, 6, 6)),
    ("bottom-right", "Bottom Right Quarter", Layout::Fraction(6, 6, 6, 6)),
    ("first-third", "First Third", Layout::Fraction(0, 0, 4, 12)),
    ("center-third", "Center Third", Layout::Fraction(4, 0, 4, 12)),
    ("last-third", "Last Third", Layout::Fraction(8, 0, 4, 12)),
    ("first-two-thirds", "First Two Thirds", Layout::Fraction(0, 0, 8, 12)),
    ("last-two-thirds", "Last Two Thirds", Layout::Fraction(4, 0, 8, 12)),
    ("almost-maximize", "Almost Maximize", Layout::Fraction(1, 1, 10, 10)),
    ("maximize", "Maximize", Layout::Maximize),
    ("center", "Center", Layout::Center),
    ("next-display", "Move to Next Display", Layout::NextDisplay),
];

/// Where a layout puts a window, given the usable area of its monitor.
pub(super) fn target(layout: Layout, area: Rect, current: Rect) -> Rect {
    match layout {
        Layout::Fraction(x, y, width, height) => Rect {
            x: area.x + area.width * x / 12,
            y: area.y + area.height * y / 12,
            width: area.width * width / 12,
            height: area.height * height / 12,
        },
        Layout::Center => Rect {
            x: area.x + (area.width - current.width.min(area.width)) / 2,
            y: area.y + (area.height - current.height.min(area.height)) / 2,
            width: current.width.min(area.width),
            height: current.height.min(area.height),
        },
        Layout::Exact(rect) => rect,
        Layout::Maximize | Layout::NextDisplay => area,
    }
}

/// Where a window goes on another monitor: the same relative place and the
/// same share of the usable area (`from` to `to`), and wholly inside it.
#[cfg(any(windows, test))]
pub(super) fn carried(frame: Rect, from: Rect, to: Rect) -> Rect {
    let scale = |length: i32, of: i32, onto: i32| (length as i64 * onto as i64 / of.max(1) as i64) as i32;
    let width = scale(frame.width, from.width, to.width).clamp(1, to.width.max(1));
    let height = scale(frame.height, from.height, to.height).clamp(1, to.height.max(1));
    let x = to.x + scale(frame.x - from.x, from.width, to.width);
    let y = to.y + scale(frame.y - from.y, from.height, to.height);
    Rect { x: x.clamp(to.x, to.x + (to.width - width).max(0)), y: y.clamp(to.y, to.y + (to.height - height).max(0)), width, height }
}

/// The monitor after `current` of these, going left to right and then top
/// to bottom, and round again. `None` when there is no other.
#[cfg(any(windows, test))]
pub(super) fn next_display(monitors: &[Rect], current: usize) -> Option<usize> {
    let mut order: Vec<usize> = (0..monitors.len()).collect();
    order.sort_by_key(|i| (monitors[*i].x, monitors[*i].y));
    let at = order.iter().position(|i| *i == current)?;
    Some(order[(at + 1) % order.len()]).filter(|next| *next != current)
}

/// Which keys are held down right now, as a 256-bit map by keycode.
pub fn keys_down() -> Option<[u8; 32]> {
    desktop().keys_down()
}

/// The active window, unless it is one of Heyo's own.
pub fn active_external() -> Option<Window> {
    desktop().active_window()
}

/// The window class of the active window, e.g. "firefox" or "Gnome-terminal".
pub fn active_class() -> Option<String> {
    desktop().active_class()
}

/// Application windows, most recently used first; `current` (the window the
/// user came from) goes last, so the first one is the one to switch back to.
pub fn list(current: Option<Window>) -> Result<Vec<WindowInfo>, String> {
    desktop().windows(current)
}

/// The application windows on screen now, the topmost first.
pub fn shown() -> Result<Vec<WindowInfo>, String> {
    desktop().windows_shown()
}

/// The application window used most recently before `window`: the one to
/// go back to from it.
pub fn before(window: Window) -> Option<Window> {
    desktop().window_before(window)
}

/// The windows whose class is one of `classes` (lowercase), most recently
/// used first.
pub fn of_classes(classes: &[String]) -> Vec<Window> {
    desktop().windows_of(classes)
}

/// Brings a window to the front, switching workspace if needed, and makes
/// sure it happened.
pub fn activate(window: Window) -> Result<(), String> {
    desktop().activate(window)
}

pub fn close(window: Window) -> Result<(), String> {
    desktop().close(window)
}

/// Moves a window into a layout. `monitors` are the monitors' rectangles.
pub fn apply(window: Window, layout: Layout, monitors: &[Rect]) -> Result<(), String> {
    desktop().arrange(window, layout, monitors)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::desktop::fake::Fake;

    const AREA: Rect = Rect { x: 0, y: 32, width: 1920, height: 1048 };

    #[test]
    fn the_window_worked_in_is_the_one_in_front_or_the_one_the_palette_came_up_over() {
        let desktop = Fake::with_windows(&[(1, "firefox"), (2, "Code")]);
        let came_from = LastWindow::default();
        desktop.during(|| {
            // Nothing remembered yet: only what is in front.
            assert_eq!(came_from.working(), Some(1));
            *desktop.heyo_in_front.lock().unwrap() = true;
            assert_eq!(came_from.working(), None);
            // The palette comes up over Firefox.
            *desktop.heyo_in_front.lock().unwrap() = false;
            came_from.remember();
            *desktop.heyo_in_front.lock().unwrap() = true;
            assert_eq!(came_from.working(), Some(1));
            // Shown again while it is up, it has no other window to come from.
            came_from.remember();
            assert_eq!(came_from.working(), Some(1));
            // With the palette away, the window in front is the one, whichever was remembered.
            *desktop.heyo_in_front.lock().unwrap() = false;
            activate(2).unwrap();
            assert_eq!(came_from.working(), Some(2));
        });
    }

    #[test]
    fn fractions_tile_the_work_area() {
        let current = Rect { x: 10, y: 40, width: 800, height: 600 };
        let left = target(Layout::Fraction(0, 0, 6, 12), AREA, current);
        let right = target(Layout::Fraction(6, 0, 6, 12), AREA, current);
        assert_eq!(left, Rect { x: 0, y: 32, width: 960, height: 1048 });
        assert_eq!(right.x, left.x + left.width);
        assert_eq!(right.x + right.width, AREA.x + AREA.width);
    }

    #[test]
    fn center_keeps_the_size() {
        let current = Rect { x: 0, y: 0, width: 800, height: 600 };
        assert_eq!(target(Layout::Center, AREA, current), Rect { x: 560, y: 256, width: 800, height: 600 });
    }

    #[test]
    fn a_window_keeps_its_place_and_share_on_another_monitor() {
        // A smaller monitor to the left, at negative coordinates.
        let small = Rect { x: -1280, y: 0, width: 1280, height: 984 };
        let right_half = target(Layout::Fraction(6, 0, 6, 12), AREA, AREA);
        assert_eq!(carried(right_half, AREA, small), Rect { x: -640, y: 0, width: 640, height: 984 });
        // And back again.
        assert_eq!(carried(Rect { x: -640, y: 0, width: 640, height: 984 }, small, AREA), right_half);
        // A window hanging off its monitor ends up wholly on the other.
        let hanging = Rect { x: 1600, y: 900, width: 800, height: 600 };
        let moved = carried(hanging, AREA, small);
        assert!(moved.x >= small.x && moved.x + moved.width <= small.x + small.width);
        assert!(moved.y >= small.y && moved.y + moved.height <= small.y + small.height);
        // One larger than the monitor is cut to it.
        let huge = Rect { x: -100, y: -100, width: 5000, height: 5000 };
        assert_eq!(carried(huge, AREA, small), small);
    }

    #[test]
    fn the_next_display_goes_left_to_right_and_round() {
        let (left, middle, right) = (Rect { x: -1280, y: 0, width: 1280, height: 1024 }, AREA, Rect { x: 1920, y: 0, width: 1920, height: 1080 });
        // However the system lists them.
        let monitors = [middle, right, left];
        assert_eq!(next_display(&monitors, 2), Some(0));
        assert_eq!(next_display(&monitors, 0), Some(1));
        assert_eq!(next_display(&monitors, 1), Some(2));
        assert_eq!(next_display(&[middle], 0), None);
        assert_eq!(next_display(&monitors, 7), None);
    }
}

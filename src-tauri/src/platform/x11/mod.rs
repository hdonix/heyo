//! The X11 session: EWMH for windows, and (as they move here) XFixes for the
//! clipboard, XInput for the hold-to-talk key, and direct screen reads.

pub mod capture;
pub mod clipboard;
#[cfg(test)]
mod field;
pub mod hold_key;
pub mod input_method;
mod offer;
pub mod overlay;
pub mod typed;
pub mod windows;

use super::capture::{Area, Shot};
use super::clipboard_watch::Change;
use super::desktop::{Desktop, Hotkeys, Supported};
use super::keys::Hold;
use super::windows::{Layout, Rect, Window, WindowInfo};
use super::Support;

/// The X11 session as a `Desktop`: everything Heyo asks, in full.
pub struct X11;

impl Desktop for X11 {
    fn supported(&self) -> Supported {
        Supported {
            windows: Support::Full,
            paste: Support::Full,
            selection: Support::Full,
            clipboard_watch: Support::Full,
            hold_key: Support::Full,
            capture: Support::Full,
            overlay: Support::Full,
            typing: Support::Full,
        }
    }

    fn keys_down(&self) -> Option<[u8; 32]> {
        windows::keys_down()
    }

    fn active_window(&self) -> Option<Window> {
        windows::active_external()
    }

    fn active_class(&self) -> Option<String> {
        windows::active_class()
    }

    fn windows(&self, current: Option<Window>) -> Result<Vec<WindowInfo>, String> {
        windows::list(current)
    }

    fn windows_shown(&self) -> Result<Vec<WindowInfo>, String> {
        windows::shown()
    }

    fn window_before(&self, window: Window) -> Option<Window> {
        windows::before(window)
    }

    fn windows_of(&self, classes: &[String]) -> Vec<Window> {
        windows::of_classes(classes)
    }

    fn activate(&self, window: Window) -> Result<(), String> {
        windows::activate(window)
    }

    fn close(&self, window: Window) -> Result<(), String> {
        windows::close(window)
    }

    fn arrange(&self, window: Window, layout: Layout, monitors: &[Rect]) -> Result<(), String> {
        windows::apply(window, layout, monitors)
    }

    fn paste(&self, terminal: bool) -> Result<(), String> {
        super::input::paste_with_tools(terminal)
    }

    /// The PRIMARY selection, which holds what is selected without its
    /// having been copied.
    fn selected_text(&self) -> String {
        super::input::primary_selection()
    }

    fn watch_clipboard(&self, on_change: &mut dyn FnMut(Change)) -> Result<(), String> {
        clipboard::watch(on_change).map_err(|e| e.to_string())
    }

    fn grab(&self, area: Area) -> Result<Shot, String> {
        capture::grab(area).or_else(|e| {
            eprintln!("heyo: capture failed ({e}); trying gnome-screenshot");
            super::capture::grab_with_tool(area)
        })
    }

    fn hold_keys(&self) -> Vec<&'static str> {
        super::keys::KEYS.iter().map(|(value, _)| *value).collect()
    }

    fn watch_hold(&self, key: &dyn Fn() -> String, on: &dyn Fn(Hold)) -> Result<(), String> {
        hold_key::watch(key, on).map_err(|e| e.to_string())
    }

    fn place_overlay(&self, window: &gtk::ApplicationWindow, x: i32, y: i32, width: i32, height: i32) {
        overlay::place(window, x, y, width, height);
    }

    fn overlay_backdrop(&self, capture: gtk::cairo::ImageSurface, x: i32, y: i32, width: i32, height: i32) -> bool {
        overlay::backdrop(capture, x, y, width, height)
    }

    fn release_overlay(&self, window: &gtk::ApplicationWindow) {
        overlay::release(window);
    }

    fn close_overlay(&self) {
        overlay::close();
    }

    fn hotkeys(&self) -> Option<&dyn Hotkeys> {
        super::hotkeys::gsettings()
    }

    fn typing(&self) -> Option<&dyn super::typed::Typing> {
        Some(&typed::Keyboard)
    }
}

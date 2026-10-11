//! The clipboard: noticing when something is copied, and reading and
//! writing its text.
//!
//! The session's adapter does it (`Desktop`), since the toolkit's way needs
//! Heyo to be in front on Wayland. The toolkit's way is what an adapter
//! without one of its own is left with, and what GNOME falls back to.

use std::sync::OnceLock;

use tauri::AppHandle;
use tauri_plugin_clipboard_manager::ClipboardExt;

use super::desktop::desktop;

/// A new clipboard owner and what it offers.
pub struct Change {
    pub has_text: bool,
    pub has_image: bool,
    /// Marked by its owner as a secret (password managers do this).
    pub secret: bool,
    /// Window class of the application in front, e.g. "firefox".
    pub source: Option<String>,
}

/// The toolkit's clipboard lives in the application.
static TOOLKIT: OnceLock<AppHandle> = OnceLock::new();

/// Tells this module the application once it exists. Until then (and in
/// tests, which bring a clipboard of their own) the toolkit has none.
pub(crate) fn attach(app: &AppHandle) {
    let _ = TOOLKIT.set(app.clone());
}

fn toolkit() -> Result<&'static AppHandle, String> {
    TOOLKIT.get().ok_or_else(|| "the clipboard is not there yet".to_owned())
}

pub(super) fn toolkit_text() -> Option<String> {
    toolkit().ok()?.clipboard().read_text().ok()
}

pub(super) fn toolkit_set_text(text: &str) -> Result<(), String> {
    toolkit()?.clipboard().write_text(text).map_err(|e| e.to_string())
}

pub(super) fn toolkit_set_html(html: &str, text: &str) -> Result<(), String> {
    toolkit()?.clipboard().write_html(html, Some(text)).map_err(|e| e.to_string())
}

/// The clipboard's image as RGBA, where the session lets it be read without
/// the asking application being in front (on Wayland an ordinary read fails
/// then). `None` means: read it the ordinary way.
pub fn image() -> Option<image::RgbaImage> {
    desktop().clipboard_image()
}

/// The text on the clipboard.
pub fn read_text() -> Option<String> {
    desktop().clipboard_text()
}

/// Puts text on the clipboard.
pub fn write_text(text: &str) -> Result<(), String> {
    desktop().set_clipboard_text(text)
}

/// Puts rich text on the clipboard, with `text` for the applications that
/// take none.
pub fn write_html(html: &str, text: &str) -> Result<(), String> {
    desktop().set_clipboard_html(html, text)
}

/// Calls `on_change` for every clipboard change, for as long as the session
/// lasts. Returns an error at once where changes cannot be observed; the
/// caller may then fall back to polling the contents.
pub fn watch(mut on_change: impl FnMut(Change)) -> Result<(), String> {
    desktop().watch_clipboard(&mut on_change)
}

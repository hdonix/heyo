//! The clipboard: noticing when something is copied, and reading and
//! writing its text where the toolkit's way needs Heyo to be in front.

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

/// The clipboard's text, where the session lets it be read without the
/// asking application being in front (on Wayland an ordinary read fails
/// then). `None` means: read it the ordinary way.
pub fn text() -> Option<String> {
    desktop().clipboard_text()
}

/// The clipboard's image as RGBA; see `text`.
pub fn image() -> Option<image::RgbaImage> {
    desktop().clipboard_image()
}

/// Puts text on the clipboard where the ordinary way needs the focus.
/// False means: set it the ordinary way.
pub fn set_text(text: &str) -> bool {
    desktop().set_clipboard_text(text)
}

/// The text on the clipboard.
pub fn read_text(app: &AppHandle) -> Option<String> {
    text().or_else(|| app.clipboard().read_text().ok())
}

/// Puts text on the clipboard.
pub fn write_text(app: &AppHandle, text: &str) -> Result<(), String> {
    if set_text(text) {
        return Ok(());
    }
    app.clipboard().write_text(text).map_err(|e| e.to_string())
}

/// Calls `on_change` for every clipboard change, for as long as the session
/// lasts. Returns an error at once where changes cannot be observed; the
/// caller may then fall back to polling the contents.
pub fn watch(mut on_change: impl FnMut(Change)) -> Result<(), String> {
    desktop().watch_clipboard(&mut on_change)
}

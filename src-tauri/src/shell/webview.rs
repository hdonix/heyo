//! Heyo's windows as web views: the windows `tauri.conf.json` describes,
//! and those made as they are needed. `src/main.tsx` picks each one's view.

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::commands::settings::SETTINGS_WINDOW;

/// Makes the windows that live as long as Heyo does (the palette, the
/// dictation pill, the screenshot overlay), hidden. They are described in
/// `tauri.conf.json` but not made by Tauri itself (`"create": false`), so
/// that the Flutter interface starts no web view at all.
pub(crate) fn start(app: &AppHandle) -> Result<(), String> {
    for config in &app.config().app.windows {
        if app.get_webview_window(&config.label).is_none() {
            WebviewWindowBuilder::from_config(app, config).and_then(|builder| builder.build()).map_err(|e| format!("the window {}: {e}", config.label))?;
        }
    }
    // The dictation window must never take keyboard focus from the
    // application being dictated into.
    #[cfg(target_os = "linux")]
    if let Some(window) = app.get_webview_window(crate::dictation::WINDOW) {
        use gtk::prelude::GtkWindowExt;
        if let Ok(gtk_window) = window.gtk_window() {
            gtk_window.set_accept_focus(false);
            gtk_window.set_focus_on_map(false);
        }
    }
    Ok(())
}

pub(crate) fn open_settings(app: &AppHandle) -> Result<(), String> {
    let window = match app.get_webview_window(SETTINGS_WINDOW) {
        Some(window) => window,
        None => WebviewWindowBuilder::new(app, SETTINGS_WINDOW, WebviewUrl::default())
            .title("Heyo Settings")
            // Drawn by Heyo itself, title bar and all: 860 by 640, in a
            // window that much larger for its shadow (the margin of
            // styles/abstracts/_scales.scss on every side).
            .inner_size(860.0 + 64.0, 640.0 + 64.0)
            .decorations(false)
            .transparent(true)
            .resizable(false)
            .center()
            .initialization_script("window.__HEYO_VIEW__ = 'settings';")
            .build()
            .map_err(|e| e.to_string())?,
    };
    let _ = window.show();
    crate::platform::focus::focus(&window);
    Ok(())
}

pub(crate) fn open_pin(app: &AppHandle, n: u32, path: &str, x: f64, y: f64, width: f64, height: f64) -> Result<(), String> {
    let script = format!("window.__HEYO_VIEW__ = 'pin'; window.__HEYO_PIN__ = {};", serde_json::to_string(path).map_err(|e| e.to_string())?);
    WebviewWindowBuilder::new(app, format!("pin-{n}"), WebviewUrl::default())
        .title("Heyo Pinned Screenshot")
        .initialization_script(&script)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .shadow(true)
        .inner_size(width, height)
        .position(x, y)
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
}

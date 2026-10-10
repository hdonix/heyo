//! The interface: Heyo's windows, and what draws them.
//!
//! There are two. The web view (`src/`), in windows of Tauri's, is the one
//! Heyo has always had and the default. The other is drawn by Flutter
//! (`ui/`), a `heyo-ui` process for each window, chosen when Heyo starts
//! with `--ui=flutter` or `HEYO_UI=flutter`: where the web view can only
//! draw on the processor (NVIDIA's driver on Linux), Flutter draws with the
//! graphics card. Everything that is not a window is the same program
//! either way.
//!
//! The rest of the host does not know which is in use. It asks for a window
//! by its label (`shell::window`, `main_window`) and shows, hides, focuses
//! or tells it things; it opens the settings or a pinned capture through
//! the functions here. Both interfaces invoke the same commands (through
//! Tauri, or `dispatch`) and hear the same events: one emitted to every
//! window (`app.emit`) reaches every process, one for a single window goes
//! through that window (`Window::emit`) and reaches it alone.

pub(crate) mod bridge;
pub(crate) mod dispatch;
#[cfg(test)]
mod fixtures;
pub(crate) mod flutter;
pub(crate) mod webview;

use std::sync::OnceLock;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Listener, Manager, WebviewWindow};

/// What draws the windows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Ui {
    Webview,
    Flutter,
}

/// Which interface was asked for: `--ui=…` on the command line, else
/// `HEYO_UI`, else the web view. An error names what was asked for and is
/// not an interface.
pub(crate) fn asked_for(args: &[String], env: Option<&str>) -> Result<Ui, String> {
    let asked = args.iter().find_map(|arg| arg.strip_prefix("--ui=")).or(env).unwrap_or("webview");
    match asked.trim().to_ascii_lowercase().as_str() {
        "" | "webview" | "web" => Ok(Ui::Webview),
        "flutter" => Ok(Ui::Flutter),
        other => Err(format!("{other} is not an interface of Heyo's (webview, flutter)")),
    }
}

/// The interface in use, and the processes of the Flutter one.
pub(crate) struct Shell {
    asked: Ui,
    /// Decided when the interface starts: what was asked for, if it can run.
    ui: OnceLock<Ui>,
    pub(crate) flutter: flutter::Processes,
}

impl Shell {
    /// Reads the choice from how Heyo was started.
    pub(crate) fn chosen() -> Shell {
        let args: Vec<String> = std::env::args().collect();
        let asked = asked_for(&args, std::env::var("HEYO_UI").ok().as_deref()).unwrap_or_else(|why| {
            eprintln!("heyo: {why}; using the web view");
            Ui::Webview
        });
        Shell { asked, ui: OnceLock::new(), flutter: Default::default() }
    }
}

fn ui(app: &AppHandle) -> Option<Ui> {
    app.state::<Shell>().ui.get().copied()
}

/// Whether the interface is up: there is a palette to show.
pub(crate) fn ready(app: &AppHandle) -> bool {
    ui(app).is_some()
}

/// Brings the interface up, with its windows hidden. Called once, as Heyo
/// starts.
pub(crate) fn start(app: &AppHandle) -> Result<(), String> {
    let shell = app.state::<Shell>();
    let mut chosen = shell.asked;
    if chosen == Ui::Flutter {
        // The program is there and came with this Heyo, and the palette starts.
        if let Err(why) = flutter::prepare(app).and_then(|()| flutter::start_kept(app)) {
            // Heyo with the other interface is better than no Heyo.
            eprintln!("heyo: {why} Using the web view.");
            crate::platform::common::notify("Heyo is using its web view interface", &why);
            chosen = Ui::Webview;
        }
    }
    match chosen {
        Ui::Webview => webview::start(app)?,
        Ui::Flutter => {
            // What is emitted to every window, for every process. (What is
            // meant for one window goes through `Window::emit`.)
            for name in crate::wire::event::ALL {
                let listener = app.clone();
                app.listen_any(*name, move |event| flutter::broadcast(&listener, name, event.payload()));
            }
        }
    }
    eprintln!("heyo: interface: {}", if chosen == Ui::Flutter { "flutter" } else { "webview" });
    let _ = shell.ui.set(chosen);
    Ok(())
}

/// One of Heyo's windows, whichever interface draws it.
#[derive(Clone)]
pub(crate) enum Window {
    Web(Box<WebviewWindow>),
    Flutter(flutter::Handle),
}

/// The window of that label, if there is one.
pub(crate) fn window(app: &AppHandle, label: &str) -> Option<Window> {
    match ui(app)? {
        Ui::Webview => app.get_webview_window(label).map(|window| Window::Web(Box::new(window))),
        Ui::Flutter => flutter::Handle::existing(app, label).map(Window::Flutter),
    }
}

impl Window {
    pub(crate) fn label(&self) -> &str {
        match self {
            Window::Web(window) => window.label(),
            Window::Flutter(handle) => handle.label(),
        }
    }

    pub(crate) fn show(&self) -> Result<(), String> {
        match self {
            Window::Web(window) => window.show().map_err(|e| e.to_string()),
            Window::Flutter(handle) => {
                handle.show();
                Ok(())
            }
        }
    }

    pub(crate) fn hide(&self) -> Result<(), String> {
        match self {
            Window::Web(window) => window.hide().map_err(|e| e.to_string()),
            Window::Flutter(handle) => {
                handle.hide();
                Ok(())
            }
        }
    }

    pub(crate) fn is_visible(&self) -> Result<bool, String> {
        match self {
            Window::Web(window) => window.is_visible().map_err(|e| e.to_string()),
            Window::Flutter(handle) => Ok(handle.is_visible()),
        }
    }

    /// Centres the window on its monitor (the web view) or on the one the
    /// pointer is on (Flutter).
    pub(crate) fn center(&self) -> Result<(), String> {
        match self {
            Window::Web(window) => window.center().map_err(|e| e.to_string()),
            Window::Flutter(handle) => {
                handle.center();
                Ok(())
            }
        }
    }

    /// Gives the window the keyboard (`platform::focus`).
    pub(crate) fn focus(&self) {
        match self {
            Window::Web(window) => crate::platform::focus::focus(window),
            Window::Flutter(handle) => handle.focus(),
        }
    }

    /// Moves and sizes the window, in physical pixels.
    pub(crate) fn set_frame(&self, x: i32, y: i32, width: i32, height: i32) {
        match self {
            Window::Web(window) => {
                let _ = window.set_size(tauri::PhysicalSize::new(width, height));
                let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
            }
            Window::Flutter(handle) => handle.set_frame(x.into(), y.into(), width.into(), height.into(), false),
        }
    }

    #[allow(dead_code)] // Not asked of a window yet, but part of what one can be told.
    pub(crate) fn set_title(&self, title: &str) {
        match self {
            Window::Web(window) => {
                let _ = window.set_title(title);
            }
            Window::Flutter(handle) => handle.set_title(title),
        }
    }

    /// Tells the window something. The web view's windows all hear what
    /// any of them is told, and listen only for what is theirs; a Flutter
    /// window is told alone.
    pub(crate) fn emit<S: Serialize + Clone>(&self, event: &str, payload: S) -> Result<(), String> {
        match self {
            Window::Web(window) => window.emit(event, payload).map_err(|e| e.to_string()),
            Window::Flutter(handle) => handle.emit(event, &payload),
        }
    }
}

/// The window that invoked a command, for a command that asks whose it is.
impl<'de> tauri::ipc::CommandArg<'de, tauri::Wry> for Window {
    fn from_command(command: tauri::ipc::CommandItem<'de, tauri::Wry>) -> Result<Self, tauri::ipc::InvokeError> {
        let webview = command.message.webview();
        let window = webview.app_handle().get_webview_window(webview.label());
        window.map(|window| Window::Web(Box::new(window))).ok_or_else(|| tauri::ipc::InvokeError::from("the window that asked is gone"))
    }
}

/// Whether one of Heyo's own windows has the keyboard.
pub(crate) fn any_focused(app: &AppHandle) -> bool {
    match ui(app) {
        Some(Ui::Flutter) => flutter::any_focused(app),
        _ => app.webview_windows().values().any(|window| window.is_focused().unwrap_or(false)),
    }
}

/// Opens the settings window, or brings it back if it is already open.
pub(crate) fn open_settings(app: &AppHandle) -> Result<(), String> {
    match ui(app) {
        Some(Ui::Flutter) => flutter::open_settings(app),
        _ => webview::open_settings(app),
    }
}

/// Keeps a capture on screen where it was taken: `path` is its picture,
/// the rectangle in logical pixels.
pub(crate) fn open_pin(app: &AppHandle, n: u32, path: &str, x: f64, y: f64, width: f64, height: f64) -> Result<(), String> {
    match ui(app) {
        Some(Ui::Flutter) => flutter::open_pin(app, n, path, x, y, width, height),
        _ => webview::open_pin(app, n, path, x, y, width, height),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|arg| arg.to_string()).collect()
    }

    #[test]
    fn the_web_view_unless_flutter_is_asked_for() {
        assert_eq!(asked_for(&args(&["heyo", "--hidden"]), None), Ok(Ui::Webview));
        assert_eq!(asked_for(&args(&["heyo"]), Some("flutter")), Ok(Ui::Flutter));
        assert_eq!(asked_for(&args(&["heyo", "--ui=flutter"]), None), Ok(Ui::Flutter));
        assert_eq!(asked_for(&args(&["heyo"]), Some(" Flutter ")), Ok(Ui::Flutter));
        assert_eq!(asked_for(&args(&["heyo"]), Some("")), Ok(Ui::Webview));
        // The command line is the nearer of the two.
        assert_eq!(asked_for(&args(&["heyo", "--ui=webview"]), Some("flutter")), Ok(Ui::Webview));
        assert_eq!(asked_for(&args(&["heyo", "--ui=flutter"]), Some("webview")), Ok(Ui::Flutter));
        assert!(asked_for(&args(&["heyo", "--ui=qt"]), None).unwrap_err().contains("qt is not an interface"));
    }

    /// An event for one window that is emitted by Tauri's `emit_to` would
    /// reach every Flutter window, which hear whatever Tauri emits: such an
    /// event goes through the window (`Window::emit`).
    #[test]
    fn an_event_for_one_window_goes_through_that_window() {
        fn sources(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            for entry in std::fs::read_dir(dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    sources(&path, out);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    out.push(path);
                }
            }
        }
        let mut files = Vec::new();
        sources(std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src")), &mut files);
        assert!(files.len() > 40);
        // Spelt in two halves, so that this file passes its own test.
        let (to, filtered) = (concat!(".emit", "_to("), concat!(".emit", "_filter("));
        for file in files {
            let text = std::fs::read_to_string(&file).unwrap();
            assert!(!text.contains(to) && !text.contains(filtered), "{}: tell one window through shell::window(…).emit(…)", file.display());
        }
    }
}

//! The palette's window, and all that is done with it.
//!
//! The rest of the host says here what it means (the palette is opened, is
//! done with, makes way for a capture, is needed for a question) and never
//! has the window in hand: how each is done, and in what order, is in this
//! one place. There is no way to try any of it without the window, so
//! none of it is tested here; what can be decided without one is decided
//! elsewhere (`cli::at_start`, `windows::LastWindow`, and where the middle
//! of a monitor is, `platform::place`).
//!
//! The palette comes up in two ways. [`show`] is the user opening it: in
//! the middle of the screen, its view told to start over, the window they
//! came from remembered. [`bring_back`] returns it as it was left, with
//! the command that is in it.

use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, WebviewWindow, Window, WindowEvent};

use crate::platform::place::{Display, Monitor, Which};
use crate::*;

/// The label of the palette's window.
pub(crate) const WINDOW: &str = "main";

fn window(app: &AppHandle) -> WebviewWindow {
    app.get_webview_window(WINDOW).expect("the palette's window")
}

/// Whether the palette is there yet: Heyo may be asked for it while it is
/// still starting.
pub(crate) fn exists(app: &AppHandle) -> bool {
    app.get_webview_window(WINDOW).is_some()
}

fn is_up(app: &AppHandle) -> bool {
    window(app).is_visible().unwrap_or(false)
}

// --- Coming up and going away ----------------------------------------------------

/// The user opens the palette.
pub(crate) fn show(app: &AppHandle) {
    // For commands that act on that window.
    remember_window(app);
    let window = window(app);
    centre(app, &window);
    let _ = window.show();
    // And once more now that it is shown. A window that was hidden while
    // the display changed has been resized for the new scale by its
    // toolkit meanwhile, and GTK, having heard of that, takes the next
    // showing for a size the desktop gave it and does not ask for its
    // place: the first showing after the change came up where the window
    // had been on the old display. Asked of a window that is shown, the
    // place is asked for at once; where it is there already, nothing moves.
    centre(app, &window);
    focus(&window);
    let _ = tell(app, event::SHOWN, ());
}

/// Puts the palette in the middle of its monitor (the primary one until it
/// has been on one).
///
/// Reckoned here, from the size the window is declared with: the toolkit's
/// own centring goes by the size the window says it has, and a window that
/// has never been on the screen does not have its size yet, so the first
/// showing of a run came up off the middle (by a quarter of the window, at
/// a scale of 2).
///
/// And the toolkit is told the place in logical pixels, by the monitor's
/// scale as it is now. Told device pixels, it converts them by the scale
/// it last knew the window to have, and a window that was hidden while the
/// display changed hears of the new scale only once it is shown: the first
/// showing after a monitor of another density was connected came up at
/// twice the place, or half.
fn centre(app: &AppHandle, window: &WebviewWindow) {
    let declared = app.config().app.windows.iter().find(|declared| declared.label == WINDOW);
    let (Some(monitor), Some(declared)) = (display(window).monitor(Which::Its), declared) else {
        let _ = window.center();
        return;
    };
    let (x, y) = monitor.middle((declared.width, declared.height));
    let _ = window.set_position(tauri::LogicalPosition::new(f64::from(x) / monitor.scale, f64::from(y) / monitor.scale));
}

/// The display as Tauri's toolkit has it at this moment, and the monitor
/// the palette is on (none until it has been on one).
fn display(window: &WebviewWindow) -> Display {
    let whole = |monitor: &tauri::Monitor| (monitor.position().x, monitor.position().y, monitor.size().width, monitor.size().height);
    let (its, primary) = (window.current_monitor().ok().flatten(), window.primary_monitor().ok().flatten());
    let mut all = window.available_monitors().unwrap_or_default();
    // What the toolkit names is there, whatever its list says.
    for named in [&its, &primary].into_iter().flatten() {
        if !all.iter().any(|monitor| whole(monitor) == whole(named)) {
            all.push(named.clone());
        }
    }
    let primary = primary.as_ref().map(whole);
    let monitors: Vec<Monitor> = all.iter().map(|monitor| Monitor::new(whole(monitor), monitor.scale_factor(), Some(whole(monitor)) == primary, None)).collect();
    Display { its: its.and_then(|its| monitors.iter().position(|monitor| monitor.whole == whole(&its))), monitors }
}

/// Returns the palette as it was left, to the command that is in it.
pub(crate) fn bring_back(app: &AppHandle) {
    let window = window(app);
    let _ = window.show();
    focus(&window);
}

/// Puts the palette away. What is in it stays, for `bring_back`.
pub(crate) fn hide(app: &AppHandle) {
    let _ = window(app).hide();
}

pub(crate) fn toggle(app: &AppHandle) {
    if is_up(app) {
        hide(app);
    } else {
        show(app);
    }
}

/// The palette's own view is done with it: Escape at the root, a click
/// beside the panel, a command that ended. A question being asked in it
/// is thereby answered No.
pub(crate) fn dismiss(app: &AppHandle) {
    // A command that was just ending as the host put a question up must
    // not take the palette away with it.
    if confirm::just_asked() {
        return;
    }
    confirm::dismiss();
    hide(app);
}

/// What the toolkit says happened to the palette's window. (It says so on
/// the main thread.)
pub(crate) fn on_window_event(window: &Window, event: &WindowEvent) {
    match event {
        // The palette lives for as long as Heyo does: closing it (Alt+F4)
        // only hides it.
        WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            hide(window.app_handle());
        }
        WindowEvent::Focused(false) if window.state::<SettingsState>().0.read().unwrap().hide_on_blur => {
            platform::focus::when_lost(window, |window| hide(window.app_handle()));
        }
        // The display it is on became another. (Not on Linux, where the
        // toolkit says so of a window only once it is shown: the desktop
        // is asked there, `platform::place::watch`.)
        #[cfg(not(target_os = "linux"))]
        WindowEvent::ScaleFactorChanged { .. } => platform::place::changed(),
        _ => {}
    }
}

// --- Making way, and coming forward ----------------------------------------------

/// How long the palette takes to be gone from the screen once it is hidden.
const GONE: Duration = Duration::from_millis(180);

/// Puts the palette away and returns when it is off the screen, for what
/// is about to look at the screen. Says whether it was up. (It waits: not
/// for the main thread.)
pub(crate) fn hide_until_gone(app: &AppHandle) -> bool {
    let up = is_up(app);
    if up {
        hide(app);
        std::thread::sleep(GONE);
    }
    up
}

/// Runs `f` with the palette off the screen, and brings it back as it was:
/// the command that asked is still in it.
pub(crate) fn away_during<T>(app: &AppHandle, f: impl FnOnce() -> T) -> T {
    let up = hide_until_gone(app);
    let result = f();
    if up {
        bring_back(app);
    }
    result
}

/// Runs `f` with the palette up, bringing it forward for as long as `f`
/// takes if it was away: a command in the background has something to ask.
pub(crate) fn up_during<T>(app: &AppHandle, f: impl FnOnce() -> T) -> T {
    // Away for certain: a toolkit that cannot say is not taken at its word.
    let away = matches!(window(app).is_visible(), Ok(false));
    if away {
        bring_back(app);
    }
    let result = f();
    if away {
        hide(app);
    }
    result
}

// --- Its view --------------------------------------------------------------------

/// Tells the palette's view something: one of `wire::event`.
pub(crate) fn tell(app: &AppHandle, event: &str, payload: impl Serialize + Clone) -> Result<(), String> {
    app.emit_to(WINDOW, event, payload).map_err(|e| e.to_string())
}

/// `heyo --clipboard`: shows the palette with Clipboard History open.
pub(crate) fn open_clipboard_history(app: &AppHandle) {
    show(app);
    let launch = HostMessage::launch(extensions::BUILTIN, "clipboard-history", None);
    if let Err(e) = app.state::<Runtime>().send(app, &launch) {
        eprintln!("heyo: {e}");
    }
}

// --- The window behind it --------------------------------------------------------

/// The window the user came from when the palette came up.
pub(crate) fn came_from(app: &AppHandle) -> &windows::LastWindow {
    app.state::<windows::LastWindow>().inner()
}

/// Takes the window in front as the one the user came from: the palette is
/// about to come up over it.
pub(crate) fn remember_window(app: &AppHandle) {
    came_from(app).remember();
}

/// The window the user is working in: the one in front, or the one that
/// was before the palette came up.
pub(crate) fn working_window(app: &AppHandle) -> Option<windows::Window> {
    came_from(app).working()
}

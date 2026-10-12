//! GNOME on Wayland, through Heyo's own Shell extension
//! (`gnome-extension/heyo@heyo.app`), which does inside the Shell what an
//! application may not do from outside: see and arrange windows, type into
//! the focused one, follow the clipboard, read the screen.

pub mod extension;

use std::sync::mpsc::RecvTimeoutError;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use serde::Deserialize;
use zbus::blocking::{Connection, Proxy};

use crate::platform::capture::{Area, Shot};
use crate::platform::clipboard_watch::Change;
use crate::platform::keys::{Hold, HOLD, REGRET};
use crate::platform::windows::{target, Layout, Rect, Window, WindowInfo};

const DESTINATION: &str = "org.gnome.Shell";
const PATH: &str = "/dev/heyo/Shell";
const INTERFACE: &str = "dev.heyo.Shell";
/// The extension's version this code was written with (the one carried
/// inside Heyo), and the oldest it still works with. The interface only
/// grows, so a newer extension is as good as this one; with an older one,
/// what it lacks is done without (see where optional fields are read).
pub(crate) const API: u32 = 2;
const OLDEST: u32 = 1;

fn connection() -> Result<&'static Connection, String> {
    static CONNECTION: OnceLock<Option<Connection>> = OnceLock::new();
    CONNECTION.get_or_init(|| Connection::session().ok()).as_ref().ok_or_else(|| "no session bus".to_owned())
}

fn call<B, R>(method: &str, body: &B) -> Result<R, String>
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
    R: for<'d> zbus::zvariant::DynamicDeserialize<'d>,
{
    let reply = connection()?.call_method(Some(DESTINATION), PATH, Some(INTERFACE), method, body).map_err(|e| match e {
        zbus::Error::MethodError(_, Some(message), _) => message,
        other => other.to_string(),
    })?;
    reply.body().deserialize().map_err(|e| e.to_string())
}

/// The version of the extension running in the Shell, if one answers.
pub fn running() -> Option<u32> {
    static RUNNING: OnceLock<Option<u32>> = OnceLock::new();
    *RUNNING.get_or_init(|| call::<_, u32>("Version", &()).ok())
}

/// Whether the extension is there and Heyo can work with it.
pub fn available() -> bool {
    running().is_some_and(|version| version >= OLDEST)
}

#[derive(Deserialize)]
struct ShellWindow {
    id: Window,
    title: String,
    class: String,
    active: bool,
    monitor: i32,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    /// On the active workspace and not minimized. An extension from before
    /// this was reported does not say, and every window counts as shown.
    #[serde(default = "yes")]
    shown: bool,
    /// Its place in the stack, the bottom window being 0.
    #[serde(default)]
    layer: i32,
}

fn yes() -> bool {
    true
}

/// A monitor as the Shell places it. Its numbers, like every position the
/// extension gives or takes, are in the Shell's own units: pixels where
/// monitors are laid out in pixels, pixels divided by the monitor's scale
/// where they are not (the layout fractional scaling needs).
#[derive(Deserialize, Clone, Copy)]
struct Monitor {
    x: i32,
    y: i32,
    work: WorkArea,
    #[serde(default = "one")]
    scale: f64,
    /// How many pixels one unit is. An extension from before this was
    /// reported does not say: then units are taken to be scaled, which is
    /// what was assumed then.
    #[serde(default)]
    density: Option<f64>,
    /// The pointer is on this monitor.
    #[serde(default)]
    pointer: bool,
}

#[derive(Deserialize, Clone, Copy)]
struct WorkArea {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

fn one() -> f64 {
    1.0
}

impl Monitor {
    /// How many of the toolkit's "physical" pixels one of the Shell's units
    /// is on this monitor. GTK 3 only knows whole scales: where the Shell
    /// lays monitors out in pixels the two agree; where it does not, the
    /// toolkit's pixels are the Shell's units times the scale rounded up
    /// (at 150 % GTK draws at 200 % and the Shell shrinks the result).
    fn toolkit(&self) -> i32 {
        if self.density == Some(1.0) {
            1
        } else {
            self.scale.ceil().max(1.0) as i32
        }
    }

    /// Where the toolkit says this monitor starts.
    fn toolkit_origin(&self) -> (i32, i32) {
        (self.x * self.toolkit(), self.y * self.toolkit())
    }
}

fn monitors() -> Result<Vec<Monitor>, String> {
    let json: String = call("Monitors", &())?;
    serde_json::from_str(&json).map_err(|e| e.to_string())
}

/// A monitor's area as the toolkit gives it, in the Shell's units.
fn stage_area(area: Area, monitors: &[Monitor]) -> (i32, i32, i32, i32) {
    let factor = match monitors.iter().find(|m| m.toolkit_origin() == (area.x, area.y)) {
        Some(monitor) => monitor.toolkit() as f64,
        // Not a monitor the Shell told of: as before it told of any.
        None if area.scale > 0.0 => area.scale,
        None => 1.0,
    };
    let unit = |v: f64| (v / factor).round() as i32;
    (unit(area.x as f64), unit(area.y as f64), unit(area.width as f64), unit(area.height as f64))
}

/// A window's frame in the toolkit's pixels, as monitors' areas are.
fn toolkit_frame(window: &ShellWindow, monitors: &[Monitor]) -> Rect {
    let factor = monitors.get(window.monitor.max(0) as usize).map_or(1, Monitor::toolkit);
    Rect { x: window.x * factor, y: window.y * factor, width: window.width * factor, height: window.height * factor }
}

/// Where the monitor the pointer is on starts, in the toolkit's pixels.
/// (On Wayland the toolkit knows the pointer only over Heyo's own windows.)
pub fn pointer_monitor() -> Option<(i32, i32)> {
    monitors().ok()?.iter().find(|m| m.pointer).map(Monitor::toolkit_origin)
}

/// Other applications' windows, most recently used first.
fn shell_windows() -> Result<Vec<ShellWindow>, String> {
    let json: String = call("Windows", &())?;
    serde_json::from_str(&json).map_err(|e| e.to_string())
}

pub mod windows {
    use super::*;

    pub fn active_external() -> Option<Window> {
        shell_windows().ok()?.into_iter().find(|w| w.active).map(|w| w.id)
    }

    pub fn active_class() -> Option<String> {
        shell_windows().ok()?.into_iter().find(|w| w.active).map(|w| w.class).filter(|c| !c.is_empty())
    }

    pub fn list(current: Option<Window>) -> Result<Vec<WindowInfo>, String> {
        let mut windows: Vec<WindowInfo> = shell_windows()?
            .into_iter()
            .filter(|w| !w.title.is_empty())
            .map(|w| WindowInfo {
                active: Some(w.id) == current,
                bounds: Some(Rect { x: w.x, y: w.y, width: w.width, height: w.height }),
                id: w.id,
                title: w.title,
                class: w.class,
            })
            .collect();
        windows.sort_by_key(|w| w.active);
        Ok(windows)
    }

    /// The windows on screen now, the topmost first, their frames in the
    /// toolkit's pixels (what a capture's area is given in).
    pub fn shown() -> Result<Vec<WindowInfo>, String> {
        let monitors = monitors().unwrap_or_default();
        let mut windows: Vec<ShellWindow> = shell_windows()?.into_iter().filter(|w| w.shown && !w.title.is_empty()).collect();
        // Stable: without layers (an older extension) the order stays most recently used first.
        windows.sort_by_key(|w| std::cmp::Reverse(w.layer));
        Ok(windows
            .into_iter()
            .map(|w| WindowInfo {
                active: w.active,
                bounds: Some(toolkit_frame(&w, &monitors)),
                id: w.id,
                title: w.title,
                class: w.class,
            })
            .collect())
    }

    pub fn before(window: Window) -> Option<Window> {
        shell_windows().ok()?.into_iter().map(|w| w.id).find(|id| *id != window)
    }

    pub fn of_classes(classes: &[String]) -> Vec<Window> {
        shell_windows().unwrap_or_default().into_iter().filter(|w| classes.contains(&w.class.to_lowercase())).map(|w| w.id).collect()
    }

    pub fn activate(window: Window) -> Result<(), String> {
        call("Activate", &(window,))
    }

    pub fn close(window: Window) -> Result<(), String> {
        call("Close", &(window,))
    }

    /// Moves a window into a layout. The Shell knows the monitors and their
    /// free areas itself; `_monitors` (from the toolkit) is not needed.
    pub fn apply(window: Window, layout: Layout, _monitors: &[Rect]) -> Result<(), String> {
        if layout == Layout::Maximize {
            return call("Maximize", &(window,));
        }
        // Frames and work areas are both in the Shell's units, whatever those are.
        let monitors = monitors()?;
        let current = shell_windows()?.into_iter().find(|w| w.id == window).ok_or("the window is gone")?;
        let area = |index: usize| monitors.get(index).map(|m| Rect { x: m.work.x, y: m.work.y, width: m.work.width, height: m.work.height });
        let index = current.monitor.max(0) as usize;
        let from = area(index).ok_or("no monitor")?;
        let frame = Rect { x: current.x, y: current.y, width: current.width, height: current.height };
        let rect = if layout == Layout::NextDisplay {
            if monitors.len() < 2 {
                return Err("There is only one display.".into());
            }
            // Same relative place on the next monitor.
            let to = area((index + 1) % monitors.len()).ok_or("no monitor")?;
            Rect {
                x: to.x + (frame.x - from.x) * to.width / from.width.max(1),
                y: to.y + (frame.y - from.y) * to.height / from.height.max(1),
                width: frame.width * to.width / from.width.max(1),
                height: frame.height * to.height / from.height.max(1),
            }
        } else {
            target(layout, from, frame)
        };
        call("MoveResize", &(window, rect.x, rect.y, rect.width.max(1), rect.height.max(1)))
    }
}

/// Whether one of Heyo's own windows has the keyboard focus, as the Shell
/// sees it. `None` if it could not be asked.
pub fn own_focused() -> Option<bool> {
    call::<_, bool>("OwnFocused", &()).ok()
}

/// Brings one of Heyo's own windows to the front.
pub fn activate_own(title: &str) -> bool {
    call::<_, bool>("ActivateOwn", &(title,)).unwrap_or(false)
}

/// Presses the paste shortcut in the focused application.
pub fn paste(terminal: bool) -> Result<(), String> {
    call("Paste", &(terminal,))
}

pub fn selection() -> String {
    call::<_, String>("Selection", &()).unwrap_or_default()
}

pub fn clipboard_text() -> Option<String> {
    call::<_, String>("ClipboardText", &()).ok()
}

pub fn set_clipboard_text(text: &str) -> Result<(), String> {
    call("SetClipboardText", &(text,))
}

/// The image on the clipboard as RGBA, if there is one.
pub fn clipboard_image() -> Option<image::RgbaImage> {
    let path = std::env::var_os("XDG_RUNTIME_DIR").map(std::path::PathBuf::from)?.join(format!("heyo-clipboard-{}.png", std::process::id()));
    let written = call::<_, bool>("ClipboardImage", &(path.to_string_lossy().as_ref(),)).unwrap_or(false);
    let image = written.then(|| image::open(&path).ok()).flatten().map(|i| i.into_rgba8());
    let _ = std::fs::remove_file(&path);
    image
}

/// Reports each clipboard change until the session ends.
pub fn watch_clipboard(mut on_change: impl FnMut(Change)) -> Result<(), String> {
    let proxy = Proxy::new(connection()?, DESTINATION, PATH, INTERFACE).map_err(|e| e.to_string())?;
    let signals = proxy.receive_signal("ClipboardChanged").map_err(|e| e.to_string())?;
    for signal in signals {
        let Ok((source, mimetypes)) = signal.body().deserialize::<(String, Vec<String>)>() else { continue };
        on_change(Change {
            has_text: mimetypes.iter().any(|m| m.starts_with("text/plain") || m == "UTF8_STRING" || m == "STRING"),
            has_image: mimetypes.iter().any(|m| m.starts_with("image/")),
            // What password managers add to secrets.
            secret: mimetypes.iter().any(|m| m == "x-kde-passwordManagerHint"),
            source: Some(source).filter(|s| !s.is_empty()),
        });
    }
    Ok(())
}

/// The keys the Shell can report being held: modifiers only.
pub const HOLD_KEYS: &[&str] = &["alt_r", "alt_l", "control_r", "control_l", "shift_r"];

/// Reports the hold-to-talk key, as `x11::hold_key::watch` does.
pub fn watch_hold(key: impl Fn() -> String, on: impl Fn(Hold)) -> Result<(), String> {
    #[derive(Clone, Copy)]
    enum Event {
        Down,
        Up,
        Interrupted,
        /// The extension was switched on again (the Shell switches
        /// extensions off while the screen is locked) and has forgotten
        /// which key to watch.
        Ready,
    }
    enum State {
        Idle,
        /// Down, not yet for `HOLD`.
        Pending(Instant),
        /// Reported as started, then.
        Held(Instant),
    }

    let proxy = Proxy::new(connection()?, DESTINATION, PATH, INTERFACE).map_err(|e| e.to_string())?;
    let (send, events) = std::sync::mpsc::channel();
    for (name, event) in [("KeyDown", Event::Down), ("KeyUp", Event::Up), ("KeyInterrupted", Event::Interrupted), ("Ready", Event::Ready)] {
        let signals = proxy.receive_signal(name).map_err(|e| e.to_string())?;
        let send = send.clone();
        std::thread::spawn(move || {
            for _ in signals {
                if send.send(event).is_err() {
                    break;
                }
            }
        });
    }

    let mut watched: Option<String> = None;
    let mut state = State::Idle;
    loop {
        // The setting may have changed.
        let wanted = key();
        if watched.as_deref() != Some(wanted.as_str()) {
            let name = if HOLD_KEYS.contains(&wanted.as_str()) { wanted.as_str() } else { "" };
            match call::<_, bool>("WatchKey", &(name,)) {
                Ok(true) => eprintln!("heyo: push-to-talk key: {wanted}"),
                Ok(false) if !wanted.is_empty() => eprintln!("heyo: push-to-talk key {wanted} cannot be watched in this session"),
                Ok(false) => {}
                Err(e) => eprintln!("heyo: push-to-talk: {e}"),
            }
            watched = Some(wanted);
            state = State::Idle;
        }
        let wait = match state {
            State::Pending(down) => HOLD.saturating_sub(down.elapsed()),
            _ => Duration::from_secs(1),
        };
        let event = match events.recv_timeout(wait) {
            Ok(event) => Some(event),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => return Err("the Shell extension went away".into()),
        };
        state = match (state, event) {
            (_, Some(Event::Ready)) => {
                watched = None;
                State::Idle
            }
            (State::Idle, Some(Event::Down)) => State::Pending(Instant::now()),
            (State::Pending(down), None) if down.elapsed() >= HOLD => {
                eprintln!("heyo: push-to-talk: started");
                on(Hold::Started);
                State::Held(Instant::now())
            }
            // Let go early, or used as a modifier: a shortcut, not dictation.
            (State::Pending(_), Some(Event::Up | Event::Interrupted)) => State::Idle,
            (State::Held(_), Some(Event::Up)) => {
                eprintln!("heyo: push-to-talk: released");
                on(Hold::Released);
                State::Idle
            }
            (State::Held(started), Some(Event::Interrupted)) if started.elapsed() < REGRET => {
                eprintln!("heyo: push-to-talk: called off, another key was pressed");
                on(Hold::Cancelled);
                State::Idle
            }
            (state, _) => state,
        };
    }
}

/// Captures an area of the screen. The picture has the monitor's own
/// pixels, which under fractional scaling are fewer than the toolkit's.
pub fn grab(area: Area) -> Result<Shot, String> {
    let dir = crate::paths::runtime_dir();
    let path = dir.join(format!("heyo-capture-{}.png", std::process::id()));
    let (x, y, width, height) = stage_area(area, &monitors().unwrap_or_default());
    let request = (x, y, width, height, path.to_string_lossy().into_owned());
    call::<_, ()>("Screenshot", &request)?;
    let image = image::open(&path).map_err(|e| e.to_string());
    let _ = std::fs::remove_file(&path);
    let image = image?.into_rgba8();
    let (width, height) = image.dimensions();
    let mut bgrx = image.into_raw();
    for pixel in bgrx.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    Ok(Shot { width, height, bgrx })
}

/// GNOME on Wayland, with the Shell extension, as a `Desktop`.
pub struct Gnome;

impl crate::platform::desktop::Desktop for Gnome {
    fn supported(&self) -> crate::platform::desktop::Supported {
        use crate::platform::Support;
        crate::platform::desktop::Supported {
            windows: Support::Full,
            paste: Support::Full,
            selection: Support::Full,
            clipboard_watch: Support::Full,
            // Only modifiers, and pointer buttons do not call it off.
            hold_key: Support::Limited,
            capture: Support::Full,
            // An ordinary fullscreen window, which the Shell may animate.
            overlay: Support::Limited,
            // The Shell does not show applications what is typed elsewhere.
            typing: Support::None,
            // Nor could it be known that nobody else touched the text: the
            // extension pastes, and erases nothing.
            rewriting: Support::None,
            // IBus is here too, with the Shell as its one client. Composing
            // through it has not been tried on Wayland.
            composing: Support::None,
        }
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

    /// Through the Shell, which is the only one whose synthetic keys GNOME
    /// accepts; the generic tools if that fails.
    fn paste(&self, terminal: bool) -> Result<(), String> {
        paste(terminal).or_else(|e| {
            eprintln!("heyo: could not paste through the Shell extension: {e}");
            crate::platform::input::paste_with_tools(terminal)
        })
    }

    fn selected_text(&self) -> String {
        selection()
    }

    /// Through the Shell, which may read the clipboard whoever is in front.
    fn clipboard_text(&self) -> Option<String> {
        clipboard_text().or_else(crate::platform::clipboard_watch::toolkit_text)
    }

    fn clipboard_image(&self) -> Option<image::RgbaImage> {
        clipboard_image()
    }

    fn set_clipboard_text(&self, text: &str) -> Result<(), String> {
        set_clipboard_text(text).or_else(|_| crate::platform::clipboard_watch::toolkit_set_text(text))
    }

    fn watch_clipboard(&self, on_change: &mut dyn FnMut(Change)) -> Result<(), String> {
        watch_clipboard(on_change)
    }

    fn grab(&self, area: Area) -> Result<Shot, String> {
        grab(area).or_else(|e| {
            eprintln!("heyo: capture failed ({e}); trying gnome-screenshot");
            crate::platform::capture::grab_with_tool(area)
        })
    }

    fn hold_keys(&self) -> Vec<&'static str> {
        HOLD_KEYS.to_vec()
    }

    fn watch_hold(&self, key: &dyn Fn() -> String, on: &dyn Fn(Hold)) -> Result<(), String> {
        watch_hold(key, on)
    }

    fn activate_own(&self, title: &str) -> bool {
        activate_own(title)
    }

    fn own_focused(&self) -> Option<bool> {
        own_focused()
    }

    fn pointer_monitor(&self) -> Option<(i32, i32)> {
        pointer_monitor()
    }


    fn hotkeys(&self) -> Option<&dyn crate::platform::desktop::Hotkeys> {
        crate::platform::hotkeys::gsettings()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor(x: i32, y: i32, scale: f64, density: Option<f64>) -> Monitor {
        Monitor { x, y, work: WorkArea { x, y, width: 100, height: 100 }, scale, density, pointer: false }
    }

    fn area(x: i32, y: i32, width: u32, height: u32, scale: f64) -> Area {
        Area { x, y, width, height, scale }
    }

    fn window(monitor: i32, x: i32, y: i32, width: i32, height: i32) -> ShellWindow {
        ShellWindow { id: 1, title: String::new(), class: String::new(), active: false, monitor, x, y, width, height, shown: true, layer: 0 }
    }

    #[test]
    fn an_older_extension_s_answers_are_still_read() {
        // Version 1 said neither how units relate to pixels nor where the pointer is.
        let old: Vec<Monitor> = serde_json::from_str(r#"[{"x":0,"y":0,"width":960,"height":540,"work":{"x":0,"y":16,"width":960,"height":524},"scale":2}]"#).unwrap();
        assert_eq!((old[0].density, old[0].pointer, old[0].toolkit()), (None, false, 2));
        let new: Vec<Monitor> =
            serde_json::from_str(r#"[{"x":0,"y":0,"width":3840,"height":2160,"work":{"x":0,"y":0,"width":1,"height":1},"scale":2,"density":1,"pointer":true}]"#).unwrap();
        assert_eq!((new[0].density, new[0].pointer, new[0].toolkit()), (Some(1.0), true, 1));
    }

    #[test]
    fn a_capture_is_asked_for_in_the_shell_s_units() {
        // Laid out scaled: a 4K monitor at 200 % is 1920 units wide, and one
        // at 100 % beside it starts at 1920. The toolkit multiplies each
        // monitor's place by that monitor's own scale.
        let scaled = [monitor(0, 0, 2.0, Some(2.0)), monitor(1920, 0, 1.0, Some(1.0))];
        assert_eq!(stage_area(area(0, 0, 3840, 2160, 2.0), &scaled), (0, 0, 1920, 1080));
        assert_eq!(stage_area(area(1920, 0, 1920, 1080, 1.0), &scaled), (1920, 0, 1920, 1080));
        // At 150 % the toolkit draws at 200 %: 1280 units are its 2560 pixels.
        let fractional = [monitor(0, 0, 1.5, Some(1.5)), monitor(1280, 0, 1.5, Some(1.5))];
        assert_eq!(stage_area(area(2560, 0, 2560, 1440, 2.0), &fractional), (1280, 0, 1280, 720));
        // Laid out in pixels, units are pixels, whatever the scale.
        let pixels = [monitor(0, 0, 2.0, Some(1.0)), monitor(3840, 0, 2.0, Some(1.0))];
        assert_eq!(stage_area(area(3840, 0, 3840, 2160, 2.0), &pixels), (3840, 0, 3840, 2160));
        // An older extension did not say: scaled is assumed, as it was.
        assert_eq!(stage_area(area(0, 0, 3840, 2160, 2.0), &[monitor(0, 0, 2.0, None)]), (0, 0, 1920, 1080));
        assert_eq!(stage_area(area(0, 0, 3840, 2160, 2.0), &[]), (0, 0, 1920, 1080));
    }

    #[test]
    fn frames_are_given_in_the_toolkit_s_pixels() {
        let scaled = [monitor(0, 0, 2.0, Some(2.0)), monitor(1920, 0, 1.0, Some(1.0))];
        assert_eq!(toolkit_frame(&window(0, 100, 50, 800, 600), &scaled), Rect { x: 200, y: 100, width: 1600, height: 1200 });
        assert_eq!(toolkit_frame(&window(1, 2000, 50, 800, 600), &scaled), Rect { x: 2000, y: 50, width: 800, height: 600 });
        let pixels = [monitor(0, 0, 2.0, Some(1.0))];
        assert_eq!(toolkit_frame(&window(0, 100, 50, 800, 600), &pixels), Rect { x: 100, y: 50, width: 800, height: 600 });
    }

    #[test]
    fn a_monitor_starts_where_the_toolkit_says() {
        assert_eq!(monitor(1280, 0, 1.5, Some(1.5)).toolkit_origin(), (2560, 0));
        assert_eq!(monitor(3840, 0, 2.0, Some(1.0)).toolkit_origin(), (3840, 0));
        assert_eq!(monitor(-1920, 0, 1.0, Some(1.0)).toolkit_origin(), (-1920, 0));
    }
}

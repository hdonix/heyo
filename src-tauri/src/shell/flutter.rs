//! Heyo's windows drawn by Flutter: one `heyo-ui` process for each (`ui/`
//! in the repository), started and spoken to like the other helpers.
//!
//! The host started the process for a window, so it knows which window is
//! asking: commands are held to the table the web view's windows are held
//! to (`commands::access`). What the palette, the dictation pill and the
//! screenshot overlay are in the web view (windows that live as long as
//! Heyo does, hidden when not in use) their processes are here: the palette
//! is started with Heyo, so that it appears at once; the other two the
//! first time they are needed, and then kept. A settings window or a pinned
//! capture is a process that ends when it is closed.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

use super::bridge::{self, Incoming};
use super::{dispatch, Shell, Window};
use crate::helper::Helper;

const BINARY: &str = if cfg!(windows) { "heyo-ui.exe" } else { "heyo-ui" };

/// What the build directory of `ui/` is called for this processor.
#[cfg(debug_assertions)]
const FLUTTER_ARCH: &str = if cfg!(target_arch = "aarch64") { "arm64" } else { "x64" };

/// The running processes, by the label of their window.
#[derive(Default)]
pub(crate) struct Processes {
    binary: OnceLock<PathBuf>,
    running: Mutex<HashMap<String, Arc<Process>>>,
}

pub(crate) struct Process {
    label: String,
    /// What the desktop is asked to bring forward (GNOME on Wayland).
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    title: String,
    /// None once it was told to end, or before it has started.
    helper: Mutex<Option<Helper>>,
    started: Instant,
    /// The host shows and hides the window, so the host knows.
    visible: AtomicBool,
    focused: AtomicBool,
    /// The number of the last ping answered.
    pongs: AtomicU64,
    /// When the window was last asked to show, for measuring.
    asked: Mutex<Option<Instant>>,
}

impl Process {
    fn send_line(&self, line: &str) -> Result<(), String> {
        match &mut *self.helper.lock().unwrap() {
            Some(helper) => helper.send_line(line).map_err(|e| format!("the window {} is gone: {e}", self.label)),
            None => Err(format!("the window {} is gone", self.label)),
        }
    }

    fn send(&self, message: &Value) -> Result<(), String> {
        self.send_line(&message.to_string())
    }
}

/// A window by its label, running or not: what `shell::Window` holds.
#[derive(Clone)]
pub(crate) struct Handle {
    app: AppHandle,
    label: String,
}

/// What a window is like, which its process is told as it starts.
#[derive(Debug, PartialEq)]
struct Spec {
    /// Which view it shows: `--window`.
    kind: &'static str,
    title: String,
    /// Logical pixels.
    width: f64,
    height: f64,
    /// Whether it takes the keyboard.
    focus: bool,
    above: bool,
    taskbar: bool,
}

/// The kind of window a label names; the same labels as the web view's.
fn kind(label: &str) -> Option<&'static str> {
    match label {
        "main" => Some("main"),
        "settings" => Some("settings"),
        "dictation" => Some("dictation"),
        "screenshot" => Some("screenshot"),
        "island" => Some("island"),
        _ => label.strip_prefix("pin-").filter(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())).map(|_| "pin"),
    }
}

/// Windows that are kept, hidden, between uses.
fn kept(label: &str) -> bool {
    matches!(label, "main" | "dictation" | "screenshot")
}

/// The window of that label, as `tauri.conf.json` describes the web view's
/// (one place says how large the palette is) or as the web view's own code
/// builds it.
fn spec(app: &AppHandle, label: &str) -> Option<Spec> {
    let kind = kind(label)?;
    if let Some(config) = app.config().app.windows.iter().find(|window| window.label == label) {
        return Some(Spec {
            kind,
            title: config.title.clone(),
            width: config.width,
            height: config.height,
            focus: config.focus,
            above: config.always_on_top,
            taskbar: !config.skip_taskbar,
        });
    }
    Some(match kind {
        // 860 by 640 and the margin its shadow falls in (`commands::settings::open_settings`).
        "settings" => Spec { kind, title: "Heyo Settings".into(), width: 860.0 + 64.0, height: 640.0 + 64.0, focus: true, above: false, taskbar: true },
        "pin" => Spec { kind, title: "Heyo Pinned Screenshot".into(), width: 24.0, height: 24.0, focus: true, above: true, taskbar: false },
        "island" => Spec { kind, title: "Heyo Island".into(), width: 420.0, height: 160.0, focus: false, above: true, taskbar: false },
        _ => return None,
    })
}

/// Where `heyo-ui` is: what `HEYO_UI_BIN` names, else the repository's
/// build in a debug build, else beside Heyo's other resources.
fn locate(app: &AppHandle) -> Result<(PathBuf, bool), String> {
    if let Some(path) = std::env::var_os("HEYO_UI_BIN") {
        // Someone trying a build of their own: theirs to keep in step.
        return Ok((path.into(), false));
    }
    #[cfg(debug_assertions)]
    {
        let _ = app;
        let bundle = format!("{}/../ui/build/linux/{FLUTTER_ARCH}/release/bundle", env!("CARGO_MANIFEST_DIR"));
        Ok((PathBuf::from(bundle).join(BINARY), true))
    }
    #[cfg(not(debug_assertions))]
    {
        let dir = app.path().resource_dir().map_err(|e| e.to_string())?;
        Ok((dir.join("ui").join(BINARY), true))
    }
}

/// What `heyo-ui --version` says it was built for: `heyo-ui <version>`.
fn built_for(output: &str) -> Option<&str> {
    output.lines().next()?.trim_end().strip_prefix("heyo-ui ").filter(|version| !version.is_empty())
}

/// Why this `heyo-ui` may not draw this Heyo's windows: only one built
/// together with it may. `said` is None when it could not be run.
fn refusal(binary: &std::path::Path, said: Option<&str>, heyo: &str) -> Option<String> {
    let place = binary.display();
    let fix = "Build it with scripts/build-ui.sh and install Heyo again (scripts/install.sh), or start Heyo without --ui=flutter.";
    let Some(said) = said else {
        return Some(format!("Heyo {heyo} cannot run its Flutter interface, {place}. {fix}"));
    };
    match built_for(said) {
        Some(version) if version == heyo => None,
        Some(version) => Some(format!("This is Heyo {heyo}, but {place} is from Heyo {version}. {fix}")),
        None => Some(format!("{place} is not Heyo's Flutter interface. {fix}")),
    }
}

/// Finds the program and makes sure it came with this Heyo. An error says
/// why the Flutter interface cannot be used.
pub(crate) fn prepare(app: &AppHandle) -> Result<(), String> {
    // `ui/` has a runner for Linux only so far.
    if !cfg!(target_os = "linux") {
        return Err("Heyo's Flutter interface is not built for this system yet.".into());
    }
    let (binary, check) = locate(app)?;
    if check {
        let said = crate::platform::common::quiet(&binary).arg("--version").output().ok().filter(|output| output.status.success());
        let said = said.map(|output| String::from_utf8_lossy(&output.stdout).into_owned());
        if let Some(why) = refusal(&binary, said.as_deref(), &app.package_info().version.to_string()) {
            return Err(why);
        }
    } else if !binary.exists() {
        return Err(format!("HEYO_UI_BIN names {}, which is not there", binary.display()));
    }
    let _ = app.state::<Shell>().flutter.binary.set(binary);
    Ok(())
}

/// The scale the host's own toolkit draws at, which is the one the web
/// view would have. On X11 it is one number for the whole display, and
/// `heyo-ui` is told it, so that the two interfaces are the same size
/// whatever each would have worked out by itself. (Wayland gives every
/// monitor its own, which the toolkit follows there.)
#[cfg(target_os = "linux")]
fn display_scale(app: &AppHandle) -> Option<u32> {
    if crate::platform::session() != crate::platform::Session::X11 || std::env::var_os("GDK_SCALE").is_some() {
        return None;
    }
    let scale = app.primary_monitor().ok().flatten()?.scale_factor();
    (scale >= 1.0).then_some(scale.round() as u32)
}

/// Starts the process of the window `label`. `init` is what its view is
/// started with (a pinned capture's file).
fn start(app: &AppHandle, label: &str, init: Value) -> Result<Arc<Process>, String> {
    let shell = app.state::<Shell>();
    let binary = shell.flutter.binary.get().ok_or("the Flutter interface is not in use")?;
    let spec = spec(app, label).ok_or_else(|| format!("{label} is not a window of Heyo's"))?;
    let mut command = crate::platform::common::quiet(binary);
    let window = json!({ "title": spec.title, "width": spec.width, "height": spec.height, "focus": spec.focus, "above": spec.above, "taskbar": spec.taskbar });
    command.arg(format!("--window={}", spec.kind)).arg(format!("--label={label}")).arg(format!("--init={}", json!({ "window": window, "view": init })));
    #[cfg(target_os = "linux")]
    if let Some(scale) = display_scale(app) {
        command.env("GDK_SCALE", scale.to_string());
    }
    let process = Arc::new(Process {
        label: label.to_owned(),
        title: spec.title,
        helper: Mutex::new(None),
        started: Instant::now(),
        visible: AtomicBool::new(false),
        focused: AtomicBool::new(false),
        pongs: AtomicU64::new(0),
        asked: Mutex::new(None),
    });
    let mut running = shell.flutter.running.lock().unwrap();
    // Two threads may have found it missing at once.
    if let Some(already) = running.get(label) {
        return Ok(already.clone());
    }
    let (receiver, ended) = ((app.clone(), process.clone()), (app.clone(), process.clone()));
    let helper = Helper::start(command, move |message| received(&receiver.0, &receiver.1, message), move || exited(&ended.0, &ended.1))
        .map_err(|e| format!("failed to start {}: {e}", binary.display()))?;
    *process.helper.lock().unwrap() = Some(helper);
    running.insert(label.to_owned(), process.clone());
    Ok(process)
}

/// A message from a window's process.
fn received(app: &AppHandle, process: &Arc<Process>, message: Value) {
    match bridge::read(&message) {
        Ok(Incoming::Invoke { id, command, args }) => {
            let window = Window::Flutter(Handle { app: app.clone(), label: process.label.clone() });
            let asker = process.clone();
            dispatch::invoke(app, &window, &command, args, move |result| {
                // An answer for a window that has gone is nobody's.
                let _ = asker.send(&bridge::reply(&id, result));
            });
        }
        Ok(Incoming::Window { what, detail }) => happened(app, process, &what, &detail),
        Ok(Incoming::Pong(n)) => process.pongs.store(n, Ordering::SeqCst),
        Err(e) => eprintln!("heyo: the window {}: {e}", process.label),
    }
}

fn debugging() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("HEYO_UI_DEBUG").is_some())
}

/// Something happened to a window, says its process.
fn happened(app: &AppHandle, process: &Arc<Process>, what: &str, detail: &Value) {
    let handle = Handle { app: app.clone(), label: process.label.clone() };
    if debugging() && what != "shown" && what != "ready" {
        eprintln!("heyo: ui {} says {what}", process.label);
    }
    match what {
        "ready" => {
            if debugging() {
                eprintln!("heyo: ui {} drew its first frame {} ms after it was started", process.label, process.started.elapsed().as_millis());
            }
        }
        "focused" => process.focused.store(true, Ordering::SeqCst),
        "blurred" => {
            process.focused.store(false, Ordering::SeqCst);
            // Only the palette hides itself; the settings window is a normal window.
            if process.label == "main" && app.state::<crate::SettingsState>().0.read().unwrap().hide_on_blur {
                let lost = process.clone();
                std::thread::spawn(move || when_lost(&lost, || handle.hide()));
            }
        }
        // The palette, the dictation window and the screenshot overlay live
        // for as long as Heyo does: closing one (Alt+F4) only hides it.
        "close" => match process.label.as_str() {
            "main" | crate::dictation::WINDOW => handle.hide(),
            crate::screenshot::WINDOW => crate::screenshot::cancel(app),
            _ => handle.close(),
        },
        "shown" => {
            if debugging() {
                let since = process.asked.lock().unwrap().take().map(|asked| asked.elapsed().as_millis());
                eprintln!("heyo: ui {} shown: {:?} ms after the host asked, {} ms of that in the window", process.label, since, detail["ms"]);
            }
        }
        _ => eprintln!("heyo: the window {} says {what}, which means nothing here", process.label),
    }
}

/// Runs `then` if the window, which just said it lost the focus, has really
/// lost it: the rule of `platform::focus::when_lost`, for a window of
/// another process. The toolkit says so whenever a keyboard appears or its
/// layout changes (Wayland), with the window still in front: look again a
/// little later, and ask the desktop where it knows better.
fn when_lost(process: &Process, then: impl FnOnce()) {
    std::thread::sleep(Duration::from_millis(150));
    if process.focused.load(Ordering::SeqCst) || !process.visible.load(Ordering::SeqCst) {
        return;
    }
    if crate::platform::desktop::desktop().own_focused() != Some(true) {
        then();
    }
}

/// A window's process has ended.
fn exited(app: &AppHandle, process: &Arc<Process>) {
    let shell = app.state::<Shell>();
    {
        let mut running = shell.flutter.running.lock().unwrap();
        if running.get(&process.label).is_some_and(|current| Arc::ptr_eq(current, process)) {
            running.remove(&process.label);
        }
    }
    let was_visible = process.visible.swap(false, Ordering::SeqCst);
    let asked_to = process.helper.lock().unwrap().is_none();
    if let Some(n) = process.label.strip_prefix("pin-") {
        // A pinned capture is deleted with its window.
        crate::screenshot::unpinned(n);
    }
    if process.label == crate::screenshot::WINDOW && was_visible {
        // The overlay went away by itself: what was under it must too.
        crate::screenshot::cancel(app);
    }
    if !asked_to && kept(&process.label) {
        eprintln!("heyo: the window {} ended by itself", process.label);
        // The palette is expected at once: have it ready again. (Not one
        // that cannot stay up; that is started when next asked for.)
        if process.label == "main" && process.started.elapsed() > Duration::from_secs(5) {
            if let Err(e) = start(app, "main", Value::Null) {
                eprintln!("heyo: {e}");
            }
        }
    }
}

impl Handle {
    /// The window of that label, if it is one that exists now: a kept
    /// window always does (its process is started when first needed), any
    /// other only while it is open.
    pub(crate) fn existing(app: &AppHandle, label: &str) -> Option<Handle> {
        let handle = Handle { app: app.clone(), label: label.to_owned() };
        (kept(label) || handle.running().is_some()).then_some(handle)
    }

    pub(crate) fn label(&self) -> &str {
        &self.label
    }

    fn running(&self) -> Option<Arc<Process>> {
        self.app.state::<Shell>().flutter.running.lock().unwrap().get(&self.label).cloned()
    }

    /// The window's process, started if it is not running.
    fn process(&self) -> Result<Arc<Process>, String> {
        match self.running() {
            Some(process) => Ok(process),
            None => start(&self.app, &self.label, Value::Null),
        }
    }

    fn tell(&self, what: &str, detail: Value) {
        let sent = self.process().and_then(|process| process.send(&bridge::control(what, detail)));
        if let Err(e) = sent {
            eprintln!("heyo: {e}");
        }
    }

    pub(crate) fn show(&self) {
        match self.process() {
            Ok(process) => {
                process.visible.store(true, Ordering::SeqCst);
                *process.asked.lock().unwrap() = Some(Instant::now());
                let _ = process.send(&bridge::control("show", Value::Null));
            }
            Err(e) => eprintln!("heyo: {e}"),
        }
    }

    pub(crate) fn hide(&self) {
        // A window that is not running is hidden already.
        if let Some(process) = self.running() {
            process.visible.store(false, Ordering::SeqCst);
            let _ = process.send(&bridge::control("hide", Value::Null));
        }
    }

    pub(crate) fn is_visible(&self) -> bool {
        self.running().is_some_and(|process| process.visible.load(Ordering::SeqCst))
    }

    /// Gives the window the keyboard (see `platform::focus::focus` for what
    /// that takes; the process does its part of it).
    pub(crate) fn focus(&self) {
        self.tell("focus", Value::Null);
        // On Wayland an application cannot bring itself to the front; the
        // Shell extension does it.
        #[cfg(target_os = "linux")]
        if let Some(process) = self.running() {
            let title = process.title.clone();
            std::thread::spawn(move || crate::platform::desktop::desktop().activate_own(&title));
        }
    }

    /// Centres the window on the monitor the pointer is on.
    pub(crate) fn center(&self) {
        self.tell("center", Value::Null);
    }

    /// Moves and sizes the window: physical pixels, or logical ones.
    pub(crate) fn set_frame(&self, x: f64, y: f64, width: f64, height: f64, logical: bool) {
        self.tell("frame", json!({ "x": x, "y": y, "width": width, "height": height, "logical": logical }));
    }

    pub(crate) fn set_title(&self, title: &str) {
        self.tell("title", json!({ "title": title }));
    }

    /// Covers a rectangle (logical pixels) above everything, with the
    /// keyboard: the screenshot overlay. `backdrop` says whether the
    /// capture is already shown underneath, by a window of the host's.
    pub(crate) fn overlay(&self, x: i32, y: i32, width: i32, height: i32, backdrop: bool) {
        match self.process() {
            Ok(process) => {
                process.visible.store(true, Ordering::SeqCst);
                let _ = process.send(&bridge::control("overlay", json!({ "x": x, "y": y, "width": width, "height": height, "backdrop": backdrop })));
                watch(process);
            }
            Err(e) => eprintln!("heyo: {e}"),
        }
    }

    /// An event for this window alone.
    pub(crate) fn emit(&self, event: &str, payload: &impl serde::Serialize) -> Result<(), String> {
        let payload = serde_json::to_string(payload).map_err(|e| e.to_string())?;
        // A kept window hears it once it is up; one that was closed does not come back for it.
        let process = if kept(&self.label) { self.process()? } else { self.running().ok_or_else(|| format!("the window {} is not open", self.label))? };
        process.send_line(&bridge::event(event, &payload))
    }

    /// Closes the window for good: its process ends.
    pub(crate) fn close(&self) {
        if let Some(process) = self.running() {
            process.visible.store(false, Ordering::SeqCst);
            let _ = process.send(&bridge::control("close", Value::Null));
            // Closing its stdin ends it, should it not have listened.
            process.helper.lock().unwrap().take();
        }
    }
}

/// A safety net for the overlay, which holds the keyboard by a grab (X11):
/// if its process were to hang with the overlay up, the keyboard would be
/// its alone. So while the overlay is up the process is asked to answer
/// every second, and ended when it has not for a few: the server then
/// releases the grab. (`platform::overlay::watch` is the same net for the
/// web view, where the process that would hang is Heyo itself. Should Heyo
/// die, `heyo-ui` ends with it: its stdin closes.)
fn watch(process: Arc<Process>) {
    static WATCHING: AtomicBool = AtomicBool::new(false);
    if WATCHING.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(move || {
        let (mut asked, mut silent) = (process.pongs.load(Ordering::SeqCst), 0);
        while process.visible.load(Ordering::SeqCst) {
            asked += 1;
            if process.send(&json!({ "ping": asked })).is_err() {
                break;
            }
            std::thread::sleep(Duration::from_secs(1));
            silent = if process.pongs.load(Ordering::SeqCst) < asked { silent + 1 } else { 0 };
            if silent >= 5 && process.visible.load(Ordering::SeqCst) {
                eprintln!("heyo: the screenshot overlay is not responding while it holds the keyboard; ending it to release the keyboard");
                if let Some(helper) = &*process.helper.lock().unwrap() {
                    helper.kill();
                }
                break;
            }
        }
        WATCHING.store(false, Ordering::SeqCst);
    });
}

/// Whether the window of that label is passed an event emitted to every
/// window. All of them are, but for what the runtime says: that is whole
/// views, many times a second, which only the palette draws, and no other
/// process should have to read them to throw them away.
fn hears(label: &str, event: &str) -> bool {
    event != crate::wire::event::RUNTIME || label == "main"
}

/// Passes an event emitted to every window on to every process.
pub(crate) fn broadcast(app: &AppHandle, event: &str, payload: &str) {
    let processes: Vec<_> = app.state::<Shell>().flutter.running.lock().unwrap().values().cloned().collect();
    let line = bridge::event(event, payload);
    for process in processes.iter().filter(|process| hears(&process.label, event)) {
        let _ = process.send_line(&line);
    }
}

/// Whether any window of Heyo's has the keyboard.
pub(crate) fn any_focused(app: &AppHandle) -> bool {
    app.state::<Shell>().flutter.running.lock().unwrap().values().any(|process| process.visible.load(Ordering::SeqCst) && process.focused.load(Ordering::SeqCst))
}

/// Starts what must be ready before it is asked for: the palette.
pub(crate) fn start_kept(app: &AppHandle) -> Result<(), String> {
    start(app, "main", Value::Null).map(|_| ())
}

/// Opens the settings window, or brings it back if it is open.
pub(crate) fn open_settings(app: &AppHandle) -> Result<(), String> {
    let handle = Handle { app: app.clone(), label: crate::commands::settings::SETTINGS_WINDOW.to_owned() };
    if handle.running().is_none() {
        start(app, &handle.label, Value::Null)?;
        handle.center();
    }
    handle.show();
    handle.focus();
    Ok(())
}

/// Shows a capture kept on screen: `path` is its picture, the rectangle
/// (logical pixels) where it was taken.
pub(crate) fn open_pin(app: &AppHandle, n: u32, path: &str, x: f64, y: f64, width: f64, height: f64) -> Result<(), String> {
    let handle = Handle { app: app.clone(), label: format!("pin-{n}") };
    start(app, &handle.label, json!({ "path": path }))?;
    handle.set_frame(x, y, width, height, true);
    handle.show();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_label_names_the_kind_of_window_the_web_view_would_show() {
        assert_eq!(kind("main"), Some("main"));
        assert_eq!(kind("settings"), Some("settings"));
        assert_eq!(kind("dictation"), Some("dictation"));
        assert_eq!(kind("screenshot"), Some("screenshot"));
        assert_eq!(kind("pin-12"), Some("pin"));
        for stranger in ["", "Main", "pin-", "pin-x", "pin-3-settings", "other"] {
            assert_eq!(kind(stranger), None, "{stranger}");
        }
        assert!(kept("main") && kept("dictation") && kept("screenshot"));
        assert!(!kept("settings") && !kept("pin-1"));
    }

    #[test]
    fn what_every_window_is_told_reaches_every_process_but_the_runtime_s_views() {
        use crate::wire::event;
        for label in ["main", "settings", "dictation", "screenshot", "pin-2"] {
            assert!(hears(label, event::SETTINGS_CHANGED), "{label}");
            assert!(hears(label, event::INDEX_READY), "{label}");
            assert_eq!(hears(label, event::RUNTIME), label == "main", "{label}");
        }
    }

    #[test]
    fn only_an_interface_built_with_this_heyo_may_draw_it() {
        let binary = std::path::Path::new("/usr/lib/Heyo/ui/heyo-ui");
        assert_eq!(built_for("heyo-ui 0.2.0\n"), Some("0.2.0"));
        assert_eq!(built_for("Usage: something else"), None);
        assert_eq!(refusal(binary, Some("heyo-ui 0.2.0\n"), "0.2.0"), None);
        let stale = refusal(binary, Some("heyo-ui 0.1.0\n"), "0.2.0").unwrap();
        assert!(stale.contains("This is Heyo 0.2.0") && stale.contains("/usr/lib/Heyo/ui/heyo-ui is from Heyo 0.1.0"), "{stale}");
        assert!(stale.contains("scripts/build-ui.sh"));
        assert!(refusal(binary, Some("heyo-ui 0.2.0-beta.1"), "0.2.0").is_some());
        assert!(refusal(binary, Some(""), "0.2.0").unwrap().contains("is not Heyo's Flutter interface"));
        assert!(refusal(binary, None, "0.2.0").unwrap().contains("cannot run"));
    }
}

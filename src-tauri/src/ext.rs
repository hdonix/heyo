//! What commands ask of the host: the `ext.*` requests behind `@heyo/api`
//! (docs/extension-api.md). Heyo's own commands ask this way too, as the
//! extension `@heyo`, which needs no permission.
//!
//! A request says which extension it comes from. Before it is answered the
//! extension's manifest and the user's grants are consulted
//! (`permissions.rs`), which may put a question up in the palette; so each
//! request is answered on a thread of its own, and the runtime's other
//! messages go on being read meanwhile. Failures carry a code.
//!
//! Some requests subscribe: the answer is a number, and events with that
//! number follow until the subscription ends by itself (a recording is
//! over), is ended by the runtime (`ext.unsubscribe`, which the runtime
//! also sends for a command that closes), or the runtime's process is gone.
//!
//! What a request is answered with is decided in `answer`, which knows
//! nothing of the application. Here is the rest: the running Heyo as
//! `answer` sees it (its tables gathered, and what only it can do), and
//! what is done once an answer is known.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

use crate::activities;
use crate::clipboard::{self, ClipboardHistory, Kind};
use crate::confirm::Consent;
use crate::dictation::{self, Heard, NotListening};
use crate::palette;
use crate::permissions;
use crate::platform::capture::{Area, Shot};
use crate::platform::windows::{self, Rect};
use crate::platform;
use crate::screenshot::{self, Picked};
use crate::wire::{Capture, CaptureTarget, CaptureWhole, ErrorCode, ExtError, HostMessage};

mod answer;

use answer::{Answered, Effect, Running, Subscriptions, World};

/// Answers a request of an extension's, from the runtime process that is
/// (`background`) or is not the one for the menu bar and the background.
pub(crate) fn handle(app: &AppHandle, message: Value, background: bool) {
    let id = message["id"].clone();
    let app = app.clone();
    // The answer may wait for the user, or for a recording to start.
    std::thread::spawn(move || {
        let Answered { answer, effects } = answer::answer(&world(&app, &Here(&app)), message, background);
        perform(&app, effects);
        crate::runtime::reply_to(&app, background, &crate::runtime::reply(id, answer));
    });
}

/// Heyo's tables, as requests are answered from them.
fn world<'a>(app: &'a AppHandle, running: &'a Here<'a>) -> World<'a> {
    World {
        index: &app.state::<crate::IndexState>().inner().0,
        settings: &app.state::<crate::SettingsState>().inner().0,
        activities: app.state::<activities::Activities>().inner().table(),
        subscriptions: &SUBSCRIPTIONS,
        granted: &permissions::GRANTED,
        history: app.state::<ClipboardHistory>().inner(),
        files: app.state::<std::sync::Arc<crate::files::Files>>().inner(),
        last_window: palette::came_from(app),
        running,
    }
}

/// The running Heyo.
struct Here<'a>(&'a AppHandle);

impl Running for Here<'_> {
    fn ask(&self, consent: Consent) -> bool {
        permissions::ask(self.0, consent)
    }

    fn monitors(&self) -> Result<Vec<Rect>, String> {
        crate::monitors(self.0)
    }

    fn hide_palette(&self) {
        let _ = crate::main_window(self.0).hide();
    }

    fn capture(&self, extension: &str, target: CaptureTarget) -> Result<Capture, ExtError> {
        capture(self.0, extension, target)
    }

    fn recognize_text(&self, image: &Path) -> Result<String, String> {
        crate::ocr::recognize(self.0, image)
    }

    fn listen(&self, subscription: u64, language: Option<String>, indicator: bool) -> Result<u64, ExtError> {
        listen(self.0, subscription, language, indicator)
    }

    fn meant(&self, query: &str, kind: Option<Kind>) -> Option<clipboard::store::Found> {
        clipboard::meant(self.0, query, kind)
    }

    fn copy(&self, entry: &clipboard::Entry) -> Result<(), String> {
        clipboard::copy_entry(self.0, entry)
    }

    fn paste(&self, text: String, html: Option<String>) -> Result<(), ExtError> {
        paste_and_restore(self.0, text, html)
    }
}

/// Does what an answer left to be done.
fn perform(app: &AppHandle, effects: Vec<Effect>) {
    for effect in effects {
        match effect {
            Effect::Notify { title, body } => crate::notify(&title, &body),
            Effect::Activities(effects) => activities::carry_out(app, effects),
            Effect::StopListening(recording) => dictation::stop_listening(app, recording),
            Effect::CancelListening(recording) => dictation::cancel_listening(app, recording),
        }
    }
}

// --- The screen ------------------------------------------------------------------

static CAPTURES: AtomicU32 = AtomicU32::new(0);

/// Where an extension's next capture goes: its cache directory, which the
/// runtime clears of captures when the command that asked has ended.
fn capture_path(extension: &str) -> PathBuf {
    let n = CAPTURES.fetch_add(1, Ordering::Relaxed);
    crate::paths::cache().join(extension).join("captures").join(format!("capture-{}-{n}.png", std::process::id()))
}

/// What of a rectangle is on the screen (all the monitors together).
fn on_screen(rect: Rect, monitors: &[Rect]) -> Option<Rect> {
    let left = monitors.iter().map(|m| m.x).min()?.max(rect.x);
    let top = monitors.iter().map(|m| m.y).min()?.max(rect.y);
    let right = monitors.iter().map(|m| m.x + m.width).max()?.min(rect.x.saturating_add(rect.width));
    let bottom = monitors.iter().map(|m| m.y + m.height).max()?.min(rect.y.saturating_add(rect.height));
    (right > left && bottom > top).then(|| Rect { x: left, y: top, width: right - left, height: bottom - top })
}

/// Writes a capture as a PNG only the user can read: it shows their screen.
fn save_png(shot: &Shot, path: &Path) -> Result<(), String> {
    let mut rgba = shot.bgrx.clone();
    for pixel in rgba.chunks_exact_mut(4) {
        pixel.swap(0, 2);
        pixel[3] = 255;
    }
    let image = image::RgbaImage::from_raw(shot.width, shot.height, rgba).ok_or("the capture is not the size it says")?;
    std::fs::create_dir_all(path.parent().ok_or("the capture has no directory")?).map_err(|e| e.to_string())?;
    image.save(path).map_err(|e| format!("failed to save {}: {e}", path.display()))?;
    private(path);
    Ok(())
}

#[cfg_attr(not(unix), allow(unused_variables))]
fn private(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
}

fn capture(app: &AppHandle, extension: &str, target: CaptureTarget) -> Result<Capture, ExtError> {
    let path = capture_path(extension);
    let shown = path.to_string_lossy().into_owned();
    let rect = match target {
        CaptureTarget::Whole(CaptureWhole::Selection) => {
            return match palette::away_during(app, || screenshot::pick(app, path.clone())) {
                Picked::Saved { width, height } => {
                    private(&path);
                    Ok(Capture { path: shown, width, height })
                }
                Picked::Cancelled => Err(ExtError::new(ErrorCode::Cancelled, "No area was selected.")),
                Picked::Failed(e) => Err(e.into()),
            };
        }
        CaptureTarget::Whole(CaptureWhole::Screen) => None,
        CaptureTarget::Window { window } => {
            let found = windows::list(None)?.into_iter().find(|w| w.id == window).ok_or_else(|| ExtError::invalid(format!("There is no window {window}.")))?;
            Some(found.bounds.ok_or_else(|| ExtError::unsupported("This desktop does not say where its windows are."))?)
        }
        CaptureTarget::Rect { rect } => Some(rect),
    };
    let area = match rect {
        None => screenshot::current_monitor(app)?,
        Some(rect) => {
            let rect = on_screen(rect, &crate::monitors(app)?).ok_or_else(|| ExtError::invalid("None of that is on the screen."))?;
            Area { x: rect.x, y: rect.y, width: rect.width as u32, height: rect.height as u32, scale: 1.0 }
        }
    };
    let shot = palette::away_during(app, || platform::capture::grab(area))?;
    save_png(&shot, &path)?;
    Ok(Capture { path: shown, width: shot.width, height: shot.height })
}

// --- Dictation -------------------------------------------------------------------

/// What the one listening is sent for what was heard.
fn heard_event(heard: &Heard) -> Value {
    match heard {
        Heard::Segment(text) => json!({ "kind": "segment", "text": text }),
        Heard::Result(text) => json!({ "kind": "result", "text": text }),
        Heard::Cancelled => json!({ "kind": "error", "code": "cancelled", "message": "The recording was discarded." }),
        Heard::Failed(message) => json!({ "kind": "error", "code": "failed", "message": message }),
    }
}

fn not_listening(why: NotListening) -> ExtError {
    match why {
        NotListening::Busy => ExtError::from("A recording is already in progress."),
        NotListening::NotInstalled => ExtError::new(ErrorCode::Unavailable, "Dictation is not set up yet. Download it in Heyo Settings → Dictation."),
        NotListening::Failed(e) => e.into(),
    }
}

/// Starts a recording for an extension, whose events go to `subscription`.
fn listen(app: &AppHandle, subscription: u64, language: Option<String>, indicator: bool) -> Result<u64, ExtError> {
    let hearer = app.clone();
    let started = dictation::listen(app, language, indicator, move |heard| {
        emit(&hearer, subscription, heard_event(&heard));
        if matches!(heard, Heard::Result(_) | Heard::Cancelled | Heard::Failed(_)) {
            SUBSCRIPTIONS.lock().unwrap().ended(subscription);
        }
    });
    started.map_err(not_listening)
}

// --- Subscriptions ---------------------------------------------------------------

static SUBSCRIPTIONS: Mutex<Subscriptions> = Mutex::new(Subscriptions::new());

/// Sends an event to the process that subscribed, if it still wants them.
pub(crate) fn emit(app: &AppHandle, subscription: u64, data: Value) {
    let listener = SUBSCRIPTIONS.lock().unwrap().listener(subscription);
    if let Some(background) = listener {
        crate::runtime::reply_to(app, background, &HostMessage::Event { subscription, data }.json());
    }
}

/// A runtime process ended: what its commands subscribed to stops.
pub(crate) fn runtime_exited(app: &AppHandle, background: bool) {
    let effects = answer::runtime_exited(&world(app, &Here(app)), background);
    perform(app, effects);
}

// --- The clipboard ---------------------------------------------------------------

/// Pastes text into the window in front (the palette has hidden) and puts
/// back the text that was on the clipboard (`pasting`).
fn paste_and_restore(app: &AppHandle, text: String, html: Option<String>) -> Result<(), ExtError> {
    Ok(crate::pasting::paste_and_give_back(app, &text, html.as_deref())?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: i32, y: i32, width: i32, height: i32) -> Rect {
        Rect { x, y, width, height }
    }

    const LEFT: Rect = Rect { x: 0, y: 0, width: 1920, height: 1080 };
    const RIGHT: Rect = Rect { x: 1920, y: 0, width: 2560, height: 1440 };

    #[test]
    fn only_what_is_on_the_screen_is_captured() {
        let monitors = [LEFT, RIGHT];
        assert_eq!(on_screen(rect(100, 100, 800, 600), &monitors), Some(rect(100, 100, 800, 600)));
        // Across two monitors: all of it.
        assert_eq!(on_screen(rect(1800, 100, 400, 300), &monitors), Some(rect(1800, 100, 400, 300)));
        // Hanging off the edges: what is left.
        assert_eq!(on_screen(rect(-50, -20, 200, 100), &monitors), Some(rect(0, 0, 150, 80)));
        assert_eq!(on_screen(rect(4400, 1400, 500, 500), &monitors), Some(rect(4400, 1400, 80, 40)));
        // Off the screen, of no size, or with no screen known: nothing.
        assert_eq!(on_screen(rect(9000, 0, 100, 100), &monitors), None);
        assert_eq!(on_screen(rect(10, 10, 0, 50), &monitors), None);
        assert_eq!(on_screen(rect(10, 10, -5, 50), &monitors), None);
        assert_eq!(on_screen(rect(0, 0, 10, 10), &[]), None);
        assert_eq!(on_screen(rect(i32::MAX - 5, 0, 100, 10), &monitors), None);
    }

    #[test]
    fn a_capture_is_a_png_in_the_extensions_cache_only_the_user_can_read() {
        let first = capture_path("tiles");
        assert!(first.starts_with(crate::paths::cache().join("tiles").join("captures")));
        assert_ne!(first, capture_path("tiles"));

        // Two pixels as the screen is read: blue, green, red and a byte unused.
        let shot = Shot { width: 2, height: 1, bgrx: vec![255, 0, 0, 0, 10, 20, 30, 0] };
        let dir = std::env::temp_dir().join(format!("heyo-ext-test-{}", std::process::id()));
        let path = dir.join("captures").join("a.png");
        save_png(&shot, &path).unwrap();
        let read = image::open(&path).unwrap().into_rgba8();
        assert_eq!(read.dimensions(), (2, 1));
        assert_eq!((read.get_pixel(0, 0).0, read.get_pixel(1, 0).0), ([0, 0, 255, 255], [30, 20, 10, 255]));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        assert!(save_png(&Shot { width: 5, height: 5, bgrx: vec![0; 8] }, &dir.join("b.png")).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn what_was_heard_is_sent_as_events() {
        assert_eq!(heard_event(&Heard::Segment("Hello.".into())), json!({ "kind": "segment", "text": "Hello." }));
        assert_eq!(heard_event(&Heard::Result("Hello. World.".into())), json!({ "kind": "result", "text": "Hello. World." }));
        assert_eq!(heard_event(&Heard::Cancelled)["code"], "cancelled");
        assert_eq!(heard_event(&Heard::Failed("no microphone".into())), json!({ "kind": "error", "code": "failed", "message": "no microphone" }));
        // One recording at a time; an engine that is not there can be installed.
        assert_eq!(not_listening(NotListening::Busy).code, ErrorCode::Failed);
        assert_eq!(not_listening(NotListening::NotInstalled).code, ErrorCode::Unavailable);
        assert_eq!(not_listening(NotListening::Failed("no node".into())).message, "no node");
    }
}

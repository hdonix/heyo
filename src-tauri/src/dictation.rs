//! Voice dictation: speak, and the text is pasted into the focused window.
//!
//! The work happens in a Node helper (`runtime/src/dictation.ts`) that
//! records the microphone, recognizes speech locally and optionally polishes
//! it with Claude. This module starts and stops it (`heyo --dictate`
//! toggles), shows the progress window, and pastes the results: sentence by
//! sentence when the text is used as recognized, or once at the end when it
//! is polished first.

use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

use crate::helper::{self, Helper};
use crate::SettingsState;

pub const WINDOW: &str = "dictation";
const EVENT: &str = crate::wire::event::DICTATION;
/// Logical size of the dictation window.
const WIDTH: f64 = 420.0;
const HEIGHT: f64 = 52.0;

#[derive(Default)]
pub struct Dictation(Mutex<State>);

#[derive(Default)]
struct State {
    helper: Option<Helper>,
    recording: bool,
    /// The hotkey that started the recording has not been released yet.
    holding: bool,
    /// Counts recordings, so a hold watcher can tell its own has ended.
    session: u64,
    /// Whether the transcript is polished (and so pasted only at the end).
    polishing: bool,
    /// Clipboard text from before dictation started, restored afterwards.
    saved_clipboard: Option<String>,
    installing: bool,
    /// Who is given the text of this recording instead of the focused
    /// window: an extension that asked to listen.
    listener: Option<Listener>,
}

/// What an extension hears of the recording it started.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Heard {
    /// A sentence, as soon as it is recognized.
    Segment(String),
    /// All of it; the recording is over.
    Result(String),
    Cancelled,
    Failed(String),
}

impl Heard {
    fn ends(&self) -> bool {
        !matches!(self, Heard::Segment(_))
    }
}

type Listener = std::sync::Arc<dyn Fn(Heard) + Send + Sync>;

/// Why listening could not start.
#[derive(Debug, PartialEq)]
pub(crate) enum NotListening {
    /// A recording is in progress, and there is one microphone.
    Busy,
    /// The speech engine has not been downloaded.
    NotInstalled,
    Failed(String),
}

/// What an event from the helper tells the one listening, if anything.
fn heard(event: &Value) -> Option<Heard> {
    let text = || event["text"].as_str().unwrap_or_default().to_owned();
    match event["type"].as_str().unwrap_or_default() {
        "segment" => Some(Heard::Segment(text())),
        "result" if event["cancelled"] == true => Some(Heard::Cancelled),
        "result" => Some(Heard::Result(text())),
        "error" => Some(Heard::Failed(event["message"].as_str().unwrap_or("Dictation failed").to_owned())),
        _ => None,
    }
}

pub fn dir() -> PathBuf {
    crate::paths::data().join("dictation")
}

/// Whether the speech engine and models have been downloaded.
pub fn installed() -> bool {
    let dir = dir();
    ["node_modules/sherpa-onnx-node", "models/sense-voice/model.int8.onnx", "models/sense-voice/tokens.txt", "models/silero_vad.onnx"]
        .iter()
        .all(|file| dir.join(file).exists())
}

fn node_command(app: &AppHandle, args: &[&str]) -> Result<Command, String> {
    let mut command = helper::node(app, "heyo-dictation.cjs", "HEYO_DICTATION")?;
    command.args(args).env("HEYO_DICTATION_DIR", dir());
    Ok(command)
}

fn emit(app: &AppHandle, event: Value) {
    if let Some(window) = crate::shell::window(app, WINDOW) {
        let _ = window.emit(EVENT, event);
    }
}

/// Shows the progress window at the bottom center of the screen, without
/// taking keyboard focus from the application being dictated into.
fn show_window(app: &AppHandle) {
    let Some(window) = crate::shell::window(app, WINDOW) else { return };
    // The size is set here rather than read back: before the first show the
    // window reports a size of zero.
    if let Ok(Some(monitor)) = app.primary_monitor() {
        let scale = monitor.scale_factor();
        let (width, height) = ((WIDTH * scale) as i32, (HEIGHT * scale) as i32);
        let area = monitor.size();
        let origin = monitor.position();
        let x = origin.x + (area.width as i32 - width) / 2;
        let y = origin.y + area.height as i32 - height - area.height as i32 / 10;
        window.set_frame(x, y, width, height);
    }
    let _ = window.show();
}

fn hide_window_after(app: &AppHandle, delay: Duration) {
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(delay);
        // A new recording may have started meanwhile.
        if !app.state::<Dictation>().0.lock().unwrap().recording {
            if let Some(window) = crate::shell::window(&app, WINDOW) {
                let _ = window.hide();
            }
        }
    });
}

fn start_helper(app: &AppHandle) -> Result<Helper, String> {
    let (receiver, ended) = (app.clone(), app.clone());
    Helper::start(
        node_command(app, &[])?,
        move |event| handle(&receiver, event),
        move || {
            // The helper exits when idle; the next recording starts a new one.
            let state = ended.state::<Dictation>();
            let mut state = state.0.lock().unwrap();
            state.helper = None;
            if std::mem::take(&mut state.recording) {
                let listener = state.listener.take();
                drop(state);
                if let Some(listener) = listener {
                    listener(Heard::Failed("Dictation stopped unexpectedly".into()));
                }
                emit(&ended, json!({ "type": "error", "message": "Dictation stopped unexpectedly" }));
                hide_window_after(&ended, Duration::from_secs(3));
            }
        },
    )
    .map_err(|e| format!("failed to start dictation: {e}"))
}

fn send(state: &mut State, message: &Value) -> Result<(), String> {
    let helper = state.helper.as_mut().ok_or("dictation is not running")?;
    helper.send(message).map_err(|e| {
        state.helper = None;
        format!("dictation is not running: {e}")
    })
}

/// Pastes into the focused window through the clipboard.
fn paste(app: &AppHandle, text: &str) {
    {
        let state = app.state::<Dictation>();
        let mut state = state.0.lock().unwrap();
        if state.saved_clipboard.is_none() {
            state.saved_clipboard = Some(crate::platform::clipboard_watch::read_text(app).unwrap_or_default());
        }
    }
    // Plumbing, not something the user copied.
    crate::clipboard::skip_next();
    if let Err(e) = crate::platform::clipboard_watch::write_text(app, text) {
        eprintln!("heyo: dictation could not copy text: {e}");
        return;
    }
    crate::paste_keystroke(Duration::from_millis(60));
}

/// Puts back what was on the clipboard before dictation used it.
fn restore_clipboard(app: &AppHandle) {
    let saved = app.state::<Dictation>().0.lock().unwrap().saved_clipboard.take();
    let Some(saved) = saved.filter(|s| !s.is_empty()) else { return };
    let app = app.clone();
    std::thread::spawn(move || {
        // Let the last paste read the clipboard first.
        std::thread::sleep(Duration::from_millis(500));
        // What comes back may be a secret the history skipped the first time.
        crate::clipboard::skip_next();
        let _ = crate::platform::clipboard_watch::write_text(&app, &saved);
    });
}

/// What an event from the helper calls for.
#[derive(Debug, PartialEq)]
enum Effect {
    Paste(String),
    /// The recording is over: the clipboard goes back, and the window
    /// leaves after the time given.
    Over(Duration),
}

/// Reads an event from the helper. Text used as recognized goes in as each
/// sentence is finished; polished text only once, at the end.
fn effects(polishing: bool, event: &Value) -> Vec<Effect> {
    let text = event["text"].as_str().unwrap_or_default();
    match event["type"].as_str().unwrap_or_default() {
        "segment" if !polishing => vec![Effect::Paste(text.to_owned())],
        "result" => {
            // Polished text, or recognized text when polishing failed.
            let paste = (polishing && !text.is_empty()).then(|| Effect::Paste(text.to_owned()));
            // A warning, or having heard nothing, stays up to be read.
            let lingers = event["warning"].is_string() || event["raw"].as_str().is_some_and(str::is_empty);
            paste.into_iter().chain([Effect::Over(Duration::from_millis(if lingers { 2500 } else { 300 }))]).collect()
        }
        "error" => vec![Effect::Over(Duration::from_secs(4))],
        _ => Vec::new(),
    }
}

/// Handles one event from the helper.
fn handle(app: &AppHandle, event: Value) {
    // A recording an extension started: the text is the extension's, and
    // nothing is pasted anywhere.
    let listener = app.state::<Dictation>().0.lock().unwrap().listener.clone();
    if let Some(listener) = listener {
        if let Some(heard) = heard(&event) {
            if heard.ends() {
                let state = app.state::<Dictation>();
                let mut state = state.0.lock().unwrap();
                state.recording = false;
                state.listener = None;
                drop(state);
                hide_window_after(app, Duration::from_millis(300));
            }
            listener(heard);
        }
        return emit(app, event);
    }
    let polishing = app.state::<Dictation>().0.lock().unwrap().polishing;
    for effect in effects(polishing, &event) {
        match effect {
            Effect::Paste(text) => paste(app, &text),
            Effect::Over(linger) => {
                app.state::<Dictation>().0.lock().unwrap().recording = false;
                restore_clipboard(app);
                hide_window_after(app, linger);
            }
        }
    }
    emit(app, event);
}

fn fail(app: &AppHandle, message: String) {
    eprintln!("heyo: {message}");
    show_window(app);
    emit(app, json!({ "type": "error", "message": message }));
    hide_window_after(app, Duration::from_secs(4));
}

/// Holding the hotkey longer than this makes it push-to-talk: releasing it
/// finishes. A shorter press leaves the recording running until the next one.
const HOLD: Duration = Duration::from_millis(450);

/// Waits for the keys that were down when recording started to be released.
fn watch_hold(app: AppHandle, keys: [u8; 32], session: u64) {
    std::thread::spawn(move || {
        let started = std::time::Instant::now();
        loop {
            std::thread::sleep(Duration::from_millis(30));
            let Some(now) = crate::windows::keys_down() else { break };
            if keys.iter().zip(now).any(|(was, is)| was & !is != 0) {
                break;
            }
        }
        let state = app.state::<Dictation>();
        let mut state = state.0.lock().unwrap();
        if state.session != session || !state.recording {
            return;
        }
        state.holding = false;
        drop(state);
        if started.elapsed() >= HOLD {
            finish(&app);
        }
    });
}

/// Starts dictating, or finishes if a recording is in progress.
pub fn toggle(app: &AppHandle) {
    let (recording, holding) = {
        let state = app.state::<Dictation>();
        let state = state.0.lock().unwrap();
        (state.recording, state.holding)
    };
    if !recording {
        begin(app, true);
    } else if !holding {
        // (While the hotkey is held, key repeat may invoke this again.)
        finish(app);
    }
}

/// Finishes the recording in progress: what was said is pasted.
pub fn finish(app: &AppHandle) {
    let state = app.state::<Dictation>();
    let mut state = state.0.lock().unwrap();
    if !state.recording {
        return;
    }
    let (polish, api_key, ai) = {
        let settings = app.state::<SettingsState>();
        let settings = settings.0.read().unwrap();
        (settings.dictation_polish.clone(), settings.anthropic_api_key.clone(), settings.ai())
    };
    // An extension is given what was said, as it was said.
    let polish = if state.listener.is_some() { "off".to_owned() } else { polish };
    if let Err(e) = send(&mut state, &json!({ "cmd": "stop", "polish": polish, "apiKey": api_key, "ai": ai })) {
        state.recording = false;
        let listener = state.listener.take();
        drop(state);
        if let Some(listener) = listener {
            listener(Heard::Failed(e.clone()));
        }
        fail(app, e);
    }
}

/// Starts dictating. With `watch_hotkey`, a hotkey held down past `HOLD`
/// finishes the recording when it is released; the push-to-talk key does
/// its own watching.
pub fn begin(app: &AppHandle, watch_hotkey: bool) {
    let state = app.state::<Dictation>();
    let mut state = state.0.lock().unwrap();
    if state.recording {
        return;
    }
    if !installed() {
        drop(state);
        return fail(app, "Dictation is not set up yet. Open Heyo Settings → Dictation to download it.".into());
    }
    let (language, source, polish, vocabulary) = {
        let settings = app.state::<SettingsState>();
        let settings = settings.0.read().unwrap();
        (settings.dictation_language.clone(), settings.dictation_source.clone(), settings.dictation_polish.clone(), settings.dictation_vocabulary.clone())
    };
    let started = (|| {
        if state.helper.is_none() {
            state.helper = Some(start_helper(app)?);
        }
        // The helper keeps the vocabulary for the whole recording, polishing included.
        send(&mut state, &json!({ "cmd": "start", "language": language, "source": source, "vocabulary": vocabulary }))
    })();
    match started {
        Ok(()) => {
            state.recording = true;
            state.polishing = polish != "off";
            state.session += 1;
            // The hotkey that started this is usually still down.
            let held = crate::windows::keys_down().filter(|keys| watch_hotkey && keys.iter().any(|b| *b != 0));
            state.holding = held.is_some();
            if let Some(keys) = held {
                watch_hold(app.clone(), keys, state.session);
            }
            drop(state);
            show_window(app);
            emit(app, json!({ "type": "start", "polish": polish }));
        }
        Err(e) => {
            drop(state);
            fail(app, e);
        }
    }
}

/// The languages the speech model takes.
pub(crate) const LANGUAGES: &[&str] = &["auto", "zh", "en", "ja", "ko", "yue"];

/// Starts a recording for an extension: `listener` is given each sentence
/// and, at the end, all of it; nothing is pasted. The recording ends like
/// any other (`finish`, the hotkey, the window's buttons) or is discarded
/// (`cancel_listening`). Returns the recording's number.
pub(crate) fn listen(app: &AppHandle, language: Option<String>, indicator: bool, listener: impl Fn(Heard) + Send + Sync + 'static) -> Result<u64, NotListening> {
    let state = app.state::<Dictation>();
    let mut state = state.0.lock().unwrap();
    if state.recording {
        return Err(NotListening::Busy);
    }
    if !installed() {
        return Err(NotListening::NotInstalled);
    }
    let (set_language, source, vocabulary) = {
        let settings = app.state::<SettingsState>();
        let settings = settings.0.read().unwrap();
        (settings.dictation_language.clone(), settings.dictation_source.clone(), settings.dictation_vocabulary.clone())
    };
    let language = language.unwrap_or(set_language);
    let started = (|| {
        if state.helper.is_none() {
            state.helper = Some(start_helper(app)?);
        }
        send(&mut state, &json!({ "cmd": "start", "language": language, "source": source, "vocabulary": vocabulary }))
    })();
    started.map_err(NotListening::Failed)?;
    state.recording = true;
    state.polishing = false;
    state.holding = false;
    state.session += 1;
    state.listener = Some(std::sync::Arc::new(listener));
    let session = state.session;
    drop(state);
    if indicator {
        show_window(app);
        emit(app, json!({ "type": "start", "polish": "off" }));
    }
    Ok(session)
}

/// Whether that recording is still going, for the extension that started it.
fn listening(state: &State, session: u64) -> bool {
    state.recording && state.session == session && state.listener.is_some()
}

/// Ends the recording an extension started, if it is still the one going:
/// what was said comes to its listener.
pub(crate) fn stop_listening(app: &AppHandle, session: u64) {
    if listening(&app.state::<Dictation>().0.lock().unwrap(), session) {
        finish(app);
    }
}

/// Discards the recording an extension started, if it is still the one going.
pub(crate) fn cancel_listening(app: &AppHandle, session: u64) {
    if listening(&app.state::<Dictation>().0.lock().unwrap(), session) {
        cancel(app);
    }
}

/// Dictates while the hold-to-talk key from the settings is held.
pub fn watch_hold_key(app: &AppHandle) {
    use crate::platform::keys::{watch_hold, Hold};
    let app = app.clone();
    std::thread::spawn(move || {
        let setting = app.clone();
        let key = move || setting.state::<SettingsState>().0.read().unwrap().dictation_hold_key.clone();
        let result = watch_hold(key, |hold| match hold {
            Hold::Started => begin(&app, false),
            Hold::Released => finish(&app),
            Hold::Cancelled => cancel(&app),
        });
        if let Err(e) = result {
            eprintln!("heyo: {e}");
        }
    });
}

/// Discards the recording in progress.
pub fn cancel(app: &AppHandle) {
    let state = app.state::<Dictation>();
    let mut state = state.0.lock().unwrap();
    if state.recording {
        let _ = send(&mut state, &json!({ "cmd": "cancel" }));
    }
}

#[derive(Serialize)]
pub struct Microphone {
    name: String,
    description: String,
}

/// Input devices known to PulseAudio/PipeWire, without output monitors.
pub fn microphones() -> Vec<Microphone> {
    let Ok(output) = Command::new("pactl").args(["-f", "json", "list", "sources"]).output() else { return Vec::new() };
    let Ok(Value::Array(sources)) = serde_json::from_slice(&output.stdout) else { return Vec::new() };
    sources
        .iter()
        .filter_map(|source| {
            let name = source["name"].as_str()?;
            let monitor = name.ends_with(".monitor") || source["monitor_source"].as_str().is_some_and(|m| !m.is_empty());
            (!monitor).then(|| Microphone {
                name: name.to_owned(),
                description: source["description"].as_str().unwrap_or(name).to_owned(),
            })
        })
        .collect()
}

#[derive(Serialize)]
pub struct Status {
    installed: bool,
    installing: bool,
    microphones: Vec<Microphone>,
}

/// For `shell::fixtures`.
#[cfg(test)]
pub(crate) fn fixture() -> Status {
    Status { installed: true, installing: false, microphones: vec![Microphone { name: "alsa_input.usb".into(), description: "USB Microphone".into() }] }
}

pub fn status(app: &AppHandle) -> Status {
    Status {
        installed: installed(),
        installing: app.state::<Dictation>().0.lock().unwrap().installing,
        microphones: microphones(),
    }
}

/// Downloads the speech engine and models, reporting progress to the
/// settings window as `heyo://dictation-install` events.
pub fn install(app: &AppHandle) -> Result<(), String> {
    if std::mem::replace(&mut app.state::<Dictation>().0.lock().unwrap().installing, true) {
        return Ok(());
    }
    let window = app.clone();
    helper::install(node_command(app, &["install"]), move |message| {
        if helper::concludes(&message) {
            window.state::<Dictation>().0.lock().unwrap().installing = false;
        }
        let _ = window.emit(crate::wire::event::DICTATION_INSTALL, message);
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOON: Effect = Effect::Over(Duration::from_millis(300));
    const LATER: Effect = Effect::Over(Duration::from_millis(2500));

    fn paste(text: &str) -> Effect {
        Effect::Paste(text.into())
    }

    #[test]
    fn text_used_as_recognized_goes_in_sentence_by_sentence() {
        assert_eq!(effects(false, &json!({ "type": "segment", "text": "Hello." })), [paste("Hello.")]);
        // All of it is in already when the result comes.
        assert_eq!(effects(false, &json!({ "type": "result", "text": "Hello. World.", "raw": "Hello. World." })), [SOON]);
    }

    #[test]
    fn polished_text_goes_in_once_at_the_end() {
        assert_eq!(effects(true, &json!({ "type": "segment", "text": "um hello" })), []);
        assert_eq!(effects(true, &json!({ "type": "result", "text": "Hello.", "raw": "um hello" })), [paste("Hello."), SOON]);
    }

    #[test]
    fn what_needs_reading_stays_up() {
        // Polishing failed: the recognized text is used, with a warning.
        let warned = json!({ "type": "result", "text": "um hello", "raw": "um hello", "warning": "no API key" });
        assert_eq!(effects(true, &warned), [paste("um hello"), LATER]);
        // Nothing was heard.
        assert_eq!(effects(false, &json!({ "type": "result", "text": "", "raw": "" })), [LATER]);
        assert_eq!(effects(true, &json!({ "type": "result", "text": "", "raw": "" })), [LATER]);
        assert_eq!(effects(true, &json!({ "type": "error", "message": "no microphone" })), [Effect::Over(Duration::from_secs(4))]);
    }

    #[test]
    fn an_extension_listening_hears_sentences_and_then_all_of_it() {
        assert_eq!(heard(&json!({ "type": "segment", "text": "Hello." })), Some(Heard::Segment("Hello.".into())));
        assert_eq!(heard(&json!({ "type": "result", "text": "Hello. World.", "raw": "Hello. World.", "polished": false })), Some(Heard::Result("Hello. World.".into())));
        // Nothing heard is still an answer; a discarded recording is not text.
        assert_eq!(heard(&json!({ "type": "result", "text": "", "raw": "" })), Some(Heard::Result(String::new())));
        assert_eq!(heard(&json!({ "type": "result", "text": "", "raw": "", "cancelled": true })), Some(Heard::Cancelled));
        assert_eq!(heard(&json!({ "type": "error", "message": "no microphone" })), Some(Heard::Failed("no microphone".into())));
        // What only the window shows.
        for progress in [json!({ "type": "partial", "text": "Hel" }), json!({ "type": "level", "value": 0.4 }), json!({ "type": "ready" }), json!({ "type": "polishing" })] {
            assert_eq!(heard(&progress), None, "{progress}");
        }
        assert!(!Heard::Segment("a".into()).ends());
        assert!(Heard::Result("a".into()).ends() && Heard::Cancelled.ends() && Heard::Failed("a".into()).ends());
    }

    #[test]
    fn a_recording_is_its_starters_only_while_it_is_the_one_going() {
        let mut state = State { recording: true, session: 4, listener: Some(std::sync::Arc::new(|_| {})), ..State::default() };
        assert!(listening(&state, 4));
        // An earlier recording's stop must not end this one.
        assert!(!listening(&state, 3));
        // Nor may an extension end what the user is dictating.
        state.listener = None;
        assert!(!listening(&state, 4));
        state.recording = false;
        assert!(!listening(&state, 4));
    }

    #[test]
    fn progress_changes_nothing() {
        assert_eq!(effects(false, &json!({ "type": "partial", "text": "Hel" })), []);
        assert_eq!(effects(true, &json!({ "type": "level", "value": 0.4 })), []);
    }
}

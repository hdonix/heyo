//! Voice dictation: speak, and the text goes into the focused window.
//!
//! The work happens in a Node helper (`runtime/src/dictation.ts`) that
//! records the microphone, recognizes speech locally and optionally polishes
//! it with Claude. This module starts and stops it (`heyo --dictate`
//! toggles), shows the progress window, and writes what is heard into the
//! application the user is working in.
//!
//! The words reach the application one of two ways ([`Way`]), chosen when
//! the recording starts, before a word is heard:
//!
//! - composed there through the session's input method (`Composer`,
//!   which says what `composed` decides): the sentence in progress is on
//!   screen as text being composed, a finished one is committed. This is
//!   the way wherever the window in front takes an input method that Heyo
//!   speaks to;
//! - pasted (`Paster`, which does what `onscreen` decides): the words as
//!   they are spoken, corrected where they stand by erasing and pasting
//!   again, where the session and the window allow that, or else each
//!   sentence when it is finished, or the polished text once at the end.
//!
//! A recording whose words can no longer be composed (the user chose
//! another input method, no field has the focus when it ends) goes on by
//! pasting, from what the application holds by then. A thread per
//! recording (`write`) carries all of it out, so that a slow application
//! holds up neither the helper nor the window, and of the sentence in
//! progress only the newest state is ever written: what was heard
//! meanwhile is skipped.

mod composed;
mod onscreen;

use std::collections::VecDeque;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

use crate::helper::{self, Helper};
use crate::platform::composing::{self, Composing, Stir};
use crate::platform::typed::{self, Lost, Writing};
use crate::{pasting, SettingsState};
use composed::{Composed, Tell};
use onscreen::{OnScreen, Shortfall, Standing, Step};


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
    /// Where what is heard goes to be written into the application: the
    /// recording's own thread takes it from there.
    writing: Option<Arc<Mailbox>>,
    installing: bool,
    /// Who is given the text of this recording instead of the focused
    /// window: an extension that asked to listen.
    listener: Option<Listener>,
}

/// What an extension hears of the recording it started.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Heard {
    /// A sentence by itself, as soon as it is recognized.
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
        // Without the space that sets it apart from the sentence before.
        "segment" => Some(Heard::Segment(text().trim_start().to_owned())),
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

/// What the dictation window is told of a recording: the helper's account
/// of it, and the host's own of one starting or failing.
#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub(crate) enum Told {
    /// A recording begins; `polish` is the mode it will be polished in.
    Start { polish: String },
    /// The microphone is open.
    Ready,
    /// How loud it is, from 0 to 1; null when that could not be measured.
    Level { value: Option<f64> },
    /// The sentence in progress, as far as it is recognized. Like a
    /// finished one it begins with what sets it apart from the sentence
    /// before (a space, where the language writes one).
    Partial { text: String },
    /// A finished sentence: the text so far is these, one after the other.
    Segment { text: String },
    /// Whether the words are going into the application as they are spoken
    /// (`live`): then the window need not show them too. Said when that is
    /// decided, at the first words, and again if it stops being so.
    Writing { live: bool },
    Polishing,
    /// The recording is over: `text` is what is pasted, `raw` what was heard.
    Result {
        text: String,
        raw: String,
        /// Why the text was not polished.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        warning: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cancelled: Option<bool>,
        /// What a cancelled recording had written is still in the
        /// application: it could not be taken back.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        kept: Option<bool>,
    },
    Error { message: String },
}

/// Tells the pill of the recording: what `Told` declares and nothing else.
/// The pill is Slint's (`slint_windows::pill`); without it, as when Slint
/// could not start, the recording goes on all the same.
fn emit(_app: &AppHandle, event: Value) {
    match serde_json::from_value::<Told>(event) {
        Ok(told) => crate::slint_windows::pill::tell(told),
        Err(e) => eprintln!("heyo: dictation: not something its window is told: {e}"),
    }
}

/// Shows the pill at the bottom center of the screen, without taking
/// keyboard focus from the application being dictated into.
fn show_window(app: &AppHandle) {
    crate::slint_windows::pill::show(app);
}

fn hide_window_after(app: &AppHandle, delay: Duration) {
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(delay);
        // A new recording may have started meanwhile.
        if !app.state::<Dictation>().0.lock().unwrap().recording {
            crate::slint_windows::pill::hide();
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
            if !state.recording {
                return;
            }
            let stopped = json!({ "type": "error", "message": "Dictation stopped unexpectedly" });
            // What the recording wrote is finished off by its own thread.
            if let Some(writing) = state.writing.clone() {
                return writing.post(Said::Ended(stopped));
            }
            state.recording = false;
            let listener = state.listener.take();
            drop(state);
            if let Some(listener) = listener {
                listener(Heard::Failed("Dictation stopped unexpectedly".into()));
            }
            emit(&ended, stopped);
            hide_window_after(&ended, Duration::from_secs(3));
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

/// What the thread that writes is told, in the order it was heard.
enum Said {
    /// The sentence in progress.
    Partial(String),
    /// A finished sentence.
    Sentence(String),
    /// The helper's last word on the recording: its result, or an error.
    Ended(Value),
}

/// Between the helper, which says what it hears as it hears it, and the
/// thread that writes, which may take longer over a piece than the next
/// one takes to come.
#[derive(Default)]
struct Mailbox {
    waiting: Mutex<VecDeque<Said>>,
    arrived: Condvar,
    /// Presses of a mouse button on the dictation window, which whoever
    /// corrects the text holds against the presses it hears of.
    own_presses: Arc<AtomicU32>,
}

impl Mailbox {
    fn post(&self, said: Said) {
        let mut waiting = self.waiting.lock().unwrap();
        // Of the sentence in progress only the newest matters: a state of
        // it nobody got round to writing is dropped.
        if matches!(waiting.back(), Some(Said::Partial(_))) {
            waiting.pop_back();
        }
        waiting.push_back(said);
        self.arrived.notify_one();
    }

    fn next(&self) -> Said {
        let mut waiting = self.waiting.lock().unwrap();
        loop {
            match waiting.pop_front() {
                Some(said) => return said,
                None => waiting = self.arrived.wait(waiting).unwrap(),
            }
        }
    }
}

/// Window classes of applications where Backspace and the paste shortcut do
/// not simply erase and insert text at a cursor: editors with modes,
/// spreadsheets (a cell that is not being edited is replaced whole), and
/// windows onto another computer, which fetch the clipboard on their own
/// and paste it when they get to it.
const NOT_TEXT_FIELDS: &[&str] = &["gvim", "emacs", "libreoffice-calc", "gnumeric", "remmina", "vnc", "rdesktop", "freerdp", "virt-manager", "virt-viewer", "virtualbox", "vmware", "qemu", "rustdesk", "anydesk", "teamviewer", "nomachine"];

/// Why words are not written as they are spoken into a window of that
/// class: `None` means they are. It is not done where a snippet's keyword
/// is left alone either (`expand::refusal`: Heyo's own windows, password
/// managers, the applications the user excluded), nor in a terminal, where
/// a shell would cope with Backspaces but an editor running in it may not.
fn not_here(class: Option<&str>, own: bool, excluded: impl FnOnce(&str) -> bool) -> Option<&'static str> {
    if let Some(why) = crate::expand::refusal(class, own, excluded) {
        return Some(why);
    }
    let class = class?.to_lowercase();
    if crate::platform::input::is_terminal(&class) {
        Some("a terminal")
    } else if NOT_TEXT_FIELDS.iter().any(|name| class.contains(name)) {
        Some("an application where erasing and pasting do not edit text at a cursor")
    } else {
        None
    }
}

/// What is heard of a recording, as whoever writes it into the application
/// is told.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Spoken<'a> {
    /// The sentence in progress, as far as it is recognized.
    Partial(&'a str),
    /// A finished sentence, as it is written after those before it.
    Sentence(&'a str),
    /// The recording is over: `text` is what should stand, which is other
    /// than what was heard if it was `polished`.
    Result { text: &'a str, polished: bool },
    Cancelled,
    /// The recording broke off: no more will be heard.
    Failed,
}

/// A way for the words of one recording to reach the application.
trait Way: Send {
    /// Something was heard. An error means this way has come to its end
    /// without acting on that: the words go on another way, from what the
    /// application holds of the recording.
    fn hear(&mut self, spoken: Spoken) -> Result<(), Standing>;

    /// Whether the words are on screen in the application as they are
    /// spoken, once that is decided: what the dictation window need not
    /// show too.
    fn live(&self) -> Option<bool>;

    /// What could not be done at the end, once the recording is over.
    fn shortfall(&self) -> Option<Shortfall>;
}

/// How long a recording that ends waits for a field to have the focus
/// again before its last words are pasted instead. Finishing with a
/// hotkey takes the focus away for as long as the key is down.
const FIELD_BACK: Duration = Duration::from_millis(if cfg!(test) { 20 } else { 600 });

/// The words composed in the application through the input method: says
/// what `Composed` decides, for the recording and for what the user does
/// in the field meanwhile.
struct Composer {
    /// Shared with the input method's own thread, which hears of the field.
    composed: Arc<(Mutex<Composed>, Condvar)>,
    /// For as long as it lives, the session's input method is Heyo's.
    field: Box<dyn Composing>,
}

/// Says to the input method what `Composed` answered.
fn tell(field: &mut dyn Composing, tells: Vec<Tell>) -> Result<(), String> {
    for tell in tells {
        match tell {
            Tell::Show(text) => field.show(&text)?,
            Tell::Commit(text) => {
                // Like pasted text, committed text must not press Enter in
                // a terminal: there it goes in on one line.
                let terminal = text.contains(['\n', '\r']) && crate::platform::input::terminal_focused();
                field.commit(&pasting::unasked(text, terminal))?;
            }
        }
    }
    Ok(())
}

impl Composer {
    /// Begins composing in the window in front, or says why not. `live`:
    /// the sentence in progress is shown while it is spoken. `own_keys`:
    /// the keys that are pressed for Heyo, and are not typing.
    fn begin(live: bool, polishing: bool, own_keys: &[&str]) -> Result<Composer, String> {
        let composed = Arc::new((Mutex::new(Composed::new(live, polishing)), Condvar::new()));
        let heard = composed.clone();
        let field = composing::compose(
            own_keys,
            Box::new(move |stir, field| {
                let (composed, changed) = &*heard;
                let mut composed = composed.lock().unwrap();
                let tells = composed.on(match stir {
                    Stir::Key => composed::Event::Key,
                    Stir::Reset => composed::Event::Reset,
                    Stir::Left => composed::Event::Left,
                    Stir::Entered => composed::Event::Entered,
                    Stir::Gone => composed::Event::Gone,
                });
                if let Err(e) = tell(field, tells) {
                    eprintln!("heyo: dictation: the input method stopped taking words ({e})");
                    composed.on(composed::Event::Gone);
                }
                changed.notify_all();
            }),
        )?;
        Ok(Composer { composed, field })
    }
}

impl Way for Composer {
    fn hear(&mut self, spoken: Spoken) -> Result<(), Standing> {
        let (composed, changed) = &*self.composed;
        let mut composed = composed.lock().unwrap();
        let event = match spoken {
            Spoken::Partial(text) => composed::Event::Partial(text),
            Spoken::Sentence(text) => composed::Event::Sentence(text),
            Spoken::Result { text, polished } => composed::Event::Result { text, polished },
            Spoken::Cancelled => composed::Event::Cancelled,
            Spoken::Failed => composed::Event::Failed,
        };
        if matches!(spoken, Spoken::Result { .. }) {
            composed = changed.wait_timeout_while(composed, FIELD_BACK, |composed| composed.between_fields()).unwrap().0;
        }
        if let Some(standing) = composed.standing() {
            return Err(standing);
        }
        let before = composed.clone();
        let tells = composed.on(event);
        if let Some(standing) = composed.standing() {
            return Err(standing);
        }
        tell(self.field.as_mut(), tells).map_err(|e| {
            eprintln!("heyo: dictation: the input method stopped taking words ({e})");
            // What was to be said is not taken to have been.
            *composed = before;
            composed.given_up()
        })
    }

    fn live(&self) -> Option<bool> {
        Some(self.composed.0.lock().unwrap().live())
    }

    fn shortfall(&self) -> Option<Shortfall> {
        self.composed.0.lock().unwrap().shortfall()
    }
}

/// The words pasted into the application: carries out what `OnScreen`
/// says, and tells it when a step could not be carried out.
struct Paster {
    /// The words are wanted as they are spoken (the setting, the session).
    live: bool,
    /// The text is polished when the recording ends.
    polishing: bool,
    /// Asked at the first words: Heyo's shortcuts for dictation (pressing
    /// one is not typing) if the words can go in as they are spoken in the
    /// window in front, or why they cannot.
    here: Box<dyn Fn() -> Result<Vec<String>, &'static str> + Send>,
    /// Borrowed for the whole recording: pieces follow each other faster
    /// than it could come back.
    clipboard: pasting::Loan,
    /// What the application holds of the recording already, where its
    /// words were composed there until now.
    standing: Option<Standing>,
    screen: Option<OnScreen>,
    /// What corrects the text where it stands, for as long as that is safe.
    hand: Option<Box<dyn Writing>>,
    /// Counts the presses of a mouse button on the dictation window.
    own_presses: Arc<AtomicU32>,
    /// The last piece could not be written for the moment: the words are
    /// not reaching the application just now.
    put_off: bool,
}

impl Paster {
    fn new(live: bool, polishing: bool, clipboard: pasting::Loan, own_presses: Arc<AtomicU32>, here: Box<dyn Fn() -> Result<Vec<String>, &'static str> + Send>) -> Paster {
        Paster { live, polishing, here, clipboard, standing: None, screen: None, hand: None, own_presses, put_off: false }
    }

    /// Decides, at the first words, how this recording is written.
    fn begin(&mut self) -> OnScreen {
        // Says why not, never what was said.
        let hand = self.live.then(|| (self.here)().map_err(str::to_owned).and_then(|hotkeys| typed::write(&hotkeys, self.own_presses.clone())));
        if let Some(Err(why)) = &hand {
            eprintln!("heyo: dictation: not writing the words as they are spoken ({why})");
        }
        self.hand = hand.and_then(Result::ok);
        OnScreen::new(self.hand.is_some(), self.polishing)
    }

    /// Something was heard, or became of the last step.
    fn on(&mut self, event: onscreen::Event) {
        let mut screen = match (self.screen.take(), self.standing.take()) {
            (Some(screen), _) => screen,
            // What was composed there is the application's: nothing of it
            // is erased, and the rest is pasted after it.
            (None, Some(standing)) => OnScreen::resuming(standing, self.polishing),
            // With nothing said there is nothing to decide.
            (None, None) if !matches!(event, onscreen::Event::Partial(_) | onscreen::Event::Sentence(_)) => OnScreen::new(false, self.polishing),
            (None, None) => self.begin(),
        };
        let mut steps: VecDeque<Step> = screen.on(event).into();
        while let Some(step) = steps.pop_front() {
            match step {
                Step::Replace { erase, insert } => {
                    let replaced = match self.hand.as_deref_mut() {
                        Some(hand) => self.clipboard.rewrite(hand, erase, &insert),
                        None => Err(Lost::untouched("nothing is correcting the text any more")),
                    };
                    self.put_off = replaced.as_ref().is_err_and(|lost| lost.later);
                    match replaced {
                        Ok(()) => {}
                        // It cannot be written just now (another program
                        // has the keyboard): what is heard next is tried again.
                        Err(_) if self.put_off => steps.extend(screen.on(onscreen::Event::PutOff)),
                        Err(lost) => {
                            eprintln!("heyo: dictation: no longer correcting the text where it stands ({})", lost.why);
                            self.hand = None;
                            steps.extend(screen.on(onscreen::Event::Lost { erased: lost.erased, put: lost.put }));
                        }
                    }
                }
                Step::Paste(text) => {
                    // There is nowhere to ask: into a terminal the text goes on one line.
                    if let Err(e) = self.clipboard.paste(&text, pasting::Lines::Joined) {
                        eprintln!("heyo: dictation could not paste: {e}");
                    }
                }
            }
        }
        self.screen = Some(screen);
    }
}

impl Way for Paster {
    fn hear(&mut self, spoken: Spoken) -> Result<(), Standing> {
        match spoken {
            Spoken::Partial(text) => self.on(onscreen::Event::Partial(text)),
            Spoken::Sentence(text) => self.on(onscreen::Event::Sentence(text)),
            Spoken::Result { text, polished } => self.on(onscreen::Event::Result { text, polished }),
            Spoken::Cancelled => self.on(onscreen::Event::Cancelled),
            // Not another word is written.
            Spoken::Failed => {}
        }
        Ok(())
    }

    fn live(&self) -> Option<bool> {
        self.screen.as_ref().map(|screen| screen.live() && !self.put_off)
    }

    fn shortfall(&self) -> Option<Shortfall> {
        self.screen.as_ref().and_then(OnScreen::shortfall)
    }
}

/// Writes one recording into the application, one way or the other.
struct Writer {
    way: Box<dyn Way>,
    /// Takes over where the words can no longer be composed.
    spare: Option<Paster>,
}

impl Writer {
    /// The words are composed if a `composer` could begin, else pasted.
    fn new(composer: Option<Composer>, paster: Paster) -> Writer {
        match composer {
            Some(composer) => Writer { way: Box::new(composer), spare: Some(paster) },
            None => Writer { way: Box::new(paster), spare: None },
        }
    }

    /// Something was heard.
    fn hear(&mut self, spoken: Spoken) {
        let Err(standing) = self.way.hear(spoken) else { return };
        // Pasting does not come to an end.
        let Some(mut paster) = self.spare.take() else { return };
        eprintln!("heyo: dictation: the words are no longer composed in the application; the rest is pasted");
        paster.standing = Some(standing);
        // Before anything is pasted, the input method is the user's again.
        self.way = Box::new(paster);
        let _ = self.way.hear(spoken);
    }

    /// Whether the words are going in as they are spoken, once that is
    /// decided: what the window need not show too.
    fn writing_live(&self) -> Option<bool> {
        self.way.live()
    }

    /// The helper's last word on the recording, as the window is to be
    /// told it: with what could not be done added.
    fn end(&mut self, mut event: Value) -> Value {
        if event["type"] != "result" {
            self.hear(Spoken::Failed);
            return event;
        }
        let text = event["text"].as_str().unwrap_or_default().to_owned();
        let heard = match event["cancelled"] == true {
            true => Spoken::Cancelled,
            false => Spoken::Result { text: &text, polished: event["polished"] == true },
        };
        self.hear(heard);
        match self.way.shortfall() {
            // The text in the application is what was heard.
            Some(Shortfall::NotPolished(why)) => {
                event["warning"] = json!(why);
                event["text"] = event["raw"].clone();
            }
            Some(Shortfall::NotTakenBack) => event["kept"] = json!(true),
            None => {}
        }
        event
    }
}

/// How long the window stays once the recording is over: longer where
/// there is something to read.
fn linger(last: &Value) -> Duration {
    if last["type"] != "result" {
        return Duration::from_secs(4);
    }
    // A warning, having heard nothing, or how a cancelled recording ended.
    let to_read = last["warning"].is_string() || last["raw"].as_str().is_some_and(str::is_empty);
    Duration::from_millis(if to_read { 2500 } else { 300 })
}

/// Why the words cannot go in as they are spoken in the window in front,
/// or Heyo's shortcuts for dictation if they can.
fn here(app: &AppHandle) -> Result<Vec<String>, &'static str> {
    let own = app.webview_windows().values().any(|window| window.is_focused().unwrap_or(false));
    match not_here(crate::windows::active_class().as_deref(), own, |class| crate::expand::excluded(app, class)) {
        Some(why) => Err(why),
        None => Ok(crate::platform::hotkeys::get(&crate::platform::hotkeys::Target::dictation()).into_iter().collect()),
    }
}

/// Begins composing the words of recording number `session` in the window
/// in front, if they can be composed there.
fn composer(app: &AppHandle, live: bool, polishing: bool, session: u64) -> Option<Composer> {
    if !composing::supported() {
        return None;
    }
    // While the hotkey that started the recording is down, the desktop has
    // the keyboard and no field has the focus: which field the words are
    // for shows when the key is let go.
    let held = || {
        let state = app.state::<Dictation>();
        let state = state.0.lock().unwrap();
        state.holding && state.recording && state.session == session
    };
    while held() {
        std::thread::sleep(Duration::from_millis(15));
    }
    let hold_key = app.state::<SettingsState>().0.read().unwrap().dictation_hold_key.clone();
    // Says why not, never what was said.
    Composer::begin(live, polishing, &[hold_key.as_str()]).map_err(|why| eprintln!("heyo: dictation: the words are not composed in the application ({why})")).ok()
}

/// Starts the thread that writes recording number `session` into the
/// application: what is heard reaches it through `mailbox`, and it ends
/// with the recording. `live`: the words are wanted as they are spoken.
fn write(app: &AppHandle, mailbox: Arc<Mailbox>, live: bool, polishing: bool, session: u64) -> std::io::Result<()> {
    let app = app.clone();
    let spawned = std::thread::Builder::new().name("dictation-writing".into()).spawn(move || {
        let asking = app.clone();
        let paster = Paster::new(live && typed::rewriting(), polishing, pasting::borrow(), mailbox.own_presses.clone(), Box::new(move || here(&asking)));
        let mut writer = Writer::new(composer(&app, live, polishing, session), paster);
        let mut told = None;
        let last = loop {
            let last = match mailbox.next() {
                Said::Partial(text) => {
                    writer.hear(Spoken::Partial(&text));
                    None
                }
                Said::Sentence(text) => {
                    writer.hear(Spoken::Sentence(&text));
                    None
                }
                Said::Ended(event) => Some(writer.end(event)),
            };
            if let Some(live) = writer.writing_live().filter(|live| told != Some(*live)) {
                told = Some(live);
                emit(&app, json!({ "type": "writing", "live": live }));
            }
            if let Some(last) = last {
                break last;
            }
        };
        {
            let state = app.state::<Dictation>();
            let mut state = state.0.lock().unwrap();
            state.recording = false;
            state.writing = None;
        }
        // The session's input method is the user's again; a loan ends, and
        // the clipboard comes back once the last piece has been read from it.
        drop(writer);
        hide_window_after(&app, linger(&last));
        emit(&app, last);
    });
    spawned.map(|_| ())
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
    let writing = app.state::<Dictation>().0.lock().unwrap().writing.clone();
    let text = || event["text"].as_str().unwrap_or_default().to_owned();
    match (event["type"].as_str().unwrap_or_default(), writing) {
        ("partial", Some(writing)) => writing.post(Said::Partial(text())),
        ("segment", Some(writing)) => writing.post(Said::Sentence(text())),
        // The window hears of the end once all of it is written.
        ("result" | "error", Some(writing)) => return writing.post(Said::Ended(event)),
        // Nothing is writing: the recording ends as it is.
        ("result" | "error", None) => {
            app.state::<Dictation>().0.lock().unwrap().recording = false;
            hide_window_after(app, linger(&event));
        }
        _ => {}
    }
    emit(app, event);
}

/// A mouse button went down on the dictation window. That moves no cursor
/// in the application being written into, and must not be taken for the
/// user's doing something there.
pub fn pressed(app: &AppHandle) {
    if let Some(writing) = &app.state::<Dictation>().0.lock().unwrap().writing {
        writing.own_presses.fetch_add(1, Ordering::SeqCst);
    }
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
        eprintln!("heyo: {e}");
        // What the recording wrote is finished off by its own thread.
        if let Some(writing) = state.writing.clone() {
            return writing.post(Said::Ended(json!({ "type": "error", "message": e })));
        }
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
    let (language, source, polish, vocabulary, live) = {
        let settings = app.state::<SettingsState>();
        let settings = settings.0.read().unwrap();
        (settings.dictation_language.clone(), settings.dictation_source.clone(), settings.dictation_polish.clone(), settings.dictation_vocabulary.clone(), settings.dictation_live)
    };
    // Before the helper is told: one that is already running answers at
    // once, and the window must not hear that it is ready before it hears
    // that a recording begins.
    emit(app, json!({ "type": "start", "polish": polish }));
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
            state.session += 1;
            let mailbox = Arc::new(Mailbox::default());
            match write(app, mailbox.clone(), live, polish != "off", state.session) {
                Ok(()) => state.writing = Some(mailbox),
                Err(e) => eprintln!("heyo: dictation has nothing to write with: {e}"),
            }
            // The hotkey that started this is usually still down.
            let held = crate::windows::keys_down().filter(|keys| watch_hotkey && keys.iter().any(|b| *b != 0));
            state.holding = held.is_some();
            if let Some(keys) = held {
                watch_hold(app.clone(), keys, state.session);
            }
            drop(state);
            show_window(app);
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

#[derive(Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../shared/bindings/")]
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

/// Whether dictation can be used, for the settings page.
#[derive(Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../shared/bindings/", rename = "DictationStatus")]
pub struct Status {
    installed: bool,
    installing: bool,
    microphones: Vec<Microphone>,
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
    helper::install(node_command(app, &["install"]), move |progress| {
        if progress.concludes() {
            window.state::<Dictation>().0.lock().unwrap().installing = false;
        }
        let _ = window.emit(crate::wire::event::DICTATION_INSTALL, progress);
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::pasting::scripted::{clipboard, desk, happened};
    use crate::platform::desktop::fake::Fake;
    use crate::platform::desktop::Supported;
    use crate::platform::Support;

    const HOTKEY: &str = "<Control><Alt>d";
    const SECOND: Duration = Duration::from_secs(1);

    /// A writer for the window in front of a desk (`pasting::scripted`),
    /// which takes words as they are spoken.
    fn writer(pasting: &'static pasting::Pasting, live: bool, polishing: bool) -> Writer {
        Writer::new(None, Paster::new(live, polishing, pasting.borrow(), Arc::default(), Box::new(|| Ok(vec![HOTKEY.to_owned()]))))
    }

    /// What the helper said, line by line: `~text` is the sentence in
    /// progress, anything else a finished one.
    fn hear(writer: &mut Writer, said: &[&str]) {
        for line in said {
            match line.strip_prefix('~') {
                Some(partial) => writer.hear(Spoken::Partial(partial)),
                None => writer.hear(Spoken::Sentence(line)),
            }
        }
    }

    /// The recording ends; what the window is told of it.
    fn end(writer: &mut Writer, text: &str, raw: &str) -> Value {
        writer.end(json!({ "type": "result", "text": text, "raw": raw, "polished": text != raw }))
    }

    fn cancel(writer: &mut Writer) -> Value {
        writer.end(json!({ "type": "result", "text": "", "raw": "", "polished": false, "cancelled": true }))
    }

    /// What was asked of the desktop, without the notes to the clipboard
    /// history that go with every piece.
    fn asked(desktop: &Fake) -> Vec<String> {
        happened(desktop).into_iter().filter(|what| what != "unrecorded").collect()
    }

    /// The key held to talk, which is Heyo's and not typing.
    const MENU: &str = "menu";

    /// A writer for the window in front of a desk whose session has an
    /// input method: the words are composed there, if anything can be.
    fn composer(desktop: &'static Fake, pasting: &'static pasting::Pasting, live: bool, polishing: bool) -> Writer {
        *desktop.input_method.lock().unwrap() = true;
        assert!(composing::supported());
        let paster = Paster::new(live, polishing, pasting.borrow(), Arc::default(), Box::new(|| Ok(vec![HOTKEY.to_owned()])));
        Writer::new(Composer::begin(live, polishing, &[MENU]).ok(), paster)
    }

    #[test]
    fn a_recording_is_composed_in_the_application_and_nothing_else_is_touched() {
        let (desktop, world, pasting) = desk("firefox", Some("the user's"));
        desktop.during(|| {
            desktop.user_types("Dear Ada, ");
            // The input method is Heyo's before a word is heard.
            let mut writer = composer(desktop, pasting, true, false);
            assert_eq!((writer.writing_live(), asked(desktop)), (Some(true), vec!["composing begins menu".to_owned()]));
            hear(&mut writer, &["~The weather", "~The whether is nise", "~The weather is nice today", "The weather is nice today.", "~ See who", " See you at 3:30."]);
            let last = end(&mut writer, "The weather is nice today. See you at 3:30.", "The weather is nice today. See you at 3:30.");
            // The sentence in progress is composed, whole each time; the
            // finished one is committed. Nothing is erased, nothing pasted.
            assert_eq!(
                asked(desktop),
                ["compose The weather", "compose The whether is nise", "compose The weather is nice today", "commit The weather is nice today.", "compose ", "compose  See who", "commit  See you at 3:30.", "compose "]
            );
            assert_eq!(desktop.field(), "Dear Ada, The weather is nice today. See you at 3:30.");
            assert_eq!((&last["warning"], &last["kept"]), (&Value::Null, &Value::Null));
            // With the recording, composing ends: the input method is the user's again.
            drop(writer);
            world.pass(SECOND);
            assert_eq!(happened(desktop), ["composing ends"]);
            assert_eq!(clipboard(desktop).as_deref(), Some("the user's"));
        });
    }

    #[test]
    fn a_key_of_the_users_goes_in_behind_what_was_composed_and_the_rest_behind_the_key() {
        let (desktop, _, pasting) = desk("firefox", None);
        desktop.during(|| {
            let mut writer = composer(desktop, pasting, true, false);
            hear(&mut writer, &["~The weather is"]);
            desktop.user_presses("!");
            hear(&mut writer, &["~The weather is nice", "The weather is nice today.", "~ See", " See you."]);
            end(&mut writer, "The weather is nice today. See you.", "The weather is nice today. See you.");
            assert_eq!(
                asked(desktop),
                ["composing begins menu", "compose The weather is", "commit The weather is", "compose ", "compose  nice", "commit  nice today.", "compose ", "compose  See", "commit  See you.", "compose "]
            );
            assert_eq!(desktop.field(), "The weather is! nice today. See you.");
        });
    }

    #[test]
    fn the_polished_text_is_what_is_committed_and_the_recording_is_composed_until_then() {
        let (desktop, _, pasting) = desk("firefox", None);
        desktop.during(|| {
            desktop.user_types("> ");
            let mut writer = composer(desktop, pasting, true, true);
            hear(&mut writer, &["~um so the meeting", "um so the meeting is on thursday.", "~ uh at ten", " uh at ten."]);
            assert_eq!((desktop.field().as_str(), desktop.composing.lock().unwrap().as_str()), ("> ", "um so the meeting is on thursday. uh at ten."));
            asked(desktop);
            let last = end(&mut writer, "The meeting is on Thursday at ten.", "um so the meeting is on thursday. uh at ten.");
            assert_eq!(asked(desktop), ["commit The meeting is on Thursday at ten.", "compose "]);
            assert_eq!(desktop.field(), "> The meeting is on Thursday at ten.");
            assert_eq!((&last["text"], &last["warning"]), (&json!("The meeting is on Thursday at ten."), &Value::Null));
        });
        // A key during the recording made part of it the application's as
        // it was heard: the rest follows as heard, and the window says so.
        let (desktop, _, pasting) = desk("firefox", None);
        desktop.during(|| {
            let mut writer = composer(desktop, pasting, true, true);
            hear(&mut writer, &["~um hello"]);
            desktop.user_presses(",");
            hear(&mut writer, &["um hello there."]);
            let last = end(&mut writer, "Hello there.", "um hello there.");
            assert_eq!(desktop.field(), "um hello, there.");
            assert_eq!(last["text"], "um hello there.");
            assert!(last["warning"].as_str().unwrap().contains("as it was heard"));
        });
    }

    #[test]
    fn polished_text_of_several_lines_is_committed_into_a_terminal_on_one() {
        let (desktop, _, pasting) = desk("org.gnome.Ptyxis", None);
        desktop.during(|| {
            let mut writer = composer(desktop, pasting, true, true);
            hear(&mut writer, &["list the files and then build."]);
            end(&mut writer, "ls -la\nmake\n", "list the files and then build.");
            assert_eq!(desktop.field(), "ls -la make");
        });
    }

    #[test]
    fn a_cancelled_recording_is_no_longer_composed() {
        let (desktop, _, pasting) = desk("firefox", None);
        desktop.during(|| {
            desktop.user_types("Dear Ada, ");
            let mut writer = composer(desktop, pasting, true, false);
            hear(&mut writer, &["One.", "~ Two th"]);
            asked(desktop);
            let last = cancel(&mut writer);
            assert_eq!(asked(desktop), ["compose "]);
            // The finished sentence is the application's: it stays, and the window says so.
            assert_eq!((desktop.field().as_str(), &last["kept"]), ("Dear Ada, One.", &json!(true)));
        });
        // With polishing nothing was committed, and nothing is left.
        let (desktop, _, pasting) = desk("firefox", None);
        desktop.during(|| {
            let mut writer = composer(desktop, pasting, true, true);
            hear(&mut writer, &["One.", "~ Two th"]);
            let last = cancel(&mut writer);
            assert_eq!((desktop.field().as_str(), desktop.composing.lock().unwrap().as_str(), &last["kept"]), ("", "", &Value::Null));
        });
    }

    #[test]
    fn a_recording_that_breaks_off_leaves_what_was_being_composed() {
        let (desktop, _, pasting) = desk("firefox", None);
        desktop.during(|| {
            let mut writer = composer(desktop, pasting, true, false);
            hear(&mut writer, &["One.", "~ Two th"]);
            let error = json!({ "type": "error", "message": "Dictation stopped unexpectedly" });
            assert_eq!(writer.end(error.clone()), error);
            assert_eq!((desktop.field().as_str(), desktop.composing.lock().unwrap().as_str()), ("One. Two th", ""));
        });
    }

    #[test]
    fn words_spoken_while_no_field_has_the_focus_go_in_when_one_has() {
        let (desktop, _, pasting) = desk("firefox", None);
        desktop.during(|| {
            let mut writer = composer(desktop, pasting, true, false);
            hear(&mut writer, &["~The wea"]);
            // A hotkey is pressed, or another window comes to the front.
            desktop.stirs(Stir::Left);
            assert_eq!((desktop.field(), writer.writing_live()), (String::new(), Some(false)));
            hear(&mut writer, &["~The weather is", "The weather is nice.", "~ See"]);
            asked(desktop);
            desktop.stirs(Stir::Entered);
            assert_eq!(asked(desktop), ["commit The weather is nice.", "compose  See"]);
            assert_eq!(writer.writing_live(), Some(true));
            hear(&mut writer, &[" See you."]);
            end(&mut writer, "The weather is nice. See you.", "The weather is nice. See you.");
            assert_eq!(desktop.field(), "The weather is nice. See you.");
        });
    }

    #[test]
    fn where_the_words_can_no_longer_be_composed_the_rest_is_pasted() {
        // The user chooses another input method in the middle of a sentence.
        let (desktop, world, pasting) = desk("firefox", Some("the user's"));
        desktop.during(|| {
            let mut writer = composer(desktop, pasting, true, false);
            hear(&mut writer, &["One.", "~ Two th"]);
            desktop.stirs(Stir::Gone);
            assert_eq!(writer.writing_live(), Some(false));
            asked(desktop);
            hear(&mut writer, &["~ Two three", " Two three.", "~ Fou", " Four."]);
            // Composing is given up before a key is pressed for pasting.
            assert_eq!(asked(desktop), ["composing ends", "copy  Two three.", "wait 60", "paste", "copy  Four.", "wait 60", "paste"]);
            let last = end(&mut writer, "One. Two three. Four.", "One. Two three. Four.");
            assert_eq!(desktop.field(), "One. Two three. Four.");
            assert_eq!(last["warning"], Value::Null);
            drop(writer);
            world.pass(SECOND);
            assert_eq!(clipboard(desktop).as_deref(), Some("the user's"));
        });
        // The recording ends with no field to commit the last sentence in.
        let (desktop, _, pasting) = desk("firefox", None);
        desktop.during(|| {
            let mut writer = composer(desktop, pasting, true, false);
            hear(&mut writer, &["One.", "~ Two th"]);
            desktop.stirs(Stir::Left);
            hear(&mut writer, &[" Two three."]);
            asked(desktop);
            end(&mut writer, "One. Two three.", "One. Two three.");
            assert_eq!(asked(desktop), ["composing ends", "copy  Two three.", "wait 60", "paste"]);
            assert_eq!(desktop.field(), "One. Two three.");
        });
        // With polishing and nothing committed, the polished text is pasted.
        let (desktop, _, pasting) = desk("firefox", None);
        desktop.during(|| {
            let mut writer = composer(desktop, pasting, true, true);
            hear(&mut writer, &["um one.", "~ um two"]);
            desktop.stirs(Stir::Gone);
            asked(desktop);
            hear(&mut writer, &[" um two."]);
            let last = end(&mut writer, "One. Two.", "um one. um two.");
            assert_eq!(asked(desktop), ["composing ends", "copy One. Two.", "wait 60", "paste"]);
            assert_eq!((desktop.field().as_str(), &last["warning"]), ("One. Two.", &Value::Null));
        });
    }

    #[test]
    fn where_nothing_can_be_composed_the_words_are_pasted_from_the_start() {
        let (desktop, _, pasting) = desk("firefox", None);
        *desktop.no_field.lock().unwrap() = Some("the window in front takes no input method".into());
        desktop.during(|| {
            let mut writer = composer(desktop, pasting, true, false);
            assert_eq!((writer.writing_live(), asked(desktop)), (None, Vec::new()));
            hear(&mut writer, &["~Hel", "~Hello wor", "Hello world."]);
            assert_eq!(asked(desktop), ["write <Control><Alt>d", "rewrite 0 with Hel", "rewrite 0 with lo wor", "rewrite 0 with ld."]);
            assert_eq!(desktop.field(), "Hello world.");
        });
        // A session without an input method that Heyo speaks to is not asked.
        let (desktop, _, _) = desk("firefox", None);
        desktop.during(|| {
            assert!(!composing::supported());
            assert!(composing::compose(&[], Box::new(|_, _| {})).is_err());
            assert_eq!(asked(desktop), Vec::<String>::new());
        });
    }

    #[test]
    fn without_live_words_sentences_are_committed_and_nothing_is_shown_meanwhile() {
        let (desktop, _, pasting) = desk("firefox", Some("the user's"));
        desktop.during(|| {
            let mut writer = composer(desktop, pasting, false, false);
            hear(&mut writer, &["~Hel", "Hello.", "~ Wor", " World."]);
            end(&mut writer, "Hello. World.", "Hello. World.");
            assert_eq!(asked(desktop), ["composing begins menu", "commit Hello.", "commit  World."]);
            assert_eq!((desktop.field().as_str(), writer.writing_live()), ("Hello. World.", Some(false)));
        });
    }

    #[test]
    fn a_recording_goes_into_the_application_as_it_is_spoken() {
        let (desktop, world, pasting) = desk("firefox", Some("the user's"));
        desktop.during(|| {
            desktop.user_types("Dear Ada, ");
            let mut writer = writer(pasting, true, false);
            // Nothing is decided, and nothing touched, before the first words.
            assert_eq!((writer.writing_live(), asked(desktop)), (None, Vec::new()));
            hear(&mut writer, &["~The weather", "~The weather is nise", "~The weather is nice today", "The weather is nice today.", "~ See who", " See you at 3:30."]);
            let last = end(&mut writer, "The weather is nice today. See you at 3:30.", "The weather is nice today. See you at 3:30.");
            // Each piece erases what turned out wrong, and no more, and
            // writes what is new.
            assert_eq!(
                asked(desktop),
                [
                    "write <Control><Alt>d",
                    "rewrite 0 with The weather",
                    "rewrite 0 with  is nise",
                    "rewrite 2 with ce today",
                    "rewrite 0 with .",
                    "rewrite 0 with  See who",
                    "rewrite 3 with you at 3:30."
                ]
            );
            assert_eq!(desktop.field(), "Dear Ada, The weather is nice today. See you at 3:30.");
            assert_eq!(writer.writing_live(), Some(true));
            assert_eq!((&last["warning"], &last["kept"]), (&Value::Null, &Value::Null));
            // The clipboard was borrowed once, and comes back as it was.
            assert_eq!(clipboard(desktop).as_deref(), Some("you at 3:30."));
            drop(writer);
            world.pass(SECOND);
            assert_eq!(happened(desktop), ["unrecorded", "copy the user's"]);
        });
    }

    #[test]
    fn once_the_user_types_the_rest_is_pasted_and_nothing_of_theirs_erased() {
        let (desktop, world, pasting) = desk("firefox", Some("the user's"));
        desktop.during(|| {
            let mut writer = writer(pasting, true, false);
            hear(&mut writer, &["~The weather is"]);
            desktop.user_types("!");
            hear(&mut writer, &["~The weather was nice", "The weather is nice today.", "~ See", " See you."]);
            end(&mut writer, "The weather is nice today. See you.", "The weather is nice today. See you.");
            // Not one Backspace after the user's key: what was not there
            // yet is pasted, sentence by sentence, at the cursor.
            assert_eq!(asked(desktop), ["write <Control><Alt>d", "rewrite 0 with The weather is", "copy  nice today.", "wait 60", "paste", "copy  See you.", "wait 60", "paste"]);
            assert_eq!(desktop.field(), "The weather is! nice today. See you.");
            assert_eq!(writer.writing_live(), Some(false));
            drop(writer);
            world.pass(SECOND);
            assert_eq!(clipboard(desktop).as_deref(), Some("the user's"));
        });
    }

    #[test]
    fn a_piece_the_application_was_not_seen_to_take_is_the_last_one_corrected() {
        let (desktop, _, pasting) = desk("firefox", None);
        desktop.during(|| {
            desktop.losing.lock().unwrap().push_back(Lost { erased: 0, put: true, later: false, why: "the application was not seen to take the text".into() });
            let mut writer = writer(pasting, true, false);
            hear(&mut writer, &["~Hello", "~Hello world", "Hello world."]);
            // It is taken to be there, and never erased.
            assert_eq!(asked(desktop), ["write <Control><Alt>d", "rewrite 0 with Hello", "copy  world.", "wait 60", "paste"]);
            assert_eq!(desktop.field(), "Hello world.");
        });
    }

    #[test]
    fn pieces_put_off_while_a_hotkey_is_held_go_in_when_it_is_let_go() {
        let (desktop, _, pasting) = desk("firefox", None);
        desktop.during(|| {
            let held = || desktop.losing.lock().unwrap().push_back(Lost::put_off("another program has the keyboard to itself"));
            let mut writer = writer(pasting, true, false);
            held();
            hear(&mut writer, &["~The weather"]);
            held();
            hear(&mut writer, &["The weather is nice."]);
            assert_eq!(desktop.field(), "");
            // The words are not in the application yet: the window shows them.
            assert_eq!(writer.writing_live(), Some(false));
            hear(&mut writer, &["~ See who", " See you."]);
            assert_eq!(writer.writing_live(), Some(true));
            assert_eq!(asked(desktop), ["write <Control><Alt>d", "rewrite 0 with nothing", "rewrite 0 with nothing", "rewrite 0 with The weather is nice. See who", "rewrite 3 with you."]);
            assert_eq!(desktop.field(), "The weather is nice. See you.");
        });
    }

    #[test]
    fn where_words_cannot_go_in_as_they_are_spoken_sentences_are_pasted() {
        // A terminal: on one line, with the terminal's shortcut.
        let (desktop, _, pasting) = desk("org.gnome.Ptyxis", None);
        desktop.during(|| {
            let here = || not_here(crate::windows::active_class().as_deref(), false, |_| false).map_or(Ok(Vec::new()), Err);
            let mut writer = Writer::new(None, Paster::new(true, false, pasting.borrow(), Arc::default(), Box::new(here)));
            hear(&mut writer, &["~ls", "ls minus la.", "~ And", " And then\nmake."]);
            assert_eq!(asked(desktop), ["copy ls minus la.", "wait 60", "paste terminal", "copy  And then\nmake.", "wait 60", "copy And then make.", "wait 60", "paste terminal"]);
            assert_eq!(writer.writing_live(), Some(false));
        });
        // The setting is off.
        let (desktop, _, pasting) = desk("firefox", None);
        desktop.during(|| {
            let mut writer = writer(pasting, false, false);
            hear(&mut writer, &["~Hel", "Hello.", "~ Wor", " World."]);
            assert_eq!(asked(desktop), ["copy Hello.", "wait 60", "paste", "copy  World.", "wait 60", "paste"]);
            assert_eq!(desktop.field(), "Hello. World.");
        });
        // The session cannot correct text in another window.
        let (desktop, _, pasting) = desk("firefox", None);
        *desktop.supported.lock().unwrap() = Some(Supported { paste: Support::Full, ..Supported::NOTHING });
        desktop.during(|| {
            assert!(!typed::rewriting());
            let mut writer = writer(pasting, true, false);
            hear(&mut writer, &["~Hel", "Hello."]);
            assert_eq!(asked(desktop), ["copy Hello.", "wait 60", "paste"]);
        });
    }

    #[test]
    fn polished_text_takes_the_place_of_what_was_written() {
        let (desktop, world, pasting) = desk("firefox", Some("the user's"));
        desktop.during(|| {
            desktop.user_types("> ");
            let mut writer = writer(pasting, true, true);
            hear(&mut writer, &["~um so the meeting", "um so the meeting is on thursday."]);
            assert_eq!(desktop.field(), "> um so the meeting is on thursday.");
            asked(desktop);
            let last = end(&mut writer, "The meeting is on Thursday.", "um so the meeting is on thursday.");
            assert_eq!(asked(desktop), ["rewrite 33 with The meeting is on Thursday."]);
            assert_eq!(desktop.field(), "> The meeting is on Thursday.");
            assert_eq!((&last["text"], &last["warning"]), (&json!("The meeting is on Thursday."), &Value::Null));
            drop(writer);
            world.pass(SECOND);
            assert_eq!(clipboard(desktop).as_deref(), Some("the user's"));
        });
    }

    #[test]
    fn polished_text_is_left_out_where_it_cannot_safely_go_in_and_the_window_says_so() {
        let (desktop, _, pasting) = desk("firefox", None);
        desktop.during(|| {
            let mut writer = writer(pasting, true, true);
            hear(&mut writer, &["um hello there."]);
            // The user goes on typing while the text is being polished.
            desktop.user_types(" ok");
            asked(desktop);
            let last = end(&mut writer, "Hello there.", "um hello there.");
            assert_eq!(asked(desktop), Vec::<String>::new());
            assert_eq!(desktop.field(), "um hello there. ok");
            // What the window shows is what is in the application.
            assert_eq!(last["text"], "um hello there.");
            assert!(last["warning"].as_str().unwrap().contains("left as it was heard"));
            assert_eq!(linger(&last), Duration::from_millis(2500));
        });
        // Without live words, polished text is pasted at the end as before.
        let (desktop, _, pasting) = desk("firefox", None);
        desktop.during(|| {
            let mut writer = writer(pasting, false, true);
            hear(&mut writer, &["~um hel", "um hello there."]);
            assert_eq!(asked(desktop), Vec::<String>::new());
            let last = end(&mut writer, "Hello there.", "um hello there.");
            assert_eq!(asked(desktop), ["copy Hello there.", "wait 60", "paste"]);
            assert_eq!(last["warning"], Value::Null);
            assert_eq!(linger(&last), Duration::from_millis(300));
        });
    }

    #[test]
    fn a_cancelled_recording_takes_its_words_back_if_it_can() {
        let (desktop, _, pasting) = desk("firefox", None);
        desktop.during(|| {
            desktop.user_types("Dear Ada, ");
            let mut writer = writer(pasting, true, false);
            hear(&mut writer, &["One.", "~ Two th"]);
            asked(desktop);
            let last = cancel(&mut writer);
            assert_eq!(asked(desktop), ["rewrite 11 with "]);
            assert_eq!(desktop.field(), "Dear Ada, ");
            assert_eq!(last["kept"], Value::Null);
        });
        // The user typed: the words stay, and the window says that they do.
        let (desktop, _, pasting) = desk("firefox", None);
        desktop.during(|| {
            let mut writer = writer(pasting, true, false);
            hear(&mut writer, &["One."]);
            desktop.user_types("!");
            let last = cancel(&mut writer);
            assert_eq!(desktop.field(), "One.!");
            assert_eq!(last["kept"], true);
        });
        // Nothing was said: nothing is decided, written or kept.
        let (desktop, _, pasting) = desk("firefox", None);
        desktop.during(|| {
            let mut writer = writer(pasting, true, true);
            let last = cancel(&mut writer);
            assert_eq!((asked(desktop), &last["kept"]), (Vec::new(), &Value::Null));
        });
    }

    #[test]
    fn presses_on_the_dictation_window_are_counted_where_the_writing_reads_them() {
        let (desktop, _, pasting) = desk("firefox", None);
        desktop.during(|| {
            let mailbox = Mailbox::default();
            let mut writer = Writer::new(None, Paster::new(true, false, pasting.borrow(), mailbox.own_presses.clone(), Box::new(|| Ok(Vec::new()))));
            writer.hear(Spoken::Partial("One"));
            // The window's own button is pressed, to finish.
            mailbox.own_presses.fetch_add(1, Ordering::SeqCst);
            writer.hear(Spoken::Sentence("One two."));
            assert_eq!(asked(desktop), ["write", "rewrite 0 with One", "own presses 1", "rewrite 0 with  two."]);
        });
    }

    #[test]
    fn words_are_not_written_as_they_are_spoken_where_that_is_not_safe() {
        let never = |_: &str| false;
        for class in ["firefox", "Code", "org.gnome.TextEditor", "Slack", "libreoffice-writer"] {
            assert_eq!(not_here(Some(class), false, never), None, "{class}");
        }
        for class in ["org.gnome.Ptyxis", "kitty", "Gnome-terminal", "Gvim", "Emacs", "libreoffice-calc", "org.remmina.Remmina", "VirtualBox Machine", "KeePassXC", "Heyo"] {
            assert!(not_here(Some(class), false, |class| class == "KeePassXC").is_some(), "{class}");
        }
        // One of Heyo's own windows, an unknown one, an excluded application.
        assert!(not_here(Some("firefox"), true, never).is_some());
        assert!(not_here(None, false, never).is_some());
        assert!(not_here(Some("Signal"), false, |class| class == "Signal").is_some());
    }

    #[test]
    fn only_the_newest_state_of_a_sentence_waits_to_be_written() {
        let mailbox = Mailbox::default();
        let waiting = |mailbox: &Mailbox| {
            let mut all = Vec::new();
            while !mailbox.waiting.lock().unwrap().is_empty() {
                all.push(match mailbox.next() {
                    Said::Partial(text) => format!("~{text}"),
                    Said::Sentence(text) => text,
                    Said::Ended(event) => event["type"].as_str().unwrap().to_owned(),
                });
            }
            all
        };
        // The writer is busy with a slow application meanwhile.
        for partial in ["The", "The weather", "The weather is"] {
            mailbox.post(Said::Partial(partial.into()));
        }
        assert_eq!(waiting(&mailbox), ["~The weather is"]);
        // A sentence that finished makes what was heard of it in progress moot,
        mailbox.post(Said::Partial("The weather is nice".into()));
        mailbox.post(Said::Sentence("The weather is nice.".into()));
        mailbox.post(Said::Partial(" And".into()));
        mailbox.post(Said::Partial(" And so".into()));
        // but no sentence and no ending is ever dropped.
        mailbox.post(Said::Sentence(" And so on.".into()));
        mailbox.post(Said::Sentence(" Third.".into()));
        mailbox.post(Said::Partial(" Fou".into()));
        mailbox.post(Said::Ended(json!({ "type": "result" })));
        assert_eq!(waiting(&mailbox), ["The weather is nice.", " And so on.", " Third.", "result"]);
    }

    #[test]
    fn what_needs_reading_stays_up() {
        let soon = Duration::from_millis(300);
        let later = Duration::from_millis(2500);
        assert_eq!(linger(&json!({ "type": "result", "text": "Hello.", "raw": "Hello." })), soon);
        // Polishing failed, nothing was heard, or the recording was cancelled.
        assert_eq!(linger(&json!({ "type": "result", "text": "um hello", "raw": "um hello", "warning": "no API key" })), later);
        assert_eq!(linger(&json!({ "type": "result", "text": "", "raw": "" })), later);
        assert_eq!(linger(&json!({ "type": "result", "text": "", "raw": "", "cancelled": true, "kept": true })), later);
        assert_eq!(linger(&json!({ "type": "error", "message": "no microphone" })), Duration::from_secs(4));
    }

    #[test]
    fn an_error_ends_a_recording_without_another_word_written() {
        let (desktop, _, pasting) = desk("firefox", None);
        desktop.during(|| {
            let mut writer = writer(pasting, true, false);
            hear(&mut writer, &["~Hello"]);
            asked(desktop);
            let error = json!({ "type": "error", "message": "Dictation stopped unexpectedly" });
            assert_eq!(writer.end(error.clone()), error);
            assert_eq!(asked(desktop), Vec::<String>::new());
            assert_eq!(desktop.field(), "Hello");
        });
    }

    #[test]
    fn an_extension_listening_hears_sentences_and_then_all_of_it() {
        assert_eq!(heard(&json!({ "type": "segment", "text": "Hello." })), Some(Heard::Segment("Hello.".into())));
        // A later sentence comes with the space before it, which is not part of it.
        assert_eq!(heard(&json!({ "type": "segment", "text": " World." })), Some(Heard::Segment("World.".into())));
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

    /// Everything `runtime/src/dictation.ts` writes while recording, and what
    /// the host says itself, is something the window is told.
    #[test]
    fn the_window_is_told_what_the_helper_and_the_host_say() {
        let told = |event: Value| serde_json::from_value::<Told>(event);
        for event in [
            json!({ "type": "start", "polish": "off" }),
            json!({ "type": "ready" }),
            json!({ "type": "level", "value": 0.4 }),
            json!({ "type": "partial", "text": "Hel" }),
            json!({ "type": "segment", "text": "Hello." }),
            json!({ "type": "writing", "live": true }),
            json!({ "type": "polishing" }),
            json!({ "type": "result", "text": "um hello", "raw": "um hello", "warning": "no API key" }),
            json!({ "type": "result", "text": "", "raw": "", "cancelled": true }),
            json!({ "type": "result", "text": "", "raw": "", "cancelled": true, "kept": true }),
            json!({ "type": "error", "message": "no microphone" }),
        ] {
            // As it was said, nothing added and nothing lost.
            assert_eq!(serde_json::to_value(told(event.clone()).unwrap_or_else(|e| panic!("{event}: {e}"))).unwrap(), event);
        }
        // What the window does not read stays behind; a level that could not be measured is none.
        let result = told(json!({ "type": "result", "text": "Hello.", "raw": "um hello", "polished": true })).unwrap();
        assert_eq!(result, Told::Result { text: "Hello.".into(), raw: "um hello".into(), warning: None, cancelled: None, kept: None });
        assert_eq!(told(json!({ "type": "level", "value": null })).unwrap(), Told::Level { value: None });
        // An installer's progress, or anything else, is not for this window.
        assert!(told(json!({ "type": "progress", "step": "Speech engine" })).is_err());
        assert!(told(json!({ "text": "hi" })).is_err());
    }
}

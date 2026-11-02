//! Voice dictation: speak, and the text goes into the focused window.
//!
//! A recording (`listening`) records the microphone (`microphone`) and
//! has a recognizer hear it (`engine`: a model of the catalogue on this
//! computer, from files downloaded on demand, `models`; or a service, as
//! the provider the settings name opens it). The life of the recordings,
//! one after the other, is the `session`'s: what starts, finishes or
//! discards one, what is done with what it hears, when it is polished by
//! the model the settings name (`polish`) and when it is over, and what the
//! pill is told meanwhile. It answers each thing that happens with what is
//! to be done; the shell here does that, and is the only part that holds
//! the application. What is heard is written the user's way (`vocabulary`)
//! into the application the user is working in, or given to an extension
//! that asked to listen.
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
//! holds up neither the recognizer nor the window, and of the sentence in
//! progress only the newest state is ever written: what was heard
//! meanwhile is skipped.

mod composed;
pub(crate) mod engine;
mod listening;
mod microphone;
pub(crate) mod models;
mod onscreen;
mod polish;
mod session;
pub(crate) mod vocabulary;

use crate::i18n::translate;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::platform::composing::{self, Composing, Stir};
use crate::platform::typed::{self, Lost, Writing};
use crate::models::registry;
use crate::{pasting, SettingsState};
use composed::{Composed, Tell};
use engine::Opener;
pub(crate) use engine::LANGUAGES;
use onscreen::{OnScreen, Shortfall, Standing, Step};
use session::{Effect, End, Input, Said, Session, Setup};
pub(crate) use session::{Heard, NotListening};

/// Dictation in Tauri's state: the session, and what its effects hold
/// while they last.
#[derive(Default)]
pub struct Dictation(Mutex<Shell>);

/// What carries out what the session calls for. Each is the recording's
/// of that number.
#[derive(Default)]
struct Shell {
    session: Session,
    /// The recording in progress, until it is told to end.
    listening: Option<(u64, listening::Listening)>,
    /// Where what is heard goes to be written into the application: the
    /// recording's own thread takes it from there.
    writing: Option<(u64, Arc<Mailbox>)>,
    /// The polishing of the recording that is over, while it goes on.
    polishing: Option<tauri::async_runtime::JoinHandle<()>>,
}

/// Doubao, as a provider of speech recognition (`models::registry`): its
/// configuration, its models, checking it.
pub(crate) use engine::doubao;

/// What hears dictation, as the settings choose it (`dictationModel`,
/// resolved through the providers and opened by the provider's kind: the
/// one place it is chosen). A reference to what cannot recognize speech,
/// or a provider that is not set up (Doubao without a key), is what to say
/// instead.
fn opener(settings: &crate::settings::Settings) -> Result<Box<dyn Opener>, String> {
    let (provider, model) = registry::resolve(&settings.providers, &settings.dictation_model, registry::Use::DictationRecognition).map_err(|why| crate::models::ModelError::Unusable(why).message())?;
    match provider.kind().recognizer(provider, model, &vocabulary::parse(&settings.dictation_vocabulary)) {
        Some(opened) => opened.map_err(|failure| crate::models::kinds::worded(&failure)),
        None => Err(crate::models::ModelError::Unusable(registry::Unusable::Cannot(provider.name.clone())).message()),
    }
}

pub fn dir() -> PathBuf {
    crate::paths::data().join("dictation")
}

/// Whether what `settings` choose to hear with can be used without a
/// download first. What cannot be used at all needs no download: starting
/// says what it does need.
pub fn ready(settings: &crate::settings::Settings) -> bool {
    opener(settings).map_or(true, |opener| opener.ready())
}

/// The entry of the catalogue in use: the model the settings choose, if
/// it is one Heyo loads itself.
fn in_use(app: &AppHandle) -> Option<String> {
    let settings = app.state::<SettingsState>();
    let settings = settings.0.read().unwrap();
    opener(&settings).ok()?.kept().map(str::to_owned)
}

/// What the dictation window is told of a recording: the recording's account
/// of it, and the host's own of one starting or failing.
#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub(crate) enum Told {
    /// A recording begins; `polish` is the mode it will be polished in,
    /// and `cloud` whether the audio is sent to a service to be recognized.
    Start {
        polish: String,
        cloud: bool,
    },
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

/// Between the recording, which says what it hears as it hears it, and the
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

    /// The recording's last word, as the window is to be told it: with
    /// what could not be done added.
    fn end(&mut self, end: End) -> Told {
        let mut told = end.told();
        let polished;
        let heard = match &end {
            End::Failed(_) => {
                self.hear(Spoken::Failed);
                return told;
            }
            End::Cancelled => Spoken::Cancelled,
            End::Over(outcome) => {
                polished = outcome.polished;
                Spoken::Result { text: &outcome.text, polished }
            }
        };
        self.hear(heard);
        if let Told::Result { text, raw, warning, kept, .. } = &mut told {
            match self.way.shortfall() {
                // The text in the application is what was heard.
                Some(Shortfall::NotPolished(why)) => {
                    *warning = Some(translate(why).into());
                    *text = raw.clone();
                }
                Some(Shortfall::NotTakenBack) => *kept = Some(true),
                None => {}
            }
        }
        told
    }
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

/// Begins composing the words of recording number `recording` in the
/// window in front, if they can be composed there.
fn composer(app: &AppHandle, live: bool, polishing: bool, recording: u64) -> Option<Composer> {
    if !composing::supported() {
        return None;
    }
    // While the hotkey that started the recording is down, the desktop has
    // the keyboard and no field has the focus: which field the words are
    // for shows when the key is let go.
    while app.state::<Dictation>().0.lock().unwrap().session.holding(recording) {
        std::thread::sleep(Duration::from_millis(15));
    }
    let hold_key = app.state::<SettingsState>().0.read().unwrap().dictation_hold_key.clone();
    // Says why not, never what was said.
    Composer::begin(live, polishing, &[hold_key.as_str()]).map_err(|why| eprintln!("heyo: dictation: the words are not composed in the application ({why})")).ok()
}

/// Starts the thread that writes recording number `recording` into the
/// application: what is heard reaches it through `mailbox`, and it ends
/// with the recording. `live`: the words are wanted as they are spoken.
fn write(app: &AppHandle, mailbox: Arc<Mailbox>, live: bool, polishing: bool, recording: u64) -> std::io::Result<()> {
    let app = app.clone();
    let spawned = std::thread::Builder::new().name("dictation-writing".into()).spawn(move || {
        let asking = app.clone();
        let paster = Paster::new(live && typed::rewriting(), polishing, pasting::borrow(), mailbox.own_presses.clone(), Box::new(move || here(&asking)));
        let mut writer = Writer::new(composer(&app, live, polishing, recording), paster);
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
                Said::Ended(end) => Some(writer.end(end)),
            };
            if let Some(live) = writer.writing_live().filter(|live| told != Some(*live)) {
                told = Some(live);
                crate::slint_windows::pill::tell(Told::Writing { live });
            }
            if let Some(last) = last {
                break last;
            }
        };
        // The session's input method is the user's again; a loan ends, and
        // the clipboard comes back once the last piece has been read from it.
        drop(writer);
        run(&app, Input::Written(recording, last));
    });
    spawned.map(|_| ())
}

/// The settings a recording starts or finishes with, as they are now.
fn setup(app: &AppHandle) -> Setup {
    let settings = app.state::<SettingsState>();
    let settings = settings.0.read().unwrap();
    Setup {
        opener: opener(&settings),
        source: settings.dictation_source.clone(),
        language: settings.dictation_language.clone(),
        polish: settings.dictation_polish.clone(),
        vocabulary: settings.dictation_vocabulary.clone(),
        live: settings.dictation_live,
    }
}

/// Has the session take `input`, and carries out what it calls for, with
/// what that comes to taken in turn. One at a time: what the session
/// called for is done before it takes the next. What reaches an extension
/// is said once nothing is held. The answer to an extension that asked to
/// listen, if it did.
fn run(app: &AppHandle, input: Input) -> Option<Result<u64, NotListening>> {
    let mut inputs = VecDeque::from([input]);
    let mut heard = Vec::new();
    let mut answer = None;
    let dictation = app.state::<Dictation>();
    let mut shell = dictation.0.lock().unwrap();
    while let Some(input) = inputs.pop_front() {
        if let Input::Written(recording, _) = &input {
            shell.writing.take_if(|(writing, _)| writing == recording);
        }
        let effects = shell.session.on(input, Instant::now(), &|| setup(app));
        for effect in effects {
            match effect {
                Effect::Hear(ear, said) => heard.push((ear, said)),
                Effect::Listening(answered) => answer = Some(answered),
                effect => inputs.extend(shell.perform(app, effect)),
            }
        }
    }
    drop(shell);
    for (ear, said) in heard {
        (ear.0)(said);
    }
    answer
}

impl Shell {
    /// Carries out `effect`, and says what became of it where the session
    /// is to know.
    fn perform(&mut self, app: &AppHandle, effect: Effect) -> Option<Input> {
        match effect {
            Effect::Tell(told) => crate::slint_windows::pill::tell(told),
            // Without the pill, as when Slint could not start, the
            // recording goes on all the same.
            Effect::ShowPill => crate::slint_windows::pill::show(app),
            Effect::HidePill => crate::slint_windows::pill::hide(),
            Effect::HideAfter(after) => {
                let app = app.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(after);
                    run(&app, Input::Lingered);
                });
            }
            Effect::Start { recording, opener, source, language } => {
                let receiver = app.clone();
                let tell = Arc::new(move |event| drop(run(&receiver, Input::Recorded(recording, event))));
                match listening::listen(&source, opener, &language, tell) {
                    Ok(listening) => {
                        self.listening = Some((recording, listening));
                        return Some(Input::Started(recording));
                    }
                    Err(why) => return Some(Input::Unstarted(recording, why)),
                }
            }
            Effect::Write { recording, live, polishing } => {
                let mailbox = Arc::new(Mailbox::default());
                match write(app, mailbox.clone(), live, polishing, recording) {
                    Ok(()) => self.writing = Some((recording, mailbox)),
                    Err(e) => eprintln!("heyo: dictation has nothing to write with: {e}"),
                }
            }
            Effect::WatchKeys(recording) => watch_keys(app, recording),
            Effect::Stop(recording) => {
                let listening = self.listening.take_if(|(listening, _)| *listening == recording);
                let stopped = listening.ok_or_else(|| crate::i18n::t("dictation is not running").to_owned()).and_then(|(_, listening)| listening.stop());
                if let Err(why) = stopped {
                    return Some(Input::Unstopped(recording, why));
                }
            }
            Effect::Discard(recording) => {
                if let Some((_, listening)) = self.listening.take_if(|(listening, _)| *listening == recording) {
                    listening.cancel();
                }
            }
            Effect::Post(recording, said) => match &self.writing {
                Some((writing, mailbox)) if *writing == recording => mailbox.post(said),
                // Nothing is writing: the recording ends as it is.
                _ => {
                    if let Said::Ended(end) = said {
                        return Some(Input::Written(recording, end.told()));
                    }
                }
            },
            Effect::Polish { recording, raw, instructions, words } => {
                let model = crate::models::Model::for_use(&app.state::<SettingsState>().0.read().unwrap(), registry::Use::DictationPolish);
                let app = app.clone();
                self.polishing = Some(tauri::async_runtime::spawn(async move {
                    let outcome = match &model {
                        Ok(model) => polish::polished(model, &raw, instructions, &words.corrector).await,
                        Err(why) => polish::unpolished(&raw, why),
                    };
                    run(&app, Input::Polished(recording, outcome));
                }));
            }
            Effect::StopPolishing => {
                if let Some(polishing) = self.polishing.take() {
                    polishing.abort();
                }
            }
            Effect::Hear(..) | Effect::Listening(_) => unreachable!("said once nothing is held"),
        }
        None
    }
}

/// Waits for the keys that are down to be released: those of the hotkey
/// that started recording `recording`.
fn watch_keys(app: &AppHandle, recording: u64) {
    let app = app.clone();
    std::thread::spawn(move || {
        if let Some(keys) = crate::windows::keys_down().filter(|keys| keys.iter().any(|b| *b != 0)) {
            loop {
                std::thread::sleep(Duration::from_millis(30));
                let Some(now) = crate::windows::keys_down() else { break };
                if keys.iter().zip(now).any(|(was, is)| was & !is != 0) {
                    break;
                }
            }
        }
        run(&app, Input::LetGo(recording));
    });
}

/// A mouse button went down on the dictation window. That moves no cursor
/// in the application being written into, and must not be taken for the
/// user's doing something there.
pub fn pressed(app: &AppHandle) {
    if let Some((_, writing)) = &app.state::<Dictation>().0.lock().unwrap().writing {
        writing.own_presses.fetch_add(1, Ordering::SeqCst);
    }
}

/// Starts dictating, or finishes if a recording is in progress.
pub fn toggle(app: &AppHandle) {
    run(app, Input::Toggle);
}

/// Discards the recording in progress.
pub fn cancel(app: &AppHandle) {
    run(app, Input::Cancel);
}

/// Starts a recording for an extension: `listener` is given each sentence
/// and, at the end, all of it; nothing is pasted. The recording ends like
/// any other (the hotkey, the window's buttons) or is discarded
/// (`cancel_listening`). Returns the recording's number.
pub(crate) fn listen(app: &AppHandle, language: Option<String>, indicator: bool, listener: impl Fn(Heard) + Send + Sync + 'static) -> Result<u64, NotListening> {
    let ear = session::Ear(Arc::new(listener));
    run(app, Input::Listen { language, indicator, ear }).unwrap_or(Err(NotListening::Busy))
}

/// Ends the recording an extension started, if it is still the one going:
/// what was said comes to its listener.
pub(crate) fn stop_listening(app: &AppHandle, recording: u64) {
    run(app, Input::StopListening(recording));
}

/// Discards the recording an extension started, if it is still the one going.
pub(crate) fn cancel_listening(app: &AppHandle, recording: u64) {
    run(app, Input::CancelListening(recording));
}

/// Dictates while the hold-to-talk key from the settings is held.
pub fn watch_hold_key(app: &AppHandle) {
    use crate::platform::keys::{watch_hold, Hold};
    let app = app.clone();
    std::thread::spawn(move || {
        let setting = app.clone();
        let key = move || setting.state::<SettingsState>().0.read().unwrap().dictation_hold_key.clone();
        let result = watch_hold(key, |hold| {
            run(
                &app,
                match hold {
                    Hold::Started => Input::HoldStarted,
                    Hold::Released => Input::HoldReleased,
                    Hold::Cancelled => Input::Cancel,
                },
            );
        });
        if let Err(e) = result {
            eprintln!("heyo: {e}");
        }
    });
}

/// Whether dictation can be used, for the settings page.
#[derive(Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../shared/bindings/", rename = "DictationStatus")]
pub struct Status {
    installed: bool,
    installing: bool,
    microphones: Vec<microphone::Microphone>,
}

pub fn status(app: &AppHandle) -> Status {
    Status { installed: ready(&app.state::<SettingsState>().0.read().unwrap()), installing: models::downloads::any(), microphones: microphone::microphones() }
}

/// The speech models, and what each is to this installation.
pub(crate) fn models(app: &AppHandle) -> Vec<models::downloads::ModelState> {
    models::downloads::listed(in_use(app).as_deref())
}

/// Downloads a speech model (`models::downloads`).
pub(crate) fn download(app: &AppHandle, id: &str) -> Result<(), String> {
    models::downloads::start(app, id)
}

/// Stops a model's download; what it fetched so far is thrown away.
pub(crate) fn cancel_download(_app: &AppHandle, id: &str) -> Result<(), String> {
    models::downloads::cancel(id)
}

/// Deletes a model's files, unless recordings are heard with it: another
/// is chosen first.
pub(crate) fn delete(app: &AppHandle, id: &str) -> Result<(), String> {
    models::downloads::delete(id, in_use(app).as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::pasting::scripted::{clipboard, desk, happened};
    use crate::platform::desktop::fake::Fake;
    use crate::platform::desktop::Supported;
    use crate::platform::Support;
    use serde_json::{json, Value};
    use session::Outcome;

    const HOTKEY: &str = "<Control><Alt>d";
    const SECOND: Duration = Duration::from_secs(1);

    /// A writer for the window in front of a desk (`pasting::scripted`),
    /// which takes words as they are spoken.
    fn writer(pasting: &'static pasting::Pasting, live: bool, polishing: bool) -> Writer {
        Writer::new(None, Paster::new(live, polishing, pasting.borrow(), Arc::default(), Box::new(|| Ok(vec![HOTKEY.to_owned()]))))
    }

    /// What the recording said, line by line: `~text` is the sentence in
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
        serde_json::to_value(writer.end(End::Over(Outcome { text: text.into(), raw: raw.into(), polished: text != raw, warning: None }))).unwrap()
    }

    fn cancel(writer: &mut Writer) -> Value {
        serde_json::to_value(writer.end(End::Cancelled)).unwrap()
    }

    /// The recording breaks off; what the window is told of it.
    fn break_off(writer: &mut Writer) -> Told {
        writer.end(End::Failed("the sound server stopped the recording".into()))
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
            assert_eq!(break_off(&mut writer), Told::Error { message: "the sound server stopped the recording".into() });
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
            assert_eq!(session::linger(&serde_json::from_value(last).unwrap()), Duration::from_millis(2500));
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
            assert_eq!(session::linger(&serde_json::from_value(last).unwrap()), Duration::from_millis(300));
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
                    Said::Ended(end) => format!("{end:?}"),
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
        mailbox.post(Said::Ended(End::Cancelled));
        assert_eq!(waiting(&mailbox), ["The weather is nice.", " And so on.", " Third.", "Cancelled"]);
    }

    #[test]
    fn an_error_ends_a_recording_without_another_word_written() {
        let (desktop, _, pasting) = desk("firefox", None);
        desktop.during(|| {
            let mut writer = writer(pasting, true, false);
            hear(&mut writer, &["~Hello"]);
            asked(desktop);
            assert_eq!(break_off(&mut writer), Told::Error { message: "the sound server stopped the recording".into() });
            assert_eq!(asked(desktop), Vec::<String>::new());
            assert_eq!(desktop.field(), "Hello");
        });
    }

    /// Everything a recording says (`listening`), and what the host says
    /// itself, is something the window is told.
    #[test]
    fn the_window_is_told_what_the_recording_and_the_host_say() {
        let told = |event: Value| serde_json::from_value::<Told>(event);
        for event in [
            json!({ "type": "start", "polish": "off", "cloud": false }),
            json!({ "type": "start", "polish": "clean", "cloud": true }),
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

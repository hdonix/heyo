//! The life of dictation's recordings, one after the other: what starts
//! one, what finishes it or discards it, what is done with what it hears,
//! when it is polished and when it is over, and what the pill is told
//! meanwhile. Whether it is the user's, written into the application in
//! front, or an extension's, which is given what is heard.
//!
//! The [`Session`] is plain data with the time passed in, in the manner of
//! `activities::Table`: it takes an [`Input`] (a hotkey was pressed, the
//! recording heard a sentence, the polishing is done...) and answers with
//! the [`Effect`]s it calls for (start a recording with this opener, show
//! the pill, post to the writer, polish...). The shell in `dictation`
//! carries them out, and is the only part that holds the application. So
//! the order of things (the pill hears of a recording before it can hear
//! that it is ready; a repeat of the hotkey after a hold neither finishes
//! nor starts anything) is tested here without a microphone or a window.

use std::sync::Arc;
use std::time::{Duration, Instant};

use super::engine::{Failure, Opener};
use super::listening::Event;
use super::polish;
use super::vocabulary::{self, Corrector, Vocabulary};
use super::Told;
use crate::i18n::t;

/// Holding the hotkey longer than this makes it push-to-talk: releasing it
/// finishes. A shorter press leaves the recording running until the next one.
pub(crate) const HOLD: Duration = Duration::from_millis(450);

/// How long after a hotkey held to talk is let go a press of it is taken
/// for one of its repeats: they can reach Heyo (as `heyo --dictate`, each a
/// process of its own) after the release, and even after the recording it
/// finished is over, where one would start another.
pub(crate) const REPEATS_FOR: Duration = Duration::from_secs(1);

/// How long the pill shows what went wrong before a recording could start.
const FAILED_FOR: Duration = Duration::from_secs(4);

/// How long the pill stays once an extension's recording is over.
const LISTENED_FOR: Duration = Duration::from_millis(300);

/// The settings a recording starts with, or finishes with (`polish`), as
/// they are at that moment.
pub(crate) struct Setup {
    /// What hears it, or why nothing can (`dictation::opener`).
    pub opener: Result<Box<dyn Opener>, String>,
    /// The microphone (`dictationSource`); empty for the system's default.
    pub source: String,
    pub language: String,
    /// How it is polished (`dictationPolish`): `off`, `clean`, `translate`.
    pub polish: String,
    pub vocabulary: Vec<String>,
    /// The words are wanted in the application as they are spoken.
    pub live: bool,
}

/// A vocabulary, and the correction pass made of it.
#[derive(Default)]
pub(crate) struct Words {
    pub vocabulary: Vocabulary,
    pub corrector: Corrector,
}

impl Words {
    pub fn of(lines: &[String]) -> Arc<Words> {
        let vocabulary = vocabulary::parse(lines);
        Arc::new(Words { corrector: Corrector::new(&vocabulary), vocabulary })
    }
}

impl std::fmt::Debug for Words {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("Words")
    }
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

/// Why listening could not start.
#[derive(Debug, PartialEq)]
pub(crate) enum NotListening {
    /// A recording is in progress, and there is one microphone.
    Busy,
    /// The speech engine has not been downloaded.
    NotInstalled,
    Failed(String),
}

/// Who is given what an extension's recording hears.
#[derive(Clone)]
pub(crate) struct Ear(pub Arc<dyn Fn(Heard) + Send + Sync>);

impl std::fmt::Debug for Ear {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("Ear")
    }
}

/// What a recording that was heard to its end came to.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Outcome {
    /// What should stand in the application.
    pub text: String,
    /// What was heard.
    pub raw: String,
    pub polished: bool,
    /// Why it was not polished.
    pub warning: Option<String>,
}

impl Outcome {
    /// What was heard, as it was heard.
    pub fn heard(raw: &str) -> Outcome {
        Outcome { text: raw.into(), raw: raw.into(), polished: false, warning: None }
    }
}

/// How a recording of the user's ends, as its writer is told.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum End {
    Over(Outcome),
    Cancelled,
    /// It broke off, for the reason in words.
    Failed(String),
}

impl End {
    /// What the pill is told of it, before the writer adds what it could
    /// not do.
    pub fn told(&self) -> Told {
        match self {
            End::Over(outcome) => Told::Result { text: outcome.text.clone(), raw: outcome.raw.clone(), warning: outcome.warning.clone(), cancelled: None, kept: None },
            End::Cancelled => Told::Result { text: String::new(), raw: String::new(), warning: None, cancelled: Some(true), kept: None },
            End::Failed(message) => Told::Error { message: message.clone() },
        }
    }
}

/// What the thread that writes is told, in the order it was heard.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Said {
    /// The sentence in progress.
    Partial(String),
    /// A finished sentence.
    Sentence(String),
    /// The recording's last word.
    Ended(End),
}

/// What happened.
#[derive(Debug)]
pub(crate) enum Input {
    /// The dictation hotkey (`heyo --dictate`), or the pill's button to
    /// finish: starts a recording, or finishes the one going.
    Toggle,
    /// The hold-to-talk key went down...
    HoldStarted,
    /// ...and was let go.
    HoldReleased,
    /// The keys that were down when the hotkey started recording `n`
    /// (`Effect::WatchKeys`) are let go.
    LetGo(u64),
    /// Discard the recording, or its polishing.
    Cancel,
    /// An extension asks to listen: `ear` is given what is heard; the
    /// pill is shown with `indicator`.
    Listen { language: Option<String>, indicator: bool, ear: Ear },
    /// The extension that started recording `n` ends it...
    StopListening(u64),
    /// ...or discards it.
    CancelListening(u64),
    /// Recording `n` started (`Effect::Start`)...
    Started(u64),
    /// ...or could not, for the reason in words.
    Unstarted(u64, String),
    /// Recording `n` could not be told to stop: it is gone.
    Unstopped(u64, String),
    /// Recording `n` says something.
    Recorded(u64, Event),
    /// Recording `n` is polished (`Effect::Polish`).
    Polished(u64, Outcome),
    /// All of recording `n` is written into the application
    /// (`Effect::Write`): its last word, as the pill is told it.
    Written(u64, Told),
    /// A wait the session asked for (`Effect::HideAfter`) is over.
    Lingered,
}

/// What the session calls for.
#[derive(Debug)]
pub(crate) enum Effect {
    /// Tell the pill.
    Tell(Told),
    ShowPill,
    /// Hide the pill after a while: `Lingered` comes then.
    HideAfter(Duration),
    HidePill,
    /// Start recording number `recording` from `source`, heard by what
    /// `opener` opens in `language`: `Started` or `Unstarted` comes.
    Start { recording: u64, opener: Box<dyn Opener>, source: String, language: String },
    /// Start writing recording `recording` into the application: `Written`
    /// comes at its end.
    Write { recording: u64, live: bool, polishing: bool },
    /// Watch the keys that are down now: `LetGo` comes when they are let go.
    WatchKeys(u64),
    /// End the recording: what was said is wanted. `Unstopped` comes if
    /// it cannot be told.
    Stop(u64),
    /// End the recording without what was said.
    Discard(u64),
    /// Tell the writer of the recording.
    Post(u64, Said),
    /// Have `raw` polished as `instructions` say: `Polished` comes.
    Polish { recording: u64, raw: String, instructions: String, words: Arc<Words> },
    /// Stop the polishing.
    StopPolishing,
    /// Tell the extension listening.
    Hear(Ear, Heard),
    /// Answer the extension that asked to listen.
    Listening(Result<u64, NotListening>),
}

/// Whose a recording is.
#[derive(Debug)]
enum Whose {
    /// The user's: written into the application. `watched`: the hotkey
    /// that started it may be held to talk (`WatchKeys`); the hold-to-talk
    /// key does its own watching.
    User { live: bool, polishing: bool, watched: bool },
    /// An extension's, which hears it, with the pill `shown` or not.
    Extension { ear: Ear, shown: bool },
}

/// How far a recording is.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Stage {
    /// Told to start, not started yet.
    Starting,
    Listening,
    /// Told to end: its last word is to come.
    Stopping,
    Polishing,
    /// Its last word is with the writer.
    Writing,
}

#[derive(Debug)]
struct Recording {
    number: u64,
    whose: Whose,
    stage: Stage,
    /// The hotkey that started it has not been let go yet.
    holding: bool,
    /// When it started.
    began: Instant,
    /// Written the user's way, for all of it, polishing included.
    words: Arc<Words>,
    /// How it is polished, as it was set when it was finished.
    polish: String,
}

/// Dictation's recordings: one at a time, and how far it is.
#[derive(Debug, Default)]
pub(crate) struct Session {
    /// Counts recordings: what is said of one that is over is not taken
    /// for the next.
    count: u64,
    recording: Option<Recording>,
    /// When a hotkey held to talk was let go and so finished the recording.
    let_go: Option<Instant>,
}

impl Session {
    /// Takes `input` at `now`. `setup` reads the settings, where they are
    /// needed.
    pub fn on(&mut self, input: Input, now: Instant, setup: &dyn Fn() -> Setup) -> Vec<Effect> {
        let mut effects = Vec::new();
        match input {
            Input::Toggle => match &self.recording {
                None if self.let_go.is_some_and(|at| now.saturating_duration_since(at) < REPEATS_FOR) => {}
                None => self.begin(setup(), true, now, &mut effects),
                // While the hotkey is held, key repeat presses it again.
                Some(recording) if recording.holding => {}
                Some(_) => self.finish(setup, &mut effects),
            },
            Input::HoldStarted => {
                if self.recording.is_none() {
                    self.begin(setup(), false, now, &mut effects);
                }
            }
            Input::HoldReleased => self.finish(setup, &mut effects),
            Input::LetGo(number) => {
                let Some(recording) = self.current(number) else { return effects };
                recording.holding = false;
                if now.saturating_duration_since(recording.began) >= HOLD {
                    self.let_go = Some(now);
                    self.finish(setup, &mut effects);
                }
            }
            Input::Cancel => self.cancel(&mut effects),
            Input::Listen { language, indicator, ear } => self.listen(setup(), language, indicator, ear, now, &mut effects),
            Input::StopListening(number) => {
                if self.listens(number) {
                    self.finish(setup, &mut effects);
                }
            }
            Input::CancelListening(number) => {
                if self.listens(number) {
                    self.cancel(&mut effects);
                }
            }
            Input::Started(number) => self.started(number, now, &mut effects),
            Input::Unstarted(number, why) => {
                let Some(recording) = self.current(number).filter(|recording| recording.stage == Stage::Starting) else { return effects };
                let extension = matches!(recording.whose, Whose::Extension { .. });
                self.recording = None;
                match extension {
                    true => effects.push(Effect::Listening(Err(NotListening::Failed(why)))),
                    false => fail(why, &mut effects),
                }
            }
            Input::Unstopped(number, why) => {
                if self.current(number).is_some() {
                    eprintln!("heyo: {why}");
                    self.ended(End::Failed(why), &mut effects);
                }
            }
            Input::Recorded(number, event) => self.recorded(number, event, &mut effects),
            Input::Polished(number, outcome) => {
                if self.current(number).is_some_and(|recording| recording.stage == Stage::Polishing) {
                    self.ended(End::Over(outcome), &mut effects);
                }
            }
            Input::Written(number, told) => {
                if self.current(number).is_some() {
                    self.recording = None;
                    effects.extend([Effect::HideAfter(linger(&told)), Effect::Tell(told)]);
                }
            }
            // A new recording may have started meanwhile.
            Input::Lingered => {
                if self.recording.is_none() {
                    effects.push(Effect::HidePill);
                }
            }
        }
        effects
    }

    /// Whether recording `number` is going, and the hotkey that started it
    /// is still down: no field has the focus meanwhile.
    pub fn holding(&self, number: u64) -> bool {
        self.recording.as_ref().is_some_and(|recording| recording.number == number && recording.holding)
    }

    /// Whether recording `number` is going, and an extension's: only then
    /// may that extension end it.
    pub fn listens(&self, number: u64) -> bool {
        self.recording.as_ref().is_some_and(|recording| recording.number == number && matches!(recording.whose, Whose::Extension { .. }))
    }

    fn current(&mut self, number: u64) -> Option<&mut Recording> {
        self.recording.as_mut().filter(|recording| recording.number == number)
    }

    /// Starts a recording of the user's (`Whose::User`, for `watched`).
    fn begin(&mut self, setup: Setup, watched: bool, now: Instant, effects: &mut Vec<Effect>) {
        let opener = match setup.opener {
            Ok(opener) if opener.ready() => opener,
            Ok(_) => return fail(t("Dictation is not set up yet. Open Heyo Settings → Models to download a speech model.").into(), effects),
            Err(why) => return fail(why, effects),
        };
        self.count += 1;
        // Before the recording starts: a recognizer that is kept answers at
        // once, and the pill must not hear that it is ready before it hears
        // that a recording begins.
        effects.push(Effect::Tell(Told::Start { polish: setup.polish.clone(), cloud: opener.sends_audio() }));
        effects.push(Effect::Start { recording: self.count, opener, source: setup.source, language: setup.language });
        let whose = Whose::User { live: setup.live, polishing: setup.polish != "off", watched };
        self.recording = Some(Recording { number: self.count, whose, stage: Stage::Starting, holding: false, began: now, words: Words::of(&setup.vocabulary), polish: "off".into() });
    }

    /// Starts a recording for an extension, in `language` or the one set.
    fn listen(&mut self, setup: Setup, language: Option<String>, indicator: bool, ear: Ear, now: Instant, effects: &mut Vec<Effect>) {
        if self.recording.is_some() {
            return effects.push(Effect::Listening(Err(NotListening::Busy)));
        }
        let opener = match setup.opener {
            Ok(opener) if opener.ready() => opener,
            Ok(_) => return effects.push(Effect::Listening(Err(NotListening::NotInstalled))),
            Err(why) => return effects.push(Effect::Listening(Err(NotListening::Failed(why)))),
        };
        self.count += 1;
        if indicator {
            effects.push(Effect::Tell(Told::Start { polish: "off".into(), cloud: opener.sends_audio() }));
        }
        effects.push(Effect::Start { recording: self.count, opener, source: setup.source, language: language.unwrap_or(setup.language) });
        // The pill is shown once it has started.
        let whose = Whose::Extension { ear, shown: indicator };
        self.recording = Some(Recording { number: self.count, whose, stage: Stage::Starting, holding: false, began: now, words: Words::of(&setup.vocabulary), polish: "off".into() });
    }

    fn started(&mut self, number: u64, now: Instant, effects: &mut Vec<Effect>) {
        let Some(recording) = self.current(number).filter(|recording| recording.stage == Stage::Starting) else { return };
        recording.stage = Stage::Listening;
        recording.began = now;
        match recording.whose {
            Whose::User { live, polishing, watched } => {
                effects.push(Effect::Write { recording: number, live, polishing });
                if watched {
                    // The hotkey that started this is usually still down.
                    recording.holding = true;
                    effects.push(Effect::WatchKeys(number));
                }
                effects.push(Effect::ShowPill);
            }
            Whose::Extension { shown, .. } => {
                effects.push(Effect::Listening(Ok(number)));
                if shown {
                    effects.push(Effect::ShowPill);
                }
            }
        }
    }

    /// Finishes the recording in progress: what was said is written, or
    /// given to the extension listening.
    fn finish(&mut self, setup: &dyn Fn() -> Setup, effects: &mut Vec<Effect>) {
        // Not recording, or told to finish already and finishing: the hotkey
        // repeats while it is held, and each repeat may come as a `heyo
        // --dictate` after the release that finished it.
        let Some(recording) = self.recording.as_mut().filter(|recording| recording.stage == Stage::Listening) else { return };
        // An extension is given what was said, as it was said.
        recording.polish = match recording.whose {
            Whose::User { .. } => setup().polish,
            Whose::Extension { .. } => "off".into(),
        };
        recording.stage = Stage::Stopping;
        effects.push(Effect::Stop(recording.number));
    }

    /// Discards the recording in progress, or stops its polishing.
    fn cancel(&mut self, effects: &mut Vec<Effect>) {
        let Some(recording) = self.recording.as_mut() else { return };
        match recording.stage {
            // The recording is over, and the polishing stops.
            Stage::Polishing => {
                effects.push(Effect::StopPolishing);
                self.ended(End::Cancelled, effects);
            }
            // It says it was cancelled.
            Stage::Listening => {
                recording.stage = Stage::Stopping;
                effects.push(Effect::Discard(recording.number));
            }
            Stage::Starting | Stage::Stopping | Stage::Writing => {}
        }
    }

    fn recorded(&mut self, number: u64, event: Event, effects: &mut Vec<Effect>) {
        let Some(recording) = self.current(number) else { return };
        let words = recording.words.clone();
        let correct = |text: &str| words.corrector.correct(text);
        let ear = match &recording.whose {
            Whose::Extension { ear, .. } => Some(ear.clone()),
            Whose::User { .. } => None,
        };
        match event {
            Event::Ready => effects.push(Effect::Tell(Told::Ready)),
            Event::Level(value) => effects.push(Effect::Tell(Told::Level { value: Some(value) })),
            Event::Partial(text) => {
                let text = correct(&text);
                if ear.is_none() {
                    effects.push(Effect::Post(number, Said::Partial(text.clone())));
                }
                effects.push(Effect::Tell(Told::Partial { text }));
            }
            Event::Sentence(text) => {
                let text = correct(&text);
                effects.push(match ear {
                    // Without the space that sets it apart from the sentence before.
                    Some(ear) => Effect::Hear(ear, Heard::Segment(text.trim_start().to_owned())),
                    None => Effect::Post(number, Said::Sentence(text.clone())),
                });
                effects.push(Effect::Tell(Told::Segment { text }));
            }
            Event::Heard(raw) => {
                let raw = correct(&raw);
                if let Some(ear) = ear {
                    return self.listened(ear, Heard::Result(raw.clone()), End::Over(Outcome::heard(&raw)).told(), effects);
                }
                match polish::instructions(&recording.polish, &words.vocabulary).filter(|_| !raw.is_empty()) {
                    Some(instructions) => {
                        recording.stage = Stage::Polishing;
                        effects.push(Effect::Tell(Told::Polishing));
                        effects.push(Effect::Polish { recording: number, raw, instructions, words });
                    }
                    None => self.ended(End::Over(Outcome::heard(&raw)), effects),
                }
            }
            Event::Cancelled => match ear {
                Some(ear) => self.listened(ear, Heard::Cancelled, End::Cancelled.told(), effects),
                None => self.ended(End::Cancelled, effects),
            },
            Event::Failed(failure) => {
                let message = worded(&failure);
                match ear {
                    Some(ear) => self.listened(ear, Heard::Failed(message.clone()), Told::Error { message }, effects),
                    None => self.ended(End::Failed(message), effects),
                }
            }
        }
    }

    /// An extension's recording is over: it hears the last of it.
    fn listened(&mut self, ear: Ear, heard: Heard, told: Told, effects: &mut Vec<Effect>) {
        self.recording = None;
        effects.extend([Effect::HideAfter(LISTENED_FOR), Effect::Hear(ear, heard), Effect::Tell(told)]);
    }

    /// The user's recording is over: the pill hears of it once all of it
    /// is written (`Written`). An extension's ends at once.
    fn ended(&mut self, end: End, effects: &mut Vec<Effect>) {
        let Some(recording) = self.recording.as_mut() else { return };
        match &recording.whose {
            Whose::User { .. } => {
                recording.stage = Stage::Writing;
                effects.push(Effect::Post(recording.number, Said::Ended(end)));
            }
            Whose::Extension { ear, .. } => {
                let heard = match &end {
                    End::Over(outcome) => Heard::Result(outcome.text.clone()),
                    End::Cancelled => Heard::Cancelled,
                    End::Failed(message) => Heard::Failed(message.clone()),
                };
                let ear = ear.clone();
                self.listened(ear, heard, end.told(), effects);
            }
        }
    }
}

/// Says what went wrong before a recording could start.
fn fail(message: String, effects: &mut Vec<Effect>) {
    eprintln!("heyo: {message}");
    effects.extend([Effect::ShowPill, Effect::Tell(Told::Error { message }), Effect::HideAfter(FAILED_FOR)]);
}

/// What went wrong with a recording, in words that fit the pill
/// (`helper::worded`); all of it goes to the log.
fn worded(failure: &Failure) -> String {
    let worded = crate::helper::worded(&failure.said(), t("Dictation failed"));
    eprintln!("heyo: dictation: {}", worded.full());
    worded.short
}

/// How long the pill stays once the recording is over: longer where
/// there is something to read.
pub(crate) fn linger(last: &Told) -> Duration {
    match last {
        // A warning, having heard nothing, or how a cancelled recording ended.
        Told::Result { warning, raw, .. } if warning.is_some() || raw.is_empty() => Duration::from_millis(2500),
        Told::Result { .. } => Duration::from_millis(300),
        _ => FAILED_FOR,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// An opener that opens nothing: the session only asks it questions.
    #[derive(Debug)]
    struct Fake {
        ready: bool,
        cloud: bool,
    }

    impl Opener for Fake {
        fn open(&self, _language: &str) -> Result<Box<dyn super::super::engine::Recognizer>, Failure> {
            Err(Failure::failed("not here"))
        }
        fn kept(&self) -> Option<&str> {
            None
        }
        fn ready(&self) -> bool {
            self.ready
        }
        fn sends_audio(&self) -> bool {
            self.cloud
        }
        fn languages(&self) -> &'static [&'static str] {
            &[]
        }
    }

    /// The settings of a test, as `Setup` makes them for each input.
    #[derive(Clone)]
    struct Set {
        opener: Result<(bool, bool), String>,
        polish: &'static str,
        vocabulary: Vec<String>,
    }

    impl Default for Set {
        fn default() -> Set {
            Set { opener: Ok((true, false)), polish: "off", vocabulary: Vec::new() }
        }
    }

    /// A session at a time of the test's, with settings of its own.
    struct Run {
        session: Session,
        start: Instant,
        now: Instant,
        set: Set,
    }

    impl Run {
        fn new() -> Run {
            Run::with(Set::default())
        }

        fn with(set: Set) -> Run {
            let start = Instant::now();
            Run { session: Session::default(), start, now: start, set }
        }

        fn pass(&mut self, millis: u64) {
            self.now += Duration::from_millis(millis);
        }

        /// What the session called for, a line each.
        fn on(&mut self, input: Input) -> Vec<String> {
            let set = self.set.clone();
            let setup = move || Setup {
                opener: set.opener.clone().map(|(ready, cloud)| Box::new(Fake { ready, cloud }) as Box<dyn Opener>),
                source: "mic".into(),
                language: "auto".into(),
                polish: set.polish.into(),
                vocabulary: set.vocabulary.clone(),
                live: true,
            };
            self.session.on(input, self.now, &setup).iter().map(line).collect()
        }

        /// Starts a recording with the hotkey, as the shell does it: the
        /// recording starts and its keys are watched.
        fn toggle_on(&mut self) -> u64 {
            self.on(Input::Toggle);
            let number = self.session.count;
            self.on(Input::Started(number));
            number
        }
    }

    /// An effect in a line: what the tests compare.
    fn line(effect: &Effect) -> String {
        match effect {
            Effect::Tell(told) => format!("tell {}", serde_json::to_value(told).unwrap()),
            Effect::ShowPill => "show".into(),
            Effect::HideAfter(after) => format!("hide after {}", after.as_millis()),
            Effect::HidePill => "hide".into(),
            Effect::Start { recording, opener, source, language } => format!("start {recording} from {source} in {language}{}", if opener.sends_audio() { " sent" } else { "" }),
            Effect::Write { recording, live, polishing } => format!("write {recording}{}{}", if *live { " live" } else { "" }, if *polishing { " polishing" } else { "" }),
            Effect::WatchKeys(recording) => format!("watch keys {recording}"),
            Effect::Stop(recording) => format!("stop {recording}"),
            Effect::Discard(recording) => format!("discard {recording}"),
            Effect::Post(recording, said) => format!("post {recording} {said:?}"),
            Effect::Polish { recording, raw, .. } => format!("polish {recording} {raw}"),
            Effect::StopPolishing => "stop polishing".into(),
            Effect::Hear(_, heard) => format!("hear {heard:?}"),
            Effect::Listening(answer) => format!("listening {answer:?}"),
        }
    }

    fn heard(text: &str) -> Input {
        Input::Recorded(1, Event::Heard(text.into()))
    }

    #[test]
    fn a_press_starts_a_recording_and_the_next_finishes_it() {
        let mut run = Run::new();
        // The pill hears of the recording before the recording can say it
        // is ready (5b599ea): a kept recognizer answers at once.
        assert_eq!(run.on(Input::Toggle), [r#"tell {"cloud":false,"polish":"off","type":"start"}"#, "start 1 from mic in auto"]);
        assert_eq!(run.on(Input::Started(1)), ["write 1 live", "watch keys 1", "show"]);
        assert!(run.session.holding(1));
        // A short press: let go before it is a hold, it keeps recording.
        run.pass(100);
        assert_eq!(run.on(Input::LetGo(1)), Vec::<String>::new());
        assert!(!run.session.holding(1));
        assert_eq!(run.on(Input::Recorded(1, Event::Ready)), [r#"tell {"type":"ready"}"#]);
        assert_eq!(run.on(Input::Recorded(1, Event::Partial("Hel".into()))), [r#"post 1 Partial("Hel")"#, r#"tell {"text":"Hel","type":"partial"}"#]);
        assert_eq!(run.on(Input::Recorded(1, Event::Sentence("Hello.".into()))), [r#"post 1 Sentence("Hello.")"#, r#"tell {"text":"Hello.","type":"segment"}"#]);
        run.pass(2000);
        assert_eq!(run.on(Input::Toggle), ["stop 1"]);
        assert_eq!(run.on(heard("Hello.")), [r#"post 1 Ended(Over(Outcome { text: "Hello.", raw: "Hello.", polished: false, warning: None }))"#]);
        let told = Told::Result { text: "Hello.".into(), raw: "Hello.".into(), warning: None, cancelled: None, kept: None };
        assert_eq!(run.on(Input::Written(1, told)), ["hide after 300", r#"tell {"raw":"Hello.","text":"Hello.","type":"result"}"#]);
        assert_eq!(run.on(Input::Lingered), ["hide"]);
        // The next press starts the next one.
        assert_eq!(run.on(Input::Toggle)[1], "start 2 from mic in auto");
    }

    #[test]
    fn a_hotkey_held_to_talk_finishes_when_it_is_let_go_and_its_repeats_do_nothing() {
        let mut run = Run::new();
        let number = run.toggle_on();
        // Key repeat while it is held.
        run.pass(200);
        assert_eq!(run.on(Input::Toggle), Vec::<String>::new());
        run.pass(400);
        assert_eq!(run.on(Input::LetGo(number)), ["stop 1"]);
        // A repeat that comes after the release while it finishes (c7135f3)
        // does not end what is being written.
        run.pass(36);
        assert_eq!(run.on(Input::Toggle), Vec::<String>::new());
        run.on(heard("Hello."));
        run.on(Input::Written(number, Told::Error { message: "x".into() }));
        // Nor does one after it is over start another (ac0362c)...
        run.pass(100);
        assert_eq!(run.on(Input::Toggle), Vec::<String>::new());
        // ...until a second has gone by.
        run.pass(1000);
        assert_eq!(run.on(Input::Toggle)[1], "start 2 from mic in auto");
        let _ = run.start;
    }

    #[test]
    fn the_hold_to_talk_key_records_while_it_is_down() {
        let mut run = Run::new();
        assert_eq!(run.on(Input::HoldStarted)[1], "start 1 from mic in auto");
        // It watches itself: nothing else is.
        assert_eq!(run.on(Input::Started(1)), ["write 1 live", "show"]);
        assert_eq!(run.on(Input::HoldStarted), Vec::<String>::new());
        assert_eq!(run.on(Input::HoldReleased), ["stop 1"]);
        assert_eq!(run.on(Input::HoldReleased), Vec::<String>::new());
    }

    #[test]
    fn what_cannot_start_says_why_and_starts_nothing() {
        let mut run = Run::with(Set { opener: Err("Doubao has no key".into()), ..Set::default() });
        assert_eq!(run.on(Input::Toggle), ["show", r#"tell {"message":"Doubao has no key","type":"error"}"#, "hide after 4000"]);
        let mut run = Run::with(Set { opener: Ok((false, false)), ..Set::default() });
        assert!(run.on(Input::Toggle)[1].contains("not set up yet"));
        assert_eq!(run.on(Input::Lingered), ["hide"]);
        // The recording itself could not start.
        let mut run = Run::new();
        run.on(Input::Toggle);
        assert_eq!(run.on(Input::Unstarted(1, "no thread".into())), ["show", r#"tell {"message":"no thread","type":"error"}"#, "hide after 4000"]);
        assert_eq!(run.on(Input::Toggle)[1], "start 2 from mic in auto");
    }

    #[test]
    fn the_pill_is_told_whether_the_audio_is_sent_away() {
        let mut run = Run::with(Set { opener: Ok((true, true)), polish: "clean", ..Set::default() });
        assert_eq!(run.on(Input::Toggle), [r#"tell {"cloud":true,"polish":"clean","type":"start"}"#, "start 1 from mic in auto sent"]);
        assert_eq!(run.on(Input::Started(1))[0], "write 1 live polishing");
    }

    #[test]
    fn what_was_heard_is_written_the_users_way_and_polished_as_set_when_it_was_finished() {
        let mut run = Run::with(Set { vocabulary: vec!["Heyo".into(), "jason -> JSON".into()], ..Set::default() });
        let number = run.toggle_on();
        assert_eq!(run.on(Input::Recorded(number, Event::Partial(" open heyo".into())))[0], r#"post 1 Partial(" open Heyo")"#);
        // Polishing is turned on during the recording: it is polished.
        run.set.polish = "clean";
        run.pass(100);
        run.on(Input::LetGo(number));
        run.on(Input::Toggle);
        assert_eq!(run.on(heard("send the jason")), [r#"tell {"type":"polishing"}"#, "polish 1 send the JSON"]);
        let polished = Outcome { text: "Send the JSON.".into(), raw: "send the JSON".into(), polished: true, warning: None };
        assert_eq!(run.on(Input::Polished(number, polished.clone())), [format!("post 1 Ended(Over({polished:?}))")]);
        // Nothing heard is not polished.
        let mut run = Run::with(Set { polish: "clean", ..Set::default() });
        run.toggle_on();
        run.on(Input::HoldReleased);
        assert_eq!(run.on(heard("")), [r#"post 1 Ended(Over(Outcome { text: "", raw: "", polished: false, warning: None }))"#]);
    }

    #[test]
    fn cancelling_while_it_is_polished_ends_it_and_the_polishing_comes_to_nothing() {
        let mut run = Run::with(Set { polish: "clean", ..Set::default() });
        let number = run.toggle_on();
        run.on(Input::HoldReleased);
        run.on(heard("um hello"));
        assert_eq!(run.on(Input::Cancel), ["stop polishing", "post 1 Ended(Cancelled)"]);
        // What the polishing came to meanwhile is not written.
        assert_eq!(run.on(Input::Polished(number, Outcome::heard("Hello."))), Vec::<String>::new());
        assert_eq!(run.on(Input::Cancel), Vec::<String>::new());
        let told = End::Cancelled.told();
        assert_eq!(run.on(Input::Written(number, told)), ["hide after 2500", r#"tell {"cancelled":true,"raw":"","text":"","type":"result"}"#]);
    }

    #[test]
    fn a_recording_is_discarded_and_says_so() {
        let mut run = Run::new();
        let number = run.toggle_on();
        assert_eq!(run.on(Input::Cancel), ["discard 1"]);
        // Finishing it now is nothing.
        assert_eq!(run.on(Input::HoldReleased), Vec::<String>::new());
        assert_eq!(run.on(Input::Recorded(number, Event::Cancelled)), ["post 1 Ended(Cancelled)"]);
    }

    #[test]
    fn a_recording_that_ends_while_its_model_loads_is_heard_to_its_end() {
        let mut run = Run::new();
        let number = run.toggle_on();
        // Finished before the recognizer is ready: what was recorded waits for it.
        run.on(Input::HoldReleased);
        assert_eq!(run.on(Input::Recorded(number, Event::Ready)), [r#"tell {"type":"ready"}"#]);
        assert_eq!(run.on(Input::Recorded(number, Event::Sentence("Hello.".into())))[0], r#"post 1 Sentence("Hello.")"#);
        assert_eq!(run.on(heard("Hello.")).len(), 1);
        // A model that cannot be loaded ends it with why.
        let mut run = Run::new();
        let number = run.toggle_on();
        run.on(Input::HoldReleased);
        assert_eq!(run.on(Input::Recorded(number, Event::Failed(Failure::new("no-recorder")))), [r#"post 1 Ended(Failed("No audio recorder"))"#]);
    }

    #[test]
    fn what_an_earlier_recording_says_is_not_taken_for_the_next() {
        let mut run = Run::new();
        let first = run.toggle_on();
        run.on(Input::HoldReleased);
        run.on(heard("One."));
        run.on(Input::Written(first, End::Failed("x".into()).told()));
        run.pass(2000);
        let second = run.toggle_on();
        assert_eq!(run.on(Input::Recorded(first, Event::Sentence("Late.".into()))), Vec::<String>::new());
        assert_eq!(run.on(Input::LetGo(first)), Vec::<String>::new());
        assert!(run.session.holding(second));
        // Nor does a wait that it asked for hide the pill of this one.
        assert_eq!(run.on(Input::Lingered), Vec::<String>::new());
    }

    #[test]
    fn a_recording_that_cannot_be_told_to_stop_ends_with_why() {
        let mut run = Run::new();
        let number = run.toggle_on();
        run.on(Input::HoldReleased);
        assert_eq!(run.on(Input::Unstopped(number, "dictation is not running".into())), [r#"post 1 Ended(Failed("dictation is not running"))"#]);
    }

    #[test]
    fn an_extension_listening_hears_sentences_and_then_all_of_it_and_nothing_is_written() {
        let mut run = Run::with(Set { polish: "clean", ..Set::default() });
        let ear = Ear(Arc::new(|_| {}));
        assert_eq!(
            run.on(Input::Listen { language: Some("en".into()), indicator: true, ear: ear.clone() }),
            [r#"tell {"cloud":false,"polish":"off","type":"start"}"#, "start 1 from mic in en"]
        );
        assert_eq!(run.on(Input::Started(1)), ["listening Ok(1)", "show"]);
        assert!(run.session.listens(1) && !run.session.listens(0));
        // The user's hotkey cannot start another meanwhile.
        assert_eq!(run.on(Input::Listen { language: None, indicator: false, ear: ear.clone() }), ["listening Err(Busy)"]);
        assert_eq!(run.on(Input::Recorded(1, Event::Partial("Hel".into()))), [r#"tell {"text":"Hel","type":"partial"}"#]);
        assert_eq!(run.on(Input::Recorded(1, Event::Sentence("Hello.".into()))), [r#"hear Segment("Hello.")"#, r#"tell {"text":"Hello.","type":"segment"}"#]);
        // A later sentence comes with the space before it, which is not part of it.
        assert_eq!(run.on(Input::Recorded(1, Event::Sentence(" World.".into())))[0], r#"hear Segment("World.")"#);
        // It is not polished, whatever the setting.
        assert_eq!(run.on(Input::StopListening(1)), ["stop 1"]);
        assert_eq!(run.on(heard("Hello. World.")), ["hide after 300", r#"hear Result("Hello. World.")"#, r#"tell {"raw":"Hello. World.","text":"Hello. World.","type":"result"}"#]);
        assert!(!run.session.listens(1));
        // Without the pill, discarded.
        let mut run = Run::new();
        run.on(Input::Listen { language: None, indicator: false, ear: ear.clone() });
        assert_eq!(run.on(Input::Started(1)), ["listening Ok(1)"]);
        // An earlier recording's stop does not end this one.
        assert_eq!(run.on(Input::CancelListening(0)), Vec::<String>::new());
        assert_eq!(run.on(Input::CancelListening(1)), ["discard 1"]);
        assert_eq!(run.on(Input::Recorded(1, Event::Cancelled))[1], "hear Cancelled");
        // Why it could not listen.
        let mut run = Run::with(Set { opener: Ok((false, false)), ..Set::default() });
        assert_eq!(run.on(Input::Listen { language: None, indicator: true, ear: ear.clone() }), ["listening Err(NotInstalled)"]);
        let mut run = Run::new();
        run.on(Input::Listen { language: None, indicator: true, ear: ear.clone() });
        assert_eq!(run.on(Input::Unstarted(1, "no thread".into())), [r#"listening Err(Failed("no thread"))"#]);
    }

    #[test]
    fn an_extension_cannot_end_what_the_user_is_dictating() {
        let mut run = Run::new();
        let number = run.toggle_on();
        assert!(!run.session.listens(number));
        assert_eq!(run.on(Input::StopListening(number)), Vec::<String>::new());
        assert_eq!(run.on(Input::CancelListening(number)), Vec::<String>::new());
    }

    #[test]
    fn an_extension_hears_what_went_wrong() {
        let said = Arc::new(Mutex::new(Vec::new()));
        let hearing = said.clone();
        let mut run = Run::new();
        run.on(Input::Listen { language: None, indicator: true, ear: Ear(Arc::new(move |heard| hearing.lock().unwrap().push(heard))) });
        run.on(Input::Started(1));
        let effects = run.session.on(Input::Recorded(1, Event::Failed(Failure::failed("the speech model could not be loaded"))), run.now, &|| unreachable!());
        for effect in effects {
            if let Effect::Hear(Ear(ear), heard) = effect {
                ear(heard);
            }
        }
        // A failure it has no words for: those of what failed are for the log.
        assert_eq!(*said.lock().unwrap(), [Heard::Failed("Dictation failed".into())]);
    }

    #[test]
    fn what_needs_reading_stays_up() {
        let result = |text: &str, raw: &str, warning: Option<&str>| Told::Result { text: text.into(), raw: raw.into(), warning: warning.map(Into::into), cancelled: None, kept: None };
        assert_eq!(linger(&result("Hello.", "Hello.", None)), Duration::from_millis(300));
        // Polishing failed, nothing was heard, or the recording was cancelled.
        assert_eq!(linger(&result("um hello", "um hello", Some("no API key"))), Duration::from_millis(2500));
        assert_eq!(linger(&result("", "", None)), Duration::from_millis(2500));
        assert_eq!(linger(&End::Cancelled.told()), Duration::from_millis(2500));
        assert_eq!(linger(&Told::Error { message: "no microphone".into() }), Duration::from_secs(4));
    }
}

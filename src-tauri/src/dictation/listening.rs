//! One recording, from the microphone to what was heard: records, has the
//! recognizer hear it, measures how loud it is for the pill, and says all
//! of it as it happens, in the events the rest of `dictation` reads:
//!
//!   ready | level | partial | segment | result | error
//!
//! The recording starts before the recognizer is loaded, so that nothing
//! said meanwhile is lost: what is recorded waits for it. The recognizer is
//! kept for the next recording, and let go after a while without one.

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::engine::{doubao, Doubao, Failure, Recognized, Recognizer, SenseVoice};
use super::microphone::{self, Taker};

/// How long a recognizer is kept without a recording: it holds a few
/// hundred megabytes.
const KEPT_FOR: Duration = Duration::from_secs(10 * 60);

/// What the thread of a recording is told, in order.
enum Input {
    Audio(Vec<f32>),
    /// The recording is over: what was said is wanted.
    Stop,
    /// The recording is over, and what was said is not wanted.
    Cancel,
    /// The microphone stopped giving audio.
    Failed(Failure),
}

/// Says where the events of a recording go.
pub(crate) type Tell = Arc<dyn Fn(Value) + Send + Sync>;

/// The recording in progress, as whoever started it holds it.
pub(crate) struct Listening(Sender<Input>);

impl Listening {
    /// Ends the recording: its result is told.
    pub fn stop(&self) -> Result<(), String> {
        self.0.send(Input::Stop).map_err(|_| crate::i18n::t("dictation is not running").to_owned())
    }

    /// Discards the recording: it is told to have been cancelled.
    pub fn cancel(&self) {
        let _ = self.0.send(Input::Cancel);
    }
}

/// The recognizer kept between recordings, and when the last one ended.
struct Kept {
    recognizer: Option<SenseVoice>,
    idle_since: Option<Instant>,
}

static KEPT: Mutex<Kept> = Mutex::new(Kept { recognizer: None, idle_since: None });

/// Starts a recording from `source` (empty: the system's default
/// microphone), recognized in `language` from the files in `dir`, or by
/// Doubao where `cloud` says how.
pub(crate) fn listen(dir: std::path::PathBuf, source: &str, language: &str, cloud: Option<doubao::Config>, tell: Tell) -> Result<Listening, String> {
    let (tx, rx) = mpsc::channel();
    let (audio, failing, refused) = (tx.clone(), tx.clone(), tx.clone());
    // Record first, load second: nothing said during the load is lost.
    let recording = microphone::record(
        source,
        Taker {
            samples: Box::new(move |samples| drop(audio.send(Input::Audio(samples)))),
            failed: Box::new(move |failure| drop(failing.send(Input::Failed(failure)))),
        },
    );
    let language = language.to_owned();
    std::thread::Builder::new()
        .name("dictation-listening".into())
        .spawn(move || {
            // Whatever becomes of it, a recording that is waited for ends
            // with a word on how.
            let ran = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(&dir, &language, cloud, recording, (refused, rx), &tell)));
            match ran {
                Ok(Some(last)) => tell(last),
                // Nobody waits for it any more.
                Ok(None) => {}
                Err(_) => tell(error(&Failure::failed("the recording ended unexpectedly"))),
            }
        })
        .map_err(|e| crate::i18n::tf("failed to start dictation: {0}", &[&e]))?;
    Ok(Listening(tx))
}

fn error(failure: &Failure) -> Value {
    let mut said = failure.said();
    said["type"] = "error".into();
    said
}

/// The recording, until it ends: what it heard is told as it is heard,
/// and its last word returned.
/// A recognizer that fails says so where the microphone does (`tx`).
fn run(dir: &std::path::Path, language: &str, cloud: Option<doubao::Config>, recording: microphone::Recording, (tx, rx): (Sender<Input>, Receiver<Input>), tell: &Tell) -> Option<Value> {
    let mut kept = KEPT.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    kept.idle_since = None;
    // A connection is a recording's own: nothing is kept of it.
    let mut doubao = cloud.map(Doubao::new);
    if doubao.is_none() && kept.recognizer.as_ref().is_none_or(|recognizer| recognizer.language() != language) {
        kept.recognizer = None;
        match SenseVoice::load(dir, language) {
            Ok(recognizer) => kept.recognizer = Some(recognizer),
            Err(failure) => {
                drop(recording);
                return Some(error(&failure));
            }
        }
    }
    let recognizer: &mut dyn Recognizer = match doubao.as_mut() {
        Some(doubao) => doubao,
        None => kept.recognizer.as_mut().expect("loaded above"),
    };
    let heard = tell.clone();
    recognizer.begin(Box::new(move |recognized| match recognized {
        Recognized::Partial(text) => heard(json!({ "type": "partial", "text": text })),
        Recognized::Sentence(text) => heard(json!({ "type": "segment", "text": text })),
        Recognized::Failed(failure) => drop(tx.send(Input::Failed(failure))),
    }));
    tell(json!({ "type": "ready" }));
    let mut meter = Meter::new(Instant::now());
    let mut recording = Some(recording);
    let last = loop {
        let Ok(input) = rx.recv() else { break None };
        match input {
            Input::Audio(samples) => {
                if let Some(value) = meter.feed(&samples, Instant::now()) {
                    tell(json!({ "type": "level", "value": value }));
                }
                recognizer.feed(&samples);
            }
            Input::Stop => {
                recording = None;
                break Some(match recognizer.finish() {
                    Ok(raw) => json!({ "type": "result", "text": raw, "raw": raw, "polished": false }),
                    Err(failure) => error(&failure),
                });
            }
            Input::Cancel => break Some(json!({ "type": "result", "text": "", "raw": "", "polished": false, "cancelled": true })),
            Input::Failed(failure) => break Some(error(&failure)),
        }
    };
    drop(recording);
    // A connection still open is closed.
    drop(doubao);
    // What was heard but not finished is forgotten when the next begins.
    kept.idle_since = Some(Instant::now());
    drop(kept);
    let_go_later();
    last
}

/// Lets the recognizer go once it has not been used for `KEPT_FOR`.
fn let_go_later() {
    std::thread::spawn(|| {
        std::thread::sleep(KEPT_FOR);
        let mut kept = KEPT.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if kept.idle_since.is_some_and(|since| since.elapsed() >= KEPT_FOR) {
            kept.recognizer = None;
        }
    });
}

/// How loud a recording is, for the pill: from 0 to 1, about every 40 ms.
struct Meter {
    energy: f64,
    count: usize,
    at: Instant,
}

impl Meter {
    fn new(now: Instant) -> Meter {
        // As if the last was long ago: the first samples say how loud it is.
        Meter { energy: 0.0, count: 0, at: now.checked_sub(Duration::from_secs(1)).unwrap_or(now) }
    }

    /// Loudness as RMS (peaks saturate on any speech), on a square-root
    /// scale so quiet and loud speech both show movement.
    fn feed(&mut self, samples: &[f32], now: Instant) -> Option<f64> {
        self.energy += samples.iter().map(|&s| (s as f64) * (s as f64)).sum::<f64>();
        self.count += samples.len();
        if now.duration_since(self.at) <= Duration::from_millis(40) || self.count == 0 {
            return None;
        }
        let rms = (self.energy / self.count as f64).sqrt();
        (self.energy, self.count, self.at) = (0.0, 0, now);
        Some((rms.sqrt() * 1.6).min(1.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A recording end to end, as Heyo makes one: the WAV file in place of
    /// the microphone (`HEYO_DICTATION_WAV`), heard by SenseVoice with the
    /// models of the Heyo installed on this computer (read, never written).
    #[test]
    fn a_recording_says_it_is_ready_how_loud_it_is_what_it_hears_and_all_of_it() {
        let dir = std::env::var_os("HEYO_TEST_SPEECH").map(Into::into).unwrap_or_else(|| crate::paths::home().join(".local/share/heyo/dictation"));
        let wav = std::env::temp_dir().join(format!("heyo-listening-{}.wav", std::process::id()));
        let say = concat!(env!("CARGO_MANIFEST_DIR"), "/../scripts/gui-test/say.py");
        let spoken = std::process::Command::new("python3").arg(say).arg(&wav).args(["Good morning.", "See you soon."]).status();
        if !crate::dictation::models::installed(&dir) || !spoken.is_ok_and(|status| status.success()) {
            return eprintln!("skipped: no speech models installed, or no Flite for say.py");
        }
        std::env::set_var("HEYO_DICTATION_WAV", &wav);
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let listening = listen(dir, "", "en", None, Arc::new(move |event| drop(tx.lock().unwrap().send(event)))).unwrap();
        std::env::remove_var("HEYO_DICTATION_WAV");
        let mut events = Vec::new();
        while events.iter().filter(|event: &&Value| event["type"] == "segment").count() < 2 {
            events.push(rx.recv_timeout(Duration::from_secs(20)).expect("the two sentences"));
        }
        listening.stop().unwrap();
        let last = rx.iter().find(|event| event["type"] != "level").unwrap();
        let _ = std::fs::remove_file(&wav);
        assert_eq!(events[0], json!({ "type": "ready" }));
        let levels: Vec<f64> = events.iter().filter_map(|event| event["value"].as_f64()).collect();
        assert!(levels.len() > 20 && levels.iter().any(|level| *level > 0.1), "{levels:?}");
        let segments: Vec<&str> = events.iter().filter(|event| event["type"] == "segment").filter_map(|event| event["text"].as_str()).collect();
        assert_eq!(segments, ["Good morning.", " See you soon."]);
        assert!(events.iter().any(|event| event["type"] == "partial"), "heard in progress, at the pace of speech");
        assert_eq!(last, json!({ "type": "result", "text": "Good morning. See you soon.", "raw": "Good morning. See you soon.", "polished": false }));
    }

    #[test]
    fn says_how_loud_it_is_from_0_to_1() {
        let start = Instant::now();
        let mut meter = Meter::new(start);
        // A second of speech, then a second of silence, a tenth at a time.
        let levels: Vec<f64> = (0..20).filter_map(|tenth| meter.feed(&vec![if tenth < 10 { 0.1 } else { 0.0 }; 1600], start + Duration::from_millis(100 * tenth))).collect();
        assert_eq!(levels.len(), 20);
        assert!(levels.iter().all(|value| (0.0..=1.0).contains(value)));
        // Speech first, silence after.
        assert!((levels[1] - (0.1f64.sqrt() * 1.6)).abs() < 1e-6);
        assert_eq!(levels[levels.len() - 1], 0.0);
        // Loud speech is as loud as it gets.
        assert_eq!(meter.feed(&[1.0; 1600], start + Duration::from_secs(3)), Some(1.0));
        // Not more often than every 40 ms.
        assert_eq!(meter.feed(&[0.5; 160], start + Duration::from_millis(3010)), None);
    }
}

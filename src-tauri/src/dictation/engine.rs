//! What turns speech into text: a [`Recognizer`], fed the audio of a
//! recording as it is recorded, says the sentence in progress and each
//! finished one as it hears them.
//!
//! The ones on this computer are models of the catalogue (`models`) run
//! by sherpa-onnx. Most are not streaming models (`offline`): Silero's
//! voice-activity detector finds where sentences end, and the sentence
//! being spoken is simply recognized again every 300 ms or so
//! (`sentences`). A streaming model (`streaming`) hears the audio as it
//! comes and says itself where a sentence ends. Which of them a recording
//! gets is decided in one place, `load`.
//!
//! The other sends the audio to a service: Doubao's streaming recognition,
//! at Volcengine (`doubao`), where the user chose it and gave it a key.

pub(crate) mod doubao;
mod offline;
mod sentences;
mod streaming;

use std::fmt;
use std::path::Path;

use serde_json::{json, Value};

pub(crate) use doubao::Doubao;
pub(crate) use offline::Offline;
pub(crate) use streaming::Streaming;

use super::models::{self, Kind};

/// The model on this computer a recording is to be heard with: the
/// `dictationModel` setting (with no provider) and `dictationLanguage` as
/// they were when it began, the model resolved (`models::chosen`). A
/// recognizer that was loaded for the same is used again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Chosen {
    /// An entry of the catalogue, by its id.
    pub model: String,
    pub language: String,
}

impl Chosen {
    /// The setting's model, as a recording is heard with it: one that is
    /// not named, or not known, is the one `models::chosen` picks.
    pub fn of(dir: &Path, model: &str, language: &str) -> Chosen {
        Chosen { model: models::chosen(dir, model).id.to_owned(), language: language.to_owned() }
    }

    /// Whether it can be loaded without a download first.
    pub fn ready(&self, dir: &Path) -> bool {
        models::find(&self.model).is_some_and(|model| model.installed(dir))
    }
}

/// Opens the recognizer `chosen` names, from its files in the dictation
/// directory `dir`.
pub(crate) fn load(dir: &Path, chosen: &Chosen) -> Result<Box<dyn Recognizer>, Failure> {
    open(dir, &chosen.model, &chosen.language)
}

/// Opens the recognizer of the catalogue's entry `id`, in `language`
/// where the model is told one (`dictation::LANGUAGES`).
pub(crate) fn open(dir: &Path, id: &str, language: &str) -> Result<Box<dyn Recognizer>, Failure> {
    let Some(model) = models::find(id) else {
        return Err(Failure::failed(format!("there is no speech model {id}")));
    };
    Ok(match model.kind {
        Kind::Streaming => Box::new(Streaming::load(dir, model)?),
        _ => Box::new(Offline::load(dir, model, language)?),
    })
}

/// Samples a second: what a recognizer is fed is 16 kHz mono, as `f32`
/// from -1 to 1.
pub(crate) const SAMPLE_RATE: usize = 16000;

/// What a recognizer says of a recording while it runs. Text is as it is
/// to be written one piece after the other: a sentence begins with what
/// sets it apart from the one before (a space, where the language writes
/// one), so that the pieces only need joining.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Recognized {
    /// The sentence in progress, as far as it is recognized; it may be
    /// written differently the next time.
    Partial(String),
    /// A finished sentence: the text so far is these, one after the other.
    Sentence(String),
    /// The recognizer cannot go on (a service that refused, or went
    /// away): the recording ends with this. What it said before stays said.
    Failed(Failure),
}

/// Where a recognizer says what it heard. It may say it from a thread of
/// its own (one that waits on a service, say).
pub(crate) type Say = Box<dyn FnMut(Recognized) + Send>;

/// Speech to text, one recording at a time. A recognizer is kept from one
/// recording to the next: loading one takes longer than a recording waits.
pub(crate) trait Recognizer: Send {
    /// A recording begins: what it hears goes to `say`. Whatever was left
    /// of the one before, finished or not, is forgotten.
    fn begin(&mut self, say: Say);

    /// The next of the recording's audio (`SAMPLE_RATE`, mono).
    fn feed(&mut self, samples: &[f32]);

    /// The recording is over: what was still being said is finished, and
    /// said, and all of it is returned, the sentences joined.
    fn finish(&mut self) -> Result<String, Failure>;
}

/// What went wrong, as the host words it (`helper::worded`): a code, and
/// its particulars. `detail` is what only the words of whatever failed can
/// say (a server's answer, a library's message), shown as it is.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Failure {
    pub code: &'static str,
    pub particulars: Vec<(&'static str, String)>,
}

impl Failure {
    pub fn new(code: &'static str) -> Failure {
        Failure { code, particulars: Vec::new() }
    }

    /// A failure the host has no other words for than its detail.
    pub fn failed(detail: impl fmt::Display) -> Failure {
        Failure::new("failed").with("detail", detail)
    }

    pub fn with(mut self, name: &'static str, value: impl fmt::Display) -> Failure {
        self.particulars.push((name, value.to_string()));
        self
    }

    /// As `helper::worded` reads it.
    pub fn said(&self) -> Value {
        let mut said = json!({ "code": self.code });
        for (name, value) in &self.particulars {
            said[*name] = value.as_str().into();
        }
        said
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(self.code)?;
        for (name, value) in &self.particulars {
            write!(f, ", {name}: {value}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failure_is_said_as_its_code_and_particulars() {
        let failure = Failure::new("download").with("step", "speech-model").with("detail", "500 Nope");
        assert_eq!(failure.said(), json!({ "code": "download", "step": "speech-model", "detail": "500 Nope" }));
        assert_eq!(crate::helper::worded(&failure.said(), "Dictation failed").full(), "Could not download: Speech model: 500 Nope");
        assert_eq!(Failure::failed("ENOSPC").said(), json!({ "code": "failed", "detail": "ENOSPC" }));
    }

    /// Every model of the catalogue in `HEYO_MEASURE_SPEECH` (a dictation
    /// directory), on each of the WAV files `HEYO_MEASURE_CLIPS` names
    /// (16 kHz mono 16-bit, separated by `:`): how long loading takes, how
    /// fast it hears (time taken over the length of the audio, fed as fast
    /// as it is taken), and what it heard. For choosing among them:
    /// `cargo test --release measure_the_models -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn measure_the_models() {
        let dir = std::path::PathBuf::from(std::env::var("HEYO_MEASURE_SPEECH").expect("HEYO_MEASURE_SPEECH"));
        let clips: Vec<(String, Vec<f32>)> = std::env::var("HEYO_MEASURE_CLIPS")
            .expect("HEYO_MEASURE_CLIPS")
            .split(':')
            .map(|path| {
                let wav = std::fs::read(path).unwrap();
                (path.rsplit('/').next().unwrap().to_owned(), wav[44..].chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0).collect())
            })
            .collect();
        let only = std::env::var("HEYO_MEASURE_ONLY").unwrap_or_default();
        for model in models::CATALOGUE.iter().filter(|model| model.installed(&dir) && (only.is_empty() || only.split(',').any(|id| id == model.id))) {
            let loading = std::time::Instant::now();
            let mut recognizer = open(&dir, model.id, "auto").unwrap();
            let loaded = loading.elapsed().as_secs_f64();
            for (name, audio) in &clips {
                // Once to warm up, then measured.
                offline::tests::heard(recognizer.as_mut(), audio);
                let hearing = std::time::Instant::now();
                let (text, said) = offline::tests::heard(recognizer.as_mut(), audio);
                let rtf = hearing.elapsed().as_secs_f64() / (audio.len() as f64 / SAMPLE_RATE as f64);
                let partials = said.iter().filter(|said| matches!(said, Recognized::Partial(_))).count();
                println!("{:32} {name:10} load {loaded:5.2}s  RTF {rtf:.3}  partials {partials:3}  {text}", model.id);
            }
        }
    }
}

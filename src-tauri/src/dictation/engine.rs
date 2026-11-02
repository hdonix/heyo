//! What turns speech into text: a [`Recognizer`], fed the audio of a
//! recording as it is recorded, says the sentence in progress and each
//! finished one as it hears them.
//!
//! The one there is recognizes on this computer: SenseVoice, with Silero's
//! voice-activity detector finding where sentences end, through sherpa-onnx
//! (`sense_voice`). SenseVoice is not a streaming model, but it decodes
//! seconds of audio in tens of milliseconds, so the sentence being spoken is
//! simply recognized again every 300 ms (`sentences`). A recognizer that
//! streams, or one that sends the audio to a service, is fed the same way
//! and says the same things.

mod sense_voice;
mod sentences;

use std::fmt;

use serde_json::{json, Value};

pub(crate) use sense_voice::SenseVoice;

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
}

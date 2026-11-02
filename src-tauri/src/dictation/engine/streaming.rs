//! Recognizing on this computer with a streaming model (X-ASR's zipformer
//! transducer), run by sherpa-onnx's online recognizer: it hears the audio
//! as it is fed, a chunk at a time, and what it says of the sentence in
//! progress is what it has decoded so far, not the sentence decoded again.
//! Where a sentence ends it decides itself (its endpointing), by the
//! silence after it: no voice detector is needed.

use std::path::Path;

use sherpa_onnx::{OnlineRecognizer, OnlineRecognizerConfig, OnlineStream, OnlineTransducerModelConfig};

use super::offline::{file, THREADS};
use super::sentences::{between, meaningful, unfinished};
use super::{Failure, Recognized, Recognizer, Say, SAMPLE_RATE};
use crate::dictation::models::Model;

/// Silence after speech that ends a sentence. Shorter than the voice
/// detector's of the other models: what the model counts as silence
/// begins only once it has decoded the last word, a chunk and more later.
const PAUSE_SECONDS: f32 = 0.5;
/// Silence fed at the end of a recording, so that the last words, still
/// in the model's chunk, are decoded.
const TAIL_SECONDS: f32 = 0.6;

pub(crate) struct Streaming {
    recognizer: OnlineRecognizer,
    recording: Option<Recording>,
}

struct Recording {
    stream: OnlineStream,
    say: Say,
    /// The finished sentences, each as it joins those before it.
    sentences: Vec<String>,
    /// The sentence in progress as it was last said, without the joining.
    partial: String,
}

impl Streaming {
    /// Loads the model from the dictation directory (`models`).
    pub fn load(dir: &Path, model: &Model) -> Result<Streaming, Failure> {
        let part = |name: &str| file(model.part(dir, name));
        let parts = &model.parts;
        let mut config = OnlineRecognizerConfig::default();
        config.feat_config.sample_rate = SAMPLE_RATE as i32;
        config.feat_config.feature_dim = 80;
        config.model_config.transducer = OnlineTransducerModelConfig { encoder: part(parts.encoder)?, decoder: part(parts.decoder)?, joiner: part(parts.joiner)? };
        config.model_config.tokens = part(parts.tokens)?;
        config.model_config.num_threads = THREADS;
        config.model_config.provider = Some("cpu".into());
        config.decoding_method = Some("greedy_search".into());
        config.enable_endpoint = true;
        // Silence before anything was said ends nothing worth saying; after
        // speech, a pause ends the sentence; and none goes on for ever.
        config.rule1_min_trailing_silence = 2.4;
        config.rule2_min_trailing_silence = PAUSE_SECONDS;
        config.rule3_min_utterance_length = 25.0;
        let recognizer = OnlineRecognizer::create(&config).ok_or_else(|| Failure::failed("the speech model could not be loaded"))?;
        Ok(Streaming { recognizer, recording: None })
    }
}

impl Recording {
    fn joined(&self, text: &str) -> String {
        between(&self.sentences.concat(), text).to_owned() + text
    }

    /// Decodes what it can of what it was fed, and says what that changed.
    fn decode(&mut self, recognizer: &OnlineRecognizer) {
        while recognizer.is_ready(&self.stream) {
            recognizer.decode(&self.stream);
        }
        let text = recognizer.get_result(&self.stream).map(|result| result.text.trim().to_owned()).unwrap_or_default();
        if recognizer.is_endpoint(&self.stream) {
            self.finished(text);
            recognizer.reset(&self.stream);
        } else if meaningful(&text) && unfinished(&text) != self.partial {
            self.partial = unfinished(&text).to_owned();
            let partial = self.joined(&self.partial);
            (self.say)(Recognized::Partial(partial));
        }
    }

    /// The sentence in progress is over.
    fn finished(&mut self, text: String) {
        self.partial.clear();
        if meaningful(&text) {
            let sentence = self.joined(&text);
            self.sentences.push(sentence.clone());
            (self.say)(Recognized::Sentence(sentence));
        }
    }
}

impl Recognizer for Streaming {
    fn begin(&mut self, say: Say) {
        self.recording = Some(Recording { stream: self.recognizer.create_stream(), say, sentences: Vec::new(), partial: String::new() });
    }

    fn feed(&mut self, samples: &[f32]) {
        if let Some(recording) = &mut self.recording {
            recording.stream.accept_waveform(SAMPLE_RATE as i32, samples);
            recording.decode(&self.recognizer);
        }
    }

    fn finish(&mut self) -> Result<String, Failure> {
        let Some(mut recording) = self.recording.take() else { return Ok(String::new()) };
        recording.stream.accept_waveform(SAMPLE_RATE as i32, &vec![0.0; (TAIL_SECONDS * SAMPLE_RATE as f32) as usize]);
        recording.stream.input_finished();
        while self.recognizer.is_ready(&recording.stream) {
            self.recognizer.decode(&recording.stream);
        }
        let text = self.recognizer.get_result(&recording.stream).map(|result| result.text.trim().to_owned()).unwrap_or_default();
        recording.finished(text);
        Ok(recording.sentences.concat())
    }
}

#[cfg(test)]
mod tests {
    use super::super::offline::tests::{heard, installed_in, sentences, spoken};
    use super::super::Recognized;
    use super::*;
    use crate::dictation::models;

    /// The streaming model on a WAV clip: it says the sentence in progress
    /// as it grows, each sentence when the speaker pauses, and all of it.
    /// With the model in the dictation directory of the Heyo installed
    /// here, or in `HEYO_TEST_SPEECH`.
    #[test]
    fn the_streaming_model_hears_as_it_is_fed() {
        let id = "x-asr-2026-06-05";
        let (Some(dir), Some(audio)) = (installed_in(id), spoken(&["The weather is nice today.", "Please send me the report."])) else {
            return eprintln!("skipped: no streaming model installed, or no Flite for say.py");
        };
        let mut recognizer = Streaming::load(&dir, models::find(id).unwrap()).unwrap();
        for _ in 0..2 {
            let (text, said) = heard(&mut recognizer, &audio);
            let words = |text: &str| text.to_lowercase().chars().filter(|c| c.is_alphanumeric() || *c == ' ').collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ");
            assert_eq!(words(&text), "the weather is nice today please send me the report", "{text}");
            let sentences = sentences(&said);
            assert_eq!(sentences.len(), 2, "{said:?}");
            assert_eq!(sentences.concat(), text);
            assert!(sentences[1].starts_with(' '), "{sentences:?}");
            // In progress, the words come one after the other.
            let partials: Vec<&String> = said.iter().filter_map(|said| if let Recognized::Partial(text) = said { Some(text) } else { None }).collect();
            assert!(partials.len() >= 3, "{partials:?}");
        }
    }
}

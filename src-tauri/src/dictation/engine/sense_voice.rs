//! Recognizing on this computer: SenseVoice, a model that knows Chinese,
//! English, Japanese, Korean and Cantonese, and Silero's voice-activity
//! detector, both run by sherpa-onnx. What it heard is all it says: the
//! host writes the user's vocabulary into it and has it polished.

use std::path::Path;
use std::time::Instant;

use sherpa_onnx::{OfflineRecognizer, OfflineRecognizerConfig, OfflineSenseVoiceModelConfig, SileroVadModelConfig, VadModelConfig, VoiceActivityDetector};

use super::sentences::{Engine, Segment, Session, VAD_WINDOW};
use super::{Failure, Recognizer, Say, SAMPLE_RATE};
use crate::dictation::models;

/// The recognizer and the detector, which outlive a recording.
struct Sherpa {
    recognizer: OfflineRecognizer,
    vad: VoiceActivityDetector,
}

impl Engine for Sherpa {
    fn decode(&mut self, samples: &[f32]) -> String {
        let stream = self.recognizer.create_stream();
        stream.accept_waveform(SAMPLE_RATE as i32, samples);
        self.recognizer.decode(&stream);
        stream.get_result().map(|result| result.text.trim().to_owned()).unwrap_or_default()
    }

    fn accept(&mut self, window: &[f32]) {
        self.vad.accept_waveform(window);
    }

    fn detected(&self) -> bool {
        self.vad.detected()
    }

    fn front(&self) -> Option<Segment> {
        if self.vad.is_empty() {
            return None;
        }
        self.vad.front().map(|segment| Segment { start: segment.start().max(0) as usize, samples: segment.samples().to_vec() })
    }

    fn pop(&mut self) {
        self.vad.pop();
    }

    fn flush(&mut self) {
        self.vad.flush();
    }

    fn reset(&mut self) {
        self.vad.reset();
    }
}

/// SenseVoice in one of its languages (`dictation::LANGUAGES`; "auto"
/// tells them apart by itself).
pub(crate) struct SenseVoice {
    engine: Sherpa,
    language: String,
    recording: Option<(Session, Say)>,
    began: Instant,
}

impl SenseVoice {
    /// Loads the model and the detector from the dictation directory
    /// (`models`), which takes a second or so.
    pub fn load(dir: &Path, language: &str) -> Result<SenseVoice, Failure> {
        let file = |pinned: &models::Pinned| {
            let path = dir.join(pinned.path);
            match path.exists() {
                true => Ok(path.to_string_lossy().into_owned()),
                false => Err(Failure::failed(format!("{} is missing", path.display()))),
            }
        };
        let mut config = OfflineRecognizerConfig::default();
        config.feat_config.sample_rate = SAMPLE_RATE as i32;
        config.feat_config.feature_dim = 80;
        // No hotwords: sherpa-onnx biases only transducer models decoded
        // with modified_beam_search, and SenseVoice is a CTC model decoded
        // greedily. Its other way in, the homophone replacer (`hr`), wants
        // rule FSTs compiled ahead of time. The host applies the vocabulary
        // to the text instead.
        config.model_config.sense_voice = OfflineSenseVoiceModelConfig { model: Some(file(&models::SPEECH_MODEL)?), language: Some(language.to_owned()), use_itn: true };
        config.model_config.tokens = Some(file(&models::TOKENS)?);
        config.model_config.num_threads = 4;
        config.model_config.provider = Some("cpu".into());
        config.model_config.debug = false;
        let recognizer = OfflineRecognizer::create(&config).ok_or_else(|| Failure::failed("the speech model could not be loaded"))?;
        let vad_config = VadModelConfig {
            silero_vad: SileroVadModelConfig {
                model: Some(file(&models::VOICE_DETECTOR)?),
                threshold: 0.5,
                min_speech_duration: 0.25,
                min_silence_duration: 0.6,
                max_speech_duration: 25.0,
                window_size: VAD_WINDOW as i32,
            },
            sample_rate: SAMPLE_RATE as i32,
            num_threads: 1,
            debug: false,
            ..Default::default()
        };
        let vad = VoiceActivityDetector::create(&vad_config, 60.0).ok_or_else(|| Failure::failed("the voice detector could not be loaded"))?;
        Ok(SenseVoice { engine: Sherpa { recognizer, vad }, language: language.to_owned(), recording: None, began: Instant::now() })
    }

    /// The language it was loaded for.
    pub fn language(&self) -> &str {
        &self.language
    }
}

impl Recognizer for SenseVoice {
    fn begin(&mut self, say: Say) {
        self.began = Instant::now();
        self.recording = Some((Session::new(&mut self.engine), say));
    }

    fn feed(&mut self, samples: &[f32]) {
        let now = self.began.elapsed().as_millis() as u64;
        if let Some((session, say)) = &mut self.recording {
            session.feed(&mut self.engine, samples, now, say);
        }
    }

    fn finish(&mut self) -> Result<String, Failure> {
        Ok(match self.recording.take() {
            Some((mut session, mut say)) => session.finish(&mut self.engine, &mut say),
            None => String::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::Recognized;
    use std::sync::{Arc, Mutex};

    /// The models of the Heyo installed on this computer, read and never
    /// written (`HEYO_TEST_SPEECH` names another dictation directory).
    fn models() -> Option<std::path::PathBuf> {
        let dir = std::env::var_os("HEYO_TEST_SPEECH").map(Into::into).unwrap_or_else(|| crate::paths::home().join(".local/share/heyo/dictation"));
        models::installed(&dir).then_some(dir)
    }

    /// Two sentences spoken by `scripts/gui-test/say.py`, as 16 kHz mono
    /// samples.
    fn spoken(sentences: &[&str]) -> Option<Vec<f32>> {
        let out = std::env::temp_dir().join(format!("heyo-sense-voice-{}.wav", std::process::id()));
        let say = concat!(env!("CARGO_MANIFEST_DIR"), "/../scripts/gui-test/say.py");
        let made = std::process::Command::new("python3").arg(say).arg(&out).args(sentences).output().ok()?;
        if !made.status.success() {
            return None;
        }
        let wav = std::fs::read(&out).ok()?;
        let _ = std::fs::remove_file(&out);
        Some(wav[44..].chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0).collect())
    }

    #[test]
    fn sense_voice_hears_what_was_said() {
        let (Some(dir), Some(audio)) = (models(), spoken(&["The weather is nice today.", "Please send me the report."])) else {
            return eprintln!("skipped: no speech models installed, or no Flite for say.py");
        };
        let mut recognizer = SenseVoice::load(&dir, "en").unwrap();
        let said = Arc::new(Mutex::new(Vec::new()));
        for _ in 0..2 {
            said.lock().unwrap().clear();
            let heard = said.clone();
            recognizer.begin(Box::new(move |recognized| heard.lock().unwrap().push(recognized)));
            for chunk in audio.chunks(1600) {
                recognizer.feed(chunk);
            }
            let text = recognizer.finish().unwrap();
            assert_eq!(text, "The weather is nice today. Please send me the report.");
            let sentences: Vec<_> = said.lock().unwrap().iter().filter_map(|said| if let Recognized::Sentence(text) = said { Some(text.clone()) } else { None }).collect();
            // (Fed faster than it is spoken, little time passes for the
            // sentence in progress to be recognized: `sentences` shows that.)
            assert_eq!(sentences, ["The weather is nice today.", " Please send me the report."]);
        }
    }
}

//! Recognizing on this computer with a model that is not a streaming one
//! (SenseVoice, Paraformer, FireRedASR, Whisper, Parakeet), and Silero's
//! voice-activity detector finding where sentences end, both run by
//! sherpa-onnx. The sentence being spoken is recognized again and again
//! (`sentences`). What it heard is all it says: the host writes the user's
//! vocabulary into it and has it polished.

use std::path::Path;
use std::time::Instant;

use sherpa_onnx::{
    OfflineFireRedAsrModelConfig, OfflineParaformerModelConfig, OfflineRecognizer, OfflineRecognizerConfig, OfflineSenseVoiceModelConfig, OfflineTransducerModelConfig, OfflineWhisperModelConfig, SileroVadModelConfig, VadModelConfig, VoiceActivityDetector,
};

use super::sentences::{Engine, Segment, Session, VAD_WINDOW};
use super::{Failure, Recognizer, Say, SAMPLE_RATE};
use crate::dictation::models::{self, Kind, Model};

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

/// A file of the model, which must be there.
pub(super) fn file(path: std::path::PathBuf) -> Result<Option<String>, Failure> {
    match path.exists() {
        true => Ok(Some(path.to_string_lossy().into_owned())),
        false => Err(Failure::failed(format!("{} is missing", path.display()))),
    }
}

/// How many threads a recognizer decodes on.
pub(super) const THREADS: i32 = 4;

/// An offline model of the catalogue, in one of its languages where it is
/// told one (`dictation::LANGUAGES`; "auto" tells them apart by itself).
pub(crate) struct Offline {
    engine: Sherpa,
    recording: Option<(Session, Say)>,
    began: Instant,
}

/// What sherpa-onnx is told of `model`, its files in `dir`.
fn config(dir: &Path, model: &Model, language: &str) -> Result<OfflineRecognizerConfig, Failure> {
    let part = |name: &str| file(model.part(dir, name));
    let mut config = OfflineRecognizerConfig::default();
    config.feat_config.sample_rate = SAMPLE_RATE as i32;
    config.feat_config.feature_dim = 80;
    // No hotwords: sherpa-onnx biases only transducer models decoded with
    // modified_beam_search, and most of these are not. The host applies
    // the vocabulary to the text instead, the same for every model.
    let parts = &model.parts;
    match model.kind {
        Kind::SenseVoice => config.model_config.sense_voice = OfflineSenseVoiceModelConfig { model: part(parts.model)?, language: Some(language.to_owned()), use_itn: true },
        Kind::Paraformer => config.model_config.paraformer = OfflineParaformerModelConfig { model: part(parts.model)? },
        Kind::FireRed => config.model_config.fire_red_asr = OfflineFireRedAsrModelConfig { encoder: part(parts.encoder)?, decoder: part(parts.decoder)? },
        Kind::Whisper => {
            // Whisper names Chinese "zh" whatever the writing, and detects
            // the language where it is told none.
            let language = if language == "auto" { "" } else { language };
            config.model_config.whisper = OfflineWhisperModelConfig { encoder: part(parts.encoder)?, decoder: part(parts.decoder)?, language: Some(language.to_owned()), task: Some("transcribe".into()), tail_paddings: -1, ..Default::default() };
        }
        Kind::NemoTransducer => {
            config.model_config.transducer = OfflineTransducerModelConfig { encoder: part(parts.encoder)?, decoder: part(parts.decoder)?, joiner: part(parts.joiner)? };
            config.model_config.model_type = Some("nemo_transducer".into());
        }
        Kind::Streaming => return Err(Failure::failed(format!("{} is a streaming model", model.id))),
    }
    config.model_config.tokens = part(parts.tokens)?;
    config.model_config.num_threads = THREADS;
    config.model_config.provider = Some("cpu".into());
    config.model_config.debug = false;
    config.decoding_method = Some("greedy_search".into());
    Ok(config)
}

impl Offline {
    /// Loads the model and the detector from the dictation directory
    /// (`models`), which takes a second or more.
    pub fn load(dir: &Path, model: &Model, language: &str) -> Result<Offline, Failure> {
        let recognizer = OfflineRecognizer::create(&config(dir, model, language)?).ok_or_else(|| Failure::failed("the speech model could not be loaded"))?;
        let vad_config = VadModelConfig {
            silero_vad: SileroVadModelConfig {
                model: file(dir.join(models::VOICE_DETECTOR.path))?,
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
        Ok(Offline { engine: Sherpa { recognizer, vad }, recording: None, began: Instant::now() })
    }
}

impl Recognizer for Offline {
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
pub(super) mod tests {
    use super::super::Recognized;
    use super::*;
    use std::sync::{Arc, Mutex};

    /// The dictation directory of the Heyo installed on this computer,
    /// read and never written (`HEYO_TEST_SPEECH` names another), if it
    /// has `model`.
    pub fn installed_in(model: &str) -> Option<std::path::PathBuf> {
        let dir = std::env::var_os("HEYO_TEST_SPEECH").map(Into::into).unwrap_or_else(|| crate::paths::home().join(".local/share/heyo/dictation"));
        models::find(model).is_some_and(|model| model.installed(&dir)).then_some(dir)
    }

    /// Sentences spoken by `scripts/gui-test/say.py`, as 16 kHz mono
    /// samples.
    pub fn spoken(sentences: &[&str]) -> Option<Vec<f32>> {
        let out = std::env::temp_dir().join(format!("heyo-spoken-{}-{}.wav", std::process::id(), sentences.len()));
        let say = concat!(env!("CARGO_MANIFEST_DIR"), "/../scripts/gui-test/say.py");
        let made = std::process::Command::new("python3").arg(say).arg(&out).args(sentences).output().ok()?;
        if !made.status.success() {
            return None;
        }
        let wav = std::fs::read(&out).ok()?;
        let _ = std::fs::remove_file(&out);
        Some(wav[44..].chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0).collect())
    }

    /// What `recognizer` hears of `audio`, fed a tenth of a second at a
    /// time: all of it, and the sentences as it said them.
    pub fn heard(recognizer: &mut dyn Recognizer, audio: &[f32]) -> (String, Vec<Recognized>) {
        let said = Arc::new(Mutex::new(Vec::new()));
        let heard = said.clone();
        recognizer.begin(Box::new(move |recognized| heard.lock().unwrap().push(recognized)));
        for chunk in audio.chunks(1600) {
            recognizer.feed(chunk);
        }
        let text = recognizer.finish().unwrap();
        let said = said.lock().unwrap().clone();
        (text, said)
    }

    pub fn sentences(said: &[Recognized]) -> Vec<String> {
        said.iter().filter_map(|said| if let Recognized::Sentence(text) = said { Some(text.clone()) } else { None }).collect()
    }

    #[test]
    fn sense_voice_hears_what_was_said() {
        let (Some(dir), Some(audio)) = (installed_in("sense-voice-2024-07-17"), spoken(&["The weather is nice today.", "Please send me the report."])) else {
            return eprintln!("skipped: no speech models installed, or no Flite for say.py");
        };
        let mut recognizer = Offline::load(&dir, models::find("sense-voice-2024-07-17").unwrap(), "en").unwrap();
        for _ in 0..2 {
            let (text, said) = heard(&mut recognizer, &audio);
            assert_eq!(text, "The weather is nice today. Please send me the report.");
            // (Fed faster than it is spoken, little time passes for the
            // sentence in progress to be recognized: `sentences` shows that.)
            assert_eq!(sentences(&said), ["The weather is nice today.", " Please send me the report."]);
        }
    }
}

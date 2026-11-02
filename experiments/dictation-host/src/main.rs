//! Recognizes a 16 kHz mono 16-bit WAV file (what HEYO_DICTATION_WAV names)
//! with SenseVoice and Silero VAD through sherpa-onnx's own Rust API, the
//! way the Node.js helper configures them, and lists the microphones cpal
//! sees.
//!
//!   cargo run --release -- MODELS-DIR FILE.wav [LANGUAGE]
//!   cargo run --release -- --microphones

use std::time::Instant;

use sherpa_onnx::{OfflineRecognizer, OfflineRecognizerConfig, OfflineSenseVoiceModelConfig, SileroVadModelConfig, VadModelConfig, VoiceActivityDetector};

const RATE: i32 = 16000;
const WINDOW: usize = 512;

fn samples(wav: &[u8]) -> Vec<f32> {
    wav[44..].chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0).collect()
}

#[cfg(target_os = "linux")]
fn microphones() {
    println!("(on Linux the PulseAudio sources; not asked here)");
}

#[cfg(not(target_os = "linux"))]
fn microphones() {
    use cpal::traits::{DeviceTrait, HostTrait};
    let host = cpal::default_host();
    println!("host: {:?}", host.id());
    for device in host.input_devices().into_iter().flatten() {
        let description = device.description().map(|d| d.to_string()).unwrap_or_default();
        let config = device.default_input_config().map(|c| format!("{} Hz, {} ch, {:?}", c.sample_rate(), c.channels(), c.sample_format()));
        println!("{description}: {config:?}");
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--microphones") {
        return microphones();
    }
    let (models, wav) = (&args[0], &args[1]);
    let language = args.get(2).cloned().unwrap_or_else(|| "auto".into());

    let loading = Instant::now();
    let mut config = OfflineRecognizerConfig::default();
    config.feat_config.sample_rate = RATE;
    config.feat_config.feature_dim = 80;
    config.model_config.sense_voice = OfflineSenseVoiceModelConfig {
        model: Some(format!("{models}/sense-voice/model.int8.onnx")),
        language: Some(language),
        use_itn: true,
    };
    config.model_config.tokens = Some(format!("{models}/sense-voice/tokens.txt"));
    config.model_config.num_threads = 4;
    config.model_config.provider = Some("cpu".into());
    let recognizer = OfflineRecognizer::create(&config).expect("recognizer");
    let vad = VoiceActivityDetector::create(
        &VadModelConfig {
            silero_vad: SileroVadModelConfig {
                model: Some(format!("{models}/silero_vad.onnx")),
                threshold: 0.5,
                min_speech_duration: 0.25,
                min_silence_duration: 0.6,
                max_speech_duration: 25.0,
                window_size: WINDOW as i32,
            },
            sample_rate: RATE,
            num_threads: 1,
            ..Default::default()
        },
        60.0,
    )
    .expect("vad");
    println!("loaded in {:?}", loading.elapsed());

    let audio = samples(&std::fs::read(wav).expect("wav"));
    let decode = |samples: &[f32]| {
        let stream = recognizer.create_stream();
        stream.accept_waveform(RATE, samples);
        recognizer.decode(&stream);
        stream.get_result().map(|r| r.text.trim().to_owned()).unwrap_or_default()
    };
    let started = Instant::now();
    let mut said = Vec::new();
    let drain = |vad: &VoiceActivityDetector, said: &mut Vec<String>| {
        while !vad.is_empty() {
            let segment = vad.front().unwrap();
            let at = Instant::now();
            let text = decode(segment.samples());
            println!("[{:.2}s, {} samples, decoded in {:?}] {text}", segment.start() as f32 / RATE as f32, segment.n(), at.elapsed());
            said.push(text);
            vad.pop();
        }
    };
    for window in audio.chunks(WINDOW) {
        vad.accept_waveform(window);
        drain(&vad, &mut said);
    }
    vad.flush();
    drain(&vad, &mut said);
    println!("all of it in {:?}: {}", started.elapsed(), said.join(" "));
}

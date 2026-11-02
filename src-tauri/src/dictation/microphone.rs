//! Recording the microphone, as 16 kHz mono samples for a recognizer.
//!
//! Each system has its own way to the microphone, and that is all that is
//! its own here: on Linux the sound server, spoken to in its own protocol
//! (PulseAudio's, which PipeWire speaks too), or ALSA's `arecord` where
//! there is no sound server; elsewhere cpal (WASAPI on Windows, CoreAudio
//! on macOS), whose audio is mixed down to one channel and resampled.
//!
//! Tests feed a WAV file at the pace of speech instead
//! (`HEYO_DICTATION_WAV`): 16 kHz mono 16-bit PCM, as
//! `scripts/gui-test/say.py` writes it.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;

use super::engine::{Failure, SAMPLE_RATE};

/// What a recording is given: samples as they are recorded, and why there
/// are no more, if it ends before it is stopped.
pub(crate) struct Taker {
    pub samples: Box<dyn FnMut(Vec<f32>) + Send>,
    pub failed: Box<dyn FnOnce(Failure) + Send>,
}

/// A microphone being recorded; dropping it stops the recording.
pub(crate) struct Recording {
    stop: Option<Box<dyn FnOnce() + Send>>,
}

impl Recording {
    fn new(stop: impl FnOnce() + Send + 'static) -> Recording {
        Recording { stop: Some(Box::new(stop)) }
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            stop();
        }
    }
}

/// An input device the user can choose (`dictationSource`): `name` is what
/// the setting holds, which on Linux is the sound server's name for it.
#[derive(Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../shared/bindings/")]
pub struct Microphone {
    pub name: String,
    pub description: String,
}

/// Records `source` (a [`Microphone`]'s name; empty for the system's
/// default) until the recording is dropped.
pub(crate) fn record(source: &str, taker: Taker) -> Recording {
    match std::env::var_os("HEYO_DICTATION_WAV") {
        Some(wav) => from_file(wav.into(), taker),
        None => system::record(source, taker),
    }
}

/// The input devices there are, without the sound server's monitors of
/// its outputs.
pub fn microphones() -> Vec<Microphone> {
    system::microphones()
}

/// 16-bit little-endian PCM as samples.
fn samples(bytes: &[u8]) -> Vec<f32> {
    bytes.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0).collect()
}

/// Feeds a WAV file a tenth of a second at a time, at the pace of speech.
fn from_file(wav: std::path::PathBuf, mut taker: Taker) -> Recording {
    let stopped = Arc::new(AtomicBool::new(false));
    let running = stopped.clone();
    std::thread::spawn(move || {
        let data = match std::fs::read(&wav) {
            Ok(data) => data,
            Err(e) => return (taker.failed)(Failure::failed(format!("{}: {e}", wav.display()))),
        };
        for chunk in data.get(44..).unwrap_or_default().chunks(SAMPLE_RATE / 10 * 2) {
            std::thread::sleep(Duration::from_millis(100));
            if running.load(Ordering::SeqCst) {
                return;
            }
            (taker.samples)(samples(chunk));
        }
    });
    Recording::new(move || stopped.store(true, Ordering::SeqCst))
}

#[cfg(target_os = "linux")]
mod system {
    //! PulseAudio's protocol, which PipeWire's server speaks as well: the
    //! server is asked for 16 kHz mono and converts to it. Where none
    //! answers, ALSA's `arecord`.

    use std::ffi::CString;
    use std::io::{BufReader, Read};
    use std::os::unix::net::UnixStream;
    use std::process::{Child, Command, Stdio};

    use pulseaudio::protocol;

    use super::*;

    type Reader = BufReader<UnixStream>;

    /// Asks the server what it is asked, and reads its answer.
    fn ask<T: protocol::CommandReply>(socket: &mut Reader, seq: u32, command: protocol::Command, version: u16) -> Result<T, protocol::ProtocolError> {
        protocol::write_command_message(socket.get_mut(), seq, &command, version)?;
        protocol::read_reply_message::<T>(socket, version).map(|(_, reply)| reply)
    }

    /// Connects to the sound server of this session, as the client named
    /// Heyo; and the protocol version both speak.
    fn connect() -> Result<(Reader, u16), String> {
        let path = pulseaudio::socket_path_from_env().ok_or("no sound server")?;
        let mut socket = BufReader::new(UnixStream::connect(&path).map_err(|e| format!("{}: {e}", path.display()))?);
        let cookie = pulseaudio::cookie_path_from_env().and_then(|path| std::fs::read(path).ok()).unwrap_or_default();
        let auth = protocol::AuthParams { version: protocol::MAX_VERSION, supports_shm: false, supports_memfd: false, cookie };
        let reply: protocol::AuthReply = ask(&mut socket, 0, protocol::Command::Auth(auth), protocol::MAX_VERSION).map_err(|e| e.to_string())?;
        let version = protocol::MAX_VERSION.min(reply.version);
        let mut props = protocol::Props::new();
        props.set(protocol::Prop::ApplicationName, c"Heyo".to_owned());
        let _: protocol::SetClientNameReply = ask(&mut socket, 1, protocol::Command::SetClientName(props), version).map_err(|e| e.to_string())?;
        Ok((socket, version))
    }

    /// A recording stream from `source` (the default one if empty), 40 ms
    /// a piece, as `parec --latency-msec=40` asked for.
    fn open(socket: &mut Reader, version: u16, source: &str) -> Result<(), protocol::ProtocolError> {
        let params = protocol::RecordStreamParams {
            sample_spec: protocol::SampleSpec { format: protocol::SampleFormat::S16Le, channels: 1, sample_rate: SAMPLE_RATE as u32 },
            channel_map: protocol::ChannelMap::mono(),
            source_name: Some(if source.is_empty() { protocol::DEFAULT_SOURCE.to_owned() } else { CString::new(source).unwrap_or_default() }),
            buffer_attr: protocol::stream::BufferAttr { fragment_size: (SAMPLE_RATE * 2 * 40 / 1000) as u32, ..Default::default() },
            flags: protocol::stream::StreamFlags { adjust_latency: true, ..Default::default() },
            ..Default::default()
        };
        let _: protocol::CreateRecordStreamReply = ask(socket, 2, protocol::Command::CreateRecordStream(params), version)?;
        Ok(())
    }

    /// Reads what the server sends until the connection is shut: the
    /// recording's audio, and messages, which are passed over.
    fn read(mut socket: Reader, samples: &mut dyn FnMut(Vec<f32>)) -> std::io::Result<()> {
        let mut leftover: Vec<u8> = Vec::new();
        let mut buffer = Vec::new();
        loop {
            let descriptor = protocol::read_descriptor(&mut socket).map_err(std::io::Error::other)?;
            buffer.resize(descriptor.length as usize, 0);
            socket.read_exact(&mut buffer)?;
            if descriptor.channel == u32::MAX {
                continue;
            }
            leftover.extend_from_slice(&buffer);
            let usable = leftover.len() - leftover.len() % 2;
            samples(super::samples(&leftover[..usable]));
            leftover.drain(..usable);
        }
    }

    pub fn record(source: &str, taker: Taker) -> Recording {
        let opened = connect().and_then(|(mut socket, version)| {
            let opened = open(&mut socket, version, source);
            // A microphone the settings name that is not there now (one
            // unplugged): the one the system records from.
            if let Err(protocol::ProtocolError::ServerError(why)) = &opened {
                if !source.is_empty() {
                    eprintln!("heyo: dictation: no microphone {source} ({why:?}); recording the default one");
                    let (mut socket, version) = connect()?;
                    open(&mut socket, version, "").map_err(|e| e.to_string())?;
                    return Ok(socket);
                }
            }
            opened.map(|()| socket).map_err(|e| e.to_string())
        });
        match opened {
            Ok(socket) => from_server(socket, taker),
            Err(why) => {
                eprintln!("heyo: dictation: no sound server to record from ({why}); trying arecord");
                from_alsa(taker)
            }
        }
    }

    fn from_server(socket: Reader, mut taker: Taker) -> Recording {
        let shut = socket.get_ref().try_clone();
        let stopped = Arc::new(AtomicBool::new(false));
        let running = stopped.clone();
        std::thread::spawn(move || {
            let ended = read(socket, &mut *taker.samples);
            if !running.load(Ordering::SeqCst) {
                let why = ended.err().map(|e| e.to_string()).unwrap_or_default();
                (taker.failed)(Failure::failed(format!("the sound server stopped the recording: {why}")));
            }
        });
        Recording::new(move || {
            stopped.store(true, Ordering::SeqCst);
            if let Ok(socket) = shut {
                let _ = socket.shutdown(std::net::Shutdown::Both);
            }
        })
    }

    /// ALSA's default device, through `arecord`.
    fn from_alsa(mut taker: Taker) -> Recording {
        let spawned = Command::new("arecord").args(["-q", "-f", "S16_LE", "-r", "16000", "-c", "1", "-t", "raw"]).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn();
        let mut child: Child = match spawned {
            Ok(child) => child,
            Err(_) => {
                (taker.failed)(Failure::new("no-recorder"));
                return Recording::new(|| {});
            }
        };
        let mut out = child.stdout.take().expect("arecord's output is piped");
        let stopped = Arc::new(AtomicBool::new(false));
        let running = stopped.clone();
        let child = Arc::new(std::sync::Mutex::new(child));
        let reaped = child.clone();
        std::thread::spawn(move || {
            let mut buffer = vec![0u8; 3200];
            let mut leftover = Vec::new();
            while let Ok(n) = out.read(&mut buffer) {
                if n == 0 {
                    break;
                }
                leftover.extend_from_slice(&buffer[..n]);
                let usable = leftover.len() - leftover.len() % 2;
                (taker.samples)(super::samples(&leftover[..usable]));
                leftover.drain(..usable);
            }
            let _ = reaped.lock().unwrap().wait();
            if !running.load(Ordering::SeqCst) {
                (taker.failed)(Failure::failed("arecord stopped recording"));
            }
        });
        Recording::new(move || {
            stopped.store(true, Ordering::SeqCst);
            let _ = child.lock().unwrap().kill();
        })
    }

    pub fn microphones() -> Vec<Microphone> {
        let Ok((mut socket, version)) = connect() else { return Vec::new() };
        let Ok(sources) = ask::<protocol::SourceInfoList>(&mut socket, 2, protocol::Command::GetSourceInfoList, version) else { return Vec::new() };
        sources
            .into_iter()
            .filter(|source| source.monitor_of_sink_index.is_none() && !source.name.to_string_lossy().ends_with(".monitor"))
            .map(|source| {
                let name = source.name.to_string_lossy().into_owned();
                let description = source.description.map(|d| d.to_string_lossy().into_owned()).unwrap_or_else(|| name.clone());
                Microphone { name, description }
            })
            .collect()
    }
}

#[cfg(not(target_os = "linux"))]
mod system {
    //! cpal: the system's own audio interface, in the format the device
    //! gives, mixed down and resampled here.

    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use cpal::{Device, SampleFormat};

    use super::*;

    fn device(source: &str) -> Option<Device> {
        let host = cpal::default_host();
        let named = (!source.is_empty()).then(|| host.input_devices().ok()?.find(|device| device.id().is_ok_and(|id| id.to_string() == source))).flatten();
        if named.is_none() && !source.is_empty() {
            eprintln!("heyo: dictation: no microphone {source}; recording the default one");
        }
        named.or_else(|| host.default_input_device())
    }

    /// Mixes down and resamples what the device gives.
    struct Converter {
        channels: usize,
        resampler: Option<sherpa_onnx::LinearResampler>,
    }

    impl Converter {
        fn convert(&self, interleaved: impl Iterator<Item = f32>) -> Vec<f32> {
            let all: Vec<f32> = interleaved.collect();
            let mono: Vec<f32> = all.chunks(self.channels).map(|frame| frame.iter().sum::<f32>() / frame.len() as f32).collect();
            match &self.resampler {
                Some(resampler) => resampler.resample(&mono, false),
                None => mono,
            }
        }
    }

    /// Why the recording ended, said once.
    type Failed = std::sync::Arc<std::sync::Mutex<Option<Box<dyn FnOnce(Failure) + Send>>>>;

    fn fail(failed: &Failed, failure: Failure) {
        if let Some(failed) = failed.lock().unwrap().take() {
            failed(failure);
        }
    }

    fn stream(device: &Device, samples: Box<dyn FnMut(Vec<f32>) + Send>, failed: Failed) -> Result<cpal::Stream, String> {
        let supported = device.default_input_config().map_err(|e| e.to_string())?;
        let config = supported.config();
        let rate = config.sample_rate;
        let converter = Converter {
            channels: (config.channels as usize).max(1),
            resampler: (rate as usize != SAMPLE_RATE).then(|| sherpa_onnx::LinearResampler::create(rate as i32, SAMPLE_RATE as i32)).flatten(),
        };
        let error = move |e: cpal::Error| fail(&failed, Failure::failed(e));
        let mut samples = samples;
        let stream = match supported.sample_format() {
            SampleFormat::F32 => device.build_input_stream(config, move |data: &[f32], _: &_| samples(converter.convert(data.iter().copied())), error, None),
            SampleFormat::I16 => device.build_input_stream(config, move |data: &[i16], _: &_| samples(converter.convert(data.iter().map(|&s| s as f32 / 32768.0))), error, None),
            SampleFormat::I32 => device.build_input_stream(config, move |data: &[i32], _: &_| samples(converter.convert(data.iter().map(|&s| s as f32 / 2147483648.0))), error, None),
            SampleFormat::U16 => device.build_input_stream(config, move |data: &[u16], _: &_| samples(converter.convert(data.iter().map(|&s| (s as f32 - 32768.0) / 32768.0))), error, None),
            other => return Err(format!("the microphone gives samples as {other}, which Heyo does not read")),
        };
        let stream = stream.map_err(|e| e.to_string())?;
        stream.play().map_err(|e| e.to_string())?;
        Ok(stream)
    }

    pub fn record(source: &str, taker: Taker) -> Recording {
        let source = source.to_owned();
        let (stop, stopped) = std::sync::mpsc::channel::<()>();
        // A stream is not to be moved between threads on every system: it
        // lives and ends on one of its own.
        std::thread::spawn(move || {
            let failed: Failed = std::sync::Arc::new(std::sync::Mutex::new(Some(taker.failed)));
            let Some(device) = device(&source) else { return fail(&failed, Failure::new("no-microphone")) };
            match stream(&device, taker.samples, failed.clone()) {
                Ok(stream) => {
                    let _ = stopped.recv();
                    drop(stream);
                }
                Err(why) => fail(&failed, Failure::failed(why)),
            }
        });
        Recording::new(move || {
            let _ = stop.send(());
        })
    }

    pub fn microphones() -> Vec<Microphone> {
        let Ok(devices) = cpal::default_host().input_devices() else { return Vec::new() };
        devices
            .filter_map(|device| {
                let name = device.id().ok()?.to_string();
                let description = device.description().map(|d| d.to_string()).unwrap_or_else(|_| name.clone());
                Some(Microphone { name, description })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    /// Against a sound server of the test's own, which it names in
    /// `HEYO_TEST_PULSE_SOURCE`: a source of 16 kHz mono that is fed a
    /// steady tone of a quarter of full scale
    /// (`module-pipe-source` of PipeWire or PulseAudio, with a fifo).
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "needs a sound server of the test's own"]
    fn the_sound_servers_microphone_is_recorded_as_it_gives_it() {
        let source = std::env::var("HEYO_TEST_PULSE_SOURCE").expect("HEYO_TEST_PULSE_SOURCE");
        assert!(microphones().iter().any(|microphone| microphone.name == source), "{:?}", microphones().iter().map(|m| &m.name).collect::<Vec<_>>());
        assert!(microphones().iter().all(|microphone| !microphone.name.ends_with(".monitor")));
        for named in [source.as_str(), "", "no-such-microphone"] {
            let (tx, rx) = mpsc::channel();
            let recording = system::record(named, Taker { samples: Box::new(move |samples| drop(tx.send(samples))), failed: Box::new(|failure| panic!("{failure}")) });
            let mut heard = Vec::new();
            while heard.len() < SAMPLE_RATE / 2 {
                heard.extend(rx.recv_timeout(Duration::from_secs(5)).expect("audio from the sound server"));
            }
            drop(recording);
            let loud = heard.iter().filter(|sample| (**sample - 0.25).abs() < 0.01).count();
            assert!(loud > heard.len() * 9 / 10, "{named}: {loud} of {} samples are the tone", heard.len());
        }
    }

    #[test]
    fn a_file_is_fed_at_the_pace_of_speech_until_the_recording_is_stopped() {
        let wav = std::env::temp_dir().join(format!("heyo-microphone-{}.wav", std::process::id()));
        let mut bytes = vec![0u8; 44];
        // Half a second of a quiet tone, then half a second of silence.
        bytes.extend((0..SAMPLE_RATE).flat_map(|i| (if i < SAMPLE_RATE / 2 { 1638i16 } else { 0 }).to_le_bytes()));
        std::fs::write(&wav, &bytes).unwrap();
        let (tx, rx) = mpsc::channel();
        let started = std::time::Instant::now();
        let recording = from_file(wav.clone(), Taker { samples: Box::new(move |samples| tx.send(samples).unwrap()), failed: Box::new(|failure| panic!("{failure}")) });
        let first = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(first.len(), SAMPLE_RATE / 10);
        assert!((first[0] - 0.05).abs() < 0.001);
        let mut fed = first.len();
        while fed < SAMPLE_RATE / 2 {
            fed += rx.recv_timeout(Duration::from_secs(2)).unwrap().len();
        }
        assert!(started.elapsed() >= Duration::from_millis(450), "{:?}", started.elapsed());
        drop(recording);
        std::thread::sleep(Duration::from_millis(250));
        assert!(rx.try_iter().count() <= 1);
        let _ = std::fs::remove_file(&wav);
    }
}

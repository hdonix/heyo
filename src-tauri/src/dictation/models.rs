//! The speech models dictation can use (`CATALOGUE`), and the files each
//! is made of, fetched on demand into `dictation::dir()` so that what runs
//! is what was chosen when Heyo was built and not whatever the network
//! hands over that day: each from an address that names one revision, or
//! one asset of sherpa-onnx's `asr-models` release, checked against a
//! SHA-256 written here before it gets its name.
//!
//! What goes wrong is said as a code and its particulars (`Failure`), and
//! each step of an installation as a code, which `helper::worded` and
//! `helper::step` word in the user's language.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

use super::engine::Failure;
use crate::i18n::marked;

/// A file to download: where it goes in the dictation directory, where it
/// comes from, and what it must be.
#[derive(Debug)]
pub(crate) struct Pinned {
    pub path: &'static str,
    /// What it is, as a step of the installation (`helper::step`).
    pub step: &'static str,
    /// Lowercase hex.
    pub sha256: &'static str,
    /// Tried in order; mirrors of the same bytes.
    pub urls: &'static [&'static str],
}

/// The model repository at one revision (its head on 2026-10-10, unchanged
/// since 2024-07-18), so the address always names the same bytes.
macro_rules! sense_voice {
    ($host:literal, $file:literal) => {
        concat!($host, "/csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17/resolve/2365baeacb507f821a0c8120fcee3d484dba7a07/", $file)
    };
}

/// An asset of sherpa-onnx's release of speech models.
macro_rules! release {
    ($file:literal) => {
        concat!("https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/", $file)
    };
}

// The hashes are of the files as downloaded from these addresses on
// 2026-10-10. Two of them can be compared with what the hosts publish: the
// model's with the `lfs.sha256` Hugging Face lists for it, the voice
// detector's with the digest GitHub lists for the release asset.
const SENSE_VOICE_2024: [Pinned; 2] = [
    Pinned {
        path: "models/sense-voice/model.int8.onnx",
        step: "speech-model",
        sha256: "c71f0ce00bec95b07744e116345e33d8cbbe08cef896382cf907bf4b51a2cd51",
        urls: &[sense_voice!("https://huggingface.co", "model.int8.onnx"), sense_voice!("https://hf-mirror.com", "model.int8.onnx")],
    },
    Pinned {
        path: "models/sense-voice/tokens.txt",
        step: "vocabulary",
        sha256: "f449eb28dc567533d7fa59be34e2abca8784f771850c78a47fb731a31429a1dc",
        urls: &[sense_voice!("https://huggingface.co", "tokens.txt"), sense_voice!("https://hf-mirror.com", "tokens.txt")],
    },
];

/// Finds where sentences end, for every model that is not a streaming one.
pub(crate) const VOICE_DETECTOR: Pinned = Pinned {
    path: "models/silero_vad.onnx",
    step: "voice-detector",
    // A release asset can be replaced under the same address; the hash is what pins it.
    sha256: "9e2449e1087496d8d4caba907f23e0bd3f78d91fa552479bb9c23ac09cbb1fd6",
    urls: &[release!("silero_vad.onnx")],
};

/// How sherpa-onnx runs a model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    SenseVoice,
    Paraformer,
    FireRed,
    Whisper,
    /// NVIDIA NeMo's transducers (Parakeet).
    NemoTransducer,
    /// A streaming zipformer transducer: it hears as it is fed, and says
    /// itself where a sentence ends.
    Streaming,
}

/// A model's files, by what they are to sherpa-onnx, in its directory
/// ("" where it has no such file).
#[derive(Debug, Default)]
pub(crate) struct Parts {
    pub model: &'static str,
    pub encoder: &'static str,
    pub decoder: &'static str,
    pub joiner: &'static str,
    pub tokens: &'static str,
}

impl Parts {
    fn all(&self) -> impl Iterator<Item = &'static str> {
        [self.model, self.encoder, self.decoder, self.joiner, self.tokens].into_iter().filter(|part| !part.is_empty())
    }
}

/// Where a model comes from.
#[derive(Debug)]
pub(crate) enum Source {
    /// Files of their own, each pinned.
    Files(&'static [Pinned]),
    /// An archive (.tar.bz2) of sherpa-onnx's release, whose directory
    /// `root` holds the parts, among other things (examples, scripts) that
    /// are not kept.
    Archive { url: &'static str, sha256: &'static str, root: &'static str },
}

/// A speech model of the catalogue.
#[derive(Debug)]
#[allow(dead_code)] // `licence` is for reading here; the rest the Models page shows.
pub(crate) struct Model {
    /// The `dictationModel` setting.
    pub id: &'static str,
    /// Its name, which is the same in every language.
    pub name: &'static str,
    /// The languages it knows, worded (`marked`).
    pub languages: &'static str,
    /// What sets it apart from the others, worded (`marked`): relative
    /// words only, which hold on any computer.
    pub note: &'static str,
    /// How much is downloaded, in megabytes.
    pub megabytes: u32,
    pub kind: Kind,
    /// Its directory, in the dictation directory.
    pub dir: &'static str,
    pub parts: Parts,
    pub source: Source,
    /// Whether it is told the language (the `dictationLanguage` setting).
    pub language: bool,
    /// Whose it is, and under what licence it may be used.
    pub licence: &'static str,
}

/// The models, in the order the settings list them.
///
/// The archives' hashes are the digests GitHub lists for the release
/// assets, where it lists one, and otherwise (FireRedASR, Whisper, from
/// before GitHub listed them) of the assets as downloaded on 2026-11-02.
pub(crate) const CATALOGUE: &[Model] = &[
    Model {
        id: "sense-voice-2024-07-17",
        name: "SenseVoice Small",
        languages: marked("Chinese, Cantonese, English, Japanese, Korean"),
        note: marked("Fast, with punctuation."),
        megabytes: 240,
        kind: Kind::SenseVoice,
        // Where it was put before there was a choice.
        dir: "models/sense-voice",
        parts: Parts { model: "model.int8.onnx", encoder: "", decoder: "", joiner: "", tokens: "tokens.txt" },
        source: Source::Files(&SENSE_VOICE_2024),
        language: true,
        licence: "FunASR Model License 1.1 (FunAudioLLM, Alibaba): use, copy, modify and share, naming the source",
    },
    Model {
        id: "sense-voice-2025-09-09",
        // Despite its name, not a new SenseVoice for all of its languages:
        // ASLP-lab's fine-tuning for Cantonese (its README says so). Heard
        // on 2026-11-02: no punctuation, English in capitals and worse than
        // the 2024 model's.
        name: "SenseVoice Small Cantonese",
        languages: marked("Cantonese, Chinese, English"),
        note: marked("Fast, made for Cantonese. No punctuation."),
        megabytes: 166,
        kind: Kind::SenseVoice,
        dir: "models/sense-voice-2025-09-09",
        parts: Parts { model: "model.int8.onnx", encoder: "", decoder: "", joiner: "", tokens: "tokens.txt" },
        source: Source::Archive {
            url: release!("sherpa-onnx-sense-voice-zh-en-ja-ko-yue-int8-2025-09-09.tar.bz2"),
            sha256: "7305f7905bfcf77fa0b39388a313f3da35c68d971661a65475b56fb2162c8e63",
            root: "sherpa-onnx-sense-voice-zh-en-ja-ko-yue-int8-2025-09-09",
        },
        language: true,
        licence: "Apache-2.0 (ASLP-lab WSYue-ASR, fine-tuned from FunAudioLLM's SenseVoice Small)",
    },
    Model {
        id: "x-asr-2026-06-05",
        name: "X-ASR Streaming",
        languages: marked("Chinese, English"),
        note: marked("Streaming: the words come as you speak them, with punctuation."),
        megabytes: 134,
        kind: Kind::Streaming,
        dir: "models/x-asr-2026-06-05",
        parts: Parts { model: "", encoder: "encoder.int8.onnx", decoder: "decoder.onnx", joiner: "joiner.int8.onnx", tokens: "tokens.txt" },
        source: Source::Archive {
            url: release!("sherpa-onnx-x-asr-480ms-streaming-zipformer-transducer-zh-en-punct-int8-2026-06-05.tar.bz2"),
            sha256: "fa5f63d618e5a01526e275a358bb7772e403f84808a4769fba52cffd8160bf74",
            root: "sherpa-onnx-x-asr-480ms-streaming-zipformer-transducer-zh-en-punct-int8-2026-06-05",
        },
        language: false,
        licence: "Apache-2.0 (GilgameshWind, X-ASR)",
    },
    Model {
        id: "paraformer-zh-2025-10-07",
        name: "Paraformer",
        // ASLP-lab's Paraformer-large for Sichuanese (WenetSpeech-Chuan).
        languages: marked("Chinese, Sichuanese"),
        note: marked("Fast, made for Sichuanese. No punctuation, little English."),
        megabytes: 228,
        kind: Kind::Paraformer,
        dir: "models/paraformer-zh-2025-10-07",
        parts: Parts { model: "model.int8.onnx", encoder: "", decoder: "", joiner: "", tokens: "tokens.txt" },
        source: Source::Archive {
            url: release!("sherpa-onnx-paraformer-zh-int8-2025-10-07.tar.bz2"),
            sha256: "a071ee5419e14adb34d7f970ab98105a45e6608018b168f023ca2e4810744abe",
            root: "sherpa-onnx-paraformer-zh-int8-2025-10-07",
        },
        language: false,
        licence: "Apache-2.0 (ASLP-lab WSChuan-ASR, Paraformer-large-Chuan)",
    },
    Model {
        id: "fire-red-asr-large-2025-02-16",
        name: "FireRedASR Large",
        languages: marked("Chinese, English"),
        note: marked("Accurate for Chinese and English, and slow. No punctuation."),
        megabytes: 1470,
        kind: Kind::FireRed,
        dir: "models/fire-red-asr-large-2025-02-16",
        parts: Parts { model: "", encoder: "encoder.int8.onnx", decoder: "decoder.int8.onnx", joiner: "", tokens: "tokens.txt" },
        source: Source::Archive {
            url: release!("sherpa-onnx-fire-red-asr-large-zh_en-2025-02-16.tar.bz2"),
            sha256: "1b158e9d46715ed1cd387402b125de26f2e09bf2cb73926414b7fbd74d1973e2",
            root: "sherpa-onnx-fire-red-asr-large-zh_en-2025-02-16",
        },
        language: false,
        licence: "Apache-2.0 (FireRedTeam, FireRedASR-AED-L)",
    },
    Model {
        id: "whisper-turbo",
        name: "Whisper Turbo",
        languages: marked("99 languages"),
        note: marked("Many languages, slow."),
        megabytes: 564,
        kind: Kind::Whisper,
        dir: "models/whisper-turbo",
        parts: Parts { model: "", encoder: "turbo-encoder.int8.onnx", decoder: "turbo-decoder.int8.onnx", joiner: "", tokens: "turbo-tokens.txt" },
        source: Source::Archive { url: release!("sherpa-onnx-whisper-turbo.tar.bz2"), sha256: "b11acbbcd660b44a8e0df33724feb5aaa709cf65668f2823d59f656312544f22", root: "sherpa-onnx-whisper-turbo" },
        language: true,
        licence: "MIT (OpenAI, Whisper large-v3-turbo)",
    },
    Model {
        id: "parakeet-tdt-0.6b-v3",
        name: "Parakeet TDT 0.6B v3",
        languages: marked("English and 24 other European languages"),
        note: marked("Accurate and fast, with punctuation."),
        megabytes: 487,
        kind: Kind::NemoTransducer,
        dir: "models/parakeet-tdt-0.6b-v3",
        parts: Parts { model: "", encoder: "encoder.int8.onnx", decoder: "decoder.int8.onnx", joiner: "joiner.int8.onnx", tokens: "tokens.txt" },
        source: Source::Archive {
            url: release!("sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8.tar.bz2"),
            sha256: "5793d0fd397c5778d2cf2126994d58e9d56b1be7c04d13c7a15bb1b4eafb16bf",
            root: "sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8",
        },
        language: false,
        licence: "CC-BY-4.0 (NVIDIA, parakeet-tdt-0.6b-v3)",
    },
];

/// The model of an installation where none was chosen: of those that
/// are fast, the one that heard Chinese, English and the two mixed best,
/// with punctuation.
pub(crate) const DEFAULT: &str = "sense-voice-2024-07-17";

pub(crate) fn find(id: &str) -> Option<&'static Model> {
    CATALOGUE.iter().find(|model| model.id == id)
}

impl Model {
    /// Where one of its parts is.
    pub fn part(&self, dir: &Path, part: &str) -> PathBuf {
        dir.join(self.dir).join(part)
    }

    /// Whether it needs the voice detector to find where sentences end.
    pub fn detected(&self) -> bool {
        self.kind != Kind::Streaming
    }

    /// Whether all its files are there (not whether they are what they
    /// should be: that is checked when they are fetched).
    pub fn installed(&self, dir: &Path) -> bool {
        self.parts.all().all(|part| self.part(dir, part).exists()) && (!self.detected() || dir.join(VOICE_DETECTOR.path).exists())
    }
}

/// The model `setting` (`dictationModel`) names: that one, if there is
/// such a model; where it names none, the default, or if that is not
/// installed and another is, the one that is.
pub(crate) fn chosen(dir: &Path, setting: &str) -> &'static Model {
    if let Some(model) = find(setting) {
        return model;
    }
    let default = find(DEFAULT).expect("the default is in the catalogue");
    if default.installed(dir) {
        return default;
    }
    CATALOGUE.iter().find(|model| model.installed(dir)).unwrap_or(default)
}

/// Where a file is fetched from: its own addresses, or only the server
/// `HEYO_DICTATION_MIRROR` names, which has the files by their names (for
/// trying the download against a server of one's own).
fn addresses(url: &str, others: &[&str]) -> Vec<String> {
    match std::env::var("HEYO_DICTATION_MIRROR") {
        Ok(mirror) if !mirror.is_empty() => {
            let name = url.rsplit('/').next().unwrap_or(url);
            vec![format!("{}/{name}", mirror.trim_end_matches('/'))]
        }
        _ => std::iter::once(url).chain(others.iter().copied()).map(str::to_owned).collect(),
    }
}

fn pinned_addresses(file: &Pinned) -> Vec<String> {
    addresses(file.urls[0], &file.urls[1..])
}

/// How an installation is going: a step, and bytes `received` of `total`
/// (0 where the server did not say).
pub(crate) type Progress<'a> = &'a (dyn Fn(&str, u64, u64) + Sync);

fn cancelled() -> Failure {
    Failure::new("cancelled")
}

/// Makes sure every file of `model` is there and is the pinned one,
/// downloading what is missing or is something else. Stops, leaving
/// nothing half done under the model's name, once `cancel` is set.
pub(crate) async fn install(dir: &Path, model: &Model, progress: Progress<'_>, cancel: &AtomicBool) -> Result<(), Failure> {
    if model.detected() {
        download(dir, &VOICE_DETECTOR, &pinned_addresses(&VOICE_DETECTOR), progress, cancel).await?;
    }
    match &model.source {
        Source::Files(files) => {
            for file in files.iter() {
                download(dir, file, &pinned_addresses(file), progress, cancel).await?;
            }
            Ok(())
        }
        Source::Archive { url, sha256, root } => {
            if model.installed(dir) {
                return Ok(());
            }
            let archive = Pinned { path: "", step: "speech-model", sha256, urls: &[] };
            let at = dir.join(format!("{}.tar.bz2", model.dir));
            let failure = |detail: &dyn std::fmt::Display| Failure::new("download").with("step", "speech-model").with("detail", detail);
            std::fs::create_dir_all(at.parent().unwrap_or(dir)).map_err(|e| failure(&e))?;
            fetch_into(&at, &archive, &addresses(url, &[]), progress, cancel).await?;
            let parts: Vec<&'static str> = model.parts.all().collect();
            // This takes a while, and holds the thread: an installation runs
            // on a thread of its own (`dictation::download`).
            let done = unpack(&at, root, &parts, &dir.join(model.dir), progress, cancel);
            let _ = std::fs::remove_file(&at);
            done
        }
    }
}

/// Takes `parts` out of the directory `root` of the archive at `archive`
/// into `into`, which has them all or is not there.
fn unpack(archive: &Path, root: &str, parts: &[&str], into: &Path, progress: Progress<'_>, cancel: &AtomicBool) -> Result<(), Failure> {
    let failure = |detail: &dyn std::fmt::Display| Failure::new("download").with("step", "unpacking").with("detail", detail);
    let partial = PathBuf::from(format!("{}.part", into.display()));
    let _ = std::fs::remove_dir_all(&partial);
    std::fs::create_dir_all(&partial).map_err(|e| failure(&e))?;
    let file = std::fs::File::open(archive).map_err(|e| failure(&e))?;
    let total = file.metadata().map(|meta| meta.len()).unwrap_or(0);
    let counted = Counted { inner: file, read: 0, total, reported: None, progress, cancel };
    let mut tar = tar::Archive::new(bzip2::read::MultiBzDecoder::new(counted));
    let mut found = 0;
    let result = (|| {
        for entry in tar.entries().map_err(|e| failure(&e))? {
            let mut entry = entry.map_err(|e| if cancel.load(Ordering::Relaxed) { cancelled() } else { failure(&e) })?;
            let path = entry.path().map_err(|e| failure(&e))?.into_owned();
            let Ok(name) = path.strip_prefix(root) else { continue };
            let Some(part) = parts.iter().find(|part| Path::new(part) == name) else { continue };
            entry.unpack(partial.join(part)).map_err(|e| if cancel.load(Ordering::Relaxed) { cancelled() } else { failure(&e) })?;
            found += 1;
        }
        if cancel.load(Ordering::Relaxed) {
            return Err(cancelled());
        }
        if found < parts.len() {
            return Err(failure(&format!("the archive does not have all of {}", parts.join(", "))));
        }
        let _ = std::fs::remove_dir_all(into);
        std::fs::rename(&partial, into).map_err(|e| failure(&e))
    })();
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&partial);
    }
    result
}

/// The archive as it is read, saying how far, and failing once cancelled.
struct Counted<'a> {
    inner: std::fs::File,
    read: u64,
    total: u64,
    reported: Option<Instant>,
    progress: Progress<'a>,
    cancel: &'a AtomicBool,
}

impl Read for Counted<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err(std::io::Error::other("cancelled"));
        }
        let n = self.inner.read(buffer)?;
        self.read += n as u64;
        if self.reported.is_none_or(|at| at.elapsed() > Duration::from_millis(200)) {
            self.reported = Some(Instant::now());
            (self.progress)("unpacking", self.read, self.total);
        }
        Ok(n)
    }
}

#[allow(dead_code)] // The Models page's (`dictation::delete`).
/// Takes a model's files away. The voice detector, which the others use
/// too, stays.
pub(crate) fn delete(dir: &Path, model: &Model) -> std::io::Result<()> {
    match std::fs::remove_dir_all(dir.join(model.dir)) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

fn sha256(file: &Path) -> std::io::Result<String> {
    let mut hash = Sha256::new();
    let mut reader = std::fs::File::open(file)?;
    let mut buffer = vec![0; 1 << 16];
    loop {
        match reader.read(&mut buffer)? {
            0 => return Ok(hex(&hash.finalize())),
            n => hash.update(&buffer[..n]),
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Makes sure `file` is in `dir` and is the pinned file, downloading it
/// from the first of `urls` that gives it if it is missing or is something
/// else. A download that is not the pinned file never gets the file's name.
async fn download(dir: &Path, file: &Pinned, urls: &[String], progress: Progress<'_>, cancel: &AtomicBool) -> Result<(), Failure> {
    let path = dir.join(file.path);
    let failure = |detail: &dyn std::fmt::Display| Failure::new("download").with("step", file.step).with("detail", detail);
    // Left by an earlier installation, which did not check what it got.
    if path.exists() {
        if sha256(&path).map_err(|e| failure(&e))? == file.sha256 {
            return Ok(());
        }
        std::fs::remove_file(&path).map_err(|e| failure(&e))?;
    }
    std::fs::create_dir_all(path.parent().unwrap_or(dir)).map_err(|e| failure(&e))?;
    fetch_into(&path, file, urls, progress, cancel).await
}

/// Fetches `file` from the first of `urls` that gives it, into `path`.
async fn fetch_into(path: &Path, file: &Pinned, urls: &[String], progress: Progress<'_>, cancel: &AtomicBool) -> Result<(), Failure> {
    let failure = |detail: &dyn std::fmt::Display| Failure::new("download").with("step", file.step).with("detail", detail);
    let partial = PathBuf::from(format!("{}.part", path.display()));
    let mut last_error = String::new();
    for url in urls {
        match fetch(url, &partial, file, progress, cancel).await {
            Ok(()) => return std::fs::rename(&partial, path).map_err(|e| failure(&e)),
            Err(e) => {
                let _ = std::fs::remove_file(&partial);
                if cancel.load(Ordering::Relaxed) {
                    return Err(cancelled());
                }
                last_error = e;
            }
        }
    }
    Err(failure(&last_error))
}

/// Fetches `url` into `partial`, and says why that is not the pinned file
/// if it is not.
async fn fetch(url: &str, partial: &Path, file: &Pinned, progress: Progress<'_>, cancel: &AtomicBool) -> Result<(), String> {
    let mut response = crate::models::client(false).get(url).send().await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(response.status().to_string());
    }
    let total = response.content_length().unwrap_or(0);
    let mut out = std::io::BufWriter::new(std::fs::File::create(partial).map_err(|e| e.to_string())?);
    let mut hash = Sha256::new();
    let mut received = 0;
    let mut reported: Option<Instant> = None;
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".into());
        }
        hash.update(&chunk);
        out.write_all(&chunk).map_err(|e| e.to_string())?;
        received += chunk.len() as u64;
        if reported.is_none_or(|at| at.elapsed() > Duration::from_millis(200)) {
            reported = Some(Instant::now());
            progress(file.step, received, total);
        }
    }
    out.flush().map_err(|e| e.to_string())?;
    let got = hex(&hash.finalize());
    if got != file.sha256 {
        let host = reqwest::Url::parse(url).ok().and_then(|url| url.host_str().map(str::to_owned)).unwrap_or_default();
        return Err(format!("what {host} sent is not the expected file (SHA-256 {got}, expected {}); it was not installed", file.sha256));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufRead;
    use std::net::TcpListener;
    use std::sync::Mutex;

    /// A file server on this computer: each path to its status and body.
    fn serve(files: Vec<(&'static str, u16, Vec<u8>)>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let path = line.split_whitespace().nth(1).unwrap_or_default().to_owned();
                // The rest of the request.
                while reader.read_line(&mut line).is_ok_and(|n| n > 2) {
                    line.clear();
                }
                let (status, body) = files.iter().find(|(served, ..)| *served == path).map(|(_, status, body)| (*status, body.clone())).unwrap_or((404, Vec::new()));
                let reason = if status == 200 { "OK" } else { "Not Found" };
                let _ = write!(stream, "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                let _ = stream.write_all(&body);
            }
        });
        address
    }

    fn pinned(body: &[u8]) -> Pinned {
        let sha256 = Box::leak(hex(&Sha256::digest(body)).into_boxed_str());
        Pinned { path: "models/thing.onnx", step: "speech-model", sha256, urls: &[] }
    }

    fn dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("heyo-dictation-models-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn run(dir: &Path, file: &Pinned, urls: &[String]) -> (Result<(), Failure>, Vec<(String, u64, u64)>) {
        let seen = Mutex::new(Vec::new());
        let progress = |step: &str, received, total| seen.lock().unwrap().push((step.to_owned(), received, total));
        let result = tauri::async_runtime::block_on(download(dir, file, urls, &progress, &AtomicBool::new(false)));
        (result, seen.into_inner().unwrap())
    }

    #[test]
    fn a_file_is_fetched_from_the_first_address_that_has_it_and_checked() {
        let body = b"the model".repeat(1000);
        let server = serve(vec![("/model", 200, body.clone())]);
        let dir = dir("fetched");
        let file = pinned(&body);
        let (result, seen) = run(&dir, &file, &[format!("{server}/missing"), format!("{server}/model")]);
        assert_eq!(result, Ok(()));
        assert_eq!(std::fs::read(dir.join(file.path)).unwrap(), body);
        assert!(!dir.join("models/thing.onnx.part").exists());
        assert_eq!(seen.first(), Some(&("speech-model".to_owned(), seen[0].1, body.len() as u64)));
        // There already, and what it should be: nothing is fetched.
        let (result, seen) = run(&dir, &file, &[]);
        assert_eq!((result, seen.len()), (Ok(()), 0));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn what_is_not_the_pinned_file_never_gets_its_name() {
        let server = serve(vec![("/model", 200, b"something else".to_vec())]);
        let dir = dir("wrong");
        let file = pinned(b"the model");
        let (result, _) = run(&dir, &file, &[format!("{server}/model")]);
        let failure = result.unwrap_err();
        assert_eq!((failure.code, failure.particulars[0].clone()), ("download", ("step", "speech-model".to_owned())));
        assert!(failure.particulars[1].1.starts_with("what 127.0.0.1 sent is not the expected file"), "{failure}");
        assert!(!dir.join(file.path).exists() && !dir.join("models/thing.onnx.part").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_that_is_something_else_is_fetched_again() {
        let server = serve(vec![("/model", 200, b"the model".to_vec())]);
        let dir = dir("replaced");
        let file = pinned(b"the model");
        std::fs::create_dir_all(dir.join("models")).unwrap();
        std::fs::write(dir.join(file.path), b"half a mod").unwrap();
        assert_eq!(run(&dir, &file, &[format!("{server}/model")]).0, Ok(()));
        assert_eq!(std::fs::read(dir.join(file.path)).unwrap(), b"the model");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn where_no_address_has_it_the_last_answer_is_said() {
        let server = serve(vec![]);
        let dir = dir("missing");
        let (result, _) = run(&dir, &pinned(b"x"), &[format!("{server}/model")]);
        assert_eq!(crate::helper::worded(&result.unwrap_err().said(), "Dictation failed").full(), "Could not download: Speech model: 404 Not Found");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_files_are_where_the_installed_ones_are() {
        // The layout of installations made before there was a choice, which
        // keep working without a download.
        let installed = find("sense-voice-2024-07-17").unwrap();
        let Source::Files(files) = installed.source else { panic!("pinned files") };
        assert_eq!(files.iter().map(|file| file.path).collect::<Vec<_>>(), ["models/sense-voice/model.int8.onnx", "models/sense-voice/tokens.txt"]);
        assert_eq!(installed.parts.all().map(|part| format!("{}/{part}", installed.dir)).collect::<Vec<_>>(), ["models/sense-voice/model.int8.onnx", "models/sense-voice/tokens.txt"]);
        assert_eq!(VOICE_DETECTOR.path, "models/silero_vad.onnx");
        let mirror = |file: &Pinned| {
            std::env::set_var("HEYO_DICTATION_MIRROR", "http://127.0.0.1:8000/");
            let urls = pinned_addresses(file);
            std::env::remove_var("HEYO_DICTATION_MIRROR");
            urls
        };
        assert_eq!(mirror(&files[1]), ["http://127.0.0.1:8000/tokens.txt"]);
        assert_eq!(pinned_addresses(&VOICE_DETECTOR), VOICE_DETECTOR.urls);
    }

    #[test]
    fn every_model_is_pinned_and_has_a_place_of_its_own() {
        let mut ids = std::collections::BTreeSet::new();
        let mut dirs = std::collections::BTreeSet::new();
        for model in CATALOGUE {
            assert!(ids.insert(model.id) && dirs.insert(model.dir), "{} twice", model.id);
            assert!(model.dir.starts_with("models/") && !model.parts.tokens.is_empty(), "{}", model.id);
            let hashes: Vec<&str> = match &model.source {
                Source::Files(files) => files.iter().map(|file| file.sha256).collect(),
                Source::Archive { url, sha256, root } => {
                    // One asset of the release, by its name; its directory is named after it.
                    assert_eq!(*url, format!("https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/{root}.tar.bz2"));
                    vec![sha256]
                }
            };
            for hash in hashes {
                assert!(hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)), "{}: {hash}", model.id);
            }
            // Only the models that are told a language are given one.
            assert_eq!(model.language, matches!(model.kind, Kind::SenseVoice | Kind::Whisper), "{}", model.id);
        }
        assert!(find(DEFAULT).is_some());
    }

    /// A streaming model of four parts, fetched from `server`.
    fn archived(id: &'static str, sha256: &'static str, server: &str) -> Model {
        Model {
            id,
            name: "Test",
            languages: "",
            note: "",
            megabytes: 1,
            kind: Kind::Streaming,
            dir: Box::leak(format!("models/{id}").into_boxed_str()),
            parts: Parts { model: "", encoder: "encoder.onnx", decoder: "decoder.onnx", joiner: "joiner.onnx", tokens: "tokens.txt" },
            source: Source::Archive { url: Box::leak(format!("{server}/{id}.tar.bz2").into_boxed_str()), sha256, root: id },
            language: false,
            licence: "",
        }
    }

    /// A .tar.bz2 as the release has them: a directory of the model's
    /// files, and others besides.
    fn tarball(root: &str, files: &[&str]) -> Vec<u8> {
        let mut tar = tar::Builder::new(bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::fast()));
        for name in files {
            let body = format!("the {name} of {root}").repeat(2000);
            let mut header = tar::Header::new_gnu();
            header.set_size(body.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            tar.append_data(&mut header, format!("{root}/{name}"), body.as_bytes()).unwrap();
        }
        tar.into_inner().unwrap().finish().unwrap()
    }

    fn hashed(body: &[u8]) -> &'static str {
        Box::leak(hex(&Sha256::digest(body)).into_boxed_str())
    }

    fn install_of(dir: &Path, model: &Model, cancel: &AtomicBool) -> (Result<(), Failure>, Vec<String>) {
        let seen = Mutex::new(Vec::new());
        let progress = |step: &str, _, _| seen.lock().unwrap().push(step.to_owned());
        let result = tauri::async_runtime::block_on(install(dir, model, &progress, cancel));
        (result, seen.into_inner().unwrap())
    }

    #[test]
    fn a_model_is_downloaded_unpacked_and_deleted() {
        let all = ["encoder.onnx", "decoder.onnx", "joiner.onnx", "tokens.txt", "test_wavs/0.wav", "README.md"];
        let (body, short) = (tarball("tiny", &all), tarball("short", &all[..2]));
        let (tiny_sha, short_sha) = (hashed(&body), hashed(&short));
        let server = serve(vec![("/tiny.tar.bz2", 200, body), ("/short.tar.bz2", 200, short)]);
        let dir = dir("archive");
        let model = archived("tiny", tiny_sha, &server);
        assert!(!model.installed(&dir));
        let (result, steps) = install_of(&dir, &model, &AtomicBool::new(false));
        assert_eq!(result, Ok(()));
        assert!(model.installed(&dir));
        assert_eq!(steps.first().map(String::as_str), Some("speech-model"));
        assert!(steps.iter().any(|step| step == "unpacking"), "{steps:?}");
        // The parts and nothing else; nothing of the download left.
        let mut kept: Vec<_> = std::fs::read_dir(dir.join("models/tiny")).unwrap().map(|entry| entry.unwrap().file_name().into_string().unwrap()).collect();
        kept.sort();
        assert_eq!(kept, ["decoder.onnx", "encoder.onnx", "joiner.onnx", "tokens.txt"]);
        assert_eq!(std::fs::read_to_string(dir.join("models/tiny/tokens.txt")).unwrap(), "the tokens.txt of tiny".repeat(2000));
        assert_eq!(std::fs::read_dir(dir.join("models")).unwrap().count(), 1);
        delete(&dir, &model).unwrap();
        assert!(!model.installed(&dir) && !dir.join("models/tiny").exists());
        // An archive without all the parts installs nothing.
        let short = archived("short", short_sha, &server);
        let failure = install_of(&dir, &short, &AtomicBool::new(false)).0.unwrap_err();
        assert_eq!(failure.particulars[0], ("step", "unpacking".to_owned()));
        assert!(!short.installed(&dir));
        assert_eq!(std::fs::read_dir(dir.join("models")).unwrap().count(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_cancelled_download_leaves_nothing() {
        let body = tarball("tiny", &["encoder.onnx", "decoder.onnx", "joiner.onnx", "tokens.txt"]);
        let sha256 = hashed(&body);
        let server = serve(vec![("/tiny.tar.bz2", 200, body)]);
        let dir = dir("cancelled");
        let model = archived("tiny", sha256, &server);
        let (result, _) = install_of(&dir, &model, &AtomicBool::new(true));
        assert_eq!(result.unwrap_err().code, "cancelled");
        assert!(!model.installed(&dir));
        assert_eq!(std::fs::read_dir(dir.join("models")).map(|entries| entries.count()).unwrap_or(0), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_model_chosen_is_the_one_named_or_else_one_that_is_there() {
        let dir = dir("chosen");
        // Nothing installed: the default, to be downloaded.
        assert_eq!(chosen(&dir, "").id, DEFAULT);
        assert_eq!(chosen(&dir, "no-such-model").id, DEFAULT);
        // Only another one installed: that one, unless one is named.
        let streaming = find("x-asr-2026-06-05").unwrap();
        for part in streaming.parts.all() {
            std::fs::create_dir_all(streaming.part(&dir, part).parent().unwrap()).unwrap();
            std::fs::write(streaming.part(&dir, part), b"").unwrap();
        }
        assert_eq!(chosen(&dir, "").id, "x-asr-2026-06-05");
        assert_eq!(chosen(&dir, "whisper-turbo").id, "whisper-turbo");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

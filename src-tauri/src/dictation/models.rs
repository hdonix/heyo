//! The files the speech recognizer is made of, fetched on demand into
//! `dictation::dir()` so that what runs is what was chosen when Heyo was
//! built and not whatever the network hands over that day: each from an
//! address that names one revision, checked against a SHA-256 written here
//! before it gets its name.
//!
//! What goes wrong is said as a code and its particulars (`Failure`), and
//! each file's step of the installation as a code, which `helper::worded`
//! and `helper::step` word in the user's language.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

use super::engine::Failure;

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

// The hashes are of the files as downloaded from these addresses on
// 2026-10-10. Two of them can be compared with what the hosts publish: the
// model's with the `lfs.sha256` Hugging Face lists for it, the voice
// detector's with the digest GitHub lists for the release asset.
pub(crate) const SPEECH_MODEL: Pinned = Pinned {
    path: "models/sense-voice/model.int8.onnx",
    step: "speech-model",
    sha256: "c71f0ce00bec95b07744e116345e33d8cbbe08cef896382cf907bf4b51a2cd51",
    urls: &[sense_voice!("https://huggingface.co", "model.int8.onnx"), sense_voice!("https://hf-mirror.com", "model.int8.onnx")],
};

pub(crate) const TOKENS: Pinned = Pinned {
    path: "models/sense-voice/tokens.txt",
    step: "vocabulary",
    sha256: "f449eb28dc567533d7fa59be34e2abca8784f771850c78a47fb731a31429a1dc",
    urls: &[sense_voice!("https://huggingface.co", "tokens.txt"), sense_voice!("https://hf-mirror.com", "tokens.txt")],
};

pub(crate) const VOICE_DETECTOR: Pinned = Pinned {
    path: "models/silero_vad.onnx",
    step: "voice-detector",
    // A release asset can be replaced under the same address; the hash is what pins it.
    sha256: "9e2449e1087496d8d4caba907f23e0bd3f78d91fa552479bb9c23ac09cbb1fd6",
    urls: &["https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/silero_vad.onnx"],
};

pub(crate) const FILES: [&Pinned; 3] = [&SPEECH_MODEL, &TOKENS, &VOICE_DETECTOR];

/// Whether all of them are there (not whether they are what they should
/// be: that is checked when they are fetched).
pub(crate) fn installed(dir: &Path) -> bool {
    FILES.iter().all(|file| dir.join(file.path).exists())
}

/// Where a file is fetched from: its own addresses, or only the server
/// `HEYO_DICTATION_MIRROR` names, which has the files by their names (for
/// trying the download against a server of one's own).
fn addresses(file: &Pinned) -> Vec<String> {
    match std::env::var("HEYO_DICTATION_MIRROR") {
        Ok(mirror) if !mirror.is_empty() => {
            let name = file.path.rsplit('/').next().unwrap_or(file.path);
            vec![format!("{}/{name}", mirror.trim_end_matches('/'))]
        }
        _ => file.urls.iter().map(|url| url.to_string()).collect(),
    }
}

/// How an installation is going: a file's step, and bytes `received` of
/// `total` (0 where the server did not say).
pub(crate) type Progress<'a> = &'a (dyn Fn(&str, u64, u64) + Sync);

/// Makes sure every file is there and is the pinned one, downloading what
/// is missing or is something else.
pub(crate) async fn install(dir: &Path, progress: Progress<'_>) -> Result<(), Failure> {
    for file in FILES {
        download(dir, file, &addresses(file), progress).await?;
    }
    Ok(())
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
async fn download(dir: &Path, file: &Pinned, urls: &[String], progress: Progress<'_>) -> Result<(), Failure> {
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
    let partial = PathBuf::from(format!("{}.part", path.display()));
    let mut last_error = String::new();
    for url in urls {
        match fetch(url, &partial, file, progress).await {
            Ok(()) => return std::fs::rename(&partial, &path).map_err(|e| failure(&e)),
            Err(e) => {
                last_error = e;
                let _ = std::fs::remove_file(&partial);
            }
        }
    }
    Err(failure(&last_error))
}

/// Fetches `url` into `partial`, and says why that is not the pinned file
/// if it is not.
async fn fetch(url: &str, partial: &Path, file: &Pinned, progress: Progress<'_>) -> Result<(), String> {
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
        let result = tauri::async_runtime::block_on(download(dir, file, urls, &progress));
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
        // The layout of installations made before the engine moved into
        // the host, which keep working without a download.
        assert_eq!(FILES.map(|file| file.path), ["models/sense-voice/model.int8.onnx", "models/sense-voice/tokens.txt", "models/silero_vad.onnx"]);
        let mirror = |file: &Pinned| {
            std::env::set_var("HEYO_DICTATION_MIRROR", "http://127.0.0.1:8000/");
            let urls = addresses(file);
            std::env::remove_var("HEYO_DICTATION_MIRROR");
            urls
        };
        assert_eq!(mirror(&TOKENS), ["http://127.0.0.1:8000/tokens.txt"]);
        assert_eq!(addresses(&VOICE_DETECTOR), VOICE_DETECTOR.urls);
    }
}

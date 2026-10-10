//! The model that says what a text means, through the Node helper
//! (`runtime/src/embed.ts`). It is downloaded on demand, started when first
//! asked, kept running while it is being asked, and ends itself after a
//! while of not being.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::Engine;
use serde_json::{json, Value};
use tauri::AppHandle;

use crate::helper;

/// The model and everything about how it turns a text into a vector, as the
/// helper names it. Vectors are stored under it: when the helper comes to
/// make them another way it says another name, and the old ones are made again.
pub const MODEL: &str = "multilingual-e5-small-int8/1";

/// A model that is busy for longer than this with one request is not coming back.
const PATIENCE: Duration = Duration::from_secs(60);

/// What a text is to the model: something asked, or something to be found.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Role {
    Query,
    Passage,
}

/// Turns texts into vectors of length one.
pub trait Embed: Send + Sync {
    /// The name the vectors are stored under.
    fn model(&self) -> &str;
    /// A vector for each text, in order.
    fn embed(&self, role: Role, texts: &[String]) -> Result<Vec<Vec<f32>>, String>;
}

pub fn dir() -> PathBuf {
    crate::paths::data().join("embed")
}

/// Whether the model and something to run it with have been downloaded. The
/// runtime may be the one text recognition brought.
pub fn installed() -> bool {
    let runtime = |dir: PathBuf| dir.join("node_modules/onnxruntime-node").exists();
    ["model_quantized.onnx", "tokenizer.json"].iter().all(|file| dir().join("model").join(file).exists()) && (runtime(dir()) || runtime(crate::ocr::dir()))
}

fn with_places(mut command: Command) -> Command {
    command.env("HEYO_EMBED_DIR", dir()).env("HEYO_OCR_DIR", crate::ocr::dir());
    command
}

/// The helper as Heyo runs it.
pub fn command(app: &AppHandle) -> Result<Command, String> {
    helper::node(app, "heyo-embed.cjs", "HEYO_EMBED").map(with_places)
}

/// The helper without a running Heyo, for measuring from the command line:
/// the script `HEYO_EMBED` names, or the one in the repository.
pub fn command_alone() -> Result<Command, String> {
    let script = match std::env::var_os("HEYO_EMBED") {
        Some(path) => PathBuf::from(path),
        None if cfg!(debug_assertions) => PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../runtime/dist/heyo-embed.cjs")),
        None => return Err("set HEYO_EMBED to the path of heyo-embed.cjs".into()),
    };
    let node = crate::settings::Settings::load_from(&crate::paths::settings()).0.node();
    let mut command = crate::platform::common::quiet(node);
    command.arg("--no-deprecation").arg(script);
    Ok(with_places(command))
}

type Answer = mpsc::Sender<Value>;

#[derive(Default)]
struct Shared {
    /// The running helper, and which start of it this is.
    running: Mutex<Option<(u64, helper::Helper)>>,
    /// Who waits for the answer to which request, and which start it was put to.
    waiting: Mutex<HashMap<u64, (u64, Answer)>>,
}

/// The helper process, started when needed.
pub struct Model {
    start: Box<dyn Fn() -> Result<Command, String> + Send + Sync>,
    shared: Arc<Shared>,
    next: AtomicU64,
}

impl Model {
    /// `start` makes the command that runs the helper, each time it is to be started.
    pub fn new(start: impl Fn() -> Result<Command, String> + Send + Sync + 'static) -> Model {
        Model { start: Box::new(start), shared: Arc::default(), next: AtomicU64::new(1) }
    }

    /// Ends the helper, and with it the memory the model takes. Whoever is
    /// waiting for an answer is told there will be none.
    pub fn stop(&self) {
        self.shared.running.lock().unwrap().take();
    }

    /// Sends a request, starting the helper if it is not running. The
    /// answer, if one comes, goes to `answer`.
    fn send(&self, id: u64, request: &Value, answer: Answer) -> Result<(), String> {
        let mut running = self.shared.running.lock().unwrap();
        // One that has ended since it was last used fails here, once.
        for _ in 0..2 {
            if running.is_none() {
                let generation = self.next.fetch_add(1, Ordering::Relaxed);
                let (answers, ended) = (self.shared.clone(), self.shared.clone());
                let helper = helper::Helper::start(
                    (self.start)()?,
                    move |message| {
                        let waiter = message["id"].as_u64().and_then(|id| answers.waiting.lock().unwrap().remove(&id));
                        if let Some((_, waiter)) = waiter {
                            let _ = waiter.send(message);
                        }
                    },
                    move || {
                        let mut running = ended.running.lock().unwrap();
                        if running.as_ref().is_some_and(|(current, _)| *current == generation) {
                            *running = None;
                        }
                        // Nobody will answer what was put to this one now.
                        ended.waiting.lock().unwrap().retain(|_, (asked_of, _)| *asked_of != generation);
                    },
                )
                .map_err(|e| format!("could not start the language model: {e}"))?;
                *running = Some((generation, helper));
            }
            let (generation, helper) = running.as_mut().unwrap();
            self.shared.waiting.lock().unwrap().insert(id, (*generation, answer.clone()));
            match helper.send(request) {
                Ok(()) => return Ok(()),
                Err(_) => *running = None,
            }
        }
        Err("the language model is not running".into())
    }
}

/// The vectors in a helper's answer to a request for `count`.
fn vectors(answer: &Value, model: &str, count: usize) -> Result<Vec<Vec<f32>>, String> {
    if let Some(error) = answer["error"].as_str() {
        return Err(error.to_owned());
    }
    if answer["model"] != model {
        return Err(format!("the helper's model is {}, not {model}: Heyo and its helper scripts are of different versions", answer["model"]));
    }
    let size = answer["size"].as_u64().unwrap_or(0) as usize;
    let bytes = base64::engine::general_purpose::STANDARD.decode(answer["vectors"].as_str().unwrap_or_default()).map_err(|e| e.to_string())?;
    if count == 0 {
        return Ok(Vec::new());
    }
    if size == 0 || bytes.len() != count * size * 4 {
        return Err(format!("the language model answered with {} bytes for {count} texts of {size} numbers", bytes.len()));
    }
    Ok(bytes.chunks_exact(size * 4).map(|vector| vector.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect()).collect())
}

impl Embed for Model {
    fn model(&self) -> &str {
        MODEL
    }

    fn embed(&self, role: Role, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let (answer, answered) = mpsc::channel();
        let role = if role == Role::Query { "query" } else { "passage" };
        let sent = self.send(id, &json!({ "id": id, "role": role, "texts": texts }), answer);
        let result = sent.and_then(|()| match answered.recv_timeout(PATIENCE) {
            Ok(message) => vectors(&message, MODEL, texts.len()),
            Err(mpsc::RecvTimeoutError::Timeout) => Err("the language model did not answer".into()),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err("the language model stopped".into()),
        });
        self.shared.waiting.lock().unwrap().remove(&id);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packed(values: &[f32]) -> String {
        base64::engine::general_purpose::STANDARD.encode(values.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<u8>>())
    }

    #[test]
    fn an_answer_is_as_many_vectors_as_were_asked_for() {
        let answer = json!({ "id": 1, "model": "m", "size": 2, "vectors": packed(&[1.0, 0.0, 0.6, 0.8]) });
        assert_eq!(vectors(&answer, "m", 2).unwrap(), [[1.0, 0.0], [0.6, 0.8]]);
        // Not what was asked for.
        assert!(vectors(&answer, "m", 3).unwrap_err().contains("for 3 texts"));
        // Another model's vectors are not comparable with the ones stored.
        assert!(vectors(&answer, "other", 2).unwrap_err().contains("different versions"));
        assert_eq!(vectors(&json!({ "id": 1, "error": "not set up" }), "m", 1).unwrap_err(), "not set up");
        assert!(vectors(&json!({ "id": 1, "model": "m", "size": 2, "vectors": "!!" }), "m", 1).is_err());
        assert_eq!(vectors(&json!({ "id": 1, "model": "m", "size": 0, "vectors": "" }), "m", 0).unwrap(), Vec::<Vec<f32>>::new());
    }

    /// The real model, where it has been downloaded (Heyo Settings, or
    /// `HEYO_EMBED_DIR=$XDG_DATA_HOME/heyo/embed node runtime/dist/heyo-embed.cjs install`):
    /// `cargo test clipboard::embed::tests::the_real_model -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn the_real_model_tells_meanings_apart() {
        assert!(installed(), "the model is not in {}", dir().display());
        let model = Model::new(command_alone);
        let dot = |a: &[f32], b: &[f32]| a.iter().zip(b).map(|(x, y)| x * y).sum::<f32>();
        // A query, a text that says the same otherwise, and one about something else.
        let cases = [
            ("English", "how do I reset my password", "steps to recover a forgotten login credential", "recipe for pumpkin soup with ginger"),
            ("Chinese", "怎么重置密码", "忘记登录口令后的找回步骤", "南瓜汤的做法"),
            ("English for Chinese", "invoice for last month", "上个月的发票已经寄出，请查收", "会议纪要：周三下午讨论 Wayland 支持"),
            ("Chinese for English", "服务器登录命令", "ssh deploy@10.0.3.17 -p 2222", "The quarterly report is due on Friday"),
        ];
        for (what, query, alike, unlike) in cases {
            let asked = model.embed(Role::Query, &[query.to_owned()]).unwrap().remove(0);
            let texts = model.embed(Role::Passage, &[alike.to_owned(), unlike.to_owned()]).unwrap();
            assert_eq!(asked.len(), 384);
            assert!((dot(&asked, &asked) - 1.0).abs() < 1e-3, "{what}: not of length one");
            let (near, far) = (dot(&asked, &texts[0]), dot(&asked, &texts[1]));
            println!("{what:<20} alike {near:.3}   unlike {far:.3}");
            assert!(near > far, "{what}: {near} is not more than {far}");
        }
        // The same text gives the same vector, alone or among others.
        let alone = model.embed(Role::Passage, &["发票".to_owned()]).unwrap().remove(0);
        let among = model.embed(Role::Passage, &["a much longer text than the other one".to_owned(), "发票".to_owned()]).unwrap().remove(1);
        assert!(dot(&alone, &among) > 0.9999);
    }

    /// A stand-in helper: answers every request with one vector a text, the
    /// model's name and its own process id (so that a restart shows).
    #[cfg(unix)]
    fn stand_in(script: &'static str) -> Model {
        Model::new(move || {
            let mut command = Command::new("sh");
            command.args(["-c", script]);
            Ok(command)
        })
    }

    #[cfg(unix)]
    const ANSWERING: &str = r#"while read -r line; do
        id=$(printf '%s' "$line" | sed 's/.*"id":\([0-9]*\).*/\1/')
        printf '{"id":%s,"model":"multilingual-e5-small-int8/1","size":1,"vectors":"AACAPw=="}\n' "$id"
    done"#;

    #[cfg(unix)]
    #[test]
    fn the_helper_is_started_when_asked_and_again_when_it_has_ended() {
        let model = stand_in(ANSWERING);
        assert!(model.shared.running.lock().unwrap().is_none());
        assert_eq!(model.embed(Role::Query, &["one".into()]).unwrap(), [[1.0]]);
        let first = model.shared.running.lock().unwrap().as_ref().unwrap().0;
        // Still the same one.
        assert_eq!(model.embed(Role::Passage, &["two".into()]).unwrap(), [[1.0]]);
        assert_eq!(model.shared.running.lock().unwrap().as_ref().unwrap().0, first);
        model.stop();
        assert!(model.shared.running.lock().unwrap().is_none());
        assert_eq!(model.embed(Role::Query, &["three".into()]).unwrap(), [[1.0]]);
        assert_ne!(model.shared.running.lock().unwrap().as_ref().unwrap().0, first);
    }

    #[cfg(unix)]
    #[test]
    fn a_helper_that_ends_without_answering_is_not_waited_for() {
        // Reads the request and leaves, as one does that ran out of patience
        // or crashed.
        let model = stand_in("read -r line; exit 0");
        let started = std::time::Instant::now();
        assert_eq!(model.embed(Role::Query, &["one".into()]).unwrap_err(), "the language model stopped");
        assert!(started.elapsed() < Duration::from_secs(10));
        // And is noticed to be gone.
        for _ in 0..50 {
            if model.shared.running.lock().unwrap().is_none() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(model.shared.running.lock().unwrap().is_none());
        let none = Model::new(|| Err("no Node.js".into()));
        assert_eq!(none.embed(Role::Query, &["one".into()]).unwrap_err(), "no Node.js");
    }
}

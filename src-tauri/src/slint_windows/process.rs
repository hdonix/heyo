//! Slint on macOS, in a helper process whose main thread belongs to AppKit.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command as ProcessCommand, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::AppHandle;

use super::host::{Command, Host, Running};

pub(super) const ARGUMENT: &str = "--heyo-slint-ui";

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
enum FromChild {
    Call { id: u64, command: Command },
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
enum ToChild {
    Answer { id: u64, result: Result<Value, String> },
    Event { name: String, payload: Value },
    ShowSettings { system_dark: bool },
}

fn send(input: &Arc<Mutex<ChildStdin>>, message: &ToChild) -> Result<(), String> {
    let mut input = input.lock().map_err(|_| "the Slint helper's input is unavailable")?;
    serde_json::to_writer(&mut *input, message).map_err(|error| error.to_string())?;
    input.write_all(b"\n").and_then(|()| input.flush()).map_err(|error| error.to_string())
}

struct ChildUi {
    child: Child,
    input: Arc<Mutex<ChildStdin>>,
}

static CHILD: Mutex<Option<ChildUi>> = Mutex::new(None);

pub(super) fn open(app: &AppHandle) -> bool {
    let mut slot = CHILD.lock().unwrap();
    if slot.as_mut().is_some_and(|ui| ui.child.try_wait().ok().flatten().is_some()) {
        *slot = None;
    }
    if slot.is_none() {
        match start(app) {
            Ok(child) => *slot = Some(child),
            Err(error) => {
                eprintln!("heyo: could not start the macOS Slint UI: {error}");
                return false;
            }
        }
    }
    let Some(ui) = slot.as_ref() else { return false };
    send(
        &ui.input,
        &ToChild::ShowSettings {
            system_dark: crate::platform::system::dark().unwrap_or(false),
        },
    )
    .is_ok()
}

fn start(app: &AppHandle) -> Result<ChildUi, String> {
    let mut child = ProcessCommand::new(std::env::current_exe().map_err(|error| error.to_string())?)
        .arg(ARGUMENT)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|error| error.to_string())?;
    let input = Arc::new(Mutex::new(child.stdin.take().ok_or("the Slint helper has no input")?));
    let output = child.stdout.take().ok_or("the Slint helper has no output")?;

    let events = input.clone();
    Running(app.clone()).listen(Box::new(move |name, payload| {
        let _ = send(&events, &ToChild::Event { name: name.into(), payload });
    }));

    let calls = input.clone();
    let running = Running(app.clone());
    std::thread::Builder::new()
        .name("heyo-slint-ipc".into())
        .spawn(move || {
            for line in BufReader::new(output).lines().map_while(Result::ok) {
                let Ok(FromChild::Call { id, command }) = serde_json::from_str(&line) else {
                    eprintln!("heyo: the Slint helper sent an invalid message");
                    continue;
                };
                let result = running.call(command);
                if send(&calls, &ToChild::Answer { id, result }).is_err() {
                    break;
                }
            }
        })
        .map_err(|error| error.to_string())?;
    Ok(ChildUi { child, input })
}

type Listener = Arc<dyn Fn(&str, Value) + Send + Sync>;

struct Remote {
    output: Mutex<std::io::Stdout>,
    next: AtomicU64,
    pending: Mutex<HashMap<u64, mpsc::Sender<Result<Value, String>>>>,
    listener: Mutex<Option<Listener>>,
    dark: AtomicBool,
    show_on_start: AtomicBool,
}

impl Remote {
    fn new() -> Arc<Remote> {
        let remote = Arc::new(Remote {
            output: Mutex::new(std::io::stdout()),
            next: AtomicU64::new(1),
            pending: Mutex::new(HashMap::new()),
            listener: Mutex::new(None),
            dark: AtomicBool::new(false),
            show_on_start: AtomicBool::new(false),
        });
        let reading = remote.clone();
        std::thread::Builder::new()
            .name("heyo-slint-parent".into())
            .spawn(move || reading.read())
            .expect("a thread for the Slint helper's parent");
        remote
    }

    fn read(&self) {
        for line in std::io::stdin().lock().lines().map_while(Result::ok) {
            match serde_json::from_str::<ToChild>(&line) {
                Ok(ToChild::Answer { id, result }) => {
                    if let Some(waiting) = self.pending.lock().unwrap().remove(&id) {
                        let _ = waiting.send(result);
                    }
                }
                Ok(ToChild::Event { name, payload }) => {
                    if let Some(listener) = self.listener.lock().unwrap().clone() {
                        listener(&name, payload);
                    }
                }
                Ok(ToChild::ShowSettings { system_dark }) => {
                    self.dark.store(system_dark, Ordering::Relaxed);
                    self.show_on_start.store(true, Ordering::Release);
                    let _ = super::thread::on(|| super::window::reveal(std::time::Instant::now(), |_| {}));
                }
                Err(error) => eprintln!("heyo: invalid message from the primary process: {error}"),
            }
        }
        let _ = slint::quit_event_loop();
    }
}

impl Host for Remote {
    fn call(&self, command: Command) -> Result<Value, String> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let (sent, received) = mpsc::channel();
        self.pending.lock().unwrap().insert(id, sent);
        let mut output = self.output.lock().map_err(|_| "the primary process connection is unavailable")?;
        serde_json::to_writer(&mut *output, &FromChild::Call { id, command }).map_err(|error| error.to_string())?;
        output.write_all(b"\n").and_then(|()| output.flush()).map_err(|error| error.to_string())?;
        drop(output);
        received.recv().map_err(|_| String::from("the primary process stopped answering"))?
    }

    fn listen(&self, hear: Box<dyn Fn(&str, Value) + Send + Sync>) {
        *self.listener.lock().unwrap() = Some(Arc::from(hear));
    }

    fn system_dark(&self) -> bool {
        self.dark.load(Ordering::Relaxed)
    }

    fn forward(&self, _title: String) {}
    fn pill(&self, _did: super::pill::Did) {}
    fn screenshot(&self, _outcome: crate::screenshot::selecting::Outcome, _drawn: Option<image::RgbaImage>) {}
}

pub(super) fn run_child() {
    let settings = crate::settings::Settings::load();
    crate::i18n::choose(&settings.language);
    let host = Remote::new();
    let showing = host.clone();
    let (begun, heard) = mpsc::channel();
    std::thread::spawn(move || {
        if heard.recv().is_ok() && showing.show_on_start.swap(false, Ordering::AcqRel) {
            let _ = super::thread::on(|| super::window::reveal(std::time::Instant::now(), |_| {}));
        }
    });
    super::thread::run(host, begun);
}

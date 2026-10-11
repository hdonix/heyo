//! The Node.js process that runs extensions (`runtime/` in the repo).
//!
//! It is one of Heyo's helpers (`helper.rs`). Requests whose method starts with
//! `host.` are answered here and those of extensions (`ext.`) in `ext.rs`;
//! every other message is relayed to the webview as a `heyo://runtime`
//! event, and the webview replies through [`Runtime::send`].

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

use crate::helper::{self, Helper};
use crate::wire::{event, HostRequest};

#[derive(Default)]
pub struct Runtime {
    process: Mutex<Option<Process>>,
    spawned: AtomicU64,
    /// The process for menu bar and background commands, whose messages go
    /// to `menubar` instead of the webview.
    background: bool,
}

/// A second runtime process, so menu bar and background commands never
/// disturb the command open in the palette.
pub struct Background(pub Runtime);

impl Default for Background {
    fn default() -> Self {
        Background(Runtime { background: true, ..Runtime::default() })
    }
}

/// The runtime a process belongs to.
fn runtime_of(app: &AppHandle, background: bool, f: impl FnOnce(&Runtime)) {
    if background {
        f(&app.state::<Background>().0)
    } else {
        f(&app.state::<Runtime>())
    }
}

/// Tells both runtime processes, each if it is running: what either may
/// be waiting to hear. An error when neither is running.
pub(crate) fn tell_running(app: &AppHandle, message: &Value) -> Result<(), String> {
    let palette = app.state::<Runtime>().send_if_running(message);
    let background = app.state::<Background>().0.send_if_running(message);
    match (palette, background) {
        (Err(e), Err(_)) => Err(e),
        _ => Ok(()),
    }
}

struct Process {
    generation: u64,
    helper: Helper,
}

impl Runtime {
    /// Sends a message, starting the runtime if it is not running.
    pub fn send(&self, app: &AppHandle, message: &Value) -> Result<(), String> {
        let mut guard = self.process.lock().unwrap();
        if guard.is_none() {
            *guard = Some(spawn(app, self.spawned.fetch_add(1, Ordering::Relaxed), self.background)?);
        }
        write(&mut guard, message)
    }

    /// Answers a request from the running process; never starts a new one.
    fn reply(&self, message: &Value) -> Result<(), String> {
        write(&mut self.process.lock().unwrap(), message)
    }

    /// Sends a message to the running process; never starts a new one.
    pub fn send_if_running(&self, message: &Value) -> Result<(), String> {
        self.reply(message)
    }

    /// Stops the runtime (closing its stdin ends it); the next message
    /// starts a fresh one, e.g. with a different Node.js.
    pub fn stop(&self) {
        self.process.lock().unwrap().take();
    }

    fn exited(&self, generation: u64) {
        let mut guard = self.process.lock().unwrap();
        if guard.as_ref().is_some_and(|p| p.generation == generation) {
            *guard = None;
        }
    }
}

fn write(process: &mut Option<Process>, message: &Value) -> Result<(), String> {
    let Some(p) = process else { return Err("extension runtime is not running".into()) };
    if let Err(e) = p.helper.send(message) {
        *process = None;
        return Err(format!("extension runtime is not running: {e}"));
    }
    Ok(())
}

fn spawn(app: &AppHandle, generation: u64, background: bool) -> Result<Process, String> {
    let mut command = helper::node(app, "heyo-runtime.cjs", "HEYO_RUNTIME")?;
    command.envs(crate::paths::for_runtime()).envs(background.then_some(("HEYO_BACKGROUND", "1")));
    // What `environment` of `@heyo/api` says, which extensions read as they load.
    command
        .env("HEYO_VERSION", app.package_info().version.to_string())
        .env("HEYO_API_VERSION", crate::permissions::API_VERSION.to_string())
        .env("HEYO_SESSION", session_name())
        .env("HEYO_APPEARANCE", if app.state::<crate::SettingsState>().0.read().unwrap().dark() { "dark" } else { "light" });
    let (receiver, ended) = (app.clone(), app.clone());
    let helper = Helper::start(
        command,
        move |message| handle(&receiver, message, background),
        move || {
            runtime_of(&ended, background, |runtime| runtime.exited(generation));
            // Whatever its commands subscribed to has nobody to tell.
            crate::ext::runtime_exited(background);
            if background {
                crate::menubar::runtime_exited(&ended);
            } else {
                let _ = ended.emit(event::RUNTIME, json!({ "type": "exited" }));
            }
        },
    )
    .map_err(|e| {
        let node = helper::node_name(app);
        format!("failed to start the extension runtime ({node}): {e}. Extensions need Node.js 22 or newer; set its path in Heyo Settings.")
    })?;
    Ok(Process { generation, helper })
}

/// The session, as extensions are told of it (`Capabilities::session`).
fn session_name() -> &'static str {
    use crate::platform::Session;
    match crate::platform::session() {
        _ if cfg!(windows) => "windows",
        Session::X11 => "x11",
        Session::Wayland => "wayland",
        Session::Unknown => "unknown",
    }
}

/// Writes to the palette's runtime process or the background one, if it is
/// running: a reply to a request of its own, or an event it subscribed to.
pub(crate) fn reply_to(app: &AppHandle, background: bool, message: &Value) {
    runtime_of(app, background, |runtime| {
        if let Err(e) = runtime.reply(message) {
            eprintln!("heyo: {e}");
        }
    });
}

fn handle(app: &AppHandle, message: Value, background: bool) {
    let method = message["method"].as_str().unwrap_or_default().to_owned();
    // What an extension asks through `@heyo/api`: answered with care for
    // who asks, and off this thread (`ext.rs`).
    if message["type"] == "request" && method.starts_with("ext.") {
        return crate::ext::handle(app, message, background);
    }
    if message["type"] != "request" || !method.starts_with("host.") {
        // The runtime saying it is up; nothing waits for it.
        if message["type"] == "ready" {
            return;
        }
        if !background {
            let _ = app.emit(event::RUNTIME, message);
        } else if message["type"] == "request" {
            // Nobody is there to answer a dialog (`ui.confirm`): decline it.
            let reply = json!({ "type": "reply", "id": message["id"], "result": false });
            runtime_of(app, true, |runtime| drop(runtime.reply(&reply)));
        } else {
            crate::menubar::handle(app, &message);
        }
        return;
    }
    let id = message["id"].clone();
    let answer = serde_json::from_value::<HostRequest>(message)
        .map_err(|e| format!("the host cannot answer {method}: {e}"))
        .and_then(|request| match request {
            // The palette is the user's and the command's in it: a background
            // command neither hides nor shows it.
            request if background && request.moves_palette() => Ok(Value::Null),
            request => crate::host_call(app, request),
        });
    let reply = match answer {
        Ok(result) => json!({ "type": "reply", "id": id, "result": result }),
        Err(error) => json!({ "type": "reply", "id": id, "error": error }),
    };
    reply_to(app, background, &reply);
}

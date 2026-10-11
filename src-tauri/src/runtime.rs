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
            crate::ext::runtime_exited(&ended, background);
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

/// The answer to a request of the runtime's, as the runtime reads it: the
/// result, or what went wrong (a sentence, or an error with its code).
pub(crate) fn reply(id: Value, answer: Result<Value, impl serde::Serialize>) -> Value {
    match answer {
        Ok(result) => json!({ "type": "reply", "id": id, "result": result }),
        Err(error) => json!({ "type": "reply", "id": id, "error": error }),
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

/// Where a message of a runtime process goes.
#[derive(Debug, PartialEq)]
enum Route {
    /// What an extension asks through `@heyo/api`: answered with care for
    /// who asks, and off this thread (`ext.rs`).
    Extension,
    /// A request of the runtime's own (`host.rs`).
    Host(HostRequest),
    /// A request that is answered as it stands.
    Answered(Result<Value, String>),
    /// The runtime saying it is up; nothing waits for it.
    Ready,
    /// Anything else of the palette's process is for the palette.
    Webview,
    /// And of the other process, for the menu bar.
    MenuBar,
}

fn route(message: &Value, background: bool) -> Route {
    let method = message["method"].as_str().unwrap_or_default();
    let request = message["type"] == "request";
    if request && method.starts_with("ext.") {
        return Route::Extension;
    }
    if request && method.starts_with("host.") {
        return match serde_json::from_value::<HostRequest>(message.clone()) {
            // The palette is the user's and the command's in it: a background
            // command neither hides nor shows it.
            Ok(request) if background && request.moves_palette() => Route::Answered(Ok(Value::Null)),
            Ok(request) => Route::Host(request),
            Err(e) => Route::Answered(Err(format!("the host cannot answer {method}: {e}"))),
        };
    }
    match () {
        _ if message["type"] == "ready" => Route::Ready,
        _ if !background => Route::Webview,
        // Nobody is there to answer a dialog (`ui.confirm`): decline it.
        _ if request => Route::Answered(Ok(false.into())),
        _ => Route::MenuBar,
    }
}

fn handle(app: &AppHandle, message: Value, background: bool) {
    let answer = match route(&message, background) {
        Route::Extension => return crate::ext::handle(app, message, background),
        Route::Host(request) => crate::host_call(app, request),
        Route::Answered(answer) => answer,
        Route::Ready => return,
        Route::Webview => return drop(app.emit(event::RUNTIME, message)),
        Route::MenuBar => return crate::menubar::handle(app, &message),
    };
    reply_to(app, background, &reply(message["id"].clone(), answer));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::{ErrorCode, ExtError};

    #[test]
    fn a_reply_carries_a_result_or_what_went_wrong() {
        assert_eq!(reply(json!(41), Ok::<_, String>(json!([1]))), json!({ "type": "reply", "id": 41, "result": [1] }));
        assert_eq!(reply(json!(7), Err("the window is gone")), json!({ "type": "reply", "id": 7, "error": "the window is gone" }));
        // What an extension asked for fails with a code.
        assert_eq!(
            reply(json!(41), Err(ExtError::new(ErrorCode::Denied, "Window access was not allowed."))),
            json!({ "type": "reply", "id": 41, "error": { "code": "denied", "message": "Window access was not allowed." } })
        );
    }

    fn request(method: &str, params: Value) -> Value {
        json!({ "type": "request", "id": 3, "method": method, "params": params })
    }

    #[test]
    fn a_request_goes_to_whoever_answers_its_kind() {
        for background in [false, true] {
            assert_eq!(route(&request("ext.windows.list", Value::Null), background), Route::Extension);
            // One nobody knows too: it is refused there, with a code.
            assert_eq!(route(&request("ext.teleport", Value::Null), background), Route::Extension);
            assert_eq!(route(&request("host.hud", json!({ "text": "Done" })), background), Route::Host(HostRequest::Hud { text: "Done".into() }));
            let unknown = route(&request("host.format-disk", Value::Null), background);
            assert!(matches!(&unknown, Route::Answered(Err(why)) if why.starts_with("the host cannot answer host.format-disk")), "{unknown:?}");
            assert_eq!(route(&json!({ "type": "ready" }), background), Route::Ready);
        }
        // What the interface is asked (`ui.confirm`) and told (`render`) is the palette's.
        assert_eq!(route(&request("ui.confirm", json!({ "title": "Sure?" })), false), Route::Webview);
        assert_eq!(route(&json!({ "type": "render", "tree": {} }), false), Route::Webview);
        // A message that only looks like a request of the host's is no request.
        assert_eq!(route(&json!({ "type": "event", "method": "host.hide" }), false), Route::Webview);
    }

    #[test]
    fn a_background_command_has_no_palette_and_nobody_to_ask() {
        // The palette is neither hidden nor shown for it, and it is not told No.
        for method in ["host.hide", "host.show"] {
            assert_eq!(route(&request(method, Value::Null), true), Route::Answered(Ok(Value::Null)));
        }
        assert_eq!(route(&request("host.hide", Value::Null), false), Route::Host(HostRequest::Hide));
        assert_eq!(route(&request("host.show", Value::Null), false), Route::Host(HostRequest::Show));
        // A dialog is declined; what it draws is for the menu bar.
        assert_eq!(route(&request("ui.confirm", json!({ "title": "Sure?" })), true), Route::Answered(Ok(Value::Bool(false))));
        assert_eq!(route(&json!({ "type": "menubar", "items": [] }), true), Route::MenuBar);
    }
}

//! The settings window. It is drawn by Slint, not by the web view: on a
//! thread of its own in this process, beside Tauri's loop on the main
//! thread, from the first time the settings are asked for until Heyo ends
//! (winit's loop cannot be made a second time, so there is one, and closing
//! the window only hides it). If that thread is lost, the settings cannot
//! be shown again in this run, and the palette says so (`open_settings`).
//!
//! - `ui/` is the window, in Slint's language, drawn from the tokens of
//!   the web interface (`tokens`, written into it by `build.rs`).
//! - `model` works out what it shows from what the host's commands answer,
//!   and what to ask of the host when a control is used.
//! - `icons` are the symbols it draws for the host's icon names.
//! - `host` is how those commands are called, and the list of them.
//! - `window` puts the two together on Slint's thread.
//! - `fonts` gives Slint the fonts Heyo carries.
//! - `guard` keeps a panic on Slint's thread from ending Heyo.

mod area;
mod fonts;
mod guard;
mod host;
mod icons;
mod keys;
mod model;
#[cfg(test)]
mod tokens;
mod window;

#[allow(clippy::all, unused)]
mod ui {
    slint::include_modules!();
}

use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use tauri::AppHandle;

use guard::State;

/// Why the window cannot be shown in this run, once it cannot.
pub(crate) fn failure() -> String {
    guard::reason()
}

/// Opens the window, or brings it back. False when Slint cannot show it,
/// now or for the rest of this run (`failure` says why): the caller tells
/// the user.
pub(crate) fn open(app: &AppHandle) -> bool {
    let asked = Instant::now();
    if guard::moved(State::Idle, State::Starting) {
        return start(app, asked);
    }
    match guard::state() {
        State::Running => {
            if !window::reopen(asked) {
                guard::gone();
            }
            guard::state() == State::Running
        }
        // Asked for twice at once: the first asking shows it.
        State::Starting => true,
        State::Idle | State::Gone => false,
    }
}

/// Starts Slint's thread and waits to hear that its window is up.
fn start(app: &AppHandle, asked: Instant) -> bool {
    guard::install();
    let (started, outcome) = mpsc::channel();
    let host = Arc::new(host::Running(app.clone()));
    if let Err(e) = std::thread::Builder::new().name(guard::THREAD.into()).spawn(move || window::run(host, asked, started)) {
        eprintln!("heyo: no thread for Slint: {e}");
        guard::gone();
        return false;
    }
    // A thread that panicked says nothing more, and is not ended either
    // (`guard`): that it is gone is all there is to see.
    let failed = loop {
        match outcome.recv_timeout(Duration::from_millis(50)) {
            Ok(Ok(())) => break None,
            Ok(Err(reason)) => break Some(reason),
            Err(mpsc::RecvTimeoutError::Disconnected) => break Some("its thread ended".to_string()),
            Err(mpsc::RecvTimeoutError::Timeout) if guard::state() == State::Gone => break Some("its thread panicked".to_string()),
            Err(mpsc::RecvTimeoutError::Timeout) if asked.elapsed() > Duration::from_secs(30) => break Some("it did not start in 30 s".to_string()),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
    };
    match failed {
        None => guard::moved(State::Starting, State::Running),
        Some(reason) => {
            eprintln!("heyo: the settings window cannot be shown in this run: {reason}");
            guard::gone_because(&reason);
            false
        }
    }
}

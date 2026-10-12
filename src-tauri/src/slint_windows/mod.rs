//! The windows Slint draws, not the web view: the settings and the
//! dictation pill. They are on a thread of their own in this process
//! (`thread`), beside Tauri's loop on the main thread, from the first time
//! one is wanted until Heyo ends. If that thread is lost, neither can be
//! shown again in this run: the palette says so when the settings are asked
//! for (`open_settings`), and dictation goes on without its pill.
//!
//! - `ui/` is the window, in Slint's language, drawn from the tokens of
//!   the web interface (`tokens`, written into it by `build.rs`).
//! - `model` works out what it shows from what the host's commands answer,
//!   and what to ask of the host when a control is used.
//! - `icons` are the symbols it draws for the host's icon names.
//! - `host` is how those commands are called, and the list of them.
//! - `window` puts the two together on Slint's thread.
//! - `pill` is the dictation pill: what it shows, its window, and what a
//!   desktop must be told for it to float over everything unfocused.
//! - `screenshot` is the screenshot overlay: its window, and what a
//!   desktop must be told for it to cover a monitor and have every key.
//! - `reach` is where in its window a window takes the pointer.
//! - `fonts` gives Slint the fonts Heyo carries.
//! - `guard` keeps a panic on Slint's thread from ending Heyo.

mod area;
mod fonts;
mod guard;
mod host;
mod icons;
mod keys;
mod model;
pub(crate) mod pill;
mod reach;
pub(crate) mod screenshot;
mod thread;
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

/// Opens the settings window, or brings it back. False when Slint cannot
/// show it, now or for the rest of this run (`failure` says why): the
/// caller tells the user.
pub(crate) fn open(app: &AppHandle) -> bool {
    let asked = Instant::now();
    if !ensure(app) {
        return false;
    }
    let (shown, outcome) = mpsc::channel();
    let asked_on = thread::on(move || {
        window::reveal(asked, move |result| {
            let _ = shown.send(result);
        })
    });
    match if asked_on { heard(&outcome, asked) } else { Some("its thread ended".into()) } {
        None => true,
        Some(reason) => {
            eprintln!("heyo: the settings window cannot be shown in this run: {reason}");
            guard::gone_because(&reason);
            // With it goes whatever else Slint had on screen.
            thread::on(|| {
                let _ = slint::quit_event_loop();
            });
            false
        }
    }
}

/// Starts Slint's thread ahead of the first window, so that the first one
/// asked for is as quick as the ones after. It does not wait.
pub(crate) fn warm(app: &AppHandle) {
    let app = app.clone();
    let _ = std::thread::Builder::new().name("heyo-slint-warm".into()).spawn(move || {
        if ensure(&app) {
            // The pill is the window that cannot wait to be made, and the
            // screenshot overlay the other.
            thread::on(pill::prepare);
            thread::on(screenshot::prepare);
        }
    });
}

/// Sees to it that Slint's thread runs: starts it the first time, and
/// waits for it while another is starting it. False when it does not run,
/// and will not in this run.
fn ensure(app: &AppHandle) -> bool {
    let asked = Instant::now();
    if guard::moved(State::Idle, State::Starting) {
        return start(app, asked);
    }
    loop {
        match guard::state() {
            State::Running => return true,
            State::Idle | State::Gone => return false,
            State::Starting if asked.elapsed() > Duration::from_secs(30) => return false,
            State::Starting => std::thread::sleep(Duration::from_millis(2)),
        }
    }
}

/// Waits to hear how something asked of Slint's thread went: None when it
/// went well, else why not. A thread that panicked says nothing more, and
/// is not ended either (`guard`): that it is gone is all there is to see.
fn heard(outcome: &mpsc::Receiver<Result<(), String>>, asked: Instant) -> Option<String> {
    loop {
        match outcome.recv_timeout(Duration::from_millis(50)) {
            Ok(Ok(())) => return None,
            Ok(Err(reason)) => return Some(reason),
            Err(mpsc::RecvTimeoutError::Disconnected) => return Some("its thread ended".to_string()),
            Err(mpsc::RecvTimeoutError::Timeout) if guard::state() == State::Gone => return Some("its thread panicked".to_string()),
            Err(mpsc::RecvTimeoutError::Timeout) if asked.elapsed() > Duration::from_secs(30) => return Some("it did not answer in 30 s".to_string()),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
    }
}

/// Starts Slint's thread and waits to hear that its loop turns.
fn start(app: &AppHandle, asked: Instant) -> bool {
    guard::install();
    let (begun, outcome) = mpsc::channel();
    let host = Arc::new(host::Running(app.clone()));
    if let Err(e) = std::thread::Builder::new().name(guard::THREAD.into()).spawn(move || thread::run(host, begun)) {
        eprintln!("heyo: no thread for Slint: {e}");
        guard::gone();
        return false;
    }
    match heard(&outcome, asked) {
        None => {
            eprintln!("heyo: Slint began in {} ms", asked.elapsed().as_millis());
            guard::moved(State::Starting, State::Running)
        }
        Some(reason) => {
            eprintln!("heyo: Slint did not start, and its windows cannot be shown in this run: {reason}");
            guard::gone_because(&reason);
            false
        }
    }
}

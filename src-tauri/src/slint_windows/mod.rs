//! The windows Slint draws, not the web view: the settings, the dictation
//! pill, the screenshot overlay and the pinned captures. They are on a
//! thread of their own in this process (`thread`), beside Tauri's loop on
//! the main thread, from when Heyo starts until it ends. If that thread is
//! lost, none can be shown again in this run: the palette says so when the
//! settings are asked for (`open_settings`), dictation goes on without its
//! pill, and a screenshot comes to nothing.
//!
//! - `ui/` is the windows, in Slint's language, drawn from the tokens of
//!   the web interface (`tokens`, written into it by `build.rs`).
//! - `model` works out what the settings show from what the host's
//!   commands answer, and what to ask of the host when a control is used.
//! - `icons` are the symbols drawn for the host's icon names.
//! - `host` is how those commands are called, and the list of them.
//! - `window` is the settings window: it puts the two together.
//! - `pill` is the dictation pill: what it shows, and its window.
//! - `screenshot` is the screenshot overlay and the pinned captures.
//! - `standing` is what a desktop is told of each of them (that it floats
//!   over everything, that it never has the keyboard), said by the window
//!   as data and carried out once for each desktop.
//! - `life` is a window's life on Slint's thread, the same for each: its
//!   making, its going on and off the screen, its drawing at once.
//! - `reach` is where in its window the settings take the pointer.
//! - `pointer` keeps the pointer over them the size the desktop says.
//!
//! When the display changes while Heyo runs (a monitor connected, another
//! scale), every one of these windows is made again (`display_changed`).
//! - `fonts` gives Slint the fonts Heyo carries.
//! - `guard` keeps a panic on Slint's thread from ending Heyo.

mod area;
mod fonts;
mod guard;
mod host;
mod icons;
mod keys;
mod life;
mod model;
pub(crate) mod pill;
mod pointer;
mod reach;
pub(crate) mod screenshot;
mod standing;
mod thread;
#[cfg(test)]
mod tokens;
mod window;

#[allow(clippy::all, unused)]
mod ui {
    slint::include_modules!();
}

use std::sync::atomic::{AtomicU64, Ordering};
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
            // Said once: a window that could not be made was said by its slot.
            if !window::unmade_said() {
                eprintln!("heyo: the settings window cannot be shown in this run: {reason}");
            }
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
        // The pill is the window that cannot wait to be made, and the
        // screenshot overlay the other.
        life::begin(&app, || {
            pill::prepare();
            screenshot::prepare();
        });
    });
}

/// How long the desktop is given to finish saying how the display changed:
/// it says so piece by piece (the monitors, the scale, the pointer), each
/// a moment after the other.
const SETTLED: Duration = Duration::from_millis(400);

/// The display changed in some way (`platform::display`). From any
/// thread; it does not wait. What is done about it is done once, when
/// nothing more has been said for a moment.
pub(crate) fn display_changed() {
    static SAID: AtomicU64 = AtomicU64::new(0);
    let this = SAID.fetch_add(1, Ordering::SeqCst) + 1;
    let _ = std::thread::Builder::new().name("heyo-slint-display".into()).spawn(move || {
        std::thread::sleep(SETTLED);
        // A Slint that has not begun has made nothing for the old display.
        if SAID.load(Ordering::SeqCst) == this {
            life::ask(follow);
        }
    });
}

/// Makes Slint's windows again, for the display as it is now. On Slint's
/// thread.
///
/// A window that was there through the change cannot be relied on to
/// follow it. winit tells a window of another scale only if the monitor it
/// was on still goes by the same name, and a window that is off the
/// screen (the pill, the overlay) is on none: connect another monitor and
/// it draws for the old one, a screenshot's whole interface at half its
/// size. And a window that is told draws at the new scale in a window of
/// the old size, which the window manager will not let it change: the
/// settings showed a quarter of themselves, with nothing where it was
/// drawn. A window made now finds the display as it is.
fn follow() {
    eprintln!("heyo: the display changed: Slint's windows are made again for it");
    pointer::follow();
    life::again();
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

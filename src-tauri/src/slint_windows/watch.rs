//! The one watcher of whatever of Slint's is on the screen.
//!
//! A window of Slint's is drawn and answered by Slint's thread. When that
//! thread stops answering, what it had on the screen stays there as it
//! was last drawn: a pill above everything that nothing can press, a
//! pinned capture that cannot be closed, an overlay that covers the
//! screen and has every key. So while anything of Slint's is on the
//! screen, one thread asks Slint's whether it is there, and when it is
//! not, takes all of it off the screen from outside and says so to
//! whoever each window was for.
//!
//! The interface (docs/architecture.md has it in full): no window says
//! anything of this. `life` tells the watcher when a window goes on the
//! screen and when it goes off ([`on`], [`off`]), with what the window is
//! known by outside Slint's thread (`standing::Outside`) and what the
//! host is to hear if it was lost. What is decided is decided as values
//! ([`patience`], [`verdict`], [`order`]), which is what the tests are of.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::guard::{self, State};
use super::standing::{self, Outside};
use super::thread;

/// How often Slint's thread is asked whether it is there while something
/// of its is on the screen, and how many times it may not answer.
const BEAT: Duration = Duration::from_millis(250);
const MISSED_BEATS: u32 = 6;
/// How long a window that has just come on may keep the thread from
/// answering, while nothing on the screen has every key: its first
/// picture can take seconds (OpenGL's shaders made with Mesa's cache
/// empty), and a window that does not have the keys traps no one while
/// it is waited for.
const FIRST_PICTURE: Duration = Duration::from_secs(10);

/// A window on the screen, as the watcher knows it.
#[derive(Clone, Copy)]
pub(super) struct Watched {
    /// Which life's it is ([`id`]).
    pub id: u64,
    pub outside: Outside,
    /// It has every key: it is the first to go, and the thread is given
    /// no more than [`MISSED_BEATS`] while it is on the screen.
    pub keys: bool,
    /// When it came on the screen.
    pub since: Instant,
    /// What whoever it was for is told when it was lost. From the
    /// watcher's thread.
    pub lost: Option<fn()>,
}

/// What is on the screen.
static ON: Mutex<Vec<Watched>> = Mutex::new(Vec::new());
/// Counts answers from Slint's thread.
static BEATS: AtomicU64 = AtomicU64::new(0);
static WATCHING: AtomicBool = AtomicBool::new(false);

thread_local! {
    /// On Slint's thread: how each window on the screen is told that it
    /// was taken off from outside, should the thread ever go on.
    static BACK: RefCell<Vec<(u64, Rc<dyn Fn()>)>> = const { RefCell::new(Vec::new()) };
}

/// A number for a life, by which the watcher knows its window.
pub(super) fn id() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::SeqCst)
}

/// A window is on the screen. `back` is done on Slint's thread if the
/// window was taken off from outside and the thread then goes on: the
/// window is off the screen as far as its life knows from then. On
/// Slint's thread.
pub(super) fn on(window: Watched, back: impl Fn() + 'static) {
    BACK.with_borrow_mut(|all| {
        all.retain(|(id, _)| *id != window.id);
        all.push((window.id, Rc::new(back)));
    });
    if let Ok(mut on) = ON.lock() {
        on.retain(|known| known.id != window.id);
        on.push(window);
    }
    watch();
}

/// It is off the screen. On Slint's thread.
pub(super) fn off(id: u64) {
    BACK.with_borrow_mut(|all| all.retain(|(known, _)| *known != id));
    if let Ok(mut on) = ON.lock() {
        on.retain(|known| known.id != id);
    }
}

/// What has become of Slint's thread.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Verdict {
    /// It answers.
    There,
    /// It has not answered for too long: what it has on the screen is
    /// taken off from here.
    Stalled,
    /// It is gone for good, and its windows with it (`guard`).
    Gone,
}

/// How many beats the thread may miss with these windows on the screen,
/// at `now`: [`MISSED_BEATS`] whenever one of them has every key; else as
/// many as [`FIRST_PICTURE`] has while one of them came on less than that
/// long ago, and [`MISSED_BEATS`] after.
pub(super) fn patience(on: &[Watched], now: Instant) -> u32 {
    let keys = on.iter().any(|window| window.keys);
    let new = on.iter().any(|window| now.saturating_duration_since(window.since) < FIRST_PICTURE);
    if new && !keys {
        (FIRST_PICTURE.as_millis() / BEAT.as_millis()) as u32
    } else {
        MISSED_BEATS
    }
}

/// By whether it could be asked at all, what the guard says of it, how
/// many times running it has not answered, and how many it may not
/// ([`patience`]).
pub(super) fn verdict(asked: bool, state: State, missed: u32, patience: u32) -> Verdict {
    if !asked || state != State::Running {
        Verdict::Gone
    } else if missed >= patience {
        Verdict::Stalled
    } else {
        Verdict::There
    }
}

/// The order in which windows are taken off the screen: one that has
/// every key first, so that the keyboard is back before anything else;
/// the others as they came.
pub(super) fn order(mut on: Vec<Watched>) -> Vec<Watched> {
    on.sort_by_key(|window| !window.keys);
    on
}

fn anything() -> bool {
    ON.lock().is_ok_and(|on| !on.is_empty())
}

/// Starts the watcher, if it is not watching.
fn watch() {
    if WATCHING.swap(true, Ordering::SeqCst) {
        return;
    }
    let watching = std::thread::Builder::new().name("heyo-slint-watch".into()).spawn(|| loop {
        let mut missed = 0;
        let mut found = Verdict::There;
        while found == Verdict::There && anything() {
            let before = BEATS.load(Ordering::SeqCst);
            let asked = thread::on(|| {
                BEATS.fetch_add(1, Ordering::SeqCst);
            });
            std::thread::sleep(BEAT);
            missed = if BEATS.load(Ordering::SeqCst) == before { missed + 1 } else { 0 };
            let patience = ON.lock().map(|on| patience(&on, Instant::now())).unwrap_or(MISSED_BEATS);
            found = verdict(asked, guard::state(), missed, patience);
        }
        if found != Verdict::There {
            let taken = order(ON.lock().map(|mut on| std::mem::take(&mut *on)).unwrap_or_default());
            // A thread that is gone has had its windows taken away already (`guard`).
            if found == Verdict::Stalled && !taken.is_empty() {
                eprintln!("heyo: what draws Heyo's own windows does not answer; what it had on the screen is taken off ({} of them)", taken.len());
                let left = taken.iter().filter(|window| !standing::take_down(window.outside)).count();
                if left > 0 {
                    eprintln!("heyo: {left} of them could not be taken off the screen from outside");
                }
                let ids: Vec<u64> = taken.iter().map(|window| window.id).collect();
                // Heard only if the thread ever goes on.
                thread::on(move || back(&ids));
            }
            for lost in taken.iter().filter_map(|window| window.lost) {
                lost();
            }
        }
        WATCHING.store(false, Ordering::SeqCst);
        // A window that came on meanwhile found the watcher still there.
        if found == Verdict::Gone || !anything() || WATCHING.swap(true, Ordering::SeqCst) {
            return;
        }
    });
    if watching.is_err() {
        WATCHING.store(false, Ordering::SeqCst);
    }
}

/// Slint's thread goes on after all: the windows that were taken off the
/// screen meanwhile are told.
fn back(ids: &[u64]) {
    let told: Vec<Rc<dyn Fn()>> = BACK.with_borrow_mut(|all| {
        let (gone, kept): (Vec<_>, Vec<_>) = std::mem::take(all).into_iter().partition(|(id, _)| ids.contains(id));
        *all = kept;
        gone.into_iter().map(|(_, back)| back).collect()
    });
    for back in told {
        back();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(id: u64, keys: bool, since: Instant) -> Watched {
        Watched { id, outside: Outside::None, keys, since, lost: None }
    }

    #[test]
    fn a_thread_is_given_its_patience_to_answer() {
        assert_eq!(verdict(true, State::Running, 0, MISSED_BEATS), Verdict::There);
        assert_eq!(verdict(true, State::Running, MISSED_BEATS - 1, MISSED_BEATS), Verdict::There);
        assert_eq!(verdict(true, State::Running, MISSED_BEATS, MISSED_BEATS), Verdict::Stalled);
        assert_eq!(verdict(true, State::Running, MISSED_BEATS, 40), Verdict::There);
        assert_eq!(verdict(true, State::Running, 40, 40), Verdict::Stalled);
    }

    #[test]
    fn a_thread_that_is_gone_is_not_waited_for() {
        // Its loop no longer hears, or the guard has said so.
        assert_eq!(verdict(false, State::Running, 0, MISSED_BEATS), Verdict::Gone);
        assert_eq!(verdict(true, State::Gone, 0, 40), Verdict::Gone);
        assert_eq!(verdict(true, State::Gone, MISSED_BEATS, MISSED_BEATS), Verdict::Gone);
    }

    #[test]
    fn a_new_window_without_the_keys_is_given_ten_seconds_for_its_first_picture() {
        let now = Instant::now();
        let later = now + Duration::from_secs(3);
        assert_eq!(patience(&[window(1, false, now)], later), 40);
        // As much as is left of ten seconds from its coming on, then six beats.
        assert_eq!(patience(&[window(1, false, now)], now + FIRST_PICTURE - BEAT), 40);
        assert_eq!(patience(&[window(1, false, now)], now + FIRST_PICTURE), MISSED_BEATS);
        // One new among others that are not.
        assert_eq!(patience(&[window(1, false, now), window(2, false, later)], later + Duration::from_secs(8)), 40);
        assert_eq!(patience(&[], now), MISSED_BEATS);
    }

    #[test]
    fn with_a_window_that_has_the_keys_the_thread_is_given_six_beats_whatever_is_new() {
        let now = Instant::now();
        assert_eq!(patience(&[window(1, true, now)], now), MISSED_BEATS);
        assert_eq!(patience(&[window(1, false, now), window(2, true, now)], now + BEAT), MISSED_BEATS);
    }

    #[test]
    fn the_window_with_the_keys_goes_first_and_the_others_as_they_came() {
        let now = Instant::now();
        let window = |id, keys| window(id, keys, now);
        let taken = order(vec![window(1, false), window(2, false), window(3, true), window(4, false)]);
        assert_eq!(taken.iter().map(|window| window.id).collect::<Vec<_>>(), [3, 1, 2, 4]);
    }
}

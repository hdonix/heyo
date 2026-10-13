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
//! ([`verdict`], [`order`]), which is what the tests are of.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use super::guard::{self, State};
use super::standing::{self, Outside};
use super::thread;

/// How often Slint's thread is asked whether it is there while something
/// of its is on the screen, and how many times it may not answer.
const BEAT: Duration = Duration::from_millis(250);
const MISSED_BEATS: u32 = 6;

/// A window on the screen, as the watcher knows it.
#[derive(Clone, Copy)]
pub(super) struct Watched {
    /// Which life's it is ([`id`]).
    pub id: u64,
    pub outside: Outside,
    /// It has every key: it is the first to go.
    pub keys: bool,
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

/// By whether it could be asked at all, what the guard says of it, and
/// how many times running it has not answered.
pub(super) fn verdict(asked: bool, state: State, missed: u32) -> Verdict {
    if !asked || state != State::Running {
        Verdict::Gone
    } else if missed >= MISSED_BEATS {
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
            found = verdict(asked, guard::state(), missed);
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

    #[test]
    fn a_thread_is_given_six_beats_to_answer() {
        assert_eq!(verdict(true, State::Running, 0), Verdict::There);
        assert_eq!(verdict(true, State::Running, MISSED_BEATS - 1), Verdict::There);
        assert_eq!(verdict(true, State::Running, MISSED_BEATS), Verdict::Stalled);
    }

    #[test]
    fn a_thread_that_is_gone_is_not_waited_for() {
        // Its loop no longer hears, or the guard has said so.
        assert_eq!(verdict(false, State::Running, 0), Verdict::Gone);
        assert_eq!(verdict(true, State::Gone, 0), Verdict::Gone);
        assert_eq!(verdict(true, State::Gone, MISSED_BEATS), Verdict::Gone);
    }

    #[test]
    fn the_window_with_the_keys_goes_first_and_the_others_as_they_came() {
        let window = |id, keys| Watched { id, outside: Outside::None, keys, lost: None };
        let taken = order(vec![window(1, false), window(2, false), window(3, true), window(4, false)]);
        assert_eq!(taken.iter().map(|window| window.id).collect::<Vec<_>>(), [3, 1, 2, 4]);
    }
}

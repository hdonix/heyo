//! A window's life on Slint's thread, the same for every window: its
//! making, its going on and off the screen, its drawing at once, its
//! registration against the loss of Slint, and whether Slint is there to
//! be asked. A window says what it is and what it shows; nothing of this
//! is in the window.
//!
//! The interface (docs/architecture.md has it in full):
//!
//! - [`Slot`] is where the one window of a kind is kept on Slint's thread:
//!   made the first time it is used (`with`), and if it cannot be made,
//!   that is said once and not tried again. `there` never makes it.
//!   When the display changes, every window there is is made again
//!   ([`again`]), each saying what goes across to the new one.
//! - [`Life`] is one window: `ahead` has winit's window made before it is
//!   wanted, `on` and `off` say whether it is to be on the screen, `draw`
//!   has what changed seen now. A window never shows or hides Slint's
//!   window itself, never waits for winit's, and never asks winit to draw.
//! - [`ask`] and [`begin`] are how another thread has something done here:
//!   what is asked when Slint is not there is dropped, and said to be.
//!
//! That is the interface every window uses. Four more exist for one
//! window each, and are not to be taken for it:
//!
//! - `Life::made` and `Slot::give_up` are the overlay's. It comes on the
//!   screen only once its window has the monitor's size, so it must know
//!   whether winit's window is there to be sized; and it gives its window
//!   up a minute after a screenshot, because the window keeps a buffer of
//!   a whole screen.
//! - `Life::wanted` is read by the settings when they are made again, to
//!   be back on the screen if they were there.
//! - `Slot::why_not` is the settings' too: whoever asked for them is told
//!   why they cannot be had, which the palette then shows.
//!
//! What is decided is decided as values ([`Stage`], [`reach`]), which is
//! what the tests are of; Slint's window only carries the answers out.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::thread::LocalKey;

use slint::winit_030::winit::window::{Window, WindowId};
use slint::winit_030::WinitWindowAccessor;
use slint::ComponentHandle;
use tauri::AppHandle;

use crate::platform::desktop::OwnPlace;
use crate::platform::place::{self, Display, Monitor, Rect};
use super::guard::{self, State};
use super::standing::{self, Standing, Told};
use super::watch::{self, Watched};
use super::{pointer, thread};

// --- On and off the screen, as values ---------------------------------------------

/// What is to be done for a window to be where it is wanted.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Do {
    Nothing,
    /// Slint shows it, which has winit make its window; that is waited
    /// for, and `Stage::made` says what follows.
    Make,
    /// winit puts its window on the screen, or takes it off.
    PutOn,
    TakeOff,
    /// Slint shows it, or hides it.
    Show,
    Hide,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Made {
    No,
    Asked,
    Yes,
}

/// Where a window is in its life.
///
/// A kept window is made once and then only put on and taken off the
/// screen by winit: Slint, hiding a window, may make it anew when it is
/// next shown, without what the desktop was told of it (that it floats,
/// that it never has the keyboard). Any other window is left to Slint.
#[derive(Clone, Copy, Debug)]
pub(super) struct Stage {
    kept: bool,
    made: Made,
    wanted: bool,
}

impl Stage {
    pub(super) fn new(kept: bool) -> Self {
        Self { kept, made: Made::No, wanted: false }
    }

    /// Whether it is to be on the screen.
    pub(super) fn wanted(&self) -> bool {
        self.wanted
    }

    /// Whether winit's window is there, for a kept window.
    pub(super) fn made(&self) -> bool {
        self.made == Made::Yes
    }

    /// It is to be made ahead of when it is wanted.
    pub(super) fn ahead(&mut self) -> Do {
        if self.kept && self.made == Made::No {
            self.made = Made::Asked;
            Do::Make
        } else {
            Do::Nothing
        }
    }

    /// It is to be on the screen.
    pub(super) fn on(&mut self) -> Do {
        self.wanted = true;
        match (self.kept, self.made) {
            (false, _) => Do::Show,
            (true, Made::Yes) => Do::PutOn,
            // It comes on when it is made.
            (true, Made::Asked) => Do::Nothing,
            (true, Made::No) => {
                self.made = Made::Asked;
                Do::Make
            }
        }
    }

    /// It is to be off the screen.
    pub(super) fn off(&mut self) -> Do {
        self.wanted = false;
        match (self.kept, self.made) {
            (false, _) => Do::Hide,
            (true, Made::Yes) => Do::TakeOff,
            // Not there yet: it does not come on when it is.
            (true, _) => Do::Nothing,
        }
    }

    /// winit has made the window that `Do::Make` asked for. It was shown
    /// to be made, so it is on the screen until it is taken off.
    pub(super) fn arrived(&mut self) -> Do {
        self.made = Made::Yes;
        if self.wanted {
            Do::PutOn
        } else {
            Do::TakeOff
        }
    }

    /// It could not be made: it may be asked for again.
    pub(super) fn failed(&mut self) {
        self.made = Made::No;
    }
}

// --- One window ----------------------------------------------------------------------

type Then = Box<dyn FnOnce(Result<(), String>)>;

/// What a window is told of winit's window: each time it is about to be
/// seen, and whether this is the first time for that window of winit's.
type Arrive<C> = Rc<dyn Fn(&C, &Window, bool)>;

/// One window of Slint's, through its life.
pub(super) struct Life<C: ComponentHandle + 'static> {
    window: C,
    standing: Standing,
    /// What the desktop is told of it (`standing`), which also says
    /// whether winit keeps it.
    told: Told,
    stage: Rc<Cell<Stage>>,
    arrive: Arrive<C>,
    /// winit's window that `arrive` was last told of.
    known: Rc<Cell<Option<WindowId>>>,
    /// What waits for the window to be made.
    waiting: Rc<RefCell<Vec<Then>>>,
    /// What the watcher of Slint's thread knows it by, and what whoever
    /// it is for hears if it was lost with that thread (`watch`).
    watched: u64,
    lost: Rc<Cell<Option<fn()>>>,
}

/// Tells the watcher that a window is on the screen, and how it is told
/// if it was taken off from outside: as far as its life knows it is off
/// the screen from then.
fn seen<C: ComponentHandle + 'static>(window: &C, id: u64, told: Told, stage: &Rc<Cell<Stage>>, lost: Option<fn()>) {
    let outside = standing::outside(told, window.window());
    let (weak, stage) = (window.as_weak(), stage.clone());
    watch::on(Watched { id, outside, keys: told.keys != standing::Keys::Left, lost }, move || {
        if let Some(window) = weak.upgrade() {
            leave(&window, &stage, told);
        }
    });
}

/// Takes a window off the screen, as its stage has it done.
fn leave<C: ComponentHandle + 'static>(window: &C, stage: &Cell<Stage>, told: Told) {
    let mut now = stage.get();
    let what = now.off();
    stage.set(now);
    match what {
        Do::Hide => {
            let _ = window.hide();
        }
        Do::TakeOff => {
            window.window().with_winit_window(|made| standing::off(told, made));
        }
        _ => {}
    }
}

impl<C: ComponentHandle + 'static> Life<C> {
    /// Makes a window of that standing: the desktop is told of it
    /// whatever its standing asks, here and from now on. `arrive` is told
    /// of winit's window each time it is about to be seen: the window's
    /// place is its to say then.
    pub(super) fn new(standing: Standing, make: impl FnOnce() -> Result<C, slint::PlatformError>, arrive: impl Fn(&C, &Window, bool) + 'static) -> Result<Self, String> {
        let window = thread::making(standing, make).map_err(|e| format!("Slint could not make it: {e}"))?;
        // Left to itself Slint speaks the system's language from its first
        // window on: the language is Heyo's to choose.
        thread::speak();
        let told = standing::here(standing);
        Ok(Self { window, standing, told, stage: Rc::new(Cell::new(Stage::new(told.kept))), arrive: Rc::new(arrive), known: Rc::default(), waiting: Rc::default(), watched: watch::id(), lost: Rc::default() })
    }

    /// What whoever the window is for is told when it was on the screen
    /// and Slint's thread was lost: from another thread, once.
    pub(super) fn when_lost(&self, lost: fn()) {
        self.lost.set(Some(lost));
    }

    /// Where a window cannot say its standing of itself, asks the desktop
    /// for it (`standing::ask_desktop`): the window is known there by its
    /// title, and `at` is where it goes. To be said before it is shown.
    pub(super) fn ask_desktop(&self, title: &'static str, at: Option<OwnPlace>) {
        standing::ask_desktop(self.standing, self.told, self.window.window(), title, at);
    }

    /// Has `hear` told what the pointer does in a window that is not
    /// winit's (`standing::pointed_of`).
    pub(super) fn pointed(&self, hear: impl Fn(standing::Pointed) -> bool + 'static) {
        standing::pointed_of(self.told, self.window.window(), hear);
    }

    /// How the pointer is to look over the window (`standing::cursor_of`).
    pub(super) fn cursor(&self, icon: slint::winit_030::winit::window::CursorIcon) {
        standing::cursor_of(self.told, self.window.window(), icon);
    }

    /// The same for the keys (`standing::keyed_of`).
    pub(super) fn keyed(&self, hear: impl Fn(standing::Keyed) -> bool + 'static) {
        standing::keyed_of(self.told, self.window.window(), hear);
    }

    /// Begins moving the window by the pointer pressed in it: true where
    /// the desktop does from here on, false where the window is to be
    /// moved as the pointer goes (`move_by`).
    pub(super) fn drag(&self) -> bool {
        standing::drag(self.told, self.window.window())
    }

    /// Moves the window by so many of its device pixels, and says by how
    /// many it went.
    pub(super) fn move_by(&self, by: (f64, f64)) -> (f64, f64) {
        standing::move_by(self.told, self.window.window(), by)
    }

    /// What the desktop is told of it here, and what it cannot be told.
    pub(super) fn told(&self) -> Told {
        self.told
    }

    /// Takes every key for a window whose standing has it have them, now
    /// that it is on the screen. False when it was not given them yet: it
    /// is to be asked again. (They are given back when it goes off.)
    pub(super) fn hold(&self) -> bool {
        let told = self.told;
        self.window.window().with_winit_window(|made| standing::hold(told, made)).unwrap_or(true)
    }

    /// Has the window take the pointer in `within` only, in its own
    /// device pixels, or everywhere (`standing::reach`).
    pub(super) fn reach(&self, within: Option<Rect>) {
        standing::reach_of(self.told, self.window.window(), within);
    }

    /// Slint's window: for what it shows, never to show or hide it.
    pub(super) fn window(&self) -> &C {
        &self.window
    }

    /// Whether it is to be on the screen.
    pub(super) fn wanted(&self) -> bool {
        self.stage.get().wanted()
    }

    /// Whether winit's window is there to be put on the screen: for a
    /// window that is not kept, Slint sees to that when it shows it.
    pub(super) fn made(&self) -> bool {
        !self.told.kept || self.stage.get().made()
    }

    fn step(&self, step: impl FnOnce(&mut Stage) -> Do) -> Do {
        let mut stage = self.stage.get();
        let what = step(&mut stage);
        self.stage.set(stage);
        what
    }

    /// Has winit's window made ahead of when it is wanted, off the
    /// screen. `then` is done when it is there: at once if it is.
    pub(super) fn ahead(&self, then: impl FnOnce() + 'static) {
        if self.made() {
            return then();
        }
        self.waiting.borrow_mut().push(Box::new(move |made| {
            if made.is_ok() {
                then();
            }
        }));
        if self.step(Stage::ahead) == Do::Make {
            self.make();
        }
    }

    /// Puts it on the screen: at once if winit's window is there, when it
    /// is if not. `then` hears whether it came.
    pub(super) fn on(&self, then: impl FnOnce(Result<(), String>) + 'static) {
        match self.step(Stage::on) {
            Do::Show => {
                if let Err(e) = self.window.show() {
                    return then(Err(e.to_string()));
                }
                // A window that is not winit's has no window of winit's
                // to wait for: it is where its standing put it.
                if self.told.layered {
                    seen(&self.window, self.watched, self.told, &self.stage, self.lost.get());
                    return then(Ok(()));
                }
                self.waiting.borrow_mut().push(Box::new(then));
                self.wait(false);
            }
            Do::Make => {
                self.waiting.borrow_mut().push(Box::new(then));
                self.make();
            }
            Do::PutOn => {
                let (window, arrive, told) = (&self.window, &self.arrive, self.told);
                window.window().with_winit_window(|made| {
                    arrive(window, made, false);
                    standing::on(told, made);
                    made.request_redraw();
                });
                seen(&self.window, self.watched, self.told, &self.stage, self.lost.get());
                then(Ok(()));
            }
            _ => self.waiting.borrow_mut().push(Box::new(then)),
        }
    }

    /// Takes it off the screen.
    pub(super) fn off(&self) {
        watch::off(self.watched);
        leave(&self.window, &self.stage, self.told);
    }

    /// Has what changed in the window drawn at the next turn of the
    /// loop. Slint paces its drawing with a timer that first fires a
    /// frame's time after something changed (on X11 and on Windows): what
    /// a key, a click or the pointer did would be seen a frame late.
    pub(super) fn draw(&self) {
        if self.wanted() {
            self.window.window().with_winit_window(|made| made.request_redraw());
        }
    }

    /// Has Slint show the window, which makes winit's, and waits for it.
    fn make(&self) {
        if let Err(e) = self.window.show() {
            self.step(|stage| {
                stage.failed();
                Do::Nothing
            });
            for then in self.waiting.take() {
                then(Err(e.to_string()));
            }
            return;
        }
        self.wait(true);
    }

    /// Waits for winit's window, registers it, tells the window of it,
    /// and puts it where the stage says: `making` when it was shown only
    /// to be made.
    fn wait(&self, making: bool) {
        let (window, stage, arrive, known, waiting, told) = (self.window.as_weak(), self.stage.clone(), self.arrive.clone(), self.known.clone(), self.waiting.clone(), self.told);
        let (watched, lost) = (self.watched, self.lost.clone());
        let waited = slint::spawn_local(async move {
            let heard = |outcome: Result<(), String>| {
                for then in waiting.take() {
                    then(outcome.clone());
                }
            };
            let Some(window) = window.upgrade() else { return heard(Err("the window is gone".into())) };
            // Held no longer than this: a window still referred to stays
            // on the screen when Slint hides it.
            let made = match window.window().winit_window().await {
                Ok(made) => made,
                Err(e) => return heard(Err(e.to_string())),
            };
            // So that it can be taken off the screen if Slint is lost,
            // and its pointer follows the desktop's.
            guard::remember(&made);
            pointer::remember(&made);
            let first = known.replace(Some(made.id())) != Some(made.id());
            if first {
                standing::made(told, &made);
            }
            arrive(&window, &made, first);
            // Shown by Slint, it is on the screen; made ahead, it is if
            // it was wanted meanwhile.
            let mut on = !making && stage.get().wanted();
            if making {
                let mut now = stage.get();
                let what = now.arrived();
                stage.set(now);
                match what {
                    Do::PutOn => {
                        standing::on(told, &made);
                        made.request_redraw();
                        on = true;
                    }
                    Do::TakeOff => standing::off(told, &made),
                    _ => {}
                }
            }
            drop(made);
            if on {
                seen(&window, watched, told, &stage, lost.get());
            }
            heard(Ok(()));
        });
        if let Err(e) = waited {
            eprintln!("heyo: slint could not wait for a window: {e}");
        }
    }
}

impl<C: ComponentHandle + 'static> Drop for Life<C> {
    fn drop(&mut self) {
        watch::off(self.watched);
        // Slint keeps a window it was asked to show until it is asked to
        // hide it.
        let _ = self.window.hide();
    }
}

// --- The one window of a kind --------------------------------------------------------

/// A window there is one of.
pub(super) trait Lives: Sized + 'static {
    /// What it is called when something is said of it.
    const NAME: &'static str;
    /// Makes it. On Slint's thread.
    fn make() -> Result<Self, String>;
    /// It was made again, for a display that changed: what it showed goes
    /// across to the new one, which is on the screen again if this one
    /// was. On Slint's thread.
    fn carry(self, new: &mut Self);
}

thread_local! {
    /// How each slot that has had a window makes it again.
    static AGAIN: RefCell<Vec<fn()>> = const { RefCell::new(Vec::new()) };
}

/// Makes every window there is again, for the display as it is now: one
/// made for another display cannot be relied on to follow it
/// (`super::follow` says why). On Slint's thread.
pub(super) fn again() {
    let all = AGAIN.with(|all| all.borrow().clone());
    for one in all {
        one();
    }
}

/// A slot of the windows of one kind: `again` is told of it once.
pub(super) trait Slotted: Lives {
    fn slot() -> &'static LocalKey<Slot<Self>>;
}

/// Where the one window of a kind is kept, on Slint's thread.
pub(super) struct Slot<T> {
    kept: RefCell<Option<T>>,
    /// Why it could not be made: said once, and not tried again.
    failed: RefCell<Option<String>>,
    /// `again` knows of it.
    known: Cell<bool>,
}

impl<T: Slotted> Slot<T> {
    pub(super) const fn new() -> Self {
        Self { kept: RefCell::new(None), failed: RefCell::new(None), known: Cell::new(false) }
    }

    /// Does `work` with the window, making it first if this is the first
    /// time. None when there is none to do it with: it could not be made,
    /// or it is in the middle of something else.
    pub(super) fn with<R>(slot: &'static LocalKey<Self>, work: impl FnOnce(&mut T) -> R) -> Option<R> {
        slot.with(|slot| {
            let mut kept = slot.kept.try_borrow_mut().ok()?;
            if kept.is_none() && slot.failed.borrow().is_none() {
                match T::make() {
                    Ok(made) => {
                        *kept = Some(made);
                        if !slot.known.replace(true) {
                            AGAIN.with(|all| all.borrow_mut().push(|| Slot::remake(T::slot())));
                        }
                    }
                    Err(reason) => {
                        eprintln!("heyo: {} cannot be shown in this run: {reason}", T::NAME);
                        *slot.failed.borrow_mut() = Some(reason);
                    }
                }
            }
            kept.as_mut().map(work)
        })
    }

    /// Why there is no window, when it could not be made.
    pub(super) fn why_not(slot: &'static LocalKey<Self>) -> Option<String> {
        slot.with(|slot| slot.failed.borrow().clone())
    }

    /// Has the window made ahead of when it is wanted, where a window of
    /// that standing can exist unseen (one winit keeps). Elsewhere a
    /// window that exists is on the screen, and the desktop lists it:
    /// nothing is made until it is wanted.
    pub(super) fn ahead(slot: &'static LocalKey<Self>, standing: Standing, work: impl FnOnce(&mut T)) {
        if standing::here(standing).kept {
            Self::with(slot, work);
        }
    }

    /// Does `work` with the window if it has been made, and never makes
    /// it.
    pub(super) fn there<R>(slot: &'static LocalKey<Self>, work: impl FnOnce(&mut T) -> R) -> Option<R> {
        slot.with(|slot| slot.kept.try_borrow_mut().ok()?.as_mut().map(work))
    }

    /// Gives the window up, if `which` says so of it: the next use makes
    /// another.
    pub(super) fn give_up(slot: &'static LocalKey<Self>, which: impl FnOnce(&T) -> bool) -> Option<T> {
        slot.with(|slot| {
            let mut kept = slot.kept.try_borrow_mut().ok()?;
            kept.as_ref().is_some_and(which).then(|| kept.take()).flatten()
        })
    }

    /// Makes the window again, if there is one, and has the old one carry
    /// what it showed across to the new. One that was never made, or was
    /// given up, is left to be made when it is next used.
    fn remake(slot: &'static LocalKey<Self>) {
        let Some(old) = Self::give_up(slot, |_| true) else { return };
        let mut old = Some(old);
        Self::with(slot, |new| {
            if let Some(old) = old.take() {
                old.carry(new);
            }
        });
    }
}

// --- From other threads --------------------------------------------------------------

/// Whether something asked of Slint's thread can be done there.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Reach {
    /// Its loop turns.
    Now,
    /// It has not begun, or is beginning: it is started, or waited for.
    Started,
    /// It is not there, and what was asked is dropped.
    No,
}

/// What comes of asking Slint's thread for something, by the state it is
/// in and whether the asker may start it. Something that only tells a
/// window (a recording's level, a capture's windows) never starts Slint:
/// the window it tells is not there to be told.
pub(super) fn reach(state: State, may_start: bool) -> Reach {
    match state {
        State::Running => Reach::Now,
        State::Idle | State::Starting if may_start => Reach::Started,
        State::Idle | State::Starting | State::Gone => Reach::No,
    }
}

/// Has `work` done on Slint's thread if Slint is there. From any thread;
/// it does not wait. False when it was dropped.
pub(super) fn ask(work: impl FnOnce() + Send + 'static) -> bool {
    reach(guard::state(), false) == Reach::Now && thread::on(work)
}

/// Has `work` done on Slint's thread, starting Slint first if it has not
/// begun (which is waited for). From any thread. False when Slint cannot
/// be had, now or for the rest of this run.
pub(super) fn begin(app: &AppHandle, work: impl FnOnce() + Send + 'static) -> bool {
    match reach(guard::state(), true) {
        Reach::No => false,
        Reach::Now | Reach::Started => super::ensure(app) && thread::on(work),
    }
}

/// The display as winit has it at this moment, and the monitor the
/// window is on, for the window to pick its monitor from and ask where it
/// goes (`platform::place`). `rooms` says whether the room each monitor
/// has is wanted, which the desktop is asked for: without it a monitor's
/// room is all of it.
pub(super) fn display(made: &Window, rooms: bool) -> Display {
    let whole = |monitor: &slint::winit_030::winit::monitor::MonitorHandle| (monitor.position().x, monitor.position().y, monitor.size().width, monitor.size().height);
    let free = if rooms { place::kept_free() } else { None };
    let (its, primary) = (made.current_monitor().map(|monitor| whole(&monitor)), made.primary_monitor().map(|monitor| whole(&monitor)));
    let monitors: Vec<Monitor> = made.available_monitors().map(|monitor| Monitor::new(whole(&monitor), monitor.scale_factor(), Some(whole(&monitor)) == primary, free)).collect();
    Display { its: its.and_then(|its| monitors.iter().position(|monitor| monitor.whole == its)), monitors }
}

/// Has a window drawn clear where it draws nothing, or opaque, as what
/// draws it allows (`thread::seen_through`), and corrected when that is
/// only learnt at its first frame.
pub(super) fn seen_through(window: &slint::Window, renderer: &str, set_opaque: impl Fn(bool) + 'static) {
    // A window that is not winit's is not drawn by `renderer`: its
    // standing says whether it is seen through.
    if let Some(clear) = standing::drawn_clear(window) {
        return set_opaque(!clear);
    }
    set_opaque(!thread::seen_through(renderer));
    let set_opaque = Rc::new(set_opaque);
    thread::watch(window, renderer, move |clear| {
        // Not from inside the drawing it was learnt in.
        let set_opaque = set_opaque.clone();
        slint::Timer::single_shot(std::time::Duration::ZERO, move || set_opaque(!clear));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_kept_window_is_made_once_and_then_put_on_and_taken_off_by_winit() {
        let mut stage = Stage::new(true);
        assert_eq!(stage.on(), Do::Make);
        assert_eq!(stage.arrived(), Do::PutOn);
        assert_eq!(stage.off(), Do::TakeOff);
        assert_eq!(stage.on(), Do::PutOn);
        assert_eq!(stage.off(), Do::TakeOff);
        // Never by Slint, which would make it anew.
        assert!(![stage.on(), stage.off(), stage.ahead()].iter().any(|what| matches!(what, Do::Show | Do::Hide | Do::Make)));
    }

    #[test]
    fn a_window_made_ahead_stays_off_the_screen_until_it_is_wanted() {
        let mut stage = Stage::new(true);
        assert_eq!(stage.ahead(), Do::Make);
        // Asked for again while winit is making it: not made twice.
        assert_eq!(stage.ahead(), Do::Nothing);
        assert_eq!(stage.arrived(), Do::TakeOff);
        assert!(stage.made() && !stage.wanted());
        assert_eq!(stage.ahead(), Do::Nothing);
        assert_eq!(stage.on(), Do::PutOn);
    }

    #[test]
    fn a_window_wanted_while_it_is_being_made_comes_on_when_it_is() {
        let mut stage = Stage::new(true);
        assert_eq!(stage.ahead(), Do::Make);
        assert_eq!(stage.on(), Do::Nothing);
        assert_eq!(stage.arrived(), Do::PutOn);
    }

    #[test]
    fn a_window_no_longer_wanted_when_it_is_made_does_not_come_on() {
        let mut stage = Stage::new(true);
        assert_eq!(stage.on(), Do::Make);
        assert_eq!(stage.off(), Do::Nothing);
        assert_eq!(stage.arrived(), Do::TakeOff);
    }

    #[test]
    fn a_window_that_could_not_be_made_may_be_asked_for_again() {
        let mut stage = Stage::new(true);
        assert_eq!(stage.on(), Do::Make);
        stage.failed();
        assert_eq!(stage.on(), Do::Make);
    }

    #[test]
    fn a_window_that_is_not_kept_is_shown_and_hidden_by_slint() {
        let mut stage = Stage::new(false);
        assert_eq!(stage.ahead(), Do::Nothing);
        assert_eq!(stage.on(), Do::Show);
        assert!(stage.wanted());
        assert_eq!(stage.off(), Do::Hide);
        assert_eq!(stage.on(), Do::Show);
    }

    /// A stand-in for a window: it counts how often it was made, and can
    /// be told not to be makeable.
    struct Stand {
        shows: &'static str,
    }

    thread_local! {
        static STAND: Slot<Stand> = const { Slot::new() };
        static MADE: Cell<u32> = const { Cell::new(0) };
        static MAKEABLE: Cell<bool> = const { Cell::new(true) };
    }

    impl Lives for Stand {
        const NAME: &'static str = "the stand-in";

        fn make() -> Result<Self, String> {
            MADE.set(MADE.get() + 1);
            if MAKEABLE.get() {
                Ok(Stand { shows: "" })
            } else {
                Err("it is not makeable".into())
            }
        }

        fn carry(self, new: &mut Self) {
            new.shows = self.shows;
        }
    }

    impl Slotted for Stand {
        fn slot() -> &'static LocalKey<Slot<Self>> {
            &STAND
        }
    }

    // Each test has a thread of its own, and so a slot of its own.

    #[test]
    fn the_window_is_made_the_first_time_it_is_used_and_then_kept() {
        assert_eq!(Slot::there(&STAND, |_| ()), None);
        assert_eq!(MADE.get(), 0);
        assert_eq!(Slot::with(&STAND, |stand| stand.shows = "a page"), Some(()));
        assert_eq!(Slot::with(&STAND, |stand| stand.shows), Some("a page"));
        assert_eq!(Slot::there(&STAND, |stand| stand.shows), Some("a page"));
        assert_eq!(MADE.get(), 1);
    }

    #[test]
    fn a_window_that_cannot_be_made_is_not_tried_again() {
        assert_eq!(Slot::why_not(&STAND), None);
        MAKEABLE.set(false);
        assert_eq!(Slot::with(&STAND, |_| ()), None);
        MAKEABLE.set(true);
        assert_eq!(Slot::with(&STAND, |_| ()), None);
        assert_eq!(MADE.get(), 1);
        // And why is there for whoever asked for it.
        assert_eq!(Slot::why_not(&STAND).as_deref(), Some("it is not makeable"));
    }

    #[test]
    fn a_window_in_the_middle_of_something_is_not_done_another_thing_with() {
        let inner = Slot::with(&STAND, |_| Slot::with(&STAND, |_| ()));
        assert_eq!(inner, Some(None));
    }

    #[test]
    fn a_window_made_again_is_another_that_shows_what_the_old_one_showed() {
        // Never made: there is nothing to make again.
        again();
        assert_eq!(MADE.get(), 0);
        Slot::with(&STAND, |stand| stand.shows = "a page");
        again();
        assert_eq!(MADE.get(), 2);
        assert_eq!(Slot::there(&STAND, |stand| stand.shows), Some("a page"));
        // And again, each time the display changes.
        again();
        assert_eq!(MADE.get(), 3);
        assert_eq!(Slot::there(&STAND, |stand| stand.shows), Some("a page"));
    }

    #[test]
    fn a_window_that_was_given_up_is_not_made_again_until_it_is_used() {
        Slot::with(&STAND, |stand| stand.shows = "a page");
        assert!(Slot::give_up(&STAND, |_| true).is_some());
        again();
        assert_eq!(MADE.get(), 1);
        assert_eq!(Slot::there(&STAND, |_| ()), None);
    }

    #[test]
    fn a_window_given_up_is_made_anew_when_it_is_next_used() {
        Slot::with(&STAND, |stand| stand.shows = "a page");
        assert!(Slot::give_up(&STAND, |stand| stand.shows == "another").is_none());
        assert!(Slot::give_up(&STAND, |stand| stand.shows == "a page").is_some());
        assert_eq!(Slot::there(&STAND, |_| ()), None);
        assert_eq!(Slot::with(&STAND, |stand| stand.shows), Some(""));
    }

    #[test]
    fn what_is_asked_of_slint_is_dropped_when_it_is_not_there() {
        // Telling a window something never starts Slint.
        assert_eq!(reach(State::Running, false), Reach::Now);
        assert_eq!(reach(State::Idle, false), Reach::No);
        assert_eq!(reach(State::Starting, false), Reach::No);
        assert_eq!(reach(State::Gone, false), Reach::No);
        // Showing one does, unless Slint is lost for this run.
        assert_eq!(reach(State::Idle, true), Reach::Started);
        assert_eq!(reach(State::Starting, true), Reach::Started);
        assert_eq!(reach(State::Running, true), Reach::Now);
        assert_eq!(reach(State::Gone, true), Reach::No);
    }
}

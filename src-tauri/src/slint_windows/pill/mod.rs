//! The dictation pill: a small dark pill at the bottom of the screen that
//! says what a recording is doing. It is one of Slint's windows
//! (`super::thread`), so it shares their thread and their fate: if Slint
//! is lost, dictation goes on without it.
//!
//! - `model` is what it shows of a recording and what a press on it means.
//! - `native` is what a desktop is told for it to float unfocused.
//! - This file is the window: made the first time it is shown, placed at
//!   the bottom centre of the primary monitor, shown what the model says,
//!   and what is done to it told to the host.
//!
//! The host calls `show`, `tell` and `hide` from its own threads. None of
//! them waits for Slint, except the first `show` of a run when Slint was
//! not started ahead (`super::warm`, which also has the window made:
//! `prepare`). Showing it is then only putting on screen a window that is
//! there, placed, with the pill drawn in it from its first frame.

mod model;
pub(super) mod native;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use slint::winit_030::winit::dpi::PhysicalPosition;
use slint::winit_030::WinitWindowAccessor;
use slint::{ComponentHandle, Model as _, ModelRc, VecModel};
use tauri::AppHandle;

use super::guard::{self, State};
use super::thread::{self, Job, Kind};
use super::ui;
use crate::dictation::Told;

pub(crate) use model::Did;
use model::{Model, Phase};

/// How long it takes to go: it is taken off the screen when it has.
const LEAVING: Duration = Duration::from_millis(140);

struct Pill {
    window: ui::PillWindow,
    model: Model,
    bars: Rc<VecModel<f32>>,
    /// The host is told what was done to the pill on a thread kept for
    /// that alone: a press is said at once, whatever else is being asked.
    telling: mpsc::Sender<Job>,
    /// Whether it is to be on screen. It leaves a moment after it is not.
    wanted: bool,
    leaving: slint::Timer,
    /// Its window has been made, which Slint does when it first shows it.
    ready: bool,
}

thread_local! {
    static PILL: RefCell<Option<Pill>> = const { RefCell::new(None) };
    /// It could not be made: said once, and not tried again.
    static FAILED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Does `work` with the pill, on Slint's thread, making it first if this
/// is the first time.
fn with(work: impl FnOnce(&mut Pill)) {
    PILL.with(|slot| {
        let Ok(mut slot) = slot.try_borrow_mut() else { return };
        if slot.is_none() && !FAILED.with(std::cell::Cell::get) {
            match make() {
                Ok(pill) => *slot = Some(pill),
                Err(reason) => {
                    FAILED.with(|failed| failed.set(true));
                    eprintln!("heyo: dictation has no window in this run: {reason}");
                }
            }
        }
        if let Some(pill) = slot.as_mut() {
            work(pill);
        }
    });
}

fn make() -> Result<Pill, String> {
    let shared = thread::shared().ok_or("Slint has not begun")?;
    let telling = thread::asker("heyo-slint-pill", shared.host.clone())?;
    let window = thread::making(Kind::Pill, ui::PillWindow::new).map_err(|e| format!("Slint could not make it: {e}"))?;
    window.set_opaque(!thread::seen_through(&shared.renderer));
    let weak = window.as_weak();
    thread::watch(window.window(), &shared.renderer, move |clear| {
        let weak = weak.clone();
        slint::Timer::single_shot(Duration::ZERO, move || {
            if let Some(window) = weak.upgrade() {
                window.set_opaque(!clear);
            }
        });
    });
    let bars = Rc::new(VecModel::from(vec![0.0_f32; 5]));
    window.set_bars(ModelRc::from(bars.clone()));
    window.on_pressed(|| with(|pill| pill.did(pill.model.pressed())));
    window.on_clicked(|| with(|pill| pill.did(pill.model.clicked())));
    window.on_cancel(|| with(|pill| pill.did(pill.model.cancelled())));
    let mut pill = Pill { window, model: Model::new(), bars, telling, wanted: false, leaving: slint::Timer::default(), ready: false };
    pill.push();
    Ok(pill)
}

impl Pill {
    fn did(&self, did: Vec<Did>) {
        for did in did {
            let _ = self.telling.send(Box::new(move |host| host.pill(did)));
        }
    }

    /// Shows what the model says.
    fn push(&mut self) {
        let (settled, changing) = self.model.words();
        self.window.set_words(settled.into());
        self.window.set_partial(changing.into());
        self.window.set_saying(self.model.saying().into());
        self.window.set_recording(self.model.recording());
        self.window.set_phase(
            match self.model.phase {
                Phase::Loading => "loading",
                Phase::Listening => "listening",
                Phase::Polishing => "polishing",
                Phase::Done => "done",
                Phase::Error => "error",
            }
            .into(),
        );
        for (at, bar) in self.model.bars().into_iter().enumerate() {
            if self.bars.row_data(at) != Some(bar) {
                self.bars.set_row_data(at, bar);
            }
        }
    }

    /// Has Slint make the window, which it does by showing it: with
    /// nothing drawn in it, and taken off the screen again at once, in its
    /// place. `then` is done when it is there.
    fn prepare(&mut self, then: impl FnOnce() + 'static) {
        if let Err(e) = self.window.show() {
            return eprintln!("heyo: the dictation window could not be made: {e}");
        }
        let window = self.window.as_weak();
        let waiting = slint::spawn_local(async move {
            let Some(window) = window.upgrade() else { return };
            let Ok(made) = window.window().winit_window().await else { return };
            // So that it can be taken off the screen if Slint is lost.
            guard::remember(&made);
            native::made(&made);
            place(&made);
            // Not when it was wanted meanwhile: it is about to be shown.
            let mut wanted = false;
            with(|pill| {
                pill.ready = true;
                wanted = pill.wanted;
            });
            if !wanted {
                native::hide(&made);
            }
            then();
        });
        if let Err(e) = waiting {
            eprintln!("heyo: slint could not wait for the dictation window: {e}");
        }
    }

    /// Puts it on screen. To Slint it is shown once and stays so (see
    /// `native`): this is winit putting the window there.
    fn show(&mut self, asked: Instant) {
        self.wanted = true;
        self.leaving.stop();
        if !native::kept() {
            self.window.set_leaving(false);
            self.window.set_shown(true);
            if let Err(e) = self.window.show() {
                eprintln!("heyo: the dictation window could not be shown: {e}");
            }
            return;
        }
        if !self.ready {
            return self.prepare(move || {
                with(|pill| {
                    if pill.wanted {
                        pill.show(asked);
                    }
                })
            });
        }
        // Said before it is on screen: its first frame there has the pill
        // in it, arriving.
        self.window.set_leaving(false);
        self.window.set_shown(true);
        self.window.window().with_winit_window(|made| {
            place(made);
            native::show(made);
            made.request_redraw();
        });
    }

    /// Has it go, and takes it off the screen when it has gone.
    fn hide(&mut self) {
        self.wanted = false;
        self.window.set_shown(false);
        self.window.set_leaving(true);
        self.leaving.start(slint::TimerMode::SingleShot, LEAVING, || {
            with(|pill| {
                if !pill.wanted {
                    if native::kept() {
                        pill.window.window().with_winit_window(native::hide);
                    } else {
                        let _ = pill.window.hide();
                    }
                    // Off the screen, it is where it arrives from.
                    pill.window.set_leaving(false);
                }
            });
        });
    }
}

/// At the bottom centre of the primary monitor, a tenth of the monitor's
/// height above its bottom.
fn place(made: &slint::winit_030::winit::window::Window) {
    let monitor = made.primary_monitor().or_else(|| made.current_monitor()).or_else(|| made.available_monitors().next());
    if let Some(monitor) = monitor {
        let (origin, area, size) = (monitor.position(), monitor.size(), made.outer_size());
        let x = origin.x + (area.width as i32 - size.width as i32) / 2;
        let y = origin.y + area.height as i32 - size.height as i32 - area.height as i32 / 10;
        made.set_outer_position(PhysicalPosition::new(x, y));
    }
}

/// Has the pill's window made ahead of the first recording. On Slint's
/// thread.
pub(super) fn prepare() {
    with(|pill| {
        if !pill.ready && native::kept() {
            pill.prepare(|| {});
        }
    });
}

/// Shows the pill. From any thread; it waits only for Slint to start, if
/// it has not.
pub(crate) fn show(app: &AppHandle) {
    let asked = Instant::now();
    if super::ensure(app) {
        thread::on(move || with(|pill| pill.show(asked)));
    }
}

/// Tells the pill of the recording. From any thread; it does not wait.
pub(crate) fn tell(told: Told) {
    if guard::state() == State::Running {
        thread::on(move || {
            with(|pill| {
                pill.model.told(&told);
                pill.push();
            })
        });
    }
}

/// Takes the pill away. From any thread; it does not wait.
pub(crate) fn hide() {
    if guard::state() == State::Running {
        thread::on(|| with(Pill::hide));
    }
}

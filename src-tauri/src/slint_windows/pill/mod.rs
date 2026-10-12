//! The dictation pill: a small dark pill at the bottom of the screen that
//! says what a recording is doing. It is one of Slint's windows
//! (`super::thread`), so it shares their thread and their fate: if Slint
//! is lost, dictation goes on without it.
//!
//! - `model` is what it shows of a recording and what a press on it means.
//! - This file is the window: placed at the bottom centre of the primary
//!   monitor, shown what the model says, and what is done to it told to
//!   the host. Its making and its going on and off the screen are
//!   `super::life`'s.
//!
//! The host calls `show`, `tell` and `hide` from its own threads. None of
//! them waits for Slint, except the first `show` of a run when Slint was
//! not started ahead (`super::warm`, which also has the window made:
//! `prepare`). Showing it is then only putting on screen a window that is
//! there, placed, with the pill drawn in it from its first frame.

mod model;

use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use slint::winit_030::winit::dpi::PhysicalPosition;
use slint::{ComponentHandle, Model as _, ModelRc, VecModel};
use tauri::AppHandle;

use super::life::{self, Life, Lives, Slot, Slotted};
use super::area::Rect;
use super::standing::{self, Keyboard, Layer, Standing};
use super::thread::{self, Job};
use super::ui;
use crate::dictation::Told;

pub(crate) use model::Did;
use model::{Model, Phase};

/// It floats over everything, a fullscreen window too, and never has the
/// keyboard: not by appearing, not by being clicked. What is dictated
/// goes to the window that has it.
const STANDING: Standing = Standing { layer: Layer::Over, keyboard: Keyboard::Never, listed: false, clear: true };

/// The window, and how high the pill is in it, in logical pixels: as the
/// markup has them. The window is wider than the pill, which grows with
/// the words to at most the window's width.
const WINDOW: (f32, f32) = (360.0, 44.0);
const HIGH: f32 = 30.0;

/// Where the pill is in its window, in the window's device pixels, when
/// it is `wide` logical pixels wide: in the middle. That is where the
/// window takes the pointer; the rest of it is seen through, and a press
/// there belongs to what is seen there.
fn body(wide: f32, scale: f32) -> Rect {
    let wide = wide.clamp(0.0, WINDOW.0);
    let (left, top) = (((WINDOW.0 - wide) / 2.0 * scale).floor(), ((WINDOW.1 - HIGH) / 2.0 * scale).floor());
    let (right, bottom) = (((WINDOW.0 + wide) / 2.0 * scale).ceil(), ((WINDOW.1 + HIGH) / 2.0 * scale).ceil());
    (left as i32, top as i32, (right - left) as u32, (bottom - top) as u32)
}

/// How long it takes to go: it is taken off the screen when it has.
const LEAVING: Duration = Duration::from_millis(140);

struct Pill {
    life: Life<ui::PillWindow>,
    model: Model,
    bars: Rc<VecModel<f32>>,
    /// The host is told what was done to the pill on a thread kept for
    /// that alone: a press is said at once, whatever else is being asked.
    telling: mpsc::Sender<Job>,
    /// Whether a recording has it up. It leaves the screen a moment after
    /// it no longer has.
    wanted: bool,
    /// How wide the pill was when the desktop was last told where it is.
    wide: Option<f32>,
    leaving: slint::Timer,
}

thread_local! {
    static PILL: Slot<Pill> = const { Slot::new() };
}

/// Does `work` with the pill, on Slint's thread, making it first if this
/// is the first time.
fn with(work: impl FnOnce(&mut Pill)) {
    Slot::with(&PILL, work);
}

impl Lives for Pill {
    const NAME: &'static str = "the dictation pill";

    fn make() -> Result<Self, String> {
        let shared = thread::shared().ok_or("Slint has not begun")?;
        let telling = thread::asker("heyo-slint-pill", shared.host.clone())?;
        let life = Life::new(STANDING, ui::PillWindow::new, |window, made, _| {
            place(made);
            standing::reach(made, Some(body(window.get_body_width(), window.window().scale_factor())));
        })?;
        let window = life.window();
        let weak = window.as_weak();
        life::seen_through(window.window(), &shared.renderer, move |opaque| {
            if let Some(window) = weak.upgrade() {
                window.set_opaque(opaque);
            }
        });
        let bars = Rc::new(VecModel::from(vec![0.0_f32; 5]));
        window.set_bars(ModelRc::from(bars.clone()));
        window.on_pressed(|| with(|pill| pill.did(pill.model.pressed())));
        window.on_clicked(|| with(|pill| pill.did(pill.model.clicked())));
        window.on_cancel(|| with(|pill| pill.did(pill.model.cancelled())));
        let mut pill = Pill { life, model: Model::new(), bars, telling, wanted: false, wide: None, leaving: slint::Timer::default() };
        pill.push();
        Ok(pill)
    }

    /// What the pill shows is kept, and it is back on screen if a
    /// recording has it there.
    fn carry(self, new: &mut Self) {
        new.model = self.model;
        new.push();
        if self.wanted {
            new.show();
        } else {
            new.life.ahead(|| {});
        }
    }
}

impl Slotted for Pill {
    fn slot() -> &'static std::thread::LocalKey<Slot<Self>> {
        &PILL
    }
}

impl Pill {
    fn did(&self, did: Vec<Did>) {
        for did in did {
            let _ = self.telling.send(Box::new(move |host| host.pill(did)));
        }
    }

    /// Shows what the model says.
    fn push(&mut self) {
        let window = self.life.window();
        let (settled, changing) = self.model.words();
        window.set_words(settled.into());
        window.set_partial(changing.into());
        window.set_saying(self.model.saying().into());
        window.set_note(self.model.note().into());
        window.set_recording(self.model.recording());
        window.set_phase(
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
        // The pill is as wide as what it says: the pointer is the
        // window's where the pill is.
        let wide = window.get_body_width();
        if self.wide != Some(wide) {
            self.wide = Some(wide);
            self.life.reach(Some(body(wide, window.window().scale_factor())));
        }
        self.life.draw();
    }

    /// Puts it on screen, arriving: said before it is there, so that its
    /// first frame on screen has the pill in it.
    fn show(&mut self) {
        self.wanted = true;
        self.leaving.stop();
        self.life.window().set_leaving(false);
        self.life.window().set_shown(true);
        self.life.on(|came| {
            if let Err(e) = came {
                eprintln!("heyo: the dictation pill could not be shown: {e}");
            }
        });
    }

    /// Has it go, and takes it off the screen when it has gone.
    fn hide(&mut self) {
        self.wanted = false;
        self.life.window().set_shown(false);
        self.life.window().set_leaving(true);
        self.life.draw();
        self.leaving.start(slint::TimerMode::SingleShot, LEAVING, || {
            with(|pill| {
                if !pill.wanted {
                    pill.life.off();
                    // Off the screen, it is where it arrives from.
                    pill.life.window().set_leaving(false);
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
    with(|pill| pill.life.ahead(|| {}));
}

/// Shows the pill. From any thread; it waits only for Slint to start, if
/// it has not.
pub(crate) fn show(app: &AppHandle) {
    life::begin(app, || with(Pill::show));
}

/// Tells the pill of the recording. From any thread; it does not wait.
pub(crate) fn tell(told: Told) {
    life::ask(move || {
        with(|pill| {
            pill.model.told(&told);
            pill.push();
        })
    });
}

/// Takes the pill away. From any thread; it does not wait.
pub(crate) fn hide() {
    // One that was never made is not made to be taken away.
    life::ask(|| {
        Slot::there(&PILL, Pill::hide);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pill_takes_the_pointer_where_it_is_drawn_and_nowhere_else_in_its_window() {
        // "Listening", about a third of the window, at scale 1 and at 2.
        assert_eq!(body(110.0, 1.0), (125, 7, 110, 30));
        assert_eq!(body(110.0, 2.0), (250, 14, 220, 60));
        // Whole pixels that cover it, where it does not fall on them.
        assert_eq!(body(111.0, 1.0), (124, 7, 112, 30));
        assert_eq!(body(110.0, 1.25), (156, 8, 138, 39));
        // Never wider than its window, whatever is said.
        assert_eq!(body(500.0, 1.0), (0, 7, 360, 30));
    }
}

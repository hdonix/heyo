//! The settings window itself: made on Slint's thread, filled from the
//! model, and what is done in it carried to the host.
//!
//! Everything here runs on Slint's thread (`thread`) except what is asked
//! of the host, which is asked on a thread of its own (`asking`) and its
//! answer brought back to Slint's.
//!
//! Nothing is decided here: a control that was used is handed to the model
//! (`Model::acted`), which says what to ask of the host; the answer is
//! handed back to it (`Model::answered`); and the window is shown what the
//! model then says (`push`).

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::Value;
use slint::winit_030::winit::dpi::PhysicalPosition;
use slint::winit_030::winit::event::{ElementState, WindowEvent};
use slint::winit_030::winit::keyboard::{KeyCode, PhysicalKey};
use slint::winit_030::{EventResult, WinitWindowAccessor};
use slint::{ComponentHandle, ModelNotify, ModelRc, ModelTracker, SharedString, VecModel};

use super::keys::{self, Held};
use super::host::Command;
use super::model::{Ask, Line, Model, Then};
use super::thread::{self, Job};
use super::life::{self, Life, Lives, Slot, Slotted};
use super::standing::{self, Keyboard, Layer, Standing};
use super::{icons, reach, ui};
use crate::platform::place::Which;

/// For measuring: with `HEYO_SETTINGS_FRAMES` naming a file, how long each
/// frame took is written there when the window is closed, a line a frame:
/// when it began (the system's clock, in ms) and how long it took (ms), from
/// winit asking for the window to be drawn to winit's loop having nothing
/// more to do. Slint draws, and hands the frame to the display, in between.
pub(super) mod frames {
    use std::cell::{Cell, RefCell};
    use std::time::{Instant, SystemTime, UNIX_EPOCH};

    use slint::winit_030::winit::event_loop::ActiveEventLoop;
    use slint::winit_030::{CustomApplicationHandler, EventResult};

    thread_local! {
        static BEGUN: Cell<Option<(Instant, SystemTime)>> = const { Cell::new(None) };
        static TAKEN: RefCell<Vec<(u128, f64)>> = const { RefCell::new(Vec::new()) };
    }

    pub(in crate::slint_windows) fn wanted() -> Option<String> {
        std::env::var("HEYO_SETTINGS_FRAMES").ok().filter(|file| !file.is_empty())
    }

    /// winit asked for a window to be drawn.
    pub(in crate::slint_windows) fn begun() {
        BEGUN.set(Some((Instant::now(), SystemTime::now())));
    }

    pub(in crate::slint_windows) struct Timing;

    impl CustomApplicationHandler for Timing {
        fn about_to_wait(&mut self, _: &ActiveEventLoop) -> EventResult {
            if let Some((begun, at)) = BEGUN.take() {
                let at = at.duration_since(UNIX_EPOCH).map_or(0, |since| since.as_millis());
                TAKEN.with_borrow_mut(|taken| taken.push((at, begun.elapsed().as_secs_f64() * 1e3)));
            }
            EventResult::Propagate
        }
    }

    /// Writes down the frames so far, and starts again.
    pub(in crate::slint_windows) fn write() {
        let Some(file) = wanted() else { return };
        let lines: String = TAKEN.with_borrow_mut(|taken| taken.drain(..).map(|(at, took)| format!("{at} {took:.3}\n")).collect());
        if let Err(e) = std::fs::write(&file, lines) {
            eprintln!("heyo: {file}: {e}");
        }
    }
}

/// What is shown for an icon the model names: a file's picture, or a
/// symbol (which is drawn in the colour of text). Each is read once.
#[derive(Default)]
struct Pictures(RefCell<HashMap<String, Option<(slint::Image, bool)>>>);

impl Pictures {
    fn of(&self, icon: &str) -> Option<(slint::Image, bool)> {
        self.0
            .borrow_mut()
            .entry(icon.to_string())
            .or_insert_with(|| {
                let path = std::path::Path::new(icon);
                if path.is_absolute() {
                    return slint::Image::load_from_path(path).ok().map(|picture| (picture, false));
                }
                slint::Image::load_from_svg_data(icons::svg(icon)?.as_bytes()).ok().map(|symbol| (symbol, true))
            })
            .clone()
    }
}

fn strings(items: &[String]) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(items.iter().map(SharedString::from).collect::<Vec<_>>()))
}

/// The lines of the page, as Slint asks for them: a line is made, and its
/// icon read, when it comes into sight.
struct Lines {
    lines: RefCell<Vec<Line>>,
    pictures: Rc<Pictures>,
    notify: ModelNotify,
}

impl Lines {
    /// Shows these lines, telling Slint only what is different: the lines
    /// that begin and end both the old page and the new stay (changed, where
    /// they changed), and what is between them is taken out and put in. So
    /// a control that is being used stays the control it was (a switch
    /// moves, a field keeps what is typed into it), a list that is opened
    /// further down stays where it was scrolled to, and another page is
    /// not left scrolled to where the last one was: Slint's list keeps its
    /// place by the number of its first row, and forgets it only when that
    /// row is taken out.
    fn show(&self, lines: Vec<Line>) {
        let mut shown = self.lines.borrow_mut();
        let alike = |was: &Line, is: &Line| was.kind == is.kind && was.id == is.id && was.label == is.label;
        let head = shown.iter().zip(&lines).take_while(|(was, is)| alike(was, is)).count();
        let most = shown.len().min(lines.len()) - head;
        let tail = shown.iter().rev().zip(lines.iter().rev()).take(most).take_while(|(was, is)| alike(was, is)).count();
        let (out, come) = (shown.len() - head - tail, lines.len() - head - tail);
        let changed: Vec<usize> = (0..head).filter(|at| shown[*at] != lines[*at]).chain((0..tail).filter(|back| shown[shown.len() - 1 - back] != lines[lines.len() - 1 - back]).map(|back| lines.len() - 1 - back)).collect();
        // Slint asks for rows while it is told of them: first what is no
        // longer there goes, then the lines are the new ones.
        if out > 0 {
            shown.drain(head..head + out);
            drop(shown);
            self.notify.row_removed(head, out);
            shown = self.lines.borrow_mut();
        }
        *shown = lines;
        drop(shown);
        if come > 0 {
            self.notify.row_added(head, come);
        }
        for at in changed {
            self.notify.row_changed(at);
        }
    }
}

impl slint::Model for Lines {
    type Data = ui::Line;

    fn row_count(&self) -> usize {
        self.lines.borrow().len()
    }

    fn row_data(&self, row: usize) -> Option<ui::Line> {
        let lines = self.lines.borrow();
        let line = lines.get(row)?;
        // An extension that brings no icon has the one all extensions share.
        let icon = line.icon.as_deref().and_then(|icon| self.pictures.of(icon)).or_else(|| (line.kind == "head").then(|| self.pictures.of("blocks-16")).flatten());
        Some(ui::Line {
            kind: line.kind.into(),
            id: line.id.as_str().into(),
            first: line.first,
            last: line.last,
            space: line.space,
            indent: line.indent,
            label: line.label.as_str().into(),
            description: line.description.as_str().into(),
            danger: line.danger,
            has_icon: icon.is_some(),
            tinted: icon.as_ref().is_some_and(|(_, tinted)| *tinted),
            icon: icon.map(|(picture, _)| picture).unwrap_or_default(),
            control: line.control.into(),
            on: line.on,
            mixed: line.mixed,
            value: line.value.as_str().into(),
            placeholder: line.placeholder.as_str().into(),
            choices: ModelRc::new(VecModel::from(line.choices.iter().map(|(value, title)| ui::Choice { value: value.into(), title: title.into() }).collect::<Vec<_>>())),
            hotkey: line.hotkey.as_str().into(),
            keys: strings(&line.keys),
            more: line.more.as_str().into(),
            alias: line.alias.as_str().into(),
            checkable: line.checkable,
            open: line.open,
            busy: line.busy,
        })
    }

    fn model_tracker(&self) -> &dyn ModelTracker {
        &self.notify
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// An ordinary window, with a shadow: what is behind it shows round its
/// panel.
const STANDING: Standing = Standing { layer: Layer::Among, keyboard: Keyboard::Ordinary, listed: true, clear: true };

/// The panel the window draws, in logical pixels, as the markup has it
/// (`panel-width`, `panel-height`) where the screen has room for it.
const PANEL: (f64, f64) = (860.0, 640.0);

/// The window while Slint runs.
struct Live {
    life: Life<ui::SettingsWindow>,
    /// When it was last asked for, for saying how long it took to be up.
    asked: Rc<Cell<Option<Instant>>>,
    asking: mpsc::Sender<Job>,
    model: Model,
    lines: Rc<Lines>,
    pictures: Rc<Pictures>,
    /// The hotkey field that is recording, and the modifiers held.
    recording: Option<String>,
    held: Held,
    /// Where the pointer is in the window, in its device pixels.
    pointer: (f64, f64),
    /// Asks again, while something is under way that the page counts down.
    /// Kept, so that it goes on asking.
    _poll: slint::Timer,
}

thread_local! {
    static LIVE: Slot<Live> = const { Slot::new() };
    /// The host's events are listened to, once for the run.
    static LISTENING: Cell<bool> = const { Cell::new(false) };
}

/// Does `work` with the window if there is one, on Slint's thread: only
/// asking for it makes it (`reveal`).
fn with<R>(work: impl FnOnce(&mut Live) -> R) -> Option<R> {
    Slot::there(&LIVE, work)
}

impl Live {
    /// Asks the host, one thing after another on the thread kept for that,
    /// and hands each answer to the model back here; what the model then
    /// wants asked is asked too.
    fn ask(&self, asks: Vec<Ask>) {
        for Ask { command, then } in asks {
            let _ = self.asking.send(Box::new(move |host| {
                let answer = host.call(command);
                let _ = slint::invoke_from_event_loop(move || {
                    with(|live| live.answered(then, answer));
                });
            }));
        }
    }

    fn answered(&mut self, then: Then, answer: Result<Value, String>) {
        let copied = then == Then::Copied && answer.is_ok();
        let more = self.model.answered(then, answer);
        self.ask(more);
        self.push();
        if copied {
            // "Copied" is said for a moment.
            slint::Timer::single_shot(Duration::from_millis(2500), || {
                with(|live| {
                    live.model.copied = false;
                    live.push();
                });
            });
        }
    }

    /// Shows what the model says.
    fn push(&mut self) {
        let host = self.life.window().global::<ui::Host>();
        let (entries, pages) = self.model.nav();
        let selected = entries.iter().position(|entry| entry.id == self.model.page).map_or(-1, |at| at as i32);
        let nav: Vec<ui::NavItem> = entries
            .iter()
            .map(|entry| {
                let icon = entry.icon.as_deref().and_then(|icon| self.pictures.of(icon)).or_else(|| self.pictures.of("blocks-16"));
                ui::NavItem { id: entry.id.as_str().into(), title: entry.title.as_str().into(), extension: entry.extension, tinted: icon.as_ref().is_some_and(|(_, tinted)| *tinted), icon: icon.map(|(picture, _)| picture).unwrap_or_default() }
            })
            .collect();
        host.set_nav(ModelRc::new(VecModel::from(nav)));
        host.set_nav_pages(pages as i32);
        host.set_nav_selected(selected);
        host.set_page(self.model.page.as_str().into());
        host.set_title(self.model.title().into());
        host.set_error(self.model.error.as_str().into());
        host.set_hotkeys(self.model.hotkeys_supported());
        host.set_find_placeholder(self.model.find_placeholder().into());
        host.set_find_text(self.model.find.as_str().into());
        let (title, ticked, picks) = self.model.filter().unwrap_or_default();
        host.set_filter_title(title.into());
        host.set_filter_count(ticked as i32);
        host.set_filter_picks(ModelRc::new(VecModel::from(picks.into_iter().map(|(id, title, heading, checked)| ui::Pick { id: id.into(), title: title.into(), heading, checked }).collect::<Vec<_>>())));
        self.lines.show(self.model.lines());
        self.life.window().set_appearance(self.model.settings.get("appearance").and_then(Value::as_str).unwrap_or("system").into());
    }

    fn push_recording(&mut self) {
        let host = self.life.window().global::<ui::Host>();
        host.set_recording(self.recording.as_deref().unwrap_or_default().into());
        host.set_held(strings(&self.held.caps()));
    }

    fn stop_recording(&mut self) {
        if self.recording.take().is_some() {
            self.push_recording();
        }
    }

    /// What winit tells the window, before Slint has it. The keyboard above
    /// all: while a hotkey field records, a key press is the hotkey, by
    /// where the key is.
    fn told(&mut self, event: &WindowEvent) -> EventResult {
        match event {
            WindowEvent::RedrawRequested => {
                frames::begun();
                EventResult::Propagate
            }
            WindowEvent::Moved(_) => {
                self.life.window().set_moved(self.life.window().get_moved().wrapping_add(1));
                EventResult::Propagate
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.pointer = (position.x, position.y);
                EventResult::Propagate
            }
            // Its panel is elsewhere in it, or of another size: once Slint
            // has laid it out for what it was just told.
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                slint::Timer::single_shot(Duration::ZERO, || {
                    with(|live| live.reach());
                });
                EventResult::Propagate
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                let state = modifiers.state();
                self.held = Held { control: state.control_key(), shift: state.shift_key(), alt: state.alt_key(), superkey: state.super_key() };
                if self.recording.is_some() {
                    self.push_recording();
                }
                EventResult::Propagate
            }
            WindowEvent::KeyboardInput { event, .. } if self.recording.is_some() => {
                if let (ElementState::Pressed, false, PhysicalKey::Code(code)) = (event.state, event.repeat, event.physical_key) {
                    if code == KeyCode::Escape {
                        self.stop_recording();
                    } else if let Some(accelerator) = keys::accelerator(self.held, &format!("{code:?}")) {
                        let id = self.recording.take().unwrap_or_default();
                        self.push_recording();
                        let asks = self.model.bind(&id, &accelerator);
                        self.ask(asks);
                    }
                }
                self.life.draw();
                EventResult::PreventDefault
            }
            // What a key, a click or the wheel does is seen at once.
            WindowEvent::KeyboardInput { .. } | WindowEvent::MouseInput { .. } | WindowEvent::MouseWheel { .. } | WindowEvent::Ime(_) => {
                self.life.draw();
                EventResult::Propagate
            }
            WindowEvent::Focused(false) => {
                self.held = Held::default();
                self.stop_recording();
                EventResult::Propagate
            }
            _ => EventResult::Propagate,
        }
    }

    fn hide(&mut self) {
        self.stop_recording();
        self.life.off();
        frames::write();
    }

    /// Has the window take the pointer where its panel is, and not in the
    /// margin its shadow falls in (`reach`).
    fn reach(&self) {
        let window = self.life.window();
        let (margin, scale, size) = (window.get_margin(), window.window().scale_factor(), window.window().size());
        self.life.reach(reach::panel((size.width, size.height), margin, scale));
    }

    /// Puts the window on screen, in front, and reads again what may have
    /// changed while it was away. `then` hears whether winit made the
    /// window, which it does in its own time.
    fn reveal(&mut self, asked: Instant, then: impl FnOnce(Result<(), String>) + 'static) {
        self.asked.set(Some(asked));
        self.life.on(then);
        self.ask(self.model.opened());
        // Where Slint cannot tell whether the desktop is dark (it asks the
        // desktop's portal), the host's own answer stands in, as of now.
        let _ = self.asking.send(Box::new(|host| {
            let dark = host.system_dark();
            let _ = slint::invoke_from_event_loop(move || {
                with(|live| live.life.window().set_system_dark(dark));
            });
        }));
    }
}

/// What the window does is said to `Live`.
fn wire(window: &ui::SettingsWindow) {
    let host = window.global::<ui::Host>();
    host.on_show(|id| {
        with(|live| {
            live.stop_recording();
            let asks = live.model.show(&id);
            live.ask(asks);
            live.push();
        });
    });
    host.on_search(|text| {
        with(|live| {
            live.model.nav_search = text.into();
            live.push();
        });
    });
    host.on_act(|id, value| {
        with(|live| {
            let asks = live.model.acted(&id, &value);
            live.ask(asks);
            live.push();
        });
    });
    host.on_find(|text| {
        with(|live| {
            live.model.find = text.into();
            live.push();
        });
    });
    host.on_pick(|id| {
        with(|live| {
            live.model.pick(&id);
            live.push();
        });
    });
    host.on_record(|id| {
        with(|live| {
            live.recording = Some(id.into());
            live.push_recording();
        });
    });
    host.on_stop_recording(|| {
        with(Live::stop_recording);
    });
    host.on_clear_hotkey(|id| {
        with(|live| {
            let asks = live.model.bind(&id, "");
            live.ask(asks);
        });
    });
    host.on_close(|| {
        with(Live::hide);
    });
    host.on_drag(|| {
        with(|live| {
            live.life.window().window().with_winit_window(|window| {
                let _ = window.drag_window();
            });
        });
        // Once Slint is done with the press that asked for this.
        slint::Timer::single_shot(Duration::ZERO, let_go);
    });
    window.window().on_close_requested(|| {
        with(Live::hide);
        slint::CloseRequestResponse::HideWindow
    });
    window.window().on_winit_window_event(|_, event| with(|live| live.told(event)).unwrap_or(EventResult::Propagate));
}

/// The window is being moved by the desktop, which has the pointer from
/// the press that began the move until the button is let go: the window
/// is never told that it was. Slint would go on taking what was pressed
/// for pressed, and hand it every press after: each would begin another
/// move, and nothing in the window could be clicked again. So Slint is
/// told now what it will not hear: the button is up, and the pointer is
/// away until it is next heard of.
fn let_go() {
    let Some((window, at)) = with(|live| (live.life.window().as_weak(), live.pointer)) else { return };
    // Not while the window is being done something with here: what Slint
    // is told may come back as something a control did.
    let Some(window) = window.upgrade() else { return };
    let scale = f64::from(window.window().scale_factor()).max(0.1);
    let position = slint::LogicalPosition::new((at.0 / scale) as f32, (at.1 / scale) as f32);
    let _ = window.window().dispatch_event_with_result(slint::platform::WindowEvent::PointerReleased { position, button: slint::platform::PointerEventButton::Left });
    let _ = window.window().dispatch_event_with_result(slint::platform::WindowEvent::PointerExited);
}

/// What the window is given when it is made: where its lines come from,
/// and what is done in it said to `Live`.
fn furnish(window: &ui::SettingsWindow, shared: &thread::Shared, lines: &Rc<Lines>) {
    window.global::<ui::Theme>().set_plain(shared.renderer == "software");
    let weak = window.as_weak();
    life::seen_through(window.window(), &shared.renderer, move |opaque| {
        if let Some(window) = weak.upgrade() {
            window.set_opaque(opaque);
        }
    });
    window.global::<ui::Host>().set_lines(ModelRc::from(lines.clone() as Rc<dyn slint::Model<Data = ui::Line>>));
    wire(window);
}

/// Where winit's window goes when it is about to be seen: in the middle
/// of the room the screen has for it, and no larger; taking the pointer
/// where its panel is; and in front.
fn arrived(window: &ui::SettingsWindow, made: &slint::winit_030::winit::window::Window, asking: &mpsc::Sender<Job>, asked: Option<Instant>) {
    if let Some(monitor) = life::display(made, true).monitor(Which::Its) {
        let (wide, high) = monitor.room_size();
        window.set_room_width(wide as f32);
        window.set_room_height(high as f32);
        // As large as it will be, which it may not be yet: the panel the
        // markup draws, no larger than the room, and its margin.
        let margin = 2.0 * f64::from(window.get_margin());
        let (x, y) = monitor.middle_of_room(((PANEL.0.min(wide) + margin) * monitor.scale, (PANEL.1.min(high) + margin) * monitor.scale));
        made.set_outer_position(PhysicalPosition::new(x, y));
    }
    standing::reach(made, reach::panel((made.inner_size().width, made.inner_size().height), window.get_margin(), window.window().scale_factor()));
    made.focus_window();
    // Which on Wayland a window may not do for itself: the desktop is
    // asked to.
    let title = made.title();
    let _ = asking.send(Box::new(move |host| host.forward(title)));
    if let Some(asked) = asked {
        eprintln!("heyo: settings up {} ms after they were asked for", asked.elapsed().as_millis());
    }
}

impl Lives for Live {
    const NAME: &'static str = "the settings window";

    /// Makes the window, the first time it is asked for.
    fn make() -> Result<Self, String> {
        make()
    }

    /// It shows what it showed, and is back on screen if it was there.
    /// What is not the model's goes with the old window: how far a page
    /// was scrolled, and what was being typed into a field.
    fn carry(self, new: &mut Self) {
        new.life.window().set_system_dark(self.life.window().get_system_dark());
        let shown = self.life.wanted();
        new.model = self.model;
        new.push();
        if shown {
            new.reveal(Instant::now(), |_| {});
        }
    }
}

impl Slotted for Live {
    fn slot() -> &'static std::thread::LocalKey<Slot<Self>> {
        &LIVE
    }
}

fn make() -> Result<Live, String> {
    let shared = thread::shared().ok_or("Slint has not begun")?;
    let asking = thread::asker("heyo-slint-host", shared.host.clone())?;
    let pictures = Rc::new(Pictures::default());
    let lines = Rc::new(Lines { lines: RefCell::default(), pictures: pictures.clone(), notify: ModelNotify::default() });
    let asked = Rc::new(Cell::new(None));
    // Left to Slint to show and hide: it is an ordinary window.
    let life = Life::new(STANDING, ui::SettingsWindow::new, {
        let (asking, asked) = (asking.clone(), asked.clone());
        move |window, made, _| arrived(window, made, &asking, asked.take())
    })?;
    let window = life.window();
    furnish(window, &shared, &lines);

    // What the first page shows is read before the window is: it comes
    // up as it is, not empty and then filled.
    let (first, read) = mpsc::channel();
    let _ = asking.send(Box::new(move |host| {
        let _ = first.send((host.call(Command::GetSettings), host.call(Command::GetHotkeys), host.call(Command::GetCapabilities), host.system_dark()));
    }));
    let (settings, hotkeys, capabilities, dark) = read.recv().map_err(|_| "the host did not answer".to_string())?;
    let mut model = Model::new();
    model.answered(Then::Settings, Ok(settings?));
    model.answered(Then::Hotkeys, Ok(hotkeys?));
    model.answered(Then::Capabilities, Ok(capabilities?));
    window.set_system_dark(dark);

    let poll = slint::Timer::default();
    poll.start(slint::TimerMode::Repeated, Duration::from_secs(2), || {
        with(|live| live.ask(live.model.poll()));
    });
    let mut live = Live { life, asked, asking, model, lines, pictures, recording: None, held: Held::default(), pointer: (0.0, 0.0), _poll: poll };
    live.push();

    // Whichever window there is hears them: it may be made again.
    if !LISTENING.replace(true) {
        shared.host.listen(Box::new(|event, told| {
            let event = event.to_string();
            life::ask(move || {
                with(|live| {
                    let asks = live.model.heard(&event, &told);
                    live.ask(asks);
                    live.push();
                });
            });
        }));
    }
    Ok(live)
}

/// Shows the window, made first if this is the first time, and brings it
/// forward. On Slint's thread; `then` hears whether it came up.
pub(super) fn reveal(asked: Instant, then: impl FnOnce(Result<(), String>) + 'static) {
    let mut then = Some(then);
    Slot::with(&LIVE, |live| {
        if let Some(then) = then.take() {
            live.reveal(asked, then);
        }
    });
    if let Some(then) = then {
        let unmade = Slot::why_not(&LIVE);
        UNMADE_SAID.store(unmade.is_some(), std::sync::atomic::Ordering::SeqCst);
        then(Err(unmade.unwrap_or_else(|| "the settings window was busy".into())));
    }
}

/// Whether the reason the window last gave for not coming up was that it
/// could not be made, which its slot has said in the log already.
pub(super) fn unmade_said() -> bool {
    UNMADE_SAID.load(std::sync::atomic::Ordering::SeqCst)
}

static UNMADE_SAID: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg(test)]
mod tests {
    use super::*;

    /// winit names a key's place as the web does, which is what lets the
    /// recorder's table be the one the palette's recorder has.
    #[test]
    fn winit_names_keys_as_the_web_does() {
        let named = |code: KeyCode| format!("{code:?}");
        assert_eq!(named(KeyCode::KeyA), "KeyA");
        assert_eq!(named(KeyCode::Digit7), "Digit7");
        assert_eq!(named(KeyCode::F12), "F12");
        let theirs = [
            KeyCode::Space,
            KeyCode::Enter,
            KeyCode::Tab,
            KeyCode::Backspace,
            KeyCode::Delete,
            KeyCode::Insert,
            KeyCode::Home,
            KeyCode::End,
            KeyCode::PageUp,
            KeyCode::PageDown,
            KeyCode::ArrowUp,
            KeyCode::ArrowDown,
            KeyCode::ArrowLeft,
            KeyCode::ArrowRight,
            KeyCode::Backquote,
            KeyCode::Minus,
            KeyCode::Equal,
            KeyCode::BracketLeft,
            KeyCode::BracketRight,
            KeyCode::Backslash,
            KeyCode::Semicolon,
            KeyCode::Quote,
            KeyCode::Comma,
            KeyCode::Period,
            KeyCode::Slash,
        ];
        assert_eq!(theirs.map(named).to_vec(), keys::CODE_NAMES.iter().map(|(code, _)| code.to_string()).collect::<Vec<_>>());
    }
}

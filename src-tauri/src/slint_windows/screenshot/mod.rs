//! The screenshot overlay: a frozen picture of a monitor, covering it, on
//! which part is selected. It is one of Slint's windows (`super::thread`).
//!
//! - What it shows and how it answers the pointer and the keys is decided
//!   by `crate::screenshot::selecting`, which knows no toolkit.
//! - `native` is what a desktop is told for the window to cover a monitor
//!   and have every key.
//! - `marks` is what is drawn on the capture, as the markup is given it.
//! - `pin` is a capture kept on the screen in a window of its own.
//! - This file is the window: made ahead, put over the monitor that was
//!   read, told of the pointer and the keys by winit (not by Slint's own
//!   elements: no element can then stand between a key and the overlay),
//!   and what was decided told to the host. Two things in it are Slint's
//!   own, and hear of the pointer and the keys from it: the toolbar's
//!   buttons, and the line text is typed in.
//!
//! It covers the screen and holds the keyboard, so it must not outlive
//! what answers it:
//!
//! - Escape is read first of all, from winit's event, whatever else the
//!   overlay is doing.
//! - If Slint's thread panics, `guard` ends its connection to the display,
//!   and the display drops the window and gives the keyboard back.
//! - If Slint's thread stops answering, `watch` takes the window off the
//!   screen from outside, which gives the keyboard back too.
//! - If Heyo ends, however, the display does the same.

mod marks;
pub(super) mod native;
pub(crate) mod pin;

use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use slint::winit_030::winit::event::{ElementState, MouseButton, WindowEvent};
use slint::winit_030::winit::keyboard::{Key as WinitKey, KeyCode, NamedKey, PhysicalKey};
use slint::winit_030::winit::window::CursorIcon;
use slint::winit_030::{EventResult, WinitWindowAccessor};
use slint::{ComponentHandle, Image, Model as _, ModelRc, Rgba8Pixel, SharedPixelBuffer, VecModel};
use tauri::AppHandle;

use super::guard::{self, State};
use super::life::{self, Kept, Life, Lives, Slot};
use super::thread::{self, Job, Kind};
use super::ui;
use crate::platform::capture::{Area, Shot};
use crate::screenshot::selecting::{self, Action, Button, Cursor, Event, Key, Mode, Outcome, Selecting, Shape, Tool};

/// How bright the capture is around the selection, out of 256.
const DIM: u16 = 154;
/// The size of the window while it waits to be wanted.
const IDLE: u32 = 16;
/// How often the keyboard is asked for again, and how many times.
const HOLD_AGAIN: Duration = Duration::from_millis(50);
const HOLD_TIMES: u32 = 60;
/// How long the window is waited for to have the monitor's size.
const UNSIZED: Duration = Duration::from_millis(150);
/// How often Slint's thread is asked whether it is there while the overlay
/// is up, and how many times it may not answer.
const BEAT: Duration = Duration::from_millis(250);
const MISSED_BEATS: u32 = 6;
/// How long after a screenshot the window is kept as it is, at the
/// monitor's size, for the next one to be as quick as can be.
const RESTING: Duration = Duration::from_secs(60);

/// The overlay is on screen, or about to be.
static UP: AtomicBool = AtomicBool::new(false);
/// What its window is known by outside Slint's thread (`native::id`).
static WINDOW: AtomicU64 = AtomicU64::new(0);
/// Counts answers from Slint's thread, for `watch`.
static BEATS: AtomicU64 = AtomicU64::new(0);

/// A monitor as it was read, ready to be shown: dimmed, which is what is
/// seen first. The same as it was read follows (`Overlay::cleared`).
struct Capture {
    area: Area,
    mode: Mode,
    dimmed: SharedPixelBuffer<Rgba8Pixel>,
    asked: Instant,
}

/// What is on screen.
struct Up {
    selecting: Selecting,
    /// How many device pixels a logical one is, for this window.
    scale: f64,
    /// The size the window is to have before it is put on screen.
    size: (u32, u32),
    mapped: bool,
    asked: Instant,
    /// The monitor as it was read, once that picture is here: the loupe
    /// shows pixels of it.
    clear: Option<SharedPixelBuffer<Rgba8Pixel>>,
}

struct Overlay {
    life: Life<ui::ScreenshotWindow>,
    handles: Rc<VecModel<ui::Spot>>,
    /// The host is told what was decided on a thread kept for that.
    telling: mpsc::Sender<Job>,
    up: Option<Up>,
    holding: slint::Timer,
    held_tries: u32,
    resting: slint::Timer,
    /// The pointer, in device pixels, and whether its button is down.
    pointer: (f64, f64),
    pressed: bool,
    moved_pressed: u32,
    ctrl: bool,
    alt: bool,
    cursor: Option<Cursor>,
    /// What is drawn on the capture, as the markup has it, and of what.
    marks: Rc<VecModel<ui::Mark>>,
    drawn: Vec<Shape>,
    /// The pointer is over the toolbar, which is Slint's; and its button
    /// went down there, or in the line that is being typed.
    on_toolbar: bool,
    pressed_in_slint: bool,
    /// Where text is being typed, as the markup has it.
    typing: Option<selecting::Point>,
}

/// The toolbar's tools and what they are called there.
const TOOLS: [(Tool, &str, &str, &str); 5] =
    [(Tool::Rect, "rect", "square", "Rectangle"), (Tool::Arrow, "arrow", "arrow-up-right", "Arrow"), (Tool::Pen, "pen", "pen", "Pen"), (Tool::Text, "text", "type", "Text"), (Tool::Blur, "blur", "grid-3x3", "Blur")];

/// The buttons that say what becomes of the selection. None is Cancel.
const ACTIONS: [(Option<Action>, &str, &str, &str); 5] = [
    (Some(Action::Text), "text", "scan-text", "Copy text in the selection"),
    (Some(Action::Pin), "pin", "pin", "Pin to screen"),
    (Some(Action::Save), "save", "download", "Save (Ctrl+S)"),
    (Some(Action::Copy), "copy", "copy", "Copy (Enter)"),
    (None, "cancel", "x", "Cancel (Esc)"),
];

/// A symbol by its name, as a picture.
fn symbol(name: &str) -> Image {
    super::icons::svg(name).and_then(|svg| Image::load_from_svg_data(svg.as_bytes()).ok()).unwrap_or_default()
}

thread_local! {
    static OVERLAY: Slot<Overlay> = const { Slot::new() };
}

/// Does `work` with the overlay, on Slint's thread, making it first if
/// this is the first time.
fn with(work: impl FnOnce(&mut Overlay)) {
    Slot::with(&OVERLAY, work);
}

impl Lives for Overlay {
    const NAME: &'static str = "the screenshot overlay";

    fn make() -> Result<Self, String> {
        let shared = thread::shared().ok_or("Slint has not begun")?;
        let telling = thread::asker("heyo-slint-shot", shared.host.clone())?;
        let window = thread::making(Kind::Screenshot, ui::ScreenshotWindow::new).map_err(|e| format!("Slint could not make it: {e}"))?;
        let handles = Rc::new(VecModel::default());
        window.set_handles(ModelRc::from(handles.clone()));
        let marks = Rc::new(VecModel::default());
        window.set_marks(ModelRc::from(marks.clone()));
        let item = |id: &str, icon: &str, tip: &str, primary: bool| ui::ToolItem { id: id.into(), icon: symbol(icon), tip: tip.into(), primary };
        window.set_tools(ModelRc::new(VecModel::from(TOOLS.iter().map(|(_, id, icon, tip)| item(id, icon, tip, false)).collect::<Vec<_>>())));
        window.set_actions(ModelRc::new(VecModel::from(ACTIONS.iter().map(|(action, id, icon, tip)| item(id, icon, tip, *action == Some(Action::Copy))).collect::<Vec<_>>())));
        window.set_undo_icon(symbol("undo-2"));
        window.set_colours(ModelRc::new(VecModel::from((0..selecting::COLOURS.len()).map(marks::colour).collect::<Vec<_>>())));
        // The toolbar's buttons: each says what was pressed.
        window.on_pick_tool(|id| {
            if let Some((tool, ..)) = TOOLS.iter().find(|(_, name, ..)| *name == id.as_str()) {
                with(|overlay| overlay.act(Event::Tool(*tool)));
            }
        });
        window.on_pick_colour(|index| with(|overlay| overlay.act(Event::Colour(index.max(0) as usize))));
        window.on_undo(|| with(|overlay| overlay.act(Event::Undo)));
        window.on_act(|id| {
            if let Some((action, ..)) = ACTIONS.iter().find(|(_, name, ..)| *name == id.as_str()) {
                with(|overlay| overlay.act(action.map_or(Event::Cancel, Event::Do)));
            }
        });
        // Off the screen and small until it is wanted.
        window.window().set_position(slint::PhysicalPosition::new(-(IDLE as i32) * 4, -(IDLE as i32) * 4));
        window.window().set_size(slint::PhysicalSize::new(IDLE, IDLE));
        window.window().on_winit_window_event(|_, event| Slot::there(&OVERLAY, |overlay| overlay.told(event)).unwrap_or(EventResult::Propagate));
        // Kept, where winit can take a window off the screen: Slint would
        // make it anew without what `native` says of it.
        let kept = native::kept().then_some(Kept { on: native::show, off: native::hide });
        let life = Life::new(window, kept, |_, made, first| {
            if first {
                WINDOW.store(native::id(made), Ordering::SeqCst);
            }
        });
        Ok(Overlay {
            life,
            handles,
            telling,
            up: None,
            holding: slint::Timer::default(),
            held_tries: 0,
            resting: slint::Timer::default(),
            pointer: (0.0, 0.0),
            pressed: false,
            moved_pressed: 0,
            ctrl: false,
            alt: false,
            cursor: None,
            marks,
            drawn: Vec::new(),
            on_toolbar: false,
            pressed_in_slint: false,
            typing: None,
        })
    }
}

/// Gives up the window, which had grown to a monitor's size, and makes
/// the small one that waits for the next screenshot. On Slint's thread,
/// and not from anything of the overlay's own.
fn rest() {
    if Slot::give_up(&OVERLAY, |overlay| overlay.up.is_none() && overlay.life.made()).is_some() {
        prepare();
    }
}

/// The display changed (`super::follow`): the overlay's window is made
/// again, for the display as it is, and the pinned captures with it. A
/// capture that is up is of the screen as it was, and is given up as
/// Escape gives it up. On Slint's thread.
pub(super) fn remake() {
    Slot::there(&OVERLAY, |overlay| {
        if overlay.up.is_some() {
            overlay.after(Some(Outcome::Cancel));
        }
    });
    rest();
    pin::remake();
}

/// For measuring how soon the overlay follows the pointer: with
/// `HEYO_OVERLAY_TRACE` set, says what happened and when by the system's
/// clock.
fn trace(what: &str) {
    static WANTED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if *WANTED.get_or_init(|| std::env::var_os("HEYO_OVERLAY_TRACE").is_some()) {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.as_micros());
        eprintln!("heyo: {what} at {}.{:03} ms", now / 1000, now % 1000);
    }
}

/// A key as the overlay's rules know keys.
fn key_of(logical: &WinitKey, physical: PhysicalKey) -> Option<Key> {
    match logical {
        WinitKey::Named(NamedKey::Escape) => Some(Key::Escape),
        WinitKey::Named(NamedKey::Enter) => Some(Key::Enter),
        WinitKey::Named(NamedKey::F4) => Some(Key::F4),
        WinitKey::Character(text) => {
            let made = text.chars().next()?.to_ascii_lowercase();
            // On a layout whose letters are not these, a shortcut is the
            // key where the letter is on the layout they are named after.
            let placed = match physical {
                PhysicalKey::Code(KeyCode::KeyC) => Some('c'),
                PhysicalKey::Code(KeyCode::KeyS) => Some('s'),
                PhysicalKey::Code(KeyCode::KeyZ) => Some('z'),
                _ => None,
            };
            Some(Key::Character(if made.is_ascii() { made } else { placed.unwrap_or(made) }))
        }
        _ => None,
    }
}

fn icon(cursor: Cursor) -> CursorIcon {
    match cursor {
        Cursor::Crosshair => CursorIcon::Crosshair,
        Cursor::Move => CursorIcon::Move,
        Cursor::Resize(0, _) => CursorIcon::NsResize,
        Cursor::Resize(_, 0) => CursorIcon::EwResize,
        Cursor::Resize(across, down) if across == down => CursorIcon::NwseResize,
        Cursor::Resize(..) => CursorIcon::NeswResize,
        Cursor::Grab => CursorIcon::Grab,
        Cursor::Arrow => CursorIcon::Default,
    }
}

impl Overlay {
    /// Shows a monitor as it was read, over that monitor.
    fn show(&mut self, capture: Capture) {
        if !self.life.made() {
            return self.life.ahead(move || with(|overlay| overlay.show(capture)));
        }
        let Capture { area, mode, dimmed, asked } = capture;
        self.resting.stop();
        let scale = f64::from(self.life.window().window().scale_factor()).max(0.1);
        self.life.window().set_dimmed(Image::from_rgba8_premultiplied(dimmed));
        let selecting = Selecting::new((area.width as f64 / scale, area.height as f64 / scale), (area.width, area.height), mode);
        self.up = Some(Up { selecting, scale, size: (area.width, area.height), mapped: false, asked, clear: None });
        (self.pressed, self.moved_pressed, self.ctrl, self.alt, self.cursor) = (false, 0, false, false, None);
        (self.on_toolbar, self.pressed_in_slint) = (false, false);
        self.push();
        let window = self.life.window().window();
        window.set_position(slint::PhysicalPosition::new(area.x, area.y));
        window.set_size(slint::PhysicalSize::new(area.width, area.height));
        // It is put on screen when it has the monitor's size: at once if
        // it has it already, else when winit says it has (`told`).
        let sized = window.with_winit_window(|made| made.inner_size()).is_some_and(|size| (size.width, size.height) == (area.width, area.height));
        if !native::kept() {
            // Where a window cannot place itself, it fills a monitor.
            window.set_fullscreen(true);
            self.map();
        } else if sized {
            self.map();
        } else {
            // And if winit never says so, it is shown all the same.
            slint::Timer::single_shot(UNSIZED, || with(Overlay::map));
        }
    }

    /// The monitor as it was read is here too. It is drawn at once, though
    /// nothing may be selected yet, so that the renderer's copy of it is
    /// made now and not when a drag begins.
    fn cleared(&mut self, clear: SharedPixelBuffer<Rgba8Pixel>) {
        let Some(up) = self.up.as_mut() else { return };
        self.life.window().set_clear(Image::from_rgba8_premultiplied(clear.clone()));
        crate::screenshot::mark("the capture as it was read is in the overlay", up.asked);
        up.clear = Some(clear);
        self.push();
    }

    /// Puts the window on screen and asks for the keyboard.
    fn map(&mut self) {
        let Some(up) = self.up.as_mut().filter(|up| !up.mapped) else { return };
        up.mapped = true;
        let asked = up.asked;
        self.life.on(|came| {
            if let Err(e) = came {
                eprintln!("heyo: the screenshot overlay could not be shown: {e}");
            }
        });
        self.life.window().window().with_winit_window(|made| made.set_cursor(CursorIcon::Crosshair));
        crate::screenshot::mark("overlay put on screen", asked);
        self.held_tries = 0;
        self.hold();
    }

    fn hold(&mut self) {
        let held = self.life.window().window().with_winit_window(native::hold).unwrap_or(true);
        self.held_tries += 1;
        if held || self.held_tries >= HOLD_TIMES {
            if !held {
                eprintln!("heyo: the screenshot overlay was not given the keyboard");
            }
            return;
        }
        self.holding.start(slint::TimerMode::SingleShot, HOLD_AGAIN, || {
            with(|overlay| {
                if overlay.up.as_ref().is_some_and(|up| up.mapped) {
                    overlay.hold();
                }
            })
        });
    }

    /// Takes the overlay off the screen and lets go of the capture.
    fn close(&mut self) {
        UP.store(false, Ordering::SeqCst);
        self.holding.stop();
        if self.up.take().is_none() {
            return;
        }
        self.life.off();
        // The window keeps a buffer of its own size, a whole screen: when
        // no screenshot has followed for a while, it is given up with the
        // window, and a small one made in its place (`rest`).
        self.resting.start(slint::TimerMode::SingleShot, RESTING, || slint::Timer::single_shot(Duration::ZERO, rest));
        super::window::frames::write();
        // Tens of megabytes each.
        self.life.window().set_clear(Image::default());
        self.life.window().set_dimmed(Image::default());
        self.life.window().set_loupe_picture(Image::default());
        self.life.window().set_selected(false);
        self.life.window().set_loupe(false);
        self.life.window().set_hint("".into());
        self.life.window().set_status("".into());
        self.life.window().set_label("".into());
        self.handles.set_vec(Vec::new());
        self.marks.set_vec(Vec::new());
        self.drawn.clear();
        self.typing = None;
        self.life.window().set_typing(false);
        self.life.window().set_toolbar(false);
    }

    /// A button of the toolbar was pressed. What was being typed is done
    /// with first: the pointer went elsewhere.
    fn act(&mut self, event: Event) {
        self.typed();
        let Some(up) = self.up.as_mut() else { return };
        let outcome = up.selecting.handle(event);
        self.after(outcome);
    }

    /// The text that is being typed is done with: it stays where it was
    /// typed, as it is.
    fn typed(&mut self) {
        let Some(up) = self.up.as_mut().filter(|up| up.selecting.typing().is_some()) else { return };
        let (text, width) = (self.life.window().get_typed().to_string(), f64::from(self.life.window().get_typed_width()));
        up.selecting.handle(Event::Typed { text, width });
    }

    /// Shows what the overlay has become, or does what was decided.
    fn after(&mut self, outcome: Option<Outcome>) {
        let Some(outcome) = outcome else { return self.push() };
        let drawn = match outcome {
            Outcome::Finish { area, .. } => self.pictured(area),
            Outcome::Cancel => None,
        };
        // Handed over, the overlay is done with; except where what becomes
        // of it may fail and is said in the overlay.
        if matches!(outcome, Outcome::Finish { action: Action::Text, .. }) {
            self.push();
        } else {
            self.close();
        }
        let _ = self.telling.send(Box::new(move |host| host.screenshot(outcome, drawn)));
    }

    /// The selected part with what is drawn on it, as a picture taken of
    /// the overlay itself with nothing else in it: what was seen is what
    /// is had. None when nothing is drawn on it: it is then cut from the
    /// capture as it was read.
    fn pictured(&mut self, area: selecting::Area) -> Option<image::RgbaImage> {
        if self.up.as_ref()?.selecting.shapes().is_empty() {
            return None;
        }
        self.push();
        self.life.window().set_exporting(true);
        let taken = self.life.window().window().take_snapshot();
        self.life.window().set_exporting(false);
        let taken = taken.map_err(|e| eprintln!("heyo: the annotations could not be drawn into the screenshot: {e}")).ok()?;
        let (width, height) = (taken.width(), taken.height());
        let (x, y) = (area.x.min(width), area.y.min(height));
        let (across, down) = (area.width.min(width - x), area.height.min(height - y));
        let mut part = Vec::with_capacity((across * down * 4) as usize);
        for row in y..y + down {
            let from = ((row * width + x) * 4) as usize;
            part.extend_from_slice(&taken.as_bytes()[from..from + across as usize * 4]);
        }
        image::RgbaImage::from_raw(across, down, part)
    }

    /// What winit told the window. Escape is the overlay's whatever else
    /// has the keyboard. The rest goes on to Slint only where something of
    /// Slint's is to answer it: over the toolbar, and for the line that is
    /// being typed.
    fn told(&mut self, event: &WindowEvent) -> EventResult {
        let Some(up) = self.up.as_mut() else { return EventResult::Propagate };
        let scale = up.scale;
        let at = |pointer: (f64, f64)| (pointer.0 / scale, pointer.1 / scale);
        let typing = up.selecting.typing();
        let event = match event {
            WindowEvent::RedrawRequested => {
                super::window::frames::begun();
                return EventResult::Propagate;
            }
            WindowEvent::Resized(size) => {
                if !up.mapped && (size.width, size.height) == up.size {
                    // Once Slint has heard of it too.
                    slint::Timer::single_shot(Duration::ZERO, || with(Overlay::map));
                }
                return EventResult::Propagate;
            }
            WindowEvent::KeyboardInput { event, is_synthetic, .. } => {
                let down = event.state == ElementState::Pressed;
                match event.physical_key {
                    PhysicalKey::Code(KeyCode::ControlLeft | KeyCode::ControlRight) => self.ctrl = down,
                    PhysicalKey::Code(KeyCode::AltLeft | KeyCode::AltRight) => self.alt = down,
                    _ => {}
                }
                match key_of(&event.logical_key, event.physical_key).filter(|_| down && !is_synthetic) {
                    // It gives up the text that is being typed, or the screenshot.
                    Some(Key::Escape) => Event::Key { key: Key::Escape, ctrl: self.ctrl, alt: self.alt },
                    Some(Key::Enter) if typing.is_some() => {
                        self.typed();
                        self.push();
                        return EventResult::PreventDefault;
                    }
                    // Every other key is the text's while text is typed.
                    _ if typing.is_some() => return EventResult::Propagate,
                    Some(key) => Event::Key { key, ctrl: self.ctrl, alt: self.alt },
                    None => return EventResult::PreventDefault,
                }
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                (self.ctrl, self.alt) = (modifiers.state().control_key(), modifiers.state().alt_key());
                return if typing.is_some() { EventResult::Propagate } else { EventResult::PreventDefault };
            }
            WindowEvent::CursorMoved { position, .. } => {
                trace("pointer moved");
                self.pointer = (position.x, position.y);
                if self.pressed {
                    self.moved_pressed += 1;
                    guard::test_in_drag(self.moved_pressed);
                }
                if self.pressed_in_slint {
                    return EventResult::Propagate;
                }
                let to = at(self.pointer);
                up.selecting.handle(Event::Moved { to });
                // Slint hears of the pointer while it is over the toolbar,
                // and that it has left when it has.
                let on_toolbar = up.selecting.toolbar().is_some_and(|toolbar| toolbar.contains(to));
                if self.on_toolbar && !on_toolbar {
                    let _ = self.life.window().window().dispatch_event_with_result(slint::platform::WindowEvent::PointerExited);
                    // And had the pointer's shape meanwhile.
                    self.cursor = None;
                }
                self.on_toolbar = on_toolbar;
                self.push();
                return if on_toolbar { EventResult::Propagate } else { EventResult::PreventDefault };
            }
            WindowEvent::CursorLeft { .. } => {
                let was_on_toolbar = std::mem::take(&mut self.on_toolbar);
                up.selecting.handle(Event::Left);
                self.push();
                return if was_on_toolbar { EventResult::Propagate } else { EventResult::PreventDefault };
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let down = *state == ElementState::Pressed;
                let place = at(self.pointer);
                match button {
                    // Gives the screenshot up, wherever it is pressed.
                    MouseButton::Right if down => Event::Pressed { at: place, button: Button::Secondary },
                    MouseButton::Left if !down && self.pressed_in_slint => {
                        self.pressed_in_slint = false;
                        return EventResult::Propagate;
                    }
                    MouseButton::Left if !down => {
                        (self.pressed, self.moved_pressed) = (false, 0);
                        Event::Released { button: Button::Primary }
                    }
                    MouseButton::Left => {
                        // A press on the toolbar is the toolbar's, and one in
                        // the line that is being typed is the line's.
                        let line = typing.map(|line| selecting::Rect { x: line.0 - 4.0, y: line.1 - 3.0, width: (f64::from(self.life.window().get_typed_width()) + 2.0).max(120.0) + 8.0, height: selecting::TEXT_LINE + 6.0 });
                        if up.selecting.toolbar().is_some_and(|toolbar| toolbar.contains(place)) || line.is_some_and(|line| line.contains(place)) {
                            self.pressed_in_slint = true;
                            return EventResult::Propagate;
                        }
                        // Anywhere else, what was being typed is done with.
                        self.typed();
                        (self.pressed, self.moved_pressed) = (true, 0);
                        Event::Pressed { at: place, button: Button::Primary }
                    }
                    _ => return EventResult::PreventDefault,
                }
            }
            WindowEvent::MouseWheel { .. } | WindowEvent::Touch(_) | WindowEvent::CursorEntered { .. } => return EventResult::PreventDefault,
            _ => return EventResult::Propagate,
        };
        let outcome = self.up.as_mut().and_then(|up| up.selecting.handle(event));
        self.after(outcome);
        EventResult::PreventDefault
    }

    /// Shows what the selection says is to be seen.
    fn push(&mut self) {
        let Some(up) = self.up.as_ref() else { return };
        let (look, scale) = (up.selecting.look(), up.scale);
        let spot = |point: selecting::Point| ui::Spot { x: point.0 as f32, y: point.1 as f32 };
        let window = self.life.window();
        window.set_selected(look.clear.is_some());
        // With nothing selected, one pixel in the middle, where the hint is
        // over it: see the markup for why it is never nothing.
        let middle = selecting::Rect { x: (up.size.0 / 2) as f64 / scale, y: (up.size.1 / 2) as f64 / scale, width: 1.0 / scale, height: 1.0 / scale };
        let part = look.clear.unwrap_or(middle);
        // Edges, not sizes: a pixel of the capture is a pixel of the screen.
        let pixel = |logical: f64| (logical * scale).round() as i32;
        let (left, top) = (pixel(part.x), pixel(part.y));
        window.set_part(ui::Part { x: part.x as f32, y: part.y as f32, width: part.width as f32, height: part.height as f32 });
        window.set_from(ui::Pixels { x: left, y: top, width: pixel(part.right()) - left, height: pixel(part.bottom()) - top });
        window.set_outline_inside(look.outline_inside);
        let handles: Vec<ui::Spot> = look.handles.iter().copied().map(spot).collect();
        if self.handles.iter().ne(handles.iter().cloned()) {
            self.handles.set_vec(handles);
        }
        let (label, label_at) = look.label.unwrap_or_default();
        window.set_label(label.into());
        window.set_label_at(spot(label_at));
        window.set_hint(look.hint.unwrap_or_default().into());
        window.set_status_error(look.status.as_ref().is_some_and(|status| status.error));
        window.set_status(look.status.map(|status| status.text).unwrap_or_default().into());
        let loupe = look.loupe.zip(up.clear.as_ref());
        window.set_loupe(loupe.is_some());
        if let Some((loupe, clear)) = loupe {
            window.set_loupe_at(spot((loupe.frame.x, loupe.frame.y)));
            window.set_loupe_picture(Image::from_rgba8_premultiplied(magnified(clear, loupe.source)));
        }
        // What is drawn on it: all of it anew when one was added or taken
        // away, the last one alone while it is being drawn or moved.
        let shapes = up.selecting.shapes();
        if shapes != self.drawn.as_slice() {
            let drawn = |shape: &Shape| marks::mark(shape, up.clear.as_ref(), scale);
            match shapes.split_last() {
                Some((last, before)) if shapes.len() == self.drawn.len() && before == &self.drawn[..before.len()] => self.marks.set_row_data(before.len(), drawn(last)),
                _ => self.marks.set_vec(shapes.iter().map(drawn).collect::<Vec<_>>()),
            }
            self.drawn = shapes.to_vec();
        }
        window.set_toolbar(look.toolbar.is_some());
        if let Some(toolbar) = look.toolbar {
            window.set_toolbar_at(spot((toolbar.x, toolbar.y)));
            // Where a button's tip has no room below, it is above.
            window.set_tips_above(toolbar.bottom() + 40.0 > up.size.1 as f64 / scale);
        }
        let tool = up.selecting.tool();
        window.set_tool(TOOLS.iter().find(|(known, ..)| *known == tool).map_or("", |(_, id, ..)| *id).into());
        window.set_colour(up.selecting.colour() as i32);
        window.set_ink(marks::colour(up.selecting.colour()));
        window.set_can_undo(!shapes.is_empty());
        if look.typing != self.typing {
            self.typing = look.typing;
            window.set_typing(look.typing.is_some());
            if let Some(line) = look.typing {
                // A new line each time: empty, and with the keyboard. The
                // window has the keyboard by its grab, which Slint does not
                // know of: it is told that the window is the one in use.
                window.set_typed("".into());
                window.set_typing_at(spot(line));
                let _ = window.window().dispatch_event_with_result(slint::platform::WindowEvent::WindowActiveChanged(true));
                window.invoke_focus_typing();
            }
        }
        let cursor = up.selecting.cursor();
        let changed = self.cursor != Some(cursor);
        self.cursor = Some(cursor);
        // Drawn now. Asked through Slint, a frame on X11 is drawn a
        // refresh interval after it was asked for, by a timer
        // (`frame_throttle.rs` of Slint's winit backend): the selection
        // would follow the pointer that much later.
        if changed {
            window.window().with_winit_window(|made| made.set_cursor(icon(cursor)));
        }
        self.life.draw();
    }
}

/// The pixels of the capture that the loupe shows, as a picture of their
/// own: black where the capture ends.
fn magnified(clear: &SharedPixelBuffer<Rgba8Pixel>, from: (i64, i64)) -> SharedPixelBuffer<Rgba8Pixel> {
    let cells = selecting::LOUPE_CELLS;
    let (width, height) = (clear.width() as i64, clear.height() as i64);
    let mut shown = SharedPixelBuffer::<Rgba8Pixel>::new(cells, cells);
    let pixels = clear.as_slice();
    for (row, line) in shown.make_mut_slice().chunks_exact_mut(cells as usize).enumerate() {
        for (column, pixel) in line.iter_mut().enumerate() {
            let (x, y) = (from.0 + column as i64, from.1 + row as i64);
            *pixel = if x >= 0 && y >= 0 && x < width && y < height { pixels[(y * width + x) as usize] } else { Rgba8Pixel { r: 0, g: 0, b: 0, a: 255 } };
        }
    }
    shown
}

/// The capture as Slint draws pictures: as it was read, or as bright as
/// `light` of 256. Made on the thread that asks, in bands at once: it is a
/// whole screen.
fn picture(shot: &Shot, light: u16) -> SharedPixelBuffer<Rgba8Pixel> {
    let mut picture = SharedPixelBuffer::<Rgba8Pixel>::new(shot.width, shot.height);
    let lit = |value: u8| (value as u16 * light / 256) as u8;
    let band = (shot.height as usize).div_ceil(8).max(1) * shot.width as usize * 4;
    std::thread::scope(|scope| {
        for (picture, read) in picture.make_mut_bytes().chunks_mut(band).zip(shot.bgrx.chunks(band)) {
            scope.spawn(move || {
                // Read as blue, green, red, unused.
                for (pixel, read) in picture.chunks_exact_mut(4).zip(read.chunks_exact(4)) {
                    pixel.copy_from_slice(&[lit(read[2]), lit(read[1]), lit(read[0]), 255]);
                }
            });
        }
    });
    picture
}

/// While the overlay is up, asks Slint's thread whether it is there. When
/// it is not, the overlay is taken off the screen from here, and the host
/// told that the capture came to nothing.
fn watch() {
    static WATCHING: AtomicBool = AtomicBool::new(false);
    if WATCHING.swap(true, Ordering::SeqCst) {
        return;
    }
    let watching = std::thread::Builder::new().name("heyo-slint-shot-watch".into()).spawn(move || {
        let mut missed = 0;
        let mut lost = false;
        while UP.load(Ordering::SeqCst) && !lost {
            let before = BEATS.load(Ordering::SeqCst);
            let asked = thread::on(|| {
                BEATS.fetch_add(1, Ordering::SeqCst);
            });
            std::thread::sleep(BEAT);
            missed = if BEATS.load(Ordering::SeqCst) == before { missed + 1 } else { 0 };
            lost = !asked || guard::state() != State::Running || missed >= MISSED_BEATS;
        }
        WATCHING.store(false, Ordering::SeqCst);
        if lost && UP.swap(false, Ordering::SeqCst) {
            // A thread that panicked has had its windows taken away already (`guard`).
            if guard::state() == State::Running {
                eprintln!("heyo: what draws the screenshot overlay does not answer; the overlay is taken off the screen");
                native::take_down(WINDOW.load(Ordering::SeqCst));
            }
            crate::screenshot::cancel();
        }
    });
    if watching.is_err() {
        WATCHING.store(false, Ordering::SeqCst);
    }
}

/// Has the overlay's window made ahead of the first screenshot. On
/// Slint's thread.
pub(super) fn prepare() {
    with(|overlay| overlay.life.ahead(|| {}));
}

/// Shows `shot`, the monitor `area` as it was read, over that monitor, for
/// part of it to be selected. From any thread; it does not wait for the
/// overlay to be on screen. False when Slint cannot show it.
pub(crate) fn show(app: &AppHandle, area: Area, shot: &Shot, mode: Mode, asked: Instant) -> bool {
    UP.store(true, Ordering::SeqCst);
    // The dimmed picture is what is seen first: it is shown as soon as it
    // is made, while the other is being made beside it.
    let (shown, clear) = std::thread::scope(|scope| {
        let clear = scope.spawn(|| picture(shot, 256));
        let dimmed = picture(shot, DIM);
        crate::screenshot::mark("dimmed picture made", asked);
        let capture = Capture { area, mode, dimmed, asked };
        (life::begin(app, move || with(|overlay| overlay.show(capture))), clear.join())
    });
    // (A picture that could not be made is a capture that came to nothing.)
    let shown = shown && clear.is_ok_and(|clear| life::ask(move || with(|overlay| overlay.cleared(clear))));
    if !shown {
        UP.store(false, Ordering::SeqCst);
        return false;
    }
    watch();
    true
}

/// The windows in the capture, the topmost first, in its pixels. They
/// arrive after the overlay is up.
pub(crate) fn windows(windows: Vec<selecting::Area>) {
    life::ask(move || {
        Slot::there(&OVERLAY, |overlay| {
            if let Some(up) = overlay.up.as_mut() {
                up.selecting.windows(&windows);
                overlay.push();
            }
        });
    });
}

/// What was to become of the selection could not be done: the overlay
/// says why, if it is still there.
pub(crate) fn failed(message: String) {
    life::ask(move || {
        Slot::there(&OVERLAY, |overlay| {
            if let Some(up) = overlay.up.as_mut() {
                up.selecting.failed(message);
                overlay.push();
            }
        });
    });
}

/// Whether the overlay is on screen.
pub(crate) fn up() -> bool {
    UP.load(Ordering::SeqCst)
}

/// Takes the overlay off the screen. From any thread; it does not wait.
pub(crate) fn hide() {
    life::ask(|| {
        Slot::there(&OVERLAY, Overlay::close);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_capture_is_shown_with_its_colours_in_order_and_dimmed() {
        // Two pixels, read as blue, green, red and a byte that means nothing.
        let shot = Shot { width: 2, height: 1, bgrx: vec![10, 20, 30, 0, 255, 128, 0, 77] };
        assert_eq!(picture(&shot, 256).as_bytes(), [30, 20, 10, 255, 0, 128, 255, 255]);
        assert_eq!(picture(&shot, DIM).as_bytes(), [18, 12, 6, 255, 0, 77, 153, 255]);
        // A whole screen in bands comes out as one picture, each row in its place.
        let rows = 37;
        let shot = Shot { width: 3, height: rows, bgrx: (0..rows).flat_map(|row| [row as u8, 0, 0, 0].repeat(3)).collect() };
        let clear = picture(&shot, 256);
        assert!(clear.as_bytes().chunks_exact(12).enumerate().all(|(row, pixels)| pixels.chunks_exact(4).all(|pixel| pixel == [0, 0, row as u8, 255])));
    }

    #[test]
    fn the_loupe_shows_the_pixels_round_the_pointer_and_black_past_the_edge() {
        // Each pixel of a capture of 20 by 20 says where it is.
        let shot = Shot { width: 20, height: 20, bgrx: (0..400u32).flat_map(|at| [0, (at / 20) as u8, (at % 20) as u8, 0]).collect() };
        let clear = picture(&shot, 256);
        let cells = selecting::LOUPE_CELLS as usize;
        let at = |shown: &SharedPixelBuffer<Rgba8Pixel>, column: usize, row: usize| shown.as_slice()[row * cells + column];
        let inside = magnified(&clear, (2, 3));
        assert_eq!((at(&inside, 0, 0).r, at(&inside, 0, 0).g), (2, 3));
        assert_eq!((at(&inside, 14, 14).r, at(&inside, 14, 14).g), (16, 17));
        // The pointer at the screen's corner: seven cells of nothing each way.
        let corner = magnified(&clear, (-7, -7));
        assert_eq!(at(&corner, 6, 6), Rgba8Pixel { r: 0, g: 0, b: 0, a: 255 });
        assert_eq!((at(&corner, 7, 7).r, at(&corner, 7, 7).g), (0, 0));
        assert_eq!((at(&corner, 8, 9).r, at(&corner, 8, 9).g), (1, 2));
        let far = magnified(&clear, (13, 13));
        assert_eq!((at(&far, 6, 6).r, at(&far, 6, 6).g), (19, 19));
        assert_eq!(at(&far, 7, 6), Rgba8Pixel { r: 0, g: 0, b: 0, a: 255 });
    }

    #[test]
    fn a_shortcut_is_its_key_on_a_layout_without_its_letter() {
        let key = |logical: &str, code| key_of(&WinitKey::Character(logical.into()), PhysicalKey::Code(code));
        assert_eq!(key("c", KeyCode::KeyC), Some(Key::Character('c')));
        assert_eq!(key("C", KeyCode::KeyC), Some(Key::Character('c')));
        // Dvorak: the letter is what counts where the layout has it.
        assert_eq!(key("s", KeyCode::Semicolon), Some(Key::Character('s')));
        // Cyrillic: the key where the letter is on a Latin layout.
        assert_eq!(key("с", KeyCode::KeyC), Some(Key::Character('c')));
        assert_eq!(key_of(&WinitKey::Named(NamedKey::Escape), PhysicalKey::Code(KeyCode::Escape)), Some(Key::Escape));
        assert_eq!(key_of(&WinitKey::Named(NamedKey::Tab), PhysicalKey::Code(KeyCode::Tab)), None);
    }
}

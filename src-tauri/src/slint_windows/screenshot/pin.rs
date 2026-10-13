//! A pinned capture: the selected part of a screenshot kept on the screen,
//! where it was, in a small window of its own above the others. There can
//! be several. One is dragged to move it; a double click, a right click or
//! Escape closes it, and its picture goes with it.
//!
//! Each is one of Slint's windows (`super::super::life`), an ordinary
//! one that the window manager keeps: it can be moved and given the
//! keyboard like any other. Like the overlay it hears of the pointer and
//! the keys from winit.

use std::time::{Duration, Instant};

use slint::winit_030::winit::event::{ElementState, MouseButton, WindowEvent};
use slint::winit_030::winit::keyboard::{Key, NamedKey};
use slint::winit_030::{EventResult, WinitWindowAccessor};
use slint::{ComponentHandle, Image, Rgba8Pixel, SharedPixelBuffer};
use tauri::AppHandle;

use super::super::life::{self, Life, Lives, Slot, Slotted};
use super::super::standing::{left_to_go, Button, Keyboard, Layer, Pointed, Standing};
use crate::platform::desktop::OwnPlace;
use super::super::ui;

/// Above the other windows, and still one the desktop arranges; in no
/// taskbar; and hiding what is behind it, being a picture.
const STANDING: Standing = Standing { layer: Layer::Above, keyboard: Keyboard::Ordinary, listed: false, clear: false };

/// The smallest a pinned capture's window is, either way.
const LEAST: f32 = 24.0;
/// How far the pointer goes with its button down before the window is
/// moved: a double click must not move it.
const SLACK: f64 = 4.0;
/// Two presses this close in time and place are a double click.
const TWICE: Duration = Duration::from_millis(400);

struct Pinned {
    id: u64,
    life: Life<ui::PinWindow>,
    /// The pointer, in the window's pixels; where its button went down,
    /// while it is down; and when and where it last went down.
    pointer: (f64, f64),
    pressed: Option<(f64, f64)>,
    last: Option<(Instant, (f64, f64))>,
    /// It is being moved by the pointer, by this window itself: nobody
    /// else moves a window where it is a layer surface.
    moving: bool,
    /// How far it has moved itself since the button went down.
    moved: (f64, f64),
}

/// What the window is called, as the markup has it.
const TITLE: &str = "Heyo Pinned Screenshot";

/// The captures that are pinned, and what the next one is known by.
struct Pins {
    pinned: Vec<Pinned>,
    next: u64,
}

thread_local! {
    static PINS: Slot<Pins> = const { Slot::new() };
}

impl Lives for Pins {
    const NAME: &'static str = "a pinned capture";

    fn make() -> Result<Self, String> {
        Ok(Pins { pinned: Vec::new(), next: 0 })
    }

    /// Each is put in a window made for the display as it is, where it
    /// was, and is as before the pixels it was read as.
    fn carry(self, new: &mut Self) {
        for pin in self.pinned {
            let at = pin.life.window().window().position();
            let picture = pin.life.window().get_picture();
            drop(pin);
            new.add(picture, (at.x, at.y));
        }
    }
}

/// Pins a picture to the screen with its top left corner at `at`, in the
/// screen's pixels. From any thread; it does not wait. False when Slint
/// cannot show it.
pub(crate) fn show(app: &AppHandle, picture: image::RgbaImage, at: (i32, i32)) -> bool {
    life::begin(app, move || {
        let (width, height) = picture.dimensions();
        let picture = Image::from_rgba8_premultiplied(SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(picture.as_raw(), width, height));
        Slot::with(&PINS, |pins| pins.add(picture, at));
    })
}

impl Slotted for Pins {
    fn slot() -> &'static std::thread::LocalKey<Slot<Self>> {
        &PINS
    }
}

impl Pins {
    fn add(&mut self, picture: Image, at: (i32, i32)) {
        // One that was taken off the screen from outside, when Slint's
        // thread did not answer, is not coming back.
        self.pinned.retain(|pin| pin.life.wanted());
        let life = match Life::new(STANDING, ui::PinWindow::new, |_, _, _| {}) {
            Ok(life) => life,
            Err(e) => return eprintln!("heyo: the capture could not be pinned: {e}"),
        };
        let window = life.window();
        let (width, height) = (picture.size().width, picture.size().height);
        window.set_picture(picture);
        let least = (LEAST * window.window().scale_factor()).ceil() as u32;
        window.window().set_size(slint::PhysicalSize::new(width.max(least), height.max(least)));
        window.window().set_position(slint::PhysicalPosition::new(at.0, at.1));
        let id = self.next;
        self.next += 1;
        window.window().on_winit_window_event(move |_, event| told(id, event));
        // Where the window is not winit's, it hears of the pointer itself;
        // and where it cannot place itself, the desktop is asked to.
        life.pointed(move |pointed| {
            heard(id, pointed);
            // All of it is the pin's: Slint has nothing to answer there.
            false
        });
        life.ask_desktop(TITLE, Some(OwnPlace::At { at, size: (width.max(least), height.max(least)) }));
        window.window().on_close_requested(move || {
            close(id);
            slint::CloseRequestResponse::KeepWindowShown
        });
        life.on(|came| {
            if let Err(e) = came {
                eprintln!("heyo: the capture could not be pinned: {e}");
            }
        });
        self.pinned.push(Pinned { id, life, pointer: (0.0, 0.0), pressed: None, last: None, moving: false, moved: (0.0, 0.0) });
    }
}

/// Takes a pinned capture off the screen, once what asked for it is done
/// with: it may be the window's own event that did.
fn close(id: u64) {
    slint::Timer::single_shot(Duration::ZERO, move || {
        Slot::there(&PINS, |pins| pins.pinned.retain(|pin| pin.id != id));
    });
}

/// What the pointer did in a pinned capture, whoever said so: dragged, it
/// moves; pressed twice, or with the other button, it closes.
fn heard(id: u64, pointed: Pointed) {
    Slot::there(&PINS, |pins| {
        let Some(pin) = pins.pinned.iter_mut().find(|pin| pin.id == id) else { return };
        let scale = f64::from(pin.life.window().window().scale_factor()).max(0.1);
        let apart = |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).hypot(a.1 - b.1) / scale;
        match pointed {
            Pointed::Moved(to) => {
                pin.pointer = to;
                match pin.pressed {
                    // It goes after the pointer, by what is left to go.
                    Some(from) if pin.moving => {
                        let went = pin.life.move_by(left_to_go((to.0 - from.0, to.1 - from.1), pin.moved));
                        pin.moved = (pin.moved.0 + went.0, pin.moved.1 + went.1);
                    }
                    Some(from) if apart(from, to) >= SLACK => {
                        pin.last = None;
                        if pin.life.drag() {
                            // The desktop moves it from here, and has the pointer until the button is let go.
                            pin.pressed = None;
                        } else {
                            pin.moving = true;
                        }
                    }
                    _ => {}
                }
            }
            Pointed::Pressed(Button::Left) => {
                let now = Instant::now();
                if pin.last.is_some_and(|(when, at)| now.duration_since(when) < TWICE && apart(at, pin.pointer) < SLACK) {
                    close(id);
                } else {
                    (pin.pressed, pin.last, pin.moved) = (Some(pin.pointer), Some((now, pin.pointer)), (0.0, 0.0));
                }
            }
            Pointed::Released(Button::Left) => (pin.pressed, pin.moving) = (None, false),
            Pointed::Pressed(Button::Right) => close(id),
            Pointed::Released(Button::Right) | Pointed::Left => {}
        }
    });
}

fn told(id: u64, event: &WindowEvent) -> EventResult {
    {
        match event {
            WindowEvent::CursorMoved { position, .. } => heard(id, Pointed::Moved((position.x, position.y))),
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. } => heard(id, Pointed::Pressed(Button::Left)),
            WindowEvent::MouseInput { state: ElementState::Released, button: MouseButton::Left, .. } => heard(id, Pointed::Released(Button::Left)),
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Right, .. } => heard(id, Pointed::Pressed(Button::Right)),
            // Not a key that was down already when the window was given the
            // keyboard, which winit says as a press too: Escape that closed
            // another window must not close this one as the focus comes to it.
            WindowEvent::KeyboardInput { event, is_synthetic: false, .. } if event.state == ElementState::Pressed && event.logical_key == Key::Named(NamedKey::Escape) => close(id),
            _ => return EventResult::Propagate,
        }
        EventResult::PreventDefault
    }
}

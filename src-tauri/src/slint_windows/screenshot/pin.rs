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
use super::super::thread::{self, Kind};
use super::super::ui;

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
}

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
        let window = match thread::making(Kind::Pinned, ui::PinWindow::new) {
            Ok(window) => window,
            Err(e) => return eprintln!("heyo: the capture could not be pinned: {e}"),
        };
        let (width, height) = (picture.size().width, picture.size().height);
        window.set_picture(picture);
        let least = (LEAST * window.window().scale_factor()).ceil() as u32;
        window.window().set_size(slint::PhysicalSize::new(width.max(least), height.max(least)));
        window.window().set_position(slint::PhysicalPosition::new(at.0, at.1));
        let id = self.next;
        self.next += 1;
        window.window().on_winit_window_event(move |_, event| told(id, event));
        window.window().on_close_requested(move || {
            close(id);
            slint::CloseRequestResponse::KeepWindowShown
        });
        // Left to Slint to show and hide: it is an ordinary window. Above the
        // other windows: the markup says so too, but the window manager was
        // not told of it on X11, so winit is asked once its window is there.
        let life = Life::new(window, None, |_, made, first| {
            if first {
                made.set_window_level(slint::winit_030::winit::window::WindowLevel::AlwaysOnTop);
            }
        });
        life.on(|came| {
            if let Err(e) = came {
                eprintln!("heyo: the capture could not be pinned: {e}");
            }
        });
        self.pinned.push(Pinned { id, life, pointer: (0.0, 0.0), pressed: None, last: None });
    }
}

/// Takes a pinned capture off the screen, once what asked for it is done
/// with: it may be the window's own event that did.
fn close(id: u64) {
    slint::Timer::single_shot(Duration::ZERO, move || {
        Slot::there(&PINS, |pins| pins.pinned.retain(|pin| pin.id != id));
    });
}

fn told(id: u64, event: &WindowEvent) -> EventResult {
    Slot::there(&PINS, |pins| {
        let Some(pin) = pins.pinned.iter_mut().find(|pin| pin.id == id) else { return EventResult::Propagate };
        let scale = f64::from(pin.life.window().window().scale_factor()).max(0.1);
        let apart = |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).hypot(a.1 - b.1) / scale;
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                pin.pointer = (position.x, position.y);
                if pin.pressed.is_some_and(|from| apart(from, pin.pointer) >= SLACK) {
                    // The window manager moves it from here, and has the pointer until the button is let go.
                    (pin.pressed, pin.last) = (None, None);
                    pin.life.window().window().with_winit_window(|made| {
                        let _ = made.drag_window();
                    });
                }
            }
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. } => {
                let now = Instant::now();
                if pin.last.is_some_and(|(when, at)| now.duration_since(when) < TWICE && apart(at, pin.pointer) < SLACK) {
                    close(id);
                } else {
                    (pin.pressed, pin.last) = (Some(pin.pointer), Some((now, pin.pointer)));
                }
            }
            WindowEvent::MouseInput { state: ElementState::Released, button: MouseButton::Left, .. } => pin.pressed = None,
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Right, .. } => close(id),
            // Not a key that was down already when the window was given the
            // keyboard, which winit says as a press too: Escape that closed
            // another window must not close this one as the focus comes to it.
            WindowEvent::KeyboardInput { event, is_synthetic: false, .. } if event.state == ElementState::Pressed && event.logical_key == Key::Named(NamedKey::Escape) => close(id),
            _ => return EventResult::Propagate,
        }
        EventResult::PreventDefault
    })
    .unwrap_or(EventResult::Propagate)
}

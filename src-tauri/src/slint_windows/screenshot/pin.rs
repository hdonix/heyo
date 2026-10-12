//! A pinned capture: the selected part of a screenshot kept on the screen,
//! where it was, in a small window of its own above the others. There can
//! be several. One is dragged to move it; a double click, a right click or
//! Escape closes it, and its picture goes with it.
//!
//! Each is one of Slint's windows (`super::super::thread`), an ordinary
//! one that the window manager keeps: it can be moved and given the
//! keyboard like any other. Like the overlay it hears of the pointer and
//! the keys from winit.

use std::cell::{Cell, RefCell};
use std::time::{Duration, Instant};

use slint::winit_030::winit::event::{ElementState, MouseButton, WindowEvent};
use slint::winit_030::winit::keyboard::{Key, NamedKey};
use slint::winit_030::{EventResult, WinitWindowAccessor};
use slint::{ComponentHandle, Image, Rgba8Pixel, SharedPixelBuffer};
use tauri::AppHandle;

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
    window: ui::PinWindow,
    /// The pointer, in the window's pixels; where its button went down,
    /// while it is down; and when and where it last went down.
    pointer: (f64, f64),
    pressed: Option<(f64, f64)>,
    last: Option<(Instant, (f64, f64))>,
}

thread_local! {
    static PINNED: RefCell<Vec<Pinned>> = const { RefCell::new(Vec::new()) };
    static NEXT: Cell<u64> = const { Cell::new(0) };
}

/// Pins a picture to the screen with its top left corner at `at`, in the
/// screen's pixels. From any thread; it does not wait. False when Slint
/// cannot show it.
pub(crate) fn show(app: &AppHandle, picture: image::RgbaImage, at: (i32, i32)) -> bool {
    super::super::ensure(app) && thread::on(move || add(picture, at))
}

fn add(picture: image::RgbaImage, at: (i32, i32)) {
    let window = match thread::making(Kind::Pinned, ui::PinWindow::new) {
        Ok(window) => window,
        Err(e) => return eprintln!("heyo: the capture could not be pinned: {e}"),
    };
    let (width, height) = picture.dimensions();
    window.set_picture(Image::from_rgba8_premultiplied(SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(picture.as_raw(), width, height)));
    let least = (LEAST * window.window().scale_factor()).ceil() as u32;
    window.window().set_size(slint::PhysicalSize::new(width.max(least), height.max(least)));
    window.window().set_position(slint::PhysicalPosition::new(at.0, at.1));
    let id = NEXT.with(|next| next.replace(next.get() + 1));
    window.window().on_winit_window_event(move |_, event| told(id, event));
    window.window().on_close_requested(move || {
        close(id);
        slint::CloseRequestResponse::KeepWindowShown
    });
    if let Err(e) = window.show() {
        return eprintln!("heyo: the capture could not be pinned: {e}");
    }
    // Above the other windows. The markup says so too, but the window
    // manager was not told of it on X11: winit is asked once the window is
    // there.
    let made = window.as_weak();
    let _ = slint::spawn_local(async move {
        let Some(window) = made.upgrade() else { return };
        if let Ok(made) = window.window().winit_window().await {
            made.set_window_level(slint::winit_030::winit::window::WindowLevel::AlwaysOnTop);
        }
    });
    PINNED.with(|pinned| pinned.borrow_mut().push(Pinned { id, window, pointer: (0.0, 0.0), pressed: None, last: None }));
}

/// Takes a pinned capture off the screen, once what asked for it is done
/// with: it may be the window's own event that did.
fn close(id: u64) {
    slint::Timer::single_shot(Duration::ZERO, move || {
        let gone = PINNED.with(|pinned| {
            let mut pinned = pinned.borrow_mut();
            let at = pinned.iter().position(|pin| pin.id == id)?;
            Some(pinned.remove(at))
        });
        if let Some(pin) = gone {
            let _ = pin.window.hide();
        }
    });
}

fn told(id: u64, event: &WindowEvent) -> EventResult {
    PINNED.with(|pinned| {
        let Ok(mut pinned) = pinned.try_borrow_mut() else { return EventResult::Propagate };
        let Some(pin) = pinned.iter_mut().find(|pin| pin.id == id) else { return EventResult::Propagate };
        let scale = f64::from(pin.window.window().scale_factor()).max(0.1);
        let apart = |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).hypot(a.1 - b.1) / scale;
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                pin.pointer = (position.x, position.y);
                if pin.pressed.is_some_and(|from| apart(from, pin.pointer) >= SLACK) {
                    // The window manager moves it from here, and has the pointer until the button is let go.
                    (pin.pressed, pin.last) = (None, None);
                    pin.window.window().with_winit_window(|made| {
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
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed && event.logical_key == Key::Named(NamedKey::Escape) => close(id),
            _ => return EventResult::Propagate,
        }
        EventResult::PreventDefault
    })
}

//! An application for tests against a real X server: a window that has the
//! focus and does what a text field does with the keys that matter here. It
//! erases a character at Backspace, asks for the clipboard at Ctrl+V (or
//! Ctrl+Shift+V) and adds what it is given, and types an "a" at that key.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{AtomEnum, ConnectionExt as _, CreateWindowAux, EventMask, InputFocus, Window, WindowClass};
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;

// Keys by their place on the keyboard (evdev codes, as X numbers them).
pub(super) const CONTROL: u8 = 37;
pub(super) const A: u8 = 38;
pub(super) const V: u8 = 55;
pub(super) const BACKSPACE: u8 = 22;

pub(super) struct Field {
    text: Arc<Mutex<String>>,
    running: Arc<AtomicBool>,
    pub(super) window: Window,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Field {
    /// Opens the window and gives it the focus.
    pub(super) fn open() -> Field {
        let (conn, screen) = RustConnection::connect(None).unwrap();
        let root = conn.setup().roots[screen].root;
        let window = conn.generate_id().unwrap();
        let aux = CreateWindowAux::new().event_mask(EventMask::KEY_PRESS | EventMask::KEY_RELEASE);
        conn.create_window(0, window, root, 0, 0, 200, 100, 0, WindowClass::INPUT_OUTPUT, 0, &aux).unwrap();
        conn.map_window(window).unwrap();
        conn.set_input_focus(InputFocus::POINTER_ROOT, window, x11rb::CURRENT_TIME).unwrap();
        conn.get_input_focus().unwrap().reply().unwrap();
        let (text, running) = (Arc::new(Mutex::new(String::new())), Arc::new(AtomicBool::new(true)));
        let (field, alive) = (text.clone(), running.clone());
        let thread = std::thread::spawn(move || {
            let atom = |name: &str| conn.intern_atom(false, name.as_bytes()).unwrap().reply().unwrap().atom;
            let (clipboard, utf8, property) = (atom("CLIPBOARD"), atom("UTF8_STRING"), atom("HEYO_TEST_PASTE"));
            let mut control = false;
            while alive.load(Ordering::SeqCst) {
                let Some(event) = conn.poll_for_event().unwrap() else {
                    std::thread::sleep(Duration::from_millis(1));
                    continue;
                };
                match event {
                    Event::KeyPress(e) if e.detail == BACKSPACE => drop(field.lock().unwrap().pop()),
                    Event::KeyPress(e) if e.detail == CONTROL => control = true,
                    Event::KeyRelease(e) if e.detail == CONTROL => control = false,
                    Event::KeyPress(e) if e.detail == V && control => {
                        conn.convert_selection(window, clipboard, utf8, property, x11rb::CURRENT_TIME).unwrap();
                        conn.flush().unwrap();
                    }
                    Event::KeyPress(e) if e.detail == A => field.lock().unwrap().push('a'),
                    Event::SelectionNotify(e) if e.property != x11rb::NONE => {
                        let pasted = conn.get_property(true, window, property, AtomEnum::ANY, 0, 1 << 20).unwrap().reply().unwrap().value;
                        field.lock().unwrap().push_str(&String::from_utf8_lossy(&pasted));
                    }
                    _ => {}
                }
            }
        });
        Field { text, running, window, thread: Some(thread) }
    }

    /// What it holds, once whatever is on its way has arrived.
    pub(super) fn text(&self) -> String {
        std::thread::sleep(Duration::from_millis(60));
        self.text.lock().unwrap().clone()
    }
}

impl Drop for Field {
    fn drop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

//! Clipboard changes through the XFixes extension, which reports each new
//! owner of the selection, with what it offers.

use crate::platform::clipboard_watch::Change;

use std::collections::HashMap;
use std::error::Error;
use std::time::{Duration, Instant};

use x11rb::connection::Connection;
use x11rb::protocol::xfixes::{ConnectionExt as _, SelectionEventMask};
use x11rb::protocol::xproto::{Atom, AtomEnum, ConnectionExt as _, CreateWindowAux, Window, WindowClass};
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;
use x11rb::CURRENT_TIME;

const TEXT_TARGETS: &[&str] = &["UTF8_STRING", "text/plain;charset=utf-8", "text/plain", "STRING", "TEXT"];
/// Targets password managers (KeePassXC, KDE) add to secrets.
const SECRET_TARGETS: &[&str] = &["x-kde-passwordManagerHint", "application/x-nspasteboard-concealed-type"];

struct Watcher {
    conn: RustConnection,
    window: Window,
    names: HashMap<Atom, String>,
}

impl Watcher {
    fn atom(&self, name: &str) -> Result<Atom, Box<dyn Error>> {
        Ok(self.conn.intern_atom(false, name.as_bytes())?.reply()?.atom)
    }

    fn name(&mut self, atom: Atom) -> String {
        if let Some(name) = self.names.get(&atom) {
            return name.clone();
        }
        let name = self
            .conn
            .get_atom_name(atom)
            .ok()
            .and_then(|c| c.reply().ok())
            .map(|r| String::from_utf8_lossy(&r.name).into_owned())
            .unwrap_or_default();
        self.names.insert(atom, name.clone());
        name
    }

    /// The formats the clipboard owner offers.
    fn targets(&mut self, clipboard: Atom, targets: Atom, property: Atom) -> Result<Vec<String>, Box<dyn Error>> {
        self.conn.convert_selection(self.window, clipboard, targets, property, CURRENT_TIME)?;
        self.conn.flush()?;
        let deadline = Instant::now() + Duration::from_millis(500);
        loop {
            match self.conn.poll_for_event()? {
                Some(Event::SelectionNotify(e)) if e.requestor == self.window => {
                    if e.property == x11rb::NONE {
                        return Ok(Vec::new());
                    }
                    break;
                }
                Some(_) => {}
                None if Instant::now() > deadline => return Ok(Vec::new()),
                None => std::thread::sleep(Duration::from_millis(5)),
            }
        }
        let reply = self
            .conn
            .get_property(true, self.window, property, AtomEnum::ATOM, 0, 4096)?
            .reply()?;
        let atoms: Vec<Atom> = reply.value32().map(|v| v.collect()).unwrap_or_default();
        Ok(atoms.into_iter().map(|a| self.name(a)).collect())
    }
}

/// Reports each change of the clipboard's owner until the connection ends.
pub fn watch(mut on_change: impl FnMut(Change)) -> Result<(), Box<dyn Error>> {
    let (conn, screen) = RustConnection::connect(None)?;
    let root = conn.setup().roots[screen].root;
    conn.xfixes_query_version(5, 0)?.reply()?;
    let window = conn.generate_id()?;
    conn.create_window(0, window, root, 0, 0, 1, 1, 0, WindowClass::INPUT_ONLY, 0, &CreateWindowAux::new())?;
    let mut watcher = Watcher { conn, window, names: HashMap::new() };
    let clipboard = watcher.atom("CLIPBOARD")?;
    let targets = watcher.atom("TARGETS")?;
    let property = watcher.atom("HEYO_CLIPBOARD_TARGETS")?;
    watcher
        .conn
        .xfixes_select_selection_input(window, clipboard, SelectionEventMask::SET_SELECTION_OWNER)?;
    watcher.conn.flush()?;

    loop {
        let Event::XfixesSelectionNotify(event) = watcher.conn.wait_for_event()? else { continue };
        if event.owner == x11rb::NONE {
            continue;
        }
        let offered = watcher.targets(clipboard, targets, property)?;
        on_change(Change {
            has_text: offered.iter().any(|t| TEXT_TARGETS.contains(&t.as_str())),
            has_image: offered.iter().any(|t| t.starts_with("image/")),
            secret: offered.iter().any(|t| SECRET_TARGETS.contains(&t.as_str())),
            // The copy came from whatever window is in front.
            source: super::windows::active_class(),
        });
    }
}

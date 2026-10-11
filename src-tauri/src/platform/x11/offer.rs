//! Text offered on the clipboard for an application to paste, with Heyo
//! itself as the clipboard's owner, so that it is known when the
//! application has taken it.
//!
//! On X11 the clipboard holds nothing: its owner is asked for the text at
//! the moment of each paste. Text written the toolkit's way is answered for
//! somewhere inside the toolkit, and whoever pressed the paste shortcut can
//! only wait a while and hope. Here the question arrives in the open: once
//! the application in front has asked for the text, the paste has happened,
//! and until it has asked, nothing else may be put in the text's place.
//!
//! The offer says it is a secret (the hint password managers give), so that
//! a clipboard manager which honours that does not record pieces of text
//! that are only passing through; Heyo's own history hears of them neither
//! way (`pasting`).
//!
//! Heyo stays the owner until something else is copied, as any application
//! would: by the user, or by `pasting` putting the user's text back.

use std::error::Error;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{Atom, AtomEnum, ConnectionExt as _, CreateWindowAux, EventMask, PropMode, SelectionNotifyEvent, SelectionRequestEvent, Window, WindowClass, SELECTION_NOTIFY_EVENT};
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

/// More text than this is not offered: it would have to be handed over in
/// pieces (the INCR protocol), which nothing pasted this way needs.
pub(super) const MOST: usize = 100_000;

/// The X client a window belongs to: what the ids of all its windows have
/// in common.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Client(u32);

impl Client {
    pub(super) fn of(conn: &RustConnection, window: Window) -> Client {
        Client(window & !conn.setup().resource_id_mask)
    }
}

struct Atoms {
    clipboard: Atom,
    targets: Atom,
    /// The names plain text is asked for by.
    text: Vec<Atom>,
    /// Latin-1, which is all the oldest of them can take.
    string: Atom,
    secret: Atom,
}

#[derive(Default)]
struct Offered {
    /// The text on offer; none when the clipboard is someone else's.
    text: Option<String>,
    /// Counts the texts offered, so that one being waited for is told from
    /// the next.
    serial: u64,
    /// The clients that asked for the text now on offer.
    taken: Vec<Client>,
}

struct Offering {
    conn: RustConnection,
    window: Window,
    atoms: Atoms,
    offered: Mutex<Offered>,
    changed: Condvar,
    /// Held while the clipboard is taken, and while a notice of its loss is
    /// looked into, so that the one cannot undo the other.
    owning: Mutex<()>,
}

impl Offering {
    fn offered(&self) -> MutexGuard<'_, Offered> {
        self.offered.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn owning(&self) -> MutexGuard<'_, ()> {
        self.owning.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn start() -> Result<Arc<Offering>, Box<dyn Error>> {
        let (conn, screen) = RustConnection::connect(None)?;
        let root = conn.setup().roots[screen].root;
        let window = conn.generate_id()?;
        conn.create_window(0, window, root, -1, -1, 1, 1, 0, WindowClass::INPUT_ONLY, 0, &CreateWindowAux::new())?;
        let atom = |name: &str| -> Result<Atom, Box<dyn Error>> { Ok(conn.intern_atom(false, name.as_bytes())?.reply()?.atom) };
        let atoms = Atoms {
            clipboard: atom("CLIPBOARD")?,
            targets: atom("TARGETS")?,
            text: vec![atom("UTF8_STRING")?, atom("text/plain;charset=utf-8")?, atom("text/plain;charset=UTF-8")?, atom("text/plain")?, atom("TEXT")?],
            string: AtomEnum::STRING.into(),
            secret: atom("x-kde-passwordManagerHint")?,
        };
        let offering = Arc::new(Offering { conn, window, atoms, offered: Mutex::default(), changed: Condvar::new(), owning: Mutex::new(()) });
        let serving = offering.clone();
        std::thread::Builder::new().name("clipboard-offer".into()).spawn(move || {
            if let Err(e) = serving.serve() {
                eprintln!("heyo: the clipboard is no longer offered text: {e}");
            }
            // Whoever waits is not to wait for this any longer.
            serving.offered().text = None;
            serving.changed.notify_all();
        })?;
        Ok(offering)
    }

    /// Answers whoever asks for the clipboard for as long as the session lasts.
    fn serve(&self) -> Result<(), Box<dyn Error>> {
        loop {
            match self.conn.wait_for_event()? {
                Event::SelectionRequest(request) => {
                    let took = self.answer(&request)?;
                    // The answer is with the server before anyone hears
                    // that the text was taken: whatever is pressed after
                    // that reaches the application after its text.
                    self.conn.get_input_focus()?.reply()?;
                    if let Some((client, serial)) = took {
                        let mut offered = self.offered();
                        if offered.serial == serial && !offered.taken.contains(&client) {
                            offered.taken.push(client);
                        }
                        drop(offered);
                        self.changed.notify_all();
                    }
                }
                Event::SelectionClear(clear) if clear.selection == self.atoms.clipboard => {
                    // The notice may be of an earlier loss, with the
                    // clipboard taken back since: the server knows.
                    let _owning = self.owning();
                    if self.conn.get_selection_owner(self.atoms.clipboard)?.reply()?.owner != self.window {
                        self.offered().text = None;
                        self.changed.notify_all();
                    }
                }
                _ => {}
            }
        }
    }

    /// Answers one request. If it was for the text, says which client has
    /// taken which text.
    fn answer(&self, request: &SelectionRequestEvent) -> Result<Option<(Client, u64)>, Box<dyn Error>> {
        let atoms = &self.atoms;
        // An old requestor names no property and means the target's name.
        let property = if request.property == x11rb::NONE { request.target } else { request.property };
        let offered = self.offered();
        let mut took = None;
        let given = match offered.text.as_deref().filter(|_| request.selection == atoms.clipboard) {
            None => false,
            Some(_) if request.target == atoms.targets => {
                let all: Vec<Atom> = [atoms.targets, atoms.string, atoms.secret].into_iter().chain(atoms.text.iter().copied()).collect();
                self.conn.change_property32(PropMode::REPLACE, request.requestor, property, AtomEnum::ATOM, &all)?;
                true
            }
            Some(_) if request.target == atoms.secret => {
                self.conn.change_property8(PropMode::REPLACE, request.requestor, property, atoms.secret, b"secret")?;
                true
            }
            Some(text) if atoms.text.contains(&request.target) || request.target == atoms.string => {
                if request.target == atoms.string {
                    let latin: Vec<u8> = text.chars().map(|c| u8::try_from(u32::from(c)).unwrap_or(b'?')).collect();
                    self.conn.change_property8(PropMode::REPLACE, request.requestor, property, atoms.string, &latin)?;
                } else {
                    // TEXT is answered in an encoding of the owner's choosing.
                    let kind = if request.target == atoms.text[4] { atoms.text[0] } else { request.target };
                    self.conn.change_property8(PropMode::REPLACE, request.requestor, property, kind, text.as_bytes())?;
                }
                took = Some((Client::of(&self.conn, request.requestor), offered.serial));
                true
            }
            Some(_) => false,
        };
        drop(offered);
        let notify = SelectionNotifyEvent {
            response_type: SELECTION_NOTIFY_EVENT,
            sequence: 0,
            time: request.time,
            requestor: request.requestor,
            selection: request.selection,
            target: request.target,
            property: if given { property } else { x11rb::NONE },
        };
        self.conn.send_event(false, request.requestor, EventMask::NO_EVENT, notify)?;
        Ok(took)
    }
}

fn offering() -> Result<&'static Arc<Offering>, Box<dyn Error>> {
    static OFFERING: OnceLock<Result<Arc<Offering>, String>> = OnceLock::new();
    OFFERING.get_or_init(|| Offering::start().map_err(|e| e.to_string())).as_ref().map_err(|e| e.clone().into())
}

/// A text on offer.
pub(super) struct Offer {
    of: &'static Arc<Offering>,
    serial: u64,
}

/// Makes `text` what the clipboard holds, and returns once it does.
pub(super) fn put(text: &str) -> Result<Offer, Box<dyn Error>> {
    if text.len() > MOST {
        return Err("too much text to offer on the clipboard at once".into());
    }
    let of = offering()?;
    let _owning = of.owning();
    let serial = {
        let mut offered = of.offered();
        offered.text = Some(text.to_owned());
        offered.serial += 1;
        offered.taken.clear();
        offered.serial
    };
    // While the clipboard is still Heyo's the new text simply takes the
    // old one's place: nobody who follows the clipboard hears of it.
    let owner = || -> Result<Window, Box<dyn Error>> { Ok(of.conn.get_selection_owner(of.atoms.clipboard)?.reply()?.owner) };
    if owner()? != of.window {
        of.conn.set_selection_owner(of.window, of.atoms.clipboard, x11rb::CURRENT_TIME)?;
        if owner()? != of.window {
            return Err("the clipboard could not be taken".into());
        }
    }
    Ok(Offer { of, serial })
}

impl Offer {
    /// Waits until `client` has asked for this text, for `patience` at most.
    /// Whether it has.
    pub(super) fn taken_by(&self, client: Client, patience: Duration) -> bool {
        let deadline = Instant::now() + patience;
        let mut offered = self.of.offered();
        loop {
            if offered.serial == self.serial && offered.taken.contains(&client) {
                return true;
            }
            // Another text in its place, or the clipboard gone to someone
            // else: this one will not be asked for any more.
            if offered.serial != self.serial || offered.text.is_none() {
                return false;
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return false;
            }
            offered = self.of.changed.wait_timeout(offered, left).unwrap_or_else(|poisoned| poisoned.into_inner()).0;
        }
    }
}

//! Writing into one window a piece at a time (`platform::typed::Writing`):
//! each piece is Backspaces and a paste, pressed with XTEST, with the text
//! offered on the clipboard (`offer`).
//!
//! Erasing in someone else's text field is only safe while nothing but
//! these pieces has touched it. So from the moment writing begins the
//! keyboard and the mouse are listened to, on the connection the keys are
//! pressed through: the server answers a question only after it has sent
//! every event from before it, so "has anything happened?" can be asked and
//! answered just ahead of each piece, with no gap but the few microseconds
//! until the keys arrive. Listening is all it is (raw events, as in
//! `typed`): nothing is grabbed.
//!
//! What counts as the user's doing something to the text:
//! - a key going down that is not a modifier, whoever pressed it (a
//!   program's keys count too: someone working through a remote keyboard
//!   types with XTEST). Heyo's own keys are known by being exactly the ones
//!   it just pressed, in order. A key already down is only repeating: the
//!   key held to talk does. One of Heyo's hotkeys is the desktop's to
//!   handle and never reaches the application;
//! - a mouse button or a touch (not the wheel), unless it was on one of
//!   Heyo's own windows. Those presses are counted by whoever shows the
//!   window, and the count reaches this a moment after the press itself:
//!   so a press is not held against the user until that moment has passed;
//! - the keyboard's focus leaving the window.
//!
//! While another program has taken the keyboard for itself (the desktop,
//! for as long as a hotkey is held; a menu; a prompt for a password) keys
//! would go to that program: a piece waits for it to let go, and is not
//! written if it does not. Whether one has is asked of the server, which
//! says so when asked for the keyboard; it is asked for a window that can
//! never have it, so the answer is all that comes of asking.
//!
//! Modifiers the user holds would turn Backspace and the paste shortcut
//! into other keys. They are let go at the start of a piece and pressed
//! again at its end, in the same handing-over. Should the user let go of
//! one in just that moment, the pressing-again would leave it down for
//! good: the release shows among the events that come back, and the key is
//! then let go once more.

use std::collections::VecDeque;
use std::error::Error;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use x11rb::connection::Connection;
use x11rb::protocol::xinput::{self, ConnectionExt as _, XIEventMask};
use x11rb::protocol::xproto::{ChangeWindowAttributesAux, ConnectionExt as _, CreateWindowAux, EventMask, GrabMode, GrabStatus, NotifyMode, Window, WindowClass};
use x11rb::protocol::xtest::ConnectionExt as _;
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;
use xkbcommon::xkb as xkbc;

use super::offer::{self, Client};
use super::typed::{is_modifier, mapping, patience, press, readable, Keys, Stroke, MOST_AT_ONCE};
use crate::platform::typed::{Lost, Writing};

/// The modifier bits of the core protocol, as the modifier mapping numbers
/// them: Shift, Lock, Control, Mod1 (Alt), Mod2 (Num Lock), Mod3, Mod4
/// (Super), Mod5 (AltGr).
const SHIFT: u16 = 1 << 0;
const LOCK: u16 = 1 << 1;
const CONTROL: u16 = 1 << 2;
const ALT: u16 = 1 << 3;
const NUM_LOCK: u16 = 1 << 4;
const SUPER: u16 = 1 << 6;
/// The modifiers a hotkey is told by.
const NAMED: u16 = SHIFT | CONTROL | ALT | SUPER;
/// How long a piece waits for another program to let go of the keyboard.
const GRAB_PATIENCE: Duration = Duration::from_millis(1500);
/// How long a press of a mouse button may yet turn out to have been on one
/// of Heyo's own windows: the window says so itself, by a longer way than
/// the press takes to be heard here.
const PRESS_PATIENCE: Duration = Duration::from_millis(300);

/// What was heard of the keyboard and the mouse since writing began, and
/// what it means for the text. It only remembers and answers: `Hand` feeds
/// it what the server reports.
#[derive(Default)]
pub(super) struct Heard {
    /// The modifier keys: keycode and the modifier it is (0 for a key that
    /// only looks like one).
    modifiers: Vec<(u8, u16)>,
    /// Heyo's hotkeys: the modifiers held and the keys that complete them.
    hotkeys: Vec<(u16, Vec<u8>)>,
    /// The keys that are down, as far as the events say.
    down: Vec<u8>,
    /// Heyo's own strokes not heard back yet.
    own: VecDeque<Stroke>,
    /// The modifiers let go of for the piece under way,
    lifted: Vec<u8>,
    /// and those of them the user let go of meanwhile.
    let_go: Vec<u8>,
    /// Presses of a mouse button, and when the last was heard of.
    presses: u32,
    pressed: Option<Instant>,
    disturbed: Option<&'static str>,
}

impl Heard {
    pub(super) fn new(modifiers: Vec<(u8, u16)>, hotkeys: Vec<(u16, Vec<u8>)>, down: Vec<u8>) -> Heard {
        Heard { modifiers, hotkeys, down, ..Heard::default() }
    }

    fn modifier(&self, code: u8) -> bool {
        self.modifiers.iter().any(|(key, _)| *key == code)
    }

    /// The named modifiers that are down.
    fn holding(&self) -> u16 {
        self.modifiers.iter().filter(|(key, _)| self.down.contains(key)).fold(0, |all, (_, bit)| all | bit) & NAMED
    }

    fn disturb(&mut self, why: &'static str) {
        self.disturbed.get_or_insert(why);
    }

    /// A key went down or came up. `synthetic`: a program pressed it.
    pub(super) fn key(&mut self, code: u8, pressed: bool, synthetic: bool) {
        if synthetic && self.own.front() == Some(&(code, pressed)) {
            self.own.pop_front();
            return;
        }
        if !pressed {
            self.down.retain(|key| *key != code);
            if self.lifted.contains(&code) && !self.let_go.contains(&code) {
                self.let_go.push(code);
            }
            return;
        }
        // A key that is held repeats.
        if self.down.contains(&code) {
            return;
        }
        let holding = self.holding();
        self.down.push(code);
        let hotkey = self.hotkeys.iter().any(|(modifiers, keys)| *modifiers == holding && keys.contains(&code));
        if !self.modifier(code) && !hotkey {
            self.disturb("a key was pressed");
        }
    }

    /// A mouse button went down (4 to 7 are the wheel), or a finger.
    pub(super) fn button(&mut self, button: u32, now: Instant) {
        if !(4..=7).contains(&button) {
            self.presses += 1;
            self.pressed = Some(now);
        }
    }

    /// The window being written into lost the keyboard's focus. Losing it
    /// to a program that takes the keyboard for a while (`GRAB`) is not
    /// losing it: the focus is where it was when that program lets go.
    pub(super) fn unfocused(&mut self, mode: NotifyMode) {
        if !matches!(mode, NotifyMode::GRAB | NotifyMode::UNGRAB) {
            self.disturb("another window has the keyboard");
        }
    }

    /// Heyo is about to press these, having let go of `lifted` among them.
    pub(super) fn expect(&mut self, strokes: &[Stroke], lifted: &[u8]) {
        self.own.extend(strokes.iter().copied());
        self.lifted = lifted.to_vec();
        self.let_go.clear();
    }

    /// After a piece, with every event up to its end heard: the modifiers
    /// pressed again that the user had meanwhile let go of.
    pub(super) fn stuck(&mut self) -> Vec<u8> {
        self.lifted.clear();
        std::mem::take(&mut self.let_go)
    }

    /// Why nothing may be written at `now`, if so, `own_presses` of the
    /// presses of a mouse button having been on Heyo's own windows; and
    /// whether that may yet turn out otherwise (a press so recent that
    /// Heyo's word on it may still be on its way).
    pub(super) fn why(&self, own_presses: u32, now: Instant) -> Option<(&'static str, bool)> {
        if let Some(why) = self.disturbed {
            return Some((why, false));
        }
        let open = self.pressed.is_some_and(|at| now.saturating_duration_since(at) < PRESS_PATIENCE);
        (self.presses > own_presses).then_some(("a mouse button was pressed", open))
    }

    /// The same once Heyo's own keys should all have been heard back: any
    /// still missing went somewhere else.
    pub(super) fn why_after(&mut self, own_presses: u32, now: Instant) -> Option<&'static str> {
        if !self.own.is_empty() {
            self.own.clear();
            self.disturb("Heyo's own keys did not come back as pressed");
        }
        self.why(own_presses, now).map(|(why, _)| why)
    }
}

/// A hotkey as the desktop's settings write it (`<Control><Alt>d`): the
/// modifiers, and the key's symbol.
pub(super) fn shortcut(accelerator: &str) -> Option<(u16, u32)> {
    let (modifiers, key) = crate::platform::hotkeys::normalize(accelerator);
    let mut held = 0;
    for modifier in &modifiers {
        held |= match modifier.as_str() {
            "shift" => SHIFT,
            "control" => CONTROL,
            "alt" | "mod1" => ALT,
            "super" | "hyper" => SUPER,
            _ => return None,
        };
    }
    let symbol = xkbc::keysym_from_name(&key, xkbc::KEYSYM_CASE_INSENSITIVE).raw();
    (symbol != 0).then_some((held, symbol))
}

/// Writing into the window that had the focus when this began.
pub(super) struct Hand {
    conn: RustConnection,
    root: Window,
    focus: Window,
    /// A window that is never shown: see `taken`.
    unseen: Window,
    application: Client,
    keys: Keys,
    /// The devices programs press keys through.
    synthetic: Vec<u16>,
    /// The modifier keys that change what Backspace and paste do.
    liftable: Vec<u8>,
    /// The presses on Heyo's own windows, as their count stands and as it
    /// stood when this began.
    own_presses: Arc<AtomicU32>,
    own_before: u32,
    heard: Heard,
}

impl Hand {
    pub(super) fn begin(hotkeys: &[String], own_presses: Arc<AtomicU32>) -> Result<Hand, Box<dyn Error>> {
        let (conn, screen) = RustConnection::connect(None)?;
        let root = conn.setup().roots[screen].root;
        // From 2.1 on, raw events also arrive while another program has a
        // grab; 2.2 has those of a touch.
        conn.xinput_xi_query_version(2, 2)?.reply()?;
        conn.xtest_get_version(2, 2)?.reply()?;
        let focus = conn.get_input_focus()?.reply()?.focus;
        // None, or "wherever the pointer is".
        if focus <= 1 {
            return Err("no window has the keyboard".into());
        }
        let devices = conn.xinput_xi_query_device(xinput::Device::ALL)?.reply()?;
        let synthetic = devices.infos.iter().filter(|d| String::from_utf8_lossy(&d.name).contains("XTEST")).map(|d| d.deviceid).collect();
        let raw = xinput::EventMask {
            deviceid: xinput::Device::ALL_MASTER.into(),
            mask: vec![XIEventMask::RAW_KEY_PRESS | XIEventMask::RAW_KEY_RELEASE | XIEventMask::RAW_BUTTON_PRESS | XIEventMask::RAW_TOUCH_BEGIN],
        };
        conn.xinput_xi_select_events(root, &[raw])?.check()?;
        conn.change_window_attributes(focus, &ChangeWindowAttributesAux::new().event_mask(EventMask::FOCUS_CHANGE))?.check()?;
        let unseen = conn.generate_id()?;
        conn.create_window(0, unseen, root, -1, -1, 1, 1, 0, WindowClass::INPUT_ONLY, 0, &CreateWindowAux::new())?.check()?;

        let (mapping, min) = mapping(&conn)?;
        let keys = Keys::of(&mapping, min)?;
        let per = usize::from(mapping.keysyms_per_keycode).max(1);
        let symbols = |code: u8| mapping.keysyms.chunks(per).nth(usize::from(code - min)).unwrap_or(&[]);
        let codes = || min..=conn.setup().max_keycode;
        // The modifiers by the server's own table, and what else has a
        // modifier's symbol on it.
        let table = conn.get_modifier_mapping()?.reply()?;
        let each = (table.keycodes.len() / 8).max(1);
        let mut modifiers: Vec<(u8, u16)> = table.keycodes.chunks(each).enumerate().flat_map(|(index, keys)| keys.iter().filter(|key| **key != 0).map(move |key| (*key, 1u16 << index))).collect();
        modifiers.extend(codes().filter(|code| symbols(*code).first().is_some_and(|symbol| is_modifier(*symbol))).map(|code| (code, 0)));
        let liftable = modifiers.iter().filter(|(_, bit)| *bit & !(LOCK | NUM_LOCK) != 0).map(|(key, _)| *key).collect();
        let hotkeys = hotkeys
            .iter()
            .filter_map(|accelerator| shortcut(accelerator))
            .map(|(held, symbol)| (held, codes().filter(|code| symbols(*code).contains(&symbol)).collect()))
            .collect();
        let pressed = conn.query_keymap()?.reply()?.keys;
        let down = codes().filter(|code| pressed[usize::from(*code) / 8] & (1 << (code % 8)) != 0).collect();
        let own_before = own_presses.load(Ordering::SeqCst);
        Ok(Hand { application: Client::of(&conn, focus), conn, root, focus, unseen, keys, synthetic, liftable, own_presses, own_before, heard: Heard::new(modifiers, hotkeys, down) })
    }

    fn hear(&mut self, event: Event) {
        match event {
            Event::XinputRawKeyPress(e) | Event::XinputRawKeyRelease(e) => {
                if let Ok(code) = u8::try_from(e.detail) {
                    self.heard.key(code, e.event_type == u16::from(xinput::RAW_KEY_PRESS_EVENT), self.synthetic.contains(&e.sourceid));
                }
            }
            Event::XinputRawButtonPress(e) => self.heard.button(e.detail, Instant::now()),
            Event::XinputRawTouchBegin(_) => self.heard.button(1, Instant::now()),
            Event::FocusOut(e) if e.event == self.focus => self.heard.unfocused(e.mode),
            _ => {}
        }
    }

    /// How many presses were on Heyo's own windows since this began.
    fn own(&self) -> u32 {
        self.own_presses.load(Ordering::SeqCst).wrapping_sub(self.own_before)
    }

    /// Hears everything the server has sent so far.
    fn listen(&mut self) -> Result<(), Box<dyn Error>> {
        while let Some(event) = self.conn.poll_for_event()? {
            self.hear(event);
        }
        Ok(())
    }

    /// Asks the server where the focus is: by the time it answers, it has
    /// sent everything that happened before.
    fn look(&mut self) -> Result<(), Box<dyn Error>> {
        let focus = self.conn.get_input_focus()?.reply()?.focus;
        self.listen()?;
        if focus != self.focus {
            self.heard.disturb("another window has the keyboard");
        }
        Ok(())
    }

    /// Whether another program has taken the keyboard for itself, so that
    /// keys pressed now would go to it. The server says so to whoever asks
    /// for the keyboard; `unseen` is asked for, which cannot have it (a
    /// window must be on screen for that), so nothing is ever taken.
    fn taken(&self) -> Result<bool, Box<dyn Error>> {
        let status = self.conn.grab_keyboard(false, self.unseen, x11rb::CURRENT_TIME, GrabMode::ASYNC, GrabMode::ASYNC)?.reply()?.status;
        if status == GrabStatus::SUCCESS {
            self.conn.ungrab_keyboard(x11rb::CURRENT_TIME)?.check()?;
        }
        Ok(status == GrabStatus::ALREADY_GRABBED)
    }

    /// Whether a piece may be written now. Waits for a program that has
    /// taken the keyboard to let go of it, and puts the piece off if it
    /// does not in time; waits for word of whose window a mouse button was
    /// pressed on. Anything else that is in the way ends the writing.
    fn clear(&mut self) -> Result<(), Lost> {
        let since = Instant::now();
        loop {
            let taken = self.taken().map_err(|e| Lost::untouched(e.to_string()))?;
            self.look().map_err(|e| Lost::untouched(e.to_string()))?;
            match self.heard.why(self.own(), Instant::now()) {
                Some((why, false)) => return Err(Lost::untouched(why)),
                Some((_, true)) => {}
                None if !taken => return Ok(()),
                None if since.elapsed() > GRAB_PATIENCE => return Err(Lost::put_off("another program has the keyboard to itself")),
                None => {}
            }
            readable(&self.conn, Duration::from_millis(20));
        }
    }

    /// Presses Backspace `erase` times and then, if `paste`, the paste
    /// shortcut, between letting go of the modifiers that are held and
    /// pressing them again. Presses nothing, and says why, if the user did
    /// something to the text in the last moment (and whether that may yet
    /// turn out otherwise: see `Heard::why`).
    fn strike(&mut self, erase: usize, paste: bool) -> Result<Option<(&'static str, bool)>, Box<dyn Error>> {
        let pressed = self.conn.query_keymap()?.reply()?.keys;
        // The last look: with the server's answer came everything that
        // happened before it, and the keys go out next.
        self.listen()?;
        if let Some(why) = self.heard.why(self.own(), Instant::now()) {
            return Ok(Some(why));
        }
        let lifted: Vec<u8> = self.liftable.iter().copied().filter(|code| pressed[usize::from(*code) / 8] & (1 << (code % 8)) != 0).collect();
        let mut strokes: Vec<Stroke> = lifted.iter().map(|code| (*code, false)).collect();
        strokes.extend(self.keys.replacing(erase, paste, false));
        strokes.extend(lifted.iter().rev().map(|code| (*code, true)));
        self.heard.expect(&strokes, &lifted);
        press(&self.conn, self.root, &strokes)?;
        self.listen()?;
        let stuck: Vec<Stroke> = self.heard.stuck().into_iter().map(|code| (code, false)).collect();
        if !stuck.is_empty() {
            self.heard.expect(&stuck, &[]);
            press(&self.conn, self.root, &stuck)?;
            self.listen()?;
        }
        Ok(None)
    }
}

impl Writing for Hand {
    fn replace(&mut self, erase: usize, text: &str) -> Result<(), Lost> {
        self.clear()?;
        let offer = match text.is_empty() {
            true => None,
            false => Some(offer::put(text).map_err(|e| Lost::untouched(e.to_string()))?),
        };
        let mut erased = 0;
        loop {
            let now = (erase - erased).min(MOST_AT_ONCE);
            let last = erased + now == erase;
            if erased > 0 {
                // Half erased, the piece cannot be put off: it is lost.
                self.clear().map_err(|lost| Lost { erased, put: false, later: false, why: lost.why })?;
            }
            let paste = last && offer.is_some();
            let astray = match self.strike(now, paste) {
                // Nothing erased yet, the piece can still be put off.
                Ok(Some((why, open))) => return Err(Lost { erased, put: false, later: open && erased == 0, why: why.into() }),
                Ok(None) => self.heard.why_after(self.own(), Instant::now()).map(str::to_owned),
                // If the server did not answer, the keys may still have arrived.
                Err(e) => Some(e.to_string()),
            };
            erased += now;
            if let Some(why) = astray {
                self.heard.disturb("a piece went astray");
                return Err(Lost { erased, put: paste, later: false, why });
            }
            if last {
                break;
            }
        }
        if let Some(offer) = offer {
            if !offer.taken_by(self.application, patience(erase)) {
                self.heard.disturb("a piece was not taken");
                return Err(Lost { erased, put: true, later: false, why: "the application was not seen to take the text".into() });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Keys by their place on the keyboard (evdev codes, as X numbers them).
    const CTRL: u8 = 37;
    const ALT_L: u8 = 64;
    const SHIFT_L: u8 = 50;
    const CAPS: u8 = 66;
    const A: u8 = 38;
    const D: u8 = 40;
    const V: u8 = 55;
    const BACK: u8 = 22;
    const MENU: u8 = 135;
    const F9: u8 = 75;

    fn heard(down: &[u8]) -> Heard {
        let modifiers = vec![(SHIFT_L, SHIFT), (CAPS, LOCK), (CTRL, CONTROL), (ALT_L, ALT)];
        // Ctrl+Alt+D and F9.
        Heard::new(modifiers, vec![(CONTROL | ALT, vec![D]), (0, vec![F9])], down.to_vec())
    }

    /// Why nothing may be written, long after whatever happened, none of
    /// the presses of a mouse button having been on Heyo's own windows.
    fn why(heard: &Heard) -> Option<&'static str> {
        heard.why(0, Instant::now() + Duration::from_secs(60)).map(|(why, _)| why)
    }

    fn why_after(heard: &mut Heard) -> Option<&'static str> {
        heard.why_after(0, Instant::now() + Duration::from_secs(60))
    }

    fn press(heard: &mut Heard, keys: &[u8]) {
        for key in keys {
            heard.key(*key, true, false);
        }
    }

    fn release(heard: &mut Heard, keys: &[u8]) {
        for key in keys {
            heard.key(*key, false, false);
        }
    }

    #[test]
    fn a_key_the_user_presses_ends_the_writing() {
        for key in [A, BACK, V] {
            let mut heard = heard(&[]);
            assert_eq!(why(&heard), None);
            press(&mut heard, &[key]);
            assert_eq!(why(&heard), Some("a key was pressed"), "key {key}");
            // And nothing that happens after brings it back.
            release(&mut heard, &[key]);
            assert_eq!(heard.why(5, Instant::now()), Some(("a key was pressed", false)));
        }
        // A shortcut is a key too: Ctrl+A, Ctrl+Backspace.
        let mut shortcut = heard(&[]);
        press(&mut shortcut, &[CTRL, A]);
        assert!(why(&shortcut).is_some());
        // Typed through a program (a remote keyboard), it is typed all the same.
        let mut remote = heard(&[]);
        remote.key(A, true, true);
        assert!(why(&remote).is_some());
    }

    #[test]
    fn modifiers_and_keys_that_were_down_already_are_nothing() {
        let mut heard = heard(&[]);
        press(&mut heard, &[SHIFT_L, CTRL, ALT_L, CAPS]);
        release(&mut heard, &[SHIFT_L, CTRL, ALT_L, CAPS]);
        assert_eq!(why(&heard), None);
        // The key held to talk was down when writing began, and repeats.
        let mut talking = heard_with_menu_down();
        press(&mut talking, &[MENU, MENU, MENU]);
        assert_eq!(why(&talking), None);
        // Let go and pressed again, it is a key like any other.
        release(&mut talking, &[MENU]);
        press(&mut talking, &[MENU]);
        assert!(why(&talking).is_some());
    }

    fn heard_with_menu_down() -> Heard {
        heard(&[MENU])
    }

    #[test]
    fn heyos_hotkey_is_not_typing() {
        let mut heard = heard(&[]);
        press(&mut heard, &[CTRL, ALT_L, D]);
        release(&mut heard, &[D, ALT_L, CTRL]);
        press(&mut heard, &[F9]);
        assert_eq!(why(&heard), None);
        // The same key with other modifiers is not the hotkey.
        for held in [&[CTRL][..], &[CTRL, ALT_L, SHIFT_L], &[]] {
            let mut other = self::heard(&[]);
            press(&mut other, held);
            press(&mut other, &[D]);
            assert!(why(&other).is_some(), "{held:?}");
        }
        let mut shifted = self::heard(&[]);
        press(&mut shifted, &[SHIFT_L, F9]);
        assert!(why(&shifted).is_some());
        // Caps Lock down, or on, makes no difference to a hotkey.
        let mut locked = self::heard(&[]);
        press(&mut locked, &[CAPS, CTRL, ALT_L, D]);
        assert_eq!(why(&locked), None);
    }

    #[test]
    fn heyos_own_keys_are_known_by_coming_back_in_order() {
        let mut heard = heard(&[]);
        let strokes = [(BACK, true), (BACK, false), (CTRL, true), (V, true), (V, false), (CTRL, false)];
        heard.expect(&strokes, &[]);
        for (code, pressed) in strokes {
            heard.key(code, pressed, true);
        }
        assert_eq!(why_after(&mut heard), None);
        // The same keys from a keyboard are the user's.
        let mut typed = self::heard(&[]);
        typed.expect(&strokes, &[]);
        typed.key(BACK, true, false);
        assert!(why(&typed).is_some());
        // A program's key in between Heyo's is not Heyo's.
        let mut between = self::heard(&[]);
        between.expect(&strokes, &[]);
        between.key(BACK, true, true);
        between.key(A, true, true);
        assert!(why(&between).is_some());
        // Keys that never came back went somewhere else.
        let mut lost = self::heard(&[]);
        lost.expect(&strokes, &[]);
        lost.key(BACK, true, true);
        assert_eq!(why(&lost), None);
        assert_eq!(why_after(&mut lost), Some("Heyo's own keys did not come back as pressed"));
    }

    #[test]
    fn a_click_ends_the_writing_unless_it_was_on_heyo() {
        let mut heard = heard(&[]);
        let t = Instant::now();
        let later = t + PRESS_PATIENCE;
        // The wheel moves no cursor.
        for wheel in 4..=7 {
            heard.button(wheel, t);
        }
        assert_eq!(heard.why(0, later), None);
        heard.button(1, t);
        // For a moment Heyo may still say the press was on its own window:
        // nothing is written meanwhile, and nothing is over.
        assert_eq!(heard.why(0, t), Some(("a mouse button was pressed", true)));
        assert_eq!(heard.why(0, later - Duration::from_millis(1)), Some(("a mouse button was pressed", true)));
        assert_eq!(heard.why(0, later), Some(("a mouse button was pressed", false)));
        // It was the dictation window's own button.
        assert_eq!(heard.why(1, t), None);
        assert_eq!(heard.why(1, later), None);
        // One of three was not.
        heard.button(1, later);
        heard.button(3, later);
        assert!(heard.why(2, later + PRESS_PATIENCE).is_some());
        assert_eq!(heard.why(3, later), None);
        // A key pressed is never anything else.
        press(&mut heard, &[A]);
        assert_eq!(heard.why(3, later), Some(("a key was pressed", false)));
    }

    #[test]
    fn the_focus_leaving_ends_it_and_a_program_taking_the_keyboard_does_not() {
        let mut heard = heard(&[]);
        // The desktop takes the keyboard while its hotkey is held, and
        // gives it back.
        press(&mut heard, &[CTRL, ALT_L, D]);
        heard.unfocused(NotifyMode::GRAB);
        heard.unfocused(NotifyMode::UNGRAB);
        assert_eq!(why(&heard), None);
        // Another window comes to the front.
        heard.unfocused(NotifyMode::NORMAL);
        assert_eq!(why(&heard), Some("another window has the keyboard"));
        let mut during = self::heard(&[]);
        during.unfocused(NotifyMode::GRAB);
        during.unfocused(NotifyMode::WHILE_GRABBED);
        assert!(why(&during).is_some());
    }

    #[test]
    fn a_modifier_let_go_of_while_it_was_lifted_is_let_go_again() {
        // Ctrl is held (to talk); the piece lets go of it and presses it again.
        let mut heard = heard(&[CTRL]);
        let strokes = [(CTRL, false), (BACK, true), (BACK, false), (CTRL, true)];
        heard.expect(&strokes, &[CTRL]);
        for (code, pressed) in strokes {
            heard.key(code, pressed, true);
        }
        assert!(heard.stuck().is_empty());
        assert_eq!(why_after(&mut heard), None);
        // The user lets go in the middle: Heyo's pressing-again would stay.
        heard.expect(&strokes, &[CTRL]);
        heard.key(CTRL, false, true);
        heard.key(CTRL, false, false);
        for (code, pressed) in &strokes[1..] {
            heard.key(*code, *pressed, true);
        }
        assert_eq!(heard.stuck(), [CTRL]);
        assert_eq!(why_after(&mut heard), None);
        // Nothing is owed after that.
        assert!(heard.stuck().is_empty());
    }

    #[test]
    fn a_hotkey_is_read_as_the_desktop_writes_it() {
        assert_eq!(shortcut("<Control><Alt>d"), Some((CONTROL | ALT, 0x64)));
        assert_eq!(shortcut("<Primary><Shift>space"), Some((CONTROL | SHIFT, 0x20)));
        assert_eq!(shortcut("<Super>period"), Some((SUPER, 0x2e)));
        assert_eq!(shortcut("F9"), Some((0, 0xffc6)));
        assert_eq!(shortcut("<Alt>F9"), Some((ALT, 0xffc6)));
        // Not a key, or a modifier this does not know: no hotkey is made of it.
        assert_eq!(shortcut("<Control>"), None);
        assert_eq!(shortcut("<Mod5>d"), None);
        assert_eq!(shortcut(""), None);
    }

    /// Against a real X server: `DISPLAY=:N cargo test -- --ignored on_an_x_server`,
    /// with a server of one's own (`Xvfb :N`). Never the desktop in use:
    /// this presses keys.
    ///
    /// A window of the test's own stands in for an application (`field`).
    #[test]
    #[ignore]
    fn on_an_x_server_pieces_are_written_until_something_else_touches_the_text() {
        use super::super::field::Field;
        use x11rb::protocol::xproto::{CreateWindowAux, GrabMode, InputFocus, ModMask, WindowClass, KEY_PRESS_EVENT, KEY_RELEASE_EVENT};

        let (conn, screen) = RustConnection::connect(None).unwrap();
        let root = conn.setup().roots[screen].root;
        let key = |code: u8, down: bool| {
            conn.xtest_fake_input(if down { KEY_PRESS_EVENT } else { KEY_RELEASE_EVENT }, code, x11rb::CURRENT_TIME, root, 0, 0, 0).unwrap();
            conn.get_input_focus().unwrap().reply().unwrap();
        };
        let all_up = || conn.query_keymap().unwrap().reply().unwrap().keys.iter().all(|keys| *keys == 0);
        let untouched = |lost: Lost| assert!(lost.erased == 0 && !lost.put && !lost.later, "{lost:?}");

        // Pieces go in and are corrected where they stand, in any script.
        let field = Field::open();
        let mut hand = Hand::begin(&[], Arc::default()).unwrap();
        hand.replace(0, "The weather").unwrap();
        assert_eq!(field.text(), "The weather");
        hand.replace(0, " is nise").unwrap();
        hand.replace(2, "ce today").unwrap();
        assert_eq!(field.text(), "The weather is nice today");
        hand.replace(5, "今天😀。").unwrap();
        assert_eq!(field.text(), "The weather is nice 今天😀。");
        // More than one handing-over of Backspaces, and nothing put back.
        let long = "x".repeat(150);
        hand.replace(4, &long).unwrap();
        hand.replace(150, "").unwrap();
        assert_eq!(field.text(), "The weather is nice ");
        assert!(all_up());

        // A modifier held (the key held to talk) is let go for each piece
        // and is down again after it.
        key(CTRL, true);
        hand.replace(1, ", yes").unwrap();
        assert_eq!(field.text(), "The weather is nice, yes");
        assert!(!all_up());
        key(CTRL, false);
        assert!(all_up());
        hand.replace(0, ".").unwrap();

        // The user types: that piece is not written, nor any after it.
        key(A, true);
        key(A, false);
        untouched(hand.replace(2, "no").unwrap_err());
        untouched(hand.replace(0, "more").unwrap_err());
        assert_eq!(field.text(), "The weather is nice, yes.a");
        drop(hand);

        // A click, unless it was on a window of Heyo's own.
        let click = || {
            conn.xtest_fake_input(x11rb::protocol::xproto::BUTTON_PRESS_EVENT, 1, x11rb::CURRENT_TIME, root, 0, 0, 0).unwrap();
            conn.xtest_fake_input(x11rb::protocol::xproto::BUTTON_RELEASE_EVENT, 1, x11rb::CURRENT_TIME, root, 0, 0, 0).unwrap();
            conn.get_input_focus().unwrap().reply().unwrap();
        };
        conn.set_input_focus(InputFocus::POINTER_ROOT, field.window, x11rb::CURRENT_TIME).unwrap();
        let own_presses = Arc::new(AtomicU32::new(7));
        let mut hand = Hand::begin(&[], own_presses.clone()).unwrap();
        click();
        // Heyo's word on the press comes a moment after the press: the
        // piece waits for it.
        let saying = {
            let own_presses = own_presses.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(100));
                own_presses.fetch_add(1, Ordering::SeqCst);
            })
        };
        hand.replace(0, " So").unwrap();
        saying.join().unwrap();
        // No word comes of this one: it was the user's, in the application.
        click();
        let before = Instant::now();
        untouched(hand.replace(0, "!").unwrap_err());
        assert!(before.elapsed() >= PRESS_PATIENCE - Duration::from_millis(50));
        drop(hand);

        // A hotkey of Heyo's is the desktop's: while it is held the desktop
        // has the keyboard, and the piece waits for it to be let go.
        let desktop = RustConnection::connect(None).unwrap().0;
        desktop.grab_key(false, root, ModMask::CONTROL | ModMask::M1, D, GrabMode::ASYNC, GrabMode::ASYNC).unwrap().check().unwrap();
        conn.set_input_focus(InputFocus::POINTER_ROOT, field.window, x11rb::CURRENT_TIME).unwrap();
        let mut hand = Hand::begin(&["<Control><Alt>d".to_owned()], Arc::default()).unwrap();
        for code in [CTRL, ALT_L, D] {
            key(code, true);
        }
        let letting_go = std::thread::spawn(|| {
            std::thread::sleep(Duration::from_millis(150));
            let (conn, screen) = RustConnection::connect(None).unwrap();
            let root = conn.setup().roots[screen].root;
            for code in [D, ALT_L, CTRL] {
                conn.xtest_fake_input(KEY_RELEASE_EVENT, code, x11rb::CURRENT_TIME, root, 0, 0, 0).unwrap();
            }
            conn.get_input_focus().unwrap().reply().unwrap();
        });
        let before = Instant::now();
        hand.replace(3, " Yes.").unwrap();
        assert!(before.elapsed() >= Duration::from_millis(100), "written while the keyboard was the desktop's");
        letting_go.join().unwrap();
        assert_eq!(field.text(), "The weather is nice, yes.a Yes.");
        // A program that had the keyboard before writing began, and keeps
        // it (a prompt for a password): nothing is pressed into it.
        let prompt = RustConnection::connect(None).unwrap().0;
        assert_eq!(prompt.grab_keyboard(false, field.window, x11rb::CURRENT_TIME, GrabMode::ASYNC, GrabMode::ASYNC).unwrap().reply().unwrap().status, x11rb::protocol::xproto::GrabStatus::SUCCESS);
        let mut held = Hand::begin(&[], Arc::default()).unwrap();
        let before = Instant::now();
        let lost = held.replace(0, "secret").unwrap_err();
        assert!(lost.later && lost.erased == 0 && !lost.put, "{lost:?}");
        assert!(before.elapsed() >= GRAB_PATIENCE);
        // Once the keyboard is free again the next piece goes in.
        drop(prompt);
        held.replace(1, ".").unwrap();
        assert_eq!(field.text(), "The weather is nice, yes.a Yes.");
        // Asking whether the keyboard was taken took nothing.
        assert!(!Hand::begin(&[], Arc::default()).unwrap().taken().unwrap());
        // The same keys when they are nobody's hotkey are typing.
        let mut unknowing = Hand::begin(&[], Arc::default()).unwrap();
        for code in [CTRL, ALT_L, D] {
            key(code, true);
        }
        for code in [D, ALT_L, CTRL] {
            key(code, false);
        }
        untouched(unknowing.replace(0, "?").unwrap_err());
        drop(desktop);

        // Another window takes the focus.
        let mut hand = Hand::begin(&[], Arc::default()).unwrap();
        let other = Field::open();
        untouched(hand.replace(0, "?").unwrap_err());
        assert_eq!(other.text(), "");
        drop(other);

        // An application that does not paste at Ctrl+V takes nothing: the
        // piece is lost, and said to be.
        let deaf = conn.generate_id().unwrap();
        conn.create_window(0, deaf, root, 0, 0, 10, 10, 0, WindowClass::INPUT_OUTPUT, 0, &CreateWindowAux::new()).unwrap();
        conn.map_window(deaf).unwrap();
        conn.set_input_focus(InputFocus::POINTER_ROOT, deaf, x11rb::CURRENT_TIME).unwrap();
        conn.get_input_focus().unwrap().reply().unwrap();
        let mut hand = Hand::begin(&[], Arc::default()).unwrap();
        let lost = hand.replace(0, "anyone?").unwrap_err();
        assert!(lost.put && lost.erased == 0, "{lost:?}");
        untouched(hand.replace(0, "again").unwrap_err());
        assert!(all_up());
        assert_eq!(field.text(), "The weather is nice, yes.a Yes.");
    }
}

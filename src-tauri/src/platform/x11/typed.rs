//! Following what is typed, through the X Input extension's raw key events
//! (as `hold_key` does for one key), and replacing it: Backspaces and the
//! paste shortcut pressed with XTEST, the text offered on the clipboard
//! (`offer`). Writing piece after piece into one window is `writing`.
//!
//! Raw events are a copy of what the keyboard does, sent to whoever asks.
//! Nothing is grabbed and no key is held back or changed: if this code
//! stops, hangs or dies, the X server merely has one listener fewer.
//!
//! A raw event names a key, not a character. Which character the key gives
//! depends on the layout, on Shift, Caps Lock and AltGr, and that is worked
//! out here with xkbcommon from the layout's names (the root window's
//! `_XKB_RULES_NAMES`), the keys that are down, and what the server says is
//! locked (Caps Lock, Num Lock, which of several layouts).
//!
//! What this cannot know, and so reports as `Unknown` or not at all:
//! - what a dead key or Compose makes of the keys after it;
//! - keys latched rather than held (sticky keys);
//! - a keymap changed with `xmodmap`, which the layout's names do not show;
//! - what an input method makes of the keys: see `input_method`, asked
//!   separately, before anything is replaced.

use std::error::Error;
use std::os::fd::AsRawFd;
use std::time::Duration;

use x11rb::connection::Connection;
use x11rb::protocol::xinput::{self, ConnectionExt as _, XIEventMask};
use x11rb::protocol::xkb::{self, ConnectionExt as _};
use x11rb::protocol::xproto::{AtomEnum, ChangeWindowAttributesAux, ConnectionExt as _, EventMask, KeyButMask, Window, KEY_PRESS_EVENT, KEY_RELEASE_EVENT};
use x11rb::protocol::xtest::ConnectionExt as _;
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;
use xkbcommon::xkb as xkbc;

use super::offer::{self, Client};
use crate::platform::typed::{Typed, Typing, Writing};

/// The keyboard of the X session.
pub struct Keyboard;

impl Typing for Keyboard {
    fn watch(&self, wanted: &dyn Fn() -> bool, on: &mut dyn FnMut(Typed)) -> Result<(), String> {
        watch(wanted, on).map_err(|e| e.to_string())
    }

    fn replace(&self, erase: usize, text: &str, terminal: bool) -> Result<(), String> {
        replace(erase, text, terminal).map_err(|e| e.to_string())
    }

    fn write(&self, hotkeys: &[String], own_presses: std::sync::Arc<std::sync::atomic::AtomicU32>) -> Result<Box<dyn Writing>, String> {
        super::writing::Hand::begin(hotkeys, own_presses).map(|hand| Box::new(hand) as Box<dyn Writing>).map_err(|e| e.to_string())
    }

    fn composing(&self) -> bool {
        super::input_method::composing()
    }

    fn modifiers_held(&self) -> bool {
        modifiers_held().unwrap_or(true)
    }
}

pub(super) const BACKSPACE: u32 = 0xff08;
const MULTI_KEY: u32 = 0xff20;

/// What a key press amounts to.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Key {
    Typed(Typed),
    /// A dead key: the next character will be another one.
    Dead,
    /// Compose: the next characters make one that is not theirs.
    Compose,
    /// A modifier or a lock by itself: nothing happened to the text.
    Nothing,
}

/// Keysyms of keys that only change what other keys do.
pub(super) fn is_modifier(sym: u32) -> bool {
    // Shift_L … Hyper_R; ISO_Lock … ISO_Level5_Lock; Mode_switch, Num_Lock, Scroll_Lock.
    matches!(sym, 0xffe1..=0xffee | 0xfe01..=0xfe13 | 0xff7e | 0xff7f | 0xff14)
}

/// Reads a key press: its keysym, the text the layout gives for it, whether
/// Ctrl, Alt or Super is held (a shortcut, whatever the key), and whether a
/// modifier is latched (then the text is not to be trusted).
pub(super) fn classify(sym: u32, text: &str, shortcut: bool, latched: bool) -> Key {
    if is_modifier(sym) {
        return Key::Nothing;
    }
    if shortcut {
        return Key::Typed(Typed::Break);
    }
    match sym {
        BACKSPACE => Key::Typed(Typed::Backspace),
        // dead_grave … dead_longsolidusoverlay.
        0xfe50..=0xfe93 => Key::Dead,
        MULTI_KEY => Key::Compose,
        _ => {
            let mut chars = text.chars();
            match (chars.next(), chars.next()) {
                // Enter, Tab, Escape and Delete come as control characters.
                (Some(c), _) if c.is_control() => Key::Typed(Typed::Break),
                (Some(_), _) if latched => Key::Typed(Typed::Unknown),
                (Some(c), None) => Key::Typed(Typed::Char(c)),
                (Some(_), Some(_)) => Key::Typed(Typed::Unknown),
                // Arrows, Home, function keys, keys with nothing on them.
                (None, _) => Key::Typed(Typed::Break),
            }
        }
    }
}

/// The names a keyboard layout was built from, as X servers publish them.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Names {
    pub rules: String,
    pub model: String,
    pub layout: String,
    pub variant: String,
    pub options: String,
}

impl Names {
    /// From the `_XKB_RULES_NAMES` property: five strings, each ended by a zero.
    pub(super) fn parse(property: &[u8]) -> Option<Names> {
        let mut parts = property.split(|byte| *byte == 0).map(|part| String::from_utf8_lossy(part).into_owned());
        let names = Names { rules: parts.next()?, model: parts.next()?, layout: parts.next()?, variant: parts.next()?, options: parts.next()? };
        (!names.layout.is_empty()).then_some(names)
    }
}

/// A keyboard layout: which character each key gives.
pub(super) struct Layout {
    keymap: xkbc::Keymap,
    /// Ctrl, Alt and Super, as bits of a modifier mask.
    shortcut: xkbc::ModMask,
}

/// What a key gives, the keyboard being as it is.
pub(super) struct Pressed {
    pub sym: u32,
    pub text: String,
    pub shortcut: bool,
}

impl Layout {
    pub(super) fn of(names: &Names) -> Result<Layout, String> {
        // The names say everything: the environment is not to fill in gaps.
        let context = xkbc::Context::new(xkbc::CONTEXT_NO_ENVIRONMENT_NAMES);
        let keymap = xkbc::Keymap::new_from_names(&context, &names.rules, &names.model, &names.layout, &names.variant, Some(names.options.clone()), xkbc::KEYMAP_COMPILE_NO_FLAGS)
            .ok_or_else(|| format!("the keyboard layout {} is not known to xkbcommon", names.layout))?;
        let bit = |name: &str| match keymap.mod_get_index(name) {
            xkbc::MOD_INVALID => 0,
            index => 1 << index,
        };
        let shortcut = bit(xkbc::MOD_NAME_CTRL) | bit(xkbc::MOD_NAME_ALT) | bit(xkbc::MOD_NAME_LOGO);
        Ok(Layout { keymap, shortcut })
    }

    /// What the key `code` gives while the keys `held` are down (in the
    /// order they went down), with the modifiers `locked` and the layout
    /// number `group` in use, both as the X server reports them.
    pub(super) fn pressed(&self, code: u8, held: &[u8], locked: u32, group: u32) -> Pressed {
        // Which modifiers the held keys amount to. Only that is taken from
        // this state: a Caps Lock that is down would toggle the lock in it
        // a second time.
        let mut holding = xkbc::State::new(&self.keymap);
        holding.update_mask(0, 0, locked, 0, 0, group);
        for key in held.iter().filter(|key| **key != code) {
            holding.update_key(xkbc::Keycode::new(u32::from(*key)), xkbc::KeyDirection::Down);
        }
        let depressed = holding.serialize_mods(xkbc::STATE_MODS_DEPRESSED);
        let mut state = xkbc::State::new(&self.keymap);
        state.update_mask(depressed, 0, locked, 0, 0, group);
        let key = xkbc::Keycode::new(u32::from(code));
        Pressed {
            sym: state.key_get_one_sym(key).raw(),
            text: state.key_get_utf8(key),
            shortcut: state.serialize_mods(xkbc::STATE_MODS_EFFECTIVE) & self.shortcut != 0,
        }
    }
}

/// Waits until the server has sent something, or for `timeout`. Whether it has.
pub(super) fn readable(conn: &RustConnection, timeout: Duration) -> bool {
    let mut fd = libc::pollfd { fd: conn.stream().as_raw_fd(), events: libc::POLLIN, revents: 0 };
    // An error (a signal, say) is as good as "look again".
    unsafe { libc::poll(&mut fd, 1, timeout.as_millis() as libc::c_int) != 0 }
}

fn names(conn: &RustConnection, root: Window, property: u32) -> Result<Names, Box<dyn Error>> {
    let value = conn.get_property(false, root, property, AtomEnum::STRING, 0, 1024)?.reply()?.value;
    Names::parse(&value).ok_or_else(|| "the X server does not say which keyboard layout is in use".into())
}

fn watch(wanted: &dyn Fn() -> bool, on: &mut dyn FnMut(Typed)) -> Result<(), Box<dyn Error>> {
    let (conn, screen) = RustConnection::connect(None)?;
    let root = conn.setup().roots[screen].root;
    // From 2.1 on, raw events also arrive while another program has a grab.
    conn.xinput_xi_query_version(2, 2)?.reply()?;
    conn.xkb_use_extension(1, 0)?.reply()?;
    // Keys "pressed" by programs (Heyo's own Backspaces and paste among
    // them) come from the XTEST devices, and are not typing.
    let devices = conn.xinput_xi_query_device(xinput::Device::ALL)?.reply()?;
    let mut synthetic: Vec<u16> = devices.infos.iter().filter(|d| String::from_utf8_lossy(&d.name).contains("XTEST")).map(|d| d.deviceid).collect();
    // Tests have nothing but synthetic keys to press.
    if cfg!(debug_assertions) && std::env::var_os("HEYO_TYPED_SYNTHETIC").is_some() {
        synthetic.clear();
    }
    let active_window = conn.intern_atom(false, b"_NET_ACTIVE_WINDOW")?.reply()?.atom;
    let rules_names = conn.intern_atom(false, b"_XKB_RULES_NAMES")?.reply()?.atom;
    // The root window's properties say when another window comes to the
    // front and when the layout changes.
    conn.change_window_attributes(root, &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE))?;
    let mask = xinput::EventMask {
        deviceid: xinput::Device::ALL_MASTER.into(),
        mask: vec![XIEventMask::RAW_KEY_PRESS | XIEventMask::RAW_KEY_RELEASE | XIEventMask::RAW_BUTTON_PRESS],
    };
    conn.xinput_xi_select_events(root, &[mask])?;
    conn.flush()?;

    let mut layout = Layout::of(&names(&conn, root, rules_names)?)?;
    let front = |conn: &RustConnection| -> Option<u32> { conn.get_property(false, root, active_window, AtomEnum::ANY, 0, 1).ok()?.reply().ok()?.value32()?.next() };
    let mut in_front = front(&conn);
    // The keys that are down, in the order they went down.
    let mut held: Vec<u8> = Vec::new();
    // Characters still to come that a dead key or Compose will change.
    let mut changed = 0u8;

    while wanted() {
        while let Some(event) = conn.poll_for_event()? {
            match event {
                Event::XinputRawKeyPress(e) if !synthetic.contains(&e.sourceid) => {
                    let Ok(code) = u8::try_from(e.detail) else { continue };
                    let keyboard = conn.xkb_get_state(xkb::ID::USE_CORE_KBD.into())?.reply()?;
                    let pressed = layout.pressed(code, &held, u32::from(u16::from(keyboard.locked_mods)), u32::from(u8::from(keyboard.group)));
                    let latched = u16::from(keyboard.latched_mods) != 0 || keyboard.latched_group != 0;
                    // A key that repeats is in the list once.
                    if !held.contains(&code) {
                        held.push(code);
                    }
                    match classify(pressed.sym, &pressed.text, pressed.shortcut, latched) {
                        Key::Nothing => {}
                        Key::Dead => changed = changed.max(1),
                        Key::Compose => changed = 2,
                        Key::Typed(Typed::Char(_) | Typed::Unknown) if changed > 0 => {
                            changed -= 1;
                            on(Typed::Unknown);
                        }
                        // What Backspace does in the middle of composing
                        // depends on the application.
                        Key::Typed(Typed::Backspace) if changed > 0 => {
                            changed = 0;
                            on(Typed::Break);
                        }
                        Key::Typed(key) => {
                            if key == Typed::Break {
                                changed = 0;
                            }
                            on(key);
                        }
                    }
                }
                Event::XinputRawKeyRelease(e) if !synthetic.contains(&e.sourceid) => held.retain(|code| u32::from(*code) != e.detail),
                // A click puts the cursor somewhere else, whoever clicked.
                Event::XinputRawButtonPress(_) => {
                    changed = 0;
                    on(Typed::Break);
                }
                Event::PropertyNotify(e) if e.atom == active_window => {
                    let now = front(&conn);
                    if now != in_front {
                        in_front = now;
                        changed = 0;
                        on(Typed::Break);
                    }
                }
                Event::PropertyNotify(e) if e.atom == rules_names => {
                    layout = Layout::of(&names(&conn, root, rules_names)?)?;
                    changed = 0;
                    on(Typed::Break);
                }
                _ => {}
            }
        }
        if !readable(&conn, Duration::from_millis(500)) {
            // Quiet: a release that went unseen must not count as held for
            // good. The server knows which keys are down.
            let down = conn.query_keymap()?.reply()?.keys;
            held.retain(|code| down[usize::from(*code) / 8] & (1 << (code % 8)) != 0);
        }
    }
    Ok(())
}

/// The key that has `keysym` on it, by itself or with Shift, in any of the
/// layouts: where one of them is not Latin, shortcuts still go by the Latin one.
fn keycode(mapping: &x11rb::protocol::xproto::GetKeyboardMappingReply, min: u8, keysym: u32) -> Option<u8> {
    let per = usize::from(mapping.keysyms_per_keycode).max(1);
    let keys = || mapping.keysyms.chunks(per).enumerate();
    // The unshifted symbol of the first layout is what names a key, if any does.
    let key = keys().find(|(_, symbols)| symbols.first() == Some(&keysym)).or_else(|| keys().find(|(_, symbols)| symbols.contains(&keysym)));
    key.and_then(|(index, _)| u8::try_from(usize::from(min) + index).ok())
}

/// The most Backspaces handed over in one piece: with the paste shortcut
/// after them they are a few kilobytes, which reach the server in a single
/// write. Either all of it is pressed and let go, or none of it.
pub(super) const MOST_AT_ONCE: usize = 64;

/// How long the application in front is given to take the text, once the
/// keys are pressed: it comes to the paste shortcut after the Backspaces.
pub(super) fn patience(erased: usize) -> Duration {
    Duration::from_millis(1000 + 3 * erased as u64)
}

/// A key going down (`true`) or coming up.
pub(super) type Stroke = (u8, bool);

/// The keys Heyo presses, as this keyboard has them.
pub(super) struct Keys {
    pub backspace: u8,
    pub control: u8,
    pub shift: u8,
    pub v: u8,
}

impl Keys {
    pub(super) fn of(mapping: &x11rb::protocol::xproto::GetKeyboardMappingReply, min: u8) -> Result<Keys, String> {
        const CONTROL_L: u32 = 0xffe3;
        const SHIFT_L: u32 = 0xffe1;
        const V: u32 = 0x76;
        let key = |keysym: u32, name: &str| keycode(mapping, min, keysym).ok_or_else(|| format!("this keyboard layout has no {name} key"));
        Ok(Keys { backspace: key(BACKSPACE, "Backspace")?, control: key(CONTROL_L, "Ctrl")?, shift: key(SHIFT_L, "Shift")?, v: key(V, "V")? })
    }

    /// Backspace `erase` times, and then the paste shortcut if `paste`
    /// (a terminal's, if `terminal`).
    pub(super) fn replacing(&self, erase: usize, paste: bool, terminal: bool) -> Vec<Stroke> {
        let mut strokes: Vec<Stroke> = (0..erase).flat_map(|_| [(self.backspace, true), (self.backspace, false)]).collect();
        if paste {
            let shortcut: &[u8] = if terminal { &[self.control, self.shift, self.v] } else { &[self.control, self.v] };
            strokes.extend(shortcut.iter().map(|code| (*code, true)));
            strokes.extend(shortcut.iter().rev().map(|code| (*code, false)));
        }
        strokes
    }
}

/// The keyboard as the server maps it, for finding keys by what is on them.
pub(super) fn mapping(conn: &RustConnection) -> Result<(x11rb::protocol::xproto::GetKeyboardMappingReply, u8), Box<dyn Error>> {
    let (min, max) = (conn.setup().min_keycode, conn.setup().max_keycode);
    Ok((conn.get_keyboard_mapping(min, max - min + 1)?.reply()?, min))
}

/// Presses and lets go of keys in the focused window, as a program's key
/// presses (XTEST).
///
/// All of it goes to the server in one piece. Sent key by key with pauses
/// (as xdotool does), a key the user presses meanwhile would land between
/// the Backspaces and the paste, or be pressed "with Ctrl" and become a
/// shortcut: Ctrl+W, say. The server may still look at the keyboard between
/// two of these requests; that leaves microseconds, not hundredths of a
/// second. And should Heyo stop while this runs, the server has either all
/// of it or none: no key is left down.
pub(super) fn press(conn: &RustConnection, root: Window, strokes: &[Stroke]) -> Result<(), Box<dyn Error>> {
    for (code, down) in strokes {
        conn.xtest_fake_input(if *down { KEY_PRESS_EVENT } else { KEY_RELEASE_EVENT }, *code, x11rb::CURRENT_TIME, root, 0, 0, 0)?;
    }
    // Sends it all; an answer to anything means the server has taken it.
    conn.get_input_focus()?.reply()?;
    Ok(())
}

/// Replaces the `erase` characters before the cursor of the focused window
/// with `text`: Backspaces and the paste shortcut, which `watch` does not
/// report, with the text on the clipboard for the application to take.
fn replace(erase: usize, text: &str, terminal: bool) -> Result<(), Box<dyn Error>> {
    if erase > MOST_AT_ONCE {
        return Err("too much to erase".into());
    }
    let (conn, screen) = RustConnection::connect(None)?;
    let root = conn.setup().roots[screen].root;
    conn.xtest_get_version(2, 2)?.reply()?;
    let (mapping, min) = mapping(&conn)?;
    let keys = Keys::of(&mapping, min)?;
    let application = Client::of(&conn, conn.get_input_focus()?.reply()?.focus);
    let offer = offer::put(text)?;
    // The last look before anything is pressed.
    if held(&conn, root)? {
        return Err("a modifier key is down".into());
    }
    press(&conn, root, &keys.replacing(erase, true, terminal))?;
    if !offer.taken_by(application, patience(erase)) {
        return Err("the keys were pressed, but the application was not seen to take the text".into());
    }
    Ok(())
}

/// Whether Shift, Ctrl, Alt, Super or AltGr is down.
fn held(conn: &RustConnection, root: Window) -> Result<bool, Box<dyn Error>> {
    let mask = conn.query_pointer(root)?.reply()?.mask;
    // Not the locks (Caps Lock, Num Lock): they do nothing to Backspace.
    Ok(mask.intersects(KeyButMask::SHIFT | KeyButMask::CONTROL | KeyButMask::MOD1 | KeyButMask::MOD4 | KeyButMask::MOD5))
}

fn modifiers_held() -> Result<bool, Box<dyn Error>> {
    let (conn, screen) = RustConnection::connect(None)?;
    held(&conn, conn.setup().roots[screen].root)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Keys by their place on the keyboard (evdev codes, as X numbers them).
    const SHIFT: u8 = 50;
    const CONTROL: u8 = 37;
    const ALT: u8 = 64;
    const SUPER: u8 = 133;
    const ALTGR: u8 = 108;
    const CAPS: u8 = 66;
    const A: u8 = 38;
    const S: u8 = 39;
    const Q: u8 = 24;
    const E: u8 = 26;
    const Y: u8 = 29;
    const TWO: u8 = 11;
    const SEMICOLON: u8 = 47;
    const APOSTROPHE: u8 = 48;
    const EQUAL: u8 = 21;
    const SPACE: u8 = 65;
    const RETURN: u8 = 36;
    const TAB: u8 = 23;
    const ESCAPE: u8 = 9;
    const BACK: u8 = 22;
    const DELETE: u8 = 119;
    const LEFT: u8 = 113;
    const F5: u8 = 71;
    const KP_1: u8 = 87;
    /// The Lock and Num Lock modifiers, as the server reports them locked.
    const LOCK: u32 = 1 << 1;
    const NUM: u32 = 1 << 4;

    /// The layout of that name, if this machine has the XKB data for it.
    fn layout(layout: &str, variant: &str, options: &str) -> Option<Layout> {
        let names = Names { rules: "evdev".into(), model: "pc105".into(), layout: layout.into(), variant: variant.into(), options: options.into() };
        let found = Layout::of(&names).ok();
        if found.is_none() {
            eprintln!("skipped: no XKB data for the layout {layout}");
        }
        found
    }

    /// What pressing `code` types with `held` down.
    fn key(layout: &Layout, code: u8, held: &[u8], locked: u32, group: u32) -> Key {
        let pressed = layout.pressed(code, held, locked, group);
        classify(pressed.sym, &pressed.text, pressed.shortcut, false)
    }

    fn char(c: char) -> Key {
        Key::Typed(Typed::Char(c))
    }

    #[test]
    fn a_us_keyboard_types_what_is_printed_on_it() {
        let Some(us) = layout("us", "", "") else { return };
        assert_eq!(key(&us, A, &[], 0, 0), char('a'));
        assert_eq!(key(&us, A, &[SHIFT], 0, 0), char('A'));
        assert_eq!(key(&us, SEMICOLON, &[], 0, 0), char(';'));
        assert_eq!(key(&us, SEMICOLON, &[SHIFT], 0, 0), char(':'));
        assert_eq!(key(&us, TWO, &[SHIFT], 0, 0), char('@'));
        assert_eq!(key(&us, SPACE, &[], 0, 0), char(' '));
        // Other keys being down (a fast typist's fingers overlap) changes nothing.
        assert_eq!(key(&us, S, &[A, Q], 0, 0), char('s'));
        // A key that repeats is itself among the held ones.
        assert_eq!(key(&us, A, &[A], 0, 0), char('a'));
        assert_eq!(key(&us, SHIFT, &[], 0, 0), Key::Nothing);
    }

    #[test]
    fn caps_lock_and_num_lock_are_as_the_server_says() {
        let Some(us) = layout("us", "", "") else { return };
        assert_eq!(key(&us, A, &[], LOCK, 0), char('A'));
        assert_eq!(key(&us, A, &[SHIFT], LOCK, 0), char('a'));
        // Caps Lock capitalizes letters only.
        assert_eq!(key(&us, TWO, &[], LOCK, 0), char('2'));
        // Still held from switching it on: the lock is not undone.
        assert_eq!(key(&us, A, &[CAPS], LOCK, 0), char('A'));
        assert_eq!(key(&us, CAPS, &[], 0, 0), Key::Nothing);
        assert_eq!(key(&us, KP_1, &[], NUM, 0), char('1'));
        // Without Num Lock the key is End.
        assert_eq!(key(&us, KP_1, &[], 0, 0), Key::Typed(Typed::Break));
    }

    #[test]
    fn keys_that_are_not_text_break_the_word_or_erase() {
        let Some(us) = layout("us", "", "") else { return };
        for code in [RETURN, TAB, ESCAPE, DELETE, LEFT, F5] {
            assert_eq!(key(&us, code, &[], 0, 0), Key::Typed(Typed::Break), "key {code}");
        }
        assert_eq!(key(&us, BACK, &[], 0, 0), Key::Typed(Typed::Backspace));
        assert_eq!(key(&us, BACK, &[SHIFT], 0, 0), Key::Typed(Typed::Backspace));
    }

    #[test]
    fn a_key_pressed_with_ctrl_alt_or_super_is_a_shortcut() {
        let Some(us) = layout("us", "", "") else { return };
        for modifier in [CONTROL, ALT, SUPER] {
            assert_eq!(key(&us, A, &[modifier], 0, 0), Key::Typed(Typed::Break), "modifier {modifier}");
            assert_eq!(key(&us, A, &[modifier, SHIFT], 0, 0), Key::Typed(Typed::Break));
            // Ctrl+Backspace erases a whole word.
            assert_eq!(key(&us, BACK, &[modifier], 0, 0), Key::Typed(Typed::Break));
            assert_eq!(key(&us, modifier, &[], 0, 0), Key::Nothing);
        }
        // On this layout the right Alt key is an Alt.
        assert_eq!(key(&us, A, &[ALTGR], 0, 0), Key::Typed(Typed::Break));
    }

    #[test]
    fn a_german_keyboard_has_its_own_letters_and_altgr() {
        let Some(de) = layout("de", "", "") else { return };
        assert_eq!(key(&de, Y, &[], 0, 0), char('z'));
        assert_eq!(key(&de, APOSTROPHE, &[], 0, 0), char('ä'));
        assert_eq!(key(&de, APOSTROPHE, &[SHIFT], 0, 0), char('Ä'));
        assert_eq!(key(&de, SEMICOLON, &[], 0, 0), char('ö'));
        // AltGr is no shortcut: it reaches the third character on a key.
        assert_eq!(key(&de, Q, &[ALTGR], 0, 0), char('@'));
        assert_eq!(key(&de, E, &[ALTGR], 0, 0), char('€'));
        assert_eq!(key(&de, ALTGR, &[], 0, 0), Key::Nothing);
        // The key right of ß puts an accent on the next letter.
        assert_eq!(key(&de, EQUAL, &[], 0, 0), Key::Dead);
        // Without dead keys it types the accent itself.
        let Some(plain) = layout("de", "nodeadkeys", "") else { return };
        assert_eq!(key(&plain, EQUAL, &[], 0, 0), char('´'));
    }

    #[test]
    fn of_several_layouts_the_one_in_use_counts() {
        let Some(both) = layout("us,de", ",", "") else { return };
        assert_eq!(key(&both, Y, &[], 0, 0), char('y'));
        assert_eq!(key(&both, Y, &[], 0, 1), char('z'));
        assert_eq!(key(&both, SEMICOLON, &[SHIFT], 0, 0), char(':'));
        assert_eq!(key(&both, SEMICOLON, &[SHIFT], 0, 1), char('Ö'));
        // The right Alt key is Alt on the one and AltGr on the other.
        assert_eq!(key(&both, Q, &[ALTGR], 0, 0), Key::Typed(Typed::Break));
        assert_eq!(key(&both, Q, &[ALTGR], 0, 1), char('@'));
    }

    #[test]
    fn compose_and_what_cannot_be_trusted() {
        let Some(us) = layout("us", "", "compose:menu") else { return };
        const MENU: u8 = 135;
        assert_eq!(key(&us, MENU, &[], 0, 0), Key::Compose);
        // A latched modifier (sticky keys): the character may be another.
        assert_eq!(classify(u32::from(b'a'), "a", false, true), Key::Typed(Typed::Unknown));
        assert_eq!(classify(0xff0d, "\r", false, true), Key::Typed(Typed::Break));
        // More than one character from one key.
        assert_eq!(classify(0x1000000, "ab", false, false), Key::Typed(Typed::Unknown));
        // A key with nothing on it.
        assert_eq!(classify(0, "", false, false), Key::Typed(Typed::Break));
        // Every dead key, the first and the last.
        assert_eq!(classify(0xfe50, "", false, false), Key::Dead);
        assert_eq!(classify(0xfe93, "", false, false), Key::Dead);
    }

    /// Against a real X server: `DISPLAY=:N cargo test -- --ignored on_an_x_server`,
    /// with a server of one's own (`Xvfb :N`) and xdotool. Never the desktop
    /// in use: this presses keys.
    #[test]
    #[ignore]
    fn on_an_x_server_keys_are_heard_and_programs_keys_are_not() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::{Arc, Mutex};

        fn listen(during: impl FnOnce()) -> Vec<Typed> {
            let (heard, wanted) = (Arc::new(Mutex::new(Vec::new())), Arc::new(AtomicBool::new(true)));
            let listener = {
                let (heard, wanted) = (heard.clone(), wanted.clone());
                std::thread::spawn(move || watch(&|| wanted.load(Ordering::SeqCst), &mut |key| heard.lock().unwrap().push(key)).map_err(|e| e.to_string()))
            };
            std::thread::sleep(Duration::from_millis(500));
            during();
            std::thread::sleep(Duration::from_millis(500));
            wanted.store(false, Ordering::SeqCst);
            listener.join().unwrap().unwrap();
            let heard = heard.lock().unwrap().clone();
            heard
        }
        // A server with no program connected starts afresh, and forgets
        // the properties and the layout this sets.
        let (conn, screen) = RustConnection::connect(None).unwrap();
        let root = conn.setup().roots[screen].root;
        let xdotool = |args: &[&str]| assert!(std::process::Command::new("xdotool").args(args).status().unwrap().success());
        let chars = |text: &str| text.chars().map(Typed::Char).collect::<Vec<_>>();

        // An application to replace text in.
        let field = super::super::field::Field::open();

        // What programs press is not typing.
        std::env::remove_var("HEYO_TYPED_SYNTHETIC");
        let heard = listen(|| {
            xdotool(&["key", "a", "a", "a"]);
            replace(2, "今天😀", false).unwrap();
        });
        assert_eq!(heard, []);
        // The application has erased, and taken the text.
        assert_eq!(field.text(), "a今天😀");

        // Taken for real keys, it is what a keyboard would have typed.
        std::env::set_var("HEYO_TYPED_SYNTHETIC", "1");
        let heard = listen(|| xdotool(&["key", "s", "i", "g", "shift+a", "semicolon", "shift+semicolon", "space", "BackSpace", "ctrl+c", "Return"]));
        assert_eq!(heard, [chars("sigA;: "), vec![Typed::Backspace, Typed::Break, Typed::Break]].concat());
        // Heyo's own keys arrive, in order: the Backspaces, as many as
        // asked for, then the paste shortcut, the terminals' too.
        let replaced = [Typed::Backspace, Typed::Backspace, Typed::Backspace, Typed::Break];
        assert_eq!(listen(|| replace(3, "bc", false).unwrap()), replaced);
        assert_eq!(listen(|| replace(3, "bcd", true).unwrap()), replaced);
        assert_eq!(listen(|| replace(0, "e", false).unwrap()), [Typed::Break]);
        assert_eq!(field.text(), "bcde");
        // And leave no key down behind them.
        assert!(conn.query_keymap().unwrap().reply().unwrap().keys.iter().all(|keys| *keys == 0));
        assert_eq!(listen(|| xdotool(&["click", "1"])), [Typed::Break]);
        assert!(!modifiers_held().unwrap());
        // Another window comes to the front (as a window manager says so).
        let run = |program: &str, args: &[&str]| assert!(std::process::Command::new(program).args(args).status().unwrap().success());
        let front = |id: &str| run("xprop", &["-root", "-f", "_NET_ACTIVE_WINDOW", "32x", "-set", "_NET_ACTIVE_WINDOW", id]);
        front("0x400001");
        assert_eq!(listen(|| front("0x400002")), [Typed::Break]);
        // Said again without a change, it is nothing.
        assert_eq!(listen(|| front("0x400002")), []);
        // Another layout: the same key is another letter from then on.
        let heard = listen(|| {
            xdotool(&["key", "29"]);
            run("setxkbmap", &["de"]);
            std::thread::sleep(Duration::from_millis(300));
            xdotool(&["key", "29", "48"]);
        });
        run("setxkbmap", &["us"]);
        assert_eq!(heard.first(), Some(&Typed::Char('y')));
        assert_eq!(heard[heard.len() - 2..], [Typed::Char('z'), Typed::Char('ä')]);
        assert!(heard[1..heard.len() - 2].iter().all(|key| *key == Typed::Break) && heard.len() > 3);

        // A modifier held down, as by a finger.
        conn.xtest_fake_input(KEY_PRESS_EVENT, SHIFT, x11rb::CURRENT_TIME, root, 0, 0, 0).unwrap();
        conn.get_input_focus().unwrap().reply().unwrap();
        assert!(modifiers_held().unwrap());
        // Then nothing is pressed at all.
        assert!(replace(1, "f", false).is_err());
        conn.xtest_fake_input(KEY_RELEASE_EVENT, SHIFT, x11rb::CURRENT_TIME, root, 0, 0, 0).unwrap();
        conn.get_input_focus().unwrap().reply().unwrap();
        assert!(!modifiers_held().unwrap());
        assert_eq!(field.text(), "bcde");

        // An application that takes nothing at the paste shortcut: the keys
        // are pressed, and that the text was not taken is said.
        drop(field);
        assert!(replace(0, "anyone?", false).unwrap_err().to_string().contains("not seen to take"));
    }

    #[test]
    fn the_layout_names_are_read_from_the_root_window() {
        let names = Names::parse(b"evdev\0pc105\0us,de\0,nodeadkeys\0grp:alt_shift_toggle\0").unwrap();
        assert_eq!(
            names,
            Names { rules: "evdev".into(), model: "pc105".into(), layout: "us,de".into(), variant: ",nodeadkeys".into(), options: "grp:alt_shift_toggle".into() }
        );
        assert_eq!(Names::parse(b"evdev\0pc105\0us\0\0\0").unwrap().variant, "");
        // Not set, cut short, or naming no layout: then nothing is known.
        assert_eq!(Names::parse(b""), None);
        assert_eq!(Names::parse(b"evdev\0pc105\0"), None);
        assert_eq!(Names::parse(b"evdev\0pc105\0\0\0\0"), None);
        assert!(Layout::of(&Names { rules: "evdev".into(), layout: "no-such-layout".into(), ..Names::default() }).is_err());
    }
}

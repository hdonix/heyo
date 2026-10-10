//! Watching one key being held, through the X Input extension's raw key
//! events. Those are a copy of what the keyboard does; nothing is grabbed,
//! and the key keeps its normal meaning everywhere else.

use std::error::Error;
use std::time::{Duration, Instant};

use x11rb::connection::Connection;
use x11rb::protocol::xinput::{self, ConnectionExt as _, XIEventMask};
use x11rb::protocol::xproto::ConnectionExt as _;
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;

use crate::platform::keys::{Hold, HOLD, REGRET};

/// The X keysyms that count as each key (`keys::KEYS`). On many layouts
/// Right Alt is "AltGr", ISO_Level3_Shift.
const KEYSYMS: &[(&str, &[u32])] = &[
    ("alt_r", &[0xffea, 0xfe03]),
    ("alt_l", &[0xffe9]),
    ("control_r", &[0xffe4]),
    ("control_l", &[0xffe3]),
    ("shift_r", &[0xffe2]),
    ("super_r", &[0xffec]),
    ("menu", &[0xff67]),
    ("caps_lock", &[0xffe5]),
    ("f13", &[0xffca]),
];

/// The keycodes that produce any of `keysyms` on this keyboard.
fn keycodes(conn: &RustConnection, keysyms: &[u32]) -> Result<Vec<u8>, Box<dyn Error>> {
    let setup = conn.setup();
    let (min, max) = (setup.min_keycode, setup.max_keycode);
    let mapping = conn.get_keyboard_mapping(min, max - min + 1)?.reply()?;
    let per = mapping.keysyms_per_keycode as usize;
    Ok(mapping
        .keysyms
        .chunks(per.max(1))
        .enumerate()
        // The unshifted symbol is what names the key.
        .filter(|(_, symbols)| symbols.first().is_some_and(|s| keysyms.contains(s)))
        .map(|(i, _)| min + i as u8)
        .collect())
}

/// Whether any key other than `codes` is held, and whether one of `codes`
/// is, according to the X server's own picture of the keyboard.
fn held(conn: &RustConnection, codes: &[u8]) -> Result<(bool, bool), Box<dyn Error>> {
    let keys = conn.query_keymap()?.reply()?.keys;
    let (mut talk, mut other) = (false, false);
    for code in 8..=255u8 {
        if keys[code as usize / 8] & (1 << (code % 8)) != 0 {
            if codes.contains(&code) {
                talk = true;
            } else {
                other = true;
            }
        }
    }
    Ok((other, talk))
}

/// Watches for as long as the session lasts. `key` is asked for the
/// configured key (it can change at any time); `on` hears what happens.
pub fn watch(key: impl Fn() -> String, on: impl Fn(Hold)) -> Result<(), Box<dyn Error>> {
    let (conn, screen) = RustConnection::connect(None)?;
    let root = conn.setup().roots[screen].root;
    // From version 2.1 on, raw events reach every listener even while some
    // program holds a grab (the desktop's window switcher, a menu, a game).
    // With 2.0 they stop during a grab, and presses or releases go missing.
    conn.xinput_xi_query_version(2, 2)?.reply()?;
    // Keys "pressed" by programs (Heyo's own paste keystroke releases held
    // modifiers for a moment) are not the user letting go.
    let devices = conn.xinput_xi_query_device(xinput::Device::ALL)?.reply()?;
    let mut synthetic: Vec<u16> =
        devices.infos.iter().filter(|d| String::from_utf8_lossy(&d.name).contains("XTEST")).map(|d| d.deviceid).collect();
    // Tests have nothing but synthetic keys to press.
    if cfg!(debug_assertions) && std::env::var_os("HEYO_TALKKEY_SYNTHETIC").is_some() {
        eprintln!("heyo: push-to-talk accepts synthetic keys (ignoring {} XTEST device(s))", synthetic.len());
        synthetic.clear();
    }
    let mask = xinput::EventMask { deviceid: xinput::Device::ALL_MASTER.into(), mask: vec![XIEventMask::RAW_KEY_PRESS | XIEventMask::RAW_KEY_RELEASE] };
    conn.xinput_xi_select_events(root, &[mask])?;
    conn.flush()?;

    let mut configured = String::new();
    let mut codes: Vec<u8> = Vec::new();
    // The talk key is down (as far as the events say).
    let mut talk_down = false;
    // Held alone since; recording since.
    let mut armed: Option<Instant> = None;
    let mut recording: Option<Instant> = None;
    // While recording: when the server last agreed that the key is down.
    let mut seen_down = Instant::now();
    let mut checked = Instant::now();

    loop {
        let event = conn.poll_for_event()?;
        // The setting can change at any time; the keyboard layout rarely.
        let setting = key();
        if setting != configured {
            let keysyms = KEYSYMS.iter().find(|(id, _)| *id == setting).map_or(&[][..], |(_, keysyms)| keysyms);
            codes = keycodes(&conn, keysyms)?;
            if !setting.is_empty() && codes.is_empty() {
                eprintln!("heyo: this keyboard layout has no {setting} key for push-to-talk");
            }
            eprintln!("heyo: push-to-talk key: {} (keycodes {codes:?})", if setting.is_empty() { "none" } else { &setting });
            configured = setting;
            armed = None;
            talk_down = false;
        }
        if armed.is_some_and(|since| since.elapsed() >= HOLD) {
            armed = None;
            recording = Some(Instant::now());
            seen_down = Instant::now();
            eprintln!("heyo: push-to-talk: recording");
            on(Hold::Started);
        }
        // A release can still go unseen. The server's keymap is the second
        // opinion: when it has shown the key up for a while, that is that.
        // (Only "for a while": a paste keystroke lifts modifiers for a moment.)
        if recording.is_some() && checked.elapsed() >= Duration::from_millis(60) {
            checked = Instant::now();
            if held(&conn, &codes)?.1 {
                seen_down = Instant::now();
            } else if seen_down.elapsed() >= Duration::from_millis(400) {
                recording = None;
                talk_down = false;
                eprintln!("heyo: push-to-talk: key no longer held; finishing");
                on(Hold::Released);
            }
        }
        let (code, pressed) = match event {
            Some(Event::XinputRawKeyPress(e)) if !synthetic.contains(&e.sourceid) => (e.detail, true),
            Some(Event::XinputRawKeyRelease(e)) if !synthetic.contains(&e.sourceid) => (e.detail, false),
            Some(_) => continue,
            None => {
                std::thread::sleep(Duration::from_millis(8));
                continue;
            }
        };
        let is_talk_key = u8::try_from(code).is_ok_and(|c| codes.contains(&c));
        match (is_talk_key, pressed) {
            // Auto-repeat of the held key.
            (true, true) if talk_down => {}
            (true, true) => {
                talk_down = true;
                // Pressed with nothing else down: maybe the start of a hold.
                // Asked of the server rather than remembered from events, so
                // one lost release cannot disable the key for good.
                if recording.is_none() {
                    if held(&conn, &codes)?.0 {
                        eprintln!("heyo: push-to-talk: key pressed with other keys held; ignored");
                    } else {
                        armed = Some(Instant::now());
                    }
                }
            }
            (true, false) => {
                talk_down = false;
                if armed.take().is_some() {
                    eprintln!("heyo: push-to-talk: released before {} ms; ignored", HOLD.as_millis());
                }
                if recording.take().is_some() {
                    eprintln!("heyo: push-to-talk: released; finishing");
                    on(Hold::Released);
                }
            }
            // Another key: it was a shortcut all along.
            (false, true) => {
                armed = None;
                if recording.is_some_and(|since| since.elapsed() < REGRET) {
                    recording = None;
                    on(Hold::Cancelled);
                }
            }
            (false, false) => {}
        }
    }
}

//! Holding one key to talk: Right Alt, say, held while dictating.
//!
//! Desktop shortcuts cannot do this: they need a key combination and only
//! report the press. So the key itself is watched. A short hold is required
//! before it counts, and pressing another key meanwhile calls it off: Alt
//! followed by Tab is a shortcut, not dictation.

use std::time::Duration;

use super::desktop::desktop;

/// The keys offered: setting value and the name shown.
pub const KEYS: &[(&str, &str)] = &[
    ("alt_r", "Right Alt"),
    ("alt_l", "Left Alt"),
    ("control_r", "Right Ctrl"),
    ("control_l", "Left Ctrl"),
    ("shift_r", "Right Shift"),
    ("super_r", "Right Super"),
    ("menu", "Menu"),
    ("caps_lock", "Caps Lock"),
    ("f13", "F13"),
];

/// The X keysyms that count as each key. On many layouts Right Alt is
/// "AltGr", ISO_Level3_Shift.
#[cfg(target_os = "linux")]
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

/// The symbols a key (a setting's value) goes by where keys are known by
/// their X keysyms; none for no key.
#[cfg(target_os = "linux")]
pub fn symbols(key: &str) -> &'static [u32] {
    KEYSYMS.iter().find(|(value, _)| *value == key).map_or(&[], |(_, symbols)| symbols)
}

/// The keys this session can watch.
pub fn available() -> Vec<(&'static str, &'static str)> {
    let offered = desktop().hold_keys();
    KEYS.iter().copied().filter(|(value, _)| offered.contains(value)).collect()
}

/// How long the key must be held alone before it counts.
pub const HOLD: Duration = Duration::from_millis(220);
/// Another key this soon after the hold began means a shortcut was meant.
pub const REGRET: Duration = Duration::from_millis(400);

/// What happens with the watched key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hold {
    /// Held alone for `HOLD`.
    Started,
    /// Let go after `Started`.
    Released,
    /// Another key was pressed within `REGRET` of `Started`.
    Cancelled,
}

/// Watches the key named by `key` (asked again whenever it matters, so the
/// setting can change) and reports to `on`, for as long as the session
/// lasts. Returns an error where a single key cannot be watched.
pub fn watch_hold(key: impl Fn() -> String, on: impl Fn(Hold)) -> Result<(), String> {
    desktop().watch_hold(&key, &on)
}

/// Which key an event was about, as far as holding to talk cares.
#[cfg(any(windows, test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    /// The watched one.
    Talk,
    Other,
}

/// Decides, from the keyboard's presses and releases, when a hold starts
/// and how it ends. It only remembers and answers: the adapter feeds it
/// what the keyboard did and the time, and passes on what it says.
///
/// The rules are those of the X11 watcher (`x11/hold_key.rs`, which was
/// written first and keeps its own copy): the key pressed with nothing else
/// down and held for `HOLD` starts; letting go ends; another key before
/// `HOLD` means a shortcut, and within `REGRET` of the start calls it off.
#[cfg(any(windows, test))]
#[derive(Debug, Default)]
pub struct Watch {
    /// The watched key is down, as far as the events say.
    talk_down: bool,
    /// Held alone since then, not yet for `HOLD`.
    armed: Option<std::time::Instant>,
    /// Started then.
    recording: Option<std::time::Instant>,
}

#[cfg(any(windows, test))]
impl Watch {
    /// A key went down or came up. `others_held`: whether any key but the
    /// watched one is down at this moment.
    pub fn key(&mut self, now: std::time::Instant, key: Key, pressed: bool, others_held: bool) -> Option<Hold> {
        match (key, pressed) {
            // The held key repeating.
            (Key::Talk, true) if self.talk_down => None,
            (Key::Talk, true) => {
                self.talk_down = true;
                if self.recording.is_none() && !others_held {
                    self.armed = Some(now);
                }
                None
            }
            (Key::Talk, false) => {
                self.talk_down = false;
                self.armed = None;
                self.recording.take().map(|_| Hold::Released)
            }
            // Another key: it was a shortcut all along.
            (Key::Other, true) => {
                self.armed = None;
                let regretted = self.recording.is_some_and(|since| now.saturating_duration_since(since) < REGRET);
                regretted.then(|| {
                    self.recording = None;
                    Hold::Cancelled
                })
            }
            (Key::Other, false) => None,
        }
    }

    /// Time has passed: says `Started` once the key has been held for `HOLD`.
    pub fn tick(&mut self, now: std::time::Instant) -> Option<Hold> {
        let held = self.armed.is_some_and(|since| now.saturating_duration_since(since) >= HOLD);
        held.then(|| {
            self.armed = None;
            self.recording = Some(now);
            Hold::Started
        })
    }

    /// When `tick` will next have something to say.
    pub fn due(&self) -> Option<std::time::Instant> {
        self.armed.map(|since| since + HOLD)
    }

    /// Between `Started` and its end.
    pub fn recording(&self) -> bool {
        self.recording.is_some()
    }

    /// The key turned out to be up though no release was seen. Also what
    /// to call when the watched key changes: nothing of the old one counts.
    pub fn lost(&mut self) -> Option<Hold> {
        self.talk_down = false;
        self.armed = None;
        self.recording.take().map(|_| Hold::Released)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn a_key_held_alone_starts_and_letting_go_ends() {
        let (mut watch, t) = (Watch::default(), Instant::now());
        assert_eq!(watch.key(t, Key::Talk, true, false), None);
        assert_eq!(watch.due(), Some(t + HOLD));
        assert_eq!(watch.tick(t + ms(100)), None);
        // The keyboard repeats a held key.
        assert_eq!(watch.key(t + ms(150), Key::Talk, true, false), None);
        assert_eq!(watch.tick(t + HOLD), Some(Hold::Started));
        assert!(watch.recording());
        assert_eq!(watch.tick(t + HOLD + ms(50)), None);
        assert_eq!(watch.due(), None);
        assert_eq!(watch.key(t + ms(2000), Key::Talk, false, false), Some(Hold::Released));
        assert!(!watch.recording());
    }

    #[test]
    fn a_tap_is_not_a_hold() {
        let (mut watch, t) = (Watch::default(), Instant::now());
        watch.key(t, Key::Talk, true, false);
        assert_eq!(watch.key(t + ms(80), Key::Talk, false, false), None);
        assert_eq!(watch.tick(t + ms(500)), None);
    }

    #[test]
    fn another_key_makes_it_a_shortcut() {
        let (mut watch, t) = (Watch::default(), Instant::now());
        // Alt, then Tab before the hold counts.
        watch.key(t, Key::Talk, true, false);
        assert_eq!(watch.key(t + ms(100), Key::Other, true, false), None);
        assert_eq!(watch.tick(t + ms(500)), None);
        assert_eq!(watch.key(t + ms(600), Key::Talk, false, true), None);
        // Pressed while something else is down: never armed.
        assert_eq!(watch.key(t + ms(700), Key::Talk, true, true), None);
        assert_eq!(watch.tick(t + ms(1500)), None);
    }

    #[test]
    fn another_key_soon_after_the_start_calls_it_off_but_later_does_not() {
        let (mut watch, t) = (Watch::default(), Instant::now());
        watch.key(t, Key::Talk, true, false);
        assert_eq!(watch.tick(t + HOLD), Some(Hold::Started));
        assert_eq!(watch.key(t + HOLD + ms(100), Key::Other, true, false), Some(Hold::Cancelled));
        // Nothing more is said about a hold that was called off.
        assert_eq!(watch.key(t + ms(900), Key::Talk, false, false), None);

        let t = t + ms(5000);
        watch.key(t, Key::Talk, true, false);
        assert_eq!(watch.tick(t + HOLD), Some(Hold::Started));
        assert_eq!(watch.key(t + HOLD + REGRET, Key::Other, true, false), None);
        assert!(watch.recording());
        assert_eq!(watch.key(t + ms(3000), Key::Talk, false, true), Some(Hold::Released));
    }

    #[test]
    fn a_release_that_went_unseen_still_ends_it_and_the_key_works_again() {
        let (mut watch, t) = (Watch::default(), Instant::now());
        watch.key(t, Key::Talk, true, false);
        watch.tick(t + HOLD);
        assert_eq!(watch.lost(), Some(Hold::Released));
        assert_eq!(watch.lost(), None);
        let t = t + ms(3000);
        assert_eq!(watch.key(t, Key::Talk, true, false), None);
        assert_eq!(watch.tick(t + HOLD), Some(Hold::Started));
    }
}

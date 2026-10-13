//! What a key is, for a layer surface that has the keyboard.
//!
//! A Wayland compositor says which key went down, by its place on the
//! keyboard, and hands over the layout once; what the key means under
//! that layout, with the keys held at the time, is the program's to work
//! out, and so is pressing it again while it stays down. winit does both
//! for its windows; this does them for a surface of Heyo's own, with the
//! library every desktop works keys out with (xkbcommon, which Heyo has
//! for its snippets already): the layout, the keys held, the keys that
//! make a letter out of two (an accent and then a vowel).

use std::io::Read;
use std::os::fd::OwnedFd;

use slint::platform::Key as Named;
use slint::SharedString;
use xkbcommon::xkb;

use super::super::standing::KeyOf;

/// A key that went down or up, as the window is told of it.
#[derive(Clone)]
pub(super) struct Press {
    /// Its place on the keyboard, as the compositor numbers them.
    pub code: u32,
    pub key: KeyOf,
    /// What Slint is told it was: a character, or its own name for a key
    /// that makes none. None for a key Slint has nothing to do with, and
    /// for one that is only part of a letter so far.
    pub text: Option<SharedString>,
    /// For a key that is pressed again and again while it stays down:
    /// how many times a second, and after how many milliseconds.
    pub repeats: Option<(u32, u32)>,
}

pub(super) struct Keyboard {
    context: xkb::Context,
    keymap: Option<xkb::Keymap>,
    state: Option<xkb::State>,
    /// The letter that is being made out of several keys, where the
    /// user's language has such keys.
    compose: Option<xkb::compose::State>,
    /// How many times a second a key that stays down is pressed again,
    /// and after how many milliseconds: as the compositor says.
    pub rate: u32,
    pub delay: u32,
}

impl Keyboard {
    pub(super) fn new() -> Self {
        let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
        let locale = ["LC_ALL", "LC_CTYPE", "LANG"].iter().find_map(|name| std::env::var_os(name).filter(|value| !value.is_empty())).unwrap_or_else(|| "C".into());
        let compose = xkb::compose::Table::new_from_locale(&context, &locale, xkb::compose::COMPILE_NO_FLAGS).ok().map(|table| xkb::compose::State::new(&table, xkb::compose::STATE_NO_FLAGS));
        Self { context, keymap: None, state: None, compose, rate: 25, delay: 400 }
    }

    /// The layout, as the compositor hands it over: a file of so many
    /// bytes, in xkb's own text.
    pub(super) fn layout(&mut self, file: OwnedFd, size: u32) {
        let mut text = Vec::with_capacity(size as usize);
        if std::fs::File::from(file).take(u64::from(size)).read_to_end(&mut text).is_err() {
            return;
        }
        // It ends with a zero, which is not part of it.
        text.retain(|byte| *byte != 0);
        let Ok(text) = String::from_utf8(text) else { return };
        self.keymap = xkb::Keymap::new_from_string(&self.context, text, xkb::KEYMAP_FORMAT_TEXT_V1, xkb::KEYMAP_COMPILE_NO_FLAGS);
        self.state = self.keymap.as_ref().map(xkb::State::new);
        if self.keymap.is_none() {
            eprintln!("heyo: the keyboard layout the compositor handed over could not be read");
        }
    }

    /// The keys held with others changed. Says whether Control and Alt
    /// are among them.
    pub(super) fn held(&mut self, depressed: u32, latched: u32, locked: u32, group: u32) -> (bool, bool) {
        let Some(state) = self.state.as_mut() else { return (false, false) };
        state.update_mask(depressed, latched, locked, 0, 0, group);
        let active = |name: &str| state.mod_name_is_active(name, xkb::STATE_MODS_EFFECTIVE);
        (active(xkb::MOD_NAME_CTRL), active(xkb::MOD_NAME_ALT))
    }

    /// The key at that place went down or up.
    pub(super) fn key(&mut self, code: u32, down: bool) -> Option<Press> {
        let (state, keymap) = (self.state.as_ref()?, self.keymap.as_ref()?);
        // xkb's numbers are the compositor's and eight.
        let place = xkb::Keycode::new(code + 8);
        let sym = state.key_get_one_sym(place);
        let made = char::from_u32(xkb::keysym_to_utf32(sym)).filter(|made| !made.is_control());
        let key = match sym.raw() {
            0xff1b => KeyOf::Escape,
            0xff0d | 0xff8d => KeyOf::Enter,
            0xffc1 => KeyOf::F4,
            0xffe3 | 0xffe4 => KeyOf::Control,
            0xffe9 | 0xffea => KeyOf::Alt,
            _ => made.map_or(KeyOf::Other, |made| KeyOf::Character { made, placed: letter_at(code) }),
        };
        let mut text = named(sym.raw()).map(SharedString::from).or_else(|| {
            // With the keys held: a capital, a sign of the third level.
            // (With Control held xkb gives a control character: then the
            // key's own letter, and Slint knows that Control is down.)
            let typed = state.key_get_utf8(place);
            if !typed.is_empty() && !typed.chars().any(char::is_control) {
                Some(typed.into())
            } else {
                made.map(|made| SharedString::from(made.to_string().as_str()))
            }
        });
        if down {
            if let Some(compose) = self.compose.as_mut() {
                if compose.feed(sym) == xkb::compose::FeedResult::Accepted {
                    match compose.status() {
                        xkb::compose::Status::Composing => text = None,
                        xkb::compose::Status::Composed => {
                            text = compose.utf8().map(|made| made.as_str().into());
                            compose.reset();
                        }
                        xkb::compose::Status::Cancelled => {
                            text = None;
                            compose.reset();
                        }
                        xkb::compose::Status::Nothing => {}
                    }
                }
            }
        }
        Some(Press { code, key, text, repeats: (keymap.key_repeats(place) && self.rate > 0).then_some((self.rate, self.delay)) })
    }
}

/// Slint's name for a key that makes no character, by the name xkb has
/// for it.
fn named(sym: u32) -> Option<Named> {
    Some(match sym {
        0xff08 => Named::Backspace,
        0xff09 => Named::Tab,
        0xfe20 => Named::Backtab,
        0xff0d | 0xff8d => Named::Return,
        0xff1b => Named::Escape,
        0xffff => Named::Delete,
        0xff50 => Named::Home,
        0xff51 => Named::LeftArrow,
        0xff52 => Named::UpArrow,
        0xff53 => Named::RightArrow,
        0xff54 => Named::DownArrow,
        0xff55 => Named::PageUp,
        0xff56 => Named::PageDown,
        0xff57 => Named::End,
        0xff63 => Named::Insert,
        0xffe1 => Named::Shift,
        0xffe2 => Named::ShiftR,
        0xffe3 => Named::Control,
        0xffe4 => Named::ControlR,
        0xffe5 => Named::CapsLock,
        0xffe9 => Named::Alt,
        0xffea | 0xfe03 => Named::AltGr,
        0xffeb => Named::Meta,
        0xffec => Named::MetaR,
        _ => return None,
    })
}

/// The letter at that place of the layout the letters are named after
/// (the American one), for a shortcut on a layout that has other letters.
fn letter_at(code: u32) -> Option<char> {
    const ROWS: [(u32, &str); 3] = [(16, "qwertyuiop"), (30, "asdfghjkl"), (44, "zxcvbnm")];
    ROWS.iter().find_map(|(first, letters)| letters.chars().nth(code.checked_sub(*first)? as usize))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shortcut_is_where_its_letter_is_on_the_american_layout() {
        assert_eq!(letter_at(46), Some('c'));
        assert_eq!(letter_at(31), Some('s'));
        assert_eq!(letter_at(44), Some('z'));
        assert_eq!(letter_at(16), Some('q'));
        assert_eq!(letter_at(26), None);
        assert_eq!(letter_at(1), None);
    }

    #[test]
    fn keys_that_make_no_character_have_slints_names() {
        assert!(named(0xff08) == Some(Named::Backspace) && named(0xff51) == Some(Named::LeftArrow));
        assert!(named(0x61).is_none());
    }
}

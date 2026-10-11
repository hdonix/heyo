//! What is typed into other applications, and replacing it there, for
//! snippets that expand as their keyword is typed (`crate::expand`).
//!
//! The keyboard is only ever listened to: an adapter gets a copy of what
//! the keys do and never grabs, holds back or changes one, so whatever
//! happens to Heyo the keyboard works as before. What it hears it passes on
//! one key at a time and keeps nowhere.
//!
//! Text is replaced by erasing with Backspace and pasting what takes its
//! place: the keys of one replacement are handed over in one piece, each
//! press with its release, so that none is left down should Heyo stop in
//! the middle.
//!
//! Only X11 has an adapter. On Wayland an application is not shown what is
//! typed in others; GNOME's Shell could be made to tell (Heyo's extension
//! runs inside it), which would put a listener for every key into the
//! Shell, and has deliberately not been done. On Windows a low-level
//! keyboard hook would do it (`win/hold.rs` has one for a single key), but
//! asking Windows which character a key gives (`ToUnicodeEx`) from inside a
//! hook disturbs dead keys for the application being typed into, and that
//! needs care this has not had yet.

use super::desktop::desktop;
use super::Support;

/// What a key press did to the text being typed, as far as can be known.
#[derive(Clone, Copy, PartialEq, Eq)]
// Only X11 has an adapter that says any of it so far.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub enum Typed {
    /// This character was typed.
    Char(char),
    /// Some character was typed, and which cannot be known (a dead key or
    /// Compose was part of it).
    Unknown,
    /// The character before the cursor was erased.
    Backspace,
    /// The cursor may be somewhere else now, or what was typed was no text:
    /// arrows, Enter, Tab, Escape, a shortcut, a click, another window.
    Break,
}

/// Never shows the character: what is typed must not end up in a log by way
/// of a debug print.
impl std::fmt::Debug for Typed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Typed::Char(_) => "Char(…)",
            Typed::Unknown => "Unknown",
            Typed::Backspace => "Backspace",
            Typed::Break => "Break",
        })
    }
}

/// Following what is typed, and replacing what was just typed.
pub trait Typing: Send + Sync {
    /// Reports every key that matters to `on` until `wanted` says no (it is
    /// asked at least twice a second), then returns. Keys pressed by
    /// programs, Heyo's own among them, are not reported.
    fn watch(&self, wanted: &dyn Fn() -> bool, on: &mut dyn FnMut(Typed)) -> Result<(), String>;

    /// Replaces the `erase` characters before the cursor of the focused
    /// window with `text`: presses Backspace that many times and then the
    /// window's paste shortcut (a terminal's, if it is one), with nothing
    /// in between, the clipboard having been given `text`. A key the user
    /// presses meanwhile must not land among them, least of all while the
    /// shortcut's Ctrl is down. Returns once the application has taken the
    /// text. Does nothing, and says so, if a modifier is held at that moment.
    ///
    /// The clipboard is the caller's to have borrowed (`pasting::Loan`).
    fn replace(&self, erase: usize, text: &str, terminal: bool) -> Result<(), String>;

    /// Whether an input method may be turning keys into other text at
    /// present (Chinese, Japanese, …): then the keys are not what is typed.
    /// Where that cannot be found out, yes.
    fn composing(&self) -> bool {
        true
    }

    /// Whether Shift, Ctrl, Alt, Super or AltGr is down: a Backspace sent
    /// now would be a different key.
    fn modifiers_held(&self) -> bool {
        false
    }
}

/// Whether this session lets typing be followed.
pub fn supported() -> bool {
    desktop().supported().typing != Support::None && desktop().typing().is_some()
}

fn typing() -> Result<&'static dyn Typing, String> {
    desktop().typing().ok_or_else(|| super::unsupported("Replacing what is typed"))
}

/// See [`Typing::watch`].
pub fn watch(wanted: &dyn Fn() -> bool, on: &mut dyn FnMut(Typed)) -> Result<(), String> {
    typing()?.watch(wanted, on)
}

/// See [`Typing::replace`].
pub fn replace(erase: usize, text: &str, terminal: bool) -> Result<(), String> {
    typing()?.replace(erase, text, terminal)
}

/// See [`Typing::composing`]. Without an adapter nothing is known: yes.
pub fn composing() -> bool {
    desktop().typing().is_none_or(Typing::composing)
}

/// See [`Typing::modifiers_held`].
pub fn modifiers_held() -> bool {
    desktop().typing().is_some_and(Typing::modifiers_held)
}

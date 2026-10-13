//! What is typed into other applications, and replacing it there: for
//! snippets that expand as their keyword is typed (`crate::expand`), and for
//! dictated words that go in as they are spoken (`crate::dictation`).
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

use crate::i18n::marked;
use std::sync::atomic::AtomicU32;
use std::sync::Arc;

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

    /// Begins writing into the window that has the focus now: see
    /// [`Writing`]. `hotkeys` are the shortcuts (as `<Control><Alt>d`) the
    /// desktop keeps from applications and runs Heyo for: pressing one of
    /// them is not typing. `own_presses` counts the presses of a mouse
    /// button on Heyo's own windows, which move no cursor in the window
    /// written into: whoever shows such a window counts them there, and a
    /// press that is heard of is held against the count.
    fn write(&self, hotkeys: &[String], own_presses: Arc<AtomicU32>) -> Result<Box<dyn Writing>, String>;

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

/// Text being written into one window a piece at a time, each piece taking
/// the place of the end of what was written before: words as they are
/// recognized, corrected where they stand.
///
/// What it promises: a piece is written only while nothing but these pieces
/// can have touched the text since this began. It follows the keyboard and
/// the mouse from then on (listening only, like [`Typing::watch`], and to
/// what programs press as well), and once the user has pressed a key that
/// reaches the application or a mouse button, another window has come to
/// the front, or a piece went in without the application being seen to take
/// it, nothing more is written: every later piece is refused. (A press of a
/// mouse button on one of Heyo's own windows is none of that: see
/// [`Typing::write`].)
///
/// Modifier keys that are held (the key held to talk is often one) are let
/// go for the moment of each piece and pressed again after it. While
/// another program has the keyboard to itself (the desktop, for as long as
/// a hotkey is held) a piece cannot be written and is put off
/// ([`Lost::later`]): that ends nothing.
pub trait Writing: Send {
    /// Replaces the `erase` characters before the cursor with `text`, and
    /// returns once the application has taken the text.
    fn replace(&mut self, erase: usize, text: &str) -> Result<(), Lost>;
}

/// A piece that was not written, or not surely.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lost {
    /// How many of the characters to erase were erased all the same.
    pub erased: usize,
    /// Whether the text was sent on its way: it may be there, or not.
    pub put: bool,
    /// Nothing was touched, and nothing is over: the piece could not be
    /// written at this moment, and a later one may be.
    pub later: bool,
    /// Says why, never what the text was.
    pub why: String,
}

impl Lost {
    /// Nothing was touched.
    pub fn untouched(why: impl Into<String>) -> Lost {
        Lost { erased: 0, put: false, later: false, why: why.into() }
    }

    /// Nothing was touched, and a later piece may be written.
    // Only X11 has an adapter that puts anything off so far.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub fn put_off(why: impl Into<String>) -> Lost {
        Lost { later: true, ..Lost::untouched(why) }
    }
}

/// Whether this session lets typing be followed.
pub fn supported() -> bool {
    desktop().supported().typing != Support::None && desktop().typing().is_some()
}

/// Whether text can be written into another window piece by piece here.
pub fn rewriting() -> bool {
    desktop().supported().rewriting != Support::None && desktop().typing().is_some()
}

fn typing() -> Result<&'static dyn Typing, String> {
    desktop().typing().ok_or_else(|| super::unsupported(marked("Replacing what is typed")))
}

/// See [`Typing::watch`].
pub fn watch(wanted: &dyn Fn() -> bool, on: &mut dyn FnMut(Typed)) -> Result<(), String> {
    typing()?.watch(wanted, on)
}

/// See [`Typing::replace`].
pub fn replace(erase: usize, text: &str, terminal: bool) -> Result<(), String> {
    typing()?.replace(erase, text, terminal)
}

/// See [`Typing::write`].
pub fn write(hotkeys: &[String], own_presses: Arc<AtomicU32>) -> Result<Box<dyn Writing>, String> {
    if !rewriting() {
        return Err(super::unsupported(marked("Writing words as they are spoken")));
    }
    typing()?.write(hotkeys, own_presses)
}

/// See [`Typing::composing`]. Without an adapter nothing is known: yes.
pub fn composing() -> bool {
    desktop().typing().is_none_or(Typing::composing)
}

/// See [`Typing::modifiers_held`].
pub fn modifiers_held() -> bool {
    desktop().typing().is_some_and(Typing::modifiers_held)
}

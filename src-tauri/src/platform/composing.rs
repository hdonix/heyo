//! Composing text in another application, as an input method does: for
//! dictated words, which are on screen while the sentence is still being
//! spoken (`crate::dictation`).
//!
//! An input method has the two things that takes. Text being composed is
//! shown in the application where the cursor is, underlined, and is not
//! the application's yet: whoever composes may put other text in its place
//! as often as it likes, and it is gone without a trace when the field
//! lets go of it. Committed text is the application's. So nothing is
//! erased in someone else's text field, nothing is borrowed from the
//! clipboard, and no key is pressed on the user's behalf.
//!
//! The session has one input method at a time, and composing takes its
//! place: for as long as a [`Composing`] lives, the user's own (Pinyin,
//! say) is set aside, and their keys reach the application as plain keys.
//! It is given back when the `Composing` is dropped, and [`recover`] gives
//! it back at the next start if Heyo did not live to do that.
//!
//! The X11 adapter speaks to IBus (`ibus`), which is what GNOME runs: on
//! Wayland too, where composing through it has not been tried. Another
//! framework (Fcitx), no framework, Windows: composing is refused, and the
//! caller pastes instead.

use crate::i18n::marked;
use super::desktop::desktop;
use super::Support;

/// What happens to the text being composed that is not the composer's doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
// Only the X11 adapter says any of it so far.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub enum Stir {
    /// The user pressed a key that is on its way to the field (neither a
    /// modifier by itself nor one of the keys named when composing began).
    /// What is said to the `Composing` now reaches the field before the key.
    Key,
    /// The field let go of what was being composed, and shows none of it
    /// any more: the user clicked in it, or moved its cursor. It can be
    /// shown again.
    Reset,
    /// The same, and no field is being composed in any more: it lost the
    /// focus.
    Left,
    /// A field has the focus, and is the one composed in from now on.
    Entered,
    /// The input method is no longer this one: the user or the desktop
    /// chose another, or it is gone. What was being composed is shown no
    /// more, and nothing more can be said.
    Gone,
}

/// Text being composed in the field that has the focus.
pub trait Composing: Send {
    /// Shows `text` as what is being composed, in place of what was shown
    /// before; none shows nothing.
    fn show(&mut self, text: &str) -> Result<(), String>;

    /// Makes `text` the field's own, at its cursor. What is being composed
    /// stays as it is shown.
    fn commit(&mut self, text: &str) -> Result<(), String>;
}

/// Hears what happens in the field, on a thread of the input method's:
/// what it says to the `Composing` it is handed takes effect before
/// whatever happened does (a key reaches the field after it).
pub type Hearing = Box<dyn FnMut(Stir, &mut dyn Composing) + Send>;

/// Whether text can be composed in other applications in this session, as
/// far as can be told without trying: the session has an adapter for it,
/// and its input method is running.
pub fn supported() -> bool {
    desktop().supported().composing != Support::None && desktop().can_compose()
}

/// Begins composing in the field that has the focus, taking the place of
/// the session's input method until the answer is dropped. `own_keys` are
/// keys that are pressed for Heyo and are not typing, by the names
/// `keys::KEYS` gives them (the key held to talk). Refuses,
/// saying why, where there is nothing to compose in: no input method that
/// Heyo speaks to, no field with the focus that takes one, a password.
pub fn compose(own_keys: &[&str], on: Hearing) -> Result<Box<dyn Composing>, String> {
    if desktop().supported().composing == Support::None {
        return Err(super::unsupported(marked("Composing text in another application")));
    }
    desktop().compose(own_keys, on)
}

/// Gives the session back the input method that a Heyo which did not end
/// properly had set aside. To be called once, when Heyo starts.
pub fn recover() {
    desktop().recover_input_method();
}

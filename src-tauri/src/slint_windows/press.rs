//! Who a press of the pointer belongs to, in a window of Slint's.
//!
//! Three have a say in what the pointer does in such a window: Slint,
//! which has the controls; the window's own rule (the overlay's
//! selection, a pinned capture that is dragged); and the desktop, which
//! moves a window by a press that it is handed. Each window had its own
//! way of keeping them apart. This is the one rule, kept by a [`Pointer`]
//! for each window:
//!
//! - A press belongs to whoever it went down on, until that button goes
//!   up, wherever the pointer goes meanwhile.
//! - With no button down, the window's own rule hears of every move;
//!   Slint hears of it too while the pointer is over what is Slint's,
//!   and is told that the pointer left when it leaves that.
//! - A press that is handed to the desktop is over at once: nobody is
//!   told when the button goes up. If the press was Slint's, Slint is
//!   told what it will not hear: the button is up, and the pointer away.
//!
//! A window says what it heard, in `standing`'s words ([`Pointed`]), and
//! answers one question: whose a place is for a button ([`Whose`]). What
//! is to be done comes back as a value ([`Given`]), which is what the
//! tests are of. A window never keeps a note of its own of who has a
//! press, and never tells Slint of a release or of the pointer leaving.

use super::standing::{Button, Pointed};

/// Who a place of a window belongs to, for a button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Whose {
    /// Something of Slint's answers there: a control.
    Slint,
    /// The window's own rule does.
    Window,
}

/// What is to be done with what was heard.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Given {
    /// Slint is to have it too.
    pub slint: bool,
    /// The window's own rule hears of it.
    pub window: bool,
    /// Slint is to be told that the pointer left what is its ([`left`]).
    pub left_slint: bool,
}

/// The pointer in one window: where it is, and whose each press is.
#[derive(Debug, Default)]
pub(super) struct Pointer {
    at: (f64, f64),
    /// The left button is down: whose the press is, and where it began.
    left: Option<(Whose, (f64, f64))>,
    right: Option<Whose>,
    /// With no button down, it is over what is Slint's.
    over_slint: bool,
    /// How many times it moved since the window's own press began.
    moves: u32,
}

impl Pointer {
    /// Where it is, as the window was last told, in its device pixels.
    pub(super) fn at(&self) -> (f64, f64) {
        self.at
    }

    /// Where the press began that is the window's own, while the left
    /// button is down in one.
    pub(super) fn press_from(&self) -> Option<(f64, f64)> {
        self.left.and_then(|(whose, from)| (whose == Whose::Window).then_some(from))
    }

    /// How many times the pointer moved since that press began.
    pub(super) fn moves(&self) -> u32 {
        self.moves
    }

    /// The pointer did something in the window. `whose` says whose a
    /// place is for a button.
    pub(super) fn heard(&mut self, pointed: Pointed, whose: impl Fn((f64, f64), Button) -> Whose) -> Given {
        let to = |whose: Whose| Given { slint: whose == Whose::Slint, window: whose == Whose::Window, left_slint: false };
        match pointed {
            Pointed::Moved(at) => {
                self.at = at;
                match self.left {
                    Some((Whose::Window, _)) => {
                        self.moves += 1;
                        to(Whose::Window)
                    }
                    Some((Whose::Slint, _)) => to(Whose::Slint),
                    None => {
                        let over = whose(at, Button::Left) == Whose::Slint;
                        let left_slint = std::mem::replace(&mut self.over_slint, over) && !over;
                        Given { slint: over, window: true, left_slint }
                    }
                }
            }
            Pointed::Pressed(Button::Left) => {
                let press = whose(self.at, Button::Left);
                (self.left, self.moves) = (Some((press, self.at)), 0);
                to(press)
            }
            Pointed::Pressed(Button::Right) => {
                let press = whose(self.at, Button::Right);
                self.right = Some(press);
                to(press)
            }
            // A button that goes up without having gone down here is nobody's.
            Pointed::Released(Button::Left) => self.left.take().map(|(press, _)| to(press)).unwrap_or_default(),
            Pointed::Released(Button::Right) => self.right.take().map(to).unwrap_or_default(),
            // Out of the window: Slint hears of that itself if it was over
            // what is Slint's. A press goes on: the pointer is still heard of.
            Pointed::Left => Given { slint: std::mem::take(&mut self.over_slint), window: true, left_slint: false },
        }
    }

    /// The desktop was asked to move the window by the press: the press
    /// is its from now on, and over here. True when it was Slint's, which
    /// is then to be told ([`let_go`]).
    pub(super) fn handed_to_desktop(&mut self) -> bool {
        matches!(self.left.take(), Some((Whose::Slint, _)))
    }

    /// The window is off the screen: no button is down in it.
    pub(super) fn forget(&mut self) {
        *self = Self { at: self.at, ..Self::default() };
    }
}

/// Tells Slint that the pointer left what is its.
pub(super) fn left(window: &slint::Window) {
    let _ = window.dispatch_event_with_result(slint::platform::WindowEvent::PointerExited);
}

/// Tells Slint what it will not hear of a press the desktop took: the
/// button is up, and the pointer is away until it is next heard of.
/// Slint would go on taking what was pressed for pressed, and hand it
/// every press after. `at` is where the pointer is, in the window's
/// device pixels.
pub(super) fn let_go(window: &slint::Window, at: (f64, f64)) {
    let scale = f64::from(window.scale_factor()).max(0.1);
    let position = slint::LogicalPosition::new((at.0 / scale) as f32, (at.1 / scale) as f32);
    let _ = window.dispatch_event_with_result(slint::platform::WindowEvent::PointerReleased { position, button: slint::platform::PointerEventButton::Left });
    left(window);
}

#[cfg(test)]
mod tests {
    use super::*;

    const SLINT: Given = Given { slint: true, window: false, left_slint: false };
    const WINDOW: Given = Given { slint: false, window: true, left_slint: false };
    const NOBODY: Given = Given { slint: false, window: false, left_slint: false };

    /// A window whose left hundred pixels are Slint's, for the left button.
    fn whose(at: (f64, f64), button: Button) -> Whose {
        if button == Button::Left && at.0 < 100.0 { Whose::Slint } else { Whose::Window }
    }

    fn heard(pointer: &mut Pointer, pointed: Pointed) -> Given {
        pointer.heard(pointed, whose)
    }

    #[test]
    fn a_press_on_slints_part_is_slints_until_the_button_goes_up() {
        let mut pointer = Pointer::default();
        heard(&mut pointer, Pointed::Moved((50.0, 10.0)));
        assert_eq!(heard(&mut pointer, Pointed::Pressed(Button::Left)), SLINT);
        // It wanders off what is Slint's: still Slint's, and the window's rule hears nothing.
        assert_eq!(heard(&mut pointer, Pointed::Moved((300.0, 10.0))), SLINT);
        assert_eq!(pointer.press_from(), None);
        assert_eq!(heard(&mut pointer, Pointed::Released(Button::Left)), SLINT);
        // And then it is where the window's own rule hears alone.
        assert_eq!(heard(&mut pointer, Pointed::Moved((301.0, 10.0))), Given { left_slint: true, ..WINDOW });
        assert_eq!(heard(&mut pointer, Pointed::Moved((302.0, 10.0))), WINDOW);
    }

    #[test]
    fn a_press_on_the_windows_part_is_the_windows_across_slints() {
        let mut pointer = Pointer::default();
        heard(&mut pointer, Pointed::Moved((300.0, 10.0)));
        assert_eq!(heard(&mut pointer, Pointed::Pressed(Button::Left)), WINDOW);
        assert_eq!(pointer.press_from(), Some((300.0, 10.0)));
        // Across what is Slint's: Slint hears nothing of it.
        assert_eq!(heard(&mut pointer, Pointed::Moved((50.0, 10.0))), WINDOW);
        assert_eq!(heard(&mut pointer, Pointed::Moved((40.0, 10.0))), WINDOW);
        assert_eq!(pointer.moves(), 2);
        assert_eq!(heard(&mut pointer, Pointed::Released(Button::Left)), WINDOW);
        assert_eq!(pointer.press_from(), None);
        // Let go over what is Slint's: Slint hears of the pointer from the next move.
        assert_eq!(heard(&mut pointer, Pointed::Moved((41.0, 10.0))), Given { slint: true, window: true, left_slint: false });
    }

    #[test]
    fn with_no_button_down_slint_is_told_once_that_the_pointer_left_it() {
        let mut pointer = Pointer::default();
        assert_eq!(heard(&mut pointer, Pointed::Moved((50.0, 10.0))), Given { slint: true, window: true, left_slint: false });
        assert_eq!(heard(&mut pointer, Pointed::Moved((150.0, 10.0))), Given { left_slint: true, ..WINDOW });
        assert_eq!(heard(&mut pointer, Pointed::Moved((160.0, 10.0))), WINDOW);
        // Out of the window from Slint's part: Slint hears of that itself.
        heard(&mut pointer, Pointed::Moved((50.0, 10.0)));
        assert_eq!(heard(&mut pointer, Pointed::Left), Given { slint: true, window: true, left_slint: false });
        assert_eq!(heard(&mut pointer, Pointed::Left), WINDOW);
    }

    #[test]
    fn a_press_handed_to_the_desktop_is_over() {
        // Slint's, as when the settings are moved by their margin: Slint is to be let go.
        let mut pointer = Pointer::default();
        heard(&mut pointer, Pointed::Moved((50.0, 10.0)));
        heard(&mut pointer, Pointed::Pressed(Button::Left));
        assert!(pointer.handed_to_desktop());
        // Nothing is pressed any more, and the button going up later is nobody's.
        assert_eq!(heard(&mut pointer, Pointed::Moved((60.0, 10.0))), Given { slint: true, window: true, left_slint: false });
        assert_eq!(heard(&mut pointer, Pointed::Released(Button::Left)), NOBODY);
        // The window's own, as a pinned capture's: there is nothing to tell Slint.
        heard(&mut pointer, Pointed::Moved((300.0, 10.0)));
        heard(&mut pointer, Pointed::Pressed(Button::Left));
        assert!(!pointer.handed_to_desktop());
        assert_eq!(pointer.press_from(), None);
        // And with nothing pressed there is nothing to hand over.
        assert!(!pointer.handed_to_desktop());
    }

    #[test]
    fn the_other_button_is_whose_the_window_says_and_apart_from_the_first() {
        let mut pointer = Pointer::default();
        heard(&mut pointer, Pointed::Moved((50.0, 10.0)));
        // Over what is Slint's for the left button, the right one is the window's.
        assert_eq!(heard(&mut pointer, Pointed::Pressed(Button::Right)), WINDOW);
        assert_eq!(pointer.press_from(), None);
        assert_eq!(heard(&mut pointer, Pointed::Released(Button::Right)), WINDOW);
        assert_eq!(heard(&mut pointer, Pointed::Released(Button::Right)), NOBODY);
    }

    #[test]
    fn a_window_that_left_the_screen_has_no_button_down() {
        let mut pointer = Pointer::default();
        heard(&mut pointer, Pointed::Moved((300.0, 10.0)));
        heard(&mut pointer, Pointed::Pressed(Button::Left));
        pointer.forget();
        assert_eq!((pointer.press_from(), pointer.at()), (None, (300.0, 10.0)));
        assert_eq!(heard(&mut pointer, Pointed::Released(Button::Left)), NOBODY);
    }
}

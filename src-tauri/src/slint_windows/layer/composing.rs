//! Text from the user's input method, for a layer surface that has the
//! keyboard and a line being typed in it.
//!
//! Whoever types Chinese, or anything else a keyboard has no keys for,
//! types through an input method: a program of the desktop's that shows
//! what is being composed and hands over the text when it is chosen. On
//! Wayland a window speaks to it through the compositor (the protocol
//! text-input, version 3): it says when something in it takes text and
//! where the cursor is, and is told what is being composed and what was
//! chosen. winit does that for its windows; this does it for a surface of
//! Heyo's own, and hands both to Slint the way Slint's own backend does.
//!
//! What is not said: the text around the cursor (an input method that
//! corrects what was already typed has nothing to go by, and one that
//! asks for some of it to be deleted is not obeyed).

use smithay_client_toolkit::reexports::protocols::wp::text_input::zv3::client::zwp_text_input_manager_v3::ZwpTextInputManagerV3;
use smithay_client_toolkit::reexports::protocols::wp::text_input::zv3::client::zwp_text_input_v3::{self, ContentHint, ContentPurpose, ZwpTextInputV3};
use wayland_client::globals::GlobalList;
use wayland_client::protocol::{wl_seat, wl_surface};
use wayland_client::{Connection, Dispatch, QueueHandle};

use super::{Said, State};

/// Where the cursor of the line being typed is, in the surface's logical
/// pixels: left, top, width, height.
pub(super) type Cursor = (i32, i32, i32, i32);

#[derive(Default)]
pub(super) struct Composing {
    input: Option<ZwpTextInputV3>,
    /// The surface the input method's text goes to: the one with the keyboard.
    on: Option<wl_surface::WlSurface>,
    /// Something in the window takes text, with its cursor there.
    wanted: Option<Cursor>,
    /// The compositor has been told so.
    enabled: bool,
    /// What the input method said since it last said it was done: what
    /// is being composed, with the part of it that is marked, and what
    /// was chosen.
    composed: Option<(String, i32, i32)>,
    chosen: Option<String>,
}

impl Composing {
    /// Asks for the seat's text input, where the compositor has such a thing.
    pub(super) fn new(globals: &GlobalList, handle: &QueueHandle<State>, seat: Option<&wl_seat::WlSeat>) -> Self {
        let input = seat.and_then(|seat| globals.bind::<ZwpTextInputManagerV3, _, _>(handle, 1..=1, ()).ok().map(|manager| manager.get_text_input(seat, handle, ())));
        Self { input, ..Self::default() }
    }

    /// Something in the window takes text now, with its cursor there; or
    /// nothing does any more.
    pub(super) fn want(&mut self, cursor: Option<Cursor>) {
        self.wanted = cursor;
        self.say();
    }

    /// Says to the compositor what is wanted, once the surface is the one
    /// text goes to.
    fn say(&mut self) {
        let Some(input) = self.input.as_ref() else { return };
        if self.on.is_none() {
            return;
        }
        match self.wanted {
            Some((x, y, width, height)) => {
                if !self.enabled {
                    input.enable();
                    input.set_content_type(ContentHint::None, ContentPurpose::Normal);
                }
                input.set_cursor_rectangle(x, y, width, height);
                input.commit();
                self.enabled = true;
            }
            None if self.enabled => {
                input.disable();
                input.commit();
                self.enabled = false;
            }
            None => {}
        }
    }
}

impl Dispatch<ZwpTextInputV3, ()> for State {
    fn event(state: &mut Self, _: &ZwpTextInputV3, event: zwp_text_input_v3::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        use zwp_text_input_v3::Event;
        let composing = &mut state.composing;
        match event {
            Event::Enter { surface } => {
                composing.on = Some(surface);
                composing.enabled = false;
                composing.say();
            }
            Event::Leave { surface } => {
                // What was being composed is not the window's any more.
                (composing.on, composing.enabled, composing.composed, composing.chosen) = (None, false, None, None);
                state.said.push(Said::Composed(surface, None, None));
            }
            Event::PreeditString { text, cursor_begin, cursor_end } => composing.composed = text.map(|text| (text, cursor_begin, cursor_end)),
            Event::CommitString { text } => composing.chosen = text,
            Event::Done { .. } => {
                if let Some(surface) = composing.on.clone() {
                    let said = Said::Composed(surface, composing.chosen.take(), composing.composed.take());
                    state.said.push(said);
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwpTextInputManagerV3, ()> for State {
    fn event(_: &mut Self, _: &ZwpTextInputManagerV3, _: <ZwpTextInputManagerV3 as wayland_client::Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

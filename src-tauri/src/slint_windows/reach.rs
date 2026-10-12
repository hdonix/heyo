//! Where in its window the settings window takes the pointer.
//!
//! It is larger than the panel it draws: the margin round the panel is
//! there for the shadow to fall in, and to whoever looks at the screen it
//! is not the window at all. A press there belongs to what is seen there,
//! which is whatever is underneath. This is where the panel is; having
//! the desktop go by it is `standing::reach`.

use crate::platform::place::Rect;

/// The panel in a window of `size` device pixels that keeps `margin`
/// logical pixels round it, drawn at `scale`. None when the window keeps
/// no margin, or is all margin: it is then all of it that takes the
/// pointer.
pub(super) fn panel(size: (u32, u32), margin: f32, scale: f32) -> Option<Rect> {
    let edge = (margin * scale).round().max(0.0) as u32;
    (edge > 0 && size.0 > 2 * edge && size.1 > 2 * edge).then(|| (edge as i32, edge as i32, size.0 - 2 * edge, size.1 - 2 * edge))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_panel_is_the_window_without_its_margin() {
        // The settings window as it is at scale 1 and at scale 2.
        assert_eq!(panel((924, 704), 32.0, 1.0), Some((32, 32, 860, 640)));
        assert_eq!(panel((1848, 1408), 32.0, 2.0), Some((64, 64, 1720, 1280)));
        // At a scale between, the margin is whole pixels each side.
        assert_eq!(panel((1155, 880), 32.0, 1.25), Some((40, 40, 1075, 800)));
    }

    #[test]
    fn a_window_without_a_margin_takes_the_pointer_everywhere() {
        // On a screen too small for the shadow.
        assert_eq!(panel((860, 640), 0.0, 1.0), None);
        // And one that would be nothing but margin.
        assert_eq!(panel((60, 60), 32.0, 1.0), None);
    }
}

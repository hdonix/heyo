//! What is drawn on the capture, as the markup is given it: each
//! annotation's box on the screen and what goes in the box. A line's path
//! is written as path commands, a blurred part is a small picture of the
//! capture there, a pixel a block.

use slint::{Image, Rgba8Pixel, SharedPixelBuffer};

use super::super::ui;
use crate::screenshot::selecting::{Point, Rect, Shape, Tool, ARROW_HEAD, BLUR_BLOCK, COLOURS, TEXT_LINE};

/// How much wider than its points a line's box is each way: its width, an
/// arrow's head and its round ends are inside the box, where the renderer
/// looks for what has changed.
const ROOM: f64 = 16.0;

/// One of the colours annotations are drawn in.
pub(super) fn colour(index: usize) -> slint::Color {
    let (red, green, blue) = COLOURS[index.min(COLOURS.len() - 1)];
    slint::Color::from_rgb_u8(red, green, blue)
}

/// An annotation as it is drawn. `clear` is the capture as it was read,
/// with `scale` of its pixels to a logical one: a blurred part is made
/// from it.
pub(super) fn mark(shape: &Shape, clear: Option<&SharedPixelBuffer<Rgba8Pixel>>, scale: f64) -> ui::Mark {
    let (first, last) = (shape.points.first().copied().unwrap_or_default(), shape.points.last().copied().unwrap_or_default());
    let boxed = match shape.tool {
        Tool::Arrow | Tool::Pen => bounds(&shape.points),
        Tool::Text => Rect { x: first.0, y: first.1, width: shape.width, height: TEXT_LINE },
        Tool::Rect | Tool::Blur | Tool::Select => shape.span(),
    };
    let within = |point: Point| (point.0 - boxed.x, point.1 - boxed.y);
    let (kind, path, head) = match shape.tool {
        Tool::Rect => ("rect", String::new(), String::new()),
        Tool::Blur => ("blur", String::new(), String::new()),
        Tool::Text => ("text", String::new(), String::new()),
        Tool::Pen => ("pen", line(shape.points.iter().copied().map(within)), String::new()),
        Tool::Arrow => {
            let (shaft, head) = arrow(within(first), within(last));
            ("arrow", shaft, head)
        }
        Tool::Select => ("", String::new(), String::new()),
    };
    let picture = match (shape.tool, clear) {
        (Tool::Blur, Some(clear)) => blurred(clear, boxed, scale).map(Image::from_rgba8_premultiplied).unwrap_or_default(),
        _ => Image::default(),
    };
    ui::Mark {
        kind: kind.into(),
        colour: colour(shape.colour),
        x: boxed.x as f32,
        y: boxed.y as f32,
        width: boxed.width as f32,
        height: boxed.height as f32,
        path: path.into(),
        head: head.into(),
        text: shape.text.as_str().into(),
        picture,
    }
}

/// The box that holds a line, with room round it.
fn bounds(points: &[Point]) -> Rect {
    let low = points.iter().fold((f64::MAX, f64::MAX), |low, point| (low.0.min(point.0), low.1.min(point.1)));
    let high = points.iter().fold((f64::MIN, f64::MIN), |high, point| (high.0.max(point.0), high.1.max(point.1)));
    if points.is_empty() {
        return Rect { x: 0.0, y: 0.0, width: 0.0, height: 0.0 };
    }
    Rect { x: low.0 - ROOM, y: low.1 - ROOM, width: high.0 - low.0 + 2.0 * ROOM, height: high.1 - low.1 + 2.0 * ROOM }
}

/// A path through the points, as path commands. A point that is where the
/// one before it was is left out.
fn line(points: impl Iterator<Item = Point>) -> String {
    use std::fmt::Write;
    let mut commands = String::new();
    let mut before = None;
    for point in points {
        if before == Some(point) {
            continue;
        }
        let _ = write!(commands, "{}{:.1} {:.1}", if before.is_none() { "M" } else { " L" }, point.0, point.1);
        before = Some(point);
    }
    // One place only: a dot, which a line to where it is draws with round ends.
    if let (Some(only), false) = (before, commands.contains('L')) {
        let _ = write!(commands, " L{:.1} {:.1}", only.0, only.1);
    }
    commands
}

/// An arrow from one point to another: its shaft, which stops short of
/// the tip inside the head, and its head, as path commands.
fn arrow(from: Point, to: Point) -> (String, String) {
    let angle = (to.1 - from.1).atan2(to.0 - from.0);
    let back = |by: f64, turned: f64| (to.0 - by * (angle + turned).cos(), to.1 - by * (angle + turned).sin());
    let (neck, left, right) = (back(ARROW_HEAD * 0.6, 0.0), back(ARROW_HEAD, -0.45), back(ARROW_HEAD, 0.45));
    (
        format!("M{:.1} {:.1} L{:.1} {:.1}", from.0, from.1, neck.0, neck.1),
        format!("M{:.1} {:.1} L{:.1} {:.1} L{:.1} {:.1} Z", to.0, to.1, left.0, left.1, right.0, right.1),
    )
}

/// The capture in `part` (logical pixels) as blocks: a picture with a
/// pixel for each block of `BLUR_BLOCK`, which is shown not smoothed. Each
/// block is the mean of nine places in it. None for a part of no size.
fn blurred(clear: &SharedPixelBuffer<Rgba8Pixel>, part: Rect, scale: f64) -> Option<SharedPixelBuffer<Rgba8Pixel>> {
    if part.width < 1.0 || part.height < 1.0 || clear.width() == 0 || clear.height() == 0 {
        return None;
    }
    let (across, down) = (((part.width / BLUR_BLOCK).round() as u32).max(1), ((part.height / BLUR_BLOCK).round() as u32).max(1));
    let (width, height) = (clear.width() as i64, clear.height() as i64);
    let pixels = clear.as_slice();
    // A place in the part, as shares of its width and height, in the capture.
    let read = |share: (f64, f64)| {
        let x = (((part.x + part.width * share.0) * scale) as i64).clamp(0, width - 1);
        let y = (((part.y + part.height * share.1) * scale) as i64).clamp(0, height - 1);
        pixels[(y * width + x) as usize]
    };
    let mut blocks = SharedPixelBuffer::<Rgba8Pixel>::new(across, down);
    for (row, line) in blocks.make_mut_slice().chunks_exact_mut(across as usize).enumerate() {
        for (column, block) in line.iter_mut().enumerate() {
            let mut sum = [0u32; 3];
            for inner in 0..9 {
                let place = ((column as f64 + (inner % 3) as f64 / 3.0 + 1.0 / 6.0) / across as f64, (row as f64 + (inner / 3) as f64 / 3.0 + 1.0 / 6.0) / down as f64);
                let pixel = read(place);
                sum = [sum[0] + pixel.r as u32, sum[1] + pixel.g as u32, sum[2] + pixel.b as u32];
            }
            *block = Rgba8Pixel { r: (sum[0] / 9) as u8, g: (sum[1] / 9) as u8, b: (sum[2] / 9) as u8, a: 255 };
        }
    }
    Some(blocks)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shape(tool: Tool, points: &[Point]) -> Shape {
        Shape { tool, colour: 3, points: points.to_vec(), text: String::new(), width: 0.0 }
    }

    #[test]
    fn a_line_is_drawn_in_a_box_with_room_round_it() {
        let pen = mark(&shape(Tool::Pen, &[(100.0, 200.0), (100.0, 200.0), (130.5, 210.0), (120.0, 260.0)]), None, 1.0);
        assert_eq!((pen.kind.as_str(), pen.x, pen.y, pen.width, pen.height), ("pen", 84.0, 184.0, 62.5, 92.0));
        // Its path within the box; the place it stood at twice is there once.
        assert_eq!(pen.path.as_str(), "M16.0 16.0 L46.5 26.0 L36.0 76.0");
        assert_eq!(pen.colour, slint::Color::from_rgb_u8(0x56, 0xc2, 0xff));
        // A press that has not moved yet is a dot.
        assert_eq!(mark(&shape(Tool::Pen, &[(100.0, 200.0), (100.0, 200.0)]), None, 1.0).path.as_str(), "M16.0 16.0 L16.0 16.0");
    }

    #[test]
    fn an_arrow_has_a_shaft_that_ends_inside_its_head() {
        // To the right: the head's two corners are behind the tip, above and below it.
        let drawn = mark(&shape(Tool::Arrow, &[(100.0, 100.0), (200.0, 100.0)]), None, 1.0);
        assert_eq!((drawn.x, drawn.y, drawn.width, drawn.height), (84.0, 84.0, 132.0, 32.0));
        assert_eq!(drawn.path.as_str(), "M16.0 16.0 L107.6 16.0");
        assert_eq!(drawn.head.as_str(), "M116.0 16.0 L103.4 22.1 L103.4 9.9 Z");
    }

    #[test]
    fn a_rectangle_and_text_are_where_they_were_put() {
        let outline = mark(&shape(Tool::Rect, &[(300.0, 250.0), (200.0, 400.0)]), None, 1.0);
        assert_eq!((outline.kind.as_str(), outline.x, outline.y, outline.width, outline.height), ("rect", 200.0, 250.0, 100.0, 150.0));
        let words = mark(&Shape { tool: Tool::Text, colour: 0, points: vec![(50.0, 60.0)], text: "Look".into(), width: 44.0 }, None, 1.0);
        assert_eq!((words.kind.as_str(), words.text.as_str(), words.x, words.y, words.width, words.height), ("text", "Look", 50.0, 60.0, 44.0, 24.0));
    }

    #[test]
    fn a_blurred_part_is_the_capture_there_in_blocks() {
        // 36 by 18 pixels, the left half black and the right half white.
        let mut clear = SharedPixelBuffer::<Rgba8Pixel>::new(36, 18);
        for (at, pixel) in clear.make_mut_slice().iter_mut().enumerate() {
            let light = if at % 36 < 18 { 0 } else { 255 };
            *pixel = Rgba8Pixel { r: light, g: light, b: light, a: 255 };
        }
        // All of it: four blocks across and two down, each wholly one or the other.
        let blocks = blurred(&clear, Rect { x: 0.0, y: 0.0, width: 36.0, height: 18.0 }, 1.0).unwrap();
        assert_eq!((blocks.width(), blocks.height()), (4, 2));
        assert_eq!(blocks.as_slice().iter().map(|pixel| pixel.r).collect::<Vec<_>>(), [0, 0, 255, 255, 0, 0, 255, 255]);
        // A block across the edge is between the two.
        let across = blurred(&clear, Rect { x: 13.5, y: 0.0, width: 9.0, height: 9.0 }, 1.0).unwrap();
        assert_eq!((across.width(), across.height()), (1, 1));
        assert!((80..180).contains(&across.as_slice()[0].r), "{:?}", across.as_slice()[0]);
        // At twice the pixels to a logical one, the same part of the screen.
        let twice = blurred(&clear, Rect { x: 9.0, y: 0.0, width: 9.0, height: 9.0 }, 2.0).unwrap();
        assert_eq!(twice.as_slice()[0].r, 255);
        // Nothing of no size; and past the capture's edge its last pixels.
        assert!(blurred(&clear, Rect { x: 0.0, y: 0.0, width: 0.5, height: 9.0 }, 1.0).is_none());
        assert_eq!(blurred(&clear, Rect { x: 30.0, y: 10.0, width: 90.0, height: 90.0 }, 1.0).unwrap().as_slice()[99].r, 255);
    }
}

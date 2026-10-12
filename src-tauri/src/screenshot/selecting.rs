//! Selecting part of a frozen screen: what the screenshot overlay shows and
//! how it answers the pointer and the keys, when the overlay is drawn by
//! the host and not by the web view.
//!
//! It is told what happened (`Event`) and answers with what is to become of
//! the capture, if anything (`Outcome`); what is on the screen meanwhile is
//! asked for as a `Look`. It draws nothing and knows no toolkit and no
//! renderer, so its rules are tested with numbers. Everything is in the
//! overlay's logical pixels unless it says otherwise.
//!
//! The rules are those of the web view's overlay (`src/Screenshot.tsx`): a
//! drag selects, a click takes the window under it, the selection is moved
//! by its inside and resized by its handles, Enter copies it. With a tool
//! chosen from the toolbar a drag in the selection draws on it instead.

/// A selection smaller than this, either way, is a click, not a selection.
pub const MIN_SIZE: f64 = 6.0;
/// How far from a handle's middle a press still takes it.
const GRIP: f64 = 7.0;
/// The loupe shows this many of the capture's pixels across, each this many
/// logical pixels wide, this far from the pointer.
pub const LOUPE_CELLS: u32 = 15;
pub const LOUPE_CELL: f64 = 8.0;
const LOUPE_GAP: f64 = 20.0;
/// The size label's box: how high it is, and how far from the selection.
pub const LABEL_HEIGHT: f64 = 20.0;
const LABEL_GAP: f64 = 4.0;
/// The colours annotations are drawn in: red, yellow, green, blue.
pub const COLOURS: [(u8, u8, u8); 4] = [(0xff, 0x45, 0x3a), (0xff, 0xc5, 0x31), (0x59, 0xd4, 0x99), (0x56, 0xc2, 0xff)];
/// The height of a line of typed text, and the length of an arrow's head.
/// (The text's size and the width of a drawn line are the markup's.)
pub const TEXT_LINE: f64 = 24.0;
pub const ARROW_HEAD: f64 = 14.0;
/// The side of the blocks a blurred part is made of.
pub const BLUR_BLOCK: f64 = 9.0;
/// How far from an annotation a press still takes it.
const SLACK: f64 = 7.0;
/// The toolbar's size (fifteen buttons of 30 with 2 between them, a
/// separator of 9, 4 of padding and a border), how far it is from the
/// selection and from the screen's edge, and the room it needs below.
pub const TOOLBAR: (f64, f64) = (499.0, 40.0);
const TOOLBAR_GAP: f64 = 8.0;
const TOOLBAR_ROOM: f64 = 52.0;

pub type Point = (f64, f64);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    fn between(a: Point, b: Point) -> Self {
        Self { x: a.0.min(b.0), y: a.1.min(b.1), width: (a.0 - b.0).abs(), height: (a.1 - b.1).abs() }
    }

    pub fn right(&self) -> f64 {
        self.x + self.width
    }

    pub fn bottom(&self) -> f64 {
        self.y + self.height
    }

    pub fn contains(&self, p: Point) -> bool {
        p.0 >= self.x && p.0 <= self.right() && p.1 >= self.y && p.1 <= self.bottom()
    }

    /// The middles of the handles, from the top left corner clockwise, each
    /// with the sides it moves: left or right (-1, 1) and top or bottom.
    pub fn handles(&self) -> [(Point, (i8, i8)); 8] {
        let (left, top, right, bottom) = (self.x, self.y, self.right(), self.bottom());
        let (across, down) = (self.x + self.width / 2.0, self.y + self.height / 2.0);
        [
            ((left, top), (-1, -1)),
            ((across, top), (0, -1)),
            ((right, top), (1, -1)),
            ((right, down), (1, 0)),
            ((right, bottom), (1, 1)),
            ((across, bottom), (0, 1)),
            ((left, bottom), (-1, 1)),
            ((left, down), (-1, 0)),
        ]
    }
}

/// What the capture is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// The user's own screenshot: the selection can be adjusted, and a key
    /// says what becomes of it.
    Capture,
    /// Text is read from the selection as soon as it is made.
    Text,
    /// An extension asked for an area: making the selection is all there is.
    Pick,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Primary,
    Secondary,
}

/// What a press in the selection does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    /// Moves the selection, or an annotation.
    Select,
    Rect,
    Arrow,
    Pen,
    Text,
    Blur,
}

/// Something drawn on the capture, where it is on the screen (not in the
/// selection: the selection can be moved from under it, and cuts it off
/// at its edge).
#[derive(Clone, Debug, PartialEq)]
pub struct Shape {
    pub tool: Tool,
    /// Which of `COLOURS`.
    pub colour: usize,
    /// A rectangle's or a blurred part's two corners, an arrow's two ends,
    /// the pen's path, or the top left corner of text.
    pub points: Vec<Point>,
    /// For text: what it says, and how wide that is as it is drawn.
    pub text: String,
    pub width: f64,
}

impl Shape {
    /// The rectangle between its first and last points.
    pub fn span(&self) -> Rect {
        match (self.points.first(), self.points.last()) {
            (Some(first), Some(last)) => Rect::between(*first, *last),
            _ => Rect { x: 0.0, y: 0.0, width: 0.0, height: 0.0 },
        }
    }

    /// Whether a point is on it, with a little room round thin lines.
    fn hit(&self, p: Point) -> bool {
        let within = |r: Rect| p.0 >= r.x - SLACK && p.0 <= r.right() + SLACK && p.1 >= r.y - SLACK && p.1 <= r.bottom() + SLACK;
        match self.tool {
            Tool::Select => false,
            Tool::Text => self.points.first().is_some_and(|at| within(Rect { x: at.0, y: at.1, width: self.width, height: TEXT_LINE })),
            Tool::Blur => within(self.span()),
            Tool::Rect => {
                // The outline, not the inside: what is inside may be another annotation.
                let r = self.span();
                let corners = [(r.x, r.y), (r.right(), r.y), (r.right(), r.bottom()), (r.x, r.bottom())];
                (0..4).any(|i| distance_to_segment(p, corners[i], corners[(i + 1) % 4]) <= SLACK)
            }
            // Near any stretch of the line.
            Tool::Arrow | Tool::Pen => self.points.windows(2).any(|ends| distance_to_segment(p, ends[0], ends[1]) <= SLACK),
        }
    }
}

fn distance_to_segment(p: Point, a: Point, b: Point) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let length = dx * dx + dy * dy;
    let along = if length > 0.0 { (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / length).clamp(0.0, 1.0) } else { 0.0 };
    (p.0 - (a.0 + along * dx)).hypot(p.1 - (a.1 + along * dy))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Escape,
    Enter,
    F4,
    /// A key that makes a character, as the character it makes unshifted.
    Character(char),
}

/// What the overlay's window was told by the desktop, or by its own
/// toolbar, in the overlay's own words. Each platform's window turns its
/// events into these.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Pressed { at: Point, button: Button },
    Moved { to: Point },
    Released { button: Button },
    /// The pointer is no longer over the overlay.
    Left,
    Key { key: Key, ctrl: bool, alt: bool },
    /// A tool's button: chosen, or put down again if it was the one in use.
    Tool(Tool),
    /// One of `COLOURS` was chosen.
    Colour(usize),
    Undo,
    /// A button that says what is to become of the selection.
    Do(Action),
    Cancel,
    /// The text being typed is done with: it is added where it was typed
    /// (if it says anything), `width` wide as it was drawn.
    Typed { text: String, width: f64 },
}

/// What is to be done with the selected part.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Copy,
    Save,
    /// Kept on the screen, in a window of its own.
    Pin,
    Text,
    Pick,
}

/// What the user decided.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Outcome {
    Cancel,
    /// `area` is in the capture's own pixels.
    Finish { action: Action, area: Area },
}

/// A rectangle of the capture, in its pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Area {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// What a press began.
#[derive(Clone, Debug, PartialEq)]
enum Drag {
    /// A new selection, from where the pointer went down.
    Out { from: Point },
    /// The selection as it was, moved with the pointer.
    Move { from: Point, was: Rect },
    /// The sides a handle moves, moved with the pointer.
    Resize { from: Point, was: Rect, sides: (i8, i8) },
    /// The last annotation, which is being drawn.
    Draw,
    /// The last annotation, as it was, moved with the pointer.
    Carry { from: Point, was: Vec<Point> },
}

/// The pointer's shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cursor {
    Crosshair,
    Move,
    /// Resizing by the handle that moves these sides.
    Resize(i8, i8),
    /// Over an annotation that a press would take hold of.
    Grab,
    /// Over the toolbar.
    Arrow,
}

/// A line shown at the bottom of the overlay.
#[derive(Clone, Debug, PartialEq)]
pub struct Status {
    pub text: String,
    pub error: bool,
}

/// The magnifier beside the pointer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Loupe {
    /// Where it is drawn.
    pub frame: Rect,
    /// The top left of the capture's pixels it shows, `LOUPE_CELLS` each
    /// way: the pointer's own pixel is the middle one.
    pub source: (i64, i64),
}

/// What the overlay shows, over the frozen screen.
#[derive(Clone, Debug, PartialEq)]
pub struct Look {
    /// The part of the screen shown as it is: everything else is dimmed.
    pub clear: Option<Rect>,
    /// Whether the outline round it is drawn inside it (a window that may
    /// fill the screen) or outside (the selection).
    pub outline_inside: bool,
    /// The middles of the handles, where the selection has them.
    pub handles: Vec<Point>,
    /// The selection's size in the capture's pixels, and the top left corner
    /// of its box.
    pub label: Option<(String, Point)>,
    /// What to do, in the middle of the screen, while nothing is selected.
    pub hint: Option<String>,
    pub status: Option<Status>,
    pub loupe: Option<Loupe>,
    /// Where the toolbar is, when the selection has one.
    pub toolbar: Option<Rect>,
    /// Where text is being typed: the top left corner of its line.
    pub typing: Option<Point>,
}

pub struct Selecting {
    /// The overlay's size: the selection stays inside it.
    size: (f64, f64),
    /// The capture's size in its own pixels.
    capture: (u32, u32),
    mode: Mode,
    /// The windows in the capture, the topmost first.
    windows: Vec<Rect>,
    rect: Option<Rect>,
    drag: Option<Drag>,
    /// The window a click would select: one of `windows`.
    hover: Option<usize>,
    pointer: Option<Point>,
    status: Option<Status>,
    /// The selection has been handed over, and nothing more is taken.
    busy: bool,
    tool: Tool,
    colour: usize,
    /// What has been drawn, the topmost last.
    shapes: Vec<Shape>,
    typing: Option<Point>,
}

impl Selecting {
    /// An overlay of `size` showing a capture of `capture` pixels.
    pub fn new(size: (f64, f64), capture: (u32, u32), mode: Mode) -> Self {
        Self { size, capture, mode, windows: Vec::new(), rect: None, drag: None, hover: None, pointer: None, status: None, busy: false, tool: Tool::Select, colour: 0, shapes: Vec::new(), typing: None }
    }

    pub fn tool(&self) -> Tool {
        self.tool
    }

    pub fn colour(&self) -> usize {
        self.colour
    }

    /// What has been drawn, the topmost last.
    pub fn shapes(&self) -> &[Shape] {
        &self.shapes
    }

    /// Where text is being typed: the top left corner of its line.
    pub fn typing(&self) -> Option<Point> {
        self.typing
    }

    /// How many of the capture's pixels one logical pixel is, measured and
    /// not assumed: the overlay covers the capture exactly.
    fn density(&self) -> f64 {
        if self.size.0 > 0.0 {
            self.capture.0 as f64 / self.size.0
        } else {
            1.0
        }
    }

    /// The windows in the capture, the topmost first, in the capture's
    /// pixels. They arrive after the overlay is up.
    pub fn windows(&mut self, in_capture: &[Area]) {
        let density = self.density();
        // Edges are converted, not sizes, so that windows that touch in the
        // capture touch in the overlay.
        self.windows = in_capture
            .iter()
            .map(|w| {
                let (left, top) = (w.x as f64 / density, w.y as f64 / density);
                Rect { x: left, y: top, width: (w.x + w.width) as f64 / density - left, height: (w.y + w.height) as f64 / density - top }
            })
            .collect();
        self.hover = self.hovered();
    }

    /// What became of the selection that was handed over could not be done:
    /// the overlay stays, and says why.
    pub fn failed(&mut self, message: String) {
        self.busy = false;
        self.status = Some(Status { text: message, error: true });
    }

    pub fn handle(&mut self, event: Event) -> Option<Outcome> {
        match event {
            Event::Pressed { button: Button::Secondary, .. } => Some(Outcome::Cancel),
            Event::Pressed { at, button: Button::Primary } => {
                self.pointer = Some(at);
                if !self.busy && !self.on_toolbar(at) {
                    self.press(at);
                }
                None
            }
            Event::Moved { to } => {
                self.pointer = Some(to);
                if self.drag.is_some() {
                    self.drag_to(to);
                } else {
                    self.hover = self.hovered();
                }
                None
            }
            Event::Released { button: Button::Primary } => self.release(),
            Event::Released { button: Button::Secondary } => None,
            Event::Left => {
                if self.drag.is_none() {
                    self.pointer = None;
                    self.hover = None;
                }
                None
            }
            Event::Key { key, ctrl, alt } => self.key(key, ctrl, alt),
            Event::Tool(tool) => {
                if self.tooled() {
                    self.tool = if self.tool == tool { Tool::Select } else { tool };
                    self.typing = None;
                }
                None
            }
            Event::Colour(colour) => {
                if colour < COLOURS.len() {
                    self.colour = colour;
                }
                None
            }
            Event::Undo => {
                if self.drag.is_none() {
                    self.shapes.pop();
                }
                None
            }
            Event::Do(action) => match (self.mode, action) {
                (Mode::Capture, Action::Copy | Action::Save | Action::Pin | Action::Text) => self.finish(action),
                _ => None,
            },
            Event::Cancel => Some(Outcome::Cancel),
            Event::Typed { text, width } => {
                if let Some(at) = self.typing.take().filter(|_| !text.trim().is_empty()) {
                    self.shapes.push(Shape { tool: Tool::Text, colour: self.colour, points: vec![at], text, width });
                }
                None
            }
        }
    }

    /// Whether the selection can be drawn on: the user's own screenshot,
    /// with something selected.
    fn tooled(&self) -> bool {
        self.mode == Mode::Capture && self.rect.is_some_and(|rect| rect.width >= MIN_SIZE && rect.height >= MIN_SIZE)
    }

    /// Where the toolbar is: below the selection's right end, or above it
    /// where there is no room below; never off the screen. Not while
    /// something is being dragged.
    pub fn toolbar(&self) -> Option<Rect> {
        let rect = self.rect.filter(|_| self.tooled() && self.drag.is_none())?;
        let (width, height) = TOOLBAR;
        let below = rect.bottom() + TOOLBAR_ROOM < self.size.1;
        Some(Rect {
            x: (rect.right() - width).min(self.size.0 - width - TOOLBAR_GAP).max(TOOLBAR_GAP),
            y: if below { rect.bottom() + TOOLBAR_GAP } else { (rect.y - height - TOOLBAR_GAP + 2.0).max(TOOLBAR_GAP) },
            width,
            height,
        })
    }

    fn on_toolbar(&self, at: Point) -> bool {
        self.toolbar().is_some_and(|toolbar| toolbar.contains(at))
    }

    /// The topmost annotation under a point that a press would take hold
    /// of. With the selection tool that is any of them; with a drawing
    /// tool only text, since a press elsewhere there starts a new one.
    fn shape_at(&self, at: Point) -> Option<usize> {
        if !self.tooled() {
            return None;
        }
        self.shapes.iter().rposition(|shape| (self.tool == Tool::Select || shape.tool == Tool::Text) && shape.hit(at))
    }

    /// The topmost window under the pointer, where a click would take it:
    /// not inside the selection, where a press moves that. A window's right
    /// and bottom edges are outside it, so of two windows side by side each
    /// point belongs to one.
    fn hovered(&self) -> Option<usize> {
        let p = self.pointer?;
        if self.busy || self.drag.is_some() || self.typing.is_some() || self.rect.is_some_and(|rect| rect.contains(p)) || self.on_toolbar(p) || self.shape_at(p).is_some() {
            return None;
        }
        self.windows.iter().position(|w| p.0 >= w.x && p.0 < w.right() && p.1 >= w.y && p.1 < w.bottom())
    }

    /// Whether the selection has its handles: once it is made, where it can
    /// be adjusted.
    fn handled(&self) -> bool {
        self.tooled() && self.tool == Tool::Select && self.drag.is_none()
    }

    /// The handle a point is on, as the sides it moves.
    fn handle_at(&self, at: Point) -> Option<(i8, i8)> {
        let rect = self.rect.filter(|_| self.handled())?;
        rect.handles().into_iter().find(|(middle, _)| (middle.0 - at.0).abs() <= GRIP && (middle.1 - at.1).abs() <= GRIP).map(|(_, sides)| sides)
    }

    fn press(&mut self, at: Point) {
        self.status = None;
        self.hover = None;
        let handle = self.handle_at(at);
        let inside = self.tooled() && self.rect.is_some_and(|rect| rect.contains(at));
        self.drag = Some(match (handle, self.rect) {
            (Some(sides), Some(was)) => Drag::Resize { from: at, was, sides },
            _ if self.shape_at(at).is_some() => {
                // It comes to the front, and moves with the pointer.
                let shape = self.shapes.remove(self.shape_at(at).expect("just found"));
                let was = shape.points.clone();
                self.shapes.push(shape);
                Drag::Carry { from: at, was }
            }
            _ if inside && self.tool == Tool::Text => {
                // The line's middle is where the pointer was.
                self.typing = Some((at.0, at.1 - TEXT_LINE / 2.0));
                return self.drag = None;
            }
            _ if inside && self.tool != Tool::Select => {
                self.shapes.push(Shape { tool: self.tool, colour: self.colour, points: vec![at, at], text: String::new(), width: 0.0 });
                Drag::Draw
            }
            (_, Some(was)) if was.contains(at) => Drag::Move { from: at, was },
            // Anywhere else starts a new one, with nothing drawn on it.
            _ => {
                self.shapes.clear();
                self.tool = Tool::Select;
                self.typing = None;
                self.rect = Some(Rect { x: at.0, y: at.1, width: 0.0, height: 0.0 });
                Drag::Out { from: at }
            }
        });
    }

    fn drag_to(&mut self, to: Point) {
        let (max_x, max_y) = self.size;
        match &self.drag {
            Some(Drag::Draw) => {
                if let Some(shape) = self.shapes.last_mut() {
                    // The pen leaves a path; the others stretch from where they began.
                    if shape.tool == Tool::Pen {
                        shape.points.push(to);
                    } else {
                        shape.points.truncate(1);
                        shape.points.push(to);
                    }
                }
                return;
            }
            Some(Drag::Carry { from, was }) => {
                let (dx, dy) = (to.0 - from.0, to.1 - from.1);
                if let Some(shape) = self.shapes.last_mut() {
                    shape.points = was.iter().map(|point| (point.0 + dx, point.1 + dy)).collect();
                }
                return;
            }
            _ => {}
        }
        self.rect = Some(match self.drag.clone() {
            None | Some(Drag::Draw | Drag::Carry { .. }) => return,
            Some(Drag::Out { from }) => Rect::between(from, (to.0.clamp(0.0, max_x), to.1.clamp(0.0, max_y))),
            Some(Drag::Move { from, was }) => Rect { x: (was.x + to.0 - from.0).clamp(0.0, (max_x - was.width).max(0.0)), y: (was.y + to.1 - from.1).clamp(0.0, (max_y - was.height).max(0.0)), ..was },
            Some(Drag::Resize { from, was, sides }) => {
                let (dx, dy) = (to.0 - from.0, to.1 - from.1);
                let (mut left, mut top, mut right, mut bottom) = (was.x, was.y, was.right(), was.bottom());
                // A side stops at the screen's edge, and short of the side across from it.
                match sides.0 {
                    -1 => left = (left + dx).clamp(0.0, right - MIN_SIZE),
                    1 => right = (right + dx).clamp(left + MIN_SIZE, max_x),
                    _ => {}
                }
                match sides.1 {
                    -1 => top = (top + dy).clamp(0.0, bottom - MIN_SIZE),
                    1 => bottom = (bottom + dy).clamp(top + MIN_SIZE, max_y),
                    _ => {}
                }
                Rect { x: left, y: top, width: right - left, height: bottom - top }
            }
        });
    }

    fn release(&mut self) -> Option<Outcome> {
        let drag = self.drag.take();
        if matches!(drag, Some(Drag::Draw)) && self.shapes.last().is_some_and(|shape| shape.points.windows(2).all(|ends| ends[0] == ends[1])) {
            // A press that went nowhere drew nothing.
            self.shapes.pop();
        }
        let Some(Drag::Out { from }) = drag else {
            // Moved, resized or drawn, or nothing was pressed: the selection stays as it is.
            self.hover = self.hovered();
            return None;
        };
        if self.rect.is_none_or(|rect| rect.width < MIN_SIZE || rect.height < MIN_SIZE) {
            // A click rather than a drag: the window that was pressed on,
            // exactly, or nothing.
            self.rect = self.windows.iter().find(|w| from.0 >= w.x && from.0 < w.right() && from.1 >= w.y && from.1 < w.bottom()).copied();
        }
        self.hover = self.hovered();
        // Without tools (text, or an area for an extension) selecting is the
        // whole interaction.
        match self.mode {
            Mode::Capture => None,
            Mode::Text => self.finish(Action::Text),
            Mode::Pick => self.finish(Action::Pick),
        }
    }

    fn key(&mut self, key: Key, ctrl: bool, alt: bool) -> Option<Outcome> {
        // While text is typed, Escape gives up the text, not the screenshot.
        if key == Key::Escape && self.typing.take().is_some() {
            return None;
        }
        match (key, self.mode) {
            // Alt+F4 arrives as a key: the overlay holds the keyboard, so
            // the window manager never sees it.
            (Key::Escape, _) => Some(Outcome::Cancel),
            (Key::F4, _) if alt => Some(Outcome::Cancel),
            // An area for an extension goes to the extension, never to the
            // clipboard or a file.
            (Key::Enter, Mode::Pick) => self.finish(Action::Pick),
            (_, Mode::Pick) => None,
            (Key::Enter, _) => self.finish(Action::Copy),
            (Key::Character('c'), _) if ctrl => self.finish(Action::Copy),
            (Key::Character('s'), _) if ctrl => self.finish(Action::Save),
            (Key::Character('z'), _) if ctrl => self.handle(Event::Undo),
            _ => None,
        }
    }

    /// Hands the selection over, if there is one and none was handed over
    /// already.
    fn finish(&mut self, action: Action) -> Option<Outcome> {
        let rect = self.rect.filter(|rect| !self.busy && self.drag.is_none() && rect.width >= 1.0 && rect.height >= 1.0)?;
        self.busy = true;
        self.hover = None;
        self.typing = None;
        if action == Action::Text {
            self.status = Some(Status { text: "Recognizing text…".into(), error: false });
        }
        Some(Outcome::Finish { action, area: self.area(rect) })
    }

    /// A rectangle of the overlay in the capture's pixels.
    fn area(&self, rect: Rect) -> Area {
        let density = self.density();
        let x = ((rect.x * density).round().max(0.0) as u32).min(self.capture.0.saturating_sub(1));
        let y = ((rect.y * density).round().max(0.0) as u32).min(self.capture.1.saturating_sub(1));
        Area { x, y, width: ((rect.width * density).round() as u32).clamp(1, self.capture.0 - x), height: ((rect.height * density).round() as u32).clamp(1, self.capture.1 - y) }
    }

    pub fn cursor(&self) -> Cursor {
        match &self.drag {
            Some(Drag::Out { .. }) => Cursor::Crosshair,
            Some(Drag::Move { .. }) => Cursor::Move,
            Some(Drag::Resize { sides, .. }) => Cursor::Resize(sides.0, sides.1),
            Some(Drag::Draw) => Cursor::Crosshair,
            Some(Drag::Carry { .. }) => Cursor::Grab,
            None => {
                let Some(p) = self.pointer else { return Cursor::Crosshair };
                if self.on_toolbar(p) {
                    Cursor::Arrow
                } else if let Some(sides) = self.handle_at(p) {
                    Cursor::Resize(sides.0, sides.1)
                } else if self.shape_at(p).is_some() {
                    Cursor::Grab
                } else if self.rect.is_some_and(|rect| rect.contains(p)) && (self.tool == Tool::Select || !self.tooled()) {
                    Cursor::Move
                } else {
                    Cursor::Crosshair
                }
            }
        }
    }

    pub fn look(&self) -> Look {
        let density = self.density();
        let label = self.rect.map(|rect| {
            let text = format!("{} × {}", (rect.width * density).round() as i64, (rect.height * density).round() as i64);
            // Above the selection's corner, or inside it at the top of the screen.
            let above = rect.y - LABEL_HEIGHT - LABEL_GAP;
            (text, (rect.x, if above < 0.0 { rect.y + LABEL_GAP } else { above }))
        });
        let hint = self.rect.is_none().then(|| {
            let what = if self.mode == Mode::Text { "Drag over the text to copy" } else { "Drag to select an area" };
            format!("{what}{} · Esc to cancel", if self.windows.is_empty() { "" } else { ", or click a window" })
        });
        // While the selection is being made or changed, and before there is
        // one: where an edge will be, to the pixel.
        let placing = matches!(self.drag, Some(Drag::Out { .. } | Drag::Resize { .. })) || self.rect.is_none();
        let loupe = self.pointer.filter(|_| !self.busy && placing).map(|pointer| {
            let side = LOUPE_CELLS as f64 * LOUPE_CELL;
            // Below and to the right of the pointer, or on the other side
            // where the screen ends.
            let beside = |at: f64, limit: f64| if at + LOUPE_GAP + side > limit { at - LOUPE_GAP - side } else { at + LOUPE_GAP };
            let half = (LOUPE_CELLS / 2) as i64;
            Loupe {
                frame: Rect { x: beside(pointer.0, self.size.0).round(), y: beside(pointer.1, self.size.1).round(), width: side, height: side },
                source: ((pointer.0 * density).floor() as i64 - half, (pointer.1 * density).floor() as i64 - half),
            }
        });
        Look {
            clear: self.rect.or(self.hover.map(|at| self.windows[at])),
            outline_inside: self.rect.is_none(),
            handles: self.rect.filter(|_| self.handled()).map_or(Vec::new(), |rect| rect.handles().iter().map(|(middle, _)| *middle).collect()),
            label,
            hint,
            status: self.status.clone(),
            loupe,
            toolbar: self.toolbar(),
            typing: self.typing,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: (f64, f64) = (1920.0, 1080.0);

    fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
        Rect { x, y, width, height }
    }

    /// An overlay of 1920 by 1080 logical pixels over a capture twice that.
    fn overlay(mode: Mode) -> Selecting {
        Selecting::new(SCREEN, (3840, 2160), mode)
    }

    fn press(overlay: &mut Selecting, at: Point) -> Option<Outcome> {
        overlay.handle(Event::Pressed { at, button: Button::Primary })
    }

    fn move_to(overlay: &mut Selecting, to: Point) -> Option<Outcome> {
        overlay.handle(Event::Moved { to })
    }

    fn release(overlay: &mut Selecting) -> Option<Outcome> {
        overlay.handle(Event::Released { button: Button::Primary })
    }

    /// A screenshot of the user's own with a selection, and a tool chosen.
    fn with_tool(tool: Tool) -> Selecting {
        let mut overlay = dragged((100.0, 100.0), (700.0, 500.0));
        overlay.handle(Event::Tool(tool));
        overlay
    }

    fn key(overlay: &mut Selecting, key: Key) -> Option<Outcome> {
        overlay.handle(Event::Key { key, ctrl: false, alt: false })
    }

    fn with_ctrl(overlay: &mut Selecting, character: char) -> Option<Outcome> {
        overlay.handle(Event::Key { key: Key::Character(character), ctrl: true, alt: false })
    }

    /// A drag from one point to another, and what the release answered.
    fn drag(overlay: &mut Selecting, from: Point, to: Point) -> Option<Outcome> {
        press(overlay, from);
        move_to(overlay, to);
        release(overlay)
    }

    /// A screenshot of the user's own with a selection dragged out.
    fn dragged(from: Point, to: Point) -> Selecting {
        let mut overlay = overlay(Mode::Capture);
        drag(&mut overlay, from, to);
        overlay
    }

    #[test]
    fn dragging_out_selects_whichever_way_the_pointer_goes() {
        for (from, to, selected) in [
            ((100.0, 100.0), (400.0, 300.0), rect(100.0, 100.0, 300.0, 200.0)),
            ((400.0, 300.0), (100.0, 100.0), rect(100.0, 100.0, 300.0, 200.0)),
            ((400.0, 100.0), (100.0, 300.0), rect(100.0, 100.0, 300.0, 200.0)),
            // Past the screen's edge it ends at the edge.
            ((1800.0, 1000.0), (2500.0, 1300.0), rect(1800.0, 1000.0, 120.0, 80.0)),
            ((100.0, 100.0), (-40.0, -40.0), rect(0.0, 0.0, 100.0, 100.0)),
        ] {
            assert_eq!(dragged(from, to).rect, Some(selected), "from {from:?} to {to:?}");
        }
    }

    #[test]
    fn less_than_the_smallest_selection_is_a_click_and_selects_nothing() {
        for (to, selected) in [((103.0, 102.0), false), ((105.9, 300.0), false), ((300.0, 105.9), false), ((106.0, 106.0), true)] {
            assert_eq!(dragged((100.0, 100.0), to).rect.is_some(), selected, "to {to:?}");
        }
        // And takes away the selection it was made outside of.
        let mut overlay = dragged((100.0, 100.0), (400.0, 300.0));
        press(&mut overlay, (900.0, 900.0));
        assert_eq!(release(&mut overlay), None);
        assert_eq!(overlay.rect, None);
    }

    #[test]
    fn pressing_inside_moves_it_and_keeps_it_on_the_screen() {
        let mut overlay = dragged((100.0, 100.0), (400.0, 300.0));
        press(&mut overlay, (200.0, 200.0));
        for (to, at) in [((250.0, 180.0), (150.0, 80.0)), ((-5000.0, 5000.0), (0.0, 880.0)), ((5000.0, -5000.0), (1620.0, 0.0))] {
            move_to(&mut overlay, to);
            assert_eq!(overlay.rect, Some(rect(at.0, at.1, 300.0, 200.0)), "to {to:?}");
        }
        assert_eq!(release(&mut overlay), None);
        assert_eq!(overlay.rect, Some(rect(1620.0, 0.0, 300.0, 200.0)));
    }

    #[test]
    fn each_handle_moves_its_sides_and_no_others() {
        // Pressed a little off its middle, and dragged by (-30, 40).
        let was = rect(100.0, 100.0, 300.0, 200.0);
        for (handle, moved) in [
            ((100.0, 100.0), rect(70.0, 140.0, 330.0, 160.0)),
            ((250.0, 100.0), rect(100.0, 140.0, 300.0, 160.0)),
            ((400.0, 100.0), rect(100.0, 140.0, 270.0, 160.0)),
            ((400.0, 200.0), rect(100.0, 100.0, 270.0, 200.0)),
            ((400.0, 300.0), rect(100.0, 100.0, 270.0, 240.0)),
            ((250.0, 300.0), rect(100.0, 100.0, 300.0, 240.0)),
            ((100.0, 300.0), rect(70.0, 100.0, 330.0, 240.0)),
            ((100.0, 200.0), rect(70.0, 100.0, 330.0, 200.0)),
        ] {
            let mut overlay = dragged((was.x, was.y), (was.right(), was.bottom()));
            let pressed = (handle.0 + 3.0, handle.1 - 3.0);
            press(&mut overlay, pressed);
            move_to(&mut overlay, (pressed.0 - 30.0, pressed.1 + 40.0));
            assert_eq!(overlay.rect, Some(moved), "the handle at {handle:?}");
        }
    }

    #[test]
    fn a_side_stops_short_of_the_one_across_and_at_the_edge() {
        let mut overlay = dragged((100.0, 100.0), (400.0, 300.0));
        press(&mut overlay, (100.0, 100.0));
        move_to(&mut overlay, (900.0, 900.0));
        assert_eq!(overlay.rect, Some(rect(400.0 - MIN_SIZE, 300.0 - MIN_SIZE, MIN_SIZE, MIN_SIZE)));
        move_to(&mut overlay, (-50.0, -50.0));
        assert_eq!(overlay.rect, Some(rect(0.0, 0.0, 400.0, 300.0)));
    }

    #[test]
    fn what_a_press_takes_hold_of() {
        // On a handle, a little off it, inside, and outside.
        for (at, cursor) in [
            ((400.0, 300.0), Cursor::Resize(1, 1)),
            ((393.0, 307.0), Cursor::Resize(1, 1)),
            ((392.0, 300.0), Cursor::Move),
            ((250.0, 106.0), Cursor::Resize(0, -1)),
            ((250.0, 200.0), Cursor::Move),
            ((408.0, 292.0), Cursor::Crosshair),
            ((900.0, 900.0), Cursor::Crosshair),
        ] {
            let mut overlay = dragged((100.0, 100.0), (400.0, 300.0));
            move_to(&mut overlay, at);
            assert_eq!(overlay.cursor(), cursor, "over {at:?}");
            // And it is what the press then does.
            press(&mut overlay, at);
            assert_eq!(overlay.cursor(), cursor, "pressed at {at:?}");
        }
    }

    #[test]
    fn nothing_moves_without_a_press() {
        let mut overlay = dragged((100.0, 100.0), (400.0, 300.0));
        move_to(&mut overlay, (700.0, 700.0));
        assert_eq!(release(&mut overlay), None);
        assert_eq!(overlay.rect, Some(rect(100.0, 100.0, 300.0, 200.0)));
    }

    /// Two windows, in the capture's pixels: one in front of, and partly
    /// over, the other.
    fn windows() -> [Area; 2] {
        [Area { x: 600, y: 400, width: 1000, height: 800 }, Area { x: 200, y: 200, width: 1201, height: 900 }]
    }

    #[test]
    fn a_click_selects_the_topmost_window_under_it() {
        for (at, selected) in [
            // Where they overlap, the one in front; windows are placed by their edges.
            ((400.0, 300.0), Some(rect(300.0, 200.0, 500.0, 400.0))),
            ((150.0, 150.0), Some(rect(100.0, 100.0, 600.5, 450.0))),
            // A window's right and bottom edges are outside it.
            ((800.0, 300.0), None),
            ((1500.0, 900.0), None),
        ] {
            let mut overlay = overlay(Mode::Capture);
            overlay.windows(&windows());
            move_to(&mut overlay, at);
            assert_eq!(overlay.look().clear, selected, "over {at:?}");
            assert!(overlay.look().outline_inside);
            press(&mut overlay, at);
            release(&mut overlay);
            assert_eq!(overlay.rect, selected, "clicked at {at:?}");
        }
    }

    #[test]
    fn no_window_is_offered_where_a_press_would_move_the_selection() {
        let mut overlay = dragged((250.0, 150.0), (500.0, 350.0));
        overlay.windows(&windows());
        move_to(&mut overlay, (400.0, 300.0));
        assert_eq!((overlay.hover, overlay.look().clear), (None, Some(rect(250.0, 150.0, 250.0, 200.0))));
        // Outside it, the window there is offered again, though the selection is what is shown clear.
        move_to(&mut overlay, (600.0, 500.0));
        assert_eq!(overlay.hover, Some(0));
        // And not while something is being dragged.
        press(&mut overlay, (300.0, 200.0));
        move_to(&mut overlay, (310.0, 210.0));
        assert_eq!(overlay.hover, None);
    }

    #[test]
    fn a_drag_over_windows_selects_what_was_dragged() {
        let mut overlay = overlay(Mode::Capture);
        overlay.windows(&windows());
        drag(&mut overlay, (310.0, 210.0), (700.0, 500.0));
        assert_eq!(overlay.rect, Some(rect(310.0, 210.0, 390.0, 290.0)));
    }

    #[test]
    fn enter_copies_and_escape_cancels() {
        let selected = Area { x: 200, y: 200, width: 600, height: 400 };
        let mut overlay = dragged((100.0, 100.0), (400.0, 300.0));
        assert_eq!(key(&mut overlay, Key::Character('c')), None);
        assert_eq!(key(&mut overlay, Key::Enter), Some(Outcome::Finish { action: Action::Copy, area: selected }));
        // Handed over once: a second Enter does nothing, but Escape still cancels.
        assert_eq!(key(&mut overlay, Key::Enter), None);
        assert_eq!(key(&mut overlay, Key::Escape), Some(Outcome::Cancel));

        for (character, action) in [('c', Action::Copy), ('s', Action::Save)] {
            let mut overlay = dragged((100.0, 100.0), (400.0, 300.0));
            assert_eq!(with_ctrl(&mut overlay, character), Some(Outcome::Finish { action, area: selected }));
        }
        let mut overlay = dragged((100.0, 100.0), (400.0, 300.0));
        assert_eq!(overlay.handle(Event::Key { key: Key::F4, ctrl: false, alt: true }), Some(Outcome::Cancel));
        assert_eq!(overlay.handle(Event::Pressed { at: (10.0, 10.0), button: Button::Secondary }), Some(Outcome::Cancel));
    }

    #[test]
    fn nothing_is_handed_over_without_a_selection_or_in_the_middle_of_a_drag() {
        let mut overlay = overlay(Mode::Capture);
        assert_eq!(key(&mut overlay, Key::Enter), None);
        press(&mut overlay, (100.0, 100.0));
        move_to(&mut overlay, (400.0, 300.0));
        assert_eq!(key(&mut overlay, Key::Enter), None);
        release(&mut overlay);
        assert!(key(&mut overlay, Key::Enter).is_some());
    }

    #[test]
    fn without_tools_selecting_is_the_whole_of_it() {
        let selected = Area { x: 200, y: 200, width: 600, height: 400 };
        let mut text = overlay(Mode::Text);
        assert_eq!(drag(&mut text, (100.0, 100.0), (400.0, 300.0)), Some(Outcome::Finish { action: Action::Text, area: selected }));
        assert_eq!(text.look().status, Some(Status { text: "Recognizing text…".into(), error: false }));
        // No handles, and nothing more is taken while the text is being read.
        assert!(text.look().handles.is_empty());
        assert_eq!(drag(&mut text, (500.0, 500.0), (700.0, 700.0)), None);
        assert_eq!(text.rect, Some(rect(100.0, 100.0, 300.0, 200.0)));
        // No text was found: the overlay says so, and another selection can be made.
        text.failed("No text was found in the selection.".into());
        assert_eq!(text.look().status, Some(Status { text: "No text was found in the selection.".into(), error: true }));
        assert!(drag(&mut text, (500.0, 500.0), (700.0, 700.0)).is_some());
        assert_eq!(text.look().status.map(|status| status.error), Some(false));

        let mut pick = overlay(Mode::Pick);
        pick.windows(&windows());
        // A click on a window is a selection too; a click on nothing is not.
        assert_eq!(drag(&mut pick, (1700.0, 1000.0), (1700.0, 1000.0)), None);
        assert_eq!(drag(&mut pick, (400.0, 300.0), (401.0, 300.0)), Some(Outcome::Finish { action: Action::Pick, area: Area { x: 600, y: 400, width: 1000, height: 800 } }));
        // For an extension, never the clipboard or a file.
        let mut pick = overlay(Mode::Pick);
        assert_eq!(with_ctrl(&mut pick, 's'), None);
        assert_eq!(key(&mut pick, Key::Escape), Some(Outcome::Cancel));
    }

    #[test]
    fn the_selection_is_handed_over_in_the_captures_pixels() {
        // At 150 % the toolkit counts 1280 logical pixels for 1920 real ones.
        let mut overlay = Selecting::new((1280.0, 720.0), (1920, 1080), Mode::Capture);
        drag(&mut overlay, (100.5, 100.0), (1280.0, 720.0));
        assert_eq!(key(&mut overlay, Key::Enter), Some(Outcome::Finish { action: Action::Copy, area: Area { x: 151, y: 150, width: 1769, height: 930 } }));
        assert_eq!(overlay.look().label.map(|(text, _)| text), Some("1769 × 930".into()));
    }

    #[test]
    fn what_is_shown_with_nothing_selected() {
        let mut overlay = overlay(Mode::Capture);
        let look = overlay.look();
        assert_eq!((look.clear, look.label, look.loupe), (None, None, None));
        assert!(look.handles.is_empty());
        assert_eq!(look.hint.as_deref(), Some("Drag to select an area · Esc to cancel"));
        overlay.windows(&windows());
        assert_eq!(overlay.look().hint.as_deref(), Some("Drag to select an area, or click a window · Esc to cancel"));
        assert_eq!(self::overlay(Mode::Text).look().hint.as_deref(), Some("Drag over the text to copy · Esc to cancel"));
    }

    #[test]
    fn what_is_shown_of_a_selection() {
        let mut overlay = overlay(Mode::Capture);
        press(&mut overlay, (100.0, 100.0));
        move_to(&mut overlay, (400.0, 300.0));
        // While it is dragged out: clear inside, its size in real pixels, no handles yet.
        let look = overlay.look();
        assert_eq!((look.clear, look.outline_inside, look.hint), (Some(rect(100.0, 100.0, 300.0, 200.0)), false, None));
        assert_eq!(look.label, Some(("600 × 400".into(), (100.0, 100.0 - LABEL_HEIGHT - LABEL_GAP))));
        assert!(look.handles.is_empty());
        release(&mut overlay);
        // Then eight handles: the corners and the middles of the sides.
        assert_eq!(overlay.look().handles, [(100.0, 100.0), (250.0, 100.0), (400.0, 100.0), (400.0, 200.0), (400.0, 300.0), (250.0, 300.0), (100.0, 300.0), (100.0, 200.0)]);
    }

    #[test]
    fn the_label_goes_inside_where_there_is_no_room_above() {
        for (top, label_at) in [(100.0, 76.0), (24.0, 0.0), (23.0, 27.0), (0.0, 4.0)] {
            let overlay = dragged((50.0, top), (400.0, 600.0));
            assert_eq!(overlay.look().label.map(|(_, at)| at), Some((50.0, label_at)), "a selection from {top}");
        }
    }

    #[test]
    fn the_loupe_follows_the_pointer_while_an_edge_is_being_placed() {
        let mut overlay = overlay(Mode::Capture);
        // Before there is a selection: beside the pointer, showing the pixels round it.
        move_to(&mut overlay, (300.5, 200.0));
        assert_eq!(overlay.look().loupe, Some(Loupe { frame: rect(321.0, 220.0, 120.0, 120.0), source: (594, 393) }));
        // On the other side where the screen ends.
        for (pointer, frame) in [((1850.0, 200.0), (1710.0, 220.0)), ((300.0, 1000.0), (320.0, 860.0)), ((1779.0, 939.0), (1799.0, 959.0)), ((1781.0, 941.0), (1641.0, 801.0))] {
            move_to(&mut overlay, pointer);
            assert_eq!(overlay.look().loupe.map(|loupe| (loupe.frame.x, loupe.frame.y)), Some(frame), "the pointer at {pointer:?}");
        }
        press(&mut overlay, (100.0, 100.0));
        move_to(&mut overlay, (400.0, 300.0));
        assert!(overlay.look().loupe.is_some());
        // The selection made, it is out of the way; and back while a handle is dragged.
        release(&mut overlay);
        assert_eq!(overlay.look().loupe, None);
        press(&mut overlay, (400.0, 300.0));
        assert!(overlay.look().loupe.is_some());
        release(&mut overlay);
        // Nor when the pointer has left.
        let mut overlay = self::overlay(Mode::Capture);
        move_to(&mut overlay, (300.0, 200.0));
        overlay.handle(Event::Left);
        assert_eq!(overlay.look().loupe, None);
    }

    #[test]
    fn a_window_that_was_offered_is_not_once_the_pointer_has_left() {
        let mut overlay = overlay(Mode::Capture);
        overlay.windows(&windows());
        move_to(&mut overlay, (400.0, 300.0));
        assert!(overlay.look().clear.is_some());
        overlay.handle(Event::Left);
        assert_eq!(overlay.look().clear, None);
        // Windows that arrive with the pointer already over one are offered at once.
        let mut overlay = self::overlay(Mode::Capture);
        move_to(&mut overlay, (400.0, 300.0));
        overlay.windows(&windows());
        assert_eq!(overlay.look().clear, Some(rect(300.0, 200.0, 500.0, 400.0)));
    }

    #[test]
    fn the_toolbar_is_below_the_selections_right_end_and_stays_on_the_screen() {
        let (width, height) = TOOLBAR;
        for (from, to, at) in [
            // Below, its right end at the selection's.
            ((700.0, 100.0), (1300.0, 500.0), (1300.0 - width, 508.0)),
            // A narrow selection at the left: not off the screen.
            ((0.0, 100.0), (200.0, 500.0), (8.0, 508.0)),
            // At the right edge: 8 short of it.
            ((1500.0, 100.0), (1920.0, 500.0), (1920.0 - width - 8.0, 508.0)),
            // No room below: above, the same distance away.
            ((700.0, 300.0), (1300.0, 1040.0), (1300.0 - width, 300.0 - height - 6.0)),
            // The whole screen: at its top.
            ((0.0, 0.0), (1920.0, 1080.0), (1920.0 - width - 8.0, 8.0)),
        ] {
            let overlay = dragged(from, to);
            assert_eq!(overlay.toolbar(), Some(rect(at.0, at.1, width, height)), "a selection from {from:?} to {to:?}");
        }
        // None while the selection is dragged out, without one, or without tools.
        let mut overlay = overlay(Mode::Capture);
        assert_eq!(overlay.toolbar(), None);
        press(&mut overlay, (100.0, 100.0));
        move_to(&mut overlay, (700.0, 500.0));
        assert_eq!(overlay.toolbar(), None);
        release(&mut overlay);
        assert!(overlay.toolbar().is_some());
        press(&mut overlay, (300.0, 300.0));
        assert_eq!(overlay.toolbar(), None);
    }

    #[test]
    fn a_press_on_the_toolbar_is_the_toolbars() {
        let mut overlay = dragged((100.0, 100.0), (700.0, 500.0));
        overlay.windows(&windows());
        let on_it = (400.0, 520.0);
        move_to(&mut overlay, on_it);
        assert_eq!((overlay.cursor(), overlay.hover), (Cursor::Arrow, None));
        press(&mut overlay, on_it);
        assert!(overlay.drag.is_none());
        assert_eq!(overlay.rect, Some(rect(100.0, 100.0, 600.0, 400.0)));
    }

    #[test]
    fn a_tool_is_put_down_by_its_own_button_and_by_a_new_selection() {
        let mut overlay = with_tool(Tool::Rect);
        assert_eq!(overlay.tool(), Tool::Rect);
        // No handles while something else than the selection is drawn.
        assert!(overlay.look().handles.is_empty());
        overlay.handle(Event::Tool(Tool::Arrow));
        assert_eq!(overlay.tool(), Tool::Arrow);
        overlay.handle(Event::Tool(Tool::Arrow));
        assert_eq!(overlay.tool(), Tool::Select);
        assert_eq!(overlay.look().handles.len(), 8);
        // A drag outside starts over: nothing drawn, no tool.
        let mut overlay = with_tool(Tool::Rect);
        drag(&mut overlay, (200.0, 200.0), (300.0, 300.0));
        assert_eq!(overlay.shapes().len(), 1);
        drag(&mut overlay, (900.0, 600.0), (1200.0, 900.0));
        assert_eq!((overlay.shapes().len(), overlay.tool()), (0, Tool::Select));
        // Without a selection there is nothing to draw on.
        let mut overlay = self::overlay(Mode::Capture);
        overlay.handle(Event::Tool(Tool::Pen));
        assert_eq!(overlay.tool(), Tool::Select);
    }

    #[test]
    fn a_drag_with_a_tool_draws_in_the_colour_chosen() {
        let mut overlay = with_tool(Tool::Rect);
        overlay.handle(Event::Colour(2));
        drag(&mut overlay, (200.0, 200.0), (300.0, 260.0));
        assert_eq!(overlay.shapes(), [Shape { tool: Tool::Rect, colour: 2, points: vec![(200.0, 200.0), (300.0, 260.0)], text: String::new(), width: 0.0 }]);
        assert_eq!(overlay.rect, Some(rect(100.0, 100.0, 600.0, 400.0)));
        // The pen leaves every place it was at; an arrow only its two ends.
        overlay.handle(Event::Tool(Tool::Pen));
        press(&mut overlay, (200.0, 300.0));
        for to in [(210.0, 310.0), (220.0, 305.0), (230.0, 330.0)] {
            move_to(&mut overlay, to);
        }
        assert_eq!(overlay.cursor(), Cursor::Crosshair);
        release(&mut overlay);
        assert_eq!(overlay.shapes()[1].points, [(200.0, 300.0), (200.0, 300.0), (210.0, 310.0), (220.0, 305.0), (230.0, 330.0)]);
        overlay.handle(Event::Tool(Tool::Arrow));
        press(&mut overlay, (400.0, 200.0));
        move_to(&mut overlay, (450.0, 220.0));
        move_to(&mut overlay, (500.0, 300.0));
        release(&mut overlay);
        assert_eq!(overlay.shapes()[2].points, [(400.0, 200.0), (500.0, 300.0)]);
        // A press that goes nowhere draws nothing.
        drag(&mut overlay, (300.0, 400.0), (300.0, 400.0));
        assert_eq!(overlay.shapes().len(), 3);
        // A colour that is not one of them is not taken.
        overlay.handle(Event::Colour(9));
        assert_eq!(overlay.colour(), 2);
    }

    #[test]
    fn undo_takes_the_last_one_away() {
        let mut overlay = with_tool(Tool::Rect);
        drag(&mut overlay, (200.0, 200.0), (300.0, 260.0));
        drag(&mut overlay, (400.0, 200.0), (500.0, 260.0));
        overlay.handle(Event::Undo);
        assert_eq!(overlay.shapes().len(), 1);
        assert_eq!(with_ctrl(&mut overlay, 'z'), None);
        assert!(overlay.shapes().is_empty());
        overlay.handle(Event::Undo);
        assert!(overlay.shapes().is_empty());
    }

    #[test]
    fn an_annotation_is_taken_by_what_is_drawn_of_it_and_comes_to_the_front() {
        let mut overlay = with_tool(Tool::Rect);
        drag(&mut overlay, (200.0, 200.0), (400.0, 400.0));
        overlay.handle(Event::Tool(Tool::Arrow));
        drag(&mut overlay, (250.0, 250.0), (350.0, 350.0));
        overlay.handle(Event::Tool(Tool::Arrow));
        assert_eq!(overlay.tool(), Tool::Select);
        // On the rectangle's outline (a little off it), and inside it on the arrow.
        move_to(&mut overlay, (205.0, 300.0));
        assert_eq!(overlay.cursor(), Cursor::Grab);
        press(&mut overlay, (205.0, 300.0));
        move_to(&mut overlay, (215.0, 320.0));
        release(&mut overlay);
        // It is the last now, and moved by as much as the pointer.
        assert_eq!(overlay.shapes()[1].points, [(210.0, 220.0), (410.0, 420.0)]);
        assert_eq!(overlay.shapes()[0].tool, Tool::Arrow);
        // Inside a rectangle, away from its outline and from anything else: the selection moves.
        move_to(&mut overlay, (380.0, 250.0));
        assert_eq!(overlay.cursor(), Cursor::Move);
        // With a drawing tool only text can be taken: elsewhere a press draws.
        overlay.handle(Event::Tool(Tool::Rect));
        move_to(&mut overlay, (300.0, 300.0));
        assert_eq!(overlay.cursor(), Cursor::Crosshair);
        // The selection moved from under them leaves them where they are.
        overlay.handle(Event::Tool(Tool::Rect));
        drag(&mut overlay, (600.0, 150.0), (650.0, 180.0));
        assert_eq!(overlay.shapes()[1].points, [(210.0, 220.0), (410.0, 420.0)]);
    }

    #[test]
    fn text_is_typed_where_the_selection_is_clicked() {
        let mut overlay = with_tool(Tool::Text);
        press(&mut overlay, (300.0, 300.0));
        assert_eq!(release(&mut overlay), None);
        // The line's middle is where the pointer was.
        assert_eq!(overlay.look().typing, Some((300.0, 300.0 - TEXT_LINE / 2.0)));
        overlay.handle(Event::Typed { text: "Look here".into(), width: 90.0 });
        assert_eq!(overlay.look().typing, None);
        assert_eq!(overlay.shapes(), [Shape { tool: Tool::Text, colour: 0, points: vec![(300.0, 288.0)], text: "Look here".into(), width: 90.0 }]);
        // Done with once: a second time adds nothing. Nor does text that says nothing.
        overlay.handle(Event::Typed { text: "Look here".into(), width: 90.0 });
        press(&mut overlay, (300.0, 400.0));
        overlay.handle(Event::Typed { text: "  ".into(), width: 8.0 });
        assert_eq!(overlay.shapes().len(), 1);
        // Escape gives up the text that is being typed, and only then the screenshot.
        press(&mut overlay, (300.0, 400.0));
        assert_eq!(key(&mut overlay, Key::Escape), None);
        assert_eq!(overlay.look().typing, None);
        assert_eq!(key(&mut overlay, Key::Escape), Some(Outcome::Cancel));
        // Text can be taken hold of with a drawing tool too.
        let mut overlay = with_tool(Tool::Text);
        press(&mut overlay, (300.0, 300.0));
        overlay.handle(Event::Typed { text: "Look here".into(), width: 90.0 });
        move_to(&mut overlay, (340.0, 300.0));
        assert_eq!(overlay.cursor(), Cursor::Grab);
        press(&mut overlay, (340.0, 300.0));
        move_to(&mut overlay, (350.0, 310.0));
        release(&mut overlay);
        assert_eq!((overlay.shapes()[0].points.clone(), overlay.look().typing), (vec![(310.0, 298.0)], None));
    }

    #[test]
    fn the_toolbars_buttons_say_what_becomes_of_the_selection() {
        let selected = Area { x: 200, y: 200, width: 1200, height: 800 };
        for action in [Action::Copy, Action::Save, Action::Pin, Action::Text] {
            let mut overlay = dragged((100.0, 100.0), (700.0, 500.0));
            assert_eq!(overlay.handle(Event::Do(action)), Some(Outcome::Finish { action, area: selected }));
        }
        let mut overlay = dragged((100.0, 100.0), (700.0, 500.0));
        assert_eq!(overlay.handle(Event::Cancel), Some(Outcome::Cancel));
        // An extension's area is never the toolbar's to give away.
        let mut pick = self::overlay(Mode::Pick);
        press(&mut pick, (100.0, 100.0));
        move_to(&mut pick, (700.0, 500.0));
        assert_eq!(pick.handle(Event::Do(Action::Copy)), None);
        assert_eq!((pick.toolbar(), pick.look().handles.len()), (None, 0));
    }

    // What the web view's overlay was tested for in placing windows
    // (`screenshotWindows.test.ts`), of this one.

    #[test]
    fn windows_that_touch_in_the_capture_touch_in_the_overlay() {
        // At 150 %: two windows side by side, 1281 and 1279 pixels wide.
        let mut overlay = Selecting::new((2560.0 / 1.5, 1440.0 / 1.5), (2560, 1440), Mode::Capture);
        overlay.windows(&[Area { x: 0, y: 0, width: 1281, height: 1440 }, Area { x: 1281, y: 0, width: 1279, height: 1440 }]);
        let (left, right) = (overlay.windows[0], overlay.windows[1]);
        assert_eq!(left.right(), right.x);
        assert!((right.right() - 2560.0 / 1.5).abs() < 1e-9);
        // And at twice the pixels a window is half as large, from the same corner.
        let mut twice = self::overlay(Mode::Capture);
        twice.windows(&[Area { x: 160, y: 200, width: 1600, height: 1200 }, Area { x: 0, y: 0, width: 3840, height: 2160 }]);
        assert_eq!(twice.windows, [rect(80.0, 100.0, 800.0, 600.0), rect(0.0, 0.0, 1920.0, 1080.0)]);
    }

    #[test]
    fn a_window_that_was_clicked_is_handed_over_as_the_pixels_it_was_given_as() {
        // At 125 %, where a window's edges are not whole logical pixels.
        let window = Area { x: 37, y: 53, width: 1001, height: 777 };
        let mut overlay = Selecting::new((1920.0 / 1.25, 1080.0 / 1.25), (1920, 1080), Mode::Capture);
        overlay.windows(&[window]);
        drag(&mut overlay, (100.0, 100.0), (100.0, 100.0));
        assert_eq!(key(&mut overlay, Key::Enter), Some(Outcome::Finish { action: Action::Copy, area: window }));
    }

    #[test]
    fn the_window_offered_is_the_topmost_under_the_pointer() {
        let at_scale_one = || Selecting::new(SCREEN, (1920, 1080), Mode::Capture);
        // Topmost first: a dialog over an editor over a maximized browser.
        let mut overlay = at_scale_one();
        overlay.windows(&[Area { x: 300, y: 300, width: 400, height: 300 }, Area { x: 100, y: 100, width: 800, height: 600 }, Area { x: 0, y: 0, width: 1920, height: 1080 }]);
        // A window is found by the part of it that shows: under the dialog's
        // rectangle the editor cannot be reached; beside it, it can.
        for (at, offered) in [((350.0, 350.0), 0), ((150.0, 150.0), 1), ((1000.0, 900.0), 2), ((299.0, 350.0), 1), ((700.0, 350.0), 1)] {
            move_to(&mut overlay, at);
            assert_eq!(overlay.hover, Some(offered), "over {at:?}");
        }
        // The left and top edges are in, the right and bottom ones out.
        let mut pair = at_scale_one();
        pair.windows(&[Area { x: 0, y: 0, width: 960, height: 1080 }, Area { x: 960, y: 0, width: 960, height: 1080 }]);
        for (at, offered) in [((959.5, 10.0), Some(0)), ((960.0, 10.0), Some(1)), ((1920.0, 10.0), None), ((10.0, 1080.0), None)] {
            move_to(&mut pair, at);
            assert_eq!(pair.hover, offered, "over {at:?}");
        }
        // Nothing over the bare desktop, with a window elsewhere or with none.
        let mut one = at_scale_one();
        one.windows(&[Area { x: 100, y: 100, width: 200, height: 200 }]);
        move_to(&mut one, (50.0, 50.0));
        assert_eq!(one.hover, None);
        let mut none = at_scale_one();
        move_to(&mut none, (50.0, 50.0));
        assert_eq!(none.hover, None);
    }
}

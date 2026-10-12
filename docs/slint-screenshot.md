# The screenshot interface, drawn by Slint

The screenshot overlay and the pinned capture are to be drawn by Slint in
place of their web views, as the settings and the dictation pill are
(`docs/slint-settings.md`). This file begins as the list of everything
the web interface does today, read from `src/Screenshot.tsx`,
`src/screenshotWindows.ts`, `src/styles/components/_screenshot.scss`,
`src-tauri/src/screenshot.rs` and `src-tauri/src/platform/` (`overlay.rs`,
`capture.rs`, `x11/overlay.rs`, the `Desktop` trait). Nothing in it may be
missing when the web interface is removed. A box is ticked when the Slint
interface does it and it has been seen to.

## Inventory

### How it starts

- [ ] `heyo:screenshot`, `heyo:screenshot-text` (root search, their
  hotkeys) and an extension's `Screen.capture("selection")` start it:
  modes `capture`, `text` and `pick`.
- [ ] The palette is hidden first and waited for, and an overlay that is
  still up is closed first: neither is in the capture.
- [ ] One monitor is captured: the one the pointer is on (the desktop's
  word where the toolkit cannot follow the pointer), else the primary.
- [x] The frozen screen is on display as soon as it has been read, before
  anything else is ready. The windows on the screen are asked for
  meanwhile and arrive after.
- [ ] A capture that fails is said in a notification ("Screenshot
  failed"), or told to the extension that asked.
- [ ] A capture of the user's own takes the overlay from an extension's,
  which hears "cancelled"; an extension that waits hears within ten
  minutes whatever happens.

### What is on the screen

- [x] The frozen picture of the monitor, one of its pixels a device
  pixel, no scaling and no colour change, covering the whole monitor, the
  desktop's panels too, with no opening animation.
- [x] Everything but the selection dimmed (154 of 256).
- [ ] Before there is a selection: a hint in the middle, "Drag to select
  an area" (mode text: "Drag over the text to copy"), ", or click a
  window" when windows are known, " · Esc to cancel".
- [ ] The window under the pointer, where a click would take it: shown
  undimmed with the outline inside it (so that a window filling the screen
  has one). It changes when the pointer crosses into another window; not
  shown over the selection, a tool, an annotation, or while dragging.
- [x] The selection: undimmed, outlined 1.5 px in the accent.
- [x] Its size in the capture's pixels ("2600 × 1400") in a small dark
  label above its top left corner.
- [x] Eight round handles (9 px, accent, white edge) on corners and
  sides, in mode capture with the selection tool, not while dragging.
- [ ] The pointer: a crosshair; "move" over the selection; the resize
  arrows over each handle; a crosshair in the selection with a drawing
  tool; "grab" over an annotation that can be dragged; the default arrow
  over the toolbar.
- [ ] A status line at the bottom ("Recognizing text…", or an error in
  red, e.g. "No text was found in the selection.").
- [ ] Nothing of the last capture flashes when the next one opens.

### Selecting

- [x] A drag from outside the selection makes a new one (and drops the
  annotations and the tool).
- [ ] A press and release without a drag (less than 6 px either way)
  takes the window pressed on, exactly; where there is none, the selection
  is dropped.
- [x] A drag inside the selection moves it; it stays on the screen.
- [x] A drag on a handle resizes by that handle's sides; never smaller
  than 6 px, never past the screen's edge.
- [ ] In modes text and pick, making the selection finishes it.
- [ ] Windows: other applications' only, topmost first, cut to the
  monitor, at least 16 px showing; fitted where the toolkit counts more
  pixels than the capture has (fractional scaling on Wayland).

### The toolbar (mode capture, a selection of at least 6 px, not while dragging)

- [ ] Below the selection's right end, 8 px away; above it when there is
  no room below (52 px); kept 8 px inside the screen's edges.
- [ ] Tools, each a toggle back to selecting: Rectangle, Arrow, Pen,
  Text, Blur.
- [ ] Four colours (red `#ff453a`, yellow `#ffc531`, green `#59d499`,
  blue `#56c2ff`), the chosen one ringed.
- [ ] Undo (off when there is nothing to undo).
- [ ] After a separator: Copy text in the selection, Pin to screen,
  Save, Copy (the accent one), Cancel.
- [ ] Tooltips: "Rectangle", "Arrow", "Pen", "Text", "Blur", "Color",
  "Undo (Ctrl+Z)", "Copy text in the selection", "Pin to screen", "Save
  (Ctrl+S)", "Copy (Enter)", "Cancel (Esc)".

### Annotations (inside the selection, in the chosen colour)

- [ ] Rectangle: an outline 3 px wide from press to release.
- [ ] Arrow: a line 3 px wide with round ends and a filled head (14 px).
- [ ] Pen: the pointer's path, 3 px, round joins.
- [ ] Text: a click opens a field there (18 px, weight 600, dashed
  outline, at least 120 px wide); Enter or leaving it adds the text (once,
  and not when empty), Esc drops it; an input method composes in it.
- [ ] Blur: the rectangle dragged out, as blocks of 9 px sampled from the
  capture.
- [ ] With the selection tool any annotation can be dragged (it comes to
  the front); with a drawing tool only text can. A rectangle is taken by
  its outline, lines within 7 px.
- [ ] Annotations are where they were put on the screen, not on the
  selection: the selection moved or resized leaves them there, and cuts
  them off at its edge. Nothing jitters while it does.
- [ ] Undo takes the last one away.

### Ways out

- [ ] Copy: Enter, Ctrl+C, the button. The part with its annotations, at
  the screen's resolution, on the clipboard as an image; "Screenshot
  copied".
- [ ] Save: Ctrl+S, the button. A PNG in Pictures/Screenshots
  ("Screenshot 2026-10-12 21-30-00.png"); "Screenshot saved" with the path.
- [ ] Pin: the button. See below.
- [ ] Copy the text in it: the button, or the selection itself in mode
  text. The overlay stays and says "Recognizing text…"; the text goes to
  the clipboard ("Text copied" with its beginning); small text is read
  enlarged; none found, or no engine, is said in the status line and the
  overlay stays.
- [ ] Pick (an extension's): Enter, or the selection itself. The part is
  saved where the extension asked, never copied or shown.
- [ ] Cancel: Esc, Alt+F4, a right click anywhere, the button. Always,
  also when the overlay has lost its capture.
- [ ] Ctrl+Z undoes. Other keys do nothing.
- [ ] Whatever was chosen, the overlay is gone before the result is used,
  and the capture's file is deleted.

### The pinned capture ("Heyo Pinned Screenshot")

- [ ] A window of its own exactly where the part was on the screen, at
  least 24 px either way, above other windows, not in the taskbar, with a
  thin accent edge and a shadow; several at once.
- [ ] Drag to move (from 4 px of movement, so that a double click still
  closes). Double click, right click or Esc closes it. Its picture is
  deleted with it.
- [ ] The Shell extension keeps it out of the window lists (by title).

### Safety

- [x] The overlay holds the keyboard (a grab on X11: the focus stays
  where it was). Esc always reaches it.
- [x] If Heyo stops answering while it is up, the keyboard is let go
  within a few seconds (the web view's overlay: Heyo ends itself. Slint's:
  the overlay is taken off the screen and Heyo goes on).
- [x] If Heyo is killed, nothing is left on screen and the keyboard is
  free.

### Platforms

- [ ] X11: the overlay goes past the window manager (above the panels, no
  animation, nothing moves it), at the monitor's exact pixels; the grab is
  retried while the hotkey that started it is still held.
- [ ] GNOME on Wayland: the Shell extension captures; the overlay is a
  fullscreen window on the captured monitor, given the keyboard by the
  Shell; each monitor has its own scale.
- [ ] Windows: a fullscreen window over the monitor, focused.
- [ ] Several monitors, of different scales: the capture, the overlay and
  the windows are of the monitor the pointer is on.
- [ ] Without a way to read the screen (`capture: none`), the commands are
  not offered.

### Not there today (and not asked for)

A loupe. The selection's logic written for a host-drawn overlay
(`screenshot/selecting.rs`) has one, and it is kept: it shows where an
edge will be, to the pixel.

## The overlay Slint draws

So far on X11, with area selection, and beside the web view's: Heyo shows
it where `HEYO_SCREENSHOT=slint` is in its environment. (The switch goes
when the web view's overlay does.)

- `src-tauri/src/screenshot/selecting.rs` decides what the overlay shows
  and how it answers the pointer and the keys. It knows no toolkit, and
  its rules are tested with numbers.
- `src-tauri/src/slint_windows/ui/screenshot.slint` draws what that says.
- `src-tauri/src/slint_windows/screenshot/mod.rs` is the window, one of
  those on Slint's thread: made when Heyo starts, small, off the screen
  and hidden; put over the monitor that was read when a screenshot is
  asked for. It hears of the pointer and the keys from winit, not from
  elements in the markup.
- `src-tauri/src/slint_windows/screenshot/native.rs` is what differs by
  platform: on X11 the window goes past the window manager, and it grabs
  the keyboard on the connection it was made on.
- What was decided goes to `screenshot::decided`, on a thread of the
  host's: the selected part is cut from the capture there, which the
  host keeps, and finished as a PNG from the web view's overlay is.

Three things in it are there for speed, each found by measuring:

- **The capture is there twice, as it was read and dimmed**, and the
  selection is the first drawn over the second. A drag then changes the
  selection's part of the screen and not all of it.
- **Each of the two pictures is drawn by one element.** Slint's Skia
  renderer copies a picture the first time an element draws it
  (`i-slint-renderer-skia`, `cached_image.rs`), and a 4K one takes it 10
  to 35 ms. The dimmed picture is shown as soon as it is made, in the
  first frame; the other follows a few milliseconds later and is drawn at
  once (one pixel of it, under the hint), so that its copy is made before
  a drag can begin. The loupe has a picture of its own, of 15 by 15
  pixels.
- **A frame is drawn when the overlay changes, not when Slint would.** On
  X11 Slint's winit backend draws a frame one refresh interval after it
  was asked for, by a timer (`frame_throttle.rs`): the selection followed
  the pointer 16 ms later than it had to. The window asks winit for the
  redraw itself.

### That it cannot trap whoever uses it

The overlay covers the screen and holds the keyboard.

- Escape is read from winit's event before anything else is looked at.
- If Slint's thread panics, `guard` ends its connection to the display,
  and the display drops the window and the grab with it.
- If Slint's thread stops answering, a thread that asks it every 250 ms
  while the overlay is up unmaps the window after six unanswered, from a
  connection of its own. The display lets a grab go when the window that
  holds it leaves the screen. Heyo goes on.
- If Heyo is killed, the display does the same.

`experiments/slint-settings/scripts/screenshot-safe.sh` tries each in the
middle of a drag, with a text field of another program having the
keyboard: a key typed while the overlay is up must not reach the field,
and "hello" typed after the trouble must (`results/screenshot-safe.txt`).

| in the middle of a drag | the overlay was off the screen after | Heyo afterwards |
|---|---|---|
| Escape | 18 ms | runs; the next screenshot comes up |
| Slint's thread panics | at once (within the 150 ms the test takes to look) | runs; the palette opens |
| Slint's thread stops for a minute | 1.7 s | runs; the palette opens |
| Heyo is killed | 48 ms | gone |

### How fast it is

Measured from outside, the screen as the clock
(`experiments/slint-settings/scripts/measure-screenshot.sh`,
`results/screenshot.jsonl`, pooled by `measure-screenshot-pool.py`): a
program reads a pixel of the screen again and again and says when it
changed. Xvfb at 3840x2160, scale 1, under GNOME Shell, which composites
with software OpenGL there: part of every figure is the Shell's, the same
part for both overlays. No graphics card is used by either. Milliseconds;
where three are given they are the median, the 95th of a hundred and the
worst, of the run whose 95th is the middle one of two or three runs.

| | Slint, 20 cores | web view, 20 cores | Slint, 4 small cores | web view, 4 small cores |
|---|---|---|---|---|
| asked for (`heyo --open heyo:screenshot` started) to the frozen screen: the median of 14 to 16 screenshots, and the least and most | 177 (154 to 217) | 250 (231 to 287) | 183 (159 to 213) | 241 (220 to 322) |
| ... to ready to drag (the hint drawn) | the same frame | up to 67 later (19 or more in 6 of 15) | the same frame | up to 88 later (19 or more in 6 of 14) |
| the pointer jumps, the selection has followed (about 1000 by 400) | 11 / 15 / 16 | 15 / 20 / 79 | 11 / 15 / 19 | 15 / 20 / 213 |
| the pointer sweeps without pause, a movement every 8 ms | 21 / 32 / 34 | 28 / 37 / 37 | 21 / 33 / 34 | 30 / 41 / 41 |
| the pointer jumps, a selection of most of the screen (3040 by 1800) | 24 / 28 / 31 | 51 / 61 / 122 | 26 / 30 / 31 | 54 / 62 / 200 |
| the pointer sweeps, the same | 38 / 55 / 56 | 85 / 103 / 104 | 50 / 65 / 66 | 96 / 110 / 111 |
| a frame of Slint's, drawn and handed over: small selection | 0.9 / 1.4 / 3.1 | | 1.1 / 1.5 / 22.2 | |
| the same, most of the screen | 6.6 / 10.1 / 15.7 | | 11.8 / 14.5 / 40.4 | |

Of the 177 ms to the frozen screen, about 50 are the second `heyo`
process reaching the first, 40 reading the screen, 17 making the dimmed
picture, 20 to 35 Slint's first frame, and the rest the Shell showing a
new window of that size.

A selection of most of the screen costs Slint a frame of 7 ms (12 on a
small core) because the whole selection is drawn again when its corner
moves: the renderer redraws the rectangle that holds what an element was
and what it has become, not only the difference.

### What it holds in memory

`experiments/slint-settings/scripts/screenshot-memory.sh`,
`results/screenshot-memory.txt`: Heyo's own resident memory (not the web
view's processes), a 4K screen.

| | Slint | web view |
|---|---|---|
| before any screenshot | 177 MB | 176 MB |
| the overlay up, something selected | 368 MB | 306 to 339 MB |
| three seconds after it is closed | 209 MB | 275 to 307 MB |
| 70 s later | 178 MB | 274 MB |

While it is up the capture is there three times (as it was read, for the
host to cut the selection from; as it was read and dimmed, for Slint to
draw) and the renderer's own copies of the two it draws twice more; and
the window has a buffer of its size. A 4K screen is 31.6 MB.

Closed, it holds the window's buffer only, and after a minute not that:
the window is given up and a small one made in its place (a window made
smaller keeps the larger buffer).

That the rest comes back had to be seen to. glibc keeps freed pieces of
up to 32 MB in a heap of the thread that freed them once one such piece
has been freed, and nothing gives the end of such a heap back,
`malloc_trim` included: Heyo held 127 MB more after the first screenshot
and 130 to 290 MB more after later ones, for good (the web view's path
holds on to about 100 MB the same way). Heyo now sets glibc's threshold
for giving a piece a mapping of its own when it starts (`lib.rs`,
`give_large_memory_back`), which keeps glibc from moving it. The price is
that such a piece is new memory each time, handed out page by page: the
frozen screen is up some 35 ms later than it was without (177 ms against
about 143).

### The pixels

`experiments/slint-settings/scripts/screenshot.sh` reads the screen with a
selection on it, and again after the selection was moved and resized: of
the pixels inside the selection none differs from the desktop as it was
before, and of those outside it (30 px away from its outline, handles
and label) none differs from the desktop dimmed to 154 of 256, rounded
down (`results/screenshot-x11.txt`). The window is 24 bits deep: it is
said to hide what is behind it, so a compositor has nothing to blend.

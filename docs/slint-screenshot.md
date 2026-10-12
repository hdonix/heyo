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
- [ ] The frozen screen is on display as soon as it has been read, before
  anything else is ready. The windows on the screen are asked for
  meanwhile and arrive after.
- [ ] A capture that fails is said in a notification ("Screenshot
  failed"), or told to the extension that asked.
- [ ] A capture of the user's own takes the overlay from an extension's,
  which hears "cancelled"; an extension that waits hears within ten
  minutes whatever happens.

### What is on the screen

- [ ] The frozen picture of the monitor, one of its pixels a device
  pixel, no scaling and no colour change, covering the whole monitor, the
  desktop's panels too, with no opening animation.
- [ ] Everything but the selection dimmed (154 of 256).
- [ ] Before there is a selection: a hint in the middle, "Drag to select
  an area" (mode text: "Drag over the text to copy"), ", or click a
  window" when windows are known, " · Esc to cancel".
- [ ] The window under the pointer, where a click would take it: shown
  undimmed with the outline inside it (so that a window filling the screen
  has one). It changes when the pointer crosses into another window; not
  shown over the selection, a tool, an annotation, or while dragging.
- [ ] The selection: undimmed, outlined 1.5 px in the accent.
- [ ] Its size in the capture's pixels ("2600 × 1400") in a small dark
  label above its top left corner.
- [ ] Eight round handles (9 px, accent, white edge) on corners and
  sides, in mode capture with the selection tool, not while dragging.
- [ ] The pointer: a crosshair; "move" over the selection; the resize
  arrows over each handle; a crosshair in the selection with a drawing
  tool; "grab" over an annotation that can be dragged; the default arrow
  over the toolbar.
- [ ] A status line at the bottom ("Recognizing text…", or an error in
  red, e.g. "No text was found in the selection.").
- [ ] Nothing of the last capture flashes when the next one opens.

### Selecting

- [ ] A drag from outside the selection makes a new one (and drops the
  annotations and the tool).
- [ ] A press and release without a drag (less than 6 px either way)
  takes the window pressed on, exactly; where there is none, the selection
  is dropped.
- [ ] A drag inside the selection moves it; it stays on the screen.
- [ ] A drag on a handle resizes by that handle's sides; never smaller
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
- [ ] Annotations follow the selection when it moves or resizes without
  jitter, and are cut off at its edge.
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

- [ ] The overlay holds the keyboard (a grab on X11: the focus stays
  where it was). Esc always reaches it.
- [ ] If Heyo stops answering while it is up, the keyboard is let go
  within a few seconds (today: Heyo ends itself).
- [ ] If Heyo is killed, nothing is left on screen and the keyboard is
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

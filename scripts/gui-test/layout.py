#!/usr/bin/env python3
"""Where the things are that a window cannot be asked for yet: the one
place the scripts carry layout as numbers.

    layout.py toolbar BUTTON RIGHT BOTTOM SCALE   X Y of a button of the screenshot's toolbar, for a
                                                  selection whose right and bottom edges are those
                                                  (the screen's pixels)
    layout.py toolbar-box RIGHT BOTTOM SCALE      the toolbar itself: X Y W H
    layout.py pill body|cancel X Y W H            X Y of the dictation pill's body, or of its button,
                                                  in a pill window that is there (`place`'s figures)

The settings window says where its controls are (ask.py): its rows,
switches and buttons have a role and a name on the accessibility bus. The
screenshot overlay and the pill do not: the toolbar's buttons are there as
pictures without a name, its colours not at all, and of the pill only its
text. Until they are named (src-tauri/src/slint_windows/ui/screenshot.slint
and pill.slint), what the markup says is repeated here, once:

- the toolbar is 499 by 40 logical pixels, its right end at the
  selection's, 8 below it, or as far left as the screen lets it be (8
  from its edge); a button every 32 from 20 in, and 11 more after the
  separator that follows undo;
- the pill is in the middle of its window, which is 360 logical pixels
  wide; its button shows when the pointer is on the pill, 49 right of the
  middle while it says "Listening".
"""
import sys

TOOLBAR_WIDTH, TOOLBAR_HEIGHT, TOOLBAR_BELOW, TOOLBAR_SHADOW = 499, 40, 8, 40
BUTTONS = ["rect", "arrow", "pen", "text", "blur", "red", "yellow", "green", "blue", "undo", "ocr", "pin", "save", "copy", "cancel"]
PILL_WIDTH, PILL_CANCEL = 360, 49


def toolbar_box(right, bottom, scale):
    """The toolbar's rectangle on the screen: left, top, width, height."""
    left = max(8, right // scale - TOOLBAR_WIDTH)
    return left * scale, (bottom // scale + TOOLBAR_BELOW) * scale, TOOLBAR_WIDTH * scale, TOOLBAR_HEIGHT * scale


def toolbar_button(button, right, bottom, scale):
    """The middle of a button, by its name or by its place in the toolbar."""
    index = BUTTONS.index(button) if button in BUTTONS else int(button)
    left = max(8, right // scale - TOOLBAR_WIDTH)
    return (left + 20 + 32 * index + (11 if index >= 10 else 0)) * scale, (bottom // scale + TOOLBAR_BELOW + TOOLBAR_HEIGHT // 2) * scale


def pill(part, x, y, width, height):
    scale = width / PILL_WIDTH
    across = {"body": -20, "cancel": PILL_CANCEL}[part]
    return x + width // 2 + int(across * scale), y + height // 2


if __name__ == "__main__":
    what, arguments = (sys.argv[1] if len(sys.argv) > 1 else ""), sys.argv[2:]
    if what == "toolbar" and len(arguments) == 4:
        print(*toolbar_button(arguments[0], *map(int, arguments[1:])))
    elif what == "toolbar-box" and len(arguments) == 3:
        print(*toolbar_box(*map(int, arguments)))
    elif what == "pill" and len(arguments) == 5:
        print(*pill(arguments[0], *map(int, arguments[1:])))
    else:
        sys.exit(__doc__)

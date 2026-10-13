#!/usr/bin/env python3
"""Inside the Sway session: checks the dictation pill as a layer surface.

  check-sway.py

Sway offers the layer shell, so the pill is a surface of Heyo's own on the
overlay layer (src-tauri/src/slint_windows/layer.rs), not a window: it is
not in Sway's tree, and is looked for in a picture of the screen. Checked:
nothing of Heyo's on the screen before; with `heyo --dictate`, the pill's
pixels at the bottom centre of the output, a tenth of its height up, and
clear round its corners; above a fullscreen window; the window that has
the keyboard the same before, during, after a click on the pill, and
after; a click beside the pill, inside its 360 by 44, going to the window
underneath (which Sway then gives the keyboard, and the pill is not
pressed); and the pill gone afterwards.

And two pinned captures, which are layer surfaces too (Heyo is started
with HEYO_TEST_PIN): each where it was said, pixel for pixel, above the
windows; one dragged, which must go as far as the pointer went; closed by
a double click and by the other button; none of it moving the keyboard.

And the screenshot overlay, a layer surface over the whole output that
has every key while it is there: the screen dimmed from corner to corner;
a key typed not reaching the application; what is selected drawn as the
screen was, pixel for pixel; a line typed with the text tool, with a key
held down pressed again and again, and undone with Control and a key;
a line typed through an input method (ime.py), which is asked for text
when the line is begun, whose composing is shown and whose choice stays;
Escape giving it up, the keyboard back where it was, and what is typed
then reaching the application. And with a second output beside the
first, the overlay on the one the pointer is on, and on that one only.

Where the text tool's button is, is written here as numbers (the
toolbar's right end at the selection's, 8 below it, a button every 32
from 20 in): a layer surface has no names to ask for on the accessibility
bus, as a window of the toolkit's has (../../gui-test/ask.py).

Whether dictated words reach an application here is not checked: Heyo has
no way of typing into another application under Sway yet.
"""
import os, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from swaybed import left, pill_there, pinned, top, input_method, application, around, at, differing, check, click, dark, failed, focused, height, heyo, keys, output, outputs, picture, point, pointer, press, release, same, scale, sway, typed, wait, width, windows

# Two windows side by side, light ones, one after the other: Sway puts
# the second on the right and gives it the keyboard.
apps = [application("Left", "check-sway-left"), application("Right", "check-sway-right")]
check("an application has the keyboard", focused() == "Right", focused())
check("nothing of Heyo's is on the screen before it is asked for", not [window for window in windows() if (window.get("name") or "").startswith("Heyo")] and not pill_there()[0])

heyo("--dictate")
check("the pill comes up at the bottom centre, a tenth up, clear round its corners", bool(wait(lambda: pill_there()[0], 6)), pill_there()[1])
check("it is no window of Sway's tree: a layer surface", not [window for window in windows() if window.get("name") == "Heyo Dictation"])
check("the application still has the keyboard", focused() == "Right", focused())

# Beside the pill, inside its window, on the left: a click there is the
# window's underneath, which Sway gives the keyboard for it; it is not a
# press on the pill, which would end the recording.
point(left + 8, top + 22)
click()
check("a click beside the pill, inside its window, goes to the window underneath", focused() == "Left", focused())
check("... and is not a press on the pill", pill_there()[0], pill_there()[1])
point(width * 3 // 4, height // 3)
click()
check("... and the keyboard goes back with a click on the other window", focused() == "Right", focused())

# Above a window that fills the screen.
sway("fullscreen", "enable")
time.sleep(0.8)
check("the pill is above a fullscreen window", pill_there()[0], pill_there()[1])
check("... which has the keyboard", focused() == "Right", focused())

# A click on the pill finishes, and does not take the keyboard.
point(left + 180, top + 22)
time.sleep(0.3)
check("the pointer on the pill does not take the keyboard", focused() == "Right", focused())
click()
check("... nor does a click on it", focused() == "Right", focused())
gone = wait(lambda: not pill_there()[0], 10)
if not gone:
    heyo("--dictate")
    gone = wait(lambda: not pill_there()[0], 10)
check("the pill is gone afterwards", bool(gone), pill_there()[1])
check("... and the application still has the keyboard", focused() == "Right", focused())
sway("fullscreen", "disable")
time.sleep(0.6)

# The pinned captures: 300 by 200 at 200,160 and 200 by 120 at 800,160, in
# the screen's own pixels.
check("two pinned captures are where they were said, pixel for pixel, above the windows", pinned(200, 160, 300, 200) and pinned(800, 160, 200, 120))
# The pointer goes by logical pixels; the pictures are in the screen's own.
point((200 + 150) / scale, (160 + 100) / scale)
press()
for step in range(1, 9):
    point((350 + step * 20) / scale, (260 + step * 10) / scale, 0.06)
release()
time.sleep(0.5)
moved = [(dx, dy) for dx in range(140, 164, 2) for dy in range(70, 84, 2) if pinned(200 + dx, 160 + dy, 300, 200)] if not pinned(200, 160, 300, 200) else []
check("one dragged by 160,80 has gone about as far (less the few pixels before a drag is one)", bool(moved), moved or "still where it was")
check("... and the other has not moved", pinned(800, 160, 200, 120))
check("... and the keyboard is where it was", focused() == "Right", focused())
if moved:
    dx, dy = moved[0]
    point((350 + dx) / scale, (260 + dy) / scale)
    for _ in range(2):
        press()
        release()
    time.sleep(0.6)
    check("a double click closes it", not pinned(200 + dx, 160 + dy, 300, 200))
point((800 + 100) / scale, (160 + 60) / scale)
press("right")
release("right")
time.sleep(0.6)
check("the other button closes the other", not pinned(800, 160, 200, 120))
check("... and the keyboard is still where it was", focused() == "Right", focused())

# The screenshot overlay. The right window has the keyboard; what is
# selected is in the left one, where no cursor blinks.
point(width * 3 // 4, height // 3)
click()
keys("-k", "BackSpace")
had = typed("check-sway-right")
S = int(scale)
W, H = width * S, height * S
corners = [(6, 6), (W - 6, 6), (6, H - 6), (W - 6, H - 6), (W // 2, 6), (6, H // 2)]
before = picture()
heyo("--open", "heyo:screenshot")
up = wait(lambda: (lambda shot: shot if same(shot, before, corners) == len(corners) else None)(picture()), 8)
check("the overlay comes up, the screen dimmed from corner to corner", bool(up), "the corners are as they were")
check("it is no window of Sway's tree: a layer surface", not [window for window in windows() if (window.get("name") or "").startswith("Heyo")])
keys("a")
check("a key typed then does not reach the application", typed("check-sway-right") == had, repr(typed("check-sway-right")))

# A selection, in the output's logical pixels.
x0, y0, x1, y1 = int(width * 0.06), int(height * 0.15), int(width * 0.4), int(height * 0.6)
point(x0, y0)
press()
for step in range(1, 9):
    point(x0 + (x1 - x0) * step // 8, y0 + (y1 - y0) * step // 8, 0.05)
release()
time.sleep(0.6)
selected = picture()
# Away from its edge, where its handles are drawn over it.
inside = [(x, y) for x in range((x0 + 12) * S, (x1 - 12) * S, 7) for y in range((y0 + 12) * S, (y1 - 12) * S, 7)]
check(f"what is selected is drawn as the screen was, pixel for pixel ({len(inside)} places)", same(selected, before, inside) == 0, f"{same(selected, before, inside)} differ, {around(differing(selected, before, inside))}")
check("... and the rest of the screen stays dimmed", same(selected, before, corners) == len(corners))


def marked(shot, down):
    """How wide what is drawn over the selection is, in the band that far down it where a line is typed."""
    columns = [x for x in range((x0 + 12) * S, (x1 - 12) * S) if any(at(shot, x, y) != at(before, x, y) for y in range((y0 + down - 20) * S, (y0 + down + 20) * S, 2))]
    return (max(columns) - min(columns) + 1) if columns else 0


# The text tool, and a line typed where it is clicked.
toolbar_left = max(8, x1 - 499)
point(toolbar_left + 20 + 32 * 3, y1 + 8 + 20)
click()
point(x0 + 40, y0 + 90)
click()
keys("x")
keys("-k", "Return")
one = marked(picture(), 90)
check("a key typed with the text tool is drawn in the selection, and Enter ends the line", one > 0)
# The next line, lower down: one key, held for a second and a half.
point(x0 + 40, y0 + 150)
click()
keys("-P", "x", "-s", "1500", "-p", "x")
keys("-k", "Return")
many = marked(picture(), 150)
check("a key held down is pressed again and again", one > 0 and many > 6 * one, f"one {one} pixels wide, held {many}")
keys("-M", "ctrl", "z", "-m", "ctrl")
shot = picture()
check("Control and Z take the last line back, and not the first", marked(shot, 150) == 0 and marked(shot, 90) == one, (marked(shot, 150), marked(shot, 90)))

# A line typed through an input method, with no key pressed.
method = input_method()
check("the session takes an input method", bool(method))
if method:
    check("... which nothing asks for text while no line is being typed", method("active") == "no")
    point(x0 + 40, y0 + 210)
    click()
    check("it is asked for text when a line is begun", method("active") == "yes")
    method("compose", "nihao")
    composing = marked(picture(), 210)
    check("what it is composing is shown in the line", composing > 2 * one, f"{composing} pixels wide, one letter {one}")
    method("choose", "你好你好你好你好")
    keys("-k", "Return")
    chosen = marked(picture(), 210)
    check("what is chosen is the line, and stays when Enter ends it", chosen > 3 * one and chosen != composing, f"{chosen} pixels wide, composing was {composing}")
    check("... after which it is asked for nothing", method("active") == "no")

# Escape gives the screenshot up; the keyboard is where it was.
keys("-k", "Escape")
gone = wait(lambda: same(picture(), before, corners) == 0, 4)
if not gone:
    keys("-k", "Escape")
    gone = wait(lambda: same(picture(), before, corners) == 0, 4)
check("Escape takes the overlay off the screen", bool(gone))
check("... and the application has the keyboard again", focused() == "Right", focused())
keys("hello")
check("... and what is typed then reaches it", typed("check-sway-right") == had + "hello", repr(typed("check-sway-right")))

# A second output, which Sway puts beside the first: the screenshot is of
# the one the pointer is on.
sway("create_output")
both = wait(lambda: (lambda there: there if len(there) == 2 else None)(outputs()), 5) or []
other = next((one["rect"] for one in both if one["name"] != output["name"]), None)
check("Sway has a second output", bool(other))
if other:
    time.sleep(1.5)
    # The pointer goes by the whole desktop now, and a picture is of all of it.
    across, down = max(one["rect"]["x"] + one["rect"]["width"] for one in both), max(one["rect"]["y"] + one["rect"]["height"] for one in both)
    for _ in range(2):
        pointer("to", other["x"] + other["width"] // 2, other["y"] + other["height"] // 2, across, down)
        time.sleep(0.4)
    # Something light on it to be dimmed: an output with nothing on it is
    # black, and stays black under the overlay.
    apps.append(application("Far", "check-sway-far"))
    before = picture()
    by = before[0] / across
    on = lambda rect: [(int((rect["x"] + rect["width"] * fx) * by), int((rect["y"] + rect["height"] * fy) * by)) for fx, fy in ((0.03, 0.03), (0.97, 0.03), (0.03, 0.97), (0.97, 0.97), (0.5, 0.25))]
    heyo("--open", "heyo:screenshot")
    shot = wait(lambda: (lambda shot: shot if same(shot, before, on(other)) or same(shot, before, on(output["rect"])) else None)(picture()), 8)
    check("the overlay is on the output the pointer is on", bool(shot) and same(shot, before, on(other)) == 5, shot and (same(shot, before, on(other)), same(shot, before, on(output["rect"]))))
    check("... and not on the other", bool(shot) and same(shot, before, on(output["rect"])) == 0, shot and [(place, at(shot, *place), at(before, *place)) for place in differing(shot, before, on(output["rect"]))])
    keys("-k", "Escape")
    check("... and Escape takes it off", bool(wait(lambda: same(picture(), before, on(other)) == 0, 4)))
for app in apps:
    app.terminate()

PASSED = "/tmp/check-passed"
if os.path.exists(PASSED):
    os.remove(PASSED)
if failed:
    sys.exit(f"{len(failed)} failed")
open(PASSED, "w").close()

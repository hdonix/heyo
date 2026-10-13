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
with HEYO_TEST_PIN, since it cannot read the screen under Sway yet): each
where it was said, pixel for pixel, above the windows; one dragged, which
must go as far as the pointer went; closed by a double click and by the
other button; none of it moving the keyboard.

Whether dictated words reach an application here is not checked: Heyo has
no way of typing into another application under Sway yet.
"""
import json, os, subprocess, sys, time

HERE = os.path.dirname(os.path.abspath(__file__))
HEYO = f"/heyo/src-tauri/target/{os.environ.get('HEYO_BUILD', 'release')}/heyo"
failed = []


def check(what, ok, detail=""):
    print(("ok   " if ok else "FAIL ") + what + (f"  ({detail})" if detail and not ok else ""))
    if not ok:
        failed.append(what)


def sway(*args):
    return subprocess.run(["swaymsg", *args], capture_output=True, text=True).stdout


def tree():
    return json.loads(sway("-t", "get_tree"))


def windows(node=None):
    node = node or tree()
    found = [node] if node.get("pid") else []
    for child in node.get("nodes", []) + node.get("floating_nodes", []):
        found += windows(child)
    return found


def focused():
    return next((window.get("name") for window in windows() if window.get("focused")), None)


def wait(condition, seconds=6.0):
    end = time.time() + seconds
    while time.time() < end:
        value = condition()
        if value:
            return value
        time.sleep(0.15)
    return None


def picture():
    """The screen, as (width, height, pixels) with pixels row by row, three or four bytes each."""
    import gi
    gi.require_version("GdkPixbuf", "2.0")
    from gi.repository import GdkPixbuf
    subprocess.run(["grim", "/tmp/check-sway.png"], check=True)
    image = GdkPixbuf.Pixbuf.new_from_file("/tmp/check-sway.png")
    return image.get_width(), image.get_height(), image.get_rowstride(), image.get_n_channels(), image.get_pixels()


def at(shot, x, y):
    _, _, stride, channels, pixels = shot
    offset = int(y) * stride + int(x) * channels
    return tuple(pixels[offset:offset + 3])


def dark(colour):
    return max(colour) < 60


output = next(output for output in json.loads(sway("-t", "get_outputs")) if output.get("active"))
scale = output["scale"]
# In the output's own logical pixels, which the pointer goes by.
width, height = output["rect"]["width"], output["rect"]["height"]
# Where the pill's window is: 360 by 44, at the bottom centre, a tenth up.
left, top = (width - 360) // 2, height - 44 - height // 10

# A pointer of the session's own (pointer.py): headless Sway has none, and
# its clients are told nothing of one that `swaymsg seat - cursor` moves.
POINTER = subprocess.Popen([os.path.join(HERE, "pointer.py")], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
POINTER.stdout.readline()


def pointer(*words):
    POINTER.stdin.write(" ".join(str(word) for word in words) + "\n")
    POINTER.stdin.flush()
    POINTER.stdout.readline()


def point(x, y, settle=0.4):
    pointer("to", int(x), int(y), width, height)
    time.sleep(settle)


def press(button="left"):
    pointer("down", button)
    time.sleep(0.12)


def release(button="left"):
    pointer("up", button)
    time.sleep(0.12)


def click():
    press()
    release()
    time.sleep(0.5)




def pill_there():
    """Whether the pill is drawn where it should be: dark in its middle band, and not at the corners of its window."""
    shot = picture()
    middle = [dark(at(shot, (left + 180 + dx) * scale, (top + 22 + dy) * scale)) for dx, dy in ((-30, -10), (30, -10), (-30, 10), (30, 10))]
    corners = [dark(at(shot, x * scale, y * scale)) for x, y in ((left + 1, top + 1), (left + 358, top + 1), (left + 1, top + 42), (left + 358, top + 42))]
    return all(middle) and not any(corners), (middle, corners)


# Two windows side by side, light ones, one after the other: Sway puts
# the second on the right and gives it the keyboard.
apps = []
for title, app in (("Left", "check-sway-left"), ("Right", "check-sway-right")):
    apps.append(subprocess.Popen([os.path.join(HERE, "window.py"), title, app], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL))
    wait(lambda: [window for window in windows() if window.get("name") == title])
    time.sleep(0.8)
check("an application has the keyboard", focused() == "Right", focused())
check("nothing of Heyo's is on the screen before it is asked for", not [window for window in windows() if (window.get("name") or "").startswith("Heyo")] and not pill_there()[0])

subprocess.run([HEYO, "--dictate"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
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
    subprocess.run([HEYO, "--dictate"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    gone = wait(lambda: not pill_there()[0], 10)
check("the pill is gone afterwards", bool(gone), pill_there()[1])
check("... and the application still has the keyboard", focused() == "Right", focused())
sway("fullscreen", "disable")
time.sleep(0.6)

# The pinned captures: 300 by 200 at 200,160 and 200 by 120 at 800,160, in
# the screen's own pixels, each pink on its left half and blue on its right.
PINK, BLUE = (216, 27, 96), (30, 136, 229)


def is_colour(seen, colour):
    return all(abs(a - b) <= 3 for a, b in zip(seen, colour))


def pinned(x, y, w, h):
    """Whether a pinned capture of that size is at that place of the picture."""
    shot = picture()
    return is_colour(at(shot, x + w // 4, y + h // 2), PINK) and is_colour(at(shot, x + 3 * w // 4, y + h // 2), BLUE)


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
for app in apps:
    app.terminate()

PASSED = "/tmp/check-passed"
if os.path.exists(PASSED):
    os.remove(PASSED)
if failed:
    sys.exit(f"{len(failed)} failed")
open(PASSED, "w").close()

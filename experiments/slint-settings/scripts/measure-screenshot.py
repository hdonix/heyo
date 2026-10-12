#!/usr/bin/env python3
"""Times Heyo's screenshot overlay from outside, with the screen as the clock.

    measure-screenshot.py LABEL RUNS small|large|open|pen|blur

Run by measure-screenshot.sh, which starts Heyo in the sandbox: a screen of
3840x2160 at scale 1 under GNOME Shell, showing the desktop's own
background.

For each run a screenshot is asked for as a hotkey asks (`heyo --open
heyo:screenshot`, a second process handing the request to the first). Then:

- `small`, `large`: the pointer's button goes down and the pointer jumps to
  where the selection covers a watched pixel and back to where it does
  not, fifteen times each way, a quarter of a second apart; then it sweeps
  across that edge for eight seconds without standing still, a movement
  every 8 ms. `small` is a selection of about 1000 by 400 pixels, `large`
  one of 3040 by 1800: most of the screen.
- `open`: the screenshot is only asked for and given up.
- `pen`: a selection is made, the pen chosen, and three strokes drawn
  across it, each of some 400 places, one every 8 ms. Four pixels on each
  stroke's way are watched: how long after the pen passed each it was
  the pen's colour.
- `blur`: a selection of most of the screen, the blur tool, and a drag
  whose far corner jumps between two places, so that a watched pixel (a
  star of the background) is in the blurred part and out of it, fifteen
  times each way; the blurred part is 3400 by 1600 and 2700 by 1100.

$WATCH (`overlay-bench --watch`) reads the watched pixels again and again
and says when each changed. That tells when the frozen screen was up (a
pixel dimmed to 154 of 256), when the overlay was ready (the hint was
drawn in the middle), and when the overlay had followed the pointer.

What Heyo wrote of its own frames ($MEASURE_FRAMES, its
HEYO_SETTINGS_FRAMES) is read too: how long each took to draw and hand to
the display; the first three of a screenshot apart, in which the renderer
takes the two pictures in.

It moves the pointer with XTest, on the sandbox's display and no other.
One line of JSON is printed, and appended to $MEASURE_RESULTS.
"""
import ctypes
import json
import os
import random
import re
import statistics
import subprocess
import sys
import time

DISPLAY = os.environ.get("SLINT_SETTINGS_DISPLAY", "")
if not DISPLAY or DISPLAY == ":0" or os.environ.get("DISPLAY") != DISPLAY:
    sys.exit("not the sandbox's display")
label, runs, size = sys.argv[1], int(sys.argv[2]), sys.argv[3]
HEYO, WATCH, OUT = os.environ["HEYO"], os.environ["WATCH"], os.environ["MEASURE_OUT"]
os.makedirs(OUT, exist_ok=True)
WATCHED, MIDDLE = (3040, 1080), (1920, 1080)
# Where the button goes down, and the two places the pointer goes between:
# at the first the selection covers the watched pixel, at the second not.
if size in ("small", "open"):
    PRESS, IN, OUT_AT = (2140, 880), (3140, 1280), (2540, 980)
    SWEEP_Y, SWEEP = 1280, (2540, 3140)
else:
    PRESS, IN, OUT_AT = (100, 100), (3140, 1900), (2540, 1900)
    SWEEP_Y, SWEEP = 1900, (2540, 3140)
RED = (255, 69, 58)
# The pen's strokes: their rows, and the pixels watched on each.
ROWS, MARKS = (900, 1000, 1100), (2400, 2700, 3000, 3300)

x11 = ctypes.CDLL("libX11.so.6")
xtst = ctypes.CDLL("libXtst.so.6")
x11.XOpenDisplay.restype = ctypes.c_void_p
display = ctypes.c_void_p(x11.XOpenDisplay(DISPLAY.encode()))
if not display:
    sys.exit("no display")


def now():
    return time.time() * 1000


def move(x, y):
    xtst.XTestFakeMotionEvent(display, -1, int(x), int(y), 0)
    x11.XFlush(display)


def button(down):
    xtst.XTestFakeButtonEvent(display, 1, 1 if down else 0, 0)
    x11.XFlush(display)


def drag(start, end):
    """A selection dragged out, and let go."""
    move(*start)
    time.sleep(0.1)
    button(True)
    time.sleep(0.1)
    move((start[0] + end[0]) // 2, (start[1] + end[1]) // 2)
    time.sleep(0.1)
    move(*end)
    time.sleep(0.2)
    button(False)
    time.sleep(0.3)


def tool(index, selection_end):
    """A click on the toolbar's button of that place (at scale 1)."""
    move(selection_end[0] - 499 + 20 + 32 * index, selection_end[1] + 28)
    time.sleep(0.15)
    button(True)
    button(False)
    time.sleep(0.3)


def near(colour, other, within=8):
    return max(abs(a - b) for a, b in zip(colour, other)) <= within


def spread(values):
    """Median, 95th and worst."""
    if not values:
        return None
    ordered = sorted(values)
    return [round(statistics.median(ordered), 1), round(ordered[min(len(ordered) - 1, int(len(ordered) * 0.95))], 1), round(ordered[-1], 1)]


def star():
    """A bright pixel of the background with dark round it: blurred, it is plainly another colour."""
    from PIL import ImageGrab

    box = (300, 300, 850, 650)
    sky = ImageGrab.grab(xdisplay=DISPLAY).convert("RGB").crop(box)
    best = max(((sum(sky.getpixel((x, y))), x, y) for x in range(sky.width) for y in range(sky.height)), key=lambda found: found[0])
    return (box[0] + best[1], box[1] + best[2])


frozen, ready, jumps, sweeps, missed = [], [], [], [], 0
FRAMES = os.environ.get("MEASURE_FRAMES")
first_frames, later_frames = [], []
for run in range(1, runs + 1):
    move(300, 1900)
    time.sleep(0.3)
    if size == "pen":
        points = [(x, y) for y in ROWS for x in MARKS]
    elif size == "blur":
        points = [star()]
    else:
        points = [WATCHED, MIDDLE]
    log = f"{OUT}/{label}-{size}-{run}.watch".replace(" ", "-").replace(",", "")
    watching = [WATCH, "--watch", ":".join(f"{x},{y}" for x, y in points), {"open": "4.5", "pen": "20", "blur": "16"}.get(size, "26")]
    watcher = subprocess.Popen(watching, stdout=open(log, "w"), stderr=subprocess.DEVNULL, env={**os.environ, "GDK_SCALE": "1", "GDK_BACKEND": "x11"})
    time.sleep(1.2)
    asked = now()
    subprocess.run([HEYO, "--open", "heyo:screenshot"], check=False, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    time.sleep(2)
    moves, crossings, passed = [], [], []
    if size in ("small", "large"):
        move(*PRESS)
        time.sleep(0.1)
        button(True)
        time.sleep(0.3)
        for _ in range(15):
            moves.append(("in", now()))
            move(*IN)
            time.sleep(random.uniform(0.2, 0.3))
            moves.append(("out", now()))
            move(*OUT_AT)
            time.sleep(random.uniform(0.2, 0.3))
        # The sweep: across the edge and back, never still.
        x, step, inside = SWEEP[0], 10, False
        until = now() + 8000
        while now() < until:
            x += step
            if x >= SWEEP[1] or x <= SWEEP[0]:
                step = -step
            at = now()
            move(x, SWEEP_Y)
            if (x > WATCHED[0]) != inside:
                inside = x > WATCHED[0]
                crossings.append(("in" if inside else "out", at))
            time.sleep(0.008 - min(0.006, (now() - at) / 1000))
        time.sleep(0.4)
        button(False)
        time.sleep(0.3)
    elif size == "pen":
        end = (3400, 1500)
        drag((2000, 800), end)
        tool(2, end)
        for row, y in enumerate(ROWS):
            move(2100, y)
            time.sleep(0.1)
            button(True)
            waiting = list(enumerate(MARKS))
            for x in range(2100, 3351, 3):
                at = now()
                move(x, y)
                if waiting and x >= waiting[0][1]:
                    passed.append((row * len(MARKS) + waiting.pop(0)[0], at))
                time.sleep(0.008 - min(0.006, (now() - at) / 1000))
            button(False)
            time.sleep(0.4)
    elif size == "blur":
        end = (3700, 1900)
        drag((100, 100), end)
        tool(4, end)
        move(3600, 1800)
        time.sleep(0.1)
        button(True)
        time.sleep(0.2)
        for _ in range(15):
            moves.append(("in", now()))
            move(200, 200)
            time.sleep(random.uniform(0.2, 0.3))
            moves.append(("out", now()))
            move(900, 700)
            time.sleep(random.uniform(0.2, 0.3))
        button(False)
        time.sleep(0.3)
    subprocess.run(["xdotool", "key", "Escape"], check=False)
    watcher.wait()
    if FRAMES and os.path.exists(FRAMES):
        took = [float(line.split()[1]) for line in open(FRAMES) if line.strip()]
        first_frames.append([round(v, 1) for v in took[:3]])
        later_frames.extend(took[3:])
        os.remove(FRAMES)
    watch = [[int(v) for v in line.split()] for line in open(log) if line.strip()]
    seen = [[(t, (r, g, b)) for t, which, r, g, b in watch if which == index] for index in range(len(points))]
    if size == "pen":
        # When each watched pixel was the pen's colour, after the pen passed it.
        for index, at in passed:
            drawn = next((t for t, colour in seen[index] if t >= at - 1 and near(colour, RED, 40)), None)
            if drawn is None or drawn - at > 2000:
                missed += 1
            else:
                jumps.append(max(0, drawn - at))
        continue
    pixel = seen[0]
    # The watched pixel as the desktop has it, and as the overlay dims it.
    clear = next((colour for t, colour in reversed(pixel) if t < asked), (0, 0, 0))
    dim = [v * 154 // 256 for v in clear]
    if max(clear) < 80:
        sys.exit(f"the watched pixel is too dark to tell dimmed from not: {clear}")
    if size == "blur":
        # In the blurred part the star is not the star; out of it, it is.
        for kind, at in moves:
            wanted = (lambda colour: not near(colour, clear, 30)) if kind == "in" else (lambda colour: near(colour, clear))
            found = next((t for t, colour in pixel if t >= at - 1 and wanted(colour)), None)
            if found is None or found - at > 2000:
                missed += 1
            else:
                jumps.append(max(0, found - at))
        continue
    up = next((t for t, colour in pixel if t >= asked and near(colour, dim)), None)
    if up is None:
        print(f"run {run}: the frozen screen was not seen", file=sys.stderr)
        continue
    frozen.append(round(up - asked))
    # The hint is there when the middle is neither the desktop nor that dimmed.
    middle = seen[1]
    before = next((colour for t, colour in reversed(middle) if t < asked), (0, 0, 0))
    plain = [v * 154 // 256 for v in before]
    hinted = next((t for t, colour in middle if t >= asked and not near(colour, plain) and not near(colour, before)), None)
    ready.append(round((max(hinted, up) if hinted else up) - asked))

    def followed(kind, at, limit):
        wanted = clear if kind == "in" else dim
        found = next((t for t, colour in pixel if t >= at - 1 and near(colour, wanted)), None)
        return None if found is None or found - at > limit else max(0, found - at)

    for which, limit, into in ((moves, 2000, jumps), (crossings, 400, sweeps)):
        for kind, at in which:
            took = followed(kind, at, limit)
            if took is None:
                missed += 1
            else:
                into.append(took)

# Heyo's own marks (HEYO_OVERLAY_DEBUG): how long after it heard of each
# screenshot each step was done.
marks = {}
try:
    for line in open(os.environ.get("MEASURE_LOG", ""), errors="replace"):
        found = re.match(r"heyo: (.+) at \d+ ms, ([0-9.]+) ms after", line.strip())
        if found:
            marks.setdefault(found.group(1), []).append(round(float(found.group(2))))
except OSError:
    pass
result = {
    "label": label,
    "selection": size,
    "screenshots": len(frozen),
    "frozen_ms": frozen,
    "ready_ms": ready,
    "jump_ms": spread(jumps),
    "jumps": len(jumps),
    "sweep_ms": spread(sweeps),
    "sweeps": len(sweeps),
    "missed": missed,
    "first_frames_ms": first_frames,
    "frame_ms": spread(later_frames),
    "frames": len(later_frames),
    "heyo_says_ms": marks,
}
print(json.dumps(result))
if os.environ.get("MEASURE_RESULTS"):
    with open(os.environ["MEASURE_RESULTS"], "a") as results:
        results.write(json.dumps(result) + "\n")

#!/usr/bin/env python3
"""Times Heyo's screenshot overlay from outside, with the screen as the clock.

    measure-screenshot.py LABEL RUNS small|large|open

Run by measure-screenshot.sh, which starts Heyo in the sandbox: a screen of
3840x2160 at scale 1 under GNOME Shell, showing the desktop's own
background (the watched pixel, 3040,1080, is a bright one of it).

For each run a screenshot is asked for as a hotkey asks (`heyo --open
heyo:screenshot`, a second process handing the request to the first). Then
the pointer's button goes down and the pointer jumps to where the selection
covers the watched pixel and back to where it does not, fifteen times each
way, a quarter of a second apart; then it sweeps across that edge for
eight seconds without standing still, a movement every 8 ms. `small` is a
selection of about 1000 by 400 pixels, `large` one of 3040 by 1800: most of
the screen. With `open` the screenshot is only asked for and given up.

$WATCH (`overlay-bench --watch`) reads the watched pixel and the middle of
the screen again and again and says when each changed. That tells when the
frozen screen was up (the pixel dimmed to 154 of 256), when the overlay
was ready (the hint was drawn in the middle), and when the selection had
followed the pointer (the pixel as the desktop has it, and dimmed again).

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


def near(colour, other):
    return max(abs(a - b) for a, b in zip(colour, other)) <= 8


def spread(values):
    """Median, 95th and worst."""
    if not values:
        return None
    ordered = sorted(values)
    return [round(statistics.median(ordered), 1), round(ordered[min(len(ordered) - 1, int(len(ordered) * 0.95))], 1), round(ordered[-1], 1)]


frozen, ready, jumps, sweeps, missed = [], [], [], [], 0
FRAMES = os.environ.get("MEASURE_FRAMES")
first_frames, later_frames = [], []
for run in range(1, runs + 1):
    move(300, 1900)
    time.sleep(0.3)
    log = f"{OUT}/{label}-{size}-{run}.watch"
    watching = [WATCH, "--watch", f"{WATCHED[0]},{WATCHED[1]}:{MIDDLE[0]},{MIDDLE[1]}", "4.5" if size == "open" else "26"]
    watcher = subprocess.Popen(watching, stdout=open(log, "w"), stderr=subprocess.DEVNULL, env={**os.environ, "GDK_SCALE": "1", "GDK_BACKEND": "x11"})
    time.sleep(1.2)
    asked = now()
    subprocess.run([HEYO, "--open", "heyo:screenshot"], check=False, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    time.sleep(2)
    moves, crossings = [], []
    if size != "open":
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
    subprocess.run(["xdotool", "key", "Escape"], check=False)
    watcher.wait()
    if FRAMES and os.path.exists(FRAMES):
        took = [float(line.split()[1]) for line in open(FRAMES) if line.strip()]
        first_frames.append([round(v, 1) for v in took[:3]])
        later_frames.extend(took[3:])
        os.remove(FRAMES)
    watch = [[int(v) for v in line.split()] for line in open(log) if line.strip()]
    pixel = [(t, (r, g, b)) for t, which, r, g, b in watch if which == 0]
    middle = [(t, (r, g, b)) for t, which, r, g, b in watch if which == 1]
    # The watched pixel as the desktop has it, and as the overlay dims it.
    clear = next((colour for t, colour in reversed(pixel) if t < asked), (0, 0, 0))
    dim = [v * 154 // 256 for v in clear]
    if max(clear) < 80:
        sys.exit(f"the watched pixel is too dark to tell dimmed from not: {clear}")
    up = next((t for t, colour in pixel if t >= asked and near(colour, dim)), None)
    if up is None:
        print(f"run {run}: the frozen screen was not seen", file=sys.stderr)
        continue
    frozen.append(round(up - asked))
    # The hint is there when the middle is neither the desktop nor that dimmed.
    before = next((colour for t, colour in reversed(middle) if t < asked), (0, 0, 0))
    plain = [v * 154 // 256 for v in before]
    hinted = next((t for t, colour in middle if t >= asked and not near(colour, plain) and not near(colour, before)), None)
    ready.append(round((max(hinted, up) if hinted else up) - asked))

    def followed(kind, at, limit):
        wanted = clear if kind == "in" else dim
        seen = next((t for t, colour in pixel if t >= at - 1 and near(colour, wanted)), None)
        return None if seen is None or seen - at > limit else max(0, seen - at)

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

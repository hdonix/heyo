#!/usr/bin/env python3
"""Inside the Sway session: how soon the overlay follows a pointer that jumps.

  drag.py [JUMPS]

Heyo is to have been started with HEYO_OVERLAY_TRACE set, its output in
/tmp/heyo.log (`sway.sh drag` does). Asks for a screenshot, presses in the overlay and has the
pointer jump from corner to corner of a large selection, then reads Heyo's
own account of when it heard of each jump, when it had handed the picture
to the compositor, and when the compositor said it had shown it.
"""
import os, re, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from swaybed import height, heyo, keys, pointer, scale, width

jumps = int(sys.argv[1]) if len(sys.argv) > 1 else 12
print(f"an output of {width * int(scale)} by {height * int(scale)} at scale {scale:g}")
heyo("--open", "heyo:screenshot")
time.sleep(2)
pointer("to", width // 8, height // 8, width, height)
pointer("down", "left")
time.sleep(0.3)
marks = []
for jump in range(jumps):
    # Far apart, so that most of the screen changes with each.
    to = (width * 7 // 8, height * 7 // 8) if jump % 2 == 0 else (width // 2, height // 2)
    time.sleep(0.4)
    marks.append(time.time() * 1000)
    pointer("to", to[0], to[1], width, height)
time.sleep(0.5)
pointer("up", "left")
time.sleep(0.3)
keys("-k", "Escape")

events = []
for line in open("/tmp/heyo.log", errors="replace"):
    found = re.match(r"heyo: (pointer moved|layer surface is drawn|layer surface drawn|layer surface shown) at ([0-9.]+) ms", line)
    if found:
        events.append((float(found.group(2)), found.group(1)))
begun, drawn, shown = [], [], []
for mark in marks:
    after = [event for event in events if event[0] >= mark - 1]
    moved = next((at for at, what in after if what == "pointer moved"), None)
    if moved is None:
        continue
    handed = next((at for at, what in after if what == "layer surface drawn" and at >= moved), None)
    seen = next((at for at, what in after if what == "layer surface shown" and handed is not None and at >= handed), None)
    began = next((at for at, what in after if what == "layer surface is drawn" and at >= moved), None)
    if began is not None:
        begun.append(began - moved)
    if handed is not None:
        drawn.append(handed - moved)
    if seen is not None:
        shown.append(seen - moved)
between = [what for at, what in events if marks and marks[0] <= at <= marks[-1] + 400]
print(f"each jump, in ms, until its picture was begun: {' '.join(f'{taken:.1f}' for taken in begun)}")
print(f"... and until it was handed over: {' '.join(f'{taken:.1f}' for taken in drawn)}")
print(f"while it went on ({(marks[-1] + 400 - marks[0]) / 1000:.1f} s): the pointer moved {between.count('pointer moved')} times, {between.count('layer surface drawn')} pictures were handed over")
for name, taken in (("handed to the compositor", drawn), ("shown by the compositor", shown)):
    if taken:
        taken.sort()
        print(f"{name}: median {taken[len(taken) // 2]:.1f} ms, worst {taken[-1]:.1f} ms, of {len(taken)} jumps")
    else:
        print(f"{name}: nothing was measured")

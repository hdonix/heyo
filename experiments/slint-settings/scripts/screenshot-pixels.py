#!/usr/bin/env python3
"""Checks the overlay's pixels against the desktop it froze.

    pixels.py BEFORE.png SELECTED.png X Y W H

BEFORE is the screen before the screenshot was asked for, SELECTED the
screen with the overlay up and X Y W H selected (handles, outline and label
round it). Inside the selection every pixel must be the desktop's; outside
it, away from what the overlay draws round the selection, every pixel must
be the desktop's dimmed to 154 of 256, rounded down.
"""
import sys

from PIL import Image, ImageChops

before = Image.open(sys.argv[1]).convert("RGB")
shown = Image.open(sys.argv[2]).convert("RGB")
x, y, w, h = map(int, sys.argv[3:7])
assert before.size == shown.size, (before.size, shown.size)


def differing(a, b):
    """How many pixels differ, and by how much at most."""
    difference = ImageChops.difference(a, b)
    worst = max(high for _, high in difference.getextrema())
    count = sum(1 for pixel in difference.getdata() if pixel != (0, 0, 0)) if worst else 0
    return count, worst


# Inside, away from the handles on the edge (they reach 5 px in).
inside = (x + 6, y + 6, x + w - 6, y + h - 6)
count, worst = differing(before.crop(inside), shown.crop(inside))
print(f"inside the selection: {count} of {(w - 12) * (h - 12)} pixels differ from the desktop (by {worst} at most)")
failed = count > 0
dimmed = before.point(lambda value: value * 154 // 256)
# Outside: four bands that keep 30 px away from the selection (outline,
# handles, the size label above it).
width, height = before.size
bands = {
    "above": (0, 0, width, max(0, y - 30)),
    "below": (0, min(height, y + h + 30), width, height),
    "left": (0, max(0, y - 30), max(0, x - 30), min(height, y + h + 30)),
    "right": (min(width, x + w + 30), max(0, y - 30), width, min(height, y + h + 30)),
}
for name, box in bands.items():
    if box[2] <= box[0] or box[3] <= box[1]:
        continue
    count, worst = differing(dimmed.crop(box), shown.crop(box))
    print(f"{name}: {count} of {(box[2] - box[0]) * (box[3] - box[1])} pixels differ from the desktop dimmed (by {worst} at most)")
    failed = failed or count > 0
sys.exit(1 if failed else 0)

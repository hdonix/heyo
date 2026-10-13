#!/usr/bin/env python3
"""Checks the overlay's pixels against the desktop it froze.

    screenshot-pixels.py BEFORE.png SELECTED.png X Y W H SCALE BAR_X BAR_Y BAR_W BAR_H

BEFORE is the screen before the screenshot was asked for, SELECTED the
screen with the overlay up and X Y W H selected (handles, outline and label
round it, the toolbar below its right end). Inside the selection every
pixel must be the desktop's; outside it, away from what the overlay draws
round the selection, every pixel must be the desktop's dimmed to 154 of
256, rounded down. Where the toolbar is (BAR…, as the overlay says when
asked), and its shadow falls (40 logical pixels round it, a little of that
on the selection), is not looked at.
"""
import sys

from PIL import Image, ImageChops

before = Image.open(sys.argv[1]).convert("RGB")
shown = Image.open(sys.argv[2]).convert("RGB")
x, y, w, h = map(int, sys.argv[3:7])
scale = int(sys.argv[7])
assert before.size == shown.size, (before.size, shown.size)
left, top, width, height = map(int, sys.argv[8:12])
# How far the toolbar's shadow is drawn round it, in logical pixels.
shadow = 40 * scale
# Its shadow is cast 8 down.
toolbar = (left - shadow, top - shadow, left + width + shadow, top + height + shadow + 8 * scale)
for picture in (before, shown):
    picture.paste((0, 0, 0), toolbar)


def differing(a, b):
    """How many pixels differ, and by how much at most."""
    difference = ImageChops.difference(a, b)
    worst = max(high for _, high in difference.getextrema())
    count = sum(1 for pixel in difference.getdata() if pixel != (0, 0, 0)) if worst else 0
    return count, worst


# Inside, away from the handles on the edge (they reach 5 logical pixels in).
edge = 6 * scale
inside = (x + edge, y + edge, x + w - edge, y + h - edge)
count, worst = differing(before.crop(inside), shown.crop(inside))
print(f"inside the selection: {count} of {(w - 2 * edge) * (h - 2 * edge)} pixels differ from the desktop (by {worst} at most)")
failed = count > 0
dimmed = before.point(lambda value: value * 154 // 256)
# Outside: four bands that keep 30 logical pixels away from the selection
# (outline, handles, the size label above it).
away = 30 * scale
width, height = before.size
bands = {
    "above": (0, 0, width, max(0, y - away)),
    "below": (0, min(height, y + h + away), width, height),
    "left": (0, max(0, y - away), max(0, x - away), min(height, y + h + away)),
    "right": (min(width, x + w + away), max(0, y - away), width, min(height, y + h + away)),
}
for name, box in bands.items():
    if box[2] <= box[0] or box[3] <= box[1]:
        continue
    count, worst = differing(dimmed.crop(box), shown.crop(box))
    print(f"{name}: {count} of {(box[2] - box[0]) * (box[3] - box[1])} pixels differ from the desktop dimmed (by {worst} at most)")
    failed = failed or count > 0
sys.exit(1 if failed else 0)

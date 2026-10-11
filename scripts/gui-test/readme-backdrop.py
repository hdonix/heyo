#!/usr/bin/env python3
"""readme-backdrop.py light|dark WIDTH HEIGHT FILE: what is behind Heyo in
the pictures of the README (readme-shots.sh), as the sandbox session's
wallpaper.

A soft gradient in the blues of Heyo's icon (src-tauri/icons/source/), with
two glows whose centres are placed from the middle of the screen: that is
where the palette and the settings window open, and the pictures are cut
around them. It is quiet on purpose, the window is the subject.

Drawn small and enlarged, so every edge in it is soft and the file stays
small however large the screen. Needs Pillow only.
"""
import sys

from PIL import Image, ImageDraw, ImageFilter

# The corners of the gradient (top left, bottom right), then the glows:
# colour, opacity, centre and radius, the last two in units of the picture's
# height measured from its middle.
LOOKS = {
    "light": {
        "ends": ((198, 222, 255), (122, 178, 255)),
        "glows": [
            ((255, 255, 255), 0.55, (-0.40, -0.28), 0.32),
            ((58, 155, 255), 0.50, (0.44, 0.28), 0.40),
            ((31, 134, 255), 0.25, (-0.34, 0.40), 0.30),
        ],
    },
    "dark": {
        "ends": ((18, 34, 70), (24, 62, 128)),
        "glows": [
            ((58, 155, 255), 0.34, (0.44, 0.28), 0.40),
            ((31, 134, 255), 0.28, (-0.40, -0.28), 0.32),
            ((8, 14, 30), 0.50, (-0.34, 0.40), 0.30),
        ],
    },
}
# The picture is drawn at 1/SMALL of its size.
SMALL = 8


def backdrop(look: str, width: int, height: int) -> Image.Image:
    spec = LOOKS[look]
    w, h = width // SMALL, height // SMALL
    start, end = spec["ends"]
    # The gradient runs along the diagonal: a pixel's place on it is how far
    # it is from the top left corner, by both axes together.
    ramp = Image.new("RGB", (w, h))
    ramp.putdata([
        tuple(round(a + (b - a) * (x / (w - 1) + y / (h - 1)) / 2) for a, b in zip(start, end))
        for y in range(h) for x in range(w)
    ])
    for colour, opacity, (cx, cy), radius in spec["glows"]:
        mask = Image.new("L", (w, h), 0)
        x, y, r = w / 2 + cx * h, h / 2 + cy * h, radius * h
        ImageDraw.Draw(mask).ellipse((x - r, y - r, x + r, y + r), fill=round(255 * opacity))
        mask = mask.filter(ImageFilter.GaussianBlur(r * 0.55))
        ramp.paste(Image.new("RGB", (w, h), colour), (0, 0), mask)
    return ramp.resize((width, height), Image.Resampling.BICUBIC)


if __name__ == "__main__":
    look, width, height, path = sys.argv[1], int(sys.argv[2]), int(sys.argv[3]), sys.argv[4]
    backdrop(look, width, height).save(path)

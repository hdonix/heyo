#!/usr/bin/env python3
"""Draws the shadow of Heyo's windows as pictures, for src/styles to lay
around a window as a border image.

A CSS box-shadow with a blur is worked out again whenever anything of its
element is redrawn. Where the web view draws without a graphics card (as it
does with NVIDIA's driver) that took most of a frame, at every step of the
selection. A picture is only copied.

The pictures hold the two blurred shadows of `--shadow-window`
(src/styles/base/_tokens.scss); the hairline stays a box-shadow, which has
no blur and must stay sharp. Change the shadow there, change it here, and
run this again:

    python3 scripts/window-shadow.py
"""
from pathlib import Path
from PIL import Image, ImageDraw, ImageFilter

MARGIN = 32  # $window-margin
# (offset down, blur) of the near and the far shadow, and their opacity by scheme.
LAYERS = [(2, 4), (8, 24)]
OPACITY = {"light": [0.10, 0.26], "dark": [0.30, 0.55]}
RADII = {"2xl": 22, "xl": 16}
# Drawn at twice the size, for displays that show two pixels to one.
SCALE = 2


def shadow(radius: int, opacities: list[float]) -> Image.Image:
    # Corners of margin + radius, and a little between them to stretch.
    slice_ = MARGIN + radius
    size = (2 * slice_ + 4) * SCALE
    alpha = [0.0] * (size * size)
    for (down, blur), opacity in zip(LAYERS, opacities):
        layer = Image.new("L", (size, size), 0)
        box = [MARGIN * SCALE, (MARGIN + down) * SCALE, size - MARGIN * SCALE - 1, size - (MARGIN - down) * SCALE - 1]
        ImageDraw.Draw(layer).rounded_rectangle(box, radius * SCALE, fill=255)
        # A CSS blur of b is a Gaussian of deviation b / 2.
        layer = layer.filter(ImageFilter.GaussianBlur(blur * SCALE / 2))
        # One shadow over another: what the upper lets through of the lower.
        alpha = [cover + under * (1 - cover) for under, cover in zip(alpha, (v / 255 * opacity for v in layer.getdata()))]
    mask = Image.new("L", (size, size))
    mask.putdata([round(v * 255) for v in alpha])
    image = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    image.putalpha(mask)
    return image


root = Path(__file__).resolve().parent.parent
# The web view's, and the same pictures for the Flutter interface (which
# lays them around its windows the same way: ui/lib/widgets/panel.dart).
places = [root / "src/assets/shadows", root / "ui/assets/shadows"]
for name, radius in RADII.items():
    for scheme, opacities in OPACITY.items():
        image = shadow(radius, opacities)
        for out in places:
            out.mkdir(parents=True, exist_ok=True)
            file = out / f"window-{name}-{scheme}.png"
            image.save(file, optimize=True)
        print(file.name, file.stat().st_size, "bytes")

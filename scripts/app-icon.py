#!/usr/bin/env python3
"""Makes Heyo's icons from the one picture of it.

The picture (src-tauri/icons/source/artwork.webp) is the mark, white glass,
on its blue. Two things are made of it, beside it:

  glyph.png   the mark alone: white, with the transparency that gives the
              picture back when it is laid over the picture's blue. For
              wherever the mark is wanted on another colour.
  icon.png    the application's icon, 1024 pixels: the picture itself, cut
              square round the mark, in a tile with continuous corners.

and from icon.png, with `tauri icon`, every size and format the packages
and the installers take (src-tauri/icons/). Change the picture, and run
this again (it needs numpy and Pillow):

    python3 scripts/app-icon.py
"""
import shutil
import subprocess
import tempfile
from pathlib import Path

import numpy as np
from PIL import Image, ImageFilter

ICONS = Path(__file__).resolve().parent.parent / "src-tauri/icons"
SOURCE = ICONS / "source"

SIZE = 1024
# The tile leaves a margin: an icon that fills its square looks larger than
# its neighbours in a dock.
TILE = 880
# The corners are a superellipse's, which has no point where a straight
# edge turns into an arc.
CORNERS = 5.0
# Alpha below this is the grain of the picture's background, not glass.
GRAIN = 0.03


def background(picture: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
    """The blue the mark stands on, as it would be with the mark away, and
    where the mark is. The blue changes slowly across the picture: a cubic
    surface is fitted to what is not the mark."""
    height, width, _ = picture.shape
    ys, xs = np.mgrid[0:height, 0:width]
    x, y = (xs - width / 2) / width, (ys - height / 2) / height
    terms = np.stack([x**i * y**j for i in range(4) for j in range(4 - i)], axis=-1)
    # To begin with, only the picture's edge is surely background.
    mark = (np.abs(x) < 0.38) & (np.abs(y) < 0.38)
    for _ in range(3):
        fitted = np.empty_like(picture)
        for channel in range(3):
            weights, *_ = np.linalg.lstsq(terms[~mark], picture[..., channel][~mark], rcond=None)
            fitted[..., channel] = terms @ weights
        lighter = picture[..., 0] - fitted[..., 0] > 5
        # With room round it for the glow at its edges.
        mark = np.asarray(Image.fromarray((lighter * 255).astype(np.uint8)).filter(ImageFilter.MaxFilter(41))) > 0
    return fitted, mark


def glyph(picture: np.ndarray, blue: np.ndarray, mark: np.ndarray) -> Image.Image:
    """The mark as white over nothing. How much of the white shows is read
    from red and green; the background is as blue as white is, so the blue
    channel says nothing."""
    towards_white = 255 - blue[..., :2]
    weight = towards_white**2
    alpha = (((picture[..., :2] - blue[..., :2]) / towards_white) * weight).sum(axis=-1) / weight.sum(axis=-1)
    alpha = np.clip((np.clip(alpha, 0, 1) * mark - GRAIN) / (1 - GRAIN), 0, 1)
    white = np.full(picture.shape[:2], 255.0)
    whole = Image.fromarray(np.dstack([white, white, white, alpha * 255]).round().astype(np.uint8), "RGBA")
    # Square, with the mark in the middle and a margin a quarter of its height.
    left, top, right, bottom = whole.getbbox()
    side = round((bottom - top) * 1.5)
    square = Image.new("RGBA", (side, side), (255, 255, 255, 0))
    square.paste(whole.crop((left, top, right, bottom)), ((side - (right - left)) // 2, (side - (bottom - top)) // 2))
    return square


def tile(size: int) -> Image.Image:
    """The tile's shape, drawn four times as large and brought down for its edge."""
    side = size * 4
    at = (np.arange(side) + 0.5) / side * 2 - 1
    inside = np.abs(at[None, :]) ** CORNERS + np.abs(at[:, None]) ** CORNERS <= 1
    return Image.fromarray((inside * 255).astype(np.uint8)).resize((size, size), Image.LANCZOS)


def icon(artwork: Image.Image, mark: np.ndarray) -> Image.Image:
    """The picture cut square round the mark, as large a square as it has."""
    rows, columns = np.nonzero(mark)
    side = min(artwork.size)
    left = round((columns.min() + columns.max()) / 2 - side / 2)
    top = round((rows.min() + rows.max()) / 2 - side / 2)
    left = min(max(left, 0), artwork.width - side)
    top = min(max(top, 0), artwork.height - side)
    cut = artwork.crop((left, top, left + side, top + side)).resize((TILE, TILE), Image.LANCZOS).convert("RGBA")
    cut.putalpha(tile(TILE))
    whole = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    whole.paste(cut, ((SIZE - TILE) // 2, (SIZE - TILE) // 2))
    return whole


def main():
    artwork = Image.open(SOURCE / "artwork.webp").convert("RGB")
    picture = np.asarray(artwork).astype(np.float64)
    blue, mark = background(picture)
    glyph(picture, blue, mark).save(SOURCE / "glyph.png", optimize=True)
    icon(artwork, mark).save(SOURCE / "icon.png", optimize=True)

    # Tauri makes more than is used (the phones' icons): only what is
    # already here is taken.
    with tempfile.TemporaryDirectory() as made:
        subprocess.run(["pnpm", "tauri", "icon", str(SOURCE / "icon.png"), "--output", made], check=True, cwd=ICONS.parent.parent, capture_output=True)
        for existing in sorted(path for path in ICONS.iterdir() if path.is_file()):
            shutil.copyfile(Path(made) / existing.name, existing)
            print(existing.relative_to(ICONS.parent.parent))


if __name__ == "__main__":
    main()

"""How large the screenshot's toolbar is drawn.

    toolbar.py PNG X Y

X, Y is a place inside it in the picture of the screen, in its padding. It
is measured as far as its own colour goes from there, across and down:
prints the two, in the picture's pixels. Across is the toolbar's width but
for its round corners; down is as far as the next button.
"""
import sys

from PIL import Image

picture = Image.open(sys.argv[1]).convert("RGB")
x, y = int(sys.argv[2]), int(sys.argv[3])
own = picture.getpixel((x, y))


def alike(at):
    return all(abs(a - b) <= 10 for a, b in zip(picture.getpixel(at), own))


left = right = x
while left > 0 and alike((left - 1, y)):
    left -= 1
while right < picture.width - 1 and alike((right + 1, y)):
    right += 1
top = bottom = y
while top > 0 and alike((x, top - 1)):
    top -= 1
while bottom < picture.height - 1 and alike((x, bottom + 1)):
    bottom += 1
print(right - left + 1, bottom - top + 1)

"""shot.py DISPLAY FILE [X Y W H]: a picture of a display, or of a part of it."""
import sys

from PIL import ImageGrab

image = ImageGrab.grab(xdisplay=sys.argv[1])
if len(sys.argv) > 3:
    x, y, w, h = map(int, sys.argv[3:7])
    image = image.crop((x, y, x + w, y + h))
image.save(sys.argv[2])

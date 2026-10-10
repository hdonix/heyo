#!/usr/bin/env python3
"""grab.py FILE [LEFT TOP RIGHT BOTTOM]: a picture of the sandbox's screen."""
import os, sys
from PIL import ImageGrab
image = ImageGrab.grab(xdisplay=os.environ.get("HEYO_TEST_DISPLAY", ":99"))
if len(sys.argv) > 2:
    image = image.crop(tuple(map(int, sys.argv[2:6])))
image.save(sys.argv[1])

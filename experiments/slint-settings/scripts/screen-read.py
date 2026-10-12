#!/usr/bin/python3
"""Reads things off the sandbox's display that a picture of it does not show.

    screen-read.py clipboard OUT.png   the picture on the clipboard, saved; fails if there is none
    screen-read.py cursor              the name of the pointer's shape as it is now, and its picture's size
    screen-read.py same A.png B.png [X Y W H]
                                       whether B is pixel for pixel A (or that part of A): says
                                       how many pixels differ and by how much at most

The system's Python: it has the bindings to GTK.
"""
import ctypes
import os
import sys

if os.environ.get("DISPLAY") != os.environ.get("SLINT_SETTINGS_DISPLAY") or os.environ.get("DISPLAY") in (None, "", ":0"):
    sys.exit("not the sandbox's display")
what = sys.argv[1]
if what == "clipboard":
    import gi

    gi.require_version("Gdk", "3.0")
    gi.require_version("Gtk", "3.0")
    from gi.repository import Gdk, Gtk

    picture = Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD).wait_for_image()
    if picture is None:
        sys.exit("no picture on the clipboard")
    picture.savev(sys.argv[2], "png", [], [])
    print(f"{picture.get_width()} x {picture.get_height()}")
elif what == "cursor":

    class CursorImage(ctypes.Structure):
        _fields_ = [
            ("x", ctypes.c_short),
            ("y", ctypes.c_short),
            ("width", ctypes.c_ushort),
            ("height", ctypes.c_ushort),
            ("xhot", ctypes.c_ushort),
            ("yhot", ctypes.c_ushort),
            ("serial", ctypes.c_ulong),
            ("pixels", ctypes.POINTER(ctypes.c_ulong)),
            ("atom", ctypes.c_ulong),
            ("name", ctypes.c_char_p),
        ]

    x11 = ctypes.CDLL("libX11.so.6")
    xfixes = ctypes.CDLL("libXfixes.so.3")
    x11.XOpenDisplay.restype = ctypes.c_void_p
    display = ctypes.c_void_p(x11.XOpenDisplay(os.environ["DISPLAY"].encode()))
    xfixes.XFixesGetCursorImage.restype = ctypes.POINTER(CursorImage)
    xfixes.XFixesGetCursorImage.argtypes = [ctypes.c_void_p]
    image = xfixes.XFixesGetCursorImage(display)
    name = image.contents.name.decode() if image and image.contents.name else ""
    # Its name, and the size of its picture: a pointer of 24 is a picture of 24 or a little more.
    print(f"{name or '(unnamed)'} {image.contents.width}x{image.contents.height}")
elif what == "same":
    from PIL import Image, ImageChops

    whole, part = Image.open(sys.argv[2]).convert("RGB"), Image.open(sys.argv[3]).convert("RGB")
    if len(sys.argv) > 4:
        x, y, w, h = map(int, sys.argv[4:8])
        whole = whole.crop((x, y, x + w, y + h))
    if whole.size != part.size:
        print(f"FAIL: {part.size[0]} x {part.size[1]}, not {whole.size[0]} x {whole.size[1]}")
        sys.exit(1)
    difference = ImageChops.difference(whole, part)
    worst = max(high for _, high in difference.getextrema())
    count = sum(1 for pixel in difference.getdata() if pixel != (0, 0, 0)) if worst else 0
    print(f"{count} of {part.size[0] * part.size[1]} pixels differ (by {worst} at most)")
    sys.exit(1 if count else 0)
else:
    sys.exit(__doc__)

#!/usr/bin/env python3
"""Inside the test session: an application window to test against.

  window.py TITLE APP_ID        a window with a text box, which has the focus

Its text can be read back from /tmp/window-APP_ID.txt, rewritten on every change
(put in place whole, so that a reader never finds half of it).
"""
import os
import sys
import gi
gi.require_version("Gtk", "3.0")
from gi.repository import Gtk, GLib

title, app_id = sys.argv[1], sys.argv[2]
GLib.set_prgname(app_id)  # the Wayland app id, which GNOME reports as the class
window = Gtk.Window(title=title)
window.set_default_size(700, 450)
view = Gtk.TextView()
buffer = view.get_buffer()
if len(sys.argv) > 3:
    buffer.set_text(sys.argv[3])

def save(*_):
    start, end = buffer.get_bounds()
    path = f"/tmp/window-{app_id}.txt"
    with open(path + ".new", "w") as f:
        f.write(buffer.get_text(start, end, True))
    os.replace(path + ".new", path)

buffer.connect("changed", save)
save()
window.add(view)
window.connect("destroy", Gtk.main_quit)
window.show_all()
Gtk.main()

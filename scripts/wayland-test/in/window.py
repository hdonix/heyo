#!/usr/bin/env python3
"""Inside the test session: an application window to test against.

  window.py TITLE APP_ID [TEXT]   a window with a text box, which has the focus

Its text can be read back from /tmp/window-APP_ID.txt, rewritten on every change
(put in place whole, so that a reader never finds half of it). What an input
method is composing in it, which is shown there and is not its text yet, is in
/tmp/window-APP_ID.composing.txt.

SIGUSR1 (its process is in /tmp/window-APP_ID.pid.txt) makes its text TEXT
again, with the cursor after it: typing it would go through whatever input
method the session has.

WINDOW_DIR in the environment names another directory than /tmp for those
files: two sessions on one machine then do not read each other's window.

With WINDOW_INPUT_METHOD=none in the environment the text box takes no input
method (only the toolkit's own handling of dead keys and Compose), as in a
session whose applications are not set up for one. With WINDOW_FULLSCREEN
set it fills the screen, as a film or a presentation does.
"""
import os
import signal
import sys
import gi
gi.require_version("Gtk", "3.0")
from gi.repository import Gtk, GLib

title, app_id = sys.argv[1], sys.argv[2]
GLib.set_prgname(app_id)  # the Wayland app id, which GNOME reports as the class
window = Gtk.Window(title=title)
window.set_default_size(700, 450)
if os.environ.get("WINDOW_FULLSCREEN"):
    window.fullscreen()
view = Gtk.TextView()
view.set_wrap_mode(Gtk.WrapMode.WORD_CHAR)
if os.environ.get("WINDOW_INPUT_METHOD") == "none":
    view.set_property("im-module", "gtk-im-context-simple")
buffer = view.get_buffer()

def afresh(*_):
    buffer.set_text(sys.argv[3] if len(sys.argv) > 3 else "")
    buffer.place_cursor(buffer.get_end_iter())
    return True

def put(suffix, text):
    path = f"{os.environ.get('WINDOW_DIR', '/tmp')}/window-{app_id}{suffix}.txt"
    with open(path + ".new", "w") as f:
        f.write(text)
    os.replace(path + ".new", path)

def save(*_):
    start, end = buffer.get_bounds()
    put("", buffer.get_text(start, end, True))

buffer.connect("changed", save)
view.connect("preedit-changed", lambda _view, composing: put(".composing", composing))
afresh()
save()
put(".composing", "")
put(".pid", str(os.getpid()))
GLib.unix_signal_add(GLib.PRIORITY_DEFAULT, signal.SIGUSR1, afresh)
window.add(view)
window.connect("destroy", Gtk.main_quit)
window.show_all()
Gtk.main()

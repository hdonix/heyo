#!/usr/bin/python3
"""demo-caption.py FILE X Y: a line of words on the screen for a film.

Shows what FILE holds, in a dark bar whose middle is at X, Y of the
screen, and follows the file as it changes; an empty file hides the bar.
The bar is not a window to the desktop (nothing places it, lists it or
gives it the keyboard), so what is filmed behaves as it would without it.
Only for demo-video.sh, in the sandbox.
"""
import sys

import gi

gi.require_version("Gtk", "3.0")
from gi.repository import GLib, Gtk  # noqa: E402

path, x, y = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
window = Gtk.Window(type=Gtk.WindowType.POPUP)
window.set_app_paintable(True)
visual = window.get_screen().get_rgba_visual()
if visual:
    window.set_visual(visual)
label = Gtk.Label()
window.add(label)
style = Gtk.CssProvider()
style.load_from_data(b"""
window { background: transparent; }
label { background: rgba(12, 12, 14, 0.9); color: white; border-radius: 12px;
        padding: 9px 20px; font: 600 15px "Inter", "Noto Sans CJK SC", "Cantarell", sans-serif; }
""")
Gtk.StyleContext.add_provider_for_screen(window.get_screen(), style, Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION)
shown = None


def follow():
    global shown
    try:
        words = open(path, encoding="utf-8").read().strip()
    except OSError:
        words = ""
    if words != shown:
        shown = words
        if words:
            label.set_text(words)
            window.show_all()
            window.resize(1, 1)
            GLib.idle_add(place)
        else:
            window.hide()
    return True


def place():
    scale = window.get_scale_factor()
    width, height = window.get_size()
    window.move(x // scale - width // 2, y // scale - height // 2)
    return False


GLib.timeout_add(100, follow)
Gtk.main()

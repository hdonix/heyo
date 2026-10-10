#!/usr/bin/env python3
"""Inside the test session: a window filled with one colour, to find again in
a capture of the screen.

  colour.py TITLE APP_ID RRGGBB [WIDTH HEIGHT]
"""
import sys
import gi
gi.require_version("Gtk", "3.0")
gi.require_version("Gdk", "3.0")
from gi.repository import Gdk, Gtk, GLib

title, app_id, colour = sys.argv[1:4]
width, height = (int(v) for v in sys.argv[4:6]) if len(sys.argv) > 5 else (400, 300)
GLib.set_prgname(app_id)  # the Wayland app id, which GNOME reports as the class
style = Gtk.CssProvider()
style.load_from_data(f"window {{ background: #{colour}; }}".encode())
Gtk.StyleContext.add_provider_for_screen(Gdk.Screen.get_default(), style, Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION)
window = Gtk.Window(title=title)
# No title bar: the whole frame is the colour.
window.set_decorated(False)
window.set_default_size(width, height)
window.connect("destroy", Gtk.main_quit)
window.show_all()
Gtk.main()

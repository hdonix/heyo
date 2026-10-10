#!/usr/bin/python3
"""copy.py TEXT: puts TEXT on the sandbox's clipboard, as a copy in another
application would, and stays its owner for a moment so that Heyo's watcher
can read it (it keeps the text itself from then on). xclip and xsel are not
to be relied on here; GTK is."""
import sys

import gi

gi.require_version("Gtk", "3.0")
gi.require_version("Gdk", "3.0")
from gi.repository import Gdk, GLib, Gtk  # noqa: E402

clipboard = Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD)
clipboard.set_text(sys.argv[1], -1)
GLib.timeout_add(1500, Gtk.main_quit)
Gtk.main()

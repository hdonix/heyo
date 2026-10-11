#!/usr/bin/python3
"""clip.py TEXT: puts TEXT on the sandbox's clipboard, as a copy in another
application would, and stays its owner for a moment so that Heyo's watcher
can read it (it keeps the text itself from then on). xclip and xsel are not
to be relied on here; GTK is.

clip.py (nothing): prints what is on the clipboard, as {"text", "since"}.
"since" is when its owner took it (the X server's time): a clipboard that
was borrowed and given back has the same text and another "since"."""
import json
import sys

import gi

gi.require_version("Gtk", "3.0")
gi.require_version("Gdk", "3.0")
from gi.repository import Gdk, GLib, Gtk  # noqa: E402

clipboard = Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD)
if len(sys.argv) > 1:
    clipboard.set_text(sys.argv[1], -1)
    GLib.timeout_add(1500, Gtk.main_quit)
    Gtk.main()
else:
    taken = clipboard.wait_for_contents(Gdk.Atom.intern("TIMESTAMP", False))
    since = int.from_bytes(taken.get_data()[:4], sys.byteorder) if taken and taken.get_data() else None
    print(json.dumps({"text": clipboard.wait_for_text(), "since": since}, ensure_ascii=False))

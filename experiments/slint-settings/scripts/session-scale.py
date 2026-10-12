#!/usr/bin/python3
"""Sets the scale of the sandbox's GNOME session, as its Settings do.

    session-scale.py N     (1 or 2)

The Shell is asked to lay its one monitor out at that scale
(org.gnome.Mutter.DisplayConfig, ApplyMonitorsConfig): it then says so to
the settings daemon, which publishes it (XSETTINGS, Xft.dpi, Xcursor.size)
to every program on the display. This is what happens to a running program
when a monitor of another density is connected.

The system's Python: it has the bindings to GLib. Only ever on the
sandbox's own bus and display.
"""
import os
import sys

if os.environ.get("DISPLAY") != os.environ.get("SLINT_SETTINGS_DISPLAY") or os.environ.get("DISPLAY") in (None, "", ":0"):
    sys.exit("not the sandbox's display")
if os.environ.get("SLINT_SETTINGS_DIR", "") + "/" not in os.environ.get("DBUS_SESSION_BUS_ADDRESS", ""):
    sys.exit("not the sandbox's bus")
from gi.repository import Gio, GLib

scale = float(sys.argv[1])
bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
shell = ("org.gnome.Mutter.DisplayConfig", "/org/gnome/Mutter/DisplayConfig", "org.gnome.Mutter.DisplayConfig")
serial, monitors, logical, _ = bus.call_sync(*shell, "GetCurrentState", None, None, Gio.DBusCallFlags.NONE, 5000, None).unpack()
(connector, *_), modes, _ = monitors[0]
current = next((mode for mode in modes if mode[6].get("is-current")), modes[0])
layout = [(0, 0, scale, 0, True, [(connector, current[0], {})])]
# 1: for now, not kept in a file.
bus.call_sync(*shell, "ApplyMonitorsConfig", GLib.Variant("(uua(iiduba(ssa{sv}))a{sv})", (serial, 1, layout, {})), None, Gio.DBusCallFlags.NONE, 5000, None)

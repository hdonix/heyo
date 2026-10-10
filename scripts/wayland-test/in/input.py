#!/usr/bin/env python3
"""Inside the test session: presses keys and moves the pointer, the way a
person would, through Mutter's RemoteDesktop interface.

  input.py type "some text"        type characters
  input.py key Return ctrl+k ...   press keys or combinations (X keysym names)
  input.py seq STEP...             down:KEY up:KEY tap:COMBO wait:SECONDS nudge:PIXELS press:left
                                   (one after another, e.g. to press a key while another is held;
                                   press clicks where the pointer is)
  input.py click X Y               move the pointer there and click
  input.py drag X1 Y1 X2 Y2        press at the first point, release at the second
  input.py hold KEY SECONDS        hold one key down for a while
"""
import sys, time
import gi
gi.require_version("Gdk", "3.0")
from gi.repository import Gio, GLib, Gdk

bus = Gio.bus_get_sync(Gio.BusType.SESSION)

def call(path, iface, method, args=None, reply=None):
    return bus.call_sync("org.gnome.Mutter.RemoteDesktop", path, iface, method, args, reply, Gio.DBusCallFlags.NONE, -1, None)

session = call("/org/gnome/Mutter/RemoteDesktop", "org.gnome.Mutter.RemoteDesktop", "CreateSession", None, GLib.VariantType("(o)")).unpack()[0]
S = "org.gnome.Mutter.RemoteDesktop.Session"
call(session, S, "Start")
# The first event of a session is lost unless the session has settled, and
# even then the very first key sometimes is: wait, then spend one on a key
# that does nothing.
time.sleep(0.5)
call(session, S, "NotifyKeyboardKeysym", GLib.Variant("(ub)", (0xffe2, True)))   # Shift_R
call(session, S, "NotifyKeyboardKeysym", GLib.Variant("(ub)", (0xffe2, False)))
time.sleep(0.15)

MODIFIERS = {"ctrl": "Control_L", "shift": "Shift_L", "alt": "Alt_L", "super": "Super_L"}

def keysym(name):
    value = Gdk.keyval_from_name(MODIFIERS.get(name.lower(), name))
    if value in (0, 0xffffff):
        sys.exit(f"unknown key: {name}")
    return value

def press(sym, down):
    call(session, S, "NotifyKeyboardKeysym", GLib.Variant("(ub)", (sym, down)))
    time.sleep(0.02)

def tap(combo):
    keys = [keysym(k) for k in combo.split("+")]
    for k in keys: press(k, True)
    for k in reversed(keys): press(k, False)

def move(x, y):
    # Absolute motion needs a screen-cast stream; relative motion from a
    # corner the pointer is first pushed into does not.
    call(session, S, "NotifyPointerMotionRelative", GLib.Variant("(dd)", (-10000.0, -10000.0)))
    time.sleep(0.05)
    call(session, S, "NotifyPointerMotionRelative", GLib.Variant("(dd)", (float(x), float(y))))
    time.sleep(0.05)

def button(down):
    call(session, S, "NotifyPointerButton", GLib.Variant("(ib)", (0x110, down)))  # BTN_LEFT
    time.sleep(0.05)

command, args = sys.argv[1], sys.argv[2:]
if command == "type":
    for ch in args[0]:
        sym = Gdk.unicode_to_keyval(ord(ch))
        press(sym, True); press(sym, False)
elif command == "key":
    for combo in args:
        tap(combo); time.sleep(0.08)
elif command == "click":
    move(*args[:2]); button(True); button(False)
elif command == "drag":
    x1, y1, x2, y2 = map(float, args)
    move(x1, y1); button(True)
    for i in range(1, 9):
        call(session, S, "NotifyPointerMotionRelative", GLib.Variant("(dd)", ((x2 - x1) / 8, (y2 - y1) / 8))); time.sleep(0.03)
    button(False)
elif command == "hold":
    sym = keysym(args[0]); press(sym, True); time.sleep(float(args[1])); press(sym, False)
elif command == "seq":
    for step in args:
        what, _, value = step.partition(":")
        if what == "down": press(keysym(value), True)
        elif what == "up": press(keysym(value), False)
        elif what == "tap": tap(value)
        elif what == "wait": time.sleep(float(value))
        elif what == "press": button(True); button(False)
        elif what == "nudge": call(session, S, "NotifyPointerMotionRelative", GLib.Variant("(dd)", (float(value), 0.0)))
        else: sys.exit(__doc__)
else:
    sys.exit(__doc__)
time.sleep(0.1)
call(session, S, "Stop")

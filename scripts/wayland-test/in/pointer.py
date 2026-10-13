#!/usr/bin/env python3
"""Inside the Sway session: a pointer for a session that has none.

Headless Sway has a seat without a pointer, and tells its clients so: they
are sent nothing of one, whatever `swaymsg seat - cursor` does. This makes
a virtual pointer (the protocol wlr-virtual-pointer) and keeps it for as
long as it runs, taking what to do with it from its input, a line each:

  to X Y WIDTH HEIGHT    the pointer to that place of an output of that size (logical pixels)
  down BUTTON            left or right
  up BUTTON
  wait SECONDS

It says "ok" on its output after each, when the compositor has it.
"""
import sys, time

sys.path.insert(0, "/opt")
from pywayland.client import Display
from heyo_protocols.wlr_virtual_pointer_unstable_v1 import ZwlrVirtualPointerManagerV1

BUTTONS = {"left": 0x110, "right": 0x111}

display = Display()
display.connect()
registry = display.get_registry()
found = {}


def announced(registry, name, interface, version):
    if interface == "zwlr_virtual_pointer_manager_v1":
        found["manager"] = registry.bind(name, ZwlrVirtualPointerManagerV1, 1)


registry.dispatcher["global"] = announced
display.roundtrip()
if "manager" not in found:
    sys.exit("the compositor has no virtual pointers")
pointer = found["manager"].create_virtual_pointer(None)
display.roundtrip()
print("ok", flush=True)

for line in sys.stdin:
    words = line.split()
    if not words:
        continue
    now = int(time.monotonic() * 1000) & 0xFFFFFFFF
    if words[0] == "to":
        x, y, width, height = (int(float(word)) for word in words[1:5])
        pointer.motion_absolute(now, x, y, width, height)
        pointer.frame()
    elif words[0] in ("down", "up"):
        pointer.button(now, BUTTONS[words[1]], 1 if words[0] == "down" else 0)
        pointer.frame()
    elif words[0] == "wait":
        time.sleep(float(words[1]))
    display.roundtrip()
    print("ok", flush=True)

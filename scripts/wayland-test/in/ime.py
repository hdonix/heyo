#!/usr/bin/env python3
"""Inside the Sway session: an input method, to try what types through one.

Whoever types Chinese types through a program of the desktop's that shows
what is being composed and hands over the text that is chosen; under Sway
it speaks to the compositor by the protocol input-method (version 2), and
the compositor to the window that takes the text. This is such a program
with no keyboard of its own: it composes and chooses what it is told to,
a line each on its input:

  active           says "yes" if a window has asked it for text, else "no"
  compose TEXT     shows TEXT as what is being composed
  choose TEXT      hands TEXT over as what was chosen

It says "ok" after the other two, when the compositor has them.
"""
import sys

sys.path.insert(0, "/opt")
from pywayland.client import Display
from pywayland.protocol.wayland import WlSeat
from heyo_protocols.input_method_unstable_v2 import ZwpInputMethodManagerV2

display = Display()
display.connect()
registry = display.get_registry()
found = {}


def announced(registry, name, interface, version):
    if interface == "zwp_input_method_manager_v2":
        found["manager"] = registry.bind(name, ZwpInputMethodManagerV2, 1)
    elif interface == "wl_seat" and "seat" not in found:
        found["seat"] = registry.bind(name, WlSeat, 1)


registry.dispatcher["global"] = announced
display.roundtrip()
if "manager" not in found or "seat" not in found:
    sys.exit("the compositor takes no input method")
method = found["manager"].get_input_method(found["seat"])
# What the compositor said since it last said it was done, and how many
# times it has said that: the number goes back with whatever is handed over.
state = {"asked": False, "active": False, "done": 0}


def activate(_method):
    state["asked"] = True


def deactivate(_method):
    state["asked"] = False


def done(_method):
    state["done"] += 1
    state["active"] = state["asked"]


method.dispatcher["activate"] = activate
method.dispatcher["deactivate"] = deactivate
method.dispatcher["done"] = done
display.roundtrip()
print("ok", flush=True)

for line in sys.stdin:
    what, _, text = line.rstrip("\n").partition(" ")
    display.roundtrip()
    if what == "active":
        print("yes" if state["active"] else "no", flush=True)
        continue
    if what == "compose":
        method.set_preedit_string(text, 0, len(text.encode()))
    elif what == "choose":
        method.commit_string(text)
    method.commit(state["done"])
    display.roundtrip()
    print("ok", flush=True)

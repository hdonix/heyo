#!/usr/bin/python3
"""Asks one of Heyo's Slint windows where a thing is, instead of a script
carrying the window's layout as numbers.

    ask.py tree [WINDOW]                 every window of Heyo's on the accessibility bus (or the one
                                         of that title): role, name, rectangle on the screen, states
    ask.py where ROLE NAME [WINDOW]      X Y W H of that control, in the screen's pixels
    ask.py click ROLE NAME [WINDOW]      a click in its middle, with the sandbox's xdotool
    ask.py states ROLE NAME [WINDOW]     its states (checked, selected, focused…), one line
    ask.py wait ROLE NAME [WINDOW]       waits for it (10 s), then as `where`

ROLE is as the accessibility bus names it ("push button", "list item",
"check box"…; `tree` shows them) and NAME is the control's label, exactly.
`-N` behind NAME takes the Nth of several (from 1).

Slint's windows are on the accessibility bus (AT-SPI) of the session
through AccessKit, which stays quiet until somebody asks: the first use
here says to the session's bus that an assistive tool is there
(org.a11y.Status, IsEnabled), and waits for Heyo to appear. A window that
was never asked is as it is on a desktop without one.

The system's Python: it has the bindings (gir1.2-atspi-2.0). Only ever on
the sandbox's own bus and display.
"""
import os
import subprocess
import sys
import time

display, home = os.environ.get("DISPLAY"), os.environ.get("HEYO_TEST_HOME", "")
if display != os.environ.get("HEYO_TEST_DISPLAY") or display in (None, "", ":0", ":1"):
    sys.exit("ask.py: not the sandbox's display")
if not home or home + "/" not in os.environ.get("DBUS_SESSION_BUS_ADDRESS", ""):
    sys.exit("ask.py: not the sandbox's bus")

import gi  # noqa: E402

gi.require_version("Atspi", "2.0")
from gi.repository import Atspi, Gio, GLib  # noqa: E402


def enable():
    """Says an assistive tool is there, if nothing has yet."""
    bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
    status = ("org.a11y.Bus", "/org/a11y/bus", "org.freedesktop.DBus.Properties")
    (enabled,) = bus.call_sync(*status, "Get", GLib.Variant("(ss)", ("org.a11y.Status", "IsEnabled")), None, Gio.DBusCallFlags.NONE, 5000, None).unpack()
    if not enabled:
        bus.call_sync(*status, "Set", GLib.Variant("(ssv)", ("org.a11y.Status", "IsEnabled", GLib.Variant("b", True))), None, Gio.DBusCallFlags.NONE, 5000, None)
    return not enabled


def heyo_windows():
    """The top windows of every application on the bus that is Heyo's."""
    desktop = Atspi.get_desktop(0)
    found = []
    for index in range(desktop.get_child_count()):
        application = desktop.get_child_at_index(index)
        if application is None:
            continue
        try:
            for inner in range(application.get_child_count()):
                window = application.get_child_at_index(inner)
                if window is not None and (window.get_name() or "").startswith("Heyo"):
                    found.append(window)
        except GLib.Error:
            continue
    return found


def windows(title=None, wait=8.0):
    just_enabled = enable()
    Atspi.init()
    deadline = time.time() + wait
    while True:
        found = [window for window in heyo_windows() if title is None or window.get_name() == title]
        if found or time.time() > deadline:
            return found
        time.sleep(0.3 if not just_enabled else 0.5)


def rectangle(node):
    try:
        box = node.get_extents(Atspi.CoordType.SCREEN)
        return box.x, box.y, box.width, box.height
    except GLib.Error:
        return None


def states(node):
    try:
        return sorted(state.value_nick for state in node.get_state_set().get_states())
    except GLib.Error:
        return []


def walk(node, depth=0):
    yield node, depth
    try:
        count = node.get_child_count()
    except GLib.Error:
        return
    for index in range(count):
        try:
            child = node.get_child_at_index(index)
        except GLib.Error:
            continue
        if child is not None:
            yield from walk(child, depth + 1)


def find(role, name, title):
    nth = 1
    head, _, tail = name.rpartition(" -")
    if head and tail.isdigit():
        name, nth = head, int(tail)
    matches = []
    for window in windows(title):
        for node, _ in walk(window):
            try:
                if node.get_role_name() == role and (node.get_name() or "") == name:
                    matches.append(node)
            except GLib.Error:
                continue
    return matches[nth - 1] if len(matches) >= nth else None


def main():
    what, arguments = (sys.argv[1] if len(sys.argv) > 1 else ""), sys.argv[2:]
    if what == "tree":
        found = windows(arguments[0] if arguments else None)
        if not found:
            sys.exit("ask.py: no window of Heyo's is on the accessibility bus")
        for window in found:
            for node, depth in walk(window):
                box = rectangle(node)
                where = "%d,%d %dx%d" % box if box else "?"
                print(f"{'  ' * depth}{node.get_role_name()} \"{node.get_name() or ''}\" [{where}] {' '.join(states(node))}")
        return
    if what not in ("where", "click", "states", "wait") or len(arguments) < 2:
        sys.exit(__doc__)
    role, name, title = arguments[0], arguments[1], arguments[2] if len(arguments) > 2 else None
    node = find(role, name, title)
    deadline = time.time() + (10 if what == "wait" else 0)
    while node is None and time.time() < deadline:
        time.sleep(0.3)
        node = find(role, name, title)
    if node is None:
        sys.exit(f"ask.py: no {role} named \"{name}\"{' in ' + title if title else ''}")
    if what == "states":
        print(" ".join(states(node)))
        return
    box = rectangle(node)
    if box is None or box[2] <= 0 or box[3] <= 0:
        sys.exit(f"ask.py: the {role} \"{name}\" does not say where it is")
    if what == "click":
        subprocess.run(["xdotool", "mousemove", str(box[0] + box[2] // 2), str(box[1] + box[3] // 2), "click", "1"], check=True)
    print(*box)


if __name__ == "__main__":
    main()

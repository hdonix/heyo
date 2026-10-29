#!/usr/bin/env python3
"""Inside a test session: what one of Heyo's Slint windows says is in it,
asked of the window on the session's accessibility bus (AT-SPI, through
AccessKit), so that a check need not carry the window's layout as numbers.

  atspi.py tree [TITLE]              role, name and rectangle of everything in Heyo's windows
  atspi.py where ROLE NAME [TITLE]   X Y W H of a control, in the window's own pixels

A Wayland client does not know where its window is on the screen, so the
rectangles are the window's own (AT-SPI's window coordinates), in its
pixels; where the window is, the compositor says. From Python: `from atspi
import where, tree`.

AccessKit stays quiet until an assistive tool is there: the first question
says one is (org.a11y.Status), and waits for Heyo to appear.
"""
import sys, time

import gi

gi.require_version("Atspi", "2.0")
from gi.repository import Atspi, Gio, GLib  # noqa: E402


def enable():
    """Says an assistive tool is there; true when nothing had before."""
    bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
    status = ("org.a11y.Bus", "/org/a11y/bus", "org.freedesktop.DBus.Properties")
    (enabled,) = bus.call_sync(*status, "Get", GLib.Variant("(ss)", ("org.a11y.Status", "IsEnabled")), None, Gio.DBusCallFlags.NONE, 5000, None).unpack()
    if not enabled:
        bus.call_sync(*status, "Set", GLib.Variant("(ssv)", ("org.a11y.Status", "IsEnabled", GLib.Variant("b", True))), None, Gio.DBusCallFlags.NONE, 5000, None)
    return not enabled


_ready = False


def windows(title=None, wait=8.0):
    """Heyo's top windows on the bus (of that title)."""
    global _ready
    if not _ready:
        enable()
        Atspi.init()
        _ready = True
    deadline = time.time() + wait
    while True:
        found = []
        desktop = Atspi.get_desktop(0)
        for index in range(desktop.get_child_count()):
            application = desktop.get_child_at_index(index)
            try:
                for inner in range(application.get_child_count() if application else 0):
                    window = application.get_child_at_index(inner)
                    name = (window.get_name() or "") if window else ""
                    if name.startswith("Heyo") and (title is None or name == title):
                        found.append(window)
            except GLib.Error:
                continue
        if found or time.time() > deadline:
            return found
        time.sleep(0.3)


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


def rectangle(node):
    try:
        box = node.get_extents(Atspi.CoordType.WINDOW)
        return box.x, box.y, box.width, box.height
    except GLib.Error:
        return None


def tree(title=None):
    """(depth, role, name, rectangle) of everything in those windows."""
    found = []
    for window in windows(title):
        for node, depth in walk(window):
            try:
                found.append((depth, node.get_role_name(), node.get_name() or "", rectangle(node)))
            except GLib.Error:
                continue
    return found


def node(role, name, title=None, wait=6.0):
    """The first control of that role (any, for None) and name that says where it is, or None."""
    deadline = time.time() + wait
    while True:
        for window in windows(title):
            for found, _ in walk(window):
                try:
                    box = rectangle(found)
                    if role in (None, found.get_role_name()) and (found.get_name() or "") == name and box and box[2] > 0 and box[3] > 0:
                        return found
                except GLib.Error:
                    continue
        if time.time() > deadline:
            return None
        time.sleep(0.3)


def where(role, name, title=None, wait=6.0):
    """The rectangle of the first control of that role and name, or None."""
    found = node(role, name, title, wait)
    return rectangle(found) if found else None


def states(role, name, title=None):
    """Its states (selected, focused, checked…)."""
    found = node(role, name, title, 2.0)
    return sorted(state.value_nick for state in found.get_state_set().get_states()) if found else []


def text(role, name, title=None):
    """What a field has in it, or None."""
    found = node(role, name, title, 2.0)
    try:
        return Atspi.Text.get_text(found, 0, -1) if found else None
    except (GLib.Error, TypeError, NotImplementedError):
        return None


def in_pixels(title, width, height, density):
    """Whether the window, its sidebar's first entry and a row of its page (Heyo's settings) all say where
    they are in the window's own pixels, `width` by `height` of them and `density` to a logical one: the
    entry 208 logical pixels wide and 20 in from the panel's edge (settings.slint), the row right of it,
    both inside the window. And what each says. How large the window says it is may lag behind its size,
    though not by a scale: AccessKit hears of the window's size only from winit, which does not say it when
    the window asked for a size itself (Wayland)."""
    said = {name: where(role, name, title) for role, name in (("frame", title), ("list item", "General"), ("combo box", "Language"))}
    whole, entry, row = said[title], said["General"], said["Language"]
    if not (whole and entry and row):
        return False, said
    inside = lambda box: box[0] >= 0 and box[1] >= 0 and box[0] + box[2] <= width + 1 and box[1] + box[3] <= height + 1
    window = whole[0] == 0 and whole[1] == 0 and 0.75 * width <= whole[2] <= width + 1 and 0.75 * height <= whole[3] <= height + 1
    first = abs(entry[2] - 208 * density) <= 2 and any(abs(entry[0] - (margin + 20) * density) <= 2 for margin in (0, 32))
    return window and first and inside(entry) and inside(row) and row[0] > entry[0] + entry[2], said


if __name__ == "__main__":
    what, arguments = (sys.argv[1] if len(sys.argv) > 1 else ""), sys.argv[2:]
    if what == "tree":
        for depth, role, name, box in tree(arguments[0] if arguments else None):
            print(f"{'  ' * depth}{role} \"{name}\" [{'%d,%d %dx%d' % box if box else '?'}]")
    elif what == "where" and len(arguments) >= 2:
        box = where(*arguments[:3])
        if not box:
            sys.exit(f"no {arguments[0]} named {arguments[1]!r}")
        print(*box)
    else:
        sys.exit(__doc__)

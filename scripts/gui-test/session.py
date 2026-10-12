#!/usr/bin/python3
"""What a session publishes of its display, and changing it in the sandbox.

    session.py check              what the session on $DISPLAY publishes: the screen and its
                                  monitors, the resource database, XSETTINGS, the pointer,
                                  and what of a scale is in the environment. It only reads,
                                  so it can be run on a real desktop too, to compare.
    session.py scale N            the scale of the sandbox's GNOME session becomes N
    session.py xfce-scale N       the same of its Xfce settings daemon
    session.py screen WxH         the screen's size becomes that (RandR)
    session.py monitors N         the screen is N monitors side by side (RandR 1.5), or one again

`scale` asks the Shell to lay its monitors out at that scale
(org.gnome.Mutter.DisplayConfig, ApplyMonitorsConfig), as GNOME's Settings
do: it then says so to the settings daemon, which publishes it (XSETTINGS,
Xft.dpi, Xcursor.size) to every program on the display. This is what
happens to a running program when a monitor of another density is
connected.

The system's Python: it has the bindings to GLib. Everything but `check`
refuses a display or a bus that is not the sandbox's.
"""
import ctypes
import os
import re
import struct
import subprocess
import sys


def sandbox_only():
    display, home = os.environ.get("DISPLAY"), os.environ.get("HEYO_TEST_HOME", "")
    if display != os.environ.get("HEYO_TEST_DISPLAY") or display in (None, "", ":0", ":1"):
        sys.exit("session.py: not the sandbox's display")
    if not home or home + "/" not in os.environ.get("DBUS_SESSION_BUS_ADDRESS", ""):
        sys.exit("session.py: not the sandbox's bus")


def run(*command):
    try:
        return subprocess.run(command, capture_output=True, text=True, timeout=15).stdout
    except (OSError, subprocess.TimeoutExpired):
        return ""


# --- reading -------------------------------------------------------------------------


class CursorImage(ctypes.Structure):
    _fields_ = [
        ("x", ctypes.c_short),
        ("y", ctypes.c_short),
        ("width", ctypes.c_ushort),
        ("height", ctypes.c_ushort),
        ("xhot", ctypes.c_ushort),
        ("yhot", ctypes.c_ushort),
        ("serial", ctypes.c_ulong),
        ("pixels", ctypes.POINTER(ctypes.c_ulong)),
        ("atom", ctypes.c_ulong),
        ("name", ctypes.c_char_p),
    ]


def x_display():
    x11 = ctypes.CDLL("libX11.so.6")
    x11.XOpenDisplay.restype = ctypes.c_void_p
    x11.XInternAtom.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_int]
    x11.XInternAtom.restype = ctypes.c_ulong
    x11.XGetSelectionOwner.argtypes = [ctypes.c_void_p, ctypes.c_ulong]
    x11.XGetSelectionOwner.restype = ctypes.c_ulong
    display = x11.XOpenDisplay(os.environ.get("DISPLAY", "").encode())
    if not display:
        sys.exit("session.py: no display")
    return x11, ctypes.c_void_p(display)


def pointer():
    """The pointer as it is drawn now: (name, width, height)."""
    x11, display = x_display()
    xfixes = ctypes.CDLL("libXfixes.so.3")
    xfixes.XFixesGetCursorImage.restype = ctypes.POINTER(CursorImage)
    xfixes.XFixesGetCursorImage.argtypes = [ctypes.c_void_p]
    image = xfixes.XFixesGetCursorImage(display)
    if not image:
        return "", 0, 0
    name = image.contents.name.decode() if image.contents.name else ""
    return name, image.contents.width, image.contents.height


def xsettings():
    """What the XSETTINGS manager of the first screen publishes: (owner, {name: value}), or (0, {})."""
    x11, display = x_display()
    owner = x11.XGetSelectionOwner(display, x11.XInternAtom(display, b"_XSETTINGS_S0", 0))
    if not owner:
        return 0, {}
    kind, form, count, left, data = ctypes.c_ulong(), ctypes.c_int(), ctypes.c_ulong(), ctypes.c_ulong(), ctypes.POINTER(ctypes.c_ubyte)()
    x11.XGetWindowProperty.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_long, ctypes.c_long, ctypes.c_int, ctypes.c_ulong] + [ctypes.c_void_p] * 5
    x11.XGetWindowProperty(display, owner, x11.XInternAtom(display, b"_XSETTINGS_SETTINGS", 0), 0, 1 << 20, 0, 0, ctypes.byref(kind), ctypes.byref(form), ctypes.byref(count), ctypes.byref(left), ctypes.byref(data))
    raw = bytes(data[: count.value]) if data else b""
    if len(raw) < 12:
        return owner, {}
    order = "<" if raw[0] == 0 else ">"
    (settings,) = struct.unpack_from(order + "I", raw, 8)
    at, found = 12, {}
    for _ in range(settings):
        kind, length = struct.unpack_from(order + "BxH", raw, at)
        name = raw[at + 4 : at + 4 + length].decode(errors="replace")
        at += 4 + (length + 3) // 4 * 4 + 4
        if kind == 0:
            (found[name],) = struct.unpack_from(order + "i", raw, at)
            at += 4
        elif kind == 1:
            (length,) = struct.unpack_from(order + "I", raw, at)
            found[name] = raw[at + 4 : at + 4 + length].decode(errors="replace")
            at += 4 + (length + 3) // 4 * 4
        else:
            found[name] = struct.unpack_from(order + "4H", raw, at)
            at += 8
    return owner, found


def resources():
    return dict(re.findall(r"^(\S+?):[ \t]*(.*)$", run("xrdb", "-query"), re.M))


def mutter():
    """The Shell's own layout, if a Shell is on the session bus: (logical monitors, properties)."""
    try:
        from gi.repository import Gio

        bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
        _, monitors, logical, properties = bus.call_sync("org.gnome.Mutter.DisplayConfig", "/org/gnome/Mutter/DisplayConfig", "org.gnome.Mutter.DisplayConfig", "GetCurrentState", None, None, Gio.DBusCallFlags.NO_AUTO_START, 3000, None).unpack()
        return logical, properties
    except Exception:
        return None, None


def check():
    print(f"display {os.environ.get('DISPLAY')}")
    info = run("xdpyinfo")
    for wanted in ("dimensions", "resolution"):
        found = re.search(rf"{wanted}:\s+(.*)", info)
        print(f"  screen {wanted}: {found.group(1) if found else '?'}")
    print("  monitors (RandR):")
    for line in run("xrandr", "--listmonitors").splitlines()[1:]:
        print("   ", line.strip())
    database = resources()
    print("resource database (xrdb -query):")
    for name in ("Xft.dpi", "Xcursor.size", "Xcursor.theme"):
        print(f"  {name}: {database.get(name, '(not there)')}")
    owner, settings = xsettings()
    print(f"XSETTINGS: {'published by window ' + hex(owner) if owner else 'nobody publishes them'}")
    for name in ("Gdk/WindowScalingFactor", "Gdk/UnscaledDPI", "Xft/DPI", "Gtk/CursorThemeSize", "Gtk/CursorThemeName"):
        value = settings.get(name)
        about = f" ({value / 1024:g} dpi)" if isinstance(value, int) and name.endswith("DPI") else ""
        print(f"  {name}: {value if value is not None else '(not there)'}{about}")
    logical, properties = mutter()
    if logical is not None:
        print("the Shell's layout (org.gnome.Mutter.DisplayConfig):")
        for x, y, scale, transform, primary, monitors, _ in logical:
            print(f"  {monitors[0][0]} at {x},{y} scale {scale:g}{' primary' if primary else ''}")
        print(f"  layout-mode {properties.get('layout-mode')}, legacy-ui-scaling-factor {properties.get('legacy-ui-scaling-factor')}")
    try:
        import gi

        gi.require_version("Gdk", "3.0")
        from gi.repository import Gdk

        found = Gdk.Display.get_default()
        print("what GTK finds there:")
        for index in range(found.get_n_monitors()):
            monitor = found.get_monitor(index)
            box = monitor.get_geometry()
            print(f"  monitor {index}: {box.width}x{box.height}+{box.x}+{box.y} in its own units, scale {monitor.get_scale_factor()}{' primary' if monitor.is_primary() else ''}")
    except Exception as error:
        print(f"what GTK finds there: not asked ({error})")
    name, width, height = pointer()
    print(f"the pointer as drawn now: {name or '(unnamed)'} {width}x{height}")
    said = {name: os.environ[name] for name in ("GDK_SCALE", "GDK_DPI_SCALE", "WINIT_X11_SCALE_FACTOR", "QT_SCALE_FACTOR", "QT_AUTO_SCREEN_SCALE_FACTOR") if name in os.environ}
    print(f"of a scale in the environment: {said or 'nothing'}")
    dpi = database.get("Xft.dpi")
    print(f"so a program finds scale {float(dpi) / 96:g} and a pointer of {database.get('Xcursor.size', '?')}" if dpi else "so a program finds no scale but the monitor's own size")


# --- changing ------------------------------------------------------------------------


def scale(wanted):
    sandbox_only()
    from gi.repository import Gio, GLib

    bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
    shell = ("org.gnome.Mutter.DisplayConfig", "/org/gnome/Mutter/DisplayConfig", "org.gnome.Mutter.DisplayConfig")
    # The settings have it too, as they had it when the session started: a
    # scale said there outranks the layout's.
    subprocess.run(["gsettings", "set", "org.gnome.desktop.interface", "scaling-factor", str(int(wanted))], check=False)
    serial, monitors, logical, _ = bus.call_sync(*shell, "GetCurrentState", None, None, Gio.DBusCallFlags.NONE, 5000, None).unpack()
    modes = {connector: next((mode[0] for mode in modes if mode[6].get("is-current")), modes[0][0]) for (connector, *_), modes, _ in monitors}
    layout = [(x, y, float(wanted), transform, primary, [(connector, modes[connector], {}) for connector, *_ in members]) for x, y, _, transform, primary, members, _ in logical]
    # 1: for now, not kept in a file.
    bus.call_sync(*shell, "ApplyMonitorsConfig", GLib.Variant("(uua(iiduba(ssa{sv}))a{sv})", (serial, 1, layout, {})), None, Gio.DBusCallFlags.NONE, 5000, None)


def xfce_scale(wanted, quiet=False):
    """Xfce's settings daemon is given what GNOME's publishes at that scale,
    name for name: the window scale, the density with it and without, the
    pointer's size. (An Xfce desktop at 2x says less: its Xft.dpi stays 96.
    The sandbox is to stand for the GNOME desktop Heyo is used on, under
    another window manager.)"""
    sandbox_only()
    theme = next((name for name in ("Yaru", "Adwaita") if os.path.isdir(f"/usr/share/icons/{name}/cursors")), "default")
    wanted_settings = (
        ("/Gtk/CursorThemeName", "string", theme),
        ("/Gtk/CursorThemeSize", "int", 24 * wanted),
        ("/Gdk/UnscaledDPI", "int", 96 * 1024),
        ("/Gdk/WindowScalingFactor", "int", wanted),
        ("/Xft/DPI", "int", 96 * wanted),
    )
    for name, kind, value in wanted_settings:
        subprocess.run(["xfconf-query", "-c", "xsettings", "-p", name, "-n", "-t", kind, "-s", str(value)], check=False, capture_output=quiet)


def screen(size):
    sandbox_only()
    said = run("xrandr", "--current")
    before = re.search(r"current (\d+) x (\d+), maximum (\d+) x (\d+)", said)
    output = re.search(r"^(\S+) connected", said, re.M).group(1)
    width, height = map(int, size.split("x"))
    # Xvfb has one output and the memory it was started with: any size up
    # to that can be made a mode of it, and none above.
    if width > int(before.group(3)) or height > int(before.group(4)):
        sys.exit(f"session.py: Xvfb was started at {before.group(3)}x{before.group(4)} and cannot be larger than that")
    if not re.search(rf"^\s+{size}\s", said, re.M):
        subprocess.run(["xrandr", "--newmode", size, "0", *[str(width)] * 4, *[str(height)] * 4, ], check=False, capture_output=True)
        subprocess.run(["xrandr", "--addmode", output, size], check=False, capture_output=True)
    done = subprocess.run(["xrandr", "--output", output, "--mode", size], capture_output=True, text=True)
    after = re.search(r"current (\d+) x (\d+)", run("xrandr", "--current"))
    print(f"the screen was {before.group(1)}x{before.group(2)} and is {after.group(1)}x{after.group(2)}")
    if done.returncode:
        sys.exit(f"session.py: {done.stderr.strip()}")


def monitors(count):
    sandbox_only()
    current = re.search(r"current (\d+) x (\d+)", run("xrandr", "--current"))
    width, height = int(current.group(1)), int(current.group(2))
    for line in run("xrandr", "--listmonitors").splitlines()[1:]:
        name = line.split()[1].lstrip("+*")
        if name.startswith("HEYO-"):
            subprocess.run(["xrandr", "--delmonitor", name], check=False)
    if count > 1:
        output = re.search(r"^(\S+) connected", run("xrandr", "--current"), re.M).group(1)
        each = width // count
        for index in range(count):
            # The first has the output; the others are monitors without one.
            subprocess.run(["xrandr", "--setmonitor", f"HEYO-{index + 1}", f"{each}/{each * 254 // 960}x{height}/{height * 254 // 960}+{each * index}+0", output if index == 0 else "none"], check=False)
    print(run("xrandr", "--listmonitors").strip())


if __name__ == "__main__":
    what, arguments = (sys.argv[1] if len(sys.argv) > 1 else ""), sys.argv[2:]
    if what == "check":
        check()
    elif what == "scale" and arguments:
        scale(int(arguments[0]))
    elif what == "xfce-scale" and arguments:
        xfce_scale(int(arguments[0]), quiet=len(arguments) > 1)
    elif what == "screen" and arguments:
        screen(arguments[0])
    elif what == "monitors" and arguments:
        monitors(int(arguments[0]))
    else:
        sys.exit(__doc__)

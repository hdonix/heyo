#!/usr/bin/env python3
"""Inside the test session: sets the monitors' scales and how they are laid
out, as the Displays settings would (through Mutter's DisplayConfig).

  monitors.py 2            the one monitor at 200 %
  monitors.py 2 1          two monitors side by side, the first at 200 %
  monitors.py 1.5          fractional
  monitors.py --pixels 2   laid out in pixels instead of scaled units: what
                           GNOME does until fractional scaling is switched
                           on (the default on Ubuntu 24.04)

A scale the monitor does not offer is refused. The monitors themselves are
given when the session starts: MONITORS="1920x1080 1280x720" run.sh start.
"""
import sys
from gi.repository import Gio, GLib

args = sys.argv[1:]
pixels = "--pixels" in args
scales = [float(a) for a in args if a != "--pixels"]
if not scales:
    sys.exit(__doc__)

bus = Gio.bus_get_sync(Gio.BusType.SESSION)
def call(method, args=None):
    return bus.call_sync("org.gnome.Mutter.DisplayConfig", "/org/gnome/Mutter/DisplayConfig", "org.gnome.Mutter.DisplayConfig",
                         method, args, None, Gio.DBusCallFlags.NONE, -1, None)

def apply(scales):
    serial, monitors, _logical, _properties = call("GetCurrentState").unpack()
    if len(scales) != len(monitors):
        sys.exit(f"{len(monitors)} monitor(s), {len(scales)} scale(s)")
    layout, x = [], 0
    for (spec, modes, _props), wanted in zip(monitors, scales):
        mode_id, width, height, _rate, _preferred, supported, _mode_props = next(m for m in modes if m[6].get("is-current"))
        scale = min(supported, key=lambda s: abs(s - wanted))
        if abs(scale - wanted) > 0.01:
            sys.exit(f"{spec[0]} offers {[round(s, 2) for s in supported]}, not {wanted}")
        layout.append((x, 0, scale, 0, x == 0, [(spec[0], mode_id, {})]))
        # Scaled units, or pixels.
        x += width if pixels else round(width / scale)
    # 1: until the session ends. Layout mode 1 is scaled ("logical"), 2 is pixels ("physical").
    call("ApplyMonitorsConfig", GLib.Variant("(uua(iiduba(ssa{sv}))a{sv})", (serial, 1, layout, {"layout-mode": GLib.Variant("u", 2 if pixels else 1)})))
    return layout

# The scales a monitor offers depend on the layout: change that first.
apply([1.0] * len(scales))
layout = apply(scales)
print("applied:", ", ".join(f"{m[5][0][0]} at x={m[0]} scale {m[2]:g}" for m in layout), "(pixels)" if pixels else "(scaled)")

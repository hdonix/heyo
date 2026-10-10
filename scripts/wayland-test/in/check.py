#!/usr/bin/env python3
"""Inside the test session, with Heyo running (start-heyo.sh): checks what
Heyo does through its Shell extension against what the Shell itself says.

  check.py lists      Heyo's palette is in no list of windows or applications; its settings window is
  check.py layout     a window is put where a layout says, on every monitor
  check.py capture    a screenshot is of the monitor the pointer is on, in that monitor's own
                      pixels; the overlay covers that monitor; a click selects the window under it
  check.py relock     the extension switched off and on again (as locking the screen does):
                      the held key is watched again

Set the monitors' scales first (monitors.py) to check a layout other than
one monitor at 100 %. Prints one line per check; exits 1 if any failed.
"""
import glob, os, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from shell import run

HERE = os.path.dirname(os.path.abspath(__file__))
HEYO = f"/heyo/src-tauri/target/{os.environ.get('HEYO_BUILD', 'release')}/heyo"
failed = []

def check(what, ok, detail=""):
    print(("ok   " if ok else "FAIL ") + what + (f"  ({detail})" if detail and not ok else ""))
    if not ok:
        failed.append(what)

def heyo(*args):
    subprocess.run([HEYO, *args], check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

def tool(name, *args):
    return subprocess.Popen([os.path.join(HERE, name), *map(str, args)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

def wait(condition, seconds=6.0):
    """What `condition` gives once it gives something, or None in the end."""
    end = time.time() + seconds
    while time.time() < end:
        value = condition()
        if value:
            return value
        time.sleep(0.15)
    return None

WINDOW = """(w => w && (r => ({id: w.get_id(), title: w.get_title(), monitor: w.get_monitor(), fullscreen: w.is_fullscreen(),
    skip: w.skip_taskbar, x: r.x, y: r.y, width: r.width, height: r.height}))(w.get_frame_rect()))"""

def window(title):
    return run(f"{WINDOW}(global.get_window_actors().map(a => a.meta_window).find(w => w.get_title() === {title!r}))")

def monitors():
    """Each monitor in the Shell's units, with `density`: pixels per unit."""
    state = run("""({
        monitors: Main.layoutManager.monitors.map((m, i) => {
            const work = global.workspace_manager.get_active_workspace().get_work_area_for_monitor(i);
            return {x: m.x, y: m.y, width: m.width, height: m.height, scale: global.display.get_monitor_scale(i),
                    work: {x: work.x, y: work.y, width: work.width, height: work.height}};
        }),
        stage: global.stage.get_resource_scale(),
    })""")
    for monitor in state["monitors"]:
        monitor["density"] = monitor["scale"] if state["stage"] > 1 else 1
    return state["monitors"]

def place(title, monitor, x, y):
    find = f"global.get_window_actors().map(a => a.meta_window).find(w => w.get_title() === {title!r})"
    run(f"{find}.move_to_monitor({monitor})")
    # On a monitor of another scale the application draws itself anew, at another size.
    time.sleep(1.2)
    run(f"(w => {{ w.move_frame(true, {x}, {y}); Main.activateWindow(w); }})({find})")
    time.sleep(0.8)

def point(x, y):
    run(f"imports.gi.Clutter.get_default_backend().get_default_seat().warp_pointer({int(x)}, {int(y)})")
    time.sleep(0.2)

def close(title):
    run(f"global.get_window_actors().map(a => a.meta_window).filter(w => w.get_title() === {title!r}).forEach(w => w.delete(global.get_current_time()))")

def near(a, b, tolerance):
    return all(abs(a[k] - b[k]) <= tolerance for k in ("x", "y", "width", "height"))

def pixel(path, x, y):
    import gi
    gi.require_version("GdkPixbuf", "2.0")
    from gi.repository import GdkPixbuf
    image = GdkPixbuf.Pixbuf.new_from_file(path)
    offset = int(y) * image.get_rowstride() + int(x) * image.get_n_channels()
    return (image.get_width(), image.get_height()), tuple(image.get_pixels()[offset:offset + 3])

def is_colour(rgb, colour):
    return all(abs(a - b) <= 6 for a, b in zip(rgb, bytes.fromhex(colour)))


def lists():
    subprocess.run([os.path.join(HERE, "settings.py"), "hideOnBlur=false"], check=True, stdout=subprocess.DEVNULL)
    # Settings are read at start.
    subprocess.run([os.path.join(HERE, "start-heyo.sh")], check=True, stdout=subprocess.DEVNULL)
    time.sleep(3)
    other = tool("window.py", "An Application", "check-lists")
    wait(lambda: window("An Application"))
    lists = """({
        running: Shell.AppSystem.get_default().get_running().map(a => a.get_windows()[0]?.get_wm_class() ?? a.get_id()),
        switcher: global.display.get_tab_list(Meta.TabList.NORMAL_ALL, null).filter(w => !w.skip_taskbar).map(w => w.get_title()),
        overview: global.get_window_actors().map(a => a.meta_window).filter(w => !w.skip_taskbar).map(w => w.get_title()),
        native: typeof Meta.Window.prototype.hide_from_window_list === 'function',
    })"""
    is_heyo = lambda app: app.lower() == "heyo"

    def once(how):
        if not window("Heyo"):
            heyo()  # shows the palette
        palette = wait(lambda: window("Heyo"))
        check(f"{how}: the palette is shown", bool(palette))
        time.sleep(0.5)
        state = run(lists)
        check(f"{how}: the palette is marked to be skipped", bool(palette and window("Heyo")["skip"]))
        check(f"{how}: no Heyo among the running applications (Dash, application switcher)", not any(map(is_heyo, state["running"])), state["running"])
        check(f"{how}: the palette is not in the window switcher", "Heyo" not in state["switcher"], state["switcher"])
        check(f"{how}: the palette is not in the overview", "Heyo" not in state["overview"], state["overview"])
        check(f"{how}: another application still is in all three", any("check-lists" in a for a in state["running"])
              and "An Application" in state["switcher"] and "An Application" in state["overview"], state)
        check(f"{how}: the palette can still be given the focus", run("global.display.focus_window?.get_title()") == "Heyo")
        heyo("--open", "heyo:settings")
        settings = wait(lambda: window("Heyo Settings"))
        time.sleep(0.5)
        state = run(lists)
        check(f"{how}: the settings window is an ordinary one", bool(settings) and not settings["skip"] and "Heyo Settings" in state["switcher"]
              and any(map(is_heyo, state["running"])), state)
        close("Heyo Settings")
        wait(lambda: not window("Heyo Settings"))
        if window("Heyo"):
            heyo()  # hides the palette
        wait(lambda: not window("Heyo"))
        return state["native"]

    native = once("as this Shell does it")
    if native:
        # What GNOME 45 to 48 have to do, tried on this Shell: the extension
        # looks for the method each time it is switched on.
        run("(p => { globalThis._hide = p.hide_from_window_list; p.hide_from_window_list = undefined; })(Meta.Window.prototype)")
        subprocess.run(["gnome-extensions", "disable", "heyo@heyo.app"], check=True)
        subprocess.run(["gnome-extensions", "enable", "heyo@heyo.app"], check=True)
        time.sleep(1)
        once("as before GNOME 49")
        run("Meta.Window.prototype.hide_from_window_list = globalThis._hide")
        subprocess.run(["gnome-extensions", "disable", "heyo@heyo.app"], check=True)
        subprocess.run(["gnome-extensions", "enable", "heyo@heyo.app"], check=True)
    other.terminate()


def layout():
    app = tool("window.py", "To Arrange", "check-layout")
    wait(lambda: window("To Arrange"))
    all_monitors = monitors()
    for index, monitor in enumerate(all_monitors):
        work = monitor["work"]
        place("To Arrange", index, work["x"] + 40, work["y"] + 40)
        for name, expected in (
            ("left-half", {"x": work["x"], "y": work["y"], "width": work["width"] // 2, "height": work["height"]}),
            ("bottom-right", {"x": work["x"] + work["width"] // 2, "y": work["y"] + work["height"] // 2, "width": work["width"] // 2, "height": work["height"] // 2}),
        ):
            heyo("--open", f"heyo:window-{name}")
            got = wait(lambda: (lambda w: w if near(w, expected, 2) else None)(window("To Arrange")), 3) or window("To Arrange")
            check(f"monitor {index} (scale {monitor['scale']:g}): {name} lands on its part of the free area", near(got, expected, 2), f"{got} instead of {expected}")
    if len(all_monitors) > 1:
        place("To Arrange", 0, all_monitors[0]["work"]["x"] + 40, all_monitors[0]["work"]["y"] + 40)
        heyo("--open", "heyo:window-next-display")
        moved = wait(lambda: (lambda w: w if w["monitor"] == 1 else None)(window("To Arrange")), 3)
        check("next display: the window moves to the other monitor", bool(moved), window("To Arrange"))
        # GNOME Shell 50.1 dies when a window is closed in the moment after
        # it was moved to a monitor of another scale, laid out in pixels
        # (seen with and without this version of the extension).
        time.sleep(2)
    app.terminate()


def capture():
    colour = "d81b60"
    app = tool("colour.py", "A Colour", "check-capture", colour, 400, 300)
    wait(lambda: window("A Colour"))
    pictures = subprocess.run(["xdg-user-dir", "PICTURES"], capture_output=True, text=True).stdout.strip()
    if pictures in ("", os.path.expanduser("~")):
        pictures = os.path.expanduser("~/Pictures")
    for index, monitor in enumerate(monitors()):
        how = f"monitor {index} (scale {monitor['scale']:g}, {monitor['density']:g} px per unit)"
        place("A Colour", index, monitor["x"] + 200, monitor["y"] + 120)
        frame = window("A Colour")
        # Somewhere on this monitor that is not the window.
        point(monitor["x"] + 60, monitor["y"] + monitor["height"] - 60)
        heyo("--open", "heyo:screenshot")
        overlay = wait(lambda: window("Heyo Screenshot"))
        time.sleep(1.0)
        overlay = window("Heyo Screenshot")
        check(f"{how}: the overlay opens", bool(overlay))
        if not overlay:
            continue
        check(f"{how}: the overlay is on the monitor the pointer is on", overlay["monitor"] == index, overlay)
        check(f"{how}: the overlay covers that monitor", near(overlay, monitor, 0), f"{overlay} over {monitor}")
        check(f"{how}: the overlay is kept out of the lists", overlay["skip"])
        captures = sorted(glob.glob(os.path.expanduser("~/.cache/heyo/screenshot/capture-*.bmp")), key=os.path.getmtime)
        check(f"{how}: the capture is there", bool(captures))
        if captures:
            density = monitor["density"]
            size, inside = pixel(captures[-1], (frame["x"] - monitor["x"] + frame["width"] / 2) * density, (frame["y"] - monitor["y"] + frame["height"] / 2) * density)
            _, outside = pixel(captures[-1], (frame["x"] - monitor["x"] - 30) * density, (frame["y"] - monitor["y"] + frame["height"] / 2) * density)
            expected = (round(monitor["width"] * density), round(monitor["height"] * density))
            check(f"{how}: the capture has the monitor's own pixels", size == expected, f"{size} instead of {expected}")
            check(f"{how}: the window is where its frame says in the capture", is_colour(inside, colour) and not is_colour(outside, colour), f"inside {inside}, outside {outside}")
        # A click on the window selects it; Ctrl+S saves the selection.
        before = set(glob.glob(os.path.join(pictures, "Screenshots", "*.png")))
        point(frame["x"] + frame["width"] / 2, frame["y"] + frame["height"] / 2)
        subprocess.run([os.path.join(HERE, "input.py"), "seq", "nudge:5", "wait:0.3", "nudge:-4", "wait:0.4", "press:left", "wait:0.6", "tap:ctrl+s"], check=True)
        saved = wait(lambda: set(glob.glob(os.path.join(pictures, "Screenshots", "*.png"))) - before, 5)
        check(f"{how}: a click on a window and Ctrl+S save a picture", bool(saved))
        if saved:
            path = saved.pop()
            expected = (round(frame["width"] * monitor["density"]), round(frame["height"] * monitor["density"]))
            size, middle = pixel(path, expected[0] / 2, expected[1] / 2)
            _, corner = pixel(path, 2, 2)
            _, far = pixel(path, size[0] - 3, size[1] - 3)
            check(f"{how}: the picture is the window, pixel for pixel", all(abs(a - b) <= 2 for a, b in zip(size, expected))
                  and all(is_colour(p, colour) for p in (middle, corner, far)), f"{size} instead of {expected}; {middle} {corner} {far}")
            os.remove(path)
        if window("Heyo Screenshot"):
            subprocess.run([os.path.join(HERE, "input.py"), "key", "Escape"], check=True)
        wait(lambda: not window("Heyo Screenshot"))
        time.sleep(0.3)
    app.terminate()


def relock():
    subprocess.run([os.path.join(HERE, "settings.py"), 'dictationHoldKey="alt_r"'], check=True, stdout=subprocess.DEVNULL)
    subprocess.run([os.path.join(HERE, "start-heyo.sh")], check=True, stdout=subprocess.DEVNULL)
    watched = lambda: open("/tmp/heyo.log").read().count("push-to-talk key: alt_r")
    check("the held key is watched", bool(wait(lambda: watched() == 1, 8)), watched())
    subprocess.run(["gnome-extensions", "disable", "heyo@heyo.app"], check=True)
    time.sleep(0.5)
    subprocess.run(["gnome-extensions", "enable", "heyo@heyo.app"], check=True)
    check("... and again after the extension was switched off and on", bool(wait(lambda: watched() == 2, 5)), watched())


CHECKS = {"lists": lists, "layout": layout, "capture": capture, "relock": relock}
if len(sys.argv) < 2 or any(name not in CHECKS for name in sys.argv[1:]):
    sys.exit(__doc__)
# For test.sh, which reads this through a pipe.
PASSED = "/tmp/check-passed"
if os.path.exists(PASSED):
    os.remove(PASSED)
for name in sys.argv[1:]:
    CHECKS[name]()
if failed:
    sys.exit(f"{len(failed)} failed")
open(PASSED, "w").close()

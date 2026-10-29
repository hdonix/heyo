#!/usr/bin/env python3
"""Inside the test session, with Heyo running (start-heyo.sh): checks what
Heyo does through its Shell extension against what the Shell itself says.

  check.py lists      Heyo's palette is in no list of windows or applications; its settings window is
  check.py layout     a window is put where a layout says, on every monitor
  check.py capture    a screenshot is of the monitor the pointer is on, in that monitor's own
                      pixels; the overlay covers that monitor; a click selects the window under it
  check.py relock     the extension switched off and on again (as locking the screen does):
                      the held key is watched again
  check.py own        the pill, the overlay's keyboard and pinned captures, which the Shell stands for
  check.py language   简体中文 chosen while the settings window is open, and every window in it
  check.py ai         AI Chat answered by a stub model through the host: a follow-up, Stop
                      Answering closing the connection, Escape not stopping the answer
  check.py dictate    a recording dictated into another application, as heard and polished by the stub
  check.py placement  the palette and the settings window on the monitor the pointer is on
  check.py cold       the first of Slint's windows comes up with Mesa's shader cache emptied
  check.py warm       no check: a Slint window drawn once, for the checks after it
  check.py settings   the settings window moved, clicked after, its margin let through, closed, back
  check.py ime        Chinese typed through IBus (Pinyin) into the settings window's search field
  check.py upgrade    with the extension of an earlier Heyo running: Settings updates it
  check.py upgraded   after the next login: the new one runs

Pictures of what was checked are kept in /tmp/shots/ to be looked at
(test.sh copies them out). A control of a Slint window is found by asking
the window (atspi.py).

Set the monitors' scales first (monitors.py) to check a layout other than
one monitor at 100 %. Prints one line per check; exits 1 if any failed.
"""
import glob, json, os, subprocess, sys, time

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

# How bright the overlay shows what is not selected (DIM of 256, in
# slint_windows/screenshot/mod.rs).
DIMMED = 154 / 256


def screen(path):
    """Saves the whole screen as the Shell draws it."""
    subprocess.run([os.path.join(HERE, "screenshot.sh"), path], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def on_screen(path, x, y):
    """The colour at a place given in the Shell's units, in a picture of the whole screen."""
    (width, height), _ = pixel(path, 0, 0)
    per = width / max(monitor["x"] + monitor["width"] for monitor in monitors())
    return pixel(path, min(width - 1, round(x * per)), min(height - 1, round(y * per)))[1]


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
        # Asked for with another window in front, it comes forward: which
        # on Wayland the Shell does for it, through Heyo's extension.
        focused = lambda: run("global.display.focus_window?.get_title() ?? ''")
        run("Main.activateWindow(global.get_window_actors().map(a => a.meta_window).find(w => w.get_title() === 'An Application')) ?? null")
        wait(lambda: focused() == "An Application")
        heyo("--open", "heyo:settings")
        check(f"{how}: the settings window comes forward from behind another", bool(wait(lambda: focused() == "Heyo Settings")), focused())
        close("Heyo Settings")
        wait(lambda: not window("Heyo Settings"))
        # Shown again it is a new window to the Shell, and must still be
        # Heyo's: by its class the extension knows its own.
        heyo("--open", "heyo:settings")
        wait(lambda: window("Heyo Settings"))
        own = lambda: run("global.get_window_actors().map(a => a.meta_window).find(w => w.get_title() === 'Heyo Settings')?.get_wm_class() ?? ''")
        check(f"{how}: shown again, the settings window is still Heyo's", bool(wait(lambda: own().lower() == "heyo")), own())
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
        time.sleep(0.3)
        screen("/tmp/check-before.png")
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
        # The overlay shows the monitor as it was, dimmed, each thing where
        # it was: the screen with the overlay up, against the screen before.
        screen("/tmp/check-overlay.png")
        # (Not the window's middle: on a small monitor the overlay's hint is over it.)
        places = {"in the window": (frame["x"] + frame["width"] / 4, frame["y"] + frame["height"] / 4),
                  "left of it": (frame["x"] - 30, frame["y"] + frame["height"] / 2),
                  "below it": (frame["x"] + frame["width"] / 2, frame["y"] + frame["height"] + 40)}
        shown = {}
        for name, (x, y) in places.items():
            was, now = on_screen("/tmp/check-before.png", x, y), on_screen("/tmp/check-overlay.png", x, y)
            shown[name] = (was, now, all(abs(channel * DIMMED - seen) <= 8 for channel, seen in zip(was, now)))
        check(f"{how}: the overlay shows what was on the monitor, dimmed, where it was", all(alike for _, _, alike in shown.values()) and is_colour(shown["in the window"][0], colour), shown)
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


STANDING = """(w => w && (r => ({title: w.get_title(), above: w.is_above(), everywhere: w.is_on_all_workspaces(), focused: w.has_focus(),
    skip: w.skip_taskbar, monitor: w.get_monitor(), x: r.x, y: r.y, width: r.width, height: r.height}))(w.get_frame_rect()))"""

def standing(title):
    return run(f"{STANDING}(global.get_window_actors().map(a => a.meta_window).find(w => w.get_title() === {title!r}))")

def focused():
    return run("(w => w ? w.get_title() : '')(global.display.focus_window)")

def own():
    """Heyo's own windows that a Wayland window could not be without the Shell: the pill, and the overlay's keyboard."""
    app = tool("window.py", "Notes", "check-own")
    wait(lambda: window("Notes"))
    time.sleep(0.8)
    check("an application has the keyboard", focused() == "Notes", focused())
    check("nothing of Heyo's is on the screen before it is asked for", not [title for title in run("global.get_window_actors().map(a => a.meta_window.get_title())") if (title or "").startswith("Heyo")])
    # The pill: a recording begins (with no speech engine here it says so, which is as good).
    heyo("--dictate")
    wait(lambda: standing("Heyo Dictation"))
    time.sleep(0.8)
    pill = standing("Heyo Dictation")
    check("the pill comes up", bool(pill))
    if pill:
        primary = monitors()[run("global.display.get_primary_monitor()")]
        where = (primary["x"] + (primary["width"] - pill["width"]) // 2, primary["y"] + primary["height"] - pill["height"] - primary["height"] // 10)
        check("the pill is at the bottom centre of the primary monitor, a tenth up", abs(pill["x"] - where[0]) <= 1 and abs(pill["y"] - where[1]) <= 1, f"{pill} and not at {where}")
        check("the pill is above the other windows, on every workspace", pill["above"] and pill["everywhere"], pill)
        check("the pill does not have the keyboard: the application has", not pill["focused"] and focused() == "Notes", focused())
        check("the pill is kept out of the lists", pill["skip"], pill)
        # A click on it must not take the keyboard either.
        point(pill["x"] + pill["width"] / 2, pill["y"] + pill["height"] / 2)
        subprocess.run([os.path.join(HERE, "input.py"), "seq", "nudge:3", "wait:0.3", "nudge:-3", "wait:0.3", "press:left", "wait:0.6"], check=True)
        check("... nor after a click on it", focused() == "Notes", focused())
    if not wait(lambda: not window("Heyo Dictation"), 8):
        heyo("--dictate")
    check("the pill is gone afterwards", bool(wait(lambda: not window("Heyo Dictation"), 10)), standing("Heyo Dictation"))
    check("... and the application still has the keyboard", focused() == "Notes", focused())
    # The overlay has the keyboard while it is up, and Escape gives it back.
    point(300, 300)
    heyo("--open", "heyo:screenshot")
    overlay = wait(lambda: window("Heyo Screenshot"))
    time.sleep(1.0)
    check("the overlay has the keyboard while it is up", focused() == "Heyo Screenshot", focused())
    subprocess.run([os.path.join(HERE, "input.py"), "key", "Escape"], check=True)
    check("Escape closes the overlay", bool(wait(lambda: not window("Heyo Screenshot"), 5)))
    time.sleep(0.4)
    check("... and the keyboard is back with the application", focused() == "Notes", focused())
    # Pinned captures: two pictures pinned when Heyo starts (HEYO_TEST_PIN),
    # at places given as a capture gives them, in the toolkit's pixels.
    subprocess.run(["env", "HEYO_TEST_PIN=200,160,300,200;800,160,200,120", os.path.join(HERE, "start-heyo.sh")], check=True, stdout=subprocess.DEVNULL)
    pins = lambda: run("""global.get_window_actors().map(a => a.meta_window).filter(w => w.get_title() === 'Heyo Pinned Screenshot')
        .map(w => (r => ({x: r.x, y: r.y, above: w.is_above(), skip: w.skip_taskbar}))(w.get_frame_rect()))""")
    wait(lambda: len(pins()) == 2, 12)
    time.sleep(1.0)
    first = monitors()[0]
    # How many of the toolkit's pixels one of the Shell's units is there.
    per = 1 if first["density"] == 1 else -(-first["scale"] // 1)
    places = sorted((pin["x"], pin["y"]) for pin in pins())
    expected = sorted((first["x"] + round(x / per), first["y"] + round(160 / per)) for x in (200, 800))
    check("two pinned captures are where they were read", len(places) == 2 and all(abs(a - b) <= 1 for place, wanted in zip(places, expected) for a, b in zip(place, wanted)), f"{places} and not {expected}")
    check("... above the other windows, and out of the lists", len(pins()) == 2 and all(pin["above"] and pin["skip"] for pin in pins()), pins())
    if places:
        # A click on one gives it the keyboard, and Escape closes it: the other stays where it is.
        point(places[0][0] + 20, places[0][1] + 20)
        subprocess.run([os.path.join(HERE, "input.py"), "seq", "nudge:3", "wait:0.3", "nudge:-3", "wait:0.3", "press:left", "wait:0.6", "tap:Escape"], check=True)
        check("Escape closes the one that was clicked", bool(wait(lambda: len(pins()) == 1, 5)), pins())
        left = [(pin["x"], pin["y"]) for pin in pins()]
        check("... and the other is still where it was read", len(left) == 1 and all(abs(a - b) <= 1 for a, b in zip(left[0], expected[1])), f"{left} and not {expected[1]}")
    # Heyo as it is started for the other checks.
    subprocess.run([os.path.join(HERE, "start-heyo.sh")], check=True, stdout=subprocess.DEVNULL)
    time.sleep(4)
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


# --- Pictures, and the controls of Slint's windows -------------------------------

SHOTS = "/tmp/shots"


def picture(name, title):
    """The window of that title as it is on the screen, saved as SHOTS/NAME.png to be looked at."""
    import gi
    gi.require_version("GdkPixbuf", "2.0")
    from gi.repository import GdkPixbuf
    frame = window(title)
    if not frame:
        return None
    os.makedirs(SHOTS, exist_ok=True)
    screen("/tmp/check-shot.png")
    image = GdkPixbuf.Pixbuf.new_from_file("/tmp/check-shot.png")
    per = image.get_width() / max(monitor["x"] + monitor["width"] for monitor in monitors())
    x, y = max(0, round(frame["x"] * per)), max(0, round(frame["y"] * per))
    width, height = min(image.get_width() - x, round(frame["width"] * per)), min(image.get_height() - y, round(frame["height"] * per))
    path = f"{SHOTS}/{name}.png"
    image.new_subpixbuf(x, y, width, height).savev(path, "png", [], [])
    return path


def units(title):
    """How many of the Shell's units one of the window's own pixels is (AT-SPI's), and where the window is."""
    frame = window(title)
    monitor = monitors()[frame["monitor"]]
    return 1 / monitor["density"], frame


def press_on(title, role, name, settle=0.6):
    """A click in the middle of a control of a Slint window, found by what it is and says. False if it is not there."""
    import atspi
    box = atspi.where(role, name, title)
    if not box or not window(title):
        return False
    per, frame = units(title)
    point(frame["x"] + (box[0] + box[2] / 2) * per, frame["y"] + (box[1] + box[3] / 2) * per)
    subprocess.run([os.path.join(HERE, "input.py"), "seq", "nudge:3", "wait:0.2", "nudge:-3", "wait:0.3", "press:left", f"wait:{settle}"], check=True)
    return True


def hover_on(title, role, name):
    import atspi
    box = atspi.where(role, name, title)
    if not box or not window(title):
        return False
    per, frame = units(title)
    point(frame["x"] + (box[0] + box[2] / 2) * per, frame["y"] + (box[1] + box[3] / 2) * per)
    subprocess.run([os.path.join(HERE, "input.py"), "seq", "nudge:2", "wait:0.2", "nudge:-2"], check=True)
    return True


def names(title):
    import atspi
    return [name for _, _, name, _ in atspi.tree(title)]


def stored(key):
    try:
        return json.load(open(os.path.expanduser("~/.config/heyo/settings.json"))).get(key)
    except (OSError, ValueError):
        return None


def restart(*settings, **environment):
    """Heyo started again with those settings (key=JSON value), as settings are read at start."""
    if settings:
        subprocess.run([os.path.join(HERE, "settings.py"), *settings], check=True, stdout=subprocess.DEVNULL)
    subprocess.run(["env", *[f"{key}={value}" for key, value in environment.items()], os.path.join(HERE, "start-heyo.sh")], check=True, stdout=subprocess.DEVNULL)
    time.sleep(4)


def language():
    """The interface in 简体中文, chosen in the settings while they are open, and in each of Heyo's windows."""
    colours = tool("colour.py", "A Colour", "check-language", "d81b60", 400, 300)
    wait(lambda: window("A Colour"))
    restart("hideOnBlur=false", 'language="system"')
    heyo("--open", "heyo:settings")
    check("the settings window comes up in English", bool(wait(lambda: window("Heyo Settings"), 8)))
    time.sleep(1.5)
    picture("language-settings-en", "Heyo Settings")
    check("its Language menu is found", press_on("Heyo Settings", "combo box", "Language", 1.0))
    picture("language-menu", "Heyo Settings")
    check("简体中文 is chosen from it", press_on("Heyo Settings", "list item", "简体中文", 1.5))
    chinese = wait(lambda: window("Heyo 设置"), 5)
    check("the open settings window speaks Chinese at once: its title", bool(chinese), [w["title"] for w in [window("Heyo Settings")] if w])
    check("... and the setting is kept", stored("language") == "zh_CN", stored("language"))
    if chinese:
        time.sleep(1)
        said = names("Heyo 设置")
        check("... its pages and rows are named in Chinese", all(word in said for word in ("通用", "语言", "快捷键")), said[:30])
        picture("language-settings-zh", "Heyo 设置")
        close("Heyo 设置")
        wait(lambda: not window("Heyo 设置"))
    # The palette, in the language chosen while it was hidden.
    heyo()
    check("the palette comes up", bool(wait(lambda: window("Heyo"))))
    time.sleep(1.5)
    picture("language-palette-zh", "Heyo")
    heyo()
    wait(lambda: not window("Heyo"))
    # The pill (no speech engine here unless one is lent: it says why in Chinese).
    heyo("--dictate")
    pill = wait(lambda: window("Heyo Dictation"), 8)
    check("the pill comes up", bool(pill))
    if pill:
        time.sleep(0.8)
        picture("language-pill-zh", "Heyo Dictation")
        said = names("Heyo Dictation")
        check("... its body is named in Chinese", "听写" in said, said)
        hover_on("Heyo Dictation", "button", "听写")
        time.sleep(0.8)
        picture("language-pill-hover-zh", "Heyo Dictation")
        check("... and its X too, once the pointer is on it", "取消" in names("Heyo Dictation"), names("Heyo Dictation"))
    if not wait(lambda: not window("Heyo Dictation"), 6):
        heyo("--dictate")
    wait(lambda: not window("Heyo Dictation"), 8)
    # The overlay: a window selected, the toolbar's tip under the pointer.
    frame = window("A Colour")
    point(frame["x"] + frame["width"] + 60, frame["y"] + frame["height"] + 60)
    heyo("--open", "heyo:screenshot")
    overlay = wait(lambda: window("Heyo Screenshot"))
    time.sleep(1.0)
    check("the overlay comes up", bool(overlay))
    if overlay:
        point(frame["x"] + frame["width"] / 2, frame["y"] + frame["height"] / 2)
        subprocess.run([os.path.join(HERE, "input.py"), "seq", "nudge:5", "wait:0.3", "nudge:-4", "wait:0.4", "press:left", "wait:0.8"], check=True)
        said = names("Heyo Screenshot")
        check("its toolbar's buttons are named in Chinese", all(word in said for word in ("工具", "矩形", "撤销", "保存")), said)
        # A tip shows once the pointer has rested on a button (an enabled one).
        for name, button in (("rectangle", "矩形"), ("save", "保存")):
            if hover_on("Heyo Screenshot", None, button):
                time.sleep(1.4)
                picture(f"language-overlay-tip-{name}-zh", "Heyo Screenshot")
        subprocess.run([os.path.join(HERE, "input.py"), "key", "Escape"], check=True)
        wait(lambda: not window("Heyo Screenshot"), 5)
    # Back to English, again while the settings are open.
    heyo("--open", "heyo:settings")
    wait(lambda: window("Heyo 设置"), 8)
    time.sleep(1.5)
    check("in Chinese the Language menu is 语言", press_on("Heyo 设置", "combo box", "语言", 1.0))
    press_on("Heyo 设置", "list item", "English", 1.5)
    check("English chosen again: the open window's title is English at once", bool(wait(lambda: window("Heyo Settings"), 5)))
    close("Heyo Settings")
    wait(lambda: not window("Heyo Settings"))
    colours.terminate()


def conversation():
    """The newest conversation AI Chat keeps: its messages' roles and contents."""
    folder = os.path.expanduser("~/.local/share/heyo/support/@heyo/conversations")
    try:
        files = sorted(glob.glob(os.path.join(folder, "*")), key=os.path.getmtime)
        return json.load(open(files[-1])).get("messages", []) if files else []
    except (OSError, ValueError):
        return []


STUB_PORT, STUB_LOG = 18111, "/tmp/stub.log"


def stub():
    """The model that is not one (../../gui-test/llm-stub.py), on this session's 127.0.0.1, and the settings that ask it."""
    if os.path.exists(STUB_LOG):
        os.remove(STUB_LOG)
    server = subprocess.Popen(["python3", "/heyo/scripts/gui-test/llm-stub.py", str(STUB_PORT), STUB_LOG])
    time.sleep(0.5)
    return server, ("aiProvider=\"local\"", 'anthropicApiKey=""', f'localModelUrl="http://127.0.0.1:{STUB_PORT}/v1"', 'localModelName="stub-model"')


def stub_said():
    try:
        return open(STUB_LOG).read().splitlines()
    except OSError:
        return []


def ai():
    """AI Chat answered by a local model through the host: a question, a follow-up, an answer stopped midway, and Escape."""
    server, settings = stub()
    restart("hideOnBlur=false", *settings)
    keys = lambda *k: subprocess.run([os.path.join(HERE, "input.py"), "key", *k], check=True)
    typed = lambda text: subprocess.run([os.path.join(HERE, "input.py"), "type", text], check=True)
    heyo("--open", "cmd:@heyo:ask-ai")
    wait(lambda: window("Heyo"))
    time.sleep(2.5)
    typed("What is a launcher?")
    time.sleep(0.4)
    keys("ctrl+Return")
    check("the question reaches the model", bool(wait(lambda: any("What is a launcher?" in line for line in stub_said()), 8)), stub_said())
    time.sleep(1.2)
    picture("ai-streaming", "Heyo")
    check("... which answers it all", bool(wait(lambda: "answered" in stub_said(), 15)), stub_said())
    time.sleep(1.0)
    picture("ai-answered", "Heyo")
    kept = conversation()
    check("the answer is kept with the question", [m.get("role") for m in kept] == ["user", "assistant"] and "You asked: “What is a launcher?”" in kept[-1].get("content", ""), kept)
    # Continued in AI Chat, with a follow-up.
    keys("Return")
    time.sleep(2.5)
    typed("And a palette?")
    time.sleep(0.4)
    keys("Return")
    follow = wait(lambda: next((line for line in stub_said() if "And a palette?" in line), None), 8)
    check("a follow-up reaches the model with the conversation before it", bool(follow) and "after 4 messages" in follow, follow or stub_said())
    check("... and is answered", bool(wait(lambda: stub_said().count("answered") == 2, 15)), stub_said())
    time.sleep(1.0)
    picture("ai-followup", "Heyo")
    check("both exchanges are kept", [m.get("role") for m in conversation()] == ["user", "assistant", "user", "assistant"], [m.get("role") for m in conversation()])
    # Stopped midway (Stop Answering, Ctrl+Shift+S): the connection is closed.
    typed("Tell me slowly")
    time.sleep(0.4)
    keys("Return")
    wait(lambda: any("Tell me slowly" in line for line in stub_said()), 8)
    time.sleep(2.5)
    keys("ctrl+shift+s")
    closed = wait(lambda: any(line.startswith("closed after") for line in stub_said()), 6)
    check("Stop Answering closes the model's connection midway", bool(closed), stub_said()[-2:])
    time.sleep(1.0)
    picture("ai-stopped", "Heyo")
    last = conversation()[-1:] or [{}]
    check("... and what had come is kept, marked as cut short", last[0].get("role") == "assistant" and "Tell me slowly" in last[0].get("content", ""), last)
    # Escape in the middle of an answer only leaves the view: the answer
    # goes on being written (runtime/src/builtins/answering.ts), and the
    # palette put away does not stop it either.
    before = len(stub_said())
    typed("Once more slowly")
    time.sleep(0.4)
    keys("Return")
    wait(lambda: any("Once more slowly" in line for line in stub_said()), 8)
    time.sleep(2.0)
    keys("Escape")
    time.sleep(3.0)
    after = stub_said()[before:]
    check("Escape midway leaves the answer being written (as answering.ts means it to)", not any(line.startswith("closed") for line in after), after)
    if window("Heyo"):
        heyo()
    # Heyo ended in the middle of an answer: nothing is left asking.
    restart()
    check("... and Heyo ended in the middle of it closes the connection", bool(wait(lambda: any(line.startswith("closed") for line in stub_said()[before:]), 8)), stub_said()[before:])
    server.terminate()


SPOKEN = "/heyo/src-tauri/target/wayland-test/spoken.wav"
# What spoken.wav says (test.sh makes it with ../../gui-test/say.py).
SAID = ["The weather is nice today, so let us walk to the park.", "See you at three thirty."]


def words(text):
    return [word.strip(".,“”").lower() for word in text.split() if word.strip(".,“”")]


def dictate():
    """Dictation into another application, recognized from a recording and polished by the stub."""
    if not os.path.exists(SPOKEN) or not os.path.isdir(os.path.expanduser("~/.local/share/heyo/dictation/models")):
        check("dictation: a recording and the speech engine are lent to the session (test.sh)", False, "none")
        return
    server, settings = stub()
    app = tool("window.py", "Notes", "check-dictate", "Notes: ")
    wait(lambda: window("Notes"))
    for how, live, polish in (("sentences", "false", "off"), ("live", "true", "off"), ("polished", "true", "clean")):
        restart("hideOnBlur=false", f"dictationLive={live}", f'dictationPolish="{polish}"', 'dictationLanguage="en"', 'dictationHoldKey=""', *settings, HEYO_DICTATION_WAV=SPOKEN)
        os.kill(int(open("/tmp/window-check-dictate.pid.txt").read()), 10)
        run("Main.activateWindow(global.get_window_actors().map(a => a.meta_window).find(w => w.get_title() === 'Notes')) ?? null")
        time.sleep(0.8)
        seen, start = [], time.time()
        heyo("--dictate")
        # The recording is read at the speed it was spoken; it ends by itself or by the second --dictate.
        while time.time() - start < 14:
            text = open("/tmp/window-check-dictate.txt").read()
            if not seen or seen[-1][1] != text:
                seen.append((round(time.time() - start, 1), text))
            time.sleep(0.1)
        heyo("--dictate")
        wait(lambda: not window("Heyo Dictation"), 30)
        time.sleep(2.0)
        text = open("/tmp/window-check-dictate.txt").read()
        print(f"     ({how}) the field went {[t for _, t in seen]} and ends as {text!r}")
        heard = words(text[len("Notes: "):])
        expected = words(" ".join(SAID))
        alike = sum(1 for word in expected if word in heard)
        check(f"dictation ({how}): the words reach the focused application", text.startswith("Notes: ") and alike >= len(expected) * 0.7, repr(text))
        if polish == "off":
            check("... while it is spoken, not all at the end", len({t for _, t in seen}) >= 3, [t for _, t in seen])
        else:
            check("... replaced by what the model made of them", "You asked: “" in text, repr(text))
        check(f"dictation ({how}): the application kept the keyboard", focused() == "Notes", focused())
    app.terminate()
    server.terminate()


def placement():
    """With the pointer on each monitor: the palette in the middle of that one, the settings window on it."""
    all_monitors = monitors()
    for index, monitor in enumerate(all_monitors):
        how = f"monitor {index} (scale {monitor['scale']:g})"
        point(monitor["x"] + monitor["width"] / 3, monitor["y"] + monitor["height"] / 3)
        heyo()
        palette = wait(lambda: window("Heyo"))
        time.sleep(1.0)
        palette = window("Heyo")
        check(f"{how}: the palette comes up", bool(palette))
        if palette:
            middle = palette["x"] + palette["width"] / 2
            check(f"{how}: the palette is on the monitor the pointer is on, in its middle", palette["monitor"] == index and abs(middle - (monitor["x"] + monitor["width"] / 2)) <= 2, f"{palette} on {monitor}")
            picture(f"palette-monitor-{index}", "Heyo")
            heyo()
            wait(lambda: not window("Heyo"))
        heyo("--open", "heyo:settings")
        settings = wait(lambda: window("Heyo Settings"), 8)
        time.sleep(0.8)
        settings = window("Heyo Settings")
        check(f"{how}: the settings window comes up on the monitor the pointer is on", bool(settings) and settings["monitor"] == index, settings)
        close("Heyo Settings")
        wait(lambda: not window("Heyo Settings"))


def cold():
    """The first of Slint's windows with nothing kept of drawing before (Mesa's shader cache emptied, as after
    installing or updating the driver): it comes up, though the first picture takes Slint's thread a while."""
    for name in ("mesa_shader_cache", "mesa_shader_cache_db"):
        subprocess.run(["rm", "-rf", os.path.expanduser(f"~/.cache/{name}")])
    restart("hideOnBlur=false", 'language="system"')
    heyo("--open", "heyo:settings")
    up = wait(lambda: window("Heyo Settings"), 10)
    time.sleep(2.5)
    stalled = "does not answer" in open("/tmp/heyo.log").read()
    check("with a cold shader cache, the settings window comes up and stays", bool(up) and bool(window("Heyo Settings")) and not stalled,
          "the watcher took it for stalled" if stalled else "it did not come up")
    close("Heyo Settings")
    wait(lambda: not window("Heyo Settings"))


def settings_window():
    """The settings window as a window: moved by its top edge, a click after the move, closed, open again."""
    import atspi
    restart("hideOnBlur=false", 'language="system"')
    heyo("--open", "heyo:settings")
    first = wait(lambda: window("Heyo Settings"), 8)
    time.sleep(1.5)
    check("the settings window comes up", bool(first))
    if not first:
        return
    per, frame = units("Heyo Settings")
    # By the page's top edge: the band its close button is in, left of the button.
    close_button = atspi.where("button", "Close", "Heyo Settings")
    check("its close button is found", bool(close_button))
    if not close_button:
        return
    point(frame["x"] + (close_button[0] - 250) * per, frame["y"] + (close_button[1] + close_button[3] / 2) * per)
    subprocess.run([os.path.join(HERE, "input.py"), "seq", "nudge:2", "wait:0.2", "nudge:-2", "wait:0.2", "hold:left", "wait:0.2", "by:120,80", "wait:0.3", "let:left", "wait:0.8"], check=True)
    moved = window("Heyo Settings")
    check("moved by its top edge, it goes as far as the pointer went", abs(moved["x"] - frame["x"] - 120) <= 4 and abs(moved["y"] - frame["y"] - 80) <= 4, f"from {frame} to {moved}")
    # The desktop had the press: a click in the window must still be one.
    press_on("Heyo Settings", "list item", "Shortcuts", 0.8)
    check("a click after the move is a click: Shortcuts is the page shown", "selected" in atspi.states("list item", "Shortcuts", "Heyo Settings"), atspi.states("list item", "Shortcuts", "Heyo Settings"))
    press_on("Heyo Settings", "list item", "General", 0.8)
    check("... and again: General", "selected" in atspi.states("list item", "General", "Heyo Settings"), atspi.states("list item", "General", "Heyo Settings"))
    picture("settings-moved", "Heyo Settings")
    # Round its panel the window keeps a margin for its shadow, where what
    # is behind it shows (when it is drawn through OpenGL, as here): a
    # press there is the window's underneath.
    # The sidebar's first entry is 20 of the window's logical pixels in from the panel's edge (settings.slint).
    general = atspi.where("list item", "General", "Heyo Settings")
    per, frame = units("Heyo Settings")
    margin = round(general[0] * per - 20) if general else 0
    if margin > 4:
        under = tool("window.py", "Under", "check-under")
        wait(lambda: window("Under"))
        time.sleep(0.8)
        place("Under", frame["monitor"], frame["x"] - 300, frame["y"] + 100)
        heyo("--open", "heyo:settings")
        wait(lambda: focused() == "Heyo Settings", 3)
        point(frame["x"] + margin / 2, frame["y"] + frame["height"] / 2)
        subprocess.run([os.path.join(HERE, "input.py"), "seq", "nudge:2", "wait:0.2", "nudge:-2", "wait:0.2", "press:left", "wait:0.8"], check=True)
        check(f"a click in the margin round the panel ({margin} units) goes to the window underneath", focused() == "Under", f"{focused()} has it")
        under.terminate()
        wait(lambda: not window("Under"))
        heyo("--open", "heyo:settings")
        wait(lambda: focused() == "Heyo Settings", 3)
    else:
        print("     (the settings window keeps no margin here: drawn on the CPU, or no room for one)")
    # Its own close button.
    press_on("Heyo Settings", "button", "Close", 0.8)
    check("its close button closes it", bool(wait(lambda: not window("Heyo Settings"), 5)))
    check("... and Heyo goes on", subprocess.run(["kill", "-0", open("/tmp/heyo.pid").read().strip()]).returncode == 0)
    heyo("--open", "heyo:settings")
    again = wait(lambda: window("Heyo Settings"), 8)
    check("asked for again, it comes back, with the keyboard", bool(again) and bool(wait(lambda: focused() == "Heyo Settings", 4)), focused())
    time.sleep(1.0)
    check("... and a click in it works", press_on("Heyo Settings", "list item", "Shortcuts", 0.8) and "selected" in atspi.states("list item", "Shortcuts", "Heyo Settings"))
    subprocess.run([os.path.join(HERE, "input.py"), "key", "Escape"], check=True)
    check("Esc closes it", bool(wait(lambda: not window("Heyo Settings"), 5)))


def ime():
    """Chinese typed into the settings window's search field (in Chinese) through IBus, as GNOME runs it."""
    import atspi
    subprocess.run(["gsettings", "set", "org.gnome.desktop.input-sources", "sources", "[('xkb', 'us'), ('ibus', 'libpinyin')]"], check=True)
    time.sleep(2)
    engine = lambda: subprocess.run(["ibus", "engine"], capture_output=True, text=True).stdout.strip()
    # A pointer that stays (input.py stay): winit lets go of a window's
    # text input when the seat loses its pointer, which each input.py
    # call would otherwise do here as it ends.
    stay = subprocess.Popen([os.path.join(HERE, "input.py"), "stay"], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
    stay.stdout.readline()
    restart("hideOnBlur=false", 'language="zh_CN"')
    heyo("--open", "heyo:settings")
    wait(lambda: window("Heyo 设置"), 8)
    time.sleep(1.5)
    check("the search field is found", press_on("Heyo 设置", "text", "搜索设置", 0.5) or press_on("Heyo 设置", "entry", "搜索设置", 0.5),
          [(role, name) for _, role, name, _ in atspi.tree("Heyo 设置")][:12])
    if engine() != "libpinyin":
        subprocess.run([os.path.join(HERE, "input.py"), "key", "super+space"], check=True)
        time.sleep(1.5)
    check("Pinyin is the input method in use", bool(wait(lambda: engine() == "libpinyin", 4)), engine())
    subprocess.run([os.path.join(HERE, "input.py"), "type", "yuyan"], check=True)
    time.sleep(1.0)
    picture("ime-composing", "Heyo 设置")
    subprocess.run([os.path.join(HERE, "input.py"), "key", "space"], check=True)
    time.sleep(1.2)
    picture("ime-committed", "Heyo 设置")
    field = atspi.text("text", "搜索设置", "Heyo 设置") or atspi.text("entry", "搜索设置", "Heyo 设置")
    said = names("Heyo 设置")
    check("what Pinyin composed is committed into the field", bool(field) and "语言" in field, repr(field))
    check("... and searched for: of the pages, General is left, which has Language on it", "通用" in said and "快捷键" not in said and "听写" not in said, said[:12])
    subprocess.run([os.path.join(HERE, "input.py"), "key", "super+space"], check=True)
    subprocess.run(["gsettings", "reset", "org.gnome.desktop.input-sources", "sources"], check=True)
    close("Heyo 设置")
    wait(lambda: not window("Heyo 设置"))
    stay.stdin.close()
    stay.wait()


def shell_extension():
    """Which version of Heyo's extension the Shell loaded."""
    return run("Main.extensionManager.lookup('heyo@heyo.app')?.metadata?.version ?? null")


def open_settings(title="Heyo Settings"):
    """The settings window, asked for once more if it did not come: the first of Slint's windows in a new
    session may be taken for stalled while its first picture is drawn (see `cold`), which is not what
    the checks that use this are about."""
    for _ in range(2):
        heyo("--open", "heyo:settings")
        wait(lambda: window(title), 8)
        # Taken off again, if so, two seconds after it came.
        time.sleep(3)
        if window(title):
            return window(title)
    return None


def warm():
    """Not a check: one of Slint's windows drawn once, so that the first picture of the session (`cold`) is
    not what the checks after this one meet."""
    if open_settings():
        close("Heyo Settings")
        wait(lambda: not window("Heyo Settings"))


def upgrade():
    """Heyo with an older extension running (run.sh start old, at version 3): Settings offers the update, which waits for the next login."""
    check("the session runs the older extension", shell_extension() == 3, shell_extension())
    restart("hideOnBlur=false", 'language="system"')
    open_settings()
    picture("upgrade-offered", "Heyo Settings")
    check("Settings → General offers to update it", press_on("Heyo Settings", "button", "Update", 1.5), names("Heyo Settings")[:30])
    on_disk = open(os.path.expanduser("~/.local/share/gnome-shell/extensions/heyo@heyo.app/extension.js")).read()
    check("... which puts the new one in place", "const API = 4;" in on_disk)
    time.sleep(1.0)
    picture("upgrade-pending", "Heyo Settings")
    check("... and says to log out and back in", "Update" not in names("Heyo Settings"), names("Heyo Settings")[:30])
    check("the older one keeps serving meanwhile", shell_extension() == 3)
    close("Heyo Settings")
    wait(lambda: not window("Heyo Settings"))
    # It still serves: the pill is above, without the keyboard, as version 3 does it.
    app = tool("window.py", "Notes", "check-upgrade")
    wait(lambda: window("Notes"))
    time.sleep(0.8)
    heyo("--dictate")
    pill = wait(lambda: standing("Heyo Dictation"), 8)
    time.sleep(0.8)
    pill = standing("Heyo Dictation")
    check("... the pill is above the windows and leaves the keyboard alone", bool(pill) and pill["above"] and focused() == "Notes", (pill, focused()))
    if not wait(lambda: not window("Heyo Dictation"), 8):
        heyo("--dictate")
    wait(lambda: not window("Heyo Dictation"), 8)
    app.terminate()


def upgraded():
    """After the next login (run.sh relogin): the new extension runs, and Settings has nothing more to say about it."""
    check("after logging in again the Shell runs version 4", shell_extension() == 4, shell_extension())
    restart("hideOnBlur=false")
    open_settings()
    said = names("Heyo Settings")
    check("... and Settings → General no longer speaks of the extension", "GNOME Shell Extension" not in said, said[:30])
    picture("upgrade-done", "Heyo Settings")
    close("Heyo Settings")
    wait(lambda: not window("Heyo Settings"))


CHECKS = {"lists": lists, "layout": layout, "capture": capture, "relock": relock, "own": own, "language": language, "ai": ai, "dictate": dictate,
          "placement": placement, "settings": settings_window, "cold": cold, "warm": warm, "ime": ime, "upgrade": upgrade, "upgraded": upgraded}
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

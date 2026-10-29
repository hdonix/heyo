#!/usr/bin/env python3
"""Inside the Sway session, with Heyo running (start-heyo.sh): what
check-sway.py does not look at, each as a part that can be run alone.

  check-sway-more.py [PART…]      all of them, or those named:

  language   简体中文 chosen in the settings window while it is open (its
             title then changes at once), and the palette, the pill and the
             overlay's toolbar tips pictured in it; English again the same way
             (at scale 1: at 2 the rows of a page do not say rightly where
             they are on the accessibility bus, see `per`)
  ai         AI Chat answered by a stub model through the host
             (../../gui-test/llm-stub.py on this session's 127.0.0.1): a
             question, a follow-up with the conversation before it, Stop
             Answering closing the connection midway, and Escape, which only
             leaves the view
  dictate    a recording (HEYO_DICTATION_WAV, made by sway.sh with
             ../../gui-test/say.py) dictated with the speech engine lent to
             the session, polished by the stub: what reaches the window that
             has the keyboard
  settings   the settings window, a floating window here: its panel and
             margin at the output's scale, moved by its top edge, a click after the move, a press in the margin round its
             panel going to the window underneath, its close button, asked
             for again, Esc
  ime        Chinese typed into its search field through an input method
             (ime.py), in Chinese
  outputs    a second output beside the first: the palette and the settings
             window on the one the pointer is on, the settings window its
             panel and margin there and when moved to the other, the pill on
             the first

The controls of the settings window are found by asking the window
(atspi.py); the pill and the overlay are layer surfaces, which are not on
the accessibility bus, and are pictured where check-sway.py finds them.
Pictures are kept in /tmp/shots/ to be looked at (sway.sh copies them out).
"""
import glob, json, os, signal, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from swaybed import HERE, application, at, check, click, failed, focused, height, heyo, keys, left, output, outputs, picture, pill_there, point, pointer, press, release, scale, sway, top, typed, wait, width, windows
import atspi

SHOTS = "/tmp/shots"
SPOKEN = "/heyo/src-tauri/target/wayland-test/spoken.wav"
S = int(scale)


def window(title):
    """A window of Sway's tree by its title: its rectangle in logical pixels, and whether it has the keyboard."""
    return next((dict(found["rect"], focused=found.get("focused"), output=on_output(found)) for found in windows() if found.get("name") == title), None)


def on_output(node):
    rect = node["rect"]
    middle = (rect["x"] + rect["width"] / 2, rect["y"] + rect["height"] / 2)
    return next((one["name"] for one in outputs() if one["rect"]["x"] <= middle[0] < one["rect"]["x"] + one["rect"]["width"] and one["rect"]["y"] <= middle[1] < one["rect"]["y"] + one["rect"]["height"]), None)


def shot(name, rect):
    """A part of the screen, in logical pixels, saved as SHOTS/NAME.png."""
    os.makedirs(SHOTS, exist_ok=True)
    geometry = f"{int(rect['x'])},{int(rect['y'])} {int(rect['width'])}x{int(rect['height'])}"
    subprocess.run(["grim", "-g", geometry, f"{SHOTS}/{name}.png"], check=False)


def picture_of(name, title):
    found = window(title)
    if found:
        shot(name, found)


def per(title, frame, box=None):
    """How many logical pixels one of the window's own (AT-SPI's) is, for a control at `box`.

    What the window says is not all in one kind of pixel: at scale 2 the
    window and its sidebar say where they are in logical pixels and the
    rows of a page in the output's (seen with Slint 1.18 here). So a box
    that reaches beyond the window as it says it is taken for the output's
    pixels."""
    whole = atspi.where("frame", title, title)
    if not whole:
        return 1 / scale
    if whole[2] > frame["width"] * 1.5:
        return 1 / scale
    if box and (box[0] + box[2] > whole[2] + 1 or box[1] + box[3] > whole[3] + 1):
        return 1 / scale
    return 1


def press_on(title, role, name, settle=0.6):
    """A click in the middle of a control of one of Heyo's Slint windows (a floating one), found by what it is and says."""
    box, frame = atspi.where(role, name, title), window(title)
    if not box or not frame:
        return False
    by = per(title, frame, box)
    point(frame["x"] + (box[0] + box[2] / 2) * by, frame["y"] + (box[1] + box[3] / 2) * by)
    click()
    time.sleep(settle)
    return True


def names(title):
    return [name for _, _, name, _ in atspi.tree(title)]


def stored(key):
    try:
        return json.load(open(os.path.expanduser("~/.config/heyo/settings.json"))).get(key)
    except (OSError, ValueError):
        return None


def restart(*settings, **environment):
    if settings:
        subprocess.run([os.path.join(HERE, "settings.py"), *settings], check=True, stdout=subprocess.DEVNULL)
    subprocess.run(["env", *[f"{key}={value}" for key, value in environment.items()], os.path.join(HERE, "start-heyo.sh")], check=True, stdout=subprocess.DEVNULL)
    time.sleep(5)


def open_settings(title="Heyo Settings"):
    """The settings window, asked for once more if it did not stay: the first of Slint's windows in a
    session may be taken for stalled while its first picture is drawn (check.py's `cold`)."""
    for _ in range(2):
        heyo("--open", "heyo:settings")
        wait(lambda: window(title), 8)
        time.sleep(3)
        if window(title):
            return window(title)
    return None


def fits(found):
    """Whether the settings window is exactly its panel and its shadow's margin for the output it is on."""
    there = next((one for one in outputs() if one["name"] == found["output"]), None)
    if not there:
        return False
    room = (there["rect"]["width"], there["rect"]["height"])
    panel = (min(860, room[0]), min(640, room[1]))
    # The shadow's margin (settings.slint), where there is room for it and what is behind can show.
    margins = [0] + ([32] if room[0] >= 924 and room[1] >= 704 else [])
    return any(abs(found["width"] - panel[0] - 2 * margin) <= 1 and abs(found["height"] - panel[1] - 2 * margin) <= 1 for margin in margins)


def close_settings(title):
    sway(f'[title="^{title}$"]', "kill")
    wait(lambda: not window(title), 5)


def language():
    # Something light behind the pill, which is dark; and a recording for
    # it to listen to, where there is one, so that it stays up and says so.
    back = application("Behind", "check-sway-behind")
    restart("hideOnBlur=false", 'language="system"', **({"HEYO_DICTATION_WAV": SPOKEN} if os.path.exists(SPOKEN) else {}))
    check("the settings window comes up in English", bool(open_settings()))
    shot("language-settings-en", window("Heyo Settings") or output["rect"])
    check("its Language menu is found", press_on("Heyo Settings", "combo box", "Language", 1.0))
    check("简体中文 is chosen from it", press_on("Heyo Settings", "list item", "简体中文", 1.5))
    check("the open settings window speaks Chinese at once: its title", bool(wait(lambda: window("Heyo 设置"), 5)), [found.get("name") for found in windows()])
    check("... and the setting is kept", stored("language") == "zh_CN", stored("language"))
    said = names("Heyo 设置")
    check("... its pages and rows are named in Chinese", all(word in said for word in ("通用", "语言", "快捷键")), said[:20])
    picture_of("language-settings-zh", "Heyo 设置")
    close_settings("Heyo 设置")
    # The palette.
    heyo()
    check("the palette comes up", bool(wait(lambda: window("Heyo"), 6)))
    time.sleep(1.5)
    picture_of("language-palette-zh", "Heyo")
    heyo()
    wait(lambda: not window("Heyo"), 4)
    # The pill, a layer surface.
    heyo("--dictate")
    check("the pill comes up", bool(wait(lambda: pill_there()[0], 6)), pill_there()[1])
    time.sleep(0.6)
    shot("language-pill-zh", {"x": left, "y": top, "width": 360, "height": 44})
    point(left + 180, top + 22)
    time.sleep(0.8)
    shot("language-pill-hover-zh", {"x": left, "y": top, "width": 360, "height": 44})
    if not wait(lambda: not pill_there()[0], 6):
        heyo("--dictate")
    wait(lambda: not pill_there()[0], 8)
    # The overlay, a part selected, the pointer resting on a tool and on Save.
    point(width * 3 // 4, height // 3)
    heyo("--open", "heyo:screenshot")
    time.sleep(1.5)
    x0, y0, x1, y1 = int(width * 0.3), int(height * 0.2), int(width * 0.6), int(height * 0.5)
    point(x0, y0)
    press()
    for step in range(1, 9):
        point(x0 + (x1 - x0) * step // 8, y0 + (y1 - y0) * step // 8, 0.05)
    release()
    time.sleep(0.6)
    # The toolbar's buttons, as check-sway.py has them: its right end at the
    # selection's, 8 below it, a button every 32 from 20 in; Save third from the right.
    toolbar_left = max(8, x1 - 499)
    for name, x in (("rectangle", toolbar_left + 20), ("save", x1 - 20 - 32 * 2)):
        point(x, y1 + 8 + 20)
        time.sleep(1.4)
        shot(f"language-overlay-tip-{name}-zh", {"x": toolbar_left - 20, "y": y1 - 10, "width": 540, "height": 110})
    keys("-k", "Escape")
    time.sleep(1.0)
    # And back to English, while the settings are open: at scale 1 only,
    # where the controls of a page say rightly where they are (see `per`).
    if scale != 1:
        back.terminate()
        restart('language="system"')
        return
    check("in Chinese the Language menu is 语言", bool(open_settings("Heyo 设置")) and press_on("Heyo 设置", "combo box", "语言", 1.0))
    press_on("Heyo 设置", "list item", "English", 1.5)
    check("English chosen again (by that name): the open window's title is English at once", bool(wait(lambda: window("Heyo Settings"), 5)))
    close_settings("Heyo Settings")
    back.terminate()


STUB_PORT, STUB_LOG = 18111, "/tmp/stub.log"


def stub():
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


def conversation():
    folder = os.path.expanduser("~/.local/share/heyo/support/@heyo/conversations")
    try:
        files = sorted(glob.glob(os.path.join(folder, "*")), key=os.path.getmtime)
        return json.load(open(files[-1])).get("messages", []) if files else []
    except (OSError, ValueError):
        return []


def ai():
    server, settings = stub()
    restart("hideOnBlur=false", *settings)
    heyo("--open", "cmd:@heyo:ask-ai")
    check("Ask AI comes up with the keyboard", bool(wait(lambda: (window("Heyo") or {}).get("focused"), 6)), window("Heyo"))
    time.sleep(2.0)
    keys("What is a launcher?")
    keys("-M", "ctrl", "-k", "Return", "-m", "ctrl")
    check("the question reaches the model", bool(wait(lambda: any("What is a launcher?" in line for line in stub_said()), 8)), stub_said())
    check("... which answers it all", bool(wait(lambda: "answered" in stub_said(), 15)), stub_said())
    time.sleep(1.0)
    picture_of("ai-answered", "Heyo")
    kept = conversation()
    check("the answer is kept with the question", [m.get("role") for m in kept] == ["user", "assistant"] and "You asked: “What is a launcher?”" in kept[-1].get("content", ""), kept)
    keys("-k", "Return")
    time.sleep(2.5)
    keys("And a palette?")
    keys("-k", "Return")
    follow = wait(lambda: next((line for line in stub_said() if "And a palette?" in line), None), 8)
    check("a follow-up reaches the model with the conversation before it", bool(follow) and "after 4 messages" in follow, follow or stub_said())
    check("... and is answered", bool(wait(lambda: stub_said().count("answered") == 2, 15)), stub_said())
    # Stopped midway, from the actions (Ctrl+K) of a question asked
    # anew: Stop Answering's shortcut, Ctrl+Shift+S, cannot be typed with
    # wtype, whose keyboard has no shifted level for a letter, and in AI
    # Chat's reply bar Ctrl+K typed with wtype does not arrive here either
    # (it does under GNOME: check.py).
    heyo()
    wait(lambda: not window("Heyo"), 4)
    heyo("--open", "cmd:@heyo:ask-ai")
    wait(lambda: (window("Heyo") or {}).get("focused"), 6)
    time.sleep(2.0)
    keys("Tell me slowly")
    keys("-M", "ctrl", "-k", "Return", "-m", "ctrl")
    wait(lambda: any("Tell me slowly" in line for line in stub_said()), 8)
    time.sleep(2.5)
    keys("-M", "ctrl", "-k", "k", "-m", "ctrl")
    keys("Stop Answering")
    keys("-k", "Return")
    check("Stop Answering closes the model's connection midway", bool(wait(lambda: any(line.startswith("closed after") for line in stub_said()), 6)), stub_said()[-2:])
    time.sleep(1.0)
    picture_of("ai-stopped", "Heyo")
    # Escape in the middle of an answer only leaves the view.
    heyo()
    wait(lambda: not window("Heyo"), 4)
    heyo("--open", "cmd:@heyo:ask-ai")
    wait(lambda: (window("Heyo") or {}).get("focused"), 6)
    time.sleep(2.0)
    before = len(stub_said())
    keys("Once more slowly")
    keys("-M", "ctrl", "-k", "Return", "-m", "ctrl")
    wait(lambda: any("Once more slowly" in line for line in stub_said()), 8)
    time.sleep(2.0)
    keys("-k", "Escape")
    time.sleep(3.0)
    check("Escape midway leaves the answer being written (as answering.ts means it to)", not any(line.startswith("closed") for line in stub_said()[before:]), stub_said()[before:])
    if window("Heyo"):
        heyo()
    restart()
    server.terminate()


SAID = "The weather is nice today, so let us walk to the park. See you at three thirty."


def words(text):
    return [word.strip(".,“”").lower() for word in text.split() if word.strip(".,“”")]


def dictate():
    if not os.path.exists(SPOKEN) or not os.path.isdir(os.path.expanduser("~/.local/share/heyo/dictation/models")) and not os.path.isdir("/speech/models"):
        check("dictation: a recording and the speech engine are lent to the session (sway.sh)", False, "none")
        return
    server, settings = stub()
    notes = subprocess.Popen([os.path.join(HERE, "window.py"), "Notes", "check-sway-notes", "Notes: "], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    wait(lambda: window("Notes"))
    time.sleep(0.8)
    for how, live, polish in (("sentences", "false", "off"), ("live", "true", "off"), ("polished", "true", "clean")):
        restart("hideOnBlur=false", f"dictationLive={live}", f'dictationPolish="{polish}"', 'dictationLanguage="en"', 'dictationHoldKey=""', *settings, HEYO_DICTATION_WAV=SPOKEN)
        point(width // 2, height // 3)
        click()
        os.kill(int(open("/tmp/window-check-sway-notes.pid.txt").read()), signal.SIGUSR1)
        subprocess.run(["wl-copy", "--clear"])
        time.sleep(0.5)
        seen, start = [], time.time()
        heyo("--dictate")
        while time.time() - start < 14:
            text = typed("check-sway-notes")
            if not seen or seen[-1] != text:
                seen.append(text)
            time.sleep(0.1)
        heyo("--dictate")
        wait(lambda: not pill_there()[0], 30)
        time.sleep(2.0)
        text = typed("check-sway-notes")
        copied = subprocess.run(["wl-paste", "-n"], capture_output=True, text=True).stdout
        print(f"     ({how}) the field went {seen} and ends as {text!r}; the clipboard has {copied[-80:]!r}", flush=True)
        # Here the words go by the clipboard and Ctrl+V typed with wtype
        # (platform/input.rs). In this bed no GTK window takes a paste at
        # all (wl-copy and wtype's Ctrl+V do not either), so what is
        # checked is what Heyo put on the clipboard to be pasted.
        heard = words(copied)
        alike = sum(1 for word in words(SAID) if word in heard)
        if polish == "clean":
            check(f"dictation ({how}): what the model made of the words is put on the clipboard to be pasted", "You asked: “" in copied and alike >= len(words(SAID)) * 0.7, repr(copied[-200:]))
        else:
            # (The recognizer hears "See you at three thirty" as "See who had 3,30".)
            check(f"dictation ({how}): the last sentence is put on the clipboard to be pasted", "see" in heard, repr(copied))
        check(f"dictation ({how}): the window that had the keyboard kept it", focused() == "Notes", focused())
    notes.terminate()
    server.terminate()


def settings():
    restart("hideOnBlur=false", 'language="system"')
    first = open_settings()
    check("the settings window comes up, floating, with the keyboard", bool(first) and first["focused"], first)
    if not first:
        return
    check(f"it is its panel and margin on an output at scale {scale:g}", fits(first), f"{first} on {output['rect']}")
    close_button = atspi.where("button", "Close", "Heyo Settings")
    check("its close button is found", bool(close_button))
    if not close_button:
        return
    # By the page's top edge, left of the close button.
    by = per("Heyo Settings", first, close_button)
    point(first["x"] + (close_button[0] - 250) * by, first["y"] + (close_button[1] + close_button[3] / 2) * by)
    press()
    for step in range(1, 9):
        point(first["x"] + (close_button[0] - 250) * by + 15 * step, first["y"] + (close_button[1] + close_button[3] / 2) * by + 10 * step, 0.05)
    release()
    time.sleep(0.8)
    moved = window("Heyo Settings")
    check("moved by its top edge, it goes as far as the pointer went", abs(moved["x"] - first["x"] - 120) <= 4 and abs(moved["y"] - first["y"] - 80) <= 4, f"from {first} to {moved}")
    press_on("Heyo Settings", "list item", "Shortcuts", 0.8)
    check("a click after the move is a click: Shortcuts is the page shown", "selected" in atspi.states("list item", "Shortcuts", "Heyo Settings"), atspi.states("list item", "Shortcuts", "Heyo Settings"))
    press_on("Heyo Settings", "list item", "General", 0.8)
    picture_of("settings-moved", "Heyo Settings")
    # The margin round its panel, where what is behind shows: a press there is the window's underneath.
    general = atspi.where("list item", "General", "Heyo Settings")
    margin = round(general[0] * per("Heyo Settings", window("Heyo Settings"), general) - 20) if general else 0
    if margin > 4:
        under = application("Under", "check-sway-under")
        frame = window("Heyo Settings")
        # The tiled window is behind the floating one: a press in the margin, on its left side.
        point(frame["x"] + margin / 2, frame["y"] + frame["height"] / 2)
        click()
        check(f"a click in the margin round the panel ({margin} pixels) goes to the window underneath", focused() == "Under", focused())
        under.terminate()
        wait(lambda: not window("Under"))
        sway('[title="^Heyo Settings$"]', "focus")
        time.sleep(0.5)
    else:
        print("     (the settings window keeps no margin here)", flush=True)
    press_on("Heyo Settings", "button", "Close", 0.8)
    check("its close button closes it", bool(wait(lambda: not window("Heyo Settings"), 5)))
    again = open_settings()
    check("asked for again, it comes back, with the keyboard", bool(again) and again["focused"], again)
    check("... floating, its panel and margin again", bool(again) and fits(again), again)
    check("... and a click in it works", press_on("Heyo Settings", "list item", "Shortcuts", 0.8) and "selected" in atspi.states("list item", "Shortcuts", "Heyo Settings"))
    keys("-k", "Escape")
    check("Esc closes it", bool(wait(lambda: not window("Heyo Settings"), 5)))


def ime():
    from swaybed import input_method
    restart("hideOnBlur=false", 'language="zh_CN"')
    method = input_method()
    check("the session takes an input method", bool(method))
    if not method or not open_settings("Heyo 设置"):
        return
    check("the search field is found", press_on("Heyo 设置", "entry", "搜索设置", 0.5))
    check("... and asks the input method for text", method("active") == "yes")
    method("compose", "yuyan")
    picture_of("ime-composing", "Heyo 设置")
    method("choose", "语言")
    time.sleep(0.8)
    picture_of("ime-committed", "Heyo 设置")
    field = atspi.text("entry", "搜索设置", "Heyo 设置")
    said = names("Heyo 设置")
    check("what was chosen is in the field", bool(field) and "语言" in field, repr(field))
    check("... and searched for: of the pages, General is left, which has Language on it", "通用" in said and "快捷键" not in said and "听写" not in said, said[:12])
    close_settings("Heyo 设置")
    restart('language="system"')


def on_outputs():
    sway("create_output")
    both = wait(lambda: (lambda there: there if len(there) == 2 else None)(outputs()), 5) or []
    other = next((one for one in both if one["name"] != output["name"]), None)
    check("Sway has a second output", bool(other))
    if not other:
        return
    time.sleep(1.5)
    across = max(one["rect"]["x"] + one["rect"]["width"] for one in both)
    down = max(one["rect"]["y"] + one["rect"]["height"] for one in both)
    for target in (other, output):
        rect = target["rect"]
        for _ in range(2):
            pointer("to", rect["x"] + rect["width"] // 3, rect["y"] + rect["height"] // 3, across, down)
            time.sleep(0.4)
        heyo()
        palette = wait(lambda: window("Heyo"), 6)
        time.sleep(1.0)
        palette = window("Heyo")
        middle = palette and palette["x"] + palette["width"] / 2
        check(f"with the pointer on {target['name']}: the palette is on it, in its middle", bool(palette) and palette["output"] == target["name"] and abs(middle - (rect["x"] + rect["width"] / 2)) <= 2, palette)
        heyo()
        wait(lambda: not window("Heyo"), 4)
        found = open_settings()
        check(f"with the pointer on {target['name']}: the settings window is on it", bool(found) and found["output"] == target["name"], found)
        if found:
            scales = {one["name"]: one["scale"] for one in both}
            check(f"... and is its panel and margin there (scale {scales[target['name']]:g})", fits(found), found)
            # Moved to the other output, of another size or scale.
            away = next(one for one in both if one["name"] != target["name"])
            sway('[title="^Heyo Settings$"]', "move", "container", "to", "output", away["name"])
            time.sleep(2)
            moved = window("Heyo Settings")
            check(f"moved to {away['name']} (scale {scales[away['name']]:g}): it is its panel and margin there", bool(moved) and moved["output"] == away["name"] and fits(moved), moved)
        close_settings("Heyo Settings")
    # The pill: something light on each output behind it, and the pointer on the second.
    lights = []
    for target in (output, other):
        rect = target["rect"]
        for _ in range(2):
            pointer("to", rect["x"] + rect["width"] // 2, rect["y"] + rect["height"] // 3, across, down)
            time.sleep(0.4)
        lights.append(application(f"Light {target['name']}", f"check-sway-light-{len(lights)}"))
    heyo("--dictate")
    time.sleep(1.5)
    import gi
    gi.require_version("GdkPixbuf", "2.0")
    from gi.repository import GdkPixbuf
    subprocess.run(["grim", "/tmp/check-sway-both.png"], check=True)
    os.makedirs(SHOTS, exist_ok=True)
    subprocess.run(["cp", "/tmp/check-sway-both.png", f"{SHOTS}/outputs-pill.png"])
    image = GdkPixbuf.Pixbuf.new_from_file("/tmp/check-sway-both.png")
    per = image.get_width() / across
    both_shot = (image.get_width(), image.get_height(), image.get_rowstride(), image.get_n_channels(), image.get_pixels())

    def pill_on(rect):
        """Dark in the middle band of where the pill goes on that output, and light at that place's corners."""
        x0, y0 = rect["x"] + (rect["width"] - 360) // 2, rect["y"] + rect["height"] - 44 - rect["height"] // 10
        middle = all(max(at(both_shot, (x0 + 180 + dx) * per, (y0 + 22 + dy) * per)) < 60 for dx, dy in ((-30, -10), (30, -10), (-30, 10), (30, 10)))
        corners = any(max(at(both_shot, (x0 + dx) * per, (y0 + dy) * per)) < 60 for dx, dy in ((1, 1), (358, 1), (1, 42), (358, 42)))
        return middle and not corners

    # Where GNOME has it on the primary monitor, a layer surface here goes on
    # the first output (slint_windows/layer.rs: one output only).
    check(f"the pill is at the bottom centre of the first output ({output['name']}), with the pointer on the second", pill_on(output["rect"]) and not pill_on(other["rect"]),
          f"on {output['name']}: {pill_on(output['rect'])}, on {other['name']}: {pill_on(other['rect'])}")
    if not wait(lambda: not pill_there()[0], 6):
        heyo("--dictate")
    time.sleep(3)
    for light in lights:
        light.terminate()
    sway("output", other["name"], "unplug")
    time.sleep(1.0)


PARTS = {"language": language, "ai": ai, "dictate": dictate, "settings": settings, "ime": ime, "outputs": on_outputs}
if any(name not in PARTS for name in sys.argv[1:]):
    sys.exit(__doc__)
PASSED = "/tmp/check-passed"
if os.path.exists(PASSED):
    os.remove(PASSED)
for name in sys.argv[1:] or PARTS:
    PARTS[name]()
if failed:
    sys.exit(f"{len(failed)} failed")
open(PASSED, "w").close()

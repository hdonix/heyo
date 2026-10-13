"""Inside the Sway session: what its checks have in common.

Sway's tree and which window has the keyboard; pictures of the screen
(grim), since a layer surface is in no tree; a pointer and keys of the
session's own, since headless Sway has neither (pointer.py, and wtype for
the keys); and the count of what failed.
"""
import atexit, json, os, subprocess, time

HERE = os.path.dirname(os.path.abspath(__file__))
HEYO = f"/heyo/src-tauri/target/{os.environ.get('HEYO_BUILD', 'release')}/heyo"
failed = []


def check(what, ok, detail=""):
    print(("ok   " if ok else "FAIL ") + what + (f"  ({detail})" if detail and not ok else ""), flush=True)
    if not ok:
        failed.append(what)


def sway(*args):
    return subprocess.run(["swaymsg", *args], capture_output=True, text=True).stdout


def tree():
    return json.loads(sway("-t", "get_tree"))


def windows(node=None):
    node = node or tree()
    found = [node] if node.get("pid") else []
    for child in node.get("nodes", []) + node.get("floating_nodes", []):
        found += windows(child)
    return found


def focused():
    return next((window.get("name") for window in windows() if window.get("focused")), None)


def wait(condition, seconds=6.0, every=0.15):
    end = time.time() + seconds
    while time.time() < end:
        value = condition()
        if value:
            return value
        time.sleep(every)
    return None


def picture():
    """The screen, as (width, height, stride, channels, pixels), its pixels row by row."""
    import gi
    gi.require_version("GdkPixbuf", "2.0")
    from gi.repository import GdkPixbuf
    subprocess.run(["grim", "/tmp/check-sway.png"], check=True)
    image = GdkPixbuf.Pixbuf.new_from_file("/tmp/check-sway.png")
    return image.get_width(), image.get_height(), image.get_rowstride(), image.get_n_channels(), image.get_pixels()


def at(shot, x, y):
    _, _, stride, channels, pixels = shot
    offset = int(y) * stride + int(x) * channels
    return tuple(pixels[offset:offset + 3])


def dark(colour):
    return max(colour) < 60


def outputs():
    return [output for output in json.loads(sway("-t", "get_outputs")) if output.get("active")]


output = outputs()[0]
scale = output["scale"]
# In the output's own logical pixels, which the pointer goes by.
width, height = output["rect"]["width"], output["rect"]["height"]

# A pointer of the session's own (pointer.py): headless Sway has none, and
# its clients are told nothing of one that `swaymsg seat - cursor` moves.
POINTER = subprocess.Popen([os.path.join(HERE, "pointer.py")], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
POINTER.stdout.readline()


def pointer(*words):
    POINTER.stdin.write(" ".join(str(word) for word in words) + "\n")
    POINTER.stdin.flush()
    POINTER.stdout.readline()


def point(x, y, settle=0.4):
    pointer("to", int(x), int(y), width, height)
    time.sleep(settle)


def press(button="left"):
    pointer("down", button)
    time.sleep(0.12)


def release(button="left"):
    pointer("up", button)
    time.sleep(0.12)


def click():
    press()
    release()
    time.sleep(0.5)


# And a keyboard that stays: wtype makes one for as long as it runs, and a
# program learns of a keyboard only after the keys of one that came and
# went with them. This one types nothing and waits.
KEYBOARD = subprocess.Popen(["wtype", "-s", "86400000"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
atexit.register(KEYBOARD.terminate)
time.sleep(0.5)


def keys(*words, settle=0.4):
    """Keys, as wtype takes them: text to type, -k NAME for a key by its
    name, -M and -m to hold and let go one of the keys held with others,
    -P and -p a key, -s MILLISECONDS to wait."""
    subprocess.run(["wtype", *[str(word) for word in words]], check=False)
    time.sleep(settle)


def application(title, app):
    """A window with a text box (window.py), once Sway has it."""
    started = subprocess.Popen([os.path.join(HERE, "window.py"), title, app], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    wait(lambda: [window for window in windows() if window.get("name") == title])
    time.sleep(0.8)
    return started


def typed(app):
    """What the text box of that window has in it."""
    try:
        return open(f"/tmp/window-{app}.txt").read()
    except OSError:
        return ""


def heyo(*arguments):
    subprocess.run([HEYO, *arguments], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


# Where the pill's window is: 360 by 44, at the bottom centre, a tenth up.
left, top = (width - 360) // 2, height - 44 - height // 10


def pill_there():
    """Whether the pill is drawn where it should be: dark in its middle band, and not at the corners of its window."""
    shot = picture()
    middle = [dark(at(shot, (left + 180 + dx) * scale, (top + 22 + dy) * scale)) for dx, dy in ((-30, -10), (30, -10), (-30, 10), (30, 10))]
    corners = [dark(at(shot, x * scale, y * scale)) for x, y in ((left + 1, top + 1), (left + 358, top + 1), (left + 1, top + 42), (left + 358, top + 42))]
    return all(middle) and not any(corners), (middle, corners)


# A pinned capture of the test's (HEYO_TEST_PIN): pink on its left half
# and blue on its right.
PINK, BLUE = (216, 27, 96), (30, 136, 229)


def is_colour(seen, colour):
    return all(abs(a - b) <= 3 for a, b in zip(seen, colour))


def pinned(x, y, w, h):
    """Whether a pinned capture of that size is at that place of the picture."""
    shot = picture()
    return is_colour(at(shot, x + w // 4, y + h // 2), PINK) and is_colour(at(shot, x + 3 * w // 4, y + h // 2), BLUE)


def input_method():
    """An input method for the session (ime.py), and how to tell it what to do: its answer comes back."""
    method = subprocess.Popen([os.path.join(HERE, "ime.py")], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
    atexit.register(method.terminate)
    if method.stdout.readline().strip() != "ok":
        return None

    def tell(*words):
        method.stdin.write(" ".join(words) + "\n")
        method.stdin.flush()
        answer = method.stdout.readline().strip()
        time.sleep(0.4)
        return answer

    return tell


def same(a, b, points):
    """How many of those places of two pictures are not the same, pixel for pixel."""
    return len(differing(a, b, points))


def differing(a, b, points):
    return [(x, y) for x, y in points if at(a, x, y) != at(b, x, y)]


def around(points):
    """The rectangle those places are in, to say where two pictures differ."""
    return f"within {min(x for x, _ in points)},{min(y for _, y in points)} to {max(x for x, _ in points)},{max(y for _, y in points)}" if points else "nowhere"

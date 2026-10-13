#!/usr/bin/env python3
"""Inside the Sway session: that the screenshot overlay cannot trap whoever uses it.

  safe-sway.py escape   Escape while a drag is in progress
  safe-sway.py panic    Slint's thread panics in the middle of a drag (Heyo started with HEYO_SLINT_PANIC_IN_DRAG)
  safe-sway.py stall    Slint's thread stops answering in the middle of a drag (HEYO_SLINT_STALL_IN_DRAG)
  safe-sway.py kill     Heyo is killed in the middle of a drag

Under Sway the overlay is a layer surface that the compositor gives every
key, over everything on the output: whatever happens to what draws it, the
screen and the keyboard must come back. The same four trials as on X11
(../../gui-test/screenshot-safe.sh): an application's text box has the
keyboard; the overlay comes up, and a key typed then must not reach the
box; a drag begins, and the trouble happens in the middle of it; then
"hello" is typed, and the box must have exactly that. It says how long
after the trouble the overlay was seen to be off the screen (the screen is
looked at by taking a picture of it, which takes a tenth of a second or
so here), and whether Heyo runs and its palette opens afterwards.

Heyo is to be running (start-heyo.sh, which writes its process down in
/tmp/heyo.pid: that one is the one killed, and no other).
"""
import os, signal, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from swaybed import application, check, failed, focused, height, heyo, keys, picture, point, press, release, same, scale, typed, wait, width, windows

trial = sys.argv[1] if len(sys.argv) > 1 else ""
if trial not in ("escape", "panic", "stall", "kill"):
    sys.exit(__doc__)
print(f"== {trial}", flush=True)
process = int(open("/tmp/heyo.pid").read())
S = int(scale)
W, H = width * S, height * S
corners = [(6, 6), (W - 6, 6), (6, H - 6), (W - 6, H - 6), (W // 2, 6), (6, H // 2)]


def up(before):
    return same(picture(), before, corners) == len(corners)


def alive():
    try:
        return open(f"/proc/{process}/stat").read().split()[2] != "Z"
    except OSError:
        return False


app = application("Typed here", "check-safe")
check("an application's text box has the keyboard", focused() == "Typed here", focused())
before = picture()
heyo("--open", "heyo:screenshot")
check("the overlay comes up", bool(wait(lambda: up(before), 8)))
# Held by the overlay: this must not reach the box.
keys("a")
point(width // 4, height // 4)
press()
for step in range(1, 9):
    point(width // 4 + step * 30, height // 4 + step * 20, 0.03)
began = time.time()
if trial == "escape":
    keys("-k", "Escape", settle=0)
elif trial == "kill":
    os.kill(process, signal.SIGKILL)
else:
    # The tenth movement of the drag is where it is asked to happen.
    for step in range(9, 15):
        point(width // 4 + step * 30, height // 4 + step * 20, 0.03)
gone = wait(lambda: same(picture(), before, corners) == 0, 10, every=0.02)
check(f"the overlay is off the screen, {(time.time() - began) * 1000:.0f} ms after", bool(gone), "still there after 10 s")
release()
time.sleep(0.3)
check("the application has the keyboard again", focused() == "Typed here", focused())
keys("hello")
check('the box has exactly "hello"', typed("check-safe") == "hello", repr(typed("check-safe")))
check("Heyo is not running" if trial == "kill" else "Heyo is running", alive() != (trial == "kill"))
if trial == "escape":
    # And the next screenshot comes up as the first did.
    before = picture()
    heyo("--open", "heyo:screenshot")
    check("the next screenshot comes up", bool(wait(lambda: up(before), 8)))
    keys("-k", "Escape")
    check("... and Escape closes it", bool(wait(lambda: same(picture(), before, corners) == 0, 4)))
elif trial in ("panic", "stall"):
    # What is left of Heyo: its palette.
    heyo()
    check("the palette opens", bool(wait(lambda: [window for window in windows() if window.get("name") == "Heyo"], 5)))
    heyo()
app.terminate()
said = subprocess.run(["grep", "-E", "heyo: (panicked|Slint's thread|what draws|the screenshot overlay|the surface|HEYO_SLINT|screenshot failed)", "/tmp/heyo.log"], capture_output=True, text=True).stdout
for line in said.splitlines()[:6]:
    print("   " + line[:200])
sys.exit(1 if failed else 0)

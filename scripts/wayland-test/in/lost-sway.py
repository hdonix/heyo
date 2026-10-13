#!/usr/bin/env python3
"""Inside the Sway session: that nothing of Heyo's is left on the screen when what draws it stops answering.

  lost-sway.py

Heyo's own windows are drawn by one thread. If it stops, what it had on
the screen would stay as it was last drawn: a pill above everything that
nothing can press, pinned captures that cannot be closed. One watcher
takes all of it off the screen from outside (src-tauri/src/slint_windows/
watch.rs). Heyo is to have been started with two pictures pinned
(HEYO_TEST_PIN) and with HEYO_SLINT_STALL_AFTER, which stops that thread
for a minute so long after it began (`sway.sh lost` does both): the pill
is brought up before then, and after it the pill and both pictures must
be gone, the application still have the keyboard, and Heyo still run.
"""
import os, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from swaybed import application, check, failed, focused, heyo, keys, pill_there, pinned, typed, wait, windows

LOG = "/tmp/heyo.log"
process = int(open("/tmp/heyo.pid").read())


def said(what):
    return what in open(LOG, errors="replace").read()


app = application("Typed here", "check-lost")
check("an application's text box has the keyboard", focused() == "Typed here", focused())
check("two pinned captures are on the screen", pinned(200, 160, 300, 200) and pinned(800, 160, 200, 120))
# The pill does not stay long where dictation is not set up: it is brought
# up a little before the thread stops, twenty seconds after Heyo began.
time.sleep(max(0.0, os.path.getmtime("/tmp/heyo.pid") + 18 - time.time()))
heyo("--dictate")
check("the pill is on the screen", bool(wait(lambda: pill_there()[0], 6)), pill_there()[1])
check("what draws them has not stopped yet", not said("HEYO_SLINT_STALL_AFTER asked"))
# Looked at until then: the pill must be one of what is lost.
there, stopped, until = False, False, time.time() + 30
while not stopped and time.time() < until:
    there = pill_there()[0]
    stopped = said("HEYO_SLINT_STALL_AFTER asked")
began = time.time()
check("what draws them stops, with the pill still up", stopped and there, (stopped, there))
gone = wait(lambda: not pill_there()[0] and not pinned(200, 160, 300, 200) and not pinned(800, 160, 200, 120), 10, every=0.05)
check(f"the pill and both pictures are off the screen, {(time.time() - began) * 1000:.0f} ms after", bool(gone), (pill_there()[0], pinned(200, 160, 300, 200), pinned(800, 160, 200, 120)))
check("the application still has the keyboard", focused() == "Typed here", focused())
keys("hello")
check('... and its box has "hello"', typed("check-lost") == "hello", repr(typed("check-lost")))
check("Heyo is running", os.path.exists(f"/proc/{process}"))
heyo()
check("... and its palette opens", bool(wait(lambda: [window for window in windows() if window.get("name") == "Heyo"], 5)))
heyo()
app.terminate()
for line in [line for line in open(LOG, errors="replace") if "what draws" in line or "could not be taken" in line][:4]:
    print("   " + line.rstrip()[:200])
sys.exit(1 if failed else 0)

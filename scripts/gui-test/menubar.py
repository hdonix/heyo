#!/usr/bin/env python3
"""A menu bar command clicked twice: the count must go up, which it only
does while the command stays mounted (runtime/src/background.ts).

The sandbox has no system tray (xfwm4 alone), so the tray icon never shows
and nothing can click it. This drives the background runtime the way the
host does, over its stdin/stdout, with the view-probe extension's counter.

    menubar.py                     uses the sandbox's directories (env.sh)
"""
import json, os, select, subprocess, sys, time

here = os.path.dirname(os.path.abspath(__file__))
repo = os.path.dirname(os.path.dirname(here))
data = os.environ.get("XDG_DATA_HOME", os.path.expanduser("~/.local/share"))
ext = os.path.join(data, "heyo", "extensions")
env = dict(
    os.environ,
    HEYO_BACKGROUND="1",
    HEYO_DATA_DIR=os.path.join(data, "heyo"),
    HEYO_EXTENSIONS_DIR=ext,
    HEYO_PROBE_OUT=os.environ.get("HEYO_PROBE_OUT", "/tmp/probe.json"),
)
runtime = subprocess.Popen(
    ["node", "--no-deprecation", os.path.join(repo, "runtime", "dist", "heyo-runtime.cjs")],
    stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, env=env, text=True,
)
extension = os.path.join(ext, "view-probe")


def send(message):
    runtime.stdin.write(json.dumps(message) + "\n")
    runtime.stdin.flush()


def run(id, action=None):
    """Runs the counter (a click when `action` names an item) and waits
    until it is done, answering any request with nothing."""
    send({"type": "background-run", "id": id, "extension": extension, "command": "counter", "launchType": "userInitiated", "action": action})
    menus, error, done = [], "timed out", False
    deadline = time.time() + 20
    while time.time() < deadline:
        # A click is done before the menu it changed is sent (the command
        # renders again, then settles): read on for a moment after done.
        if done and not select.select([runtime.stdout], [], [], 1.5)[0]:
            break
        line = runtime.stdout.readline()
        if not line:
            break
        message = json.loads(line)
        if message.get("type") == "menu" and message.get("menu"):
            menus.append(message["menu"])
        elif message.get("type") == "request":
            send({"type": "reply", "id": message["id"], "error": {"code": "unsupported", "message": "not in this test"}})
        elif message.get("type") == "background-done" and message.get("id") == id:
            done, error = True, message.get("error")
    return menus, error


def title(menu):
    return menu.get("title") if isinstance(menu, dict) else menu


menus, error = run(1)
print("run:", title(menus[-1]) if menus else None, error or "")
# Twice, so the second click shows the first was kept.
for id in (2, 3):
    menus, error = run(id, [0])
    print(f"click {id - 1}:", title(menus[-1]) if menus else None, error or "")
runtime.stdin.close()
runtime.wait(timeout=5)

#!/usr/bin/env python3
"""Stops the sandbox: the processes its scripts started, and what those
started themselves.

Every process a script here starts goes through `start` (env.sh), which
writes it down in $HEYO_TEST_DIR/pids: its number, when it was started and
what it is. Those are ended here, each only while it is still the process
that was written down (a number is given out again; when a process was
started is not), together with everything they started. Nothing is ended
for what it is called or for what its environment says: a process that
only looks like the sandbox's is not the sandbox's to end.

What cannot be reached that way (a process whose parent ended before this
ran, and which did not end with the display or the bus) is left running
and said, with its number, for a person to look at.
"""
import os
import signal
import sys
import time

directory = os.environ.get("HEYO_TEST_DIR") or os.path.join(os.environ.get("TMPDIR", "/tmp"), "heyo-gui-test")
ledger = os.path.join(directory, "pids")


def stat(pid):
    """(parent, when started) of a process, or None when it is gone."""
    try:
        fields = open(f"/proc/{pid}/stat").read().rsplit(") ", 1)[1].split()
    except (OSError, IndexError):
        return None
    return fields[1], fields[19]


def born(pid):
    found = stat(pid)
    return found[1] if found else None


def command(pid):
    try:
        return open(f"/proc/{pid}/cmdline", "rb").read().replace(b"\0", b" ").decode(errors="replace").strip()
    except OSError:
        return ""


# Those written down that are still what was written down.
family = {}
try:
    lines = open(ledger).read().splitlines()
except OSError:
    lines = []
for line in lines:
    pid, started, *_ = line.split(" ", 2)
    if started and born(pid) == started:
        family[pid] = started
# And what they started, and what those started: noted before any of them
# goes, since a process whose parent has ended is nobody's afterwards.
found = True
while found:
    found = {}
    for pid in filter(str.isdigit, os.listdir("/proc")):
        about = stat(pid)
        if about and pid not in family and about[0] in family and pid != str(os.getpid()):
            found[pid] = about[1]
    family.update(found)


def end(how):
    for pid, started in family.items():
        if born(pid) == started:
            try:
                os.kill(int(pid), how)
            except OSError:
                pass


for pid, started in family.items():
    print("stopping", pid, command(pid)[:90])
end(signal.SIGTERM)
# A window manager whose display has gone can sit on the request for ever,
# and would be found managing the next sandbox's windows as well.
for _ in range(10):
    if not any(born(pid) == started for pid, started in family.items()):
        break
    time.sleep(0.2)
end(signal.SIGKILL)
try:
    os.remove(ledger)
except OSError:
    pass

# What still runs on the sandbox's bus: only looked for, never ended.
home = os.environ.get("HEYO_TEST_HOME")
if home:
    time.sleep(0.3)
    bus = f"DBUS_SESSION_BUS_ADDRESS=unix:path={home}/run/bus".encode()
    # Not this, nor the script that runs it: those have the sandbox's
    # environment too.
    mine, about = {str(os.getpid())}, stat(os.getpid())
    while about and about[0] not in mine:
        mine.add(about[0])
        about = stat(about[0])
    for pid in filter(str.isdigit, os.listdir("/proc")):
        try:
            environment = open(f"/proc/{pid}/environ", "rb").read().split(b"\0")
        except OSError:
            continue
        if bus in environment and pid not in mine:
            print(f"down.py: {pid} still runs in the sandbox and was not started by its scripts: {command(pid)[:90]}", file=sys.stderr)

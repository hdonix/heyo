#!/usr/bin/env python3
"""The sandbox's ledger: the processes its scripts started, and the only
way any of them is ended.

    ledger.py note PID NAME    writes a process down (env.sh's `start` does)
    ledger.py end [NAME…]      ends those written down under the names, or
                               all of them, and what they started themselves
    ledger.py kill NAME        kills that one alone, as nothing it could
                               prepare for; what it had started is written
                               down first, so that a later `end` reaches it
    ledger.py alive NAME       its number, if it still runs (else fails)
    ledger.py family NAME      its number and those of all it started
    ledger.py census           every process whose environment names the
                               sandbox's bus, written down or not: only looks

Every process a script starts goes through `start`, which writes it down
in $HEYO_TEST_DIR/pids: its number, when it was started and what it is.
Those are ended here, each only while it is still the process that was
written down (a number is given out again; when a process was started is
not), together with everything they started. Nothing is ended for what it
is called or for what its environment says: a process that only looks
like the sandbox's is not the sandbox's to end.

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
    # A process that has ended and waits to be reaped is gone.
    return None if fields[0] == "Z" else (fields[1], fields[19])


def born(pid):
    found = stat(pid)
    return found[1] if found else None


def command(pid):
    try:
        return open(f"/proc/{pid}/cmdline", "rb").read().replace(b"\0", b" ").decode(errors="replace").strip()
    except OSError:
        return ""


def written():
    """The lines of the ledger: (number, when started, name)."""
    try:
        lines = open(ledger).read().splitlines()
    except OSError:
        return []
    return [tuple(line.split(" ", 2)) for line in lines if line.count(" ") >= 2]


def living(names=None):
    """Those written down that are still what was written down: {number: when started}."""
    return {pid: started for pid, started, name in written() if started and born(pid) == started and (not names or name in names)}


def with_descendants(family):
    """Adds what they started, and what those started: noted before any of
    them goes, since a process whose parent has ended is nobody's afterwards."""
    family = dict(family)
    found = True
    while found:
        found = {}
        for pid in filter(str.isdigit, os.listdir("/proc")):
            about = stat(pid)
            if about and pid not in family and about[0] in family and pid != str(os.getpid()):
                found[pid] = about[1]
        family.update(found)
    return family


def send(family, how):
    for pid, started in family.items():
        if born(pid) == started:
            try:
                os.kill(int(pid), how)
            except OSError:
                pass


def newest(name):
    found = [pid for pid, started, what in written() if what == name and started and born(pid) == started]
    return found[-1] if found else None


def end(names):
    family = with_descendants(living(names))
    for pid in family:
        print("stopping", pid, command(pid)[:90])
    send(family, signal.SIGTERM)
    # A window manager whose display has gone can sit on the request for ever,
    # and would be found managing the next sandbox's windows as well.
    for _ in range(15):
        if not any(born(pid) == started for pid, started in family.items()):
            break
        time.sleep(0.2)
    send(family, signal.SIGKILL)
    if names:
        kept = [line for line in written() if line[2] not in names]
        with open(ledger, "w") as file:
            file.writelines(" ".join(line) + "\n" for line in kept)
        return
    try:
        os.remove(ledger)
    except OSError:
        pass
    time.sleep(0.3)
    for pid, parent, name, what in census():
        print(f"ledger.py: {pid} still runs in the sandbox and was not started by its scripts: {what[:90]}", file=sys.stderr)


def census():
    """What runs on the sandbox's bus: (number, parent, written down as, command). Only looked for."""
    home = os.environ.get("HEYO_TEST_HOME")
    if not home:
        return []
    bus = f"DBUS_SESSION_BUS_ADDRESS=unix:path={home}/run/bus".encode()
    names = {pid: name for pid, started, name in written() if born(pid) == started}
    # Not this, nor the script that runs it: those have the sandbox's
    # environment too.
    mine, about = {str(os.getpid())}, stat(os.getpid())
    while about and about[0] not in mine:
        mine.add(about[0])
        about = stat(about[0])
    found = []
    for pid in sorted(filter(str.isdigit, os.listdir("/proc")), key=int):
        try:
            environment = open(f"/proc/{pid}/environ", "rb").read().split(b"\0")
        except OSError:
            continue
        about = stat(pid)
        # (What the bus started itself is told its address with more behind it.)
        if any(entry == bus or entry.startswith(bus + b",") for entry in environment) and pid not in mine and about:
            found.append((pid, about[0], names.get(pid, ""), command(pid)))
    return found


def main():
    what, arguments = (sys.argv[1] if len(sys.argv) > 1 else ""), sys.argv[2:]
    if what == "note" and len(arguments) == 2:
        pid, name = arguments
        started = born(pid)
        # One that has ended already needs no ending.
        if started:
            with open(ledger, "a") as file:
                file.write(f"{pid} {started} {name.replace(' ', '-')}\n")
    elif what == "end":
        end(set(arguments))
    elif what == "kill" and len(arguments) == 1:
        pid = newest(arguments[0])
        if pid is None:
            sys.exit(f"ledger.py: nothing written down as {arguments[0]} runs")
        family = with_descendants({pid: born(pid)})
        with open(ledger, "a") as file:
            for member, started in family.items():
                if member != pid:
                    file.write(f"{member} {started} started-by-{arguments[0]}\n")
        os.kill(int(pid), signal.SIGKILL)
    elif what == "alive" and len(arguments) == 1:
        pid = newest(arguments[0])
        if pid is None:
            sys.exit(1)
        print(pid)
    elif what == "family" and len(arguments) == 1:
        pid = newest(arguments[0])
        if pid is None:
            sys.exit(1)
        print(" ".join(with_descendants({pid: born(pid)})))
    elif what == "census":
        for pid, parent, name, said in census():
            print(f"{pid} parent={parent} written-down={name or 'no'} {said[:100]}")
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main()

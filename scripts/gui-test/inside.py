#!/usr/bin/env python3
"""The processes of the sandbox (those started on its display).

inside.py count NAME       how many run the program NAME (`NAME*`: any
                           whose name starts with it)
inside.py memory NAME...   their memory, by name, and the sum: resident, and
                           with what several share counted once (PSS)
"""
import os, sys

display = os.environ.get("HEYO_TEST_DISPLAY", ":99").encode()


def processes():
    for pid in filter(str.isdigit, os.listdir("/proc")):
        try:
            args = open(f"/proc/{pid}/cmdline", "rb").read().split(b"\0")
            env = open(f"/proc/{pid}/environ", "rb").read()
            status = open(f"/proc/{pid}/status").read()
        except OSError:
            continue
        if not args[0] or b"DISPLAY=" + display + b"\0" not in env + b"\0":
            continue
        rss = next((int(line.split()[1]) for line in status.splitlines() if line.startswith("VmRSS:")), 0)
        try:
            rollup = open(f"/proc/{pid}/smaps_rollup").read()
            pss = next((int(line.split()[1]) for line in rollup.splitlines() if line.startswith("Pss:")), rss)
        except OSError:
            pss = rss
        yield os.path.basename(args[0].decode(errors="replace")), rss, pss


def named(process, wanted):
    return process.startswith(wanted[:-1]) if wanted.endswith("*") else process == wanted


what, names = sys.argv[1], sys.argv[2:]
found = [p for p in processes() if any(named(p[0], wanted) for wanted in names)]
if what == "count":
    print(len(found))
else:
    for name in names:
        mine = [p for p in found if named(p[0], name)]
        if mine:
            print(f"  {name:10} {len(mine):2} process{'es' if len(mine) > 1 else '  '} {sum(p[1] for p in mine) / 1024:6.0f} MB resident {sum(p[2] for p in mine) / 1024:6.0f} MB proportional")
    print(f"  {'together':10} {'':12} {sum(p[1] for p in found) / 1024:6.0f} MB resident {sum(p[2] for p in found) / 1024:6.0f} MB proportional")

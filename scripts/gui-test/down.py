#!/usr/bin/env python3
"""Stops the sandbox: only processes that are unmistakably part of it."""
import os, signal, time
me = os.getpid()
stopped = []
display = os.environ.get('HEYO_TEST_DISPLAY', ':99')
for pid in filter(str.isdigit, os.listdir('/proc')):
    pid = int(pid)
    if pid == me: continue
    try:
        args = open(f'/proc/{pid}/cmdline', 'rb').read().split(b'\0')
        env = open(f'/proc/{pid}/environ', 'rb').read()
    except OSError:
        continue
    line = b' '.join(args).decode(errors='replace')
    if 'shell-snapshots' in line or not args[0]: continue
    name = os.path.basename(args[0].decode(errors='replace'))
    inside = b'DISPLAY=' + display.encode() + b'\0' in env + b'\0'
    # ibus-daemon and what it starts are GNOME Shell's, and can outlive it.
    if (name == 'Xvfb' and display.encode() in args) or (inside and name.startswith('ibus-')) \
            or (inside and name in ('dbus-daemon', 'xfwm4', 'heyo', 'heyo-ui', 'python3', 'node', 'gnome-shell', 'dconf-service')) \
            or (inside and name == 'node' and 'vite' in line and '1420' in line):
        print('stopping', pid, line[:90])
        os.kill(pid, signal.SIGTERM)
        stopped.append(pid)
# A window manager whose display has gone can sit on the request for ever,
# and would be found managing the next sandbox's windows as well.
time.sleep(0.5)
for pid in stopped:
    try:
        os.kill(pid, signal.SIGKILL)
    except OSError:
        pass

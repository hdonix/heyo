#!/bin/sh
# What the platform layer does, end to end on X11 in the sandbox: pasting
# into another application, arranging its window, recording the copy in the
# clipboard history, the screenshot overlay coming and going, a system
# command reaching the (sandbox's own) desktop settings.
. "$(dirname "$0")/env.sh"
"$HERE/sandbox.sh" up
start /usr/bin/python3 "$REPO/scripts/wayland-test/in/window.py" Notes org.test.Notes >/dev/null 2>&1; w 2
xdotool search --name '^Notes$' | tail -1 | xargs xdotool windowactivate; w 4
heyo --open cmd:@heyo:emoji; w 4; xdotool type --delay 40 "rocket"; w 1.5; xdotool key Return; w 2.5
echo "pasted into Notes: [$(cat "$WINDOW_DIR/window-org.test.Notes.txt")]"
heyo --open heyo:window-left-half; w 1.5
echo "Notes after Left Half: $(xwininfo -id "$(xdotool search --name '^Notes$' | tail -1)" | grep -E 'Width|Height|Absolute upper-left X' | xargs)"
python3 - "$XDG_DATA_HOME/heyo/clipboard/history.db" <<'PY'
import sqlite3, sys
rows = sqlite3.connect(sys.argv[1]).execute("select text, app from entries order by copied_at desc limit 1").fetchall()
print("clipboard history:", rows)
PY
heyo --open heyo:screenshot; w 3
echo "overlay shown: $(xdotool search --onlyvisible --name 'Heyo Screenshot' | wc -l)"
xdotool key Escape; w 1.5
echo "overlay after Escape: $(xdotool search --onlyvisible --name 'Heyo Screenshot' | wc -l)"
scheme() { gsettings get org.gnome.desktop.interface color-scheme; }
echo "appearance: $(scheme)"; heyo --open cmd:@heyo:toggle-dark-mode; w 3
echo "after Toggle Dark Mode: $(scheme)"
grep -v "EGL\|dbind" "$T/app.log" | tail -4 | cut -c1-160
"$HERE/sandbox.sh" down > /dev/null

#!/bin/sh
# Clipboard History as a view, and pasting from it into a terminal: the
# source application's icon on the rows and in the detail (the copy is made
# in a gnome-terminal), the "recording is paused" states, and the question
# before several lines go into the terminal, answered "Paste and Don't Ask
# Again" (which must stick: the next paste asks nothing). Pictures in
# $HEYO_TEST_DIR/shots/clip-*.png and paste-*.png.
. "$(dirname "$0")/env.sh"
python3 - "$XDG_CONFIG_HOME/heyo/settings.json" <<'PY'
import json, os, sys
path = sys.argv[1]
settings = json.load(open(path)) if os.path.exists(path) else {}
settings.update(hideOnBlur=False, confirmTerminalPaste=True, clipboardPaused=False)
json.dump(settings, open(path, "w"))
PY
rm -f "$T/term.txt"
"$HERE/up.sh"
shot() {
  id=$(xdotool search --onlyvisible --name '^Heyo$' | tail -1)
  set -- "$1" $(xwininfo -id "$id" | awk '/Absolute upper-left X/{x=$4} /Absolute upper-left Y/{y=$4} /Width/{w=$2} /Height/{h=$2} END{print x-10, y-10, x+w+10, y+h+10}')
  python3 "$HERE/grab.py" "$T/shots/$1.png" $2 $3 $4 $5
}
terminal() { xdotool search --onlyvisible --name ProbeTerm | tail -1 | xargs xdotool windowactivate; w 1; }

(gnome-terminal --title=ProbeTerm -- "$HERE/term-sink.sh" "$T/term.txt" >/dev/null 2>&1 &); w 4
terminal
/usr/bin/python3 "$HERE/clip.py" "echo one
echo two
echo three
"
w 1.5
echo "recorded: $(python3 - "$XDG_DATA_HOME/heyo/clipboard/history.db" <<'PY'
import sqlite3, sys
print(sqlite3.connect(sys.argv[1]).execute("select substr(text,1,20), app from entries order by copied_at desc limit 1").fetchall())
PY
)"
heyo --clipboard; w 3; shot clip-rows
xdotool key Return; w 3; shot paste-dialog
xdotool key Tab; w 0.5; xdotool key Return; w 3
echo "the terminal got: [$(tr '\n' '|' < "$T/term.txt")]"
echo "asks again: $(grep -o '"confirmTerminalPaste":[a-z]*' "$XDG_CONFIG_HOME/heyo/settings.json")"
terminal; heyo --clipboard; w 3; xdotool key Return; w 3
echo "the terminal got, unasked: [$(tr '\n' '|' < "$T/term.txt")]"

heyo --open heyo:clipboard-pause; w 2
heyo --clipboard; w 3; shot clip-paused; xdotool key ctrl+k; w 1; shot clip-paused-actions
xdotool key Escape; w 0.5; xdotool key Escape; w 1
heyo --open heyo:clipboard-pause; w 2
grep -v "EGL\|dbind\|heyo: started" "$T/app.log" | tail -3 | cut -c1-200
python3 "$HERE/down.py" > /dev/null

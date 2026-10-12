#!/bin/sh
# What an extension's views do in the palette, with the view-probe
# extension: fields that keep their value (`storeValue`), the file picker
# over a form, the action panel's extras (a date, Heyo's forms filled in,
# Quick Look), fast typing into a search bar the extension answers slowly,
# and a menu bar command clicked twice (driven directly: there is no tray
# here, see menubar.py). Pictures in $HEYO_TEST_DIR/shots/v-*.png; what the
# extension wrote down is printed.
. "$(dirname "$0")/env.sh"
EXT="$XDG_DATA_HOME/heyo/extensions"
mkdir -p "$EXT"
rm -rf "$EXT/view-probe"
cp -r "$HERE/extensions/view-probe" "$EXT/"
export HEYO_PROBE_OUT="$T/probe.json"; rm -f "$T"/probe-*.json
# A fresh start: nothing kept from an earlier run.
rm -f "$XDG_DATA_HOME/heyo/support/view-probe/stored-values.json"
mkdir -p "$T/pick/.hidden" "$T/pick/sub"; touch "$T/pick/a.txt" "$T/pick/b.txt" "$T/pick/.secret"
"$HERE/sandbox.sh" up

open() { heyo --open "cmd:$EXT/view-probe:$1"; w 3; }
show() { printf '%s: ' "$1"; tr -d '\n' < "$T/probe-$1.json" 2>/dev/null | sed 's/  */ /g' | cut -c1-600; echo; }
# A picture of the palette, wherever it is (its top HEIGHT pixels with $2).
shot() {
  id=$(xdotool search --onlyvisible --name '^Heyo$' | tail -1)
  set -- "$1" $(xwininfo -id "$id" | awk '/Absolute upper-left X/{x=$4} /Absolute upper-left Y/{y=$4} /Width/{w=$2} /Height/{h=$2} END{print x-10, y-10, x+w+10, y+(H?H:h)+10}' H="${2:-0}")
  python3 "$HERE/grab.py" "$T/shots/v-$1.png" $2 $3 $4 $5
}
type() { xdotool type --delay 25 -- "$1"; }

# --- storeValue: a form submitted, opened again ---
open stored-form
type "Ada"; xdotool key Tab; w 0.3; xdotool key Down Down; w 0.3; xdotool key Tab space; w 0.5
shot form-filled; xdotool key Return; w 1.5
show stored-form
xdotool key Escape; w 1; open stored-form; shot form-again
echo "the form starts from what was submitted: $(python3 - "$XDG_DATA_HOME/heyo/support/view-probe/stored-values.json" <<'PY'
import json, sys; print(json.load(open(sys.argv[1]))["stored-form"])
PY
)"
xdotool key Escape; w 1

# --- storeValue: a list's dropdown ---
open stored-list; show stored-list
# Tab from the search bar puts the focus on the dropdown; Down picks the next choice.
xdotool key Tab; w 0.3; xdotool key Down; w 1; show stored-list
xdotool key Escape; w 1; open stored-list; shot list-again; show stored-list
xdotool key Escape; w 1

# --- Form.FilePicker: the browser over the form ---
open file-picker; shot pick-form; xdotool key Return; w 2; shot pick-browser
go() { type "$1"; w 0.8; xdotool key Return; w 1.8; }
go ".."; go ".."
for part in $(printf %s "$T/pick" | tr / ' '); do go "$part"; done
shot pick-dir; xdotool key ctrl+h; w 1; shot pick-hidden
type "a.txt"; w 0.6; xdotool key Return; w 0.8; xdotool key ctrl+a BackSpace; w 0.6
type "b.txt"; w 0.6; xdotool key Return; w 0.8; xdotool key ctrl+a BackSpace; w 0.6
xdotool key Home Return; w 1; shot pick-selected
xdotool key ctrl+d; w 2; shot pick-chosen; xdotool key ctrl+Return; w 1
show file-picker
xdotool key Escape; w 1

# --- Action.PickDate, CreateSnippet, CreateQuicklink, ToggleQuickLook ---
open actions; xdotool key Return; w 1.5; shot date-panel
id=$(xdotool search --onlyvisible --name '^Heyo$' | tail -1)
set -- $(xwininfo -id "$id" | awk '/Absolute upper-left X/{x=$4} /Absolute upper-left Y/{y=$4} /Width/{w=$2} /Height/{h=$2} END{print x, y, w, h}')
# "Tomorrow", the second choice of the panel at the bottom right.
xdotool mousemove $(($1 + $3 - 330)) $(($2 + $4 - 210)) click 1; w 1.2; show actions
xdotool key Down; w 0.3; xdotool key Return; w 2; shot snippet-form; xdotool key Escape; w 1
xdotool key ctrl+k; w 1; type "Quicklink"; w 0.6; xdotool key Return; w 2; shot quicklink-form; xdotool key Escape; w 1
xdotool key Down; w 0.3; xdotool key Return; w 2.5; shot quicklook; xdotool key ctrl+y; w 1.5; shot quicklook-closed 120
xdotool key Escape; w 1

# --- Typing: local-first, and a pushed view come back from ---
open typing
start xdotool type --delay 70 "the quick brown fox jumps"; w 0.9; shot typing-mid 120; w 2.5; shot typing-settled 120
show typing
xdotool key Down; w 0.5; xdotool key Return; w 2; shot typing-pushed 160; xdotool key Escape; w 1.5; shot typing-back 120
xdotool key Escape; w 1

# --- A menu bar command, run from Heyo and then clicked (driven directly) ---
open counter; w 3; show counter
echo "tray icon windows: $(xwininfo -root -tree | grep -c 'tray-icon')"
python3 "$HERE/menubar.py"
grep -v "EGL\|dbind\|heyo: started" "$T/app.log" | tail -3 | cut -c1-200
"$HERE/sandbox.sh" down > /dev/null

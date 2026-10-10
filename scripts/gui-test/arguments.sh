#!/bin/sh
# A command's arguments typed in the search bar: Tab into the field, type,
# Enter; the command must be launched with what was typed. Pictures in
# $HEYO_TEST_DIR/shots/args-*.png.
. "$(dirname "$0")/env.sh"
mkdir -p "$XDG_DATA_HOME/heyo/extensions"
rm -rf "$XDG_DATA_HOME/heyo/extensions/api-probe"
cp -r "$HERE/extensions/api-probe" "$XDG_DATA_HOME/heyo/extensions/"
export HEYO_PROBE_OUT="$T/probe.json"; rm -f "$HEYO_PROBE_OUT"
"$HERE/up.sh"
for _ in 1 2 3 4 5 6; do
  [ -n "$(xdotool search --onlyvisible --name '^Heyo$' 2>/dev/null)" ] && break
  heyo; w 3
done
xdotool type --delay 30 "probe arg"; w 1.2
python3 "$HERE/grab.py" "$T/shots/args-shown.png" 440 200 1480 920
xdotool key Return; w 0.6
echo "Enter with the required argument empty launched it: $([ -f "$HEYO_PROBE_OUT" ] && echo yes || echo no)"
# The empty required field has the focus now.
xdotool type --delay 30 "Ada"; w 0.5
python3 "$HERE/grab.py" "$T/shots/args-typed.png" 440 200 1480 920
xdotool key Return; w 2.5
echo "launched with: $(cat "$HEYO_PROBE_OUT" 2>/dev/null)"
grep -v "EGL\|dbind\|heyo: started" "$T/app.log" | tail -3 | cut -c1-200
python3 "$HERE/down.py" > /dev/null

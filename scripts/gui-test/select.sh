#!/bin/sh
# The selection in the root search, a command's list and the action panel
# moving with the arrow keys: pictures in $HEYO_TEST_DIR/shots/select-*.png.
. "$(dirname "$0")/env.sh"
"$HERE/sandbox.sh" up
for _ in 1 2 3 4 5 6; do
  [ -n "$(xdotool search --onlyvisible --name '^Heyo$' 2>/dev/null)" ] && break
  heyo; w 3
done
shot() { python3 "$HERE/grab.py" "$T/shots/select-$1.png" 440 200 1480 920; }
xdotool type --delay 30 "se"; w 1; xdotool key Down Down; w 0.6; shot root-third
xdotool key Up Up Up; w 0.6; shot root-wrapped
xdotool key ctrl+k; w 0.8; xdotool key Down; w 0.6; shot actions-second
xdotool key Escape; w 0.5
heyo --open cmd:@heyo:emoji; w 3; xdotool key Down Right; w 0.6; shot grid
"$HERE/sandbox.sh" down > /dev/null

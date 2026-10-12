#!/bin/sh
# Opening from the palette itself: a command of the runtime's, then one of
# Heyo's own. Pictures in $HEYO_TEST_DIR/shots/open-*.png.
. "$(dirname "$0")/env.sh"
"$HERE/sandbox.sh" up
for _ in 1 2 3 4 5 6; do
  [ -n "$(xdotool search --onlyvisible --name '^Heyo$' 2>/dev/null)" ] && break
  heyo; w 3
done
xdotool type --delay 30 "search emoji"; w 1; xdotool key Return; w 2.5
python3 "$HERE/grab.py" "$T/shots/open-command.png" 440 200 1480 920
# Back in the root search, what was typed is still there.
xdotool key Escape; w 1; xdotool key ctrl+a BackSpace; xdotool type --delay 30 "heyo settings"; w 1
xdotool key Return; w 4
echo "windows now: $(xwininfo -root -tree | grep -i '"heyo\|"settings' | grep -v '10x10' | cut -c1-80 | tr '\n' ';')"
grep -v "EGL\|dbind\|heyo: started" "$T/app.log" | tail -3 | cut -c1-160
"$HERE/sandbox.sh" down > /dev/null

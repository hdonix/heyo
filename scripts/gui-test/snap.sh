#!/bin/sh
# The screenshot overlay offering a window under the pointer, and taking it
# with a click: pictures in $HEYO_TEST_DIR/shots/snap-*.png.
. "$(dirname "$0")/env.sh"
"$HERE/sandbox.sh" up
start /usr/bin/python3 "$REPO/scripts/wayland-test/in/window.py" Notes org.test.Notes >/dev/null 2>&1; w 2
id=$(xdotool search --name '^Notes$' | tail -1)
xdotool windowmove "$id" 300 200; xdotool windowactivate "$id"; w 1
echo "Notes is at: $(xwininfo -id "$id" | grep -E 'Absolute|Width|Height' | xargs)"
heyo --open heyo:screenshot; w 3
xdotool mousemove 500 400; w 0.3; xdotool mousemove 520 420; w 0.8
python3 "$HERE/grab.py" "$T/shots/snap-hover.png" 0 0 1400 900
xdotool click 1; w 1
python3 "$HERE/grab.py" "$T/shots/snap-selected.png" 0 0 1400 900
xdotool key Escape; w 1
grep -v "EGL\|dbind\|heyo: started" "$T/app.log" | tail -3 | cut -c1-200
"$HERE/sandbox.sh" down > /dev/null

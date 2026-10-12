#!/bin/sh
# Live activities: the Timer starts one, and the palette's In Progress
# section shows it counting down, with its buttons.
# Pictures in $HEYO_TEST_DIR/shots/activity-*.png.
. "$(dirname "$0")/env.sh"
printf '{"hideOnBlur":false,"appearance":"light"}' > "$H/config/heyo/settings.json"
"$HERE/sandbox.sh" up
visible() { xdotool search --onlyvisible --name '^Heyo$' 2>/dev/null | wc -l; }
show() { for _ in 1 2 3 4 5 6; do [ "$(visible)" != 0 ] && break; heyo; w 3; done; }
shot() { python3 "$HERE/grab.py" "$T/shots/activity-$1.png" 440 160 1480 920; }
show
xdotool type --delay 40 "timer"; w 1; xdotool key Tab; w 0.4; xdotool type --delay 40 "25m"; w 0.8
shot typed
xdotool key Return; w 3
echo "palette after Enter: $(visible) (0: it closed)"
show; w 1; shot progress
w 2; shot later
xdotool key ctrl+k; w 1; shot actions
xdotool key Escape; w 0.5
grep -v "EGL\|dbind\|heyo: started" "$T/app.log" | tail -n 15 | cut -c1-300
[ -n "$KEEP" ] || "$HERE/sandbox.sh" down > /dev/null

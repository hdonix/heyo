#!/bin/sh
# Heyo with its interface drawn by Flutter (`HEYO_UI=flutter`, ui/ in the
# repository): the palette comes up, searches the real index, opens an
# application, and hides on Escape and when it loses the keyboard; Chinese
# is drawn; a command, the settings, the screenshot overlay and a pinned
# capture each get their window. Then how long things took and how much
# memory they hold.
# Build both first:
#   (cd src-tauri && cargo build) && scripts/build-ui.sh
# Pictures in $HEYO_TEST_DIR/shots/flutter-shell-*.png: look at them.
#
# No dev server is started: no web view is. On a display of your own:
#   HEYO_TEST_DISPLAY=:1234 HEYO_TEST_DIR=/some/dir scripts/gui-test/flutter-shell.sh
. "$(dirname "$0")/env.sh"
export HEYO_UI=flutter HEYO_UI_DEBUG=1 HEYO_TEST_FRONTEND=none
UI="$REPO/ui/build/linux/$(uname -m | sed 's/aarch64/arm64/; s/x86_64/x64/')/release/bundle/heyo-ui"
[ -x "$UI" ] || { echo "no $UI: run scripts/build-ui.sh"; exit 1; }

# An application of the sandbox's own, which says that it was opened.
mkdir -p "$XDG_DATA_HOME/applications"; rm -f "$T/launched"
cat > "$XDG_DATA_HOME/applications/heyo-probe.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Probe Launcher 测试启动器
Comment=日本語 한국어
Exec=touch $T/launched
EOF
printf '{"hideOnBlur": true, "appearance": "%s"}' "${1:-light}" > "$XDG_CONFIG_HOME/heyo/settings.json"

"$HERE/up.sh"
failed=0
check() { if eval "$2"; then echo "ok    $1"; else echo "FAIL  $1"; failed=1; fi; }
shown() { [ -n "$(xdotool search --onlyvisible --name '^Heyo$' 2>/dev/null)" ]; }
inside() { python3 "$HERE/inside.py" "$@"; }
shot() { python3 "$HERE/grab.py" "$T/shots/flutter-shell-$1.png" 440 200 1480 920; }
shot_all() { python3 "$HERE/grab.py" "$T/shots/flutter-shell-$1.png"; }

check "the Flutter interface is the one in use" "grep -q 'interface: flutter' '$T/app.log'"
check "no web view was started" "[ \"\$(inside count 'WebKit*')\" = 0 ]"
check "the palette's process is up, hidden" "[ \"\$(inside count heyo-ui)\" = 1 ] && ! shown"

heyo; w 1.5
check "the palette comes up" shown
check "its window is known as Heyo's own" "xprop -id \"\$(xdotool search --onlyvisible --name '^Heyo\$' | tail -1)\" WM_CLASS | grep -qi '\"heyo\"'"
shot home
xdotool type --delay 40 "probe"; w 1
shot search
xdotool key Return; w 1.5
check "Enter opened the application" "[ -e '$T/launched' ]"
check "and the palette went away" "! shown"

heyo; w 1.5
check "it comes back" shown
check "and was told so, alone" "grep -q 'host says {\"event\":\"heyo://shown\"' '$T/app.log'"
check "what every window is told reached it" "grep -q 'host says {\"event\":\"heyo://index-ready\"' '$T/app.log'"
xdotool type --delay 40 "测试"; w 1
shot chinese
xdotool key Escape; w 0.5
check "Escape clears what was typed" shown
xdotool key Escape; w 1
check "Escape hides it" "! shown"

(/usr/bin/python3 "$REPO/scripts/wayland-test/in/window.py" Notes org.test.Notes >/dev/null 2>&1 &); w 2
heyo; w 1.5
check "it comes up over another application's window" shown
xdotool search --name '^Notes$' | tail -1 | xargs -r xdotool windowactivate; w 1.2
check "it hides when that window takes the keyboard" "! shown"

# A command of the runtime's: what it shows is not drawn yet, but it is
# heard of, and Escape tells the runtime to close it.
heyo; w 1.5
xdotool type --delay 40 "search emoji"; w 1; xdotool key Return; w 3
shot command
check "a command's view arrives from the runtime" "grep -q 'heyo://runtime.*render' '$T/app.log'"
xdotool key Escape; w 1; xdotool key ctrl+a BackSpace; w 0.5
# The settings, opened from the palette: a window and a process of their own.
xdotool type --delay 40 "heyo settings"; w 1; xdotool key Return; w 3
check "the settings open in a window of their own" "[ -n \"\$(xdotool search --onlyvisible --name '^Heyo Settings\$')\" ] && ! shown"
check "drawn by a second process" "[ \"\$(inside count heyo-ui)\" = 2 ]"
shot_all settings
xdotool key alt+F4; w 1.5
check "which ends when the window is closed" "[ \"\$(inside count heyo-ui)\" = 1 ]"

# The screenshot overlay: over the whole screen, with the keyboard, and gone
# at a key. Then a capture kept on screen, closed by Escape.
heyo --open heyo:screenshot; w 3
check "the screenshot overlay covers the screen" "xwininfo -id \"\$(xdotool search --onlyvisible --name '^Heyo Screenshot\$' | tail -1)\" | grep -q -- '-geometry ${SCREEN:-1920x1080}+0+0'"
shot_all overlay
xdotool key p; w 3
check "a capture is pinned in a window of its own" "[ -n \"\$(xdotool search --onlyvisible --name '^Heyo Pinned Screenshot\$')\" ]"
check "and the overlay is gone" "[ -z \"\$(xdotool search --onlyvisible --name '^Heyo Screenshot\$')\" ]"
xdotool search --name '^Heyo Pinned Screenshot$' | tail -1 | xargs -r xdotool windowactivate; w 0.5; xdotool key Escape; w 1.5
check "Escape closes the pinned capture, and its picture is deleted" "[ -z \"\$(xdotool search --name '^Heyo Pinned Screenshot\$')\" ] && [ -z \"\$(ls '$XDG_CACHE_HOME/heyo/screenshot' | grep pin-)\" ]"

heyo; w 1.5
# The window is larger than the panel by the margin its shadow falls in.
eval "$(xdotool search --onlyvisible --name '^Heyo$' | tail -1 | xargs xdotool getwindowgeometry --shell)"
xdotool mousemove $((X + 12)) $((Y + 12)) click 1; w 1
check "a click in the margin around the panel hides it" "! shown"

echo "--- from the log:"
grep "heyo: ui " "$T/app.log" | cut -c1-140
echo "--- memory (resident, of every process of the sandbox's Heyo):"
inside memory heyo heyo-ui 'WebKit*' node
echo "--- frames, 2000 made-up rows (median / 95th percentile, ms):"
for what in keys scroll; do
  "$UI" --standalone --bench=$what 2>&1 >/dev/null | grep '^BENCH' | cut -c7- | python3 -c "
import json, sys
for line in sys.stdin:
    b = json.loads(line); print(f\"{b['what']}: whole frame {b['totalMedianMs']} / {b['totalP95Ms']}, drawing {b['rasterMedianMs']} / {b['rasterP95Ms']}, {b['frames']} frames, {b['over16ms']} over 16.7 ms\")"
done
python3 "$HERE/down.py" > /dev/null
[ "$failed" = 0 ] && echo "all passed" || { echo "something FAILED: see above, $T/app.log and the pictures"; exit 1; }

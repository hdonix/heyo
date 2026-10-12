#!/bin/sh
# Raycast's window API as an extension sees it: the application and window in
# front, and a window put exactly where it was asked to be.
. "$(dirname "$0")/env.sh"
mkdir -p "$XDG_DATA_HOME/heyo/extensions"
rm -rf "$XDG_DATA_HOME/heyo/extensions/api-probe"
cp -r "$HERE/extensions/api-probe" "$XDG_DATA_HOME/heyo/extensions/"
export HEYO_PROBE_OUT="$T/probe.json"; rm -f "$HEYO_PROBE_OUT"
# Reaching windows is the user's to allow; here it has been (heyo-api.sh
# shows the question being asked).
mkdir -p "$XDG_CONFIG_HOME/heyo"
printf '{ "api-probe": ["windows"] }\n' > "$XDG_CONFIG_HOME/heyo/permissions.json"
"$HERE/sandbox.sh" up
start /usr/bin/python3 "$REPO/scripts/wayland-test/in/window.py" Notes org.test.Notes >/dev/null 2>&1; w 2
xdotool search --name '^Notes$' | tail -1 | xargs xdotool windowactivate; w 2
heyo --open "cmd:$XDG_DATA_HOME/heyo/extensions/api-probe:windows"; w 4
cat "$HEYO_PROBE_OUT" 2>/dev/null | tr -d '\n' | sed 's/  */ /g'; echo
echo "Notes is at: $(xwininfo -id "$(xdotool search --name '^Notes$' | tail -1)" | grep -E 'Absolute|Width|Height' | xargs)"
grep -v "EGL\|dbind\|heyo: started" "$T/app.log" | tail -3 | cut -c1-200
"$HERE/sandbox.sh" down > /dev/null

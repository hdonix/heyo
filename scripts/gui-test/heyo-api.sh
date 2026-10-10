#!/bin/sh
# `@heyo/api` as an extension sees it, module by module: the api-probe
# extension calls each part and writes down what came back
# ($HEYO_TEST_DIR/probe-*.json, printed here), and the palette's question
# about a permission is answered once with Deny and once with Allow.
# Pictures in $HEYO_TEST_DIR/shots/api-*.png.
#
# Nothing here ends the session (restart, shut-down and log-out are not
# probed); "toggle-mute" is run twice and so leaves the sound as it was.
. "$(dirname "$0")/env.sh"
EXT="$XDG_DATA_HOME/heyo/extensions"
GRANTS="$XDG_CONFIG_HOME/heyo/permissions.json"
mkdir -p "$EXT" "$XDG_CONFIG_HOME/heyo"
rm -rf "$EXT/api-probe" "$EXT/api-probe-bare"
cp -r "$HERE/extensions/api-probe" "$HERE/extensions/api-probe-bare" "$EXT/"
export HEYO_PROBE_OUT="$T/probe.json"; rm -f "$T"/probe-*.json

run() { heyo --open "cmd:$EXT/$1:$2"; }
show() { printf '%s: ' "$1"; tr -d '\n' < "$T/probe-$1.json" 2>/dev/null | sed 's/  */ /g' | cut -c1-"${2:-1800}"; echo; }
shot() { python3 "$HERE/grab.py" "$T/shots/api-$1.png"; }
copy() { /usr/bin/python3 "$HERE/clip.py" "$1"; w 1.2; }
notes() {
  (/usr/bin/python3 "$REPO/scripts/wayland-test/in/window.py" Notes org.test.Notes >/dev/null 2>&1 &); w 2
  xdotool search --name '^Notes$' | tail -1 | xargs xdotool windowactivate; w 2
}

# Dictation hears a silent recording instead of a microphone, with the
# speech engine installed for the real Heyo (read only); without one,
# `heard` is "unavailable".
[ -d "$HOME/.local/share/heyo/dictation" ] && ln -sfn "$HOME/.local/share/heyo/dictation" "$XDG_DATA_HOME/heyo/dictation"
python3 -c "
import wave,sys
w=wave.open(sys.argv[1],'wb'); w.setnchannels(1); w.setsampwidth(2); w.setframerate(16000); w.writeframes(b'\0\0'*16000*4); w.close()" "$T/silence.wav"
export HEYO_DICTATION_WAV="$T/silence.wav"

# --- First: allowed beforehand all but the two the question is about. ---
printf '{ "api-probe": ["windows", "screen.capture", "system", "microphone"] }\n' > "$GRANTS"
"$HERE/up.sh"
notes

run api-probe heyo-info; w 4; show info
run api-probe heyo-windows; w 7; show windows
echo "Notes is at: $(xwininfo -id "$(xdotool search --name '^Notes$' | tail -1)" | grep -E 'Absolute|Width|Height' | xargs)"
run api-probe heyo-screen; w 9; show screen
echo "captures left behind after the command ended: $(ls "$XDG_CACHE_HOME/heyo/api-probe/captures" 2>/dev/null | wc -l)"

# The overlay without tools; a drag is the whole interaction.
run api-probe heyo-selection; w 3; shot selecting
xdotool mousemove 500 300 mousedown 1; w 0.3
xdotool mousemove 600 380; w 0.2; xdotool mousemove 900 600; w 0.3
xdotool mouseup 1; w 3; show selection

copy "probe text one"
copy "https://example.com/probe"
run api-probe heyo-system; w 16; show system

# The question: Deny (where the dialog starts), then Allow.
run api-probe heyo-asked; w 3; shot asked-memory
xdotool key Return; w 2.5; shot asked-clipboard
xdotool key Tab Return; w 2.5; show asked
echo "allowed now: $(tr -d '\n' < "$GRANTS" | sed 's/  */ /g') (mode $(stat -c %a "$GRANTS"))"

# No manifest: refused without a question, but for Raycast's window functions.
notes
run api-probe-bare bare; w 3; shot asked-bare
xdotool key Tab Return; w 3; show bare
echo "allowed now: $(tr -d '\n' < "$GRANTS" | sed 's/  */ /g')"
grep -v "EGL\|dbind\|heyo: started" "$T/app.log" | tail -5 | cut -c1-200
python3 "$HERE/down.py" > /dev/null

# --- Then: with the memory allowed too (Heyo reads the grants as it starts). ---
printf '{ "api-probe": ["windows", "screen.capture", "system", "microphone", "memory.read", "clipboard.history"] }\n' > "$GRANTS"
"$HERE/up.sh"
copy "probe text two"
copy "https://example.com/probe-again"
copy "the last thing copied"
run api-probe heyo-memory; w 5; show memory 2600
grep -v "EGL\|dbind\|heyo: started" "$T/app.log" | tail -3 | cut -c1-200
python3 "$HERE/down.py" > /dev/null

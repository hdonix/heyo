#!/bin/sh
# Claude Code and Codex as the model, end to end, with stand-ins for both
# (cli-stub.py, as `claude` and `codex` first on the PATH): for each, the
# settings' AI card, a question in Ask AI continued in AI Chat, and a
# dictation polished by it. The real tools are never run here.
#
# Pictures in $RESULTS/cli-<tool>-*.png; what each stub was given in
# $T/cli-log/; what was dictated into the Notes window is printed. Needs
# what dictate-live.sh needs for its first half (the speech engine, Flite).
. "$(dirname "$0")/env.sh"
mkdir -p "$T/bin" "$RESULTS"
for tool in claude codex; do ln -sfn "$GUI/cli-stub.py" "$T/bin/$tool"; done
export PATH="$T/bin:$PATH" CLI_STUB_LOG="$T/cli-log"
rm -rf "$CLI_STUB_LOG"
speech
SPOKEN="$T/spoken.wav"
LENGTH=$(/usr/bin/python3 "$HERE/say.py" "$SPOKEN" "Um, the meeting is moved to Thursday afternoon.") || exit 1
export HEYO_DICTATION_WAV="$SPOKEN"

shot() {
  id=$(xdotool search --onlyvisible --name "$2" | tail -1)
  [ -n "$id" ] || { echo "no window $2 for $1"; return; }
  set -- "$1" $(xwininfo -id "$id" | awk '/Absolute upper-left X/{x=$4} /Absolute upper-left Y/{y=$4} /Width/{w=$2} /Height/{h=$2} END{print x-10, y-10, x+w+10, y+h+10}')
  python3 "$HERE/grab.py" "$RESULTS/$1.png" $2 $3 $4 $5
}

for provider in claude-code codex; do
  tool=${provider%-code}
  seed aiProvider="$provider" anthropicApiKey="" dictationPolish=clean dictationLive=false dictationLanguage=en dictationVocabulary='[]' > /dev/null
  "$HERE/sandbox.sh" up > /dev/null || exit 1

  # The settings: the provider chosen, the tool found on the PATH.
  heyo --open heyo:settings > /dev/null 2>&1
  wait_window '^Heyo Settings$' 100 > /dev/null || { echo "$provider: no settings window"; "$HERE/sandbox.sh" down > /dev/null; exit 1; }
  w 2
  "$HERE/settings-do.sh" page Advanced wait 1.5 move 560 400 wheel 10 wait 1.5 shot "cli-$tool-settings"
  echo "$provider: the settings say: $(ask tree "Heyo Settings" 2>/dev/null | grep -E "Executable|Using|Model Name|not found" | head -4 | cut -c1-200)"
  xdotool key Escape; w 1

  # Ask AI, then AI Chat with a follow-up.
  heyo --open cmd:@heyo:ask-ai; w 3
  xdotool type --delay 20 "What is a launcher?"; w 0.5; xdotool key ctrl+Return
  w 1.2; shot "cli-$tool-streaming" '^Heyo$'; w 6; shot "cli-$tool-answered" '^Heyo$'
  xdotool key Return; w 3
  xdotool type --delay 20 "And a palette?"; w 0.5; xdotool key Return; w 8; shot "cli-$tool-followup" '^Heyo$'
  # Back to the root search, and away.
  xdotool key Escape; w 0.5; xdotool key Escape; w 0.5; xdotool key Escape; w 1

  # A dictation into another application, polished by the tool.
  start env WINDOW_INPUT_METHOD=none /usr/bin/python3 "$REPO/scripts/wayland-test/in/window.py" Notes org.test.Notes "Notes: " > /dev/null 2>&1; w 2
  xdotool search --name '^Notes$' | tail -1 | xargs xdotool windowactivate; w 1
  heyo --dictate; w "$LENGTH"; w 1; heyo --dictate
  w 10
  echo "$provider: dictated into Notes: $(cat "$WINDOW_DIR/window-org.test.Notes.txt" 2>/dev/null)"
  shot "cli-$tool-dictated" '^Notes$'
  xdotool search --name '^Notes$' | tail -1 | xargs xdotool windowclose; w 1

  "$HERE/sandbox.sh" down > /dev/null
done

echo "what the stubs were given:"
python3 - "$CLI_STUB_LOG" <<'PY'
import json, os, sys
d = sys.argv[1]
for name in sorted(os.listdir(d)):
    call = json.load(open(os.path.join(d, name)))
    asked = call["stdin"].replace("\n", " ")[:90]
    print(f"  {name}: {len(call['args'])} arguments, in an empty directory: {call['cwd_empty']}; asked: {asked}")
PY

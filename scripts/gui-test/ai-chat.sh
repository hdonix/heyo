#!/bin/sh
# Ask AI and AI Chat, end to end: without a key the answer is the friendly
# error; with a local model (llm-stub.py, a server on 127.0.0.1 that
# answers everything with canned text, streamed) a question is answered as
# it streams, continued in AI Chat with a follow-up, and the conversation
# is listed, renamed and deleted. Pictures in $HEYO_TEST_DIR/shots/ai-*.png.
. "$(dirname "$0")/env.sh"
PORT=18111
shot() {
  id=$(xdotool search --onlyvisible --name '^Heyo$' | tail -1)
  set -- "$1" $(xwininfo -id "$id" | awk '/Absolute upper-left X/{x=$4} /Absolute upper-left Y/{y=$4} /Width/{w=$2} /Height/{h=$2} END{print x-10, y-10, x+w+10, y+h+10}')
  python3 "$HERE/grab.py" "$T/shots/ai-$1.png" $2 $3 $4 $5
}
configure() {
  python3 - "$XDG_CONFIG_HOME/heyo/settings.json" "$1" "$PORT" <<'PY'
import json, os, sys
path, provider, port = sys.argv[1:4]
settings = json.load(open(path)) if os.path.exists(path) else {}
settings.update(hideOnBlur=False, aiProvider=provider, anthropicApiKey="", localModelUrl=f"http://127.0.0.1:{port}/v1", localModelName="stub-model")
json.dump(settings, open(path, "w"))
PY
}
ask() { heyo --open cmd:@heyo:ask-ai; w 3; xdotool type --delay 20 "$1"; w 0.5; xdotool key ctrl+Return; }

# --- No key: the friendly error. ---
configure claude
# Nothing of this shell's reaches the model either.
unset ANTHROPIC_API_KEY ANTHROPIC_BASE_URL ANTHROPIC_AUTH_TOKEN
"$HERE/up.sh"
ask "What is a launcher?"; w 4; shot nokey
echo "without a key: $(python3 - "$XDG_DATA_HOME/heyo/support/@heyo/conversations" <<'PY'
import json, os, sys
d = sys.argv[1]
files = sorted(os.listdir(d), key=lambda f: os.path.getmtime(os.path.join(d, f))) if os.path.isdir(d) else []
c = json.load(open(os.path.join(d, files[-1]))) if files else {}
print([m.get("role") for m in c.get("messages", [])], "(no answer kept)" if len(c.get("messages", [])) == 1 else "")
PY
)"
python3 "$HERE/down.py" > /dev/null

# --- A local model: the stub. ---
configure local
"$HERE/up.sh"
start python3 "$HERE/llm-stub.py" "$PORT"; w 1
ask "What is a launcher?"; w 1.5; shot streaming; w 6; shot answered
xdotool key Return; w 3; shot chat
xdotool type --delay 20 "And a palette?"; w 0.5; xdotool key Return; w 8; shot followup
xdotool key Escape; w 2; shot list
xdotool key ctrl+shift+r; w 2; xdotool key ctrl+a; xdotool type --delay 20 "Launchers and palettes"; xdotool key Return; w 2; shot renamed
kept() {
  python3 - "$XDG_DATA_HOME/heyo/support/@heyo/conversations" <<'PY'
import json, os, sys
d = sys.argv[1]
for f in sorted(os.listdir(d), key=lambda f: os.path.getmtime(os.path.join(d, f))):
    c = json.load(open(os.path.join(d, f)))
    print("  kept:", c.get("title"), [m.get("role") for m in c.get("messages", [])])
PY
}
echo "after the rename:"; kept
xdotool key ctrl+x; w 1.5; shot delete-dialog; xdotool key Return; w 2; shot deleted
echo "after the delete:"; kept
grep -v "EGL\|dbind\|heyo: started" "$T/app.log" | tail -3 | cut -c1-200
python3 "$HERE/down.py" > /dev/null

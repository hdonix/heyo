#!/bin/sh
# Snippets expanding as their keyword is typed in another application: one
# that expands at once (it starts with a symbol), one that waits for the
# word to end, and a word that merely contains a keyword and is left alone.
# xdotool's keys are synthetic, which Heyo ignores unless told otherwise
# (debug builds only).
. "$(dirname "$0")/env.sh"
mkdir -p "$XDG_DATA_HOME/heyo"
cat > "$XDG_DATA_HOME/heyo/snippets.json" <<'JSON'
[{ "id": "1", "name": "Signature", "text": "Kind regards, Ada", "keyword": ";sig" },
 { "id": "2", "name": "Address", "text": "1 Main Street", "keyword": "addr" }]
JSON
python3 - "$XDG_CONFIG_HOME/heyo/settings.json" <<'PY'
import json, os, sys
path = sys.argv[1]
settings = json.load(open(path)) if os.path.exists(path) else {}
settings.update(snippetExpansion=True)
json.dump(settings, open(path, "w"))
PY
export HEYO_TYPED_SYNTHETIC=1
"$HERE/up.sh"
start /usr/bin/python3 "$REPO/scripts/wayland-test/in/window.py" Notes org.test.Notes >/dev/null 2>&1; w 2
xdotool search --name '^Notes$' | tail -1 | xargs xdotool windowactivate; w 2
type() { xdotool type --delay 60 "$1"; w 1.5; }
type "a ;sig"; type " b addr "; type "c addresses d"
echo "typed into Notes: [$(cat /tmp/window-org.test.Notes.txt)]"
grep -i "snippet\|listening\|left alone" "$T/app.log" | tail -5 | cut -c1-160
python3 - "$XDG_CONFIG_HOME/heyo/settings.json" <<'PY'
import json, sys
settings = json.load(open(sys.argv[1])); settings.pop("snippetExpansion", None); json.dump(settings, open(sys.argv[1], "w"))
PY
rm -f "$XDG_DATA_HOME/heyo/snippets.json"
python3 "$HERE/down.py" > /dev/null

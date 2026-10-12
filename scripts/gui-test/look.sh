#!/bin/sh
# Pictures of the interface in a theme, in $HEYO_TEST_DIR/shots/NAME-*.png:
# the root search, a command (clipboard history) and the settings.
#
#   look.sh light|dark NAME [xfwm|gnome]
. "$(dirname "$0")/env.sh"
python3 - "$T/home/config/heyo/settings.json" "$1" <<'PY'
import json, os, sys
path, theme = sys.argv[1:3]
settings = json.load(open(path)) if os.path.exists(path) else {}
settings.update(appearance=theme, hideOnBlur=False)
json.dump(settings, open(path, "w"))
PY
"$HERE/sandbox.sh" up "${3:-xfwm}"
for _ in 1 2 3 4 5 6; do
  [ -n "$(xdotool search --onlyvisible --name '^Heyo$' 2>/dev/null)" ] && break
  heyo; w 3
done
xdotool type --delay 30 "se"; w 1.5
python3 "$HERE/grab.py" "$T/shots/$2-root.png" 440 200 1480 920
xdotool key ctrl+a BackSpace; w 0.5; heyo --clipboard; w 3
python3 "$HERE/grab.py" "$T/shots/$2-clip.png" 440 200 1480 920
xdotool key Escape Escape; w 0.5
heyo --open heyo:settings; w 4
python3 "$HERE/grab.py" "$T/shots/$2-settings.png" 440 200 1480 960
"$HERE/sandbox.sh" down > /dev/null

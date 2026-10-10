#!/bin/sh
# Pictures of compact mode: the palette before and after something is typed.
#   compact.sh NAME [xfwm|gnome]
. "$(dirname "$0")/env.sh"
"$HERE/up.sh" "${2:-xfwm}"
for _ in 1 2 3 4 5 6; do
  [ -n "$(xdotool search --onlyvisible --name '^Heyo$' 2>/dev/null)" ] && break
  heyo; w 3
done
python3 "$HERE/grab.py" "$T/shots/$1-empty.png" 440 200 1480 920
xdotool type --delay 30 "se"; w 1.5
python3 "$HERE/grab.py" "$T/shots/$1-typed.png" 440 200 1480 920
python3 "$HERE/down.py" > /dev/null

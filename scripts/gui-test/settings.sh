#!/bin/sh
# Every page of the settings window, in both themes, as pictures:
# $HEYO_TEST_DIR/shots/settings-<page>-<theme>.png (and -2, -3 where the
# page scrolls). The api-probe, prefs-probe and view-probe extensions are
# installed so that an extension's page with a permissions card, and one
# with a preference of every type, are among them. "Copy Diagnostics" on
# the About page is pressed and what it copied is printed, with the home
# directory and user name looked for (they must be scrubbed), and the
# first permission api-probe was given is taken back.
#
#   settings.sh [light|dark]     both when not given
#
# The window is drawn by Slint (src-tauri/src/slint_windows/): nothing
# in it can be asked where it is, so what is clicked is clicked where the
# window's layout puts it, in the window's own pixels (924 by 704, which
# includes the margin its shadow falls in), times the scale it is shown at.
. "$(dirname "$0")/env.sh"
EXT="$XDG_DATA_HOME/heyo/extensions"
mkdir -p "$EXT" "$XDG_CONFIG_HOME/heyo"
for extension in api-probe prefs-probe view-probe; do
  rm -rf "$EXT/$extension"
  cp -r "$HERE/extensions/$extension" "$EXT/"
done
# The pages, by their row in the sidebar: the six pages, then Applications,
# Heyo's commands and the extensions by title.
PAGES="general:0 shortcuts:1 dictation:2 screenshots:3 advanced:4 about:5 applications:6 heyo-commands:7 api-probe:8 prefs-probe:9 view-probe:10"

window() { xdotool search --onlyvisible --name '^Heyo Settings$' | tail -1; }
# at X Y: the pointer there, in the window's own pixels.
at() { xdotool mousemove $((X + $1 * W / 924)) $((Y + $2 * W / 924)); }
click() { at "$1" "$2"; xdotool click 1; }
# page ROW: the page of that row of the sidebar, the pointer then over its
# lines, for the wheel.
page() { click 152 $((32 + 8 + 58 + $1 * 34 + 17 + ($1 >= 6 ? 14 : 0))); at 560 400; }
grab() { python3 "$HERE/grab.py" "$T/shots/$1.png" $((X - 10)) $((Y - 10)) $((X + W + 10)) $((Y + H + 10)); }

for theme in ${1:-light dark}; do
  python3 - "$XDG_CONFIG_HOME/heyo/settings.json" "$theme" <<'PY'
import json, os, sys
path, theme = sys.argv[1:3]
settings = json.load(open(path)) if os.path.exists(path) else {}
settings.update(appearance=theme, hideOnBlur=False, dictationVocabulary=["TypeScript", "heyo -> Heyo"])
json.dump(settings, open(path, "w"))
PY
  printf '{ "api-probe": ["windows", "screen.capture"] }\n' > "$XDG_CONFIG_HOME/heyo/permissions.json"
  "$HERE/up.sh"
  heyo --open heyo:settings; w 4
  id=$(window)
  [ -n "$id" ] || { echo "$theme: no settings window came up" >&2; python3 "$HERE/down.py" > /dev/null; exit 1; }
  set -- $(xwininfo -id "$id" | awk '/Absolute upper-left X/{x=$4} /Absolute upper-left Y/{y=$4} /Width/{w=$2} /Height/{h=$2} END{print x, y, w, h}')
  X=$1; Y=$2; W=$3; H=$4
  echo "$theme: settings window ${W}x${H} at $X,$Y"
  for entry in $PAGES; do
    name=${entry%%:*}
    page "${entry#*:}"; w 1.5
    grab "settings-$name-$theme"
    # Pages taller than the window: the rest of them, most of a screen at
    # a time (a notch of the wheel is 120 of the window's pixels).
    last="settings-$name-$theme"
    for part in 2 3 4 5 6 7 8 9; do
      xdotool click --repeat 4 --delay 30 5; w 0.8
      grab "settings-$name-$theme-$part"
      if cmp -s "$T/shots/$last.png" "$T/shots/settings-$name-$theme-$part.png"; then rm "$T/shots/settings-$name-$theme-$part.png"; break; fi
      last="settings-$name-$theme-$part"
    done
  done
  # Copy Diagnostics, on About, then what it copied.
  page 5; w 1
  click 582 440; w 1.5
  copied=$(/usr/bin/python3 - <<'PY'
import gi
gi.require_version("Gtk", "3.0"); gi.require_version("Gdk", "3.0")
from gi.repository import Gtk, Gdk
print(Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD).wait_for_text() or "")
PY
  )
  echo "diagnostics copied ($(printf %s "$copied" | wc -l) lines):"; printf %s "$copied" | head -40 | sed 's/^/  /'
  echo "home directory in it: $(printf %s "$copied" | grep -c "$HOME"); user name in it: $(printf %s "$copied" | grep -c "$(id -un)")"
  # The permissions card of api-probe, four notches above the page's end:
  # Revoke on the first of the two it was given takes that one back.
  page 8; w 1.5
  xdotool click --repeat 40 --delay 20 5; w 0.8
  xdotool click --repeat 4 --delay 30 4; w 0.8
  click 828 471; w 1.5
  grab "settings-permissions-revoked-$theme"
  echo "allowed after Revoke: $(tr -d '\n ' < "$XDG_CONFIG_HOME/heyo/permissions.json")"
  xdotool key Escape; w 1
  python3 "$HERE/down.py" > /dev/null
done

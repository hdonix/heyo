#!/bin/sh
# Every page of the settings window, in both themes, as pictures:
# $HEYO_TEST_DIR/shots/settings-<page>-<theme>.png (and -2 where the page
# scrolls). The api-probe and view-probe extensions are installed so that
# an extension page with a permissions card is among them. "Copy
# Diagnostics" on the About page is pressed and what it copied is printed,
# with the home directory and user name looked for (they must be scrubbed).
#
#   settings.sh [light|dark]     both when not given
. "$(dirname "$0")/env.sh"
EXT="$XDG_DATA_HOME/heyo/extensions"
mkdir -p "$EXT" "$XDG_CONFIG_HOME/heyo"
rm -rf "$EXT/api-probe" "$EXT/view-probe"
cp -r "$HERE/extensions/api-probe" "$HERE/extensions/view-probe" "$EXT/"
printf '{ "api-probe": ["windows", "screen.capture"] }\n' > "$XDG_CONFIG_HOME/heyo/permissions.json"
# The pages, and what to type in the sidebar's search to have each first.
PAGES="general:General shortcuts:Shortcuts dictation:Dictation screenshots:Screenshots advanced:Advanced about:About applications:Applications heyo-commands:Heyo api-probe:API view-probe:View"

# Where the window is: X Y of its top-left corner.
place() { xwininfo -id "$(xdotool search --onlyvisible --name 'Heyo Settings' | tail -1)" | awk '/Absolute upper-left X/{x=$4} /Absolute upper-left Y/{y=$4} END{print x, y}'; }
grab() { python3 "$HERE/grab.py" "$T/shots/$1.png" $((X - 10)) $((Y - 10)) $((X + W + 10)) $((Y + H + 10)); }

for theme in ${1:-light dark}; do
  python3 - "$XDG_CONFIG_HOME/heyo/settings.json" "$theme" <<'PY'
import json, os, sys
path, theme = sys.argv[1:3]
settings = json.load(open(path)) if os.path.exists(path) else {}
settings.update(appearance=theme, hideOnBlur=False, dictationVocabulary=["TypeScript", "heyo -> Heyo"])
json.dump(settings, open(path, "w"))
PY
  "$HERE/up.sh"
  heyo --open heyo:settings; w 4
  set -- $(place); X=$1; Y=$2
  W=$(xwininfo -id "$(xdotool search --onlyvisible --name 'Heyo Settings' | tail -1)" | awk '/Width/{print $2}')
  H=$(xwininfo -id "$(xdotool search --onlyvisible --name 'Heyo Settings' | tail -1)" | awk '/Height/{print $2}')
  echo "$theme: settings window ${W}x${H} at $X,$Y"
  for page in $PAGES; do
    name=${page%%:*}; typed=${page#*:}
    # The sidebar's search narrows the list to the page, which is then first.
    xdotool mousemove $((X + 140)) $((Y + 82)) click 1; w 0.3
    xdotool key ctrl+a; xdotool type --delay 20 "$typed"; w 0.6
    xdotool mousemove $((X + 140)) $((Y + 125)) click 1; w 1.2
    grab "settings-$name-$theme"
    # Pages taller than the window: the rest of them, a screen at a time.
    xdotool mousemove $((X + 600)) $((Y + 400)); last="settings-$name-$theme"
    for part in 2 3 4 5 6 7 8 9; do
      xdotool click --repeat 7 --delay 20 5; w 0.8
      grab "settings-$name-$theme-$part"
      if cmp -s "$T/shots/$last.png" "$T/shots/settings-$name-$theme-$part.png"; then rm "$T/shots/settings-$name-$theme-$part.png"; break; fi
      last="settings-$name-$theme-$part"
    done
    xdotool click --repeat 40 --delay 10 4; w 0.3
  done
  # About is the page left open: Copy Diagnostics, then what it copied.
  xdotool mousemove $((X + 140)) $((Y + 82)) click 1; xdotool key ctrl+a; xdotool type --delay 20 "About"; w 0.5
  xdotool mousemove $((X + 140)) $((Y + 125)) click 1; w 1
  xdotool mousemove $((X + 590)) $((Y + 463)) click 1; w 1.5
  copied=$(/usr/bin/python3 - <<'PY'
import gi
gi.require_version("Gtk", "3.0"); gi.require_version("Gdk", "3.0")
from gi.repository import Gtk, Gdk
print(Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD).wait_for_text() or "")
PY
  )
  echo "diagnostics copied ($(printf %s "$copied" | wc -l) lines):"; printf %s "$copied" | head -40 | sed 's/^/  /'
  echo "home directory in it: $(printf %s "$copied" | grep -c "$HOME"); user name in it: $(printf %s "$copied" | grep -c "$(id -un)")"
  # The permissions card of api-probe: Revoke on the first grant takes it back.
  xdotool mousemove $((X + 140)) $((Y + 82)) click 1; xdotool key ctrl+a; xdotool type --delay 20 "API"; w 0.5
  xdotool mousemove $((X + 140)) $((Y + 125)) click 1; w 1.2
  xdotool mousemove $((X + 600)) $((Y + 400)); xdotool click --repeat 7 --delay 20 5; w 1
  xdotool mousemove $((X + 829)) $((Y + 176)) click 1; w 1.5
  grab "settings-permissions-revoked-$theme"
  echo "allowed after Revoke: $(tr -d '\n ' < "$XDG_CONFIG_HOME/heyo/permissions.json")"
  xdotool key Escape; w 1
  python3 "$HERE/down.py" > /dev/null
done

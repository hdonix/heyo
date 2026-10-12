#!/bin/sh
# Every page of the settings window, in both themes, as pictures:
# $HEYO_TEST_DIR/shots/settings-<page>-<theme>.png (and -2, -3 where the
# page scrolls). The api-probe, prefs-probe and view-probe extensions are
# installed so that an extension's page with a permissions card, and one
# with a preference of every type, are among them. On Shortcuts a group is
# opened and pictured. "Copy Diagnostics" on the About page is pressed and
# what it copied is printed, with the home directory and user name looked
# for (they must be scrubbed), and the first permission api-probe was
# given is taken back.
#
#   settings.sh [light|dark]     both when not given
#
# The window is drawn by Slint (src-tauri/src/slint_windows/) and says
# where its controls are on the accessibility bus: a page's entry in the
# sidebar and a button are found by what they are and say (ask.py), at
# whatever scale and size the window is.
. "$(dirname "$0")/env.sh"
extensions api-probe prefs-probe view-probe
# The pages, by the name of their picture and their entry in the sidebar:
# the six pages, then Applications, Heyo's commands and the extensions.
PAGES="general:General shortcuts:Shortcuts dictation:Dictation screenshots:Screenshots advanced:Advanced about:About applications:Applications heyo-commands:Heyo_Commands api-probe:API_Probe prefs-probe:Preferences_Probe view-probe:View_Probe"

# press ROLE NAME: a click on that control of the window.
press() { ask click "$1" "$2" "Heyo Settings" > /dev/null; }
# page TITLE: the page of that entry of the sidebar, the pointer then over
# its lines, for the wheel.
page() {
  press "list item" "$1" || { echo "no page $1 in the sidebar" >&2; return 1; }
  set -- $(ask where "list box" "" "Heyo Settings")
  xdotool mousemove $(($1 + $3 / 2)) $(($2 + $4 / 2))
}
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
  "$HERE/sandbox.sh" up
  heyo --open heyo:settings; w 4
  id=$(window '^Heyo Settings$')
  [ -n "$id" ] || { echo "$theme: no settings window came up" >&2; "$HERE/sandbox.sh" down > /dev/null; exit 1; }
  set -- $(place "$id")
  X=$1; Y=$2; W=$3; H=$4
  echo "$theme: settings window ${W}x${H} at $X,$Y, the session at scale $(scale)"
  for entry in $PAGES; do
    name=${entry%%:*}
    page "$(echo "${entry#*:}" | tr _ ' ')"; w 1.5
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
  # On Shortcuts, the applications' group opened (its row in the list: the
  # second thing that says "Applications", after the sidebar's entry).
  page Shortcuts; w 1
  press label "Applications -2"; w 1.5
  grab "settings-shortcuts-open-$theme"
  # Copy Diagnostics, on About, then what it copied.
  page About; w 1
  press "push button" "Copy Diagnostics" || echo "no Copy Diagnostics button on About" >&2
  w 1.5
  copied=$(/usr/bin/python3 - <<'PY'
import gi
gi.require_version("Gtk", "3.0"); gi.require_version("Gdk", "3.0")
from gi.repository import Gtk, Gdk
print(Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD).wait_for_text() or "")
PY
  )
  echo "diagnostics copied ($(printf %s "$copied" | wc -l) lines):"; printf %s "$copied" | head -40 | sed 's/^/  /'
  echo "home directory in it: $(printf %s "$copied" | grep -c -e "$HOME" -e "$REAL_HOME"); user name in it: $(printf %s "$copied" | grep -c "$(id -un)")"
  # The permissions card of api-probe, a little above the page's end (the
  # window says only what is in view: from the end, a notch up at a time
  # until it is): Revoke on the first of the two it was given takes that
  # one back.
  page "API Probe"; w 1.5
  xdotool click --repeat 40 --delay 20 5; w 0.8
  revoked=
  for _ in 1 2 3 4 5 6 7 8 9 10 11 12; do
    press "push button" "Revoke" 2> /dev/null && { revoked=yes; break; }
    xdotool click 4; w 0.5
  done
  [ -n "$revoked" ] || echo "no Revoke button near the end of API Probe's page" >&2
  w 1.5
  grab "settings-permissions-revoked-$theme"
  echo "allowed after Revoke: $(tr -d '\n ' < "$XDG_CONFIG_HOME/heyo/permissions.json")"
  xdotool key Escape; w 1
  "$HERE/sandbox.sh" down > /dev/null
done

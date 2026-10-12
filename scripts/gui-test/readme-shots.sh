#!/bin/sh
# The pictures of the README, taken again: docs/images/<name>-<theme>.png.
#
#   readme-shots.sh [light|dark]     both when not given
#
# Each theme gets a sandbox of its own, started from nothing (readme-env.sh
# says how, and how a picture is cut out of its screen), and a little
# sample data.
#
# It needs the network (the Raycast Store, from which one extension is
# installed, and the exchange rates) and takes about three minutes a theme.
# KEEP=1 leaves the last sandbox running, to look around in.
#
# The picture of dictation is readme-dictation.sh's, which this runs when
# it is done (unless a sandbox is kept), and which can be run by itself.
. "$(dirname "$0")/readme-env.sh"

# Heyo's settings and what it has learnt: the favourites, and what has been
# opened how often (the Suggestions of the empty search).
heyo_state() { # light|dark
  mkdir -p "$XDG_CONFIG_HOME/heyo" "$XDG_DATA_HOME/heyo"
  python3 - "$XDG_CONFIG_HOME/heyo/settings.json" "$XDG_DATA_HOME/heyo/usage.json" "$1" <<'PY'
import json, sys, time
settings, usage, theme = sys.argv[1:4]
json.dump({
    "appearance": theme,
    "favorites": ["app:org.gnome.Nautilus.desktop", "app:firefox_firefox.desktop", "cmd:@heyo:clipboard-history"],
}, open(settings, "w"))
now = int(time.time() * 1000)
used = ["app:org.gnome.Terminal.desktop", "cmd:@heyo:emoji", "app:org.gnome.Calendar.desktop",
        "heyo:screenshot", "cmd:@heyo:switch-windows", "app:org.gnome.Settings.desktop",
        "cmd:@heyo:store", "cmd:@heyo:timer"]
json.dump({id: {"count": 40 - 4 * place, "last": now - place * 3600_000} for place, id in enumerate(used)}, open(usage, "w"))
PY
}

# Things copied, for Clipboard History: each in the application it would
# come from, which is what the history says it came from. One program makes
# all the copies and then keeps the clipboard, as an application that stays
# open does (a copy whose owner goes away is handed to the desktop, and is
# then counted twice).
copies() {
  start gnome-text-editor > /dev/null 2>&1; echo "$STARTED" > "$T/editor.pid"
  start eog "$REPO/src-tauri/icons/icon.png" > /dev/null 2>&1; echo "$STARTED" > "$T/viewer.pid"
  # The terminal's own program ends, and its window with it, when the
  # copies are made.
  rm -f "$T/copied"
  start gnome-terminal -- sh -c 'while [ ! -f "$0" ]; do sleep 0.3; done' "$T/copied" > /dev/null 2>&1
  w 6
  cat > "$T/copies.py" <<'PY'
import subprocess, sys
import gi
gi.require_version("Gtk", "3.0")
gi.require_version("Gdk", "3.0")
from gi.repository import Gdk, GdkPixbuf, GLib, Gtk

picture, done = sys.argv[1:3]
# The oldest first: the window it is copied in, and what.
steps = [
    ("gnome-terminal", "pnpm tauri dev"),
    ("gnome-terminal", "ssh deploy@staging.example.com"),
    ("gnome-text-editor", "#1f86ff"),
    ("eog", None),
    ("gnome-text-editor", "https://github.com/hdonix/heyo"),
    ("gnome-text-editor", "Design review moved to Thursday 10:30, room 2B"),
]
clipboard = Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD)

def focus():
    if not steps:
        open(done, "w").close()
        return False
    subprocess.run(["xdotool", "search", "--onlyvisible", "--class", steps[0][0], "windowactivate"])
    GLib.timeout_add(900, copy)
    return False

def copy():
    _, text = steps.pop(0)
    if text is None:
        clipboard.set_image(GdkPixbuf.Pixbuf.new_from_file(picture))
    else:
        clipboard.set_text(text, -1)
    GLib.timeout_add(900, focus)
    return False

GLib.idle_add(focus)
Gtk.main()
PY
  start /usr/bin/python3 "$T/copies.py" "$REPO/src-tauri/icons/icon.png" "$T/copied" > /dev/null 2>&1
  for _ in $(seq 40); do [ -f "$T/copied" ] && break; w 0.5; done
  # The applications go, so that none is behind the palette.
  kill "$(cat "$T/editor.pid")" "$(cat "$T/viewer.pid")" 2> /dev/null
  for _ in $(seq 20); do [ -z "$(xdotool search --onlyvisible --class gnome-terminal 2> /dev/null)" ] && break; w 0.5; done
  w 1
}

palette() { xdotool search --onlyvisible --name '^Heyo$' 2>/dev/null | tail -1; }
show() { for _ in 1 2 3 4 5 6; do [ -n "$(palette)" ] && break; heyo; w 3; done; }
type() { xdotool type --delay 40 -- "$1"; }
clear() { xdotool key ctrl+a BackSpace; w 0.6; }
# In a command opened with `heyo --open` while the palette was away, the
# first key typed does not arrive in the search bar (seen here, in the
# sandbox; why has not been looked into). One is spent before the text.
first_key() { type "x"; w 0.6; clear; }
# Out of whatever is open, to nothing on the screen.
away() { xdotool key Escape; w 0.4; xdotool key Escape; w 0.4; xdotool key ctrl+a BackSpace Escape; w 1; }

for THEME in ${*:-light dark}; do
  home
  session "$THEME"
  heyo_state "$THEME"
  "$HERE/up.sh" gnome
  EXT="$XDG_DATA_HOME/heyo/extensions"

  # The palette as it opens: favourites and suggestions.
  show; w 2
  shot palette

  # Searching, and what can be done with a result.
  type "fi"; w 1.2; xdotool key ctrl+k; w 1.5
  shot search
  xdotool key Escape; w 0.5

  # The calculator converting a currency (the rates are fetched).
  clear; type "128 usd in eur"; w 5
  shot calculator

  # Clipboard History.
  away
  copies
  heyo --clipboard; w 4
  shot clipboard-history

  # The Raycast Store, and an extension installed from it.
  away
  heyo --open cmd:@heyo:store; w 8
  shot store
  first_key; type "tailwind css"; w 4; xdotool key Return
  for _ in $(seq 60); do [ -f "$EXT/tailwindcss/package.json" ] && break; w 1; done
  w 3; away
  heyo --open "cmd:$EXT/tailwindcss:search-colors"; w 6
  first_key; type "blue"; w 3
  shot extension

  # A live activity: the Timer, counting down at the top of the palette.
  away
  show; clear
  type "timer"; w 1; xdotool key Tab; w 0.4; type "25m"; w 0.8; xdotool key Return; w 3
  show; w 2
  shot live-activity

  # The settings window: 924 by 704 with the margin its shadow falls in
  # (src-tauri/src/slint_settings/ui/settings.slint), here at twice that.
  away
  heyo --open heyo:settings; w 5
  [ "$(size '^Heyo Settings$')" = "1848 1408" ] || echo "settings: the window is $(size '^Heyo Settings$'), not 1848 by 1408" >&2
  shot settings '^Heyo Settings$'

  grep -v "EGL\|dbind\|heyo: started\|heyo: renderer" "$T/app.log" | tail -n 5 | cut -c1-200
  [ -n "$KEEP" ] || python3 "$HERE/down.py" > /dev/null
done
[ -n "$KEEP" ] || exec "$HERE/readme-dictation.sh" "$@"

#!/bin/sh
# The pictures of the README, taken again: docs/images/<name>-<theme>.png.
#
#   readme-shots.sh [light|dark]     both when not given
#
# Each theme gets a sandbox of its own, started from nothing: a home
# directory with nothing personal in it, a short list of applications, a
# backdrop drawn for the purpose (readme-backdrop.py) and a little sample
# data, so that the pictures show Heyo and not the machine they were taken
# on. The session runs at twice the size (a 3840x2160 screen, GDK_SCALE=2),
# for pictures that stay sharp on a dense display; the README shows them at
# half their width.
#
# It needs the network (the Raycast Store, from which one extension is
# installed, and the exchange rates) and takes about three minutes a theme.
# The sandbox is in $HEYO_TEST_DIR (by default heyo-readme-shots in the
# temporary directory); what an earlier run left there is removed. KEEP=1
# leaves the last sandbox running, to look around in.
export HEYO_TEST_DIR="${HEYO_TEST_DIR:-${TMPDIR:-/tmp}/heyo-readme-shots}"
. "$(dirname "$0")/env.sh"
python3 "$HERE/down.py" > /dev/null; w 1
OUT="$REPO/docs/images"
mkdir -p "$OUT"

# Twice the size.
export SCREEN=3840x2160 GDK_SCALE=2
# The backdrop kept around a window, in pixels of the picture, and the
# radius the picture's corners are rounded to.
MARGIN=16
RADIUS=28

# A home with nothing in it: no path of whoever runs this can show.
export HOME="$T/home/user"
rm -rf "$HOME"; mkdir -p "$HOME"

# The applications in the pictures: a few that most GNOME desktops have,
# instead of everything installed here. The sandbox is given a data
# directory that is the system's with `applications` replaced. (Heyo adds
# the Snap and Flatpak directories itself, which is where Firefox is.)
APPS="org.gnome.Nautilus org.gnome.Terminal org.gnome.TextEditor org.gnome.Calculator
  org.gnome.Calendar org.gnome.Settings org.gnome.SystemMonitor org.gnome.Evince
  org.gnome.eog org.gnome.Rhythmbox3 org.gnome.Totem org.gnome.Characters
  org.gnome.font-viewer org.gnome.DiskUtility org.gnome.Logs org.gnome.baobab
  org.gnome.FileRoller org.gnome.Contacts org.gnome.Shotwell simple-scan
  libreoffice-writer libreoffice-calc libreoffice-impress transmission-gtk"
SHARE="$T/share"
rm -rf "$SHARE"; mkdir -p "$SHARE/applications"
for entry in /usr/share/*; do
  [ "$(basename "$entry")" = applications ] || ln -s "$entry" "$SHARE/"
done
for app in $APPS; do
  [ -f "/usr/share/applications/$app.desktop" ] && ln -s "/usr/share/applications/$app.desktop" "$SHARE/applications/"
done
export XDG_DATA_DIRS="$SHARE"

# The session's own settings, written before it starts (there is no bus to
# ask yet): the backdrop, and the icons every Ubuntu has rather than this
# machine's.
session() { # light|dark
  python3 "$HERE/readme-backdrop.py" "$1" 3840 2160 "$T/backdrop-$1.png"
  mkdir -p "$T/dconf.d" "$XDG_CONFIG_HOME/dconf"
  cat > "$T/dconf.d/readme" <<EOF
[org/gnome/desktop/background]
picture-uri='file://$T/backdrop-$1.png'
picture-uri-dark='file://$T/backdrop-$1.png'
picture-options='stretched'

[org/gnome/desktop/interface]
icon-theme='Yaru'
color-scheme='$([ "$1" = dark ] && echo prefer-dark || echo default)'
enable-animations=false
EOF
  dconf compile "$XDG_CONFIG_HOME/dconf/user" "$T/dconf.d"
}

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
  (gnome-text-editor > /dev/null 2>&1 & echo $! > "$T/editor.pid")
  (eog "$REPO/src-tauri/icons/icon.png" > /dev/null 2>&1 & echo $! > "$T/viewer.pid")
  # The terminal's own program ends, and its window with it, when the
  # copies are made.
  rm -f "$T/copied"
  (gnome-terminal -- sh -c 'while [ ! -f "$0" ]; do sleep 0.3; done' "$T/copied" > /dev/null 2>&1 &)
  w 6
  (/usr/bin/python3 - "$REPO/src-tauri/icons/icon.png" "$T/copied" <<'PY' > /dev/null 2>&1 &
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
  )
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
# size WINDOW TITLE: the width and height of the window, if it is there.
size() {
  id=$(xdotool search --onlyvisible --name "$1" | tail -1)
  [ -n "$id" ] && xwininfo -id "$id" | awk '/Width/{w=$2} /Height/{h=$2} END{print w, h}'
}
# shot NAME [WINDOW TITLE]: the window (the palette unless another is named)
# in the middle of the screen, with MARGIN of the backdrop around it and the
# corners of the picture rounded.
shot() {
  id=$(xdotool search --onlyvisible --name "${2:-^Heyo$}" | tail -1)
  if [ -z "$id" ]; then echo "$1: no window to take a picture of" >&2; return; fi
  set -- "$1" "$id" $(size "${2:-^Heyo$}")
  x=$(((3840 - $3) / 2)); y=$(((2160 - $4) / 2))
  xdotool windowmove "$2" "$x" "$y"; w 0.8
  python3 - "$OUT/$1-$THEME.png" $((x - MARGIN)) $((y - MARGIN)) $((x + $3 + MARGIN)) $((y + $4 + MARGIN)) "$RADIUS" <<'PY'
import os, sys
from PIL import Image, ImageDraw, ImageGrab
path, box, radius = sys.argv[1], tuple(map(int, sys.argv[2:6])), int(sys.argv[6])
image = ImageGrab.grab(xdisplay=os.environ["HEYO_TEST_DISPLAY"]).crop(box).convert("RGBA")
# The corners are cut at four times the size and brought down, for an edge
# that is smooth.
mask = Image.new("L", (image.width * 4, image.height * 4), 0)
ImageDraw.Draw(mask).rounded_rectangle((0, 0, mask.width - 1, mask.height - 1), radius * 4, fill=255)
image.putalpha(mask.resize(image.size, Image.Resampling.LANCZOS))
image.save(path, optimize=True)
print(f"{os.path.basename(path)}: {image.width}x{image.height}, {os.path.getsize(path) // 1024} KB")
PY
}

for THEME in ${*:-light dark}; do
  rm -rf "$T/home/config" "$T/home/data" "$T/home/cache"
  mkdir -p "$T/home/data" "$T/home/config/heyo" "$T/home/cache"
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

  # The settings window: 924 by 704 is what Heyo asks for
  # (commands/settings.rs), here at twice that. Under GNOME Shell it now
  # and then comes up taller, by the height of the title bar it does not
  # have; it is then closed and opened again, so that the two themes match.
  away
  for _ in 1 2 3 4; do
    heyo --open heyo:settings; w 5
    [ "$(size 'Heyo Settings')" = "1848 1408" ] && break
    xdotool key Escape; w 0.5; xdotool key Escape; w 1.5
  done
  shot settings 'Heyo Settings'

  grep -v "EGL\|dbind\|heyo: started\|heyo: renderer" "$T/app.log" | tail -n 5 | cut -c1-200
  [ -n "$KEEP" ] || python3 "$HERE/down.py" > /dev/null
done

#!/bin/sh
# The film that shows Heyo around, made in two stages:
#
#   demo-film.sh [capture|edit|all] [light|dark]     all and light when not given
#
# capture: the real Heyo in the sandbox's GNOME session (3840x2160 at scale
# 2), used as a person would use it, the whole screen filmed, and beside the
# film a list of moments: when each thing happened and where on the screen
# it is, asked of the windows themselves ($FILM/moments.jsonl). What is
# dictated is spoken by say.py (English) and by a kept take of say-zh.cjs
# (Chinese with English in it) in place of a microphone.
#
# edit: demo-film.py cuts that into the film: shots that begin on a beat of
# the music, a camera that moves in on what matters, the waiting left out,
# titles and captions drawn by itself, the music turned down under what is
# said. It says what it does and what it needs.
#
# What is made is in $HEYO_TEST_DIR/film ($FILM): capture.mkv and
# moments.jsonl, then heyo.mp4 and heyo.webm.
#
#   HEYO_FILM_CACHE    where what is fetched is kept from one run to the next
#                      (default: ~/.cache/heyo-build/film): the music, and
#                      the Python the edit runs under
#   HEYO_FILM_PYTHON   that Python; when not given, a virtual environment is
#                      made with uv in $HEYO_FILM_CACHE/venv, which also
#                      brings the ffmpeg both stages use (imageio-ffmpeg)
#   HEYO_FILM_SCENES   the scenes to capture, in the film's order (default:
#                      open dictation windows screenshot clipboard calculator
#                      store); for trying one
#   HEYO_FILM_SPEECH   the kept take of the Chinese sentences (default:
#                      demo-zh.wav beside the speech model say-zh.cjs uses);
#                      without one say-zh.cjs speaks them anew, and the
#                      recognizer may mishear that
#   KEEP=1             leaves the sandbox running after the capture
#
# It needs what readme-dictation.sh needs, uv (or HEYO_FILM_PYTHON), gedit,
# Files and the terminal of GNOME, and the network: the Raycast Store, the
# exchange rates, and the music (demo-film.py fetches it). The capture
# takes about five minutes, the edit about as long.
STAGE=${1:-all}; THEME=${2:-light}
case "$STAGE" in capture | edit | all) ;; *) sed -n '2,40p' "$0" | sed 's/^# \{0,1\}//'; exit 2 ;; esac
case "$THEME" in light | dark) ;; *) echo "demo-film: the theme is light or dark" >&2; exit 2 ;; esac
HERE=$(cd "$(dirname "$0")" && pwd)

# The Python of the edit, with what it needs, and its ffmpeg.
PACKAGES="numpy scipy pillow opencv-python-headless librosa imageio-ffmpeg fonttools brotli"
export HEYO_FILM_CACHE="${HEYO_FILM_CACHE:-$HOME/.cache/heyo-build/film}"
if [ -z "$HEYO_FILM_PYTHON" ]; then
  VENV="$HEYO_FILM_CACHE/venv"
  HEYO_FILM_PYTHON="$VENV/bin/python"
  if ! "$HEYO_FILM_PYTHON" -c 'import numpy, scipy, PIL, cv2, librosa, imageio_ffmpeg, fontTools, brotli' 2> /dev/null; then
    command -v uv > /dev/null || { echo "demo-film: uv is not installed (or name a Python that has $PACKAGES: HEYO_FILM_PYTHON)" >&2; exit 1; }
    [ -x "$HEYO_FILM_PYTHON" ] || uv venv -q "$VENV" || exit 1
    uv pip install -q --python "$HEYO_FILM_PYTHON" $PACKAGES || exit 1
  fi
fi
FFMPEG=$("$HEYO_FILM_PYTHON" -c 'import imageio_ffmpeg; print(imageio_ffmpeg.get_ffmpeg_exe())') || exit 1
export HEYO_FILM_PYTHON

if [ "$STAGE" = edit ]; then
  FILM="${HEYO_TEST_DIR:-${TMPDIR:-/tmp}/heyo-demo-film}/film"
  exec "$HEYO_FILM_PYTHON" "$HERE/demo-film.py" "$FILM"
fi

export HEYO_TEST_DIR="${HEYO_TEST_DIR:-${TMPDIR:-/tmp}/heyo-demo-film}"
. "$HERE/readme-env.sh"
[ -f "$HEYO_TEST_SPEECH/models/sense-voice/model.int8.onnx" ] || { echo "demo-film: no speech engine in $HEYO_TEST_SPEECH (Settings → Dictation downloads it)" >&2; exit 1; }
for needed in gedit nautilus gnome-terminal eog; do
  command -v "$needed" > /dev/null || { echo "demo-film: $needed is not installed" >&2; exit 1; }
done
FILM="$T/film"; rm -rf "$FILM"; mkdir -p "$FILM"
SCENES=${HEYO_FILM_SCENES:-open dictation windows screenshot clipboard calculator store}
unset ANTHROPIC_API_KEY ANTHROPIC_BASE_URL ANTHROPIC_AUTH_TOKEN
# What a GNOME session's settings tell the toolkits, and nothing here does.
export GTK_IM_MODULE=ibus QT_IM_MODULE=ibus XMODIFIERS=@im=ibus

# What is said. Heyo reads the file when a dictation begins, so it is given
# one recording, then the other. Each sentence is finished, so it is written.
SPOKEN="$T/spoken.wav"
ENGLISH="$FILM/spoken-en.wav"
CHINESE="$FILM/spoken-zh.wav"
ENGLISH_LENGTH=$(/usr/bin/python3 "$HERE/say.py" "$ENGLISH" \
  "We can have lunch by the lake.") || exit 1
# The voice is not the same twice, and the recognizer mishears some of its
# takes: one that is heard right is kept, and is then what is said.
TTS="${HEYO_TTS_MODEL:-$REAL_HOME/.cache/heyo-build/tts/vits-melo-tts-zh_en}"
KEPT="${HEYO_FILM_SPEECH:-$(dirname "$TTS")/demo-zh.wav}"
if [ -f "$KEPT" ]; then
  cp "$KEPT" "$CHINESE"
  CHINESE_LENGTH=$(/usr/bin/python3 -c 'import sys, wave; w = wave.open(sys.argv[1]); print(round(w.getnframes() / w.getframerate(), 1))' "$CHINESE") || exit 1
else
  CHINESE_LENGTH=$(HEYO_TTS_MODEL="$TTS" HEYO_TTS_LIB="$HEYO_TEST_SPEECH/node_modules" node "$HERE/say-zh.cjs" "$CHINESE" \
    "这个 feature 下周上线。" \
    "明天早上可以 review 一下。" \
    "有问题的话，直接发 email 给我。" \
    "中文和 English 混着说也可以。") || exit 1
fi
export HEYO_DICTATION_WAV="$SPOKEN"

# The applications the film uses, beside those of the README's pictures:
# gedit is the text editor there is (it takes the input method as GNOME's
# GTK 3 applications do), under the name GNOME gave it for years, so that a
# search for a text editor finds it.
rm -f "$SHARE/applications/org.gnome.TextEditor.desktop"
cat > "$SHARE/applications/org.gnome.gedit.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Text Editor
Comment=Edit text files
Exec=gedit --standalone %U
Icon=org.gnome.gedit
StartupWMClass=gedit
Categories=GNOME;GTK;Utility;TextEditor;
EOF

# note: what is in the editor before a word is said, with an empty line at
# its end to dictate on; and a home directory with something in it.
note() {
  NOTE="$HOME/Documents/weekend.txt"
  mkdir -p "$HOME/Documents" "$HOME/Pictures" "$HOME/Downloads" "$HOME/Music" "$HOME/Projects/heyo" "$HOME/Projects/website"
  printf '%s\n' "Weekend at the lake" "" "Saturday: the early train, ten past eight." "Sunday: back by seven in the evening." "" > "$NOTE"
  for name in tickets.pdf packing-list.txt route.gpx; do : > "$HOME/Documents/$name"; done
}

# looks light|dark: how the editor and the desktop look, written before the
# session starts. Ubuntu's dock is away: the film is of Heyo, and a window
# put on a half of the screen then has the half.
looks() {
  cat > "$T/dconf.d/film" <<EOF
[org/gnome/gedit/preferences/editor]
display-line-numbers=false
highlight-current-line=false
display-right-margin=false
wrap-mode='word'
use-default-font=false
editor-font='Noto Sans CJK SC 18'
scheme='$([ "$1" = dark ] && echo Yaru-dark || echo Yaru)'
restore-cursor-position=false
auto-save=false
create-backup-copy=false

[org/gnome/gedit/preferences/ui]
statusbar-visible=false
side-panel-visible=false

[org/gnome/desktop/interface]
gtk-theme='$([ "$1" = dark ] && echo Yaru-dark || echo Yaru)'
font-name='Cantarell 11'
cursor-blink=false

[org/gnome/desktop/notifications]
show-banners=false

[org/gnome/shell]
disabled-extensions=['ubuntu-dock@ubuntu.com']

[org/gnome/shell/extensions/dash-to-dock]
dock-fixed=false
autohide=false
intellihide=false

[org/gnome/terminal/legacy]
default-show-menubar=false
theme-variant='$([ "$1" = dark ] && echo dark || echo light)'
EOF
  # Ubuntu's colours for gedit are not loaded by the gedit it ships (46):
  # they lack the version it asks for. Here they have it.
  mkdir -p "$XDG_DATA_HOME/libgedit-gtksourceview-300/styles"
  for scheme in Yaru Yaru-dark; do
    [ -f "/usr/share/libgedit-gtksourceview-300/styles/$scheme.xml" ] &&
      sed 's/<style-scheme /<style-scheme version="1.0" /' "/usr/share/libgedit-gtksourceview-300/styles/$scheme.xml" > "$XDG_DATA_HOME/libgedit-gtksourceview-300/styles/$scheme.xml"
  done
  mkdir -p "$XDG_CONFIG_HOME/gtk-3.0"
  echo ".gedit-view { padding: 16px 24px 16px 24px; }" > "$XDG_CONFIG_HOME/gtk-3.0/gtk.css"
}

# frame ID: the window as it is seen, X Y W H. An application that draws
# its own title bar has a margin for its shadow inside its window.
frame() {
  python3 - "$1" <<'PY'
import re, subprocess, sys
info = subprocess.run(["xwininfo", "-id", sys.argv[1]], capture_output=True, text=True).stdout
at = lambda name: int(re.search(name + r":\s+(-?\d+)", info).group(1))
x, y, width, height = at("Absolute upper-left X"), at("Absolute upper-left Y"), at("Width"), at("Height")
extents = subprocess.run(["xprop", "-id", sys.argv[1], "_GTK_FRAME_EXTENTS"], capture_output=True, text=True).stdout
left, right, top, bottom = map(int, re.findall(r"\d+", extents.split("=")[-1])) if "=" in extents else (0, 0, 0, 0)
print(x + left, y + top, width - left - right, height - top - bottom)
PY
}
# put ID X Y W H: the window, as it is seen, there.
put() {
  set -- "$@" $(frame "$1") $(place "$1")
  xdotool windowsize "$1" $(($4 + ${12} - $8)) $(($5 + ${13} - $9)); w 0.6
  set -- "$1" "$2" "$3" $(frame "$1") $(place "$1")
  xdotool windowmove "$1" $(($2 - ($4 - $8))) $(($3 - ($5 - $9))); w 0.6
}
# shown CLASS: waits for a window of that class, and says which it is.
shown() {
  for _ in $(seq 60); do
    found=$(xdotool search --onlyvisible --class "$1" 2> /dev/null | tail -1)
    [ -n "$found" ] && { echo "$found"; return 0; }
    w 0.5
  done
  echo "demo-film: no window of $1 came" >&2
  return 1
}

# mark NAME [X Y W H]: a moment of the film: now, and where on the screen.
mark() {
  name=$1; shift
  printf '{"at": %s, "name": "%s", "box": [%s]}\n' "$(date +%s.%N)" "$name" "$(echo "$*" | tr -s ' ' ',')" >> "$FILM/moments.jsonl"
}
# mark_window NAME ID: that, with where the window is seen.
mark_window() { mark "$1" $(frame "$2"); }
palette() { xdotool search --onlyvisible --name '^Heyo$' 2> /dev/null | tail -1; }
# The palette's window is larger than the palette, for its shadow: 64 all
# around.
mark_palette() { set -- "$1" $(place "$(palette)"); [ $# -eq 5 ] && mark "$1" $(($2 + 64)) $(($3 + 64)) $(($4 - 128)) $(($5 - 128)); }
show() { for _ in 1 2 3 4 5 6; do [ -n "$(palette)" ] && break; heyo; w 2; done; }
# types TEXT [SECONDS]: typed a key at a time, that long apart, as a person
# types (what xdotool types in one go arrives all at once here).
types() {
  text=$1
  while [ -n "$text" ]; do
    rest=${text#?}
    xdotool type -- "${text%"$rest"}"
    text=$rest
    w "${2:-0.07}"
  done
}
clear() { xdotool key ctrl+a BackSpace; w 0.5; }
# In a command opened with `heyo --open` while the palette was away, the
# first key typed does not arrive in the search bar: one is spent first.
first_key() { xdotool type x; w 0.6; clear; }
# Out of whatever is open, to nothing of Heyo's on the screen.
away() { xdotool key Escape; w 0.4; xdotool key Escape; w 0.4; [ -n "$(palette)" ] && { xdotool key ctrl+a BackSpace Escape; w 0.4; }; w 0.6; }
# The pointer where the film does not look.
aside() { xdotool mousemove 3838 2158; }
# glide X Y X Y [STEPS]: the pointer moved from one place to the other as a
# hand moves it.
glide() {
  steps=${5:-12}
  for step in $(seq "$steps"); do
    xdotool mousemove $(($1 + ($3 - $1) * step / steps)) $(($2 + ($4 - $2) * step / steps)); w 0.03
  done
}

# Things copied, for Clipboard History: each in the application it would
# come from, which is what the history says it came from. One program makes
# all the copies and then keeps the clipboard, as an application that stays
# open does.
copies() {
  start_as copied-editor gedit --standalone > /dev/null 2>&1
  start_as copied-viewer eog "$REPO/src-tauri/icons/icon.png" > /dev/null 2>&1
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
    ("gnome-terminal", "pnpm tauri build"),
    ("gedit", "#1f86ff"),
    ("gnome-terminal", "ssh deploy@staging.example.com"),
    ("eog", None),
    ("gedit", "https://github.com/hdonix/heyo"),
    ("gnome-terminal", "git log --oneline --since=yesterday"),
    ("gedit", "Design review moved to Thursday 10:30, room 2B"),
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
  for _ in $(seq 60); do [ -f "$T/copied" ] && break; w 0.5; done
  # The applications go, so that the film opens on an empty desktop.
  stop copied-editor copied-viewer > /dev/null
  for _ in $(seq 20); do [ -z "$(xdotool search --onlyvisible --class gnome-terminal 2> /dev/null)" ] && break; w 0.5; done
  w 1
}

# --- The scenes. Each leaves the desktop as the next expects it. -------------

# The palette, a search, an application opened: the editor.
scene_open() {
  aside; w 0.6
  mark open-desktop
  heyo; w 0.3
  for _ in $(seq 30); do [ -n "$(palette)" ] && break; w 0.1; done
  mark_palette open-palette; w 1.2
  mark open-typing; types "edit" 0.16; mark open-typed; w 1
  mark_palette open-found
  xdotool key Return; mark open-enter
  EDITOR=$(shown gedit) || return 1
  mark_window open-editor "$EDITOR"; w 2
}

# The editor in the middle of the screen, with the note in it and the
# cursor at its end: where the dictation scene starts, and what the later
# scenes need of it when that one is left out.
editor_ready() {
  [ -n "$EDITOR" ] || { start_as editor gedit --standalone > /dev/null 2>&1; EDITOR=$(shown gedit) || return 1; w 1; }
  xdotool windowactivate "$EDITOR"; w 0.6
  put "$EDITOR" 1120 600 1600 1000
  xdotool windowactivate "$EDITOR"; w 0.6
  xdotool key ctrl+a BackSpace; w 0.3
  while IFS= read -r line; do
    [ -n "$line" ] && xdotool type --delay 12 -- "$line"
    xdotool key Return
  done < "$NOTE"
  w 0.8
}

# dictates WAV LENGTH NAME: a dictation of what the file says, begun and
# ended as a person does. When it began is marked with where the dictation
# window is, for the picture and for the sound.
dictates() {
  cp "$1" "$SPOKEN"
  heyo --dictate
  mark "$3-asked"
  w 0.7
  mark "$3-pill" $(ask wait "push button" Dictation "Heyo Dictation" 2> /dev/null)
  w "$2"; w 2.5
  mark "$3-said"
  heyo --dictate; w 2.5
  mark "$3-done"
}
scene_dictation() {
  editor_ready || return 1
  aside
  mark_window dictation-editor "$EDITOR"
  dictates "$ENGLISH" "$ENGLISH_LENGTH" english
  xdotool key Return Return; w 0.5
  dictates "$CHINESE" "$CHINESE_LENGTH" chinese
  mark_window dictation-end "$EDITOR"
}

# Three windows as they come, then each put in its place with a layout.
# Heyo arranges the window that is being worked in.
arrange() { # ID LAYOUT NAME
  xdotool windowactivate "$1"; w 0.7
  mark "$3-before"
  heyo --open "heyo:window-$2"; w 1.3
  mark_window "$3" "$1"
}
scene_windows() {
  [ -n "$EDITOR" ] || editor_ready || return 1
  start_as files nautilus --new-window "$HOME" > /dev/null 2>&1
  FILES=$(shown org.gnome.Nautilus) || return 1
  w 1.5
  start gnome-terminal --wait --working-directory="$HOME/Projects/heyo" -- env PS1='\[\e[1;34m\]~/Projects/heyo\[\e[0m\] \$ ' bash --norc --noprofile > /dev/null 2>&1
  TERMINAL=$(shown gnome-terminal) || return 1
  w 1.5
  # As windows are before anybody tidied them: one over the other.
  put "$FILES" 420 300 1500 900
  put "$TERMINAL" 2100 1050 1400 800
  put "$EDITOR" 1300 560 1500 1000
  xdotool windowactivate "$TERMINAL"; w 0.5
  xdotool type --delay 20 "ls ~/Documents"; xdotool key Return; w 0.5
  xdotool windowactivate "$EDITOR"; w 1
  aside
  mark windows-untidy
  # The first as a person does it: the palette, the layout's name, Enter.
  show; w 0.6; mark_palette windows-palette
  types "left half" 0.08; w 0.8; mark_palette windows-typed
  xdotool key Return; w 1.3
  mark_window windows-1 "$EDITOR"
  arrange "$FILES" right-half windows-2
  arrange "$FILES" top-right windows-3
  arrange "$TERMINAL" bottom-right windows-4
  arrange "$EDITOR" almost-maximize windows-5
  arrange "$EDITOR" maximize windows-6
  arrange "$EDITOR" left-half windows-7
  w 1
}

# A part of the screen selected, a rectangle, an arrow and a few words put
# on it, and the picture pinned: it stays above the windows while the work
# goes on beside it. The places are counted from the editor's corner: they
# are where its text is at the size looks() gives it.
button() { set -- $(toolbar "$1"); glide $PX $PY "$1" "$2" 8; PX=$1; PY=$2; w 0.3; xdotool click 1; w 0.4; }
scene_screenshot() {
  [ -n "$EDITOR" ] || editor_ready || return 1
  [ -n "$FILES" ] || { xdotool windowactivate "$EDITOR"; w 0.5; heyo --open heyo:window-left-half; w 1.3; }
  xdotool windowactivate "$EDITOR"; w 0.6
  set -- $(frame "$EDITOR")
  # The two lines of the note that say when: found from the editor's
  # corner, where its text begins.
  X0=$(($1 + 30)); Y0=$(($2 + 246)); X1=$(($1 + 1430)); Y1=$(($2 + 438))
  PX=$((X0 + 900)); PY=$((Y0 + 700)); xdotool mousemove $PX $PY; w 0.5
  mark screenshot-before
  heyo --open heyo:screenshot; w 1.6
  mark screenshot-overlay
  glide $PX $PY $X0 $Y0 10; w 0.2
  xdotool mousedown 1; w 0.15
  mark screenshot-drag
  glide $X0 $Y0 $X1 $Y1 16; w 0.2
  xdotool mouseup 1; PX=$X1; PY=$Y1; w 1
  mark screenshot-selected $X0 $Y0 $((X1 - X0)) $((Y1 - Y0))
  mark screenshot-toolbar $(toolbar_box)
  button rect
  # Around the hour, at the end of the first of the two lines.
  glide $PX $PY $((X0 + 567)) $((Y0 + 22)) 8; xdotool mousedown 1; w 0.1
  glide $((X0 + 567)) $((Y0 + 22)) $((X0 + 901)) $((Y0 + 98)) 10; xdotool mouseup 1; PX=$((X0 + 901)); PY=$((Y0 + 98)); w 0.6
  mark screenshot-rectangle
  button arrow
  glide $PX $PY $((X0 + 1025)) $((Y0 + 128)) 8; xdotool mousedown 1; w 0.1
  glide $((X0 + 1025)) $((Y0 + 128)) $((X0 + 912)) $((Y0 + 94)) 10; xdotool mouseup 1; PX=$((X0 + 912)); PY=$((Y0 + 94)); w 0.6
  mark screenshot-arrow
  button text
  glide $PX $PY $((X0 + 1040)) $((Y0 + 142)) 8; PX=$((X0 + 1040)); PY=$((Y0 + 142)); xdotool click 1; w 0.5
  types "Book this one" 0.07; w 0.4; xdotool key Return; w 0.8
  mark screenshot-text
  mark screenshot-pin-button $(ask wait "push button" Pin "Heyo Screenshot")
  button pin; w 1.5
  PIN=$(xdotool search --onlyvisible --name '^Heyo Pinned Screenshot$' 2> /dev/null | tail -1)
  [ -n "$PIN" ] || { echo "demo-film: nothing was pinned" >&2; return 1; }
  mark screenshot-pinned $(place "$PIN")
  # Taken by its middle to where three windows meet, over all of them.
  set -- $(place "$PIN")
  FROM_X=$(($1 + $3 / 2)); FROM_Y=$(($2 + $4 / 2)); TO_X=$((1250 + $3 / 2)); TO_Y=$((1010 + $4 / 2))
  glide $PX $PY $FROM_X $FROM_Y 8; w 0.3
  mark screenshot-carry
  xdotool mousedown 1; w 0.15
  glide $FROM_X $FROM_Y $TO_X $TO_Y 24; w 0.2
  xdotool mouseup 1; w 0.8
  mark screenshot-carried $(place "$PIN")
  # The work goes on beside it: the picture stays above the windows.
  xdotool windowactivate "$EDITOR"; w 0.5
  aside
  xdotool key ctrl+End; w 0.3
  mark_window screenshot-work "$EDITOR"
  xdotool key Return; types "Tickets: two seats by the window, booked." 0.05; w 1.2
  mark screenshot-worked $(place "$PIN")
  # Gone again with a double click, as it goes.
  xdotool mousemove $TO_X $TO_Y; w 0.3; xdotool click --repeat 2 --delay 80 1; w 0.8
  aside
  # The scenes after this one are the palette's, on an empty desktop.
  for id in $EDITOR $FILES $TERMINAL; do xdotool windowminimize "$id"; done; w 1
}

# Everything that was copied, and found again by what is remembered of it.
scene_clipboard() {
  aside
  mark clipboard-before
  heyo --clipboard; w 0.3
  for _ in $(seq 30); do [ -n "$(palette)" ] && break; w 0.1; done
  w 2.5
  mark_palette clipboard-open
  first_key
  mark clipboard-typing-1; types "links" 0.1; w 1.8; mark_palette clipboard-links
  clear
  mark clipboard-typing-2; types "from terminal" 0.08; w 1.8; mark_palette clipboard-terminal
  xdotool key Down; w 0.5; xdotool key Down; w 1
  mark_palette clipboard-end
  away
}

# The answer while the question is typed: a currency, a time zone.
scene_calculator() {
  aside
  show; w 1; clear
  mark_palette calculator-open
  mark calculator-typing-1; types "128 usd in eur" 0.09; w 4; mark_palette calculator-currency
  clear
  mark calculator-typing-2; types "3pm pst in beijing" 0.08; w 2; mark_palette calculator-time
  away
}

# An extension from the Raycast Store: searched, Enter, installed, running.
scene_store() {
  aside
  EXT="$XDG_DATA_HOME/heyo/extensions"
  heyo --open cmd:@heyo:store; w 7
  mark_palette store-open
  first_key
  mark store-typing; types "tailwind css" 0.09; w 3.5; mark_palette store-found
  xdotool key Return; mark store-enter
  for _ in $(seq 60); do [ -f "$EXT/tailwindcss/package.json" ] && break; w 1; done
  mark_palette store-installed; w 2.5
  mark_palette store-settled
  away
  heyo --open "cmd:$EXT/tailwindcss:search-colors"; w 5
  mark_palette store-running
  first_key
  mark store-typing-2; types "blue" 0.12; w 3; mark_palette store-blue
  away
}

# --- The capture -------------------------------------------------------------

home
looks "$THEME"
session "$THEME"
note
speech
printf '{ "appearance": "%s", "language": "en", "dictationLive": true, "dictationPolish": "off", "dictationHoldKey": "", "dictationLanguage": "auto", "favorites": ["app:org.gnome.Nautilus.desktop", "cmd:@heyo:clipboard-history", "heyo:screenshot"] }\n' "$THEME" > "$XDG_CONFIG_HOME/heyo/settings.json"
"$HERE/sandbox.sh" up gnome || exit 1
# The sandbox's own IBus, never the one of the desktop this is run from.
case "$(ibus address)" in *"$H/"*) ;; *) echo "demo-film: IBus is at $(ibus address), which is not the sandbox's" >&2; "$HERE/sandbox.sh" down > /dev/null; exit 1 ;; esac
# The accessibility bus told that somebody asks, before the film: the
# windows Slint draws answer from then on.
ask enable > /dev/null 2>&1
case " $SCENES " in *" clipboard "*) copies ;; esac

# The whole screen, with when each picture was taken kept as the clock said
# it (-copyts), counted from a second written down beside the film ($FILM/
# epoch): the moments are told by the same clock. Little is asked of
# the encoder, which shares the processors with a desktop drawn in software.
EPOCH=$(date +%s)
echo "$EPOCH" > "$FILM/epoch"
start_as film "$FFMPEG" -loglevel warning -f x11grab -draw_mouse 1 -framerate 30 -video_size 3840x2160 -i "$DISPLAY" \
  -copyts -output_ts_offset "-$EPOCH" -c:v libx264 -preset ultrafast -crf 14 -g 30 -pix_fmt yuv420p -threads 8 "$FILM/capture.mkv" > "$FILM/capture.log" 2>&1
CAMERA=$STARTED
w 2
# The film's clock against the moments': the pointer jumps, which is in
# the very next picture, and is marked as it does (demo-film.py finds the
# picture). Once more at the end, for a clock that drifted.
sync() { xdotool mousemove 3700 2000; w 1; mark "$1-before"; xdotool mousemove 3700 1700; mark "$1-after"; w 0.5; }
sync sync
mark begin
FAILED=
for scene in $SCENES; do
  "scene_$scene" || { echo "demo-film: the scene $scene did not go as written" >&2; FAILED=yes; break; }
done
mark end
sync sync-end
w 1
# Ended by its own signal for "that is all", so the file is closed whole.
kill -INT "$CAMERA" 2> /dev/null
for _ in $(seq 40); do alive film || break; w 0.5; done
grep "heyo: dictation\|input method\|panicked" "$T/app.log" | tail -n 6 | cut -c1-200 >&2
ls -la "$FILM/capture.mkv" | awk '{ print "demo-film: " $NF ", " int($5 / 1048576) " MB" }'
[ -n "$KEEP" ] || "$HERE/sandbox.sh" down > /dev/null
[ -z "$FAILED" ] || exit 1
[ "$STAGE" = capture ] || exec "$HEYO_FILM_PYTHON" "$HERE/demo-film.py" "$FILM"

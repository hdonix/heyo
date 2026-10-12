#!/bin/sh
# The README's picture of dictation, taken again:
# docs/images/dictation-<theme>.png.
#
#   readme-dictation.sh [light|dark]     both when not given
#
# A note is open in a text editor (gedit, which takes the input method as
# GNOME's GTK 3 applications do), and two sentences are dictated into it:
# spoken by say.py in place of a microphone, in a recording that ends in the
# middle of the second. Heyo goes on listening for the rest of it, so what
# is on the screen then stays as it is: the first sentence in the note, the
# second composed behind it as far as it was heard, and the dictation
# window with the level of the last words. That is the picture.
#
# It needs what readme-shots.sh needs except the network, and gedit, IBus
# (GNOME Shell starts it), Flite (libflite1) and the speech engine
# installed for your own Heyo, which is read and never written
# (HEYO_TEST_SPEECH names another directory that holds one). About a minute
# a theme. KEEP=1 leaves the last sandbox running, to look around in.
. "$(dirname "$0")/readme-env.sh"
[ -f "$HEYO_TEST_SPEECH/models/sense-voice/model.int8.onnx" ] || { echo "readme-dictation: no speech engine in $HEYO_TEST_SPEECH (Settings → Dictation downloads it)" >&2; exit 1; }
command -v gedit > /dev/null || { echo "readme-dictation: gedit is not installed" >&2; exit 1; }

# What is said: without numbers, which are heard now as words and now as
# digits. The second sentence stops where a listener still waits for more.
SPOKEN="$T/spoken.wav"
LENGTH=$(/usr/bin/python3 "$HERE/say.py" --open "$SPOKEN" \
  "We can have lunch by the lake." \
  "I will bring sandwiches and coffee for") || exit 1
export HEYO_DICTATION_WAV="$SPOKEN"
unset ANTHROPIC_API_KEY ANTHROPIC_BASE_URL ANTHROPIC_AUTH_TOKEN
# What a GNOME session's settings tell the toolkits, and nothing here does.
export GTK_IM_MODULE=ibus QT_IM_MODULE=ibus XMODIFIERS=@im=ibus

# The dictation window is 360 by 44 at the bottom of the screen, in the
# middle, a tenth of the screen's height up (Slint draws it:
# src-tauri/src/slint_windows/pill), and the editor
# is put where that is over the empty end of its page: its lower edge 152
# below the dictation window's upper one. It is as wide as the palette is
# seen to be in the other pictures, with as much of the backdrop around it:
# the palette's window has 64 of its own around it, for its shadow. In
# pixels of the screen, which is twice the size.
EDITOR_WIDTH=1500; EDITOR_HEIGHT=800
EDITOR_X=$(((3840 - EDITOR_WIDTH) / 2)); EDITOR_Y=$((2160 - 216 - 88 + 152 - EDITOR_HEIGHT))
AROUND=$((MARGIN + 64))

# note: the note as it is before a word is spoken, with an empty line at
# its end to dictate on.
note() {
  NOTE="$HOME/Documents/weekend.txt"
  mkdir -p "$HOME/Documents"
  printf '%s\n' "Weekend at the lake" "" "Saturday: the early train, ten past eight." "Sunday: back by seven in the evening." "" "" > "$NOTE"
}

# editor light|dark: how the editor looks, written before the session starts.
editor() {
  cat > "$T/dconf.d/editor" <<EOF
[org/gnome/gedit/preferences/editor]
display-line-numbers=true
highlight-current-line=false
display-right-margin=false
wrap-mode='word'
use-default-font=false
editor-font='Noto Sans Mono 11'
scheme='$([ "$1" = dark ] && echo Yaru-dark || echo Yaru)'
restore-cursor-position=false
auto-save=false
create-backup-copy=false

[org/gnome/gedit/preferences/ui]
statusbar-visible=false
side-panel-visible=false
EOF
  # Ubuntu's colours for gedit are not loaded by the gedit it ships (46):
  # they lack the version it asks for. Here they have it.
  mkdir -p "$XDG_DATA_HOME/libgedit-gtksourceview-300/styles"
  for scheme in Yaru Yaru-dark; do
    [ -f "/usr/share/libgedit-gtksourceview-300/styles/$scheme.xml" ] &&
      sed 's/<style-scheme /<style-scheme version="1.0" /' "/usr/share/libgedit-gtksourceview-300/styles/$scheme.xml" > "$XDG_DATA_HOME/libgedit-gtksourceview-300/styles/$scheme.xml"
  done
  # Without a settings daemon GTK reads its own file, not the session's
  # settings. The cursor does not blink, so that it is in every picture;
  # and the text has a little room around it.
  mkdir -p "$XDG_CONFIG_HOME/gtk-3.0"
  cat > "$XDG_CONFIG_HOME/gtk-3.0/settings.ini" <<EOF
[Settings]
gtk-theme-name=$([ "$1" = dark ] && echo Yaru-dark || echo Yaru)
gtk-application-prefer-dark-theme=$([ "$1" = dark ] && echo 1 || echo 0)
gtk-icon-theme-name=Yaru
gtk-font-name=Cantarell 11
gtk-cursor-blink=0
EOF
  echo ".gedit-view { padding: 12px 20px 12px 6px; }" > "$XDG_CONFIG_HOME/gtk-3.0/gtk.css"
}

# frame ID: the window as it is seen, LEFT TOP WIDTH HEIGHT. An application
# that draws its own title bar has a margin for its shadow inside its window.
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

FAILED=
for THEME in ${*:-light dark}; do
  home
  editor "$THEME"
  session "$THEME"
  note
  mkdir -p "$XDG_DATA_HOME/heyo"
  ln -sfn "$HEYO_TEST_SPEECH" "$XDG_DATA_HOME/heyo/dictation"
  printf '{ "appearance": "%s", "dictationLive": true, "dictationPolish": "off", "dictationHoldKey": "", "dictationLanguage": "en" }\n' "$THEME" > "$XDG_CONFIG_HOME/heyo/settings.json"
  "$HERE/up.sh" gnome
  # The sandbox's own IBus, never the one of the desktop this is run from.
  case "$(ibus address)" in *"$H/"*) ;; *) echo "readme-dictation: IBus is at $(ibus address), which is not the sandbox's" >&2; python3 "$HERE/down.py" > /dev/null; exit 1 ;; esac

  start gedit --standalone "$NOTE" > /dev/null 2>&1; echo "$STARTED" > "$T/editor.pid"
  id=
  for _ in $(seq 40); do id=$(xdotool search --onlyvisible --class gedit 2> /dev/null | tail -1); [ -n "$id" ] && break; w 0.5; done
  w 2
  # Sized and placed by what is seen of it; the cursor to the end.
  set -- $(frame "$id") $(xwininfo -id "$id" | awk '/Absolute upper-left X/{x=$4} /Absolute upper-left Y/{y=$4} /Width/{w=$2} /Height/{h=$2} END{print x, y, w, h}')
  xdotool windowsize "$id" $((EDITOR_WIDTH + $7 - $3)) $((EDITOR_HEIGHT + $8 - $4)); w 1
  xdotool windowmove "$id" $((EDITOR_X - ($1 - $5))) $((EDITOR_Y - ($2 - $6))); w 1
  xdotool windowactivate "$id"; w 1
  xdotool key ctrl+End; w 0.5

  # One recording, the first of this Heyo. All of it is heard a moment
  # after it was said, the speech engine being loaded meanwhile.
  heyo --dictate
  w "$LENGTH"; w 5
  set -- $(frame "$id")
  picture dictation $(($1 - AROUND)) $(($2 - AROUND)) $(($1 + $3 + AROUND)) $(($2 + $4 + AROUND))
  if [ -z "$(xdotool search --onlyvisible --name 'Heyo Dictation')" ] || grep -q "the rest is pasted\|not composed in the application\|dictation has nothing to write with" "$T/app.log"; then
    echo "readme-dictation: the words were not composed in the editor, and dictation-$THEME.png does not show what it should:" >&2
    FAILED=yes
  fi
  grep "heyo: dictation\|input method" "$T/app.log" | tail -n 4 | cut -c1-200 >&2

  if [ -z "$KEEP" ]; then
    heyo --dictate; w 2
    kill "$(cat "$T/editor.pid")" 2> /dev/null
    python3 "$HERE/down.py" > /dev/null
  fi
done
[ -z "$FAILED" ]

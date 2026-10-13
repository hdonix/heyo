#!/bin/sh
# A short film of Heyo at work, for the README and for showing it around:
# demo-video.sh [light|dark] writes $HEYO_TEST_DIR/demo-<theme>.webm.
#
# What is filmed is the real Heyo in the sandbox's GNOME session, used as a
# person would use it, one thing after another: a note dictated into
# (spoken by say.py in place of a microphone, so the same film comes out
# every time, and it has no sound), a search, the calculator, the clipboard
# history, a screenshot annotated and copied, the settings, the store. A
# line of words below says what is being shown (demo-caption.py). The
# screen is filmed by GStreamer (ximagesrc, VP8 in WebM: what Ubuntu has
# without anything installed), the middle of it: 2560 by 1800.
#
# It needs what readme-dictation.sh needs, and gst-launch-1.0. About a
# minute and a half a theme.
. "$(dirname "$0")/readme-env.sh"
[ -f "$HEYO_TEST_SPEECH/models/sense-voice/model.int8.onnx" ] || { echo "demo-video: no speech engine in $HEYO_TEST_SPEECH (Settings → Dictation downloads it)" >&2; exit 1; }
command -v gedit > /dev/null || { echo "demo-video: gedit is not installed" >&2; exit 1; }

# What is said: three sentences, each finished, so each is written.
SPOKEN="$T/spoken.wav"
LENGTH=$(/usr/bin/python3 "$HERE/say.py" "$SPOKEN" \
  "We can have lunch by the lake." \
  "I will bring sandwiches and coffee for everyone." \
  "Tell me if you would rather take the later train.") || exit 1
export HEYO_DICTATION_WAV="$SPOKEN"
unset ANTHROPIC_API_KEY ANTHROPIC_BASE_URL ANTHROPIC_AUTH_TOKEN
# What a GNOME session's settings tell the toolkits, and nothing here does.
export GTK_IM_MODULE=ibus QT_IM_MODULE=ibus XMODIFIERS=@im=ibus

# The dictation window is at the bottom of the screen, in the middle (Slint
# draws it: src-tauri/src/slint_windows/pill; Heyo makes it when it starts
# and keeps it off the screen, so the display can be asked where it is),
# and the editor is put where that is over the empty end of its page: its
# lower edge 152 below the dictation window's upper one. It is as wide as
# the palette is seen to be in the other pictures, with as much of the
# backdrop around it: the palette's window has 64 of its own around it,
# for its shadow. In pixels of the screen, which is twice the size.
EDITOR_WIDTH=1500; EDITOR_HEIGHT=800
EDITOR_X=$(((3840 - EDITOR_WIDTH) / 2))
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
  # What GTK looks like is the session's to say (its settings daemon
  # publishes it from these). The cursor does not blink, so that it is in
  # every picture; and the text has a little room around it.
  cat >> "$T/dconf.d/editor" <<EOF

[org/gnome/desktop/interface]
gtk-theme='$([ "$1" = dark ] && echo Yaru-dark || echo Yaru)'
font-name='Cantarell 11'
cursor-blink=false
EOF
  mkdir -p "$XDG_CONFIG_HOME/gtk-3.0"
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
  speech
  printf '{ "appearance": "%s", "dictationLive": true, "dictationPolish": "off", "dictationHoldKey": "", "dictationLanguage": "en" }\n' "$THEME" > "$XDG_CONFIG_HOME/heyo/settings.json"
  "$HERE/sandbox.sh" up gnome
  # The sandbox's own IBus, never the one of the desktop this is run from.
  case "$(ibus address)" in *"$H/"*) ;; *) echo "demo-video: IBus is at $(ibus address), which is not the sandbox's" >&2; "$HERE/sandbox.sh" down > /dev/null; exit 1 ;; esac

  hidden=$(xdotool search --name '^Heyo Dictation$' 2> /dev/null | tail -1)
  [ -n "$hidden" ] || { echo "demo-video: Heyo has no dictation window to ask where it will be" >&2; "$HERE/sandbox.sh" down > /dev/null; exit 1; }
  set -- $(place "$hidden")
  EDITOR_Y=$(($2 + 152 - EDITOR_HEIGHT))

  start_as editor gedit --standalone "$NOTE" > /dev/null 2>&1
  id=
  for _ in $(seq 40); do id=$(xdotool search --onlyvisible --class gedit 2> /dev/null | tail -1); [ -n "$id" ] && break; w 0.5; done
  w 2
  # Sized and placed by what is seen of it; the cursor to the end.
  set -- $(frame "$id") $(xwininfo -id "$id" | awk '/Absolute upper-left X/{x=$4} /Absolute upper-left Y/{y=$4} /Width/{w=$2} /Height/{h=$2} END{print x, y, w, h}')
  xdotool windowsize "$id" $((EDITOR_WIDTH + $7 - $3)) $((EDITOR_HEIGHT + $8 - $4)); w 1
  xdotool windowmove "$id" $((EDITOR_X - ($1 - $5))) $((EDITOR_Y - ($2 - $6))); w 1
  xdotool windowactivate "$id"; w 1
  xdotool key ctrl+End; w 0.5

  # Filmed: the middle of the screen, where the palette, the settings and
  # the editor all are, with a line of words below them saying what is seen.
  X0=640; Y0=340; X1=3199; Y1=2139
  OUT="$T/demo-$THEME.webm"
  CAPTION="$T/caption.txt"; : > "$CAPTION"
  start_as caption /usr/bin/python3 "$HERE/demo-caption.py" "$CAPTION" 1920 2076 > /dev/null 2>&1
  says() { printf '%s\n' "$1" > "$CAPTION"; }
  # Typed as a person types, into whatever has the keyboard.
  types() { xdotool type --delay "${2:-110}" "$1"; }
  again() { xdotool key ctrl+a; w 0.2; }
  start_as film gst-launch-1.0 -q -e ximagesrc display-name="$DISPLAY" use-damage=false show-pointer=true startx=$X0 starty=$Y0 endx=$X1 endy=$Y1 \
    ! video/x-raw,framerate=30/1 ! videoconvert ! vp8enc deadline=1 cpu-used=6 threads=12 target-bitrate=9000000 keyframe-max-dist=60 \
    ! webmmux ! filesink location="$OUT" > "$T/film.log" 2>&1
  FILM=$STARTED
  xdotool mousemove 3000 1500
  says "Heyo: a launcher for Linux you drive from the keyboard"; w 3

  says "Dictation: speak, and the words appear where you are typing. Recognized on your computer."
  heyo --dictate
  w "$LENGTH"; w 3.5
  heyo --dictate; w 2.5

  says "Search applications and commands"
  heyo; w 1.5
  types "fire"; w 1.8
  again; types "term"; w 1.8

  says "Calculate and convert as you type"
  again; types "15% of 80"; w 2.2
  again; types "5 km in mi"; w 2.2
  again; types "3pm pst in beijing" 80; w 2.5

  says "Clipboard History: everything you copied, searchable"
  xdotool key Escape; w 0.3; xdotool key Escape; w 0.8
  xdotool windowactivate "$id"; w 0.5
  xdotool key ctrl+a ctrl+c; w 0.5; xdotool key ctrl+End; w 0.4
  heyo --open "cmd:@heyo:clipboard-history"; w 3.5
  xdotool key Escape; w 0.3; xdotool key Escape; w 1

  says "Screenshots: select an area, annotate it, copy, save or pin it"
  heyo --open heyo:screenshot; w 1.8
  xdotool mousemove 1250 620 mousedown 1; w 0.2
  for step in 1 2 3 4 5 6 7 8 9 10; do xdotool mousemove $((1250 + step * 110)) $((620 + step * 60)); w 0.05; done
  xdotool mouseup 1; w 1.2
  button() { set -- $(python3 "$HERE/layout.py" toolbar "$1" 2350 1220 2); xdotool mousemove "$1" "$2"; w 0.4; xdotool click 1; w 0.4; }
  button rect
  xdotool mousemove 1330 700 mousedown 1; w 0.1; xdotool mousemove 1500 800; w 0.1; xdotool mousemove 1760 900; w 0.1; xdotool mouseup 1; w 0.7
  button arrow
  xdotool mousemove 2200 720 mousedown 1; w 0.1; xdotool mousemove 2000 800; w 0.1; xdotool mousemove 1800 880; w 0.1; xdotool mouseup 1; w 0.7
  button text
  xdotool mousemove 1900 1050 click 1; w 0.5; types "Dictated, not typed"; w 0.4; xdotool key Return; w 1.2
  button copy; w 1.5

  says "Settings, drawn natively: on screen in a fifteenth of a second"
  heyo --open heyo:settings; w 2
  for page in Shortcuts Dictation Screenshots General; do
    ask click "list item" "$page" "Heyo Settings" > /dev/null 2>&1; w 1.4
  done
  xdotool key Escape; w 1

  says "And extensions written for Raycast, from its store"
  heyo; w 1.2; types "store"; w 2.5
  xdotool key Escape; w 0.3; xdotool key Escape; w 0.8

  xdotool mousemove 3000 1500
  says "Heyo  ·  github.com/hdonix/heyo"; w 3.5
  # Ended by its own signal for "that is all", so the file is closed whole.
  kill -INT "$FILM" 2> /dev/null; w 2.5
  stop caption > /dev/null
  ls -la "$OUT" | awk '{ print "demo-video: " $NF ", " int($5 / 1024) " KB" }'
  grep "heyo: dictation\|input method" "$T/app.log" | tail -n 3 | cut -c1-200 >&2

  if [ -z "$KEEP" ]; then
    stop editor > /dev/null
    "$HERE/sandbox.sh" down > /dev/null
  fi
done
[ -z "$FAILED" ]

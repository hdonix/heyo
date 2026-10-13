#!/bin/sh
# A short film of Heyo at work, for the README and for showing it around:
# demo-video.sh [light|dark] writes $HEYO_TEST_DIR/demo-<theme>.webm.
#
# What is filmed is the real Heyo in the sandbox's GNOME session, used as a
# person would use it, one thing after another: a note dictated into, in
# English and then in Chinese with English in it (spoken by say.py and
# say-zh.cjs in place of a microphone, so the same film comes out every
# time; what they say is the film's sound), a search, the calculator, the
# clipboard history, a screenshot annotated and copied, the settings, the
# store. A line of words below says what is being shown (demo-caption.py),
# in English, or in Chinese with HEYO_DEMO_CAPTIONS=zh, which names the
# film demo-<theme>-zh.webm. The screen is filmed by GStreamer (ximagesrc,
# VP8 and Opus in WebM: what Ubuntu has without anything installed), the
# middle of it: 2560 by 1800.
#
# It needs what readme-dictation.sh needs, gst-launch-1.0, what say-zh.cjs
# needs (a speech model: see there), and the network (the Raycast Store,
# from which one extension is installed). About two minutes a theme.
. "$(dirname "$0")/readme-env.sh"
[ -f "$HEYO_TEST_SPEECH/models/sense-voice/model.int8.onnx" ] || { echo "demo-video: no speech engine in $HEYO_TEST_SPEECH (Settings → Dictation downloads it)" >&2; exit 1; }
command -v gedit > /dev/null || { echo "demo-video: gedit is not installed" >&2; exit 1; }

# What is said, twice: sentences each finished, so each is written. Heyo
# reads the file when a dictation begins, so it is given one, then the other.
SPOKEN="$T/spoken.wav"
ENGLISH="$T/spoken-en.wav"
CHINESE="$T/spoken-zh.wav"
ENGLISH_LENGTH=$(/usr/bin/python3 "$HERE/say.py" "$ENGLISH" \
  "We can have lunch by the lake." \
  "I will bring sandwiches and coffee for everyone.") || exit 1
# The voice is not the same twice, and the recognizer mishears some of its
# takes: one that is heard right can be kept and given as HEYO_DEMO_CHINESE
# (default: demo-zh.wav beside the speech model), and is then what is said.
TTS="${HEYO_TTS_MODEL:-$REAL_HOME/.cache/heyo-build/tts/vits-melo-tts-zh_en}"
KEPT="${HEYO_DEMO_CHINESE:-$(dirname "$TTS")/demo-zh.wav}"
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
case "${HEYO_DEMO_CAPTIONS:-en}" in en) NAMED= ;; zh) NAMED=-zh ;; *) echo "demo-video: HEYO_DEMO_CAPTIONS is en or zh" >&2; exit 2 ;; esac
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
editor-font='Noto Sans Mono CJK SC 11'
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
  printf '{ "appearance": "%s", "dictationLive": true, "dictationPolish": "off", "dictationHoldKey": "", "dictationLanguage": "auto" }\n' "$THEME" > "$XDG_CONFIG_HOME/heyo/settings.json"
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
  OUT="$T/demo-$THEME$NAMED.webm"; SILENT="$T/silent.webm"; SOUND="$T/sound.wav"; HEARD="$T/heard.txt"; : > "$HEARD"
  CAPTION="$T/caption.txt"; : > "$CAPTION"
  start_as caption /usr/bin/python3 "$HERE/demo-caption.py" "$CAPTION" 1920 2076 > /dev/null 2>&1
  # says ENGLISH CHINESE: the line below, in the language asked for.
  says() { if [ -n "$NAMED" ]; then printf '%s\n' "$2"; else printf '%s\n' "$1"; fi > "$CAPTION"; }
  # dictates WAV LENGTH: a dictation of what the file says, begun and ended
  # as a person does; when it began is kept, for the sound.
  dictates() {
    cp "$1" "$SPOKEN"
    heyo --dictate
    echo "$(date +%s.%N) $1" >> "$HEARD"
    w "$2"; w 3
    heyo --dictate; w 2.5
  }
  # Typed as a person types, into whatever has the keyboard.
  types() { xdotool type --delay "${2:-110}" "$1"; }
  again() { xdotool key ctrl+a; w 0.2; }
  start_as film gst-launch-1.0 -q -e ximagesrc display-name="$DISPLAY" use-damage=false show-pointer=true startx=$X0 starty=$Y0 endx=$X1 endy=$Y1 \
    ! video/x-raw,framerate=30/1 ! videoconvert ! vp8enc deadline=1 cpu-used=6 threads=12 target-bitrate=9000000 keyframe-max-dist=60 \
    ! webmmux ! filesink location="$SILENT" > "$T/film.log" 2>&1
  FILM=$STARTED; BEGAN=$(date +%s.%N)
  xdotool mousemove 3000 1500
  says "Heyo: a launcher for Linux you drive from the keyboard" "Heyo：Linux 上用键盘驱动的启动器"; w 3

  says "Dictation: speak, and the words appear where you are typing. Recognized on your computer." "听写：开口说，文字出现在你正在输入的地方。识别在本机完成。"
  dictates "$ENGLISH" "$ENGLISH_LENGTH"
  xdotool key Return Return; w 0.5
  says "Chinese, with English in it, without changing a setting" "中文夹着英文说也行，不用切换任何设置"
  dictates "$CHINESE" "$CHINESE_LENGTH"

  says "Search applications and commands" "搜索应用和命令"
  heyo; w 1.5
  types "fire"; w 1.8
  again; types "term"; w 1.8

  says "Calculate and convert as you type" "边输入边计算和换算"
  again; types "15% of 80"; w 2.2
  again; types "5 km in mi"; w 2.2
  again; types "3pm pst in beijing" 80; w 2.5

  says "Clipboard History: everything you copied, searchable" "剪贴板历史：复制过的内容都能搜到"
  xdotool key Escape; w 0.3; xdotool key Escape; w 0.8
  xdotool windowactivate "$id"; w 0.5
  xdotool key ctrl+a ctrl+c; w 0.5; xdotool key ctrl+End; w 0.4
  heyo --open "cmd:@heyo:clipboard-history"; w 3.5
  xdotool key Escape; w 0.3; xdotool key Escape; w 1

  says "Screenshots: select an area, annotate it, copy, save or pin it" "截图：框选、标注，然后复制、保存或钉在屏幕上"
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

  says "Settings, drawn natively: on screen in a fifteenth of a second" "原生绘制的设置窗口：十五分之一秒内出现"
  heyo --open heyo:settings; w 2
  for page in Shortcuts Dictation Screenshots General; do
    ask click "list item" "$page" "Heyo Settings" > /dev/null 2>&1; w 1.4
  done
  xdotool key Escape; w 1

  says "Extensions written for Raycast, from its store: search, press Enter, installed" "Raycast 商店里的扩展：搜索，按回车，装好"
  EXT="$XDG_DATA_HOME/heyo/extensions"
  heyo --open cmd:@heyo:store; w 6
  xdotool type x; w 0.6; xdotool key ctrl+a BackSpace; w 0.6
  types "tailwind css"; w 3.5; xdotool key Return
  for _ in $(seq 60); do [ -f "$EXT/tailwindcss/package.json" ] && break; w 1; done
  w 2.5
  says "The extension runs as it is: its own lists and grids, drawn by Heyo" "扩展原样运行：它的列表和网格由 Heyo 绘制"
  heyo --open "cmd:$EXT/tailwindcss:search-colors"; w 5
  types "blue"; w 3
  xdotool key Escape; w 0.3; xdotool key Escape; w 0.3; xdotool key Escape; w 0.8

  xdotool mousemove 3000 1500
  says "Heyo  ·  github.com/hdonix/heyo" "Heyo  ·  github.com/hdonix/heyo"; w 3.5
  # Ended by its own signal for "that is all", so the file is closed whole.
  kill -INT "$FILM" 2> /dev/null; w 2.5
  stop caption > /dev/null
  # The sound: what was said, where in the film it was said. A dictation
  # is listening a little after it is asked for.
  /usr/bin/python3 - "$SOUND" "$BEGAN" "$(date +%s.%N)" "$HEARD" <<'PY'
import array, sys, wave
out, began, ended, heard = sys.argv[1], float(sys.argv[2]), float(sys.argv[3]), sys.argv[4]
RATE, LATE = 16000, 0.5
sound = array.array("h", [0] * int((ended - began) * RATE))
for line in open(heard):
    at, spoken = line.split(None, 1)
    with wave.open(spoken.strip()) as w:
        said = array.array("h", w.readframes(w.getnframes()))
    start = int((float(at) - began + LATE) * RATE)
    said = said[: max(0, len(sound) - start)]
    sound[start : start + len(said)] = said
with wave.open(out, "wb") as w:
    w.setnchannels(1); w.setsampwidth(2); w.setframerate(RATE)
    w.writeframes(sound.tobytes())
PY
  gst-launch-1.0 -q filesrc location="$SILENT" ! matroskademux ! queue ! webmmux name=film ! filesink location="$OUT" \
    filesrc location="$SOUND" ! wavparse ! audioconvert ! audioresample ! opusenc bitrate=64000 ! queue ! film. > "$T/sound.log" 2>&1 ||
    { echo "demo-video: the sound could not be put in the film ($T/sound.log)" >&2; cp "$SILENT" "$OUT"; FAILED=1; }
  ls -la "$OUT" | awk '{ print "demo-video: " $NF ", " int($5 / 1024) " KB" }'
  grep "heyo: dictation\|input method" "$T/app.log" | tail -n 3 | cut -c1-200 >&2

  if [ -z "$KEEP" ]; then
    stop editor > /dev/null
    "$HERE/sandbox.sh" down > /dev/null
  fi
done
[ -z "$FAILED" ]

#!/bin/sh
# Heyo used, the display changed under it while it runs, and Heyo used again:
#
#   display-change.sh FROM TO [NAME] [VAR=value…]   what was seen goes to $RESULTS/display-NAME.txt,
#                                                   a picture of the overlay to $RESULTS/display-NAME.png
#
#   FROM   the session's scale Heyo is started at, 1 or 2
#   TO     the scale it goes to; "same" for no change at all; WIDTHxHEIGHT
#          for the screen becoming that size (the Shell then lays it out
#          anew); or monitors:N for the screen becoming N monitors side by side
#
# EXPECTED: every line "ok", and it ends "all as it should be", in both
# directions, since 1bb3f51 (Slint's windows are made again when the display
# changes) and 4d2a6e9 (the palette centred after it). Before those it
# failed as on a real desktop where a monitor was connected while Heyo ran:
# the screenshot's toolbar half size, a pointer of 24 over Slint's windows
# on a session of 48, and clicks landing elsewhere. It did not fail in a
# sandbox that said the scale in the environment (WINIT_X11_SCALE_FACTOR),
# which is why this one publishes it as a session does and nowhere else.
#
# Each time, every window is used once and what the screen shows is said
# and judged: the settings window's size, and a switch in it clicked where
# the window says it is, the button held a moment (the stored setting must
# change); the palette's place, twice (the middle both times); a
# screenshot's toolbar as drawn, and a part saved (the file is that part of
# the screen, pixel for pixel, at any scale); a capture pinned (its device
# pixels, where it was), and the one pinned before the change still there;
# the pill's window; and over each of them the pointer's size, which must
# be what it is over a GTK window of the same session.
#
#   KEEP_SETTINGS=closed   the settings window is closed while the display changes
#   FIRST=no               nothing is used before the change: every window is
#                          first made after it
#
# In the session that runs, or one of its own (GNOME Shell, 3840x2160),
# which is left at the scale gone to; Heyo must not be running in it.
. "$(dirname "$0")/env.sh"
from=${1:?FROM}; to=${2:?TO}; name=${3:-$1-to-$2}
[ $# -gt 3 ] && shift 3 || shift $#
export SCREEN=${SCREEN:-3840x2160}
session_for gnome
out=$RESULTS/display-$name.txt
READ="/usr/bin/python3 $GUI/screen-read.py"
geometry() { set -- $(place "$1"); echo "${3}x${4}+${1}+${2}"; }
over() { xdotool mousemove "$1" "$2"; sleep 0.4; $READ cursor; }
# middle X Y W H: the middle of a rectangle.
middle() { echo $(($1 + $3 / 2)) $(($2 + $4 / 2)); }
pins() { xdotool search --onlyvisible --name '^Heyo Pinned Screenshot$' 2>/dev/null; }
# Dictation hears silence instead of a microphone.
speech
python3 -c "
import wave, sys
w = wave.open(sys.argv[1], 'wb'); w.setnchannels(1); w.setsampwidth(2); w.setframerate(16000); w.writeframes(b'\0\0' * 16000 * 6); w.close()" "$T/silence.wav"
export HEYO_DICTATION_WAV="$T/silence.wav"
FAILED="$T/display-failed"; rm -f "$FAILED"
# check WHAT COMMAND…: says ok or FAIL for it.
check() {
  what=$1; shift
  if "$@"; then echo "  ok    $what"; else echo "  FAIL  $what"; echo "$when: $what" >> "$FAILED"; fi
}
same() { [ "$1" = "$2" ]; }
says() { ask where "$@" > /dev/null 2>&1; }

# use WHEN: every window once.
use() {
  when=$1
  scale=$(scale)
  screen=$(xdpyinfo | awk '/dimensions/{print $2}'); sw=${screen%x*}; sh=${screen#*x}
  echo "## $when: scale $scale, Xcursor.size $(xrdb -query | awk '/^Xcursor.size/{print $2}'), screen $screen"
  start_as gtk zenity --info --title "A window" --text "A window" --width 300 --height 150 > /dev/null 2>&1
  sleep 2
  eval "$(xdotool getwindowgeometry --shell "$(xdotool search --name '^A window$' | tail -1)")"
  gtk=$(over $((X + WIDTH / 2)) $((Y + 30 * scale)))
  echo "pointer over a GTK window: $gtk"
  size=${gtk##* }
  stop gtk > /dev/null

  for pin in $(pins); do
    eval "$(xdotool getwindowgeometry --shell "$pin")"
    pointer=$(over $((X + WIDTH / 2)) $((Y + HEIGHT / 2)))
    echo "the capture pinned before: $(geometry "$pin"); pointer over it: $pointer"
    check "the pointer over it is the session's size" same "${pointer##* }" "$size"
    xdotool click 3; sleep 0.5
    check "a right click closes it" same "$(pins | wc -l)" 0
  done

  heyo --open heyo:settings > /dev/null 2>&1; sleep 2.5
  s=$(window '^Heyo Settings$')
  set -- $(place "$s")
  # On a screen too small for the shadow's margin the window is its panel.
  if [ "$3" -lt $((924 * scale)) ]; then wanted="$((860 * scale)) $((640 * scale))"; else wanted="$((924 * scale)) $((704 * scale))"; fi
  echo "settings: $(geometry "$s")"
  check "860 by 640 logical, with the margin of 32 where there is room: $(echo "$wanted" | tr ' ' 'x')" same "$3 $4" "$wanted"
  # Where the window itself says it is, and where its switch is.
  check "the window says it is where the screen has it" same "$(ask where frame "Heyo Settings")" "$1 $2 $3 $4"
  lines=$(over $(middle $(ask where "list box" "" "Heyo Settings")))
  switch=$(ask where "toggle button" "Hide When Focus Is Lost" "Heyo Settings")
  on=$(over $(middle $switch))
  echo "  pointer over its lines: $lines; over a switch, which the window says is at $switch: $on"
  check "the pointer over the settings is the session's size" same "${lines##* } ${on##* }" "$size $size"
  check "the switch is 36 logical pixels wide" same "$(echo "$switch" | cut -d' ' -f3)" $((36 * scale))
  before=$(stored hideOnBlur)
  # As a hand clicks: the button is down for a moment.
  xdotool mousedown 1; sleep 0.15; xdotool mouseup 1; sleep 0.8
  echo "  a held click on that switch: hideOnBlur $before -> $(stored hideOnBlur)"
  check "the click took effect" [ "$before" != "$(stored hideOnBlur)" ]

  for time in first second; do
    heyo > /dev/null 2>&1; sleep 1.5
    set -- $(place "$(window '^Heyo$')")
    echo "palette, the $time time: ${3}x${4}+${1}+${2}"
    check "the palette is in the middle (+$(((sw - $3) / 2))+$(((sh - $4) / 2)))" same "$1 $2" "$(((sw - $3) / 2)) $(((sh - $4) / 2))"
    heyo > /dev/null 2>&1; sleep 0.8
  done

  # A screenshot: 1200 by 700 from 1000,600, saved.
  rm -rf "$HOME/Pictures/Screenshots"
  xdotool mousemove 300 $((sh - 200)); sleep 0.3
  png "$T/display-before.png"
  heyo --open heyo:screenshot > /dev/null 2>&1; sleep 1.5
  xdotool mousemove 1000 600 mousedown 1; sleep 0.1; xdotool mousemove 1600 900; sleep 0.1; xdotool mousemove 2200 1300; sleep 0.2; xdotool mouseup 1; sleep 0.4
  inside=$(over 1500 900); handle=$(over 2200 1300); bar=$(over $(toolbar green 2200 1300))
  echo "screenshot: pointer in the selection: $inside; on a handle: $handle; on the toolbar: $bar"
  check "the pointer over the overlay is the session's size" same "${inside##* } ${handle##* } ${bar##* }" "$size $size $size"
  check "the overlay says the selection is 1200 by 700 of the screen's pixels" says label "1200 × 700" "Heyo Screenshot"
  xdotool mousemove 1500 900; sleep 0.3
  png "$T/display-overlay.png"
  [ "$when" = before ] || png "$RESULTS/display-$name.png" 900 500 1400 $((860 + 60 * scale))
  # The toolbar as drawn, measured in its padding, clear of its buttons and
  # its round corners: all of its width but the corners.
  set -- $(python3 "$GUI/layout.py" toolbar-box 2200 1300 "$scale")
  set -- "$3" $(python3 "$GUI/toolbar.py" "$T/display-overlay.png" $(($1 + $3 / 2)) $(($2 + 2 * scale)))
  echo "  toolbar: $2 px across where it was measured, $3 px down to the next button"
  check "the toolbar is drawn $1 px wide (but for its corners)" [ "$2" -le "$1" -a "$2" -ge $(($1 * 95 / 100)) ]
  xdotool key ctrl+s; sleep 1.5
  file=$(ls -t "$HOME/Pictures/Screenshots/"*.png 2>/dev/null | head -1)
  check "the part was saved" [ -n "$file" ]
  if [ -n "$file" ]; then
    compared=$($READ same "$T/display-before.png" "$file" 1000 600 1200 700); matched=$?
    echo "  saved against the screen there: $compared"
    check "the file is that part of the screen, pixel for pixel" [ "$matched" = 0 ]
  else
    xdotool key Escape
  fi

  # Another, 300 by 200 from 400,300, pinned.
  heyo --open heyo:screenshot > /dev/null 2>&1; sleep 1.5
  xdotool mousemove 400 300 mousedown 1; sleep 0.1; xdotool mousemove 550 400; sleep 0.1; xdotool mousemove 700 500; sleep 0.2; xdotool mouseup 1; sleep 0.3
  xdotool mousemove $(toolbar pin 700 500); sleep 0.15; xdotool click 1; sleep 1.5
  pin=$(pins | tail -1)
  if [ -n "$pin" ]; then
    pointer=$(over 550 400)
    echo "pinned 300x200 from 400,300: a window at $(geometry "$pin"); pointer over it: $pointer"
    check "the capture is pinned where it was taken, in the screen's pixels" same "$(geometry "$pin")" "300x200+400+300"
    check "the pointer over it is the session's size" same "${pointer##* }" "$size"
  else
    check "a capture was pinned" false; xdotool key Escape
  fi

  heyo --dictate > /dev/null 2>&1; sleep 1.2
  d=$(pill)
  if [ -n "$d" ]; then
    pointer=$(over $(middle $(place "$d")))
    echo "pill: its window $(geometry "$d"); pointer over it: $pointer"
    check "the pill is 360 by 44 logical at the bottom centre, a tenth up" same "$(geometry "$d")" "$((360 * scale))x$((44 * scale))+$(((sw - 360 * scale) / 2))+$((sh - 44 * scale - sh / 10))"
    check "the pointer over it is the session's size" same "${pointer##* }" "$size"
  else
    check "the pill came up" false
  fi
  xdotool mousemove 300 $((sh - 200))
  heyo --dictate > /dev/null 2>&1
  for _ in $(seq 100); do [ -z "$(pill)" ] && break; sleep 0.1; done
  if [ "${KEEP_SETTINGS:-open}" = closed ]; then
    set -- $(place "$s")
    xdotool mousemove $(($1 + $3 / 2)) $(($2 + 300 * scale)) click 1; sleep 0.3; xdotool key Escape; sleep 0.8
    echo "the settings closed with Escape: $(window '^Heyo Settings$' | wc -l) on screen, $(pins | wc -l) pinned"
  fi
}

{
  "$GUI/sandbox.sh" scale "$from" > /dev/null || exit 1
  echo "# $(date +%F), $BUILD, started at scale $from, then: $to${KEEP_SETTINGS:+, the settings $KEEP_SETTINGS meanwhile}"
  seed > /dev/null
  HEYO_TEST_WARMUP=${HEYO_TEST_WARMUP:-4} "$GUI/sandbox.sh" heyo "$@" || exit 1
  [ "${FIRST:-yes}" = yes ] && use before
  was=$(xdpyinfo | awk '/dimensions/{print $2}')
  case $to in
    same) ;;
    monitors:*)
      "$GUI/sandbox.sh" monitors "${to#monitors:}" > /dev/null; sleep 4
      use "after the screen became ${to#monitors:} monitors"
      "$GUI/sandbox.sh" monitors 1 > /dev/null
      ;;
    *x*)
      "$GUI/sandbox.sh" screen "$to" > /dev/null; sleep 4
      use "after the screen became $to"
      "$GUI/sandbox.sh" screen "$was" > /dev/null
      ;;
    *)
      "$GUI/sandbox.sh" scale "$to" > /dev/null || exit 1
      sleep 3
      use "after the change"
      ;;
  esac
  echo "## what Heyo said"
  grep -E "heyo: (the display|the pointer|panicked|.*could not|.*cannot)" "$T/app.log" | sort | uniq -c
  for pin in $(pins); do xdotool mousemove $(middle $(place "$pin")) click 3; done
  xdotool key Escape
  stop heyo > /dev/null
  if [ -s "$FAILED" ]; then echo "FAILED: $(tr '\n' ';' < "$FAILED")"; else echo "all as it should be"; fi
} 2>&1 | tee "$out"
leave
[ ! -s "$FAILED" ]

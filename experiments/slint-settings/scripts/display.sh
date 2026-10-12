#!/bin/sh
# Heyo used, the display changed under it while it runs, and Heyo used again:
#
#   display.sh FROM TO [NAME] [VAR=value…]    what was seen goes to $RESULTS/display-NAME.txt,
#                                             a picture of the overlay to $RESULTS/display-NAME.png
#
#   FROM   the session's scale Heyo is started at, 1 or 2
#   TO     the scale it goes to; "same" for no change at all; or WIDTHxHEIGHT
#          for the screen becoming that size (the Shell then lays it out
#          anew, and may choose another scale with it)
#
# Each time, every window is used once and what the screen shows is said:
# the settings window's size, and a switch in it clicked (the stored setting
# must change); the palette's place, twice (the middle both times); a
# screenshot's toolbar as drawn, and a part saved (the file is that part of
# the screen, pixel for pixel, at any scale); a capture pinned (its device
# pixels, where it was), and the one pinned before the change still there;
# the pill's window; and over each of them the pointer's size, which must
# be what it is over a GTK window.
#
#   KEEP=closed   the settings window is closed while the display changes
#   FIRST=no      nothing is used before the change: every window is first
#                 made after it
#
# In the sandbox only, as a session that publishes its scale
# (SCREEN=3840x2160 heyo.sh up gnome); Heyo must not be running in it.
. "$(dirname "$0")/lib.sh"
from=${1:?FROM}; to=${2:?TO}; name=${3:-$1-to-$2}
[ $# -gt 3 ] && shift 3 || shift $#
out=$RESULTS/display-$name.txt
[ -f "$D/pids/xsettings" ] || { echo "this sandbox publishes no scale" >&2; exit 1; }
alive heyo && { echo "Heyo is running already" >&2; exit 1; }
READ="/usr/bin/python3 -I $HERE/screen-read.py"
png() { python3 -I "$HERE/shot.py" "$DISPLAY" "$@"; }
geometry() { set -- $(place "$1"); echo "${3}x${4}+${1}+${2}"; }
stored() { python3 -I -c 'import json, sys; print(json.load(open(sys.argv[1])).get(sys.argv[2]))' "$XDG_CONFIG_HOME/heyo/settings.json" "$1"; }
over() { xdotool mousemove "$1" "$2"; sleep 0.4; $READ cursor; }
pins() { xdotool search --onlyvisible --name '^Heyo Pinned Screenshot$' 2>/dev/null; }

# use WHEN: every window once.
use() {
  when=$1
  scale=$(xrdb -query | awk '/^Xft.dpi/{print int($2 / 96 + 0.5)}')
  screen=$(xdpyinfo | awk '/dimensions/{print $2}'); sw=${screen%x*}; sh=${screen#*x}
  echo "## $1: scale $scale, Xcursor.size $(xrdb -query | awk '/^Xcursor.size/{print $2}'), screen $screen"
  env -u GDK_SCALE zenity --info --title "A window" --text "A window" --width 300 --height 150 > /dev/null 2>&1 &
  info=$!
  sleep 2
  eval "$(xdotool getwindowgeometry --shell "$(xdotool search --name '^A window$' | tail -1)")"
  echo "pointer over a GTK window: $(over $((X + WIDTH / 2)) $((Y + 30 * scale)))"
  kill "$info" 2> /dev/null

  for pin in $(pins); do
    eval "$(xdotool getwindowgeometry --shell "$pin")"
    echo "the capture pinned before: $(geometry "$pin"); pointer over it: $(over $((X + WIDTH / 2)) $((Y + HEIGHT / 2)))"
    xdotool click 3; sleep 0.5
    echo "  after a right click on it: $(pins | wc -l) pinned"
  done

  timeout 10 "$HEYO" --open heyo:settings > /dev/null 2>&1; sleep 2.5
  s=$(settings_window)
  eval "$(xdotool getwindowgeometry --shell "$s")"
  # On a screen too small for the shadow's margin the window is its panel.
  margin=$(((WIDTH / scale - 860) / 2))
  echo "settings: $(geometry "$s") (860 by 640 and a margin of 32 is $((924 * scale))x$((704 * scale)); its margin is $margin)"
  echo "  pointer over its middle: $(over $((X + WIDTH / 2)) $((Y + 300 * scale))); over a switch: $(over $((X + (804 + margin) * scale)) $((Y + (394 + margin) * scale)))"
  before=$(stored hideOnBlur)
  # As a hand clicks: the button is down for a moment.
  xdotool mousedown 1; sleep 0.15; xdotool mouseup 1; sleep 0.8
  echo "  a click on that switch: hideOnBlur $before -> $(stored hideOnBlur)"

  for time in first second; do
    "$HEYO" > /dev/null 2>&1; sleep 1.5
    set -- $(place "$(xdotool search --onlyvisible --name '^Heyo$' | tail -1)")
    echo "palette, the $time time: ${3}x${4}+${1}+${2} (the middle is +$(((sw - $3) / 2))+$(((sh - $4) / 2)))"
    "$HEYO" > /dev/null 2>&1; sleep 0.8
  done

  # A screenshot: 1200 by 700 from 1000,600, saved.
  rm -rf "$HOME/Pictures/Screenshots"
  xdotool mousemove 300 $((sh - 200)); sleep 0.3
  png "$S/display-before.png"
  "$HEYO" --open heyo:screenshot > /dev/null 2>&1; sleep 1.5
  xdotool mousemove 1000 600 mousedown 1; sleep 0.1; xdotool mousemove 1600 900; sleep 0.1; xdotool mousemove 2200 1300; sleep 0.2; xdotool mouseup 1; sleep 0.4
  echo "screenshot: pointer in the selection: $(over 1500 900); on a handle: $(over 2200 1300); on the toolbar: $(over $((2200 - 250 * scale)) $((1300 + 28 * scale)))"
  xdotool mousemove 1500 900; sleep 0.3
  png "$S/display-overlay.png"
  [ "$when" = before ] || png "$RESULTS/display-$name.png" 900 500 1400 $((860 + 60 * scale))
  # The toolbar is 499 by 40 logical pixels; measured in its padding, clear of its buttons and its round corners.
  python3 -I "$HERE/toolbar.py" "$S/display-overlay.png" $((2200 - 250 * scale)) $((1300 + 10 * scale)) "$scale"
  xdotool key ctrl+s; sleep 1.5
  file=$(ls -t "$HOME/Pictures/Screenshots/"*.png 2>/dev/null | head -1)
  if [ -n "$file" ]; then
    echo "  saved: $(python3 -I -c 'import sys; from PIL import Image; print("%dx%d" % Image.open(sys.argv[1]).size)' "$file"); against the screen there: $($READ same "$S/display-before.png" "$file" 1000 600 1200 700)"
  else
    echo "  nothing was saved"
  fi

  # Another, 300 by 200 from 400,300, pinned: the toolbar's twelfth button.
  "$HEYO" --open heyo:screenshot > /dev/null 2>&1; sleep 1.5
  xdotool mousemove 400 300 mousedown 1; sleep 0.1; xdotool mousemove 550 400; sleep 0.1; xdotool mousemove 700 500; sleep 0.2; xdotool mouseup 1; sleep 0.3
  left=$((700 / scale - 499)); [ "$left" -lt 8 ] && left=8
  xdotool mousemove $(((left + 20 + 32 * 11 + 11) * scale)) $(((500 / scale + 28) * scale)); sleep 0.15; xdotool click 1; sleep 1.5
  pin=$(pins | tail -1)
  if [ -n "$pin" ]; then
    echo "pinned 300x200 from 400,300: a window at $(geometry "$pin"); pointer over it: $(over 550 400)"
  else
    echo "nothing was pinned"; xdotool key Escape
  fi

  "$HEYO" --dictate > /dev/null 2>&1; sleep 1.2
  d=$(xdotool search --onlyvisible --name '^Heyo Dictation$' | tail -1)
  if [ -n "$d" ]; then
    eval "$(xdotool getwindowgeometry --shell "$d")"
    echo "pill: its window $(geometry "$d") (360 by 44 at the bottom centre, a tenth up, is $((360 * scale))x$((44 * scale))+$(((sw - 360 * scale) / 2))+$((sh - 44 * scale - sh / 10))); pointer over it: $(over $((X + WIDTH / 2)) $((Y + HEIGHT / 2)))"
  else
    echo "no pill came up"
  fi
  sleep 3
  if [ "${KEEP:-open}" = closed ]; then
    eval "$(xdotool getwindowgeometry --shell "$s")"
    xdotool mousemove $((X + WIDTH / 2)) $((Y + 300 * scale)) click 1; sleep 0.3; xdotool key Escape; sleep 0.8
    echo "the settings closed with Escape: $(settings_window | wc -l) on screen, $(pins | wc -l) pinned"
  fi
}

{
  "$HERE/heyo.sh" scale "$from" > /dev/null
  echo "# $(date +%F), $(basename "$HEYO"), started at scale $from, then: $to${KEEP:+, the settings $KEEP meanwhile}"
  HEYO_WARMUP=${HEYO_WARMUP:-4} "$HERE/heyo.sh" start "$@" > /dev/null || exit 1
  [ "${FIRST:-yes}" = yes ] && use before
  case $to in
    same) ;;
    *x*)
      xrandr --newmode "$to" 0 "${to%x*}" "${to%x*}" "${to%x*}" "${to%x*}" "${to#*x}" "${to#*x}" "${to#*x}" "${to#*x}" 2> /dev/null
      xrandr --addmode screen "$to" 2> /dev/null
      xrandr --output screen --mode "$to"; sleep 4
      use "after the screen became $to"
      ;;
    *)
      "$HERE/heyo.sh" scale "$to" > /dev/null; sleep 3
      use "after the change"
      ;;
  esac
  echo "## what Heyo said"
  grep -E "heyo: (the display|the pointer|panicked|.*could not|.*cannot)" "$S/heyo.log" | sort | uniq -c
  "$HERE/heyo.sh" stop > /dev/null
} 2>&1 | tee "$out"

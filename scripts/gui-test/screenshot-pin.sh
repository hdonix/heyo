#!/bin/sh
# The pinned capture, used as a person would:
#
#   screenshot-pin.sh [NAME] [VAR=value…]    what was seen goes to $RESULTS/screenshot-pin-NAME.txt,
#                                            a picture to $RESULTS/screenshot-pin-NAME.png
#
# A part of the screen is selected and pinned: a window must be exactly
# where the part was, above other windows, showing the desktop there pixel
# for pixel inside its edge. A second is pinned beside it. One is dragged
# and must have moved by as much as the pointer; a click that goes nowhere
# must not move it. Then each is closed its own way: a double click, a
# right click, Escape.
#
# In the session that runs, or one of its own (GNOME Shell, 3840x2160), at
# the scale it publishes; Heyo must not be running in it.
. "$(dirname "$0")/env.sh"
name=${1:-x11}; [ $# -gt 0 ] && shift
export SCREEN=${SCREEN:-3840x2160}
session_for gnome
out=$RESULTS/screenshot-pin-$name.txt
scale=$(scale)
wait=${WAIT:-1.5}
READ="/usr/bin/python3 $GUI/screen-read.py"
pins() { xdotool search --onlyvisible --name '^Heyo Pinned Screenshot$' 2>/dev/null; }
# pin X Y X Y: selects that part and presses the toolbar's pin button.
pin() {
  heyo --open heyo:screenshot > /dev/null 2>&1; sleep 1.3
  xdotool mousemove "$1" "$2" mousedown 1; sleep 0.1; xdotool mousemove $((($1 + $3) / 2)) $((($2 + $4) / 2)); sleep 0.1; xdotool mousemove "$3" "$4"; sleep 0.2; xdotool mouseup 1; sleep 0.3
  xdotool mousemove $(toolbar pin); sleep 0.15; xdotool click 1; sleep "$wait"
}
{
  echo "# $(date +%F), $BUILD, $(xdpyinfo | awk '/dimensions/{print $2}') at scale $scale"
  HEYO_TEST_WARMUP=${HEYO_TEST_WARMUP:-4} "$GUI/sandbox.sh" heyo "$@" || exit 1
  xdotool mousemove 300 1900; sleep 0.4
  png "$T/pin-before.png"
  pin 1000 600 1600 1000
  first=$(pins | tail -1)
  echo "pinned 600 by 400 from 1000,600: a window at $(place "$first")"
  xprop -id "$first" _NET_WM_WINDOW_TYPE _NET_WM_STATE | sed 's/^/  /'
  xdotool mousemove 300 1900; sleep 0.3
  # Its edge is one logical pixel.
  set -- $((1000 + scale)) $((600 + scale)) $((600 - 2 * scale)) $((400 - 2 * scale))
  png "$T/pin-first.png" "$@"
  echo "what it shows inside its edge, against the desktop there: $($READ same "$T/pin-before.png" "$T/pin-first.png" "$@")"
  echo "its edge at its top left corner: $(/usr/bin/python3 -c 'import sys; from PIL import Image; print(Image.open(sys.argv[1]).convert("RGB").getpixel((0, 0)))' "$(png "$T/pin-edge.png" 1000 600 4 4; echo "$T/pin-edge.png")") (the accent at 70 % over the picture)"
  pin 1800 700 2100 900
  echo "a second, 300 by 200 from 1800,700: $(pins | wc -l) windows; the new one at $(place "$(pins | grep -v -x "$first" | tail -1)")"
  second=$(pins | grep -v -x "$first" | tail -1)
  xdotool mousemove 300 1900; sleep 0.3
  png "$RESULTS/screenshot-pin-$name.png" 900 500 1400 600
  echo "## moved by dragging"
  xdotool mousemove 1300 800 mousedown 1; sleep 0.1; xdotool mousemove 1310 805; sleep 0.1; xdotool mousemove 1400 850; sleep 0.1; xdotool mousemove 1500 900; sleep 0.3; xdotool mouseup 1; sleep 0.4
  echo "dragged by 200,100: now at $(place "$first")"
  xdotool mousemove 1500 900 click 1; sleep 0.5
  echo "after a click that goes nowhere: at $(place "$first"), $(pins | wc -l) windows"
  echo "## closed"
  sleep 0.6
  xdotool mousemove 1500 900 click --repeat 2 --delay 80 1; sleep 0.6
  echo "after a double click on the first: $(pins | wc -l) window(s)"
  xdotool mousemove 1950 800 click 3; sleep 0.6
  echo "after a right click on the second: $(pins | wc -l)"
  pin 1000 600 1600 1000
  third=$(pins | tail -1)
  xdotool mousemove 1300 800 click 1; sleep 0.4
  xdotool key Escape; sleep 0.6
  echo "a third, clicked and then Escape: $(pins | wc -l)"
  grep -E "heyo: (screenshot failed|the capture|panicked)" "$T/app.log"
  stop heyo > /dev/null
} 2>&1 | tee "$out"
leave

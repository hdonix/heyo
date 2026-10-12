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
# In the sandbox only (SCREEN=3840x2160 GDK_SCALE=1 heyo.sh up gnome); Heyo
# must not be running in it.
. "$(dirname "$0")/lib.sh"
name=${1:-x11}; [ $# -gt 0 ] && shift
out=$RESULTS/screenshot-pin-$name.txt
scale=${GDK_SCALE:-2}
wait=${WAIT:-1.5}
alive heyo && { echo "Heyo is running already" >&2; exit 1; }
READ="/usr/bin/python3 -I $HERE/screen-read.py"
png() { python3 -I "$HERE/shot.py" "$DISPLAY" "$1" ${2:+$2 $3 $4 $5}; }
pins() { xdotool search --onlyvisible --name '^Heyo Pinned Screenshot$' 2>/dev/null; }
# pin X Y X Y: selects that part and presses the toolbar's pin button.
pin() {
  "$HEYO" --open heyo:screenshot; sleep 1.3
  xdotool mousemove "$1" "$2" mousedown 1; sleep 0.1; xdotool mousemove $((($1 + $3) / 2)) $((($2 + $4) / 2)); sleep 0.1; xdotool mousemove "$3" "$4"; sleep 0.2; xdotool mouseup 1; sleep 0.3
  # The toolbar's right end is at the selection's, or as far left as the screen lets it be; pin is its twelfth button.
  left=$(($3 / scale - 499)); [ "$left" -lt 8 ] && left=8
  xdotool mousemove $(((left + 20 + 32 * 11 + 11) * scale)) $((($4 / scale + 28) * scale)); sleep 0.15; xdotool click 1; sleep "$wait"
}
{
  echo "# $(date +%F), $(basename "$HEYO"), $(xdpyinfo | awk '/dimensions/{print $2}') at scale $scale"
  HEYO_WARMUP=${HEYO_WARMUP:-4} "$HERE/heyo.sh" start "$@" > /dev/null || exit 1
  xdotool mousemove 300 1900; sleep 0.4
  png "$S/pin-before.png"
  pin 1000 600 1600 1000
  first=$(pins | tail -1)
  echo "pinned 600 by 400 from 1000,600: a window at $(place "$first")"
  xprop -id "$first" _NET_WM_WINDOW_TYPE _NET_WM_STATE | sed 's/^/  /'
  xdotool mousemove 300 1900; sleep 0.3
  png "$S/pin-first.png" 1001 601 598 398
  echo "what it shows inside its edge, against the desktop there: $($READ same "$S/pin-before.png" "$S/pin-first.png" 1001 601 598 398)"
  echo "its edge at its top left corner: $(/usr/bin/python3 -I -c 'import sys; from PIL import Image; print(Image.open(sys.argv[1]).convert("RGB").getpixel((0, 0)))' "$(png "$S/pin-edge.png" 1000 600 4 4; echo "$S/pin-edge.png")") (the accent at 70 % over the picture)"
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
  grep -E "heyo: (screenshot failed|the capture|panicked)" "$S/heyo.log"
  "$HERE/heyo.sh" stop > /dev/null
} 2>&1 | tee "$out"

#!/bin/sh
# The screenshot overlay Slint draws, tried as a person would use it, with
# its pixels checked against the desktop it froze:
#
#   screenshot.sh [NAME]     what was seen goes to $RESULTS/screenshot-NAME.txt,
#                            pictures to $RESULTS/screenshot-NAME-*.png
#
# A screenshot is asked for; a selection is dragged out, moved by its
# inside and resized by a handle; Enter copies it. At each stage the screen
# is read: inside the selection every pixel must be the desktop's, outside
# it the desktop's dimmed (screenshot-pixels.py), and afterwards the screen
# must be the desktop again.
#
# In the sandbox only, at scale 1 (SCREEN=3840x2160 GDK_SCALE=1 heyo.sh up
# gnome); Heyo must not be running in it.
. "$(dirname "$0")/lib.sh"
name=${1:-x11}
out=$RESULTS/screenshot-$name.txt
alive heyo && { echo "Heyo is running already" >&2; exit 1; }
png() { python3 -I "$HERE/shot.py" "$DISPLAY" "$1" ${2:+$2 $3 $4 $5}; }
overlay() { xdotool search --name '^Heyo Screenshot$' 2>/dev/null | tail -1; }
drag() { xdotool mousemove "$1" "$2" mousedown 1; sleep 0.1; xdotool mousemove $((($1 + $3) / 2)) $((($2 + $4) / 2)); sleep 0.1; xdotool mousemove "$3" "$4"; sleep 0.3; }
{
  echo "# $(date +%F), $(basename "$HEYO"), $(xdpyinfo | awk '/dimensions/{print $2}') at scale ${GDK_SCALE:-2}"
  HEYO_WARMUP=${HEYO_WARMUP:-4} "$HERE/heyo.sh" start > /dev/null || exit 1
  xdotool mousemove 300 1900
  sleep 0.5
  png "$S/shot-before.png"
  "$HEYO" --open heyo:screenshot
  sleep 1.2
  png "$RESULTS/screenshot-$name-hint.png" 1520 980 800 200
  id=$(overlay)
  echo "the overlay's window: $(xwininfo -id "$id" | grep -E "Depth|Override|geometry|Map State" | tr -s ' \n' ' ')"
  drag 1000 600 2200 1300
  png "$RESULTS/screenshot-$name-dragging.png" 1900 1100 600 400
  xdotool mouseup 1
  sleep 0.4
  # The pointer away from it: no loupe, and no handle under it.
  xdotool mousemove 300 1900
  sleep 0.4
  png "$S/shot-selected.png"
  # Its corner, with the size above it and two of its handles.
  png "$RESULTS/screenshot-$name-selected.png" 940 540 760 460
  echo "## selected 1200 by 700 at 1000,600"
  python3 -I "$HERE/screenshot-pixels.py" "$S/shot-before.png" "$S/shot-selected.png" 1000 600 1200 700
  drag 1600 900 1800 1000; xdotool mouseup 1; sleep 0.2
  drag 2400 1400 2600 1500; xdotool mouseup 1; sleep 0.2
  xdotool mousemove 300 1900
  sleep 0.4
  png "$S/shot-moved.png"
  echo "## moved by 200,100 and its bottom right corner taken 200,100 further"
  python3 -I "$HERE/screenshot-pixels.py" "$S/shot-before.png" "$S/shot-moved.png" 1200 700 1400 800
  xdotool key Return
  sleep 1
  png "$S/shot-after.png"
  python3 -I - "$S/shot-before.png" "$S/shot-after.png" <<'PY'
import sys
from PIL import Image, ImageChops
before, after = (Image.open(path).convert("RGB") for path in sys.argv[1:3])
box = ImageChops.difference(before, after).getbbox()
print("## after Enter the screen is the desktop again" if box is None else f"## FAIL: after Enter the screen differs from the desktop in {box}")
PY
  grep -E "heyo: (screenshot failed|panicked)" "$S/heyo.log"
  "$HERE/heyo.sh" stop > /dev/null
} 2>&1 | tee "$out"

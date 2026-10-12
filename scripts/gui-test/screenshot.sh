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
# In the session that runs, or one of its own (GNOME Shell, 3840x2160), at
# the scale it publishes; Heyo must not be running in it.
. "$(dirname "$0")/env.sh"
name=${1:-x11}
export SCREEN=${SCREEN:-3840x2160}
session_for gnome
out=$RESULTS/screenshot-$name.txt
scale=$(scale)
overlay() { xdotool search --name '^Heyo Screenshot$' 2>/dev/null | tail -1; }
drag() { xdotool mousemove "$1" "$2" mousedown 1; sleep 0.1; xdotool mousemove $((($1 + $3) / 2)) $((($2 + $4) / 2)); sleep 0.1; xdotool mousemove "$3" "$4"; sleep 0.3; }
{
  echo "# $(date +%F), $BUILD, $(xdpyinfo | awk '/dimensions/{print $2}') at scale $scale"
  # The Shell says "Screenshot copied" in a banner of its own, which is the
  # desktop's and not the overlay's: for as long as the screen is compared
  # with itself, the session shows none.
  banners off
  HEYO_TEST_WARMUP=${HEYO_TEST_WARMUP:-4} "$GUI/sandbox.sh" heyo || exit 1
  xdotool mousemove 300 1900
  sleep 0.5
  png "$T/shot-before.png"
  heyo --open heyo:screenshot > /dev/null 2>&1
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
  png "$T/shot-selected.png"
  # Its corner, with the size above it and two of its handles.
  png "$RESULTS/screenshot-$name-selected.png" 940 540 760 460
  echo "## selected 1200 by 700 at 1000,600"
  python3 "$GUI/screenshot-pixels.py" "$T/shot-before.png" "$T/shot-selected.png" 1000 600 1200 700 "$scale"
  drag 1600 900 1800 1000; xdotool mouseup 1; sleep 0.2
  drag 2400 1400 2600 1500; xdotool mouseup 1; sleep 0.2
  xdotool mousemove 300 1900
  sleep 0.4
  png "$T/shot-moved.png"
  echo "## moved by 200,100 and its bottom right corner taken 200,100 further"
  python3 "$GUI/screenshot-pixels.py" "$T/shot-before.png" "$T/shot-moved.png" 1200 700 1400 800 "$scale"
  xdotool key Return
  sleep 1
  png "$T/shot-after.png"
  python3 - "$T/shot-before.png" "$T/shot-after.png" <<'PY'
import sys
from PIL import Image, ImageChops
before, after = (Image.open(path).convert("RGB") for path in sys.argv[1:3])
box = ImageChops.difference(before, after).getbbox()
print("## after Enter the screen is the desktop again" if box is None else f"## FAIL: after Enter the screen differs from the desktop in {box}")
PY
  grep -E "heyo: (screenshot failed|panicked)" "$T/app.log"
  banners on
  stop heyo > /dev/null
} 2>&1 | tee "$out"
leave

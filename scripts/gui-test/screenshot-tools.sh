#!/bin/sh
# The screenshot overlay's tools and ways out, used as a person would:
#
#   screenshot-tools.sh [NAME] [VAR=value…]    what was seen goes to $RESULTS/screenshot-tools-NAME.txt,
#                                              pictures to $RESULTS/screenshot-tools-NAME-*.png
#
# - each tool draws (rectangle, arrow, pen, blur, text typed), in the
#   colour chosen; a button says what it is for; undo by key and by button;
# - Ctrl+S saves the selection: the file must be the desktop there pixel
#   for pixel when nothing is drawn on it, and with something drawn must
#   have that in it and the desktop elsewhere;
# - Enter copies: the picture on the clipboard must be the desktop there
#   pixel for pixel;
# - a click on a window selects exactly that window;
# - Escape gives up text that is being typed and then the screenshot; a
#   right click gives it up wherever it is;
# - the pointer's shape over each thing;
# - "copy text" says what it does at the bottom, and "pin" leaves a window.
#
# In the session that runs, or one of its own (GNOME Shell, 3840x2160), at
# the scale it publishes; Heyo must not be running in it.
. "$(dirname "$0")/env.sh"
name=${1:-x11}; [ $# -gt 0 ] && shift
export SCREEN=${SCREEN:-3840x2160}
session_for gnome
out=$RESULTS/screenshot-tools-$name.txt
scale=$(scale)
# How long a save or a copy is given (a debug build needs longer).
wait=${WAIT:-1.5}
READ="/usr/bin/python3 $GUI/screen-read.py"
overlay() { xdotool search --name '^Heyo Screenshot$' 2>/dev/null | tail -1; }
up() { if viewable "$(overlay)"; then echo up; else echo gone; fi; }
drag() { xdotool mousemove "$1" "$2" mousedown 1; sleep 0.1; xdotool mousemove $((($1 + $3) / 2)) $((($2 + $4) / 2 + 20)); sleep 0.1; xdotool mousemove "$3" "$4"; sleep 0.2; xdotool mouseup 1; sleep 0.25; }
# The selection every case makes, in the screen's pixels, and a button of
# its toolbar by its name (rect arrow pen text blur red yellow green blue
# undo ocr pin save copy cancel): where it is, is layout.py's to say.
select_it() { heyo --open heyo:screenshot > /dev/null 2>&1; sleep 1.3; drag 1000 600 2200 1300; }
button() { set -- $(toolbar "$1" 2200 1300); bx=$1; by=$2; }
B() { button "$1"; xdotool mousemove "$bx" "$by"; sleep 0.15; xdotool click 1; sleep 0.25; }
at() { xdotool mousemove "$1" "$2"; sleep 0.25; echo "the pointer $3: $($READ cursor)"; }
saved() { ls -t "$HOME/Pictures/Screenshots/"*.png 2>/dev/null | head -1; }
{
  echo "# $(date +%F), $BUILD, $(xdpyinfo | awk '/dimensions/{print $2}') at scale $scale"
  HEYO_TEST_WARMUP=${HEYO_TEST_WARMUP:-4} "$GUI/sandbox.sh" heyo "$@" || exit 1
  rm -rf "$HOME/Pictures/Screenshots"
  # What is copied and saved is compared with the screen: the Shell's
  # banner saying so is kept off it.
  banners off
  # A window of another program, to be selected by a click.
  start_as window zenity --info --title "A window" --text "A window to select" --width 600 --height 300 > /dev/null 2>&1
  sleep 1.5
  # Clear of the selection the other cases make, at any scale: its button
  # is drawn differently from one moment to the next, and would be in
  # what is compared with the screen.
  xdotool search --name '^A window$' | tail -1 | xargs -I{} xdotool windowmove {} 2330 1400
  sleep 0.5
  xdotool mousemove 300 1900; sleep 0.4
  png "$T/tools-before.png"

  echo "## each tool"
  select_it
  png "$RESULTS/screenshot-tools-$name-toolbar.png" $((2200 - 540 * scale)) 1260 $((580 * scale)) $((110 * scale))
  B rect; drag 1100 700 1400 900
  B arrow; drag 1500 700 1800 900
  B green
  B pen; xdotool mousemove 1100 1000 mousedown 1; for i in 1 2 3 4 5 6 7 8 9 10 11 12; do xdotool mousemove $((1100 + i * 30)) $((1000 + (i % 3) * 40)); sleep 0.03; done; xdotool mouseup 1; sleep 0.25
  B blur; drag 1850 650 2150 800
  B blue
  B text; xdotool mousemove 1600 1150 click 1; sleep 0.4
  xdotool type --delay 40 "Look here"; sleep 0.3
  png "$RESULTS/screenshot-tools-$name-typing.png" 1500 1090 $((250 * scale)) $((60 * scale))
  xdotool key Return; sleep 0.3
  button save; xdotool mousemove "$bx" "$by"; sleep 1.2
  png "$RESULTS/screenshot-tools-$name-drawn.png" 950 550 1300 $((760 + 70 * scale))
  xdotool mousemove 300 1900; sleep 0.2
  echo "## saved with all of it drawn"
  xdotool key ctrl+s; sleep "$wait"
  echo "the overlay is $(up); saved: $(basename "$(saved)")"
  cp "$(saved)" "$T/tools-drawn.png"
  /usr/bin/python3 - "$T/tools-before.png" "$T/tools-drawn.png" <<'PY'
import sys
from PIL import Image, ImageChops
before, drawn = Image.open(sys.argv[1]).convert("RGB").crop((1000, 600, 2200, 1300)), Image.open(sys.argv[2]).convert("RGB")
print(f"the file is {drawn.size[0]} x {drawn.size[1]}")
# Away from everything drawn: the bottom right quarter, but for the text.
for what, box in (("where nothing is drawn (right of the pen, below the blur)", (700, 300, 1200, 480)), ("the bottom strip", (0, 620, 1200, 700))):
    difference = ImageChops.difference(before.crop(box), drawn.crop(box))
    print(f"{what}: {'the desktop, pixel for pixel' if difference.getbbox() is None else 'FAIL: differs in ' + str(difference.getbbox())}")
# On the rectangle's outline, the arrow's shaft, the pen's path and the text.
print("the rectangle's outline (red):", drawn.getpixel((100, 200)))
print("the arrow's shaft (red):", drawn.getpixel((650, 200)))
print("the pen's first stretch (green):", drawn.getpixel((115, 420)))
reds = sum(1 for pixel in drawn.crop((90, 90, 410, 310)).getdata() if pixel == (255, 69, 58))
blues = sum(1 for pixel in drawn.crop((590, 530, 760, 580)).getdata() if pixel == (86, 194, 255))
print(f"pixels of exactly the red in the rectangle's box: {reds}; of exactly the blue where the text is: {blues}")
blurred, clear = drawn.crop((850, 50, 1150, 200)), before.crop((850, 50, 1150, 200))
print("the blurred part:", "differs from the desktop" if ImageChops.difference(blurred, clear).getbbox() else "FAIL: is the desktop")
PY

  echo "## undo, by key and by button"
  select_it
  B rect; drag 1100 700 1400 900
  B rect; drag 1500 700 1800 900
  B rect; drag 1100 1000 1400 1200
  xdotool key ctrl+z; sleep 0.2; B undo
  xdotool mousemove 300 1900; sleep 0.2
  xdotool key ctrl+s; sleep "$wait"
  /usr/bin/python3 - "$T/tools-before.png" "$(saved)" <<'PY'
import sys
from PIL import Image
before, drawn = Image.open(sys.argv[1]).convert("RGB").crop((1000, 600, 2200, 1300)), Image.open(sys.argv[2]).convert("RGB")
red = (255, 69, 58)
print("of three rectangles the first is left:", [drawn.getpixel(at) == red for at in ((100, 200), (500, 200), (100, 500))])
PY

  echo "## an annotation moved, and one that goes past the selection's edge"
  select_it
  B rect; drag 1100 700 1400 900
  # The selection tool again: the rectangle is taken by its outline and moved by 300,200.
  B rect; drag 1100 800 1400 1000
  # An arrow from inside the selection to outside it.
  B arrow; drag 2000 1000 2400 1000
  xdotool mousemove 300 1900; sleep 0.3
  png "$T/tools-moved.png"
  /usr/bin/python3 - "$T/tools-before.png" "$T/tools-moved.png" <<'PY'
import sys
from PIL import Image
before, screen = (Image.open(path).convert("RGB") for path in sys.argv[1:3])
red = (255, 69, 58)
print("the rectangle's left side is where it was moved to:", screen.getpixel((1400, 1000)) == red, "and no longer where it was drawn:", screen.getpixel((1100, 800)) == before.getpixel((1100, 800)))
dimmed = tuple(value * 154 // 256 for value in before.getpixel((2300, 1000)))
print("the arrow inside the selection:", screen.getpixel((2100, 1000)) == red, "and past its edge the desktop dimmed, with no arrow:", screen.getpixel((2300, 1000)) == dimmed)
PY
  echo "## the cancel button"
  B cancel
  echo "the overlay is $(up)"

  echo "## saved with nothing drawn"
  select_it
  xdotool mousemove 300 1900; sleep 0.2
  xdotool key ctrl+s; sleep "$wait"
  echo "the overlay is $(up); the file against the desktop there: $($READ same "$T/tools-before.png" "$(saved)" 1000 600 1200 700)"

  echo "## copied with Enter"
  select_it
  xdotool mousemove 300 1900; sleep 0.2
  xdotool key Return; sleep "$wait"
  echo "the overlay is $(up); on the clipboard: $($READ clipboard "$T/tools-copied.png")"
  echo "the clipboard's picture against the desktop there: $($READ same "$T/tools-before.png" "$T/tools-copied.png" 1000 600 1200 700)"

  echo "## copied with the button, something drawn on it"
  select_it
  B rect; drag 1100 700 1400 900
  B copy; sleep "$wait"
  echo "the overlay is $(up); on the clipboard: $($READ clipboard "$T/tools-copied-drawn.png")"
  /usr/bin/python3 -c 'import sys; from PIL import Image; print("the rectangle is in it:", Image.open(sys.argv[1]).convert("RGB").getpixel((100, 200)) == (255, 69, 58))' "$T/tools-copied-drawn.png"

  echo "## a click on a window"
  win=$(xdotool search --name '^A window$' | tail -1)
  set -- $(place "$win")
  # What is seen of it: its frame without the shadow's margin.
  set -- $(xprop -id "$win" _GTK_FRAME_EXTENTS | awk -F'[=,]' -v x="$1" -v y="$2" -v w="$3" -v h="$4" '{print x + $2, y + $4, w - $2 - $3, h - $4 - $5}')
  echo "the window is $3 x $4 at $1,$2"
  heyo --open heyo:screenshot > /dev/null 2>&1; sleep 1.3
  xdotool mousemove $(($1 + $3 / 2)) $(($2 + $4 / 2)); sleep 0.4
  png "$RESULTS/screenshot-tools-$name-window.png" 0 0 $(($1 + $3 + 300)) $(($2 + $4 + 200))
  xdotool click 1; sleep 0.4
  xdotool mousemove 300 1900; sleep 0.2
  xdotool key Return; sleep "$wait"
  echo "on the clipboard: $($READ clipboard "$T/tools-window.png")"
  echo "against the desktop where the window is: $($READ same "$T/tools-before.png" "$T/tools-window.png" "$1" "$2" "$3" "$4")"

  echo "## the pointer's shape"
  heyo --open heyo:screenshot > /dev/null 2>&1; sleep 1.3
  at 300 1900 "with nothing selected"
  drag 1000 600 2200 1300
  at 1500 900 "in the selection"
  at 1000 600 "on its top left handle"
  at 1600 600 "on its top handle"
  at 2200 600 "on its top right handle"
  at 2200 950 "on its right handle"
  # Half way from undo to the button after the separator.
  button undo; undo=$bx; button ocr; at $(((undo + bx) / 2)) "$by" "on the toolbar, between buttons"
  button rect; at "$bx" "$by" "on a button"
  at 500 500 "outside the selection"
  B rect
  at 1500 900 "in the selection with the rectangle tool"
  drag 1100 700 1400 900
  B rect
  at 1100 800 "on a rectangle that was drawn, with the selection tool"

  echo "## Escape"
  B text; xdotool mousemove 1600 1150 click 1; sleep 0.3
  xdotool type --delay 40 "gone"; sleep 0.2
  xdotool key Escape; sleep 0.3
  echo "while text is typed, after the first Escape the overlay is $(up), after the second $(xdotool key Escape; sleep 0.4; up)"

  echo "## a right click"
  select_it
  xdotool mousemove 1500 900 click 3; sleep 0.4
  echo "in the selection: the overlay is $(up)"
  heyo --open heyo:screenshot > /dev/null 2>&1; sleep 1.3
  xdotool mousemove 500 500 click 3; sleep 0.4
  echo "with nothing selected: $(up)"
  select_it
  button pen; xdotool mousemove "$bx" "$by" click 3; sleep 0.4
  echo "on the toolbar: $(up)"

  echo "## copy the text in it"
  select_it
  B ocr; sleep 0.3
  png "$RESULTS/screenshot-tools-$name-reading.png" $((1920 - 250 * scale)) $((2160 - 90 * scale)) $((500 * scale)) $((60 * scale))
  sleep 4
  png "$RESULTS/screenshot-tools-$name-read.png" $((1920 - 380 * scale)) $((2160 - 90 * scale)) $((760 * scale)) $((60 * scale))
  echo "the overlay is $(up)"
  xdotool key Escape; sleep 0.4

  echo "## asked for as \"copy text from the screen\""
  heyo --open heyo:screenshot-text > /dev/null 2>&1; sleep 1.3
  png "$RESULTS/screenshot-tools-$name-text-hint.png" $((1920 - 250 * scale)) $((1080 - 30 * scale)) $((500 * scale)) $((60 * scale))
  drag 1000 600 2200 1300
  sleep 4
  png "$RESULTS/screenshot-tools-$name-text-read.png" $((1920 - 380 * scale)) $((2160 - 90 * scale)) $((760 * scale)) $((60 * scale))
  echo "after the selection was made the overlay is $(up), with no toolbar: the pointer where the toolbar would be is $(button copy; xdotool mousemove "$bx" "$by"; sleep 0.3; $READ cursor)"
  xdotool key Escape; sleep 0.4

  echo "## pin"
  select_it
  B pin; sleep "$wait"
  pinned=$(xdotool search --name '^Heyo Pinned Screenshot$' | tail -1)
  echo "the overlay is $(up); the pinned window: $(place "$pinned")"
  grep -E "heyo: (screenshot failed|the annotations|panicked|the screenshot overlay)" "$T/app.log"
  banners on
  stop window heyo > /dev/null
} 2>&1 | tee "$out"
leave

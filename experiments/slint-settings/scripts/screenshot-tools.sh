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
# In the sandbox only (SCREEN=3840x2160 heyo.sh up gnome, GDK_SCALE 1 or 2);
# Heyo must not be running in it.
. "$(dirname "$0")/lib.sh"
name=${1:-x11}; [ $# -gt 0 ] && shift
out=$RESULTS/screenshot-tools-$name.txt
scale=${GDK_SCALE:-2}
# How long a save or a copy is given (a debug build needs longer).
wait=${WAIT:-1.5}
alive heyo && { echo "Heyo is running already" >&2; exit 1; }
READ="/usr/bin/python3 -I $HERE/screen-read.py"
png() { python3 -I "$HERE/shot.py" "$DISPLAY" "$1" ${2:+$2 $3 $4 $5}; }
overlay() { xdotool search --name '^Heyo Screenshot$' 2>/dev/null | tail -1; }
up() { if xwininfo -id "$(overlay)" 2>/dev/null | grep -q IsViewable; then echo up; else echo gone; fi; }
drag() { xdotool mousemove "$1" "$2" mousedown 1; sleep 0.1; xdotool mousemove $((($1 + $3) / 2)) $((($2 + $4) / 2 + 20)); sleep 0.1; xdotool mousemove "$3" "$4"; sleep 0.2; xdotool mouseup 1; sleep 0.25; }
# The selection every case makes, in the screen's pixels, and a button of
# its toolbar by its place in it: the toolbar is 499 logical pixels wide,
# its right end at the selection's, 8 below it; a button every 32.
select_it() { "$HEYO" --open heyo:screenshot; sleep 1.3; drag 1000 600 2200 1300; }
button() {
  case $1 in rect) i=0 ;; arrow) i=1 ;; pen) i=2 ;; text) i=3 ;; blur) i=4 ;; red) i=5 ;; yellow) i=6 ;; green) i=7 ;; blue) i=8 ;; undo) i=9 ;; ocr) i=10 ;; pin) i=11 ;; save) i=12 ;; copy) i=13 ;; cancel) i=14 ;; esac
  # After the separator the buttons are 11 further on.
  [ "$i" -ge 10 ] && extra=11 || extra=0
  bx=$(((2200 / scale - 499 + 20 + 32 * i + extra) * scale)); by=$(((1300 / scale + 28) * scale))
}
B() { button "$1"; xdotool mousemove "$bx" "$by"; sleep 0.15; xdotool click 1; sleep 0.25; }
at() { xdotool mousemove "$1" "$2"; sleep 0.25; echo "the pointer $3: $($READ cursor)"; }
saved() { ls -t "$HOME/Pictures/Screenshots/"*.png 2>/dev/null | head -1; }
{
  echo "# $(date +%F), $(basename "$HEYO"), $(xdpyinfo | awk '/dimensions/{print $2}') at scale $scale"
  HEYO_WARMUP=${HEYO_WARMUP:-4} "$HERE/heyo.sh" start "$@" > /dev/null || exit 1
  rm -rf "$HOME/Pictures/Screenshots"
  # A window of another program, to be selected by a click.
  zenity --info --title "A window" --text "A window to select" --width 600 --height 300 > /dev/null 2>&1 &
  note window $!
  sleep 1.5
  xdotool mousemove 300 1900; sleep 0.4
  png "$S/tools-before.png"

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
  cp "$(saved)" "$S/tools-drawn.png"
  /usr/bin/python3 -I - "$S/tools-before.png" "$S/tools-drawn.png" <<'PY'
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
  /usr/bin/python3 -I - "$S/tools-before.png" "$(saved)" <<'PY'
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
  png "$S/tools-moved.png"
  /usr/bin/python3 -I - "$S/tools-before.png" "$S/tools-moved.png" <<'PY'
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
  echo "the overlay is $(up); the file against the desktop there: $($READ same "$S/tools-before.png" "$(saved)" 1000 600 1200 700)"

  echo "## copied with Enter"
  select_it
  xdotool mousemove 300 1900; sleep 0.2
  xdotool key Return; sleep "$wait"
  echo "the overlay is $(up); on the clipboard: $($READ clipboard "$S/tools-copied.png")"
  echo "the clipboard's picture against the desktop there: $($READ same "$S/tools-before.png" "$S/tools-copied.png" 1000 600 1200 700)"

  echo "## copied with the button, something drawn on it"
  select_it
  B rect; drag 1100 700 1400 900
  B copy; sleep "$wait"
  echo "the overlay is $(up); on the clipboard: $($READ clipboard "$S/tools-copied-drawn.png")"
  /usr/bin/python3 -I -c 'import sys; from PIL import Image; print("the rectangle is in it:", Image.open(sys.argv[1]).convert("RGB").getpixel((100, 200)) == (255, 69, 58))' "$S/tools-copied-drawn.png"

  echo "## a click on a window"
  win=$(xdotool search --name '^A window$' | tail -1)
  set -- $(place "$win")
  # What is seen of it: its frame without the shadow's margin.
  set -- $(xprop -id "$win" _GTK_FRAME_EXTENTS | awk -F'[=,]' -v x="$1" -v y="$2" -v w="$3" -v h="$4" '{print x + $2, y + $4, w - $2 - $3, h - $4 - $5}')
  echo "the window is $3 x $4 at $1,$2"
  "$HEYO" --open heyo:screenshot; sleep 1.3
  xdotool mousemove $(($1 + $3 / 2)) $(($2 + $4 / 2)); sleep 0.4
  png "$RESULTS/screenshot-tools-$name-window.png" 0 0 $(($1 + $3 + 300)) $(($2 + $4 + 200))
  xdotool click 1; sleep 0.4
  xdotool mousemove 300 1900; sleep 0.2
  xdotool key Return; sleep "$wait"
  echo "on the clipboard: $($READ clipboard "$S/tools-window.png")"
  echo "against the desktop where the window is: $($READ same "$S/tools-before.png" "$S/tools-window.png" "$1" "$2" "$3" "$4")"

  echo "## the pointer's shape"
  "$HEYO" --open heyo:screenshot; sleep 1.3
  at 300 1900 "with nothing selected"
  drag 1000 600 2200 1300
  at 1500 900 "in the selection"
  at 1000 600 "on its top left handle"
  at 1600 600 "on its top handle"
  at 2200 600 "on its top right handle"
  at 2200 950 "on its right handle"
  button undo; at $((bx + 21 * scale)) "$by" "on the toolbar, between buttons"
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
  "$HEYO" --open heyo:screenshot; sleep 1.3
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
  "$HEYO" --open heyo:screenshot-text; sleep 1.3
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
  grep -E "heyo: (screenshot failed|the annotations|panicked|the screenshot overlay)" "$S/heyo.log"
  finish window > /dev/null
  "$HERE/heyo.sh" stop > /dev/null
} 2>&1 | tee "$out"

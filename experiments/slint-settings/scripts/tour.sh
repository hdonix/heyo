#!/bin/sh
# Pictures of the settings window's pages, the same way for the Slint one
# and the web one: each is opened by clicking its entry in the sidebar.
#
#   tour.sh NAME [PAGE…]     $RESULTS/NAME-<page>.png; all four when none is named
#
# Heyo must be running in the sandbox (heyo.sh start). The pages: general,
# shortcuts, dictation, screenshots, advanced, about, list (the
# applications), commands (Heyo's own), extension and extension2 (the first
# two installed: heyo.sh seed puts two there). Only ever in the sandbox: it
# clicks with xdotool.
. "$(dirname "$0")/lib.sh"
name=$1; shift
[ $# -gt 0 ] || set -- general dictation list about
scale=${GDK_SCALE:-2}
id=$(settings_window)
if [ -z "$id" ]; then
  timeout 10 "$HEYO" --open heyo:settings
  id=$(wait_window 100) || { echo "no settings window came up" >&2; exit 1; }
  sleep "${SETTLE:-2}"
fi
set -- "$@" -- $(place "$id")
pages=
while [ "$1" != -- ]; do pages="$pages $1"; shift; done
shift
X=$1; Y=$2; Wd=$3; Ht=$4
# click X Y: in the window's own logical pixels.
click() { xdotool mousemove $((X + $1 * scale)) $((Y + $2 * scale)) click 1; }
# The entry of the sidebar at a row: the pages, a gap, the extensions.
entry() { click 152 $((32 + 8 + 58 + $1 * 34 + 17 + ($1 >= 6 ? 14 : 0))); }
for page in $pages; do
  case $page in
    general) entry 0 ;;
    shortcuts) entry 1 ;;
    dictation) entry 2 ;;
    screenshots) entry 3 ;;
    advanced) entry 4 ;;
    about) entry 5 ;;
    list) entry 6 ;;
    commands) entry 7 ;;
    extension) entry 8 ;;
    extension2) entry 9 ;;
    *) echo "no page $page" >&2; exit 2 ;;
  esac
  # Away from what it would hover over.
  xdotool mousemove $((X + 560 * scale)) $((Y + 12 * scale))
  sleep "${SETTLE:-2}"
  mkdir -p "$(dirname "$RESULTS/$name")"
  python3 -I "$HERE/shot.py" "$DISPLAY" "$RESULTS/$name-$page.png" "$X" "$Y" "$Wd" "$Ht"
  echo "$RESULTS/$name-$page.png ${Wd}x${Ht}"
done

#!/bin/sh
# Does things to the settings window that is on the sandbox's screen, one
# after another, as a person would with pointer and keyboard:
#
#   do.sh click X Y      a click, at the window's own logical pixels
#   do.sh move X Y       the pointer there, without a click
#   do.sh drag X Y X Y   the button down at the first place, let go at the second
#   do.sh key KEYS       a key press, as xdotool names it (ctrl+shift+k)
#   do.sh keydown KEY    a key held, until keyup KEY
#   do.sh type TEXT      typing
#   do.sh wheel N        the wheel, N notches down (up when negative)
#   do.sh wait SECONDS
#   do.sh shot NAME      the window as $RESULTS/NAME.png
#   do.sh page NAME      a page by its entry in the sidebar (as tour.sh)
#
# Several may be given in one call. Only ever in the sandbox.
. "$(dirname "$0")/lib.sh"
scale=${GDK_SCALE:-2}
id=$(settings_window); [ -n "$id" ] || { echo "no settings window" >&2; exit 1; }
set -- "$@" -- $(place "$id")
todo=
while [ "$1" != -- ]; do todo="$todo \"$1\""; shift; done
shift
X=$1; Y=$2; Wd=$3; Ht=$4
eval "set -- $todo"
at() { xdotool mousemove $((X + $1 * scale)) $((Y + $2 * scale)); }
while [ $# -gt 0 ]; do
  case $1 in
    click) at "$2" "$3"; xdotool click 1; shift 3 ;;
    move) at "$2" "$3"; shift 3 ;;
    key) xdotool key "$2"; shift 2 ;;
    keydown) xdotool keydown "$2"; shift 2 ;;
    drag) at "$2" "$3"; xdotool mousedown 1; sleep 0.2; at "$4" "$5"; sleep 0.3; xdotool mouseup 1; shift 5 ;;
    keyup) xdotool keyup "$2"; shift 2 ;;
    type) xdotool type --delay 40 "$2"; shift 2 ;;
    wheel) if [ "$2" -ge 0 ]; then xdotool click --repeat "$2" --delay 30 5; else xdotool click --repeat "$((-$2))" --delay 30 4; fi; shift 2 ;;
    wait) sleep "$2"; shift 2 ;;
    shot) mkdir -p "$(dirname "$RESULTS/$2")"; python3 -I "$HERE/shot.py" "$DISPLAY" "$RESULTS/$2.png" "$X" "$Y" "$Wd" "$Ht"; echo "$RESULTS/$2.png"; shift 2 ;;
    page)
      case $2 in general) row=0 ;; shortcuts) row=1 ;; dictation) row=2 ;; screenshots) row=3 ;; advanced) row=4 ;; about) row=5 ;; list) row=6 ;; commands) row=7 ;; extension) row=8 ;; extension2) row=9 ;; extension3) row=10 ;; *) echo "no page $2" >&2; exit 2 ;; esac
      at 152 $((32 + 8 + 58 + row * 34 + 17 + (row >= 6 ? 14 : 0))); xdotool click 1; at 560 12; shift 2 ;;
    *) echo "do.sh: what is $1?" >&2; exit 2 ;;
  esac
done

#!/bin/sh
# Does things to the settings window that is on the sandbox's screen, one
# after another, as a person would with pointer and keyboard:
#
#   settings-do.sh page TITLE        a page by its entry in the sidebar ("General", "Heyo Commands")
#   settings-do.sh press ROLE NAME   a click on a control, found by what it is and says
#                                    ("push button" "Copy Diagnostics"; ask.py tree lists them)
#   settings-do.sh click X Y         a click, at the window's own logical pixels
#   settings-do.sh move X Y          the pointer there, without a click
#   settings-do.sh drag X Y X Y      the button down at the first place, let go at the second
#   settings-do.sh key KEYS          a key press, as xdotool names it (ctrl+shift+k)
#   settings-do.sh keydown KEY       a key held, until keyup KEY
#   settings-do.sh type TEXT         typing
#   settings-do.sh wheel N           the wheel, N notches down (up when negative)
#   settings-do.sh wait SECONDS
#   settings-do.sh shot NAME         the window as $RESULTS/NAME.png
#
# Several may be given in one call. A page and a control are asked of the
# window where they are (ask.py); a place in logical pixels is for what is
# the window's own: its edge, to drag it by.
. "$(dirname "$0")/env.sh"
scale=$(scale)
id=$(window '^Heyo Settings$'); [ -n "$id" ] || { echo "no settings window" >&2; exit 1; }
set -- "$@" -- $(place "$id")
todo=
while [ "$1" != -- ]; do todo="$todo \"$1\""; shift; done
shift
X=$1; Y=$2; Wd=$3; Ht=$4
eval "set -- $todo"
at() { xdotool mousemove $((X + $1 * scale)) $((Y + $2 * scale)); }
while [ $# -gt 0 ]; do
  case $1 in
    page) ask click "list item" "$2" "Heyo Settings" > /dev/null || exit 1; at 560 12; shift 2 ;;
    press) ask click "$2" "$3" "Heyo Settings" > /dev/null || exit 1; shift 3 ;;
    click) at "$2" "$3"; xdotool click 1; shift 3 ;;
    move) at "$2" "$3"; shift 3 ;;
    key) xdotool key "$2"; shift 2 ;;
    keydown) xdotool keydown "$2"; shift 2 ;;
    drag) at "$2" "$3"; xdotool mousedown 1; sleep 0.2; at "$4" "$5"; sleep 0.3; xdotool mouseup 1; shift 5 ;;
    keyup) xdotool keyup "$2"; shift 2 ;;
    type) xdotool type --delay 40 "$2"; shift 2 ;;
    wheel) if [ "$2" -ge 0 ]; then xdotool click --repeat "$2" --delay 30 5; else xdotool click --repeat "$((-$2))" --delay 30 4; fi; shift 2 ;;
    wait) sleep "$2"; shift 2 ;;
    shot) mkdir -p "$(dirname "$RESULTS/$2")"; png "$RESULTS/$2.png" "$X" "$Y" "$Wd" "$Ht"; echo "$RESULTS/$2.png"; shift 2 ;;
    *) echo "settings-do.sh: what is $1?" >&2; exit 2 ;;
  esac
done

#!/bin/sh
# Everything that is measured of the Slint settings against the web ones,
# one after another, in a sandbox that is up (heyo.sh up gnome, scale 1):
#
#   measure-all.sh NAME [open] [react] [scroll] [small]     all four when none is named
#
#   open     how long the settings take to come up, and their memory
#   react    from a click or a key to the frame that shows it
#   scroll   scrolling the long list, on all processors
#   small    react and scroll again on four small cores (PIN=0-3 here)
#
# The Slint window is that of src-tauri/target/variants/heyo-skia, the web
# one that of heyo-plain (linux-builds.sh makes both), unless HEYO_SLINT
# and HEYO_WEB say otherwise.
HERE=$(cd "$(dirname "$0")" && pwd)
W=$(cd "$HERE/../../.." && pwd)
name=$1; shift
[ $# -gt 0 ] || set -- open react scroll small
SLINT=${HEYO_SLINT:-$W/src-tauri/target/variants/heyo-skia}
WEB=${HEYO_WEB:-$W/src-tauri/target/variants/heyo-plain}
"$HERE/heyo.sh" seed
for what in "$@"; do
  case $what in
    open)
      HEYO=$SLINT "$HERE/measure-open.sh" slint "$name"
      HEYO=$WEB "$HERE/measure-open.sh" web "$name"
      ;;
    react)
      HEYO=$SLINT "$HERE/measure-react.sh" slint "$name"
      HEYO=$WEB "$HERE/measure-react.sh" web "$name"
      ;;
    scroll)
      HEYO=$SLINT "$HERE/measure-scroll.sh" slint "$name"
      HEYO=$WEB "$HERE/measure-scroll.sh" web "$name"
      ;;
    small)
      HEYO=$SLINT PIN=${SMALL:-0-3} "$HERE/measure-react.sh" slint "$name-small"
      HEYO=$WEB PIN=${SMALL:-0-3} "$HERE/measure-react.sh" web "$name-small"
      HEYO=$SLINT PIN=${SMALL:-0-3} "$HERE/measure-scroll.sh" slint "$name-small"
      HEYO=$WEB PIN=${SMALL:-0-3} "$HERE/measure-scroll.sh" web "$name-small"
      ;;
    *) echo "$0: what is $what?" >&2; exit 2 ;;
  esac
done

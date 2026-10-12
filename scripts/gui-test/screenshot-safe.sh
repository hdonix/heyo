#!/bin/sh
# That the screenshot overlay cannot trap whoever uses it. It covers the
# screen and holds the keyboard, so whatever happens to what draws it, the
# screen and the keyboard must come back:
#
#   screenshot-safe.sh escape   Escape while a drag is in progress
#   screenshot-safe.sh panic    Slint's thread panics in the middle of a drag
#   screenshot-safe.sh stall    Slint's thread stops answering in the middle of a drag
#   screenshot-safe.sh kill     Heyo is killed in the middle of a drag
#   screenshot-safe.sh all      each of them (the default)
#
# Each: a text field of another program (zenity) has the keyboard. The
# overlay comes up. A key typed then must not reach the field: the overlay
# holds the keyboard. A drag begins, and the trouble happens in the middle
# of it. Then "hello" and Enter are typed, and the field must have exactly
# that. It says how long after the trouble the overlay was off the screen,
# and whether Heyo runs and its palette opens afterwards.
#
# In the session that runs, or one of its own (GNOME Shell, 3840x2160: the
# screen must be at least 2000 by 1300); Heyo must not be running in it.
. "$(dirname "$0")/env.sh"
export SCREEN=${SCREEN:-3840x2160}
overlay() { xdotool search --name '^Heyo Screenshot$' 2>/dev/null | tail -1; }
FAILED=

one() {
  case=$1; shift
  echo "== $case"
  alive heyo && { echo "Heyo is running already" >&2; exit 1; }
  HEYO_TEST_WARMUP=${HEYO_TEST_WARMUP:-4} "$GUI/sandbox.sh" heyo "$@" || { echo "FAIL: Heyo did not start"; FAILED=yes; return 1; }
  # The other program: a text field that says what was typed into it.
  start_as field zenity --entry --title "Typed here" --text "typed here" > "$T/typed-$case" 2> /dev/null
  for _ in $(seq 50); do win=$(xdotool search --name '^Typed here$' 2>/dev/null | tail -1); [ -n "$win" ] && break; sleep 0.1; done
  sleep 0.5
  set -- $(place "$win")
  xdotool mousemove $(($1 + $3 / 2)) $(($2 + $4 / 2 - 10)) click 1
  sleep 0.3
  xdotool mousemove 600 1200
  heyo --open heyo:screenshot > /dev/null 2>&1
  for _ in $(seq 50); do id=$(overlay); viewable "$id" && break; sleep 0.1; done
  viewable "$id" || { echo "FAIL: the overlay did not come up"; FAILED=yes; }
  sleep 0.5
  # Held by the overlay: this must not reach the field.
  xdotool key a
  sleep 0.2
  xdotool mousemove 1000 600 mousedown 1
  sleep 0.1
  for step in 1 2 3 4 5 6 7 8; do xdotool mousemove $((1000 + step * 60)) $((600 + step * 40)); sleep 0.02; done
  began=$(now)
  case "$case" in
    escape) xdotool key Escape ;;
    # The one written down when it was started, and no other.
    kill) python3 "$GUI/ledger.py" kill heyo ;;
    # The tenth movement of the drag is where it is asked to happen.
    *) for step in 9 10 11 12 13 14; do xdotool mousemove $((1000 + step * 60)) $((600 + step * 40)); sleep 0.02; done ;;
  esac
  gone=
  for _ in $(seq 400); do viewable "$id" || { gone=$(($(now) - began)); break; }; sleep 0.02; done
  if [ -n "$gone" ]; then echo "the overlay was off the screen $gone ms after"; else echo "FAIL: the overlay is still on the screen after 8 s"; FAILED=yes; fi
  xdotool mouseup 1
  sleep 0.3
  xdotool type --delay 30 hello
  xdotool key Return
  for _ in $(seq 30); do alive field || break; sleep 0.1; done
  typed=$(cat "$T/typed-$case")
  if [ "$typed" = hello ]; then echo "PASS: the field has exactly \"hello\""; else echo "FAIL: the field has \"$typed\""; FAILED=yes; fi
  stop field > /dev/null
  if alive heyo; then echo "Heyo is running"; else echo "Heyo is not running"; fi
  case "$case" in
    escape)
      # And the next screenshot comes up as the first did.
      heyo --open heyo:screenshot > /dev/null 2>&1; sleep 1
      if viewable "$(overlay)"; then echo "the next screenshot comes up"; else echo "FAIL: the next screenshot did not come up"; FAILED=yes; fi
      xdotool key Escape; sleep 0.3
      viewable "$(overlay)" && { echo "FAIL: Escape did not close it"; FAILED=yes; }
      ;;
    panic | stall)
      # What is left of Heyo: its palette.
      heyo > /dev/null 2>&1; sleep 1
      if [ -n "$(window '^Heyo$')" ]; then echo "the palette opens"; else echo "FAIL: the palette did not open"; FAILED=yes; fi
      heyo > /dev/null 2>&1; sleep 0.5
      ;;
  esac
  grep -E "heyo: (panicked|Slint's thread|what draws|the screenshot overlay|HEYO_SLINT|screenshot failed)" "$T/app.log" | cut -c1-200
  # What a killed Heyo had started is ended with it here.
  stop heyo started-by-heyo > /dev/null
}

case "${1:-all}" in
  escape | panic | stall | kill | all) ;;
  *) sed -n '2,10p' "$0"; exit 2 ;;
esac
session_for gnome
echo "# $(date +%F), $BUILD, $(xdpyinfo | awk '/dimensions/{print $2}') at scale $(scale)"
case "${1:-all}" in
  escape) one escape ;;
  panic) one panic HEYO_SLINT_PANIC_IN_DRAG=1 ;;
  stall) one stall HEYO_SLINT_STALL_IN_DRAG=1 ;;
  kill) one kill ;;
  all) one escape; one panic HEYO_SLINT_PANIC_IN_DRAG=1; one stall HEYO_SLINT_STALL_IN_DRAG=1; one kill ;;
esac
leave
[ -z "$FAILED" ]

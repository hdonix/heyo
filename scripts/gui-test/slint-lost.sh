#!/bin/sh
# That nothing of Heyo's is left on the screen when what draws it stops
# answering. Heyo's own windows (the pill, a pinned capture, the screenshot
# overlay, the settings) are drawn by one thread; if it stops, what it had
# on the screen would stay as it was last drawn, above everything, with
# nothing able to press or close it. One watcher takes all of it off the
# screen from outside (src-tauri/src/slint_windows/watch.rs).
#
#   slint-lost.sh
#
# Heyo is started with two pictures pinned (HEYO_TEST_PIN) and with
# HEYO_SLINT_STALL_AFTER, which stops that thread for a minute so long
# after it began. A text field of another program (zenity) has the
# keyboard; the pill is brought up; the thread stops. Then the pill and
# both pictures must be off the screen, "hello" typed must be exactly what
# the field has, and Heyo must still run and open its palette. (The
# overlay's own trials are screenshot-safe.sh.)
#
# In the session that runs, or one of its own (GNOME Shell); Heyo must not
# be running in it.
. "$(dirname "$0")/env.sh"
FAILED=
pins() { xdotool search --name '^Heyo Pinned Screenshot$' 2>/dev/null; }
# How many of Heyo's own windows are on the screen.
shown() {
  count=0
  for id in $(pins) $(xdotool search --name '^Heyo Dictation$' 2>/dev/null); do viewable "$id" && count=$((count + 1)); done
  echo $count
}

session_for gnome
echo "# $(date +%F), $BUILD, $(xdpyinfo | awk '/dimensions/{print $2}') at scale $(scale)"
alive heyo && { echo "Heyo is running already" >&2; exit 1; }
HEYO_TEST_WARMUP=${HEYO_TEST_WARMUP:-4} "$GUI/sandbox.sh" heyo "HEYO_TEST_PIN=400,300,300,200;900,300,200,120" HEYO_SLINT_STALL_AFTER=20000 || { echo "FAIL: Heyo did not start"; exit 1; }
start_as field zenity --entry --title "Typed here" --text "typed here" > "$T/typed-lost" 2> /dev/null
for _ in $(seq 50); do win=$(xdotool search --name '^Typed here$' 2>/dev/null | tail -1); [ -n "$win" ] && break; sleep 0.1; done
sleep 0.5
set -- $(place "$win")
xdotool mousemove $(($1 + $3 / 2)) $(($2 + $4 / 2 - 10)) click 1
sleep 0.3
heyo --dictate > /dev/null 2>&1
for _ in $(seq 50); do [ "$(shown)" = 3 ] && break; sleep 0.1; done
if [ "$(shown)" = 3 ]; then echo "two pinned captures and the pill are on the screen"; else echo "FAIL: $(shown) of Heyo's three windows are on the screen"; FAILED=yes; fi
for _ in $(seq 600); do grep -q "HEYO_SLINT_STALL_AFTER asked" "$T/app.log" && break; sleep 0.05; done
began=$(now)
before=$(shown)
grep -q "HEYO_SLINT_STALL_AFTER asked" "$T/app.log" || { echo "FAIL: what draws them did not stop"; FAILED=yes; }
[ "$before" = 3 ] || { echo "FAIL: only $before of them were still up when it stopped"; FAILED=yes; }
gone=
for _ in $(seq 400); do [ "$(shown)" = 0 ] && { gone=$(($(now) - began)); break; }; sleep 0.02; done
if [ -n "$gone" ]; then echo "all three were off the screen $gone ms after it stopped"; else echo "FAIL: $(shown) of them are still on the screen after 8 s"; FAILED=yes; fi
xdotool type --delay 30 hello
xdotool key Return
for _ in $(seq 30); do alive field || break; sleep 0.1; done
typed=$(cat "$T/typed-lost")
if [ "$typed" = hello ]; then echo "PASS: the field has exactly \"hello\""; else echo "FAIL: the field has \"$typed\""; FAILED=yes; fi
stop field > /dev/null
if alive heyo; then echo "Heyo is running"; else echo "FAIL: Heyo is not running"; FAILED=yes; fi
heyo > /dev/null 2>&1; sleep 1
if [ -n "$(window '^Heyo$')" ]; then echo "the palette opens"; else echo "FAIL: the palette did not open"; FAILED=yes; fi
heyo > /dev/null 2>&1; sleep 0.5
grep -E "heyo: (panicked|what draws|.*could not be taken)" "$T/app.log" | cut -c1-200
stop heyo started-by-heyo > /dev/null
leave
[ -z "$FAILED" ]

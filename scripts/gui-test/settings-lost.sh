#!/bin/sh
# When what draws the settings is lost, Heyo must go on, and say so. Heyo
# is started so that Slint's thread panics a few seconds after it starts
# (HEYO_SLINT_PANIC_AFTER), or so that Slint cannot start at all
# (HEYO_SLINT_NO_START); then:
#
#   - the settings window must be gone from the screen, and Heyo running;
#   - the settings asked for again must bring up the palette, saying that
#     they need Heyo to be restarted (pictured, to be looked at);
#   - the palette must still answer afterwards.
#
#   settings-lost.sh panic|no-start [NAME]    what was seen goes to $RESULTS/lost-NAME.txt
#
# In the session that runs, or one of its own; Heyo must not be running in it.
. "$(dirname "$0")/env.sh"
how=${1:-panic}; name=${2:-$how}
session_for gnome
out=$RESULTS/lost-$name.txt
windows() { for id in $(xdotool search --onlyvisible --name '^Heyo' 2>/dev/null); do printf '"%s" %s; ' "$(xdotool getwindowname "$id")" "$(place "$id" | tr ' ' ',')"; done; echo; }
running() { if alive heyo; then echo "running"; else echo "NOT RUNNING"; fi; }
seed > /dev/null
{
  echo "# $(date +%F), $BUILD, $how"
  # The panic comes nine seconds after Slint began: with the settings up.
  if [ "$how" = panic ]; then lever=HEYO_SLINT_PANIC_AFTER=9000; else lever=HEYO_SLINT_NO_START=1; fi
  HEYO_TEST_WARMUP=4 "$GUI/sandbox.sh" heyo "$lever" || exit 1
  heyo --open heyo:settings > /dev/null 2>&1
  sleep 3
  echo "settings asked for: Heyo $(running); windows: $(windows)"
  png "$RESULTS/lost-$name-1.png"
  if [ "$how" = panic ]; then
    sleep 6
    echo "after the panic: Heyo $(running); windows: $(windows)"
    heyo --open heyo:settings > /dev/null 2>&1
    sleep 3
    echo "settings asked for again: Heyo $(running); windows: $(windows)"
    png "$RESULTS/lost-$name-2.png"
  fi
  xdotool key Escape; sleep 1; xdotool key Escape; sleep 1
  heyo > /dev/null 2>&1
  sleep 2
  echo "palette toggled after that: Heyo $(running); windows: $(windows)"
  xdotool key Escape; sleep 1
  echo "## what Heyo said"
  grep -v '^ \|^$' "$T/app.log" | grep -i "slint\|panic\|settings" | cut -c1-200
} > "$out" 2>&1
cat "$out"
stop heyo > /dev/null
leave

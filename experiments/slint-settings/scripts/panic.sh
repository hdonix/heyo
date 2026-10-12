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
#   panic.sh panic|no-start [NAME]    what was seen goes to $RESULTS/lost-NAME.txt
#
# In the sandbox only; Heyo must not be running in it.
. "$(dirname "$0")/lib.sh"
how=${1:-panic}; name=${2:-$how}
out=$RESULTS/lost-$name.txt
alive heyo && { echo "Heyo is running already" >&2; exit 1; }
windows() { for id in $(xdotool search --onlyvisible --name '^Heyo' 2>/dev/null); do printf '"%s" %s; ' "$(xdotool getwindowname "$id")" "$(place "$id" | tr ' ' ',')"; done; echo; }
running() { if alive heyo; then echo "running"; else echo "NOT RUNNING"; fi; }
{
  echo "# $(date +%F), $(basename "$HEYO"), $how"
  if [ "$how" = panic ]; then export HEYO_SLINT_PANIC_AFTER=6000; else export HEYO_SLINT_NO_START=1; fi
  "$HEYO" --hidden > "$S/heyo.log" 2>&1 &
  note heyo $!
  sleep "${HEYO_WARMUP:-6}"
  timeout 10 "$HEYO" --open heyo:settings
  sleep 3
  echo "settings asked for: Heyo $(running); windows: $(windows)"
  python3 -I "$HERE/shot.py" "$DISPLAY" "$RESULTS/lost-$name-1.png"
  if [ "$how" = panic ]; then
    sleep 6
    echo "after the panic: Heyo $(running); windows: $(windows)"
    timeout 10 "$HEYO" --open heyo:settings
    sleep 3
    echo "settings asked for again: Heyo $(running); windows: $(windows)"
    python3 -I "$HERE/shot.py" "$DISPLAY" "$RESULTS/lost-$name-2.png"
  fi
  xdotool key Escape; sleep 1; xdotool key Escape; sleep 1
  timeout 10 "$HEYO"
  sleep 2
  echo "palette toggled after that: Heyo $(running); windows: $(windows)"
  xdotool key Escape; sleep 1
  echo "## what Heyo said"
  grep -v '^ \|^$' "$S/heyo.log" | grep -i "slint\|panic\|settings" | cut -c1-200
  finish heyo
} > "$out" 2>&1
cat "$out"

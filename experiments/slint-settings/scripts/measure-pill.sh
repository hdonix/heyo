#!/bin/sh
# How long the dictation pill takes to be on the screen, and what Heyo holds
# in memory while it waits to be asked:
#
#   measure-pill.sh NAME [RUNS] [VAR=value…]     appends to $RESULTS/pill.tsv
#
# Heyo (the build under test) is started hidden in the sandbox, with spoken
# sentences in place of a microphone (scripts/gui-test/say.py) and the
# speech engine installed for the Heyo of whoever runs this (read, never
# written). Then, RUNS times (five unless said): a recording is asked for
# as the hotkey asks (`heyo --dictate`, a second process handing the
# request to the first), and the screen itself is the clock. $WATCH (the
# `overlay-bench --watch` of the overlay experiment, on the branch
# overlay-renderer) reads two pixels over and over. One is in the middle of
# the pill: `begins_ms` is when it is no longer the desktop's (the pill, or
# a letter on it). The other is just inside the pill's top edge, where no
# text or bar is: `there_ms` is when that is the pill's black, which for a
# pill that arrives by rising is when it has arrived. The recording is
# then finished the same way.
#
# Memory is what the kernel says Heyo and everything it started hold, each
# page counted by the share of it they have (Pss), and Heyo's own resident
# memory: after it has started and before any recording, and after the last.
#
# In the sandbox of scripts/gui-test, in a session that runs: the screen
# must be 1920x1080 at scale 1 (scripts/gui-test/sandbox.sh session gnome).
# It stays here, with what it measured (../results/pill.tsv): it is the
# record of how the pill was chosen, not a trial of Heyo.
GUI=$(cd "$(dirname "$0")/../../../scripts/gui-test" && pwd)
. "$GUI/env.sh"
RESULTS=${HEYO_TEST_RESULTS:-$REPO/experiments/slint-settings/results}
name=$1; runs=${2:-5}
shift; [ $# -gt 0 ] && shift
WATCH=${WATCH:-$REPO/experiments/overlay-bench/target/release/overlay-bench}
[ -x "$WATCH" ] || { echo "no $WATCH: name the watching program with WATCH" >&2; exit 1; }
alive Xvfb || { echo "no sandbox runs on $DISPLAY" >&2; exit 1; }
alive heyo && { echo "Heyo is running already" >&2; exit 1; }
out=$RESULTS/pill.tsv
[ -f "$out" ] || printf 'name\trun\tbegins_ms\tthere_ms\tpss_idle_mb\trss_idle_mb\tthreads_idle\tpss_after_mb\trss_after_mb\n' > "$out"

speech
seed dictationLive=false dictationPolish=off dictationLanguage=en > /dev/null
/usr/bin/python3 "$GUI/say.py" "$T/spoken.wav" "The weather is nice today, so let us walk to the park." > /dev/null || exit 1
export HEYO_DICTATION_WAV="$T/spoken.wav"
HEYO_TEST_WARMUP=${HEYO_TEST_WARMUP:-10} "$GUI/sandbox.sh" heyo "$@" || exit 1
heyo=$(pid heyo)

pss() {
  total=0
  for member in $(python3 "$GUI/ledger.py" family heyo); do
    part=$(awk '/^Pss:/ {print $2}' "/proc/$member/smaps_rollup" 2>/dev/null)
    total=$((total + ${part:-0}))
  done
  awk -v kb="$total" 'BEGIN { printf "%.1f", kb / 1024 }'
}
rss() { awk '/^VmRSS/ {printf "%.1f", $2 / 1024}' "/proc/$heyo/status"; }

# The pill's window is 360 by 44, at the bottom centre, a tenth of the
# screen's height above the bottom; the pill is 30 high in its middle.
set -- $(xdotool getdisplaygeometry)
PX=$(($1 / 2)); PY=$(($2 - 44 - $2 / 10 + 7 + 2)); MY=$(($2 - 44 - $2 / 10 + 22))
idle_pss=$(pss); idle_rss=$(rss); idle_threads=$(ls "/proc/$heyo/task" | wc -l)
for run in $(seq "$runs"); do
  start_as watch "$WATCH" --watch "$PX,$MY:$PX,$PY" 5 > "$T/watch.log" 2>/dev/null
  watcher=$STARTED
  sleep 1
  asked=$(now)
  heyo --dictate
  sleep 2.5
  heyo --dictate
  wait "$watcher" 2>/dev/null
  # What the pixel was before it was asked for is the desktop's.
  begins=$(awk -v t="$asked" '$2 != 0 { next } $1 < t { r = $3; g = $4; b = $5 } $1 >= t && (($3 - r) ^ 2 + ($4 - g) ^ 2 + ($5 - b) ^ 2 > 144) { print $1 - t; exit }' "$T/watch.log")
  there=$(awk -v t="$asked" '$2 == 1 && $1 >= t && $3 < 40 && $4 < 40 && $5 < 40 { print $1 - t; exit }' "$T/watch.log")
  cp "$T/watch.log" "$T/watch-pill-$name-$run.log"
  sleep 4
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t\t\n' "$name" "$run" "${begins:--}" "${there:--}" "$idle_pss" "$idle_rss" "$idle_threads" >> "$out"
done
printf '%s\tafter\t\t\t\t\t\t%s\t%s\n' "$name" "$(pss)" "$(rss)" >> "$out"
grep -i "slint\|dictation window" "$T/app.log" | cut -c1-160
stop heyo watch > /dev/null
tail -n "$((runs + 1))" "$out"

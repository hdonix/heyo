#!/bin/sh
# How long the settings take to come up, and what they cost in memory, the
# same way for the Slint window and the web one:
#
#   measure-open.sh slint|web NAME [RUNS]     appends to $RESULTS/open.tsv
#
# Heyo (the build $HEYO) is started hidden in the sandbox. Then, RUNS times
# (five unless said): the settings are asked for as the palette asks
# (`heyo --open heyo:settings`, a second process handing the request to the
# first), and the screen itself is the clock: `overlay-bench --watch` reads
# two pixels over and over, one in the empty part of the page (when it is
# no longer the desktop, the window is on screen) and one on the switch of
# "Show What Is in Progress", which is on (when it is the accent colour, the
# page has been drawn). The window is then closed with Esc.
#
# Memory is what the kernel says Heyo and everything it started hold, each
# page counted by the share of it they have (Pss): before the settings were
# ever asked for, with them open, and after they were closed.
#
# The theme must be light (heyo.sh seed), the scale 1.
. "$(dirname "$0")/lib.sh"
by=$1; name=$2; runs=${3:-5}
WATCH=$OB/target/release/overlay-bench
[ -x "$WATCH" ] || { echo "no $WATCH: cargo build --release in experiments/overlay-bench" >&2; exit 1; }
alive heyo && { echo "Heyo is running already" >&2; exit 1; }
out=$RESULTS/open.tsv
[ -f "$out" ] || printf 'name\tby\trun\twindow_ms\tdrawn_ms\tpss_before_mb\tpss_open_mb\tpss_closed_mb\theyo_rss_before_mb\theyo_rss_open_mb\theyo_rss_closed_mb\tprocesses_open\n' > "$out"

[ "$by" = slint ] && export HEYO_SETTINGS=slint
${PIN:+taskset -c "$PIN"} "$HEYO" --hidden > "$S/heyo.log" 2>&1 &
note heyo $!
heyo=$(cut -d' ' -f1 "$D/pids/heyo")
sleep "${HEYO_WARMUP:-8}"

# Pss of Heyo and all it started, in MB; and Heyo's own resident memory.
pss() {
  total=0
  for pid in $heyo $(family "$heyo" | cut -d: -f1); do
    part=$(awk '/^Pss:/ {print $2}' "/proc/$pid/smaps_rollup" 2>/dev/null)
    total=$((total + ${part:-0}))
  done
  awk -v kb="$total" 'BEGIN { printf "%.1f", kb / 1024 }'
}
rss() { awk '/^VmRSS/ {printf "%.1f", $2 / 1024}' "/proc/$heyo/status"; }

# Where the window will be: in the middle of the screen.
set -- $(xdotool getdisplaygeometry)
X=$((($1 - 924) / 2)); Y=$((($2 - 704) / 2))
P1="$((X + 580)),$((Y + 655))"; P2="$((X + 838)),$((Y + 598))"
before_pss=$(pss); before_rss=$(rss)
for run in $(seq "$runs"); do
  "$WATCH" --watch "$P1:$P2" 6 > "$S/watch.log" 2>/dev/null &
  watcher=$!; note watch $watcher
  sleep 1
  asked=$(now)
  timeout 10 "$HEYO" --open heyo:settings
  sleep 3.5
  if [ "$run" = 1 ]; then open_pss=$(pss); open_rss=$(rss); processes=$(family "$heyo" | wc -l); fi
  wait "$watcher" 2>/dev/null; rm -f "$D/pids/watch"
  # The page's own white, then the accent of the switch that is on.
  window=$(awk -v t="$asked" '$1 >= t && $2 == 0 && $3 > 235 && $4 > 235 && $5 > 235 { print $1 - t; exit }' "$S/watch.log")
  drawn=$(awk -v t="$asked" '$1 >= t && $2 == 1 && $3 > 190 && $4 < 110 && $5 < 80 { print $1 - t; exit }' "$S/watch.log")
  cp "$S/watch.log" "$S/watch-$name-$run.log"
  xdotool key Escape
  sleep 2
  if [ "$run" = 1 ]; then closed_pss=$(pss); closed_rss=$(rss); fi
  if [ "$run" = 1 ]; then
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$name" "$by" "$run" "${window:--}" "${drawn:--}" "$before_pss" "$open_pss" "$closed_pss" "$before_rss" "$open_rss" "$closed_rss" "$processes" >> "$out"
  else
    printf '%s\t%s\t%s\t%s\t%s\t\t\t\t\t\t\t\n' "$name" "$by" "$run" "${window:--}" "${drawn:--}" >> "$out"
  fi
done
grep -i "slint" "$S/heyo.log" | cut -c1-200
finish heyo
tail -n "$runs" "$out"

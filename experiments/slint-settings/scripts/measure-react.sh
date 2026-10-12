#!/bin/sh
# From a click or a key to the frame that shows it, the same way for the
# Slint settings and the web ones:
#
#   measure-react.sh slint|web NAME [RUNS]     appends to $RESULTS/react.tsv
#
# Three things are done, RUNS times each (seven unless said), and the
# screen itself is the clock (`overlay-bench --watch` reads a pixel over
# and over and says when it changes):
#
#   switch   "Compact Mode" is clicked; the pixels are across the switch
#   page     "Dictation", then "General", is clicked in the sidebar; the
#            pixels are along the page's title and one in the page
#   typing   a letter is typed into the sidebar's search (then deleted);
#            the pixels are on the first entry, which becomes another
#
# The time is from just before xdotool is started with the click or the
# key (so xdotool's own start, a few ms, is in it, the same for both) to
# the first change of any of the pixels. An animation counts from the
# first frame that changes one of them.
# The theme must be light (heyo.sh seed), the scale 1; PIN names the
# processors Heyo may use.
. "$(dirname "$0")/lib.sh"
by=$1; name=$2; runs=${3:-7}
WATCH=$OB/target/release/overlay-bench
alive heyo && { echo "Heyo is running already" >&2; exit 1; }
out=$RESULTS/react.tsv
[ -f "$out" ] || printf 'name\tby\tprocessors\twhat\tms_median\tms_worst\tms_each\n' > "$out"

[ "$by" = slint ] && export HEYO_SETTINGS=slint
${PIN:+taskset -c "$PIN"} "$HEYO" --hidden > "$S/heyo.log" 2>&1 &
note heyo $!
sleep "${HEYO_WARMUP:-8}"
timeout 10 "$HEYO" --open heyo:settings
id=$(wait_window 100) || { echo "no settings window came up" >&2; finish heyo; exit 1; }
sleep 3
set -- $(place "$id"); X=$1; Y=$2

# react X Y PLACES: clicks at X Y (or, with `key K` for X Y, presses a key)
# and prints how long it took for the first of the places (each PX,PY in
# the window) to change.
react() {
  places=
  for place in $3; do places="$places${places:+:}$((X + ${place%,*})),$((Y + ${place#*,}))"; done
  "$WATCH" --watch "$places" 2.2 > "$S/react.log" 2>/dev/null &
  watcher=$!; note watch $watcher
  sleep 0.7
  if [ "$1" = key ]; then
    at=$(now); xdotool key "$2"
  else
    xdotool mousemove $((X + $1)) $((Y + $2)); sleep 0.25
    at=$(now); xdotool click 1
  fi
  wait "$watcher" 2>/dev/null; rm -f "$D/pids/watch"
  awk -v t="$at" '$1 >= t { print $1 - t; found = 1; exit } END { if (!found) print "-" }' "$S/react.log"
}
row() { # what, then the times
  what=$1; shift
  printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$name" "$by" "${PIN:-all}" "$what" "$(printf '%s\n' "$@" | grep -v '^-$' | sort -n | awk '{ t[NR] = $1 } END { if (NR) printf "%d\t%d", t[int((NR + 1) / 2)], t[NR]; else printf "-\t-" }')" "$*" >> "$out"
}

times=
# Across the switch: wherever the knob is, some of these are beside it.
for _ in $(seq "$runs"); do times="$times $(react 846 372 "830,372 836,372 842,372 848,372 854,372 860,372")"; sleep 0.9; done
row switch $times
times=
for run in $(seq "$runs"); do
  # Along the page's title, which is another word at once, and in the page.
  title="302,56 308,56 314,56 320,56 326,56 332,56 338,56 344,56 350,56 356,56 700,100"
  if [ $((run % 2)) = 1 ]; then times="$times $(react 152 183 "$title")"; else times="$times $(react 152 115 "$title")"; fi
  sleep 0.8
done
row page $times
# Into the search field, and the pointer off the entries.
xdotool mousemove $((X + 150)) $((Y + 72)) click 1; sleep 0.5
times=
for _ in $(seq "$runs"); do
  times="$times $(react key d "62,106 100,115 110,115 120,115")"; sleep 0.5
  xdotool key BackSpace; sleep 0.8
done
row typing $times
xdotool key Escape; sleep 0.5; xdotool key Escape; sleep 1
finish heyo
tail -3 "$out"

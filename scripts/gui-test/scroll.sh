#!/bin/sh
# How smoothly lists scroll: the root search with everything listed, an
# extension's grid (emoji) and a list with many rows (clipboard history is
# empty here, so the Store's is not used; the root list stands for lists).
# Frame times are measured in the webview (src/dev/bench.ts). The sandbox
# renders in software, which is what a machine without a usable GPU does.
. "$(dirname "$0")/env.sh"
export HEYO_BENCH_OUT="$T/bench.txt"; rm -f "$HEYO_BENCH_OUT"
"$HERE/sandbox.sh" up
for _ in 1 2 3 4 5 6; do
  [ -n "$(xdotool search --onlyvisible --name '^Heyo$' 2>/dev/null)" ] && break
  heyo; w 3
done
bench() { xdotool key ctrl+alt+shift+b; w 4.5; echo "$1 -> $(tail -1 "$HEYO_BENCH_OUT" 2>/dev/null)"; }
w 1; bench "root list"
bench "root list again"
heyo --open cmd:@heyo:emoji; w 3; bench "emoji grid"
bench "emoji grid again"
# Walking the selection down a long list with the arrow key held.
walk() { xdotool key ctrl+alt+shift+n; xdotool key --delay 33 --repeat "${3:-70}" "$2"; w 1.5; echo "$1 -> $(tail -1 "$HEYO_BENCH_OUT" 2>/dev/null)"; }
walk "emoji grid, Down held" Down 60
xdotool key Escape; w 1; xdotool key ctrl+a BackSpace; xdotool type --delay 20 "e"; w 1
walk "root list, Down held" Down 45
"$HERE/sandbox.sh" down > /dev/null

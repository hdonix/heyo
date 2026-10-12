#!/bin/sh
# How soon the screenshot overlay is up and how closely the selection
# follows the pointer, with the screen as the clock:
#
#   measure-screenshot.sh LABEL all|small RUNS small|large|open [VAR=value…]
#
# Starts Heyo (the build $HEYO) hidden in the sandbox, on all processors
# or on four small ones (PIN names them, 0-3 unless said), runs
# measure-screenshot.py (which says what is measured) and ends Heyo. One
# line of JSON is appended to $RESULTS/screenshot.jsonl. (The lines there
# labelled "web" are the web view's overlay, from when there was one.)
#
# The screen must be 3840x2160 at scale 1 under GNOME Shell:
# SCREEN=3840x2160 GDK_SCALE=1 heyo.sh up gnome. $WATCH is the
# `overlay-bench --watch` of the overlay experiment (branch
# overlay-renderer).
. "$(dirname "$0")/lib.sh"
[ $# -ge 4 ] || { sed -n '2,15p' "$0"; exit 2; }
label=$1; cores=$2; runs=$3; size=$4; shift 4
export WATCH=${WATCH:-$W/experiments/overlay-bench/target/release/overlay-bench}
[ -x "$WATCH" ] || { echo "no $WATCH: name the watching program with WATCH" >&2; exit 1; }
alive heyo && { echo "Heyo is running already" >&2; exit 1; }
[ "$cores" = small ] && export PIN=${PIN:-0-3}
export HEYO MEASURE_OUT="$S/screenshot" MEASURE_FRAMES="$S/screenshot-frames.txt" MEASURE_LOG="$S/heyo.log" MEASURE_RESULTS="$RESULTS/screenshot.jsonl"
rm -f "$MEASURE_FRAMES"
"$HERE/heyo.sh" start HEYO_OVERLAY_DEBUG=1 "HEYO_SETTINGS_FRAMES=$MEASURE_FRAMES" "$@" > /dev/null || exit 1
python3 -I "$HERE/measure-screenshot.py" "$label" "$runs" "$size"
"$HERE/heyo.sh" stop > /dev/null

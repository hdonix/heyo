#!/bin/sh
# How soon the screenshot overlay is up and how closely the selection
# follows the pointer, with the screen as the clock:
#
#   measure-screenshot.sh LABEL all|small RUNS small|large|open [VAR=value…]
#
# Starts Heyo (the build under test) hidden in the sandbox, on all
# processors or on four small ones (PIN names them, 0-3 unless said), runs
# measure-screenshot.py (which says what is measured) and ends Heyo. One
# line of JSON is appended to $RESULTS/screenshot.jsonl. (The lines there
# labelled "web" are the web view's overlay, from when there was one.)
#
# In the sandbox of scripts/gui-test, in a session that runs: the screen
# must be 3840x2160 at scale 1 under GNOME Shell
# (SCREEN=3840x2160 scripts/gui-test/sandbox.sh session gnome). $WATCH is
# the `overlay-bench --watch` of the overlay experiment (branch
# overlay-renderer). It stays here, with what it measured
# (../results/screenshot.jsonl): it is the record of how the overlay was
# chosen, not a trial of Heyo.
GUI=$(cd "$(dirname "$0")/../../../scripts/gui-test" && pwd)
. "$GUI/env.sh"
RESULTS=${HEYO_TEST_RESULTS:-$REPO/experiments/slint-settings/results}
[ $# -ge 4 ] || { sed -n '2,19p' "$0"; exit 2; }
label=$1; cores=$2; runs=$3; size=$4; shift 4
export WATCH=${WATCH:-$REPO/experiments/overlay-bench/target/release/overlay-bench}
[ -x "$WATCH" ] || { echo "no $WATCH: name the watching program with WATCH" >&2; exit 1; }
alive Xvfb || { echo "no sandbox runs on $DISPLAY" >&2; exit 1; }
alive heyo && { echo "Heyo is running already" >&2; exit 1; }
[ "$cores" = small ] && export PIN=${PIN:-0-3}
export GUI MEASURE_OUT="$T/screenshot" MEASURE_FRAMES="$T/screenshot-frames.txt" MEASURE_LOG="$T/app.log" MEASURE_RESULTS="$RESULTS/screenshot.jsonl"
rm -f "$MEASURE_FRAMES"
"$GUI/sandbox.sh" heyo HEYO_OVERLAY_DEBUG=1 "HEYO_SETTINGS_FRAMES=$MEASURE_FRAMES" "$@" || exit 1
python3 "$HERE/measure-screenshot.py" "$label" "$runs" "$size"
stop heyo > /dev/null

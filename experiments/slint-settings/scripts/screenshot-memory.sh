#!/bin/sh
# What Heyo itself holds in memory before, during and after screenshots:
#
#   screenshot-memory.sh [SCREENSHOTS]    six unless said
#
# For each screenshot a selection is dragged out and given up with Escape.
# Said each time: Heyo's resident memory as the kernel counts it (all of
# it, what is its own, what is shared memory: the window's buffer is), and
# how many mappings hold 28 MB or more (a 4K screen is 31.6 MB) with what
# each holds. Then once more after the overlay has rested (70 s), and with
# one more screenshot after that. (results/screenshot-memory.txt has the
# same of the web view's overlay, from when there was one.)
#
# In the sandbox of scripts/gui-test, in a session that runs
# (SCREEN=3840x2160 scripts/gui-test/sandbox.sh session gnome, scale 1);
# Heyo must not be running in it. It stays here, with what it measured
# (../results/screenshot-memory.txt): it is the record of how the overlay
# was chosen, not a trial of Heyo.
GUI=$(cd "$(dirname "$0")/../../../scripts/gui-test" && pwd)
. "$GUI/env.sh"
count=${1:-6}
alive Xvfb || { echo "no sandbox runs on $DISPLAY" >&2; exit 1; }
alive heyo && { echo "Heyo is running already" >&2; exit 1; }
"$GUI/sandbox.sh" heyo || exit 1
heyo=$(pid heyo)
held() {
  awk '/VmRSS|RssAnon|RssShmem/{printf "%s %d MB  ", $1, $2 / 1024}' "/proc/$heyo/status"
  awk '/^[0-9a-f]+-[0-9a-f]+ /{name=$6} /^Rss:/{if ($2 >= 28000 && name !~ /\.so|heyo$/) {n++; all = all " " int($2 / 1024)}} END{printf "| %d large:%s\n", n, all}' "/proc/$heyo/smaps"
}
one() {
  heyo --open heyo:screenshot 2> /dev/null; sleep 1.5
  xdotool mousemove 1000 600 mousedown 1; sleep 0.1; xdotool mousemove 1500 900; sleep 0.1; xdotool mousemove 2200 1300; sleep 0.3; xdotool mouseup 1; sleep 0.3
  echo "$1 up, a selection made: $(held)"
  xdotool key Escape; sleep 3
  echo "$1 closed, 3 s later:    $(held)"
}
echo "# $(date +%F), $BUILD, $(xdpyinfo | awk '/dimensions/{print $2}')"
xdotool mousemove 300 1900
echo "before any:                $(held)"
for n in $(seq "$count"); do one "$n"; done
sleep 70
echo "70 s later:                $(held)"
one "one more"
stop heyo > /dev/null

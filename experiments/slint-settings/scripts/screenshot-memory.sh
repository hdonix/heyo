#!/bin/sh
# What Heyo itself holds in memory before, during and after screenshots:
#
#   screenshot-memory.sh [slint|web] [SCREENSHOTS]    six unless said
#
# For each screenshot a selection is dragged out and given up with Escape.
# Said each time: Heyo's resident memory as the kernel counts it (all of
# it, what is its own, what is shared memory: the window's buffer is), and
# how many mappings hold 28 MB or more (a 4K screen is 31.6 MB) with what
# each holds. Then once more after the overlay has rested (70 s), and with
# one more screenshot after that. The web view's own processes are not
# counted, for either overlay.
#
# In the sandbox only (SCREEN=3840x2160 GDK_SCALE=1 heyo.sh up gnome);
# Heyo must not be running in it.
. "$(dirname "$0")/lib.sh"
which=${1:-slint}; count=${2:-6}
alive heyo && { echo "Heyo is running already" >&2; exit 1; }
if [ "$which" = slint ]; then set -- HEYO_SCREENSHOT=slint; else set --; fi
HEYO_WARMUP=${HEYO_WARMUP:-6} "$HERE/heyo.sh" start "$@" > /dev/null 2>&1 || exit 1
read -r heyo heyo_born < "$D/pids/heyo"
held() {
  awk '/VmRSS|RssAnon|RssShmem/{printf "%s %d MB  ", $1, $2 / 1024}' "/proc/$heyo/status"
  awk '/^[0-9a-f]+-[0-9a-f]+ /{name=$6} /^Rss:/{if ($2 >= 28000 && name !~ /\.so|heyo$/) {n++; all = all " " int($2 / 1024)}} END{printf "| %d large:%s\n", n, all}' "/proc/$heyo/smaps"
}
one() {
  "$HEYO" --open heyo:screenshot 2> /dev/null; sleep 1.5
  xdotool mousemove 1000 600 mousedown 1; sleep 0.1; xdotool mousemove 1500 900; sleep 0.1; xdotool mousemove 2200 1300; sleep 0.3; xdotool mouseup 1; sleep 0.3
  echo "$1 up, a selection made: $(held)"
  xdotool key Escape; sleep 3
  echo "$1 closed, 3 s later:    $(held)"
}
echo "# $(date +%F), $(basename "$HEYO"), the overlay of $which, $(xdpyinfo | awk '/dimensions/{print $2}')"
xdotool mousemove 300 1900
echo "before any:                $(held)"
for n in $(seq "$count"); do one "$n"; done
sleep 70
echo "70 s later:                $(held)"
one "one more"
"$HERE/heyo.sh" stop > /dev/null

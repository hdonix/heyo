#!/bin/sh
# Scrolling the long list (the applications, with their icons), the same
# way for the Slint window and the web one:
#
#   measure-scroll.sh slint|web NAME      appends to $RESULTS/scroll.tsv
#
# The sandbox's applications are those of this computer and 200 more made
# for the purpose (each with an icon of this computer's), so that the list
# is a few hundred rows whatever is installed. The wheel is turned 60
# notches down, one every 30 ms, then 60 up, one every 16 ms, with the
# pointer over the list.
#
# What is measured: how often the screen showed the list somewhere new
# while the wheel turned (`overlay-bench --watch` on eight places in the
# rows: the gaps between one change and the next), the processor time Heyo
# and all it started used (so the web view's own processes count), and, for
# the Slint window, how long each frame took (Heyo is started with
# HEYO_SETTINGS_FRAMES, see src-tauri/src/slint_settings/window.rs). PIN
# names the processors Heyo may use: PIN=0-3 is four small cores here.
. "$(dirname "$0")/lib.sh"
by=$1; name=$2
WATCH=$OB/target/release/overlay-bench
alive heyo && { echo "Heyo is running already" >&2; exit 1; }
out=$RESULTS/scroll.tsv
[ -f "$out" ] || printf 'name\tby\tprocessors\tgesture\tseconds\tcpu_seconds\tscreen_updates\tgap_ms_median\tgap_ms_p95\tgap_ms_worst\tframes\tframe_ms_median\tframe_ms_p95\tframe_ms_worst\tover_16ms\n' > "$out"

# The 200 more applications.
apps=$XDG_DATA_HOME/applications
if [ ! -f "$apps/sample-200.desktop" ]; then
  mkdir -p "$apps"
  icons=$(ls /usr/share/icons/hicolor/48x48/apps/*.png /usr/share/icons/hicolor/scalable/apps/*.svg /usr/share/pixmaps/*.png 2>/dev/null)
  count=$(echo "$icons" | wc -l)
  for n in $(seq 200); do
    icon=$(echo "$icons" | sed -n "$((n % count + 1))p")
    printf '[Desktop Entry]\nType=Application\nName=Sample Application %03d\nComment=Made for measuring the list, number %d\nExec=true\nIcon=%s\n' "$n" "$n" "$icon" > "$apps/sample-$n.desktop"
  done
fi

[ "$by" = slint ] && export HEYO_SETTINGS=slint HEYO_SETTINGS_FRAMES="$S/frames.txt"
rm -f "$S/frames.txt" "$S/gestures.txt"
${PIN:+taskset -c "$PIN"} "$HEYO" --hidden > "$S/heyo.log" 2>&1 &
note heyo $!
heyo=$(cut -d' ' -f1 "$D/pids/heyo")
sleep "${HEYO_WARMUP:-8}"
# Processor time of Heyo and all it started so far, in clock ticks.
ticks() {
  total=0
  for pid in $heyo $(family "$heyo" | cut -d: -f1); do
    part=$(sed 's/.*) //' "/proc/$pid/stat" 2>/dev/null | awk '{print $12 + $13}')
    total=$((total + ${part:-0}))
  done
  echo "$total"
}
hz=$(getconf CLK_TCK)

timeout 10 "$HEYO" --open heyo:settings
id=$(wait_window 100) || { echo "no settings window came up" >&2; finish heyo; exit 1; }
sleep 2
set -- $(place "$id"); X=$1; Y=$2
scale=${GDK_SCALE:-2}
at() { xdotool mousemove $((X + $1 * scale)) $((Y + $2 * scale)); }
at 152 333; xdotool click 1
sleep 4
at 560 420
sleep 1
# Eight places in the rows, where their titles and descriptions are: when
# any of them changes, the screen has shown the list somewhere new. (The
# watcher counts in the screen's logical pixels.)
PLACES=
for n in 0 1 2 3 4 5 6 7; do PLACES="$PLACES${PLACES:+:}$((X / scale + 345 + n * 14)),$((Y / scale + 230 + n * 52))"; done

gesture() { # name notches delay button
  "$WATCH" --watch "$PLACES" "$(awk -v n="$2" -v d="$3" 'BEGIN { printf "%.1f", n * d / 1000 + 2.5 }')" > "$S/scroll-watch-$1.log" 2>/dev/null &
  watcher=$!; note watch $watcher
  sleep 1
  before=$(ticks); from=$(now)
  xdotool click --repeat "$2" --delay "$3" "$4"
  sleep 0.5
  to=$(now); after=$(ticks)
  wait "$watcher" 2>/dev/null; rm -f "$D/pids/watch"
  echo "$1 $from $to $(awk -v a="$before" -v b="$after" -v hz="$hz" 'BEGIN { printf "%.2f", (b - a) / hz }')" >> "$S/gestures.txt"
}
gesture down-30ms 60 30 5
sleep 1
python3 -I "$HERE/shot.py" "$DISPLAY" "$S/scroll-$name.png" "$X" "$Y" $((924 * scale)) $((704 * scale))
gesture up-16ms 60 16 4
sleep 1
# Closing the window writes the frames down.
xdotool key Escape
sleep 1.5

# count, median, 95th and worst of the numbers given a line each.
spread() {
  sort -n | awk '{ t[NR] = $1 } END {
    if (!NR) { printf "0\t\t\t"; exit }
    p = int(NR * 0.95); if (p < 1) p = 1
    printf "%d\t%s\t%s\t%s", NR, t[int((NR + 1) / 2)], t[p], t[NR] }'
}
while read -r what from to cpu; do
  # The gaps between one change of the screen and the next, while the wheel turned.
  # (Places that change within 6 ms of one another changed in one frame.)
  gaps=$(awk -v from="$from" -v to="$to" '$1 >= from && $1 <= to && $1 - last > 6 { if (last) print $1 - last; last = $1 }' "$S/scroll-watch-$what.log" | spread)
  cp "$S/scroll-watch-$what.log" "$S/scroll-watch-$name-$by-$what.log"
  if [ -f "$S/frames.txt" ]; then
    frames=$(awk -v from="$from" -v to="$to" '$1 >= from && $1 <= to { print $2 }' "$S/frames.txt" | spread)
    late=$(awk -v from="$from" -v to="$to" '$1 >= from && $1 <= to && $2 > 16.7 { n++ } END { print n + 0 }' "$S/frames.txt")
  else
    frames=$(printf '\t\t\t'); late=
  fi
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$name" "$by" "${PIN:-all}" "$what" "$(awk -v a="$from" -v b="$to" 'BEGIN { printf "%.2f", (b - a) / 1000 }')" "$cpu" "$gaps" "$frames" "$late" >> "$out"
done < "$S/gestures.txt"
[ -f "$S/frames.txt" ] && cp "$S/frames.txt" "$S/frames-$name.txt"
finish heyo
tail -2 "$out"

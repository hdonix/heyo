#!/bin/sh
# The pictures that set the Slint settings window beside the web one: the
# pages (and the vocabulary, further down Dictation, where the Chinese is), in
# the light theme and the dark, each taken from both windows the same way
# and joined into one picture, Slint on the left.
#
#   shots.sh SCALE      1 or 2: the sandbox must have been started with that GDK_SCALE
#
# $RESULTS/beside/<page>-<theme>-<scale>x.png, and the two halves of each
# under $RESULTS/beside/parts/. The builds: $HEYO_SLINT (one with
# --features slint-skia) and $HEYO_WEB, src-tauri/target/variants/heyo-skia
# and heyo-plain unless said. In the sandbox only.
. "$(dirname "$0")/lib.sh"
scale=$1
[ "$scale" = "${GDK_SCALE:-2}" ] || { echo "GDK_SCALE is ${GDK_SCALE:-2}, not $scale" >&2; exit 1; }
HEYO_SLINT=${HEYO_SLINT:-$W/src-tauri/target/variants/heyo-skia}
HEYO_WEB=${HEYO_WEB:-$W/src-tauri/target/variants/heyo-plain}
alive heyo && { echo "Heyo is running already" >&2; exit 1; }
mkdir -p "$RESULTS/beside/parts"
for theme in light dark; do
  for by in slint web; do
    python3 -I "$HERE/seed.py" "$XDG_CONFIG_HOME/heyo/settings.json" "appearance=$theme"
    if [ "$by" = slint ]; then HEYO=$HEYO_SLINT "$HERE/heyo.sh" start slint; else HEYO=$HEYO_WEB "$HERE/heyo.sh" start web; fi
    HEYO=$HEYO_WEB SETTLE=3 "$HERE/tour.sh" "beside/parts/$by-$theme-${scale}x" general shortcuts dictation screenshots advanced about list commands
    # The vocabulary, where the Chinese is: further down the Dictation page.
    "$HERE/do.sh" page dictation wait 2 move 560 400 wheel 14 wait 2 shot "beside/parts/$by-$theme-${scale}x-vocabulary"
    "$HERE/heyo.sh" stop
  done
  for page in general shortcuts dictation vocabulary screenshots advanced about list commands; do
    python3 -I "$HERE/beside.py" "$RESULTS/beside/parts/slint-$theme-${scale}x-$page.png" "$RESULTS/beside/parts/web-$theme-${scale}x-$page.png" "$RESULTS/beside/$page-$theme-${scale}x.png"
  done
done
ls "$RESULTS/beside"

#!/bin/sh
# One picture of every page of the settings window, in the light theme and
# the dark: $RESULTS/pages/<theme>-<page>.png, and the vocabulary (further
# down Dictation, where the Chinese is) as <theme>-vocabulary.png.
#
#   pages.sh
#
# In the sandbox, at the scale it was started with; Heyo must not be
# running in it.
. "$(dirname "$0")/lib.sh"
alive heyo && { echo "Heyo is running already" >&2; exit 1; }
for theme in light dark; do
  "$HERE/heyo.sh" seed "appearance=$theme"
  "$HERE/heyo.sh" start
  SETTLE=2 "$HERE/tour.sh" "pages/$theme" general shortcuts dictation screenshots advanced about list commands extension extension2 extension3
  "$HERE/do.sh" page dictation wait 2 move 560 400 wheel 14 wait 2 shot "pages/$theme-vocabulary" page shortcuts wait 1 click 400 156 wait 1.5 shot "pages/$theme-shortcuts-open"
  "$HERE/heyo.sh" stop
done
ls "$RESULTS/pages"

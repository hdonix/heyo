#!/bin/sh
# The language changed while Heyo runs: pictures of the root search, a
# command of Heyo's own (clipboard history, and its actions) and the
# settings in English, then of the same after 简体中文 is chosen in
# Settings → General, in the same run, then of the settings in English
# again. In $HEYO_TEST_DIR/shots/NAME-*.png.
#
#   language.sh NAME [xfwm|gnome]
. "$(dirname "$0")/env.sh"
NAME=$1
seed
"$HERE/sandbox.sh" up "${2:-xfwm}"
shot() { python3 "$HERE/grab.py" "$T/shots/$NAME-$1.png" 440 180 1480 960; }
palette() {
  for _ in 1 2 3 4 5 6; do
    [ -n "$(window '^Heyo$')" ] && break
    heyo; w 3
  done
}
# pictures LANGUAGE: the palette, the clipboard history and its actions,
# and the settings, which are left on screen.
pictures() {
  palette; w 1; shot "$1-root"
  heyo --clipboard; w 3; shot "$1-clipboard"
  xdotool key ctrl+k; w 1; shot "$1-actions"
  xdotool key Escape Escape Escape; w 0.5
  heyo --open heyo:settings; w 3; shot "$1-settings"
}
# choose WINDOW ROW LANGUAGE: in the settings on screen, which have that
# title, the menu of that row opened and a language picked from it.
choose() {
  ask click "combo box" "$2" "$1" > /dev/null && w 1 && ask click "list item" "$3" "$1" > /dev/null || { echo "language.sh: no $3 under $2 in $1" >&2; "$HERE/sandbox.sh" down > /dev/null; exit 1; }
  w 2
}
# Something for the clipboard history to list.
/usr/bin/python3 "$HERE/clip.py" "A line that was copied"; w 1.5

pictures english
choose "Heyo Settings" Language 简体中文
# The window that was on screen, as it is now.
shot chinese-settings-at-once
ask tree "Heyo 设置" | grep -E 'frame|combo box|push button|entry' | sed 's/ \[.*//'
pictures chinese
choose "Heyo 设置" 语言 English
shot english-again-settings
echo "language: $(stored language)"
"$HERE/sandbox.sh" down > /dev/null

#!/bin/sh
# What the sandbox says of itself: the colour scheme Heyo would read, the
# windows on screen, and the settings as they are stored.
. "$(dirname "$0")/lib.sh"
echo "colour scheme: $(gsettings get org.gnome.desktop.interface color-scheme 2>&1)"
echo "windows:"
for id in $(xdotool search --onlyvisible --name '.' 2>/dev/null); do
  echo "  $id $(xdotool getwindowname "$id" 2>/dev/null) [$(place "$id")] focus=$([ "$(xdotool getwindowfocus 2>/dev/null)" = "$id" ] && echo yes || echo no)"
done
echo "settings: $(cat "$XDG_CONFIG_HOME/heyo/settings.json" 2>/dev/null)"
echo "autostart: $(ls "$XDG_CONFIG_HOME/autostart" 2>/dev/null | tr '\n' ' ')"
if alive heyo; then
  heyo=$(cut -d' ' -f1 "$D/pids/heyo")
  echo "Heyo runs: $heyo, $(awk '/^VmRSS|^RssAnon|^RssFile/ {printf "%s %.1f MB  ", $1, $2 / 1024}' "/proc/$heyo/status")Pss: $(awk '/^Pss:/ {printf "%.1f MB", $2 / 1024}' "/proc/$heyo/smaps_rollup"); threads $(ls "/proc/$heyo/task" | wc -l); processor time so far $(sed 's/.*) //' "/proc/$heyo/stat" | awk -v hz="$(getconf CLK_TCK)" '{printf "%.2f s", ($12 + $13) / hz}')"
fi

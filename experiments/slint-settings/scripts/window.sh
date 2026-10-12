#!/bin/sh
# The settings window as a window: that it can be moved by its top edge,
# that Tab moves between its controls and Space works the one it is on,
# that Esc closes it, and that it comes back when asked for again.
#
#   window.sh NAME      what was seen goes to $RESULTS/window-NAME.txt
#
# Heyo must be running in the sandbox with its settings open.
. "$(dirname "$0")/lib.sh"
out=$RESULTS/window-$1.txt
where() { id=$(settings_window); [ -n "$id" ] && place "$id" || echo "none"; }
stored() { python3 -I -c 'import json, sys; print(json.load(open(sys.argv[1])).get(sys.argv[2]))' "$XDG_CONFIG_HOME/heyo/settings.json" "$1"; }
{
  echo "# $(date +%F), $(basename "$HEYO")"
  "$HERE/do.sh" click 150 72 wait 0.3 key ctrl+a key BackSpace wait 0.5 page general wait 1
  echo "at first: $(where)"
  "$HERE/do.sh" drag 500 55 600 155 wait 1
  echo "after its title bar was dragged by 100, 100: $(where)"
  "$HERE/do.sh" drag 100 44 0 -56 wait 1
  echo "after the sidebar's top edge was dragged by -100, -100: $(where)"
  echo "Open at Login before: $(ls "$XDG_CONFIG_HOME/autostart" 2>/dev/null | tr '\n' ' ')"
  # From the search field, Tab goes down the sidebar's entries (the six
  # pages, then the applications and Heyo's commands: a sandbox has no
  # other extension) and then to the first control of the page.
  "$HERE/do.sh" click 150 72 wait 0.3 key Tab key Tab key Tab wait 0.5 shot "window-$1-tab-sidebar" key Tab key Tab key Tab key Tab key Tab key Tab wait 0.5 shot "window-$1-tab"
  "$HERE/do.sh" key space wait 1.5
  echo "Open at Login after Tab and Space: $(ls "$XDG_CONFIG_HOME/autostart" 2>/dev/null | tr '\n' ' ')"
  "$HERE/do.sh" key space wait 1.5
  echo "and after Space again: $(ls "$XDG_CONFIG_HOME/autostart" 2>/dev/null | tr '\n' ' ')"
  xdotool key Escape; sleep 1
  echo "after Esc: $(where); Heyo $(alive heyo && echo runs || echo "DOES NOT RUN")"
  timeout 10 "$HEYO" --open heyo:settings; sleep 2
  echo "asked for again: $(where)"
  grep "slint settings" "$S/heyo.log" | cut -c1-120
} > "$out" 2>&1
cat "$out"

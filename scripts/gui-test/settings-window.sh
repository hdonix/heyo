#!/bin/sh
# The settings window as a window: that it can be moved by its top edge,
# that Tab moves between its controls and Space works the one it is on,
# that Esc closes it, and that it comes back when asked for again.
#
#   settings-window.sh [NAME]      what was seen goes to $RESULTS/window-NAME.txt
#
# In the session that runs, or one of its own (GNOME Shell); Heyo must not
# be running in it, and no extension is installed: Tab is counted.
. "$(dirname "$0")/env.sh"
name=${1:-x11}
session_for gnome
out=$RESULTS/window-$name.txt
where() { id=$(window '^Heyo Settings$'); [ -n "$id" ] && place "$id" || echo "none"; }
login() { ls "$XDG_CONFIG_HOME/autostart" 2>/dev/null | tr '\n' ' '; }
rm -rf "$XDG_DATA_HOME/heyo/extensions"
seed > /dev/null
"$GUI/sandbox.sh" heyo || exit 1
heyo --open heyo:settings > /dev/null 2>&1
wait_window '^Heyo Settings$' 100 > /dev/null || { echo "no settings window came up" >&2; stop heyo > /dev/null; leave; exit 1; }
sleep 2
{
  echo "# $(date +%F), $BUILD, scale $(scale)"
  "$GUI/settings-do.sh" press entry "Search settings" wait 0.3 key ctrl+a key BackSpace wait 0.5 page General wait 1
  echo "at first: $(where)"
  "$GUI/settings-do.sh" drag 500 55 600 155 wait 1
  echo "after its title bar was dragged by 100, 100: $(where)"
  "$GUI/settings-do.sh" drag 100 44 0 -56 wait 1
  echo "after the sidebar's top edge was dragged by -100, -100: $(where)"
  # The desktop had the pointer for those moves, and the window never
  # heard the button go up: a click in it must still be a click.
  "$GUI/settings-do.sh" page Shortcuts wait 0.6
  echo "a click on Shortcuts in the sidebar, after the moves: $(ask states "list item" Shortcuts "Heyo Settings")"
  "$GUI/settings-do.sh" page General wait 0.6
  echo "Open at Login before: $(login)"
  # From the search field, Tab goes down the sidebar's entries (the six
  # pages, then the applications and Heyo's commands) and then to the first
  # control of the page.
  "$GUI/settings-do.sh" press entry "Search settings" wait 0.3 key Tab key Tab key Tab wait 0.5 shot "window-$name-tab-sidebar" key Tab key Tab key Tab key Tab key Tab key Tab wait 0.5 shot "window-$name-tab"
  "$GUI/settings-do.sh" key space wait 1.5
  echo "Open at Login after Tab and Space: $(login)"
  "$GUI/settings-do.sh" key space wait 1.5
  echo "and after Space again: $(login)"
  xdotool key Escape; sleep 1
  echo "after Esc: $(where); Heyo $(alive heyo && echo runs || echo "DOES NOT RUN")"
  heyo --open heyo:settings > /dev/null 2>&1; sleep 2
  echo "asked for again: $(where)"
  grep -i "slint\|settings up" "$T/app.log" | cut -c1-120
} > "$out" 2>&1
cat "$out"
xdotool key Escape
stop heyo > /dev/null
leave

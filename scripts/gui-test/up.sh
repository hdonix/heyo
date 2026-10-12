#!/bin/sh
# Starts the sandbox: an X server, a private bus, a window manager, the dev
# server and a debug build of Heyo (hidden).
#
#   up.sh [xfwm|gnome]    gnome is a real compositor (shadows, transparency)
#
# Settings are $HEYO_TEST_DIR/home/config/heyo/settings.json; set them
# before starting. down.py stops everything again: each process is started
# through `start` (env.sh), which writes its number down for it.
. "$(dirname "$0")/env.sh"
python3 "$HERE/down.py" > /dev/null; w 1
start Xvfb "$DISPLAY" -screen 0 "${SCREEN:-1920x1080}x24" >/dev/null 2>&1; w 1
# Not left to put itself in the background: its number would not be known.
start dbus-daemon --session --address="$DBUS_SESSION_BUS_ADDRESS" --nofork > "$T/dbus.log" 2>&1
for _ in 1 2 3 4 5 6 7 8 9 10; do [ -S "$H/run/bus" ] && break; w 0.3; done
if [ "$HEYO_TEST_FRONTEND" = built ]; then
  # The frontend as a release has it (minified, React's production build),
  # with the measuring code left in.
  start sh -c 'cd "$1" && VITE_HEYO_BENCH=1 pnpm exec vite build --outDir "$2" --emptyOutDir && exec pnpm exec vite preview --outDir "$2" --port 1420 --strictPort' sh "$REPO" "$T/dist" > "$T/vite.log" 2>&1
  w 6
elif [ "$HEYO_TEST_FRONTEND" != none ]; then
  start sh -c 'cd "$1" && exec pnpm exec vite --port 1420 --strictPort' sh "$REPO" > "$T/vite.log" 2>&1
fi
if [ "${1:-xfwm}" = gnome ]; then
  start env XDG_SESSION_TYPE=x11 LIBGL_ALWAYS_SOFTWARE=1 gnome-shell --x11 > "$T/shell.log" 2>&1; w 9
  xdotool key Escape; w 1.5   # out of the overview it starts in
else
  start xfwm4 --compositor=off >/dev/null 2>&1; w 3
fi
start "$REPO/src-tauri/target/$BUILD/heyo" --hidden > "$T/app.log" 2>&1
# A debug build takes a while to read the applications and extensions; until
# it has, it cannot open anything by name.
w 20

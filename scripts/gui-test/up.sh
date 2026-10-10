#!/bin/sh
# Starts the sandbox: an X server, a private bus, a window manager, the dev
# server and a debug build of Heyo (hidden).
#
#   up.sh [xfwm|gnome]    gnome is a real compositor (shadows, transparency)
#
# Settings are $HEYO_TEST_DIR/home/config/heyo/settings.json; set them
# before starting. down.py stops everything again.
. "$(dirname "$0")/env.sh"
python3 "$HERE/down.py" > /dev/null; w 1
(Xvfb "$DISPLAY" -screen 0 "${SCREEN:-1920x1080}x24" >/dev/null 2>&1 &); w 1
dbus-daemon --session --address="$DBUS_SESSION_BUS_ADDRESS" --fork
if [ "$HEYO_TEST_FRONTEND" = built ]; then
  # The frontend as a release has it (minified, React's production build),
  # with the measuring code left in.
  (cd "$REPO" && VITE_HEYO_BENCH=1 pnpm exec vite build --outDir "$T/dist" --emptyOutDir > "$T/vite.log" 2>&1 \
    && pnpm exec vite preview --outDir "$T/dist" --port 1420 --strictPort >> "$T/vite.log" 2>&1 &)
  w 6
elif [ "$HEYO_TEST_FRONTEND" != none ]; then
  (cd "$REPO" && pnpm exec vite --port 1420 --strictPort > "$T/vite.log" 2>&1 &)
fi
if [ "${1:-xfwm}" = gnome ]; then
  (XDG_SESSION_TYPE=x11 LIBGL_ALWAYS_SOFTWARE=1 gnome-shell --x11 > "$T/shell.log" 2>&1 &); w 9
  xdotool key Escape; w 1.5   # out of the overview it starts in
else
  (xfwm4 --compositor=off >/dev/null 2>&1 &); w 3
fi
("$REPO/src-tauri/target/$BUILD/heyo" --hidden > "$T/app.log" 2>&1 &)
# A debug build takes a while to read the applications and extensions; until
# it has, it cannot open anything by name.
w 20

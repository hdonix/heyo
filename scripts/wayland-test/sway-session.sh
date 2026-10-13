#!/bin/sh
# Starts a D-Bus session bus and a headless Sway, then waits. Other
# commands join the session with `docker exec … in-session …`.
set -eu

if [ "$(id -u)" = 0 ]; then
  exec runuser -u tester -- "$0" "$@"
fi

export HOME=/home/tester
export XDG_RUNTIME_DIR=/tmp/runtime-tester
mkdir -p "$XDG_RUNTIME_DIR" && chmod 700 "$XDG_RUNTIME_DIR"
export XDG_SESSION_TYPE=wayland XDG_CURRENT_DESKTOP=sway
# No GPU in the container: software rendering, for Sway and for its clients.
export LIBGL_ALWAYS_SOFTWARE=1 GSK_RENDERER=cairo WEBKIT_DISABLE_COMPOSITING_MODE=1

if [ "${1:-}" = "in-session" ]; then
  shift
  . "$XDG_RUNTIME_DIR/session.env"
  exec "$@"
fi

find "$XDG_RUNTIME_DIR" -mindepth 1 -delete
eval "$(dbus-launch --sh-syntax)"
# One output of the size asked for, at the scale asked for; no bar, no
# borders to count in a picture, and the focus where the pointer goes
# (Sway's own default), which is how a check sees which window has the
# pointer.
size=${MONITORS:-1920x1080}
cat > "$XDG_RUNTIME_DIR/sway.conf" <<CONF
output HEADLESS-1 resolution ${size%% *} scale ${SCALE:-1}
default_border none
focus_follows_mouse yes
CONF
export WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 WLR_RENDERER=pixman
sway -c "$XDG_RUNTIME_DIR/sway.conf" > "$XDG_RUNTIME_DIR/sway.log" 2>&1 &
for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20; do
  socket=$(find "$XDG_RUNTIME_DIR" -maxdepth 1 -name 'sway-ipc.*.sock' | head -1)
  display=$(find "$XDG_RUNTIME_DIR" -maxdepth 1 -name 'wayland-*' ! -name '*.lock' | head -1)
  [ -n "$socket" ] && [ -n "$display" ] && break
  sleep 0.3
done
cat > "$XDG_RUNTIME_DIR/session.env" <<ENV
export DBUS_SESSION_BUS_ADDRESS='$DBUS_SESSION_BUS_ADDRESS'
export WAYLAND_DISPLAY=$(basename "${display:-wayland-1}") GDK_BACKEND=wayland
export SWAYSOCK='${socket:-}'
ENV
echo "session starting"
wait || true
sleep infinity

#!/bin/sh
# Starts a D-Bus session bus and a headless GNOME Shell on Wayland, then
# waits. Other commands join the session with `docker exec … in-session …`.
set -eu

# GNOME Shell expects a system bus to exist (it need not offer much). That
# takes root; everything else runs as the ordinary user.
if [ "$(id -u)" = 0 ]; then
  mkdir -p /run/dbus
  # Left behind when the container is restarted ("run.sh relogin").
  rm -f /run/dbus/pid
  dbus-daemon --system --fork
  # There is no systemd in the container; a mock login manager answers
  # for it (one seat, one session for the user).
  python3 -m dbusmock --system --template logind > /tmp/logind.log 2>&1 &
  for _ in 1 2 3 4 5 6 7 8 9 10; do
    gdbus introspect --system --dest org.freedesktop.login1 --object-path /org/freedesktop/login1 >/dev/null 2>&1 && break
    sleep 0.3
  done
  gdbus call --system --dest org.freedesktop.login1 --object-path /org/freedesktop/login1 \
    --method org.freedesktop.DBus.Mock.AddSeat seat0 >/dev/null 2>&1 || true
  gdbus call --system --dest org.freedesktop.login1 --object-path /org/freedesktop/login1 \
    --method org.freedesktop.DBus.Mock.AddSession c1 seat0 1001 tester true >/dev/null 2>&1 || true
  exec runuser -u tester -- "$0" "$@"
fi

export HOME=/home/tester
export XDG_RUNTIME_DIR=/tmp/runtime-tester
mkdir -p "$XDG_RUNTIME_DIR" && chmod 700 "$XDG_RUNTIME_DIR"
export XDG_SESSION_TYPE=wayland XDG_CURRENT_DESKTOP=ubuntu:GNOME XDG_SESSION_DESKTOP=ubuntu
export XDG_SESSION_ID=c1 XDG_SEAT=seat0
# No GPU in the container: software rendering.
export LIBGL_ALWAYS_SOFTWARE=1 GSK_RENDERER=cairo WEBKIT_DISABLE_COMPOSITING_MODE=1

if [ "${1:-}" = "in-session" ]; then
  shift
  . "$XDG_RUNTIME_DIR/session.env"
  exec "$@"
fi

# A new session: nothing of the last one's sockets.
find "$XDG_RUNTIME_DIR" -mindepth 1 -delete
eval "$(dbus-launch --sh-syntax)"
cat > "$XDG_RUNTIME_DIR/session.env" <<ENV
export DBUS_SESSION_BUS_ADDRESS='$DBUS_SESSION_BUS_ADDRESS'
export WAYLAND_DISPLAY=wayland-0 GDK_BACKEND=wayland
ENV
# The test harness extension (screenshots, window inspection); see its metadata.
gsettings set org.gnome.shell disable-user-extensions false
# The test pointer finds its position by being pushed into the top-left
# corner, which must not open the overview.
gsettings set org.gnome.desktop.interface enable-hot-corners false
if [ "${HEYO_EXTENSION:-yes}" = yes ]; then
    # Heyo's own extension, from the mounted repository, as a user would
    # have it installed.
    mkdir -p "$HOME/.local/share/gnome-shell/extensions"
    cp -r /heyo/gnome-extension/heyo@heyo.app "$HOME/.local/share/gnome-shell/extensions/"
    gsettings set org.gnome.shell enabled-extensions "['heyo-test-harness@heyo.local', 'heyo@heyo.app']"
elif [ "${HEYO_EXTENSION:-yes}" = old ] && [ ! -e "$HOME/.session-started" ]; then
    # The extension of an earlier Heyo ("run.sh start old" put it there), as
    # after Heyo alone was updated. Heyo then updates it, and "run.sh
    # relogin" starts the session with what Heyo wrote.
    mkdir -p "$HOME/.local/share/gnome-shell/extensions"
    cp -r /heyo/src-tauri/target/wayland-test/heyo@heyo.app "$HOME/.local/share/gnome-shell/extensions/"
    gsettings set org.gnome.shell enabled-extensions "['heyo-test-harness@heyo.local', 'heyo@heyo.app']"
elif [ ! -e "$HOME/.session-started" ]; then
    # Without it, to test installing it from Heyo; "run.sh relogin" then
    # starts the session again with whatever Heyo set up.
    gsettings set org.gnome.shell enabled-extensions "['heyo-test-harness@heyo.local']"
fi
touch "$HOME/.session-started"
# One virtual monitor for each size in MONITORS, side by side.
set --
for monitor in ${MONITORS:-1920x1080}; do
    set -- "$@" --virtual-monitor "$monitor"
done
gnome-shell --headless --wayland --no-x11 "$@" > "$XDG_RUNTIME_DIR/shell.log" 2>&1 &
echo "session starting"
# Stay up even if the shell dies, so its log can be read.
wait || true
sleep infinity

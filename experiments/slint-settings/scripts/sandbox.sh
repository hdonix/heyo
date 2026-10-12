#!/bin/sh
# A display of its own to try the settings window on: an X server in memory
# (Xvfb) at 4K, a session bus, a compositing window manager and home
# directories, none of them the desktop's.
#
#   sandbox.sh up        start it
#   sandbox.sh down      stop what `up` started
#   sandbox.sh env       print the environment to run something inside it
#   sandbox.sh run CMD…  run a command inside it
#
# Every process started is written down by its number when it is started
# ($SLINT_SETTINGS_DIR/pids/), and `down` ends those numbers, and what those
# had started themselves (the bus is told to start no services, so that is
# only what Heyo or GNOME Shell start): never a process found by its name.
#
#   SLINT_SETTINGS_DISPLAY   the display, :82 unless said
#   SLINT_SETTINGS_DIR       where it keeps its files, /tmp/heyo-slint
#   SCREEN                   the screen's size, 3840x2160
#   GDK_SCALE                pixels per logical pixel, 2
#   COMPOSITOR               on (xfwm4 compositing, the default), off (xfwm4
#                            not compositing) or gnome (GNOME Shell, drawing
#                            with Mesa's software OpenGL: the compositor Heyo
#                            is used under, and the slowest of the three here)
set -u
D=${SLINT_SETTINGS_DIR:-/tmp/heyo-slint}
X=${SLINT_SETTINGS_DISPLAY:-:82}
SOCKET=/tmp/.X11-unix/X${X#:}

environment() {
  echo "export DISPLAY=$X GDK_BACKEND=x11 GDK_SCALE=${GDK_SCALE:-2}"
  echo "export DBUS_SESSION_BUS_ADDRESS=unix:path=$D/home/run/bus XDG_RUNTIME_DIR=$D/home/run"
  echo "export XDG_CONFIG_HOME=$D/home/config XDG_CACHE_HOME=$D/home/cache XDG_DATA_HOME=$D/home/data"
  # Neither the desktop's settings service, nor its accessibility bus, nor
  # its file-system service.
  echo "export GSETTINGS_BACKEND=memory NO_AT_BRIDGE=1 GTK_A11Y=none GIO_USE_VFS=local"
  echo "unset WAYLAND_DISPLAY XAUTHORITY"
}

# When a process was started, as the kernel counts it: with its number,
# what tells it from a later process that was given the same number.
born() {
  sed 's/.*) //' "/proc/$1/stat" 2>/dev/null | cut -d' ' -f20
}

# Starts "$@" in the background and writes its number, and when it was
# started, as pids/$name.
start() {
  name=$1; shift
  "$@" > "$D/$name.log" 2>&1 &
  echo "$! $(born $!)" > "$D/pids/$name"
}

# Everything a process started, and what those started, each as
# number:when-started. Found by who started whom, never by a name.
family() {
  for child in $(ps -o pid= --ppid "$1" 2>/dev/null); do
    echo "$child:$(born "$child")"
    family "$child"
  done
}

# Ends the process with number $1 that was started at $2, and says so. One
# that ended by itself is left alone, whoever has its number now.
end() {
  if [ "$(born "$1")" != "$2" ] || [ -z "$2" ]; then echo "$3 $1 had ended already"; return; fi
  kill "$1" 2>/dev/null
  for _ in 1 2 3 4 5 6 7 8 9 10; do [ "$(born "$1")" = "$2" ] || break; sleep 0.3; done
  [ "$(born "$1")" = "$2" ] && kill -9 "$1" 2>/dev/null
  sleep 0.2
  if [ "$(born "$1")" = "$2" ]; then echo "$0: $1 ($3) is still there" >&2; else echo "stopped $3 $1"; fi
}

case "${1:-}" in
  up)
    [ -e "$SOCKET" ] && { echo "$0: display $X is in use" >&2; exit 1; }
    mkdir -p "$D/pids" "$D/home/config" "$D/home/cache" "$D/home/data" "$D/home/run"
    chmod 700 "$D/home/run"
    start xvfb Xvfb "$X" -screen 0 "${SCREEN:-3840x2160}x24" -nolisten tcp
    for _ in 1 2 3 4 5 6 7 8 9 10; do [ -e "$SOCKET" ] && break; sleep 0.3; done
    eval "$(environment)"
    # A bus that starts no service by itself: what it started would be a
    # process nobody here wrote down. (A socket's path may not be long:
    # keep SLINT_SETTINGS_DIR short.)
    cat > "$D/bus.conf" <<EOF
<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN" "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>$DBUS_SESSION_BUS_ADDRESS</listen>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>
EOF
    start dbus dbus-daemon --nofork --config-file="$D/bus.conf"
    for _ in 1 2 3 4 5 6 7 8 9 10; do [ -e "$D/home/run/bus" ] && break; sleep 0.3; done
    [ -e "$D/home/run/bus" ] || { echo "$0: the bus did not start: $(cat "$D/dbus.log")" >&2; exit 1; }
    if [ "${COMPOSITOR:-on}" = gnome ]; then
      # With a home of its own: its extensions read the home directory.
      HOME="$D/home" XDG_SESSION_TYPE=x11 LIBGL_ALWAYS_SOFTWARE=1 start shell gnome-shell --x11
      sleep 9
      # Out of the overview it starts in.
      xdotool key Escape
      sleep 1.5
    else
      # The window manager composites only with its settings service there,
      # so that a transparent window shows what is behind it.
      for xfconfd in /usr/lib/*/xfce4/xfconf/xfconfd /usr/lib/xfce4/xfconf/xfconfd; do
        [ -x "$xfconfd" ] && { start xfconfd "$xfconfd"; sleep 0.5; break; }
      done
      start xfwm4 xfwm4 --compositor="${COMPOSITOR:-on}" --vblank=off
      sleep 2
    fi
    echo "sandbox on $X: $(cut -d' ' -f1 "$D"/pids/* | tr '\n' ' ')"
    ;;
  down)
    # Some of them only, for a script that started those.
    files=
    for file in "$D"/pids/*; do
      [ -f "$file" ] || continue
      case " ${SLINT_SETTINGS_ONLY:-$(basename "$file")} " in *" $(basename "$file") "*) files="$files $file" ;; esac
    done
    # What each started itself, noted before any of them goes: a process
    # whose parent has ended belongs to nobody one could ask afterwards,
    # and one of these ending can end another.
    families=
    for file in $files; do
      read -r pid started < "$file"
      [ "$(born "$pid")" = "$started" ] && families="$families $(family "$pid")"
    done
    for file in $files; do
      read -r pid started < "$file"
      end "$pid" "$started" "$(basename "$file")"
      rm -f "$file"
    done
    for member in $families; do
      [ "$(born "${member%%:*}")" = "${member#*:}" ] && end "${member%%:*}" "${member#*:}" "started by one of them"
    done
    ;;
  env)
    environment
    ;;
  run)
    shift
    eval "$(environment)"
    exec "$@"
    ;;
  *)
    sed -n '2,19p' "$0" | sed 's/^# \{0,1\}//'
    exit 2
    ;;
esac

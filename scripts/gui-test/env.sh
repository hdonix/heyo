# The sandbox's environment: its own display, bus and home directories, so
# that nothing of the desktop Heyo is developed on is touched. Sourced by
# the other scripts here.
HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$HERE/../.." && pwd)
T=${HEYO_TEST_DIR:-${TMPDIR:-/tmp}/heyo-gui-test}
mkdir -p "$T/home/data" "$T/home/config/heyo" "$T/home/cache" "$T/home/run" "$T/shots"
chmod 700 "$T/home/run"
# The home directories are reached through a short link. Sockets are made
# under them, and the path of a Unix socket is at most 107 bytes: where
# $XDG_CACHE_HOME/ibus/dbus-XXXXXXXX does not fit, the ibus-daemon that
# GNOME Shell starts never finishes starting. It retries the bind without
# end, taking 2 GB of memory a minute, and nothing here would stop it.
if [ -z "$HEYO_TEST_HOME" ]; then
  HEYO_TEST_HOME=${XDG_RUNTIME_DIR:-/tmp}/heyo-gui-$(printf %s "$T" | cksum | cut -d' ' -f1)
  ln -sfn "$T/home" "$HEYO_TEST_HOME"
  export HEYO_TEST_HOME
fi
H=$HEYO_TEST_HOME
if [ "$(printf %s "$H/cache/ibus/dbus-XXXXXXXX" | wc -c)" -gt 107 ]; then
  echo "gui-test: $H is too long a path for the sockets under it" >&2
  exit 1
fi
# The display is :99 unless HEYO_TEST_DISPLAY names another (several
# sandboxes at once need one each, and a directory each: HEYO_TEST_DIR).
export HEYO_TEST_DISPLAY="${HEYO_TEST_DISPLAY:-:99}"
export DISPLAY="$HEYO_TEST_DISPLAY" XDG_DATA_HOME="$H/data" XDG_CONFIG_HOME="$H/config" XDG_CACHE_HOME="$H/cache"
export DBUS_SESSION_BUS_ADDRESS="unix:path=$H/run/bus" XDG_RUNTIME_DIR="$H/run"
# An input method's bus is found through the home directories above, unless
# the environment names one: which would be the one of the desktop in use.
unset IBUS_ADDRESS IBUS_ADDRESS_FILE
w() { python3 -c "import time; time.sleep($1)"; }
# The scripts this one starts are told the directory too (down.py reads it).
export HEYO_TEST_DIR="$T"
# start COMMAND…: runs it in the background. Every process the scripts here
# start in the sandbox is started this way: its number is written down in
# $T/pids, with when it was started (which tells it from a later process
# given the same number) and what it is, and down.py ends the processes
# written down there and what those started themselves. It ends nothing
# else: no process is ever looked for by its name or by its environment.
# STARTED is the number, for a script that ends one itself.
start() {
  "$@" &
  STARTED=$!
  # One that cannot be written down is not left running: nothing would end it.
  if ! echo "$STARTED $(sed 's/.*) //' "/proc/$STARTED/stat" 2>/dev/null | cut -d' ' -f20) $(basename "$1")" >> "$T/pids"; then
    kill "$STARTED" 2>/dev/null
    echo "gui-test: $T/pids cannot be written, so $1 was not started" >&2
    exit 1
  fi
}
# The build under test: debug (which loads the frontend from the dev server),
# or with HEYO_TEST_BUILD another directory under src-tauri/target whose
# build carries its own: release, or one made for the purpose.
BUILD=${HEYO_TEST_BUILD:-debug}
[ "$BUILD" != debug ] && HEYO_TEST_FRONTEND=none
heyo() { timeout 10 "$REPO/src-tauri/target/$BUILD/heyo" "$@"; }

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
w() { python3 -c "import time; time.sleep($1)"; }
# The build under test: debug (which loads the frontend from the dev server),
# or release (which carries its own) with HEYO_TEST_BUILD=release.
BUILD=${HEYO_TEST_BUILD:-debug}
[ "$BUILD" = release ] && HEYO_TEST_FRONTEND=none
heyo() { timeout 10 "$REPO/src-tauri/target/$BUILD/heyo" "$@"; }

#!/bin/sh
# A disposable GNOME-on-Wayland session in Docker for testing Heyo.
#
#   scripts/wayland-test/run.sh build        build the image
#   scripts/wayland-test/run.sh start        start the session (container heyo-wayland)
#   scripts/wayland-test/run.sh start bare   … without Heyo's Shell extension installed
#   scripts/wayland-test/run.sh start old    … with the extension of an earlier Heyo (version 1)
#   MONITORS="1920x1080 1280x720" scripts/wayland-test/run.sh start    … with two monitors
#   scripts/wayland-test/run.sh relogin      end the session and start it again, keeping the home directory
#   scripts/wayland-test/run.sh exec CMD…    run a command inside the session
#   scripts/wayland-test/run.sh stop
#
# Monitors' scales are set once the session runs: in/monitors.py.
#
# The repository is mounted read-only at /heyo, so the binary built on the
# host (src-tauri/target/…/heyo) and runtime/dist can be run as they are.
set -eu
HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$HERE/../.." && pwd)
# The container: another name lets two checkouts each have a session.
NAME=${HEYO_WAYLAND_NAME:-heyo-wayland}
IMAGE=heyo-wayland-test
# The last commit whose extension answers version 1.
FIRST_EXTENSION=149baf9

case "${1:-}" in
  build) docker build -t "$IMAGE" "$HERE" ;;
  start)
    docker rm -f "$NAME" >/dev/null 2>&1 || true
    case "${2:-}" in
      bare) EXTENSION=no ;;
      old)
        # The extension as it was before it had versions to compare.
        EXTENSION=old
        OLD="$REPO/src-tauri/target/wayland-test/heyo@heyo.app"
        mkdir -p "$OLD"
        for file in extension.js metadata.json; do
          git -C "$REPO" show "$FIRST_EXTENSION:gnome-extension/heyo@heyo.app/$file" > "$OLD/$file"
        done
        ;;
      *) EXTENSION=yes ;;
    esac
    docker run -d --name "$NAME" --shm-size=1g -e HEYO_EXTENSION=$EXTENSION -e MONITORS="${MONITORS:-1920x1080}" -v "$REPO:/heyo:ro" "$IMAGE" >/dev/null
    echo "started $NAME"
    ;;
  relogin) docker restart -t 2 "$NAME" >/dev/null && echo "restarted $NAME" ;;
  exec) shift; docker exec -u tester "$NAME" /usr/local/bin/session.sh in-session "$@" ;;
  stop) docker rm -f "$NAME" >/dev/null 2>&1 && echo stopped || echo "not running" ;;
  *) echo "usage: $0 build|start [bare|old]|relogin|exec CMD…|stop" >&2; exit 2 ;;
esac

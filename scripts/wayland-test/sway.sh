#!/bin/sh
# A disposable Sway session in Docker for testing Heyo where a Wayland
# compositor offers the layer shell (GNOME's does not: run.sh is that one).
#
#   scripts/wayland-test/sway.sh build       build the image (on the GNOME one: run.sh build first)
#   scripts/wayland-test/sway.sh start       start the session (container heyo-sway)
#   SCALE=2 scripts/wayland-test/sway.sh start    … with its output at that scale
#   scripts/wayland-test/sway.sh exec CMD…   run a command inside the session
#   scripts/wayland-test/sway.sh stop
#   scripts/wayland-test/sway.sh test        the checks of in/check-sway.py, at scale 1 and at 2
#
# The repository is mounted read-only at /heyo, as run.sh has it.
set -eu
HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$HERE/../.." && pwd)
# The container: another name lets two checkouts each have a session.
NAME=${HEYO_SWAY_NAME:-heyo-sway}
IMAGE=heyo-sway-test
IN=/heyo/scripts/wayland-test/in

case "${1:-}" in
  build) docker build -t "$IMAGE" -f "$HERE/Dockerfile.sway" "$HERE" ;;
  start)
    docker rm -f "$NAME" >/dev/null 2>&1 || true
    docker run -d --name "$NAME" --shm-size=1g -e MONITORS="${MONITORS:-1920x1080}" -e SCALE="${SCALE:-1}" -v "$REPO:/heyo:ro" "$IMAGE" >/dev/null
    echo "started $NAME"
    ;;
  exec) shift; docker exec -u tester "$NAME" /usr/local/bin/sway-session.sh in-session "$@" ;;
  stop) docker rm -f "$NAME" >/dev/null 2>&1 && echo stopped || echo "not running" ;;
  test)
    failed=0
    for scale in 1 2; do
      echo "== Sway, its output at scale $scale =="
      SCALE=$scale "$0" start >/dev/null
      for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20; do
        "$0" exec swaymsg -t get_version >/dev/null 2>&1 && break
        sleep 0.5
      done
      "$0" exec "$IN/start-heyo.sh" >/dev/null
      sleep 5
      "$0" exec "$IN/check-sway.py" 2>&1 || true
      "$0" exec test -e /tmp/check-passed || failed=1
    done
    "$0" stop >/dev/null
    [ $failed = 0 ] && echo "all passed" || { echo "SOME FAILED"; exit 1; }
    ;;
  *) echo "usage: $0 build|start|exec CMD…|stop|test" >&2; exit 2 ;;
esac

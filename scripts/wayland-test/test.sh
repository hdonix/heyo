#!/bin/sh
# Runs the checks of in/check.py in the test session, once for each way
# monitors can be laid out. Needs the image (run.sh build) and a release
# build of Heyo in this checkout (pnpm tauri build --no-bundle).
#
#   scripts/wayland-test/test.sh             everything
#   scripts/wayland-test/test.sh "2 1"       only that layout (arguments of in/monitors.py)
#
# Each layout gets a session of its own, with two monitors.
set -u
HERE=$(cd "$(dirname "$0")" && pwd)
IN=/heyo/scripts/wayland-test/in
failed=0

session() {
  MONITORS="1920x1080 1920x1200" "$HERE/run.sh" start >/dev/null
  # The Shell answers once it is up.
  for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20; do
    "$HERE/run.sh" exec "$IN/layout.sh" >/dev/null 2>&1 && return 0
    sleep 1
  done
  echo "the session did not start"; exit 1
}

run() {
  layout=$1; shift
  echo "== monitors: $layout =="
  session
  # shellcheck disable=SC2086
  "$HERE/run.sh" exec "$IN/monitors.py" $layout || { failed=1; return; }
  sleep 1
  "$HERE/run.sh" exec "$IN/start-heyo.sh" >/dev/null
  sleep 4
  "$HERE/run.sh" exec "$IN/check.py" "$@" 2>&1 | grep -v "Glycin" || true
  # check.py's own verdict is lost in the pipe: ask the last line.
  "$HERE/run.sh" exec test -e /tmp/check-passed || failed=1
}

if [ $# -gt 0 ]; then
  run "$1" layout capture
else
  # Which windows are listed, and the extension coming back after the lock screen.
  run "1 1" lists own relock layout capture
  # Scaled units: one monitor at 200 % beside one at 100 %, then fractional scales.
  run "2 1" own layout capture
  run "1.5 1.25" own layout capture
  # Laid out in pixels, as GNOME does until fractional scaling is switched on.
  run "--pixels 2 1" own layout capture
fi
"$HERE/run.sh" stop >/dev/null
[ $failed = 0 ] && echo "all passed" || { echo "SOME FAILED"; exit 1; }

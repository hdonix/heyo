#!/bin/sh
# Runs the checks of in/check.py in the test session, once for each way
# monitors can be laid out. Needs the image (run.sh build) and a release
# build of Heyo in this checkout (pnpm tauri build --no-bundle).
#
#   scripts/wayland-test/test.sh             everything
#   scripts/wayland-test/test.sh "2 1"       only that layout (arguments of in/monitors.py)
#   scripts/wayland-test/test.sh upgrade     only the extension's update
#
# Each layout gets a session of its own, with two monitors; and a last
# session starts with the extension of an earlier Heyo, which Heyo updates.
#
# Dictation is checked with the speech engine installed for whoever runs
# this (HEYO_TEST_SPEECH, else ~/.local/share/heyo/dictation; read only,
# empty for none) and a recording made with ../gui-test/say.py, which
# needs Flite. Pictures worth looking at are copied out to
# src-tauri/target/wayland-test/shots/, a directory for each session.
set -u
HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$HERE/../.." && pwd)
IN=/heyo/scripts/wayland-test/in
NAME=${HEYO_WAYLAND_NAME:-heyo-wayland}
OUT=$REPO/src-tauri/target/wayland-test
# The extension of the last commit at version 3, the one before this.
OLDER_EXTENSION=4e5a219
failed=0

: "${HEYO_TEST_SPEECH=$HOME/.local/share/heyo/dictation}"
export HEYO_TEST_SPEECH
mkdir -p "$OUT/shots"
python3 "$REPO/scripts/gui-test/say.py" "$OUT/spoken.wav" \
  "The weather is nice today, so let us walk to the park." \
  "See you at three thirty." > /dev/null || echo "no recording to dictate: Flite is not installed"

session() {
  "$HERE/run.sh" start "$@" >/dev/null
  # The Shell answers once it is up.
  for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20; do
    "$HERE/run.sh" exec "$IN/layout.sh" >/dev/null 2>&1 && return 0
    sleep 1
  done
  echo "the session did not start"; exit 1
}

# check SHOTS CHECK…: in/check.py, its verdict kept, its pictures copied out.
check() {
  shots=$1; shift
  # (What the accessibility library says of AccessKit's windows is noise.)
  "$HERE/run.sh" exec "$IN/check.py" "$@" 2>&1 | grep -v "Glycin\|dbind\|^$" || true
  # check.py's own verdict is lost in the pipe: ask the last line.
  "$HERE/run.sh" exec test -e /tmp/check-passed || failed=1
  rm -rf "${OUT:?}/shots/$shots"
  docker cp "$NAME:/tmp/shots" "$OUT/shots/$shots" >/dev/null 2>&1 || true
}

run() {
  layout=$1; shift
  echo "== monitors: $layout =="
  MONITORS="1920x1080 1920x1200" session
  # shellcheck disable=SC2086
  "$HERE/run.sh" exec "$IN/monitors.py" $layout || { failed=1; return; }
  sleep 1
  "$HERE/run.sh" exec "$IN/start-heyo.sh" >/dev/null
  sleep 4
  check "$(echo "$layout" | tr -d ' -')" "$@"
}

upgrade() {
  echo "== the extension of an earlier Heyo (version 3), updated =="
  HEYO_OLD_EXTENSION=$OLDER_EXTENSION session old
  "$HERE/run.sh" exec "$IN/start-heyo.sh" >/dev/null
  sleep 4
  check upgrade upgrade
  "$HERE/run.sh" relogin >/dev/null
  session_up=0
  for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20; do
    "$HERE/run.sh" exec "$IN/layout.sh" >/dev/null 2>&1 && { session_up=1; break; }
    sleep 1
  done
  [ $session_up = 1 ] || { echo "the session did not start again"; failed=1; return; }
  "$HERE/run.sh" exec "$IN/start-heyo.sh" >/dev/null
  sleep 4
  check upgraded upgraded own
}

if [ "${1:-}" = upgrade ]; then
  upgrade
elif [ $# -gt 0 ]; then
  run "$1" layout capture
else
  # Which windows are listed, and the extension coming back after the
  # lock screen; then the interface in Chinese, AI Chat and dictation
  # through the host, where windows come up, the settings window as a
  # window and an input method in it.
  run "1 1" cold lists own relock layout capture language ai dictate placement settings sizes ime
  # Scaled units: one monitor at 200 % beside one at 100 %, then fractional scales.
  # (After the first, Slint's first picture of a session is drawn before
  # the checks: see `cold`.)
  run "2 1" warm own layout capture language placement settings sizes
  run "1.5 1.25" warm own layout capture placement settings sizes
  # Laid out in pixels, as GNOME does until fractional scaling is switched on.
  run "--pixels 2 1" warm own layout capture placement settings sizes
  upgrade
fi
"$HERE/run.sh" stop >/dev/null
[ $failed = 0 ] && echo "all passed" || { echo "SOME FAILED"; exit 1; }

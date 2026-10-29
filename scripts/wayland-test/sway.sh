#!/bin/sh
# A disposable Sway session in Docker for testing Heyo where a Wayland
# compositor offers the layer shell (GNOME's does not: run.sh is that one).
#
#   scripts/wayland-test/sway.sh build       build the image (on the GNOME one: run.sh build first)
#   scripts/wayland-test/sway.sh start       start the session (container heyo-sway)
#   SCALE=2 scripts/wayland-test/sway.sh start    … with its output at that scale
#   scripts/wayland-test/sway.sh exec CMD…   run a command inside the session
#   scripts/wayland-test/sway.sh stop
#   scripts/wayland-test/sway.sh test        the checks of in/check-sway.py and in/check-sway-more.py,
#                                            at scale 1 and at 2, and the trials of `safe` and `lost`
#   scripts/wayland-test/sway.sh more [PART…]
#                                            only in/check-sway-more.py (at SCALE)
#   scripts/wayland-test/sway.sh safe        that the screenshot overlay cannot trap whoever uses
#                                            it (in/safe-sway.py): Escape, a panic, a stall and
#                                            Heyo killed, each in the middle of a drag
#   scripts/wayland-test/sway.sh lost        that nothing of Heyo's is left on the screen when
#                                            what draws it stops answering (in/lost-sway.py)
#   SCALE=2 MONITORS=3840x2160 scripts/wayland-test/sway.sh drag
#                                            how soon the overlay follows a pointer that jumps
#                                            (in/drag.py)
#
# HEYO_BUILD picks the build that is tried: release (the default) or debug.
#
# The repository is mounted read-only at /heyo, and the speech engine at
# /speech, as run.sh has them. `test` and `more` lend the speech engine
# installed for whoever runs this (HEYO_TEST_SPEECH, else
# ~/.local/share/heyo/dictation; empty for none) and make a recording
# with ../gui-test/say.py, which needs Flite. Pictures worth looking at
# are copied out to src-tauri/target/wayland-test/shots/.
set -eu
HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$HERE/../.." && pwd)
# The container: another name lets two checkouts each have a session.
NAME=${HEYO_SWAY_NAME:-heyo-sway}
IMAGE=heyo-sway-test
IN=/heyo/scripts/wayland-test/in
BUILD=${HEYO_BUILD:-release}
SPEECH=${HEYO_TEST_SPEECH:-}
LEND=
[ -n "$SPEECH" ] && [ -d "$SPEECH/models" ] && LEND="-v $SPEECH:/speech:ro"

case "${1:-}" in
  build) docker build -t "$IMAGE" -f "$HERE/Dockerfile.sway" "$HERE" ;;
  start)
    docker rm -f "$NAME" >/dev/null 2>&1 || true
    docker run -d --name "$NAME" --shm-size=1g -e MONITORS="${MONITORS:-1920x1080}" -e SCALE="${SCALE:-1}" -v "$REPO:/heyo:ro" $LEND "$IMAGE" >/dev/null
    echo "started $NAME"
    ;;
  exec) shift; docker exec -u tester "$NAME" /usr/local/bin/sway-session.sh in-session "$@" ;;
  stop) docker rm -f "$NAME" >/dev/null 2>&1 && echo stopped || echo "not running" ;;
  session)
    # A session that answers, at the scale and size asked for.
    "$0" start >/dev/null
    for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20; do
      "$0" exec swaymsg -t get_version >/dev/null 2>&1 && break
      sleep 0.5
    done
    ;;
  safe)
    failed=0
    "$0" session
    for trial in escape panic stall kill; do
      case $trial in
        panic) lever=HEYO_SLINT_PANIC_IN_DRAG=1 ;;
        stall) lever=HEYO_SLINT_STALL_IN_DRAG=1 ;;
        *) lever=HEYO_SLINT_NO_LEVER=1 ;;
      esac
      # start-heyo.sh ends the Heyo of the trial before, by the process it wrote down.
      "$0" exec env HEYO_BUILD="$BUILD" "$lever" "$IN/start-heyo.sh" >/dev/null
      sleep 5
      "$0" exec env HEYO_BUILD="$BUILD" "$IN/safe-sway.py" "$trial" 2>&1 || failed=1
    done
    [ -n "${HEYO_SWAY_KEEP:-}" ] || "$0" stop >/dev/null
    [ $failed = 0 ] && echo "all passed" || { echo "SOME FAILED"; exit 1; }
    ;;
  lost)
    "$0" session
    # Two pictures pinned from the start, and what draws them stopped
    # for a minute twenty seconds on.
    "$0" exec env HEYO_BUILD="$BUILD" HEYO_TEST_PIN="200,160,300,200;800,160,200,120" HEYO_SLINT_STALL_AFTER=20000 "$IN/start-heyo.sh" >/dev/null
    sleep 5
    failed=0
    "$0" exec env HEYO_BUILD="$BUILD" "$IN/lost-sway.py" 2>&1 || failed=1
    [ -n "${HEYO_SWAY_KEEP:-}" ] || "$0" stop >/dev/null
    [ $failed = 0 ] && echo "all passed" || { echo "SOME FAILED"; exit 1; }
    ;;
  more)
    # The checks of in/check-sway-more.py, in a session of their own.
    shift
    : "${HEYO_TEST_SPEECH=$HOME/.local/share/heyo/dictation}"
    export HEYO_TEST_SPEECH
    OUT=$REPO/src-tauri/target/wayland-test
    mkdir -p "$OUT/shots"
    python3 "$REPO/scripts/gui-test/say.py" "$OUT/spoken.wav" \
      "The weather is nice today, so let us walk to the park." \
      "See you at three thirty." > /dev/null || echo "no recording to dictate: Flite is not installed"
    "$0" session
    "$0" exec env HEYO_BUILD="$BUILD" "$IN/start-heyo.sh" >/dev/null
    sleep 5
    # (What the accessibility library says of AccessKit's windows is noise.)
    "$0" exec env HEYO_BUILD="$BUILD" "$IN/check-sway-more.py" "$@" 2>&1 | grep -v "Glycin\|dbind\|^[[:space:]]*$" || true
    failed=0
    "$0" exec test -e /tmp/check-passed || failed=1
    rm -rf "$OUT/shots/sway-scale-${SCALE:-1}"
    docker cp "$NAME:/tmp/shots" "$OUT/shots/sway-scale-${SCALE:-1}" >/dev/null 2>&1 || true
    [ -n "${HEYO_SWAY_KEEP:-}" ] || "$0" stop >/dev/null
    [ $failed = 0 ] && echo "all passed" || { echo "SOME FAILED"; exit 1; }
    ;;
  drag)
    "$0" session
    "$0" exec env HEYO_BUILD="$BUILD" HEYO_OVERLAY_TRACE=1 "$IN/start-heyo.sh" >/dev/null
    sleep 5
    "$0" exec env HEYO_BUILD="$BUILD" "$IN/drag.py"
    "$0" stop >/dev/null
    ;;
  test)
    failed=0
    for scale in 1 2; do
      echo "== Sway, its output at scale $scale =="
      SCALE=$scale "$0" session
      # With two pictures pinned from the start.
      "$0" exec env HEYO_BUILD="$BUILD" HEYO_TEST_PIN="200,160,300,200;800,160,200,120" "$IN/start-heyo.sh" >/dev/null
      sleep 5
      "$0" exec env HEYO_BUILD="$BUILD" "$IN/check-sway.py" 2>&1 || true
      "$0" exec test -e /tmp/check-passed || failed=1
      echo "== Sway at scale $scale: the interface in Chinese, AI and dictation through the host, the settings window, two outputs =="
      # (At 2 not the input method: where the search field says it is, is
      # not to be trusted there; see check-sway-more.py's `per`.)
      if [ "$scale" = 1 ]; then SCALE=1 "$0" more || failed=1; else SCALE=$scale "$0" more language ai dictate settings outputs || failed=1; fi
    done
    echo "== Sway, the overlay's safety =="
    SCALE=1 "$0" safe || failed=1
    echo "== Sway, what is on the screen when what draws it stops =="
    SCALE=1 "$0" lost || failed=1
    [ $failed = 0 ] && echo "all passed" || { echo "SOME FAILED"; exit 1; }
    ;;
  *) echo "usage: $0 build|start|exec CMD…|stop|test|more [PART…]|safe|lost|drag" >&2; exit 2 ;;
esac

#!/bin/sh
# The sandbox: a desktop of its own to try Heyo's windows on. An X server
# in memory (Xvfb), a session bus, a window manager, a session that
# publishes its scale as a real one does, and home directories: none of
# them the desktop's.
#
#   sandbox.sh up [xfwm|gnome]       the session and Heyo, hidden
#   sandbox.sh session [xfwm|gnome]  the session only
#   sandbox.sh heyo [VAR=value…]     Heyo, hidden, in the session that runs; its log is app.log
#   sandbox.sh stop NAME…            ends what was started under those names (heyo, …)
#   sandbox.sh down                  ends everything the sandbox's scripts started
#   sandbox.sh scale N               the session's scale becomes N, under what runs
#   sandbox.sh screen WIDTHxHEIGHT   the screen's size becomes that, under what runs
#   sandbox.sh monitors N            the screen is N monitors side by side (1: one again)
#   sandbox.sh check                 what the session publishes of its display
#   sandbox.sh ask …                 where a control of a Slint window is, from the window (ask.py)
#   sandbox.sh ibus [ENGINE]         an input method of the sandbox's own (under xfwm), or its engine
#   sandbox.sh look                  the windows on screen, who has the keyboard, Heyo's settings
#   sandbox.sh census                every process in the sandbox, written down or not
#   sandbox.sh run COMMAND…          a command in the sandbox's environment
#
# xfwm is xfwm4 (compositing with HEYO_TEST_COMPOSITING=1); gnome is GNOME
# Shell, the compositor Heyo is used under, drawing with Mesa's software
# OpenGL. Heyo's settings are $HEYO_TEST_DIR/home/config/heyo/settings.json;
# write them before starting it.
#
#   HEYO_TEST_DIR       where it keeps its files       HEYO_TEST_DISPLAY  its display, :99
#   HEYO_TEST_BUILD     debug, release, or a path      SCREEN             1920x1080
#   HEYO_TEST_SCALE     the scale it starts at, 1      HEYO_TEST_WARMUP   seconds Heyo is given
#   HEYO_TEST_SESSION   xfwm or gnome, when not said   (README.md has them all)
#
# The scale is said as a desktop says it and nowhere else. Under GNOME
# Shell the session's own settings (a dconf of the sandbox's) and the
# Shell's monitor layout have it, and GNOME's settings daemon publishes it:
# XSETTINGS, and Xft.dpi and Xcursor.size in the resource database. Under
# xfwm4 Xfce's settings daemon does the same from the sandbox's own xfconf.
# Nothing of it is in the environment of what runs there, so a program
# finds the scale as it would on a real desktop, and `scale N` changes it
# under a running program as connecting another monitor does.
#
# Every process is started through `start` (env.sh), which writes its
# number down; `down` ends those and what they started (ledger.py), and
# never a process found by its name.
. "$(dirname "$0")/env.sh"
SOCKET=/tmp/.X11-unix/X${DISPLAY#:}

# Whether the session that runs is this sandbox's: its X server is written down.
ours() { alive Xvfb || { echo "sandbox.sh: no sandbox runs on $DISPLAY ($T)" >&2; exit 1; }; }
published() { xrdb -query 2>/dev/null | grep -E "^(Xft.dpi|Xcursor.size)" | tr -s '\t\n' '  '; }
# Waits until the resource database has the scale given.
until_published() {
  for _ in $(seq 100); do xrdb -query 2>/dev/null | grep -q "^Xft.dpi:[[:space:]]*$((96 * $1))$" && return 0; sleep 0.1; done
  echo "sandbox.sh: the session does not publish scale $1: $(published)" >&2
  return 1
}

session() {
  python3 "$GUI/ledger.py" end > /dev/null; w 1
  [ -e "$SOCKET" ] && { echo "sandbox.sh: display $DISPLAY is in use, and not by this sandbox" >&2; exit 1; }
  wanted=${HEYO_TEST_SCALE:-1}
  start Xvfb "$DISPLAY" -screen 0 "${SCREEN:-1920x1080}x24" -nolisten tcp > "$T/xvfb.log" 2>&1
  for _ in $(seq 30); do [ -e "$SOCKET" ] && break; w 0.2; done
  alive Xvfb || { echo "sandbox.sh: the X server did not start: $(cat "$T/xvfb.log")" >&2; exit 1; }
  # Not left to put itself in the background: its number would not be known.
  # What it starts by itself when asked for a service (the settings, the
  # accessibility bus) it starts as its own, and so ends with it.
  bus=dbus-daemon; [ -x /usr/bin/dbus-daemon ] && bus=/usr/bin/dbus-daemon
  start_as bus "$bus" --session --address="$DBUS_SESSION_BUS_ADDRESS" --nofork --nopidfile > "$T/dbus.log" 2>&1
  for _ in $(seq 30); do [ -S "$H/run/bus" ] && break; w 0.2; done
  [ -S "$H/run/bus" ] || { echo "sandbox.sh: the bus did not start: $(cat "$T/dbus.log")" >&2; exit 1; }
  if [ "$1" = gnome ]; then
    # The session's settings, kept as a desktop keeps them, and the scale
    # among them: the Shell and the settings daemon read it from there.
    gsettings set org.gnome.desktop.interface scaling-factor "$wanted"
    start_as shell env XDG_SESSION_TYPE=x11 XDG_CURRENT_DESKTOP=ubuntu:GNOME LIBGL_ALWAYS_SOFTWARE=1 gnome-shell --x11 > "$T/shell.log" 2>&1
    for _ in $(seq 150); do gdbus call --session --dest org.gnome.Mutter.DisplayConfig --object-path /org/gnome/Mutter/DisplayConfig --method org.freedesktop.DBus.Peer.Ping > /dev/null 2>&1 && break; w 0.2; done
    w "${HEYO_TEST_SHELL_WARMUP:-6}"
    start_as xsettings /usr/libexec/gsd-xsettings > "$T/xsettings.log" 2>&1
    until_published "$wanted" || exit 1
    xdotool key Escape; w 1.5   # out of the overview it starts in
  else
    # Xfce's settings service and settings daemon: the window manager
    # composites only with the first, and the second publishes the scale.
    for xfconfd in /usr/lib/*/xfce4/xfconf/xfconfd /usr/lib/xfce4/xfconf/xfconfd; do
      [ -x "$xfconfd" ] && { start "$xfconfd" > "$T/xfconfd.log" 2>&1; w 0.5; break; }
    done
    if command -v xfsettingsd > /dev/null && command -v xfconf-query > /dev/null; then
      /usr/bin/python3 "$GUI/session.py" xfce-scale "$wanted" quiet
      # For what it publishes, and not for the clipboard manager it brings:
      # that is Xfce's own, and no part of the desktop this stands for.
      start_as xsettings env XFSETTINGSD_NO_CLIPBOARD=1 xfsettingsd --disable-wm-check > "$T/xsettings.log" 2>&1
      until_published "$wanted" || exit 1
    else
      # No settings daemon: the resource database alone says it, which is
      # where a program looks when nothing publishes XSETTINGS.
      printf 'Xft.dpi: %s\nXcursor.size: %s\n' $((96 * wanted)) $((24 * wanted)) | xrdb -merge
    fi
    start xfwm4 --compositor="$([ -n "$HEYO_TEST_COMPOSITING" ] && echo on || echo off)" --vblank=off > "$T/xfwm.log" 2>&1; w 3
  fi
  echo "sandbox on $DISPLAY, $(xdpyinfo | awk '/dimensions/{print $2}'), ${1}: $(published)"
}

heyo_up() {
  ours
  alive heyo && { echo "sandbox.sh: Heyo is running already" >&2; exit 1; }
  for assignment in "$@"; do export "$assignment"; done
  if [ "$HEYO_TEST_FRONTEND" = built ] && ! alive vite; then
    # The frontend as a release has it (minified, React's production build),
    # with the measuring code left in.
    start_as vite sh -c 'cd "$1" && VITE_HEYO_BENCH=1 pnpm exec vite build --outDir "$2" --emptyOutDir && exec pnpm exec vite preview --outDir "$2" --port 1420 --strictPort' sh "$REPO" "$T/dist" > "$T/vite.log" 2>&1
    w 6
  elif [ "$HEYO_TEST_FRONTEND" != none ] && ! alive vite; then
    start_as vite sh -c 'cd "$1" && exec pnpm exec vite --port 1420 --strictPort' sh "$REPO" > "$T/vite.log" 2>&1
  fi
  [ -x "$HEYO" ] || { echo "sandbox.sh: no build at $HEYO" >&2; exit 1; }
  # PIN names the processors it may use (taskset's list), when it is to be
  # measured on fewer.
  start_as heyo ${PIN:+taskset -c "$PIN"} "$HEYO" --hidden > "$T/app.log" 2>&1
  # It reads the applications and extensions before it can open anything by
  # name; a debug build takes a while over it.
  for _ in $(seq 600); do grep -q "heyo: started" "$T/app.log" 2>/dev/null && break; alive heyo || break; w 0.1; done
  alive heyo || { echo "sandbox.sh: Heyo did not stay up: $(tail -3 "$T/app.log")" >&2; exit 1; }
  w "${HEYO_TEST_WARMUP:-$([ "$BUILD" = debug ] && echo 15 || echo 6)}"
}

case "${1:-}" in
  up) session "${2:-${HEYO_TEST_SESSION:-xfwm}}"; heyo_up ;;
  session) session "${2:-${HEYO_TEST_SESSION:-xfwm}}" ;;
  heyo) shift; heyo_up "$@" ;;
  stop) shift; [ $# -gt 0 ] || { echo "sandbox.sh stop NAME…" >&2; exit 2; }; stop "$@" ;;
  down) python3 "$GUI/ledger.py" end ;;
  scale)
    ours
    if alive shell; then /usr/bin/python3 "$GUI/session.py" scale "$2" || exit 1
    elif alive xsettings; then /usr/bin/python3 "$GUI/session.py" xfce-scale "$2" || exit 1
    else printf 'Xft.dpi: %s\nXcursor.size: %s\n' $((96 * $2)) $((24 * $2)) | xrdb -merge; fi
    until_published "$2" || exit 1
    w 1
    echo "the session says: $(published)"
    ;;
  screen) ours; /usr/bin/python3 "$GUI/session.py" screen "$2" ;;
  monitors) ours; /usr/bin/python3 "$GUI/session.py" monitors "${2:-1}" ;;
  check) /usr/bin/python3 "$GUI/session.py" check ;;
  ask) ours; shift; ask "$@" ;;
  ibus)
    ours
    if [ -z "$(ibus address 2>/dev/null)" ]; then
      # The daemon ends when the process that started it does: it is kept
      # under a shell that only waits for it, and that shell is what is
      # written down (ending it ends what it started, by number).
      start_as ibus sh -c 'ibus-daemon --xim --panel disable --emoji-extension disable --cache none & wait' > "$T/ibus.log" 2>&1
      w 3
    fi
    # The sandbox's own, never the one of the desktop this is run from.
    case "$(ibus address)" in *"$H/"*) ;; *) echo "sandbox.sh: IBus is at $(ibus address), which is not the sandbox's" >&2; exit 1 ;; esac
    [ -n "$2" ] && { ibus engine "$2"; w 1; }
    echo "input method: $(ibus engine 2>&1) at $(ibus address)"
    ;;
  look)
    ours
    echo "the session says: $(published); colour scheme $(gsettings get org.gnome.desktop.interface color-scheme 2>&1)"
    focus=$(xdotool getwindowfocus 2>/dev/null)
    echo "windows:"
    for id in $(xdotool search --onlyvisible --name '.' 2>/dev/null); do
      echo "  $id $(xdotool getwindowname "$id" 2>/dev/null) [$(place "$id")]$([ "$focus" = "$id" ] && echo " has the keyboard")"
    done
    echo "settings: $(cat "$XDG_CONFIG_HOME/heyo/settings.json" 2>/dev/null)"
    echo "autostart: $(ls "$XDG_CONFIG_HOME/autostart" 2>/dev/null | tr '\n' ' ')"
    if heyo=$(pid heyo); then
      echo "Heyo runs: $heyo, $(awk '/^VmRSS|^RssAnon|^RssFile/ {printf "%s %.1f MB  ", $1, $2 / 1024}' "/proc/$heyo/status")Pss: $(awk '/^Pss:/ {printf "%.1f MB", $2 / 1024}' "/proc/$heyo/smaps_rollup"); threads $(ls "/proc/$heyo/task" | wc -l); processor time so far $(sed 's/.*) //' "/proc/$heyo/stat" | awk -v hz="$(getconf CLK_TCK)" '{printf "%.2f s", ($12 + $13) / hz}')"
    fi
    ;;
  census) python3 "$GUI/ledger.py" census ;;
  run) shift; exec "$@" ;;
  *) sed -n '2,30p' "$0" | sed 's/^# \{0,1\}//'; exit 2 ;;
esac

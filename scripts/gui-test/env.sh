# The sandbox's environment: its own display, bus and home directories, so
# that nothing of the desktop Heyo is developed on is touched. Sourced by
# every script that tries something in the sandbox; sandbox.sh starts and
# stops it. A script that is not in this directory says where this is
# first: GUI=…/scripts/gui-test.
HERE=$(cd "$(dirname "$0")" && pwd)
GUI=${GUI:-$HERE}
REPO=$(cd "$GUI/../.." && pwd)
T=${HEYO_TEST_DIR:-${TMPDIR:-/tmp}/heyo-gui-test}
mkdir -p "$T/home/data" "$T/home/config/heyo" "$T/home/cache" "$T/home/run" "$T/home/user" "$T/shots" "$T/results"
chmod 700 "$T/home/run"
# The home directories are reached through a short link. Sockets are made
# under them, and the path of a Unix socket is at most 107 bytes: where
# $XDG_CACHE_HOME/ibus/dbus-XXXXXXXX does not fit, the ibus-daemon that
# GNOME Shell starts never finishes starting. It retries the bind without
# end, taking 2 GB of memory a minute, and nothing here would stop it.
if [ -z "$HEYO_TEST_HOME" ]; then
  HEYO_TEST_HOME=${XDG_RUNTIME_DIR:-/tmp}/heyo-gui-$(printf %s "$T" | cksum | cut -d' ' -f1)
fi
if [ ! -e "$HEYO_TEST_HOME" ] || [ -L "$HEYO_TEST_HOME" ]; then
  [ "$(readlink "$HEYO_TEST_HOME" 2>/dev/null)" = "$T/home" ] || ln -sfn "$T/home" "$HEYO_TEST_HOME"
fi
export HEYO_TEST_HOME
H=$HEYO_TEST_HOME
if [ "$(printf %s "$H/cache/ibus/dbus-XXXXXXXX" | wc -c)" -gt 107 ]; then
  echo "gui-test: $H is too long a path for the sockets under it" >&2
  exit 1
fi
# The display is :99 unless HEYO_TEST_DISPLAY names another (several
# sandboxes at once need one each, and a directory each: HEYO_TEST_DIR).
# Never the display of a desktop somebody works on.
export HEYO_TEST_DISPLAY="${HEYO_TEST_DISPLAY:-:99}"
case "$HEYO_TEST_DISPLAY" in
  :0 | :1) echo "gui-test: $HEYO_TEST_DISPLAY is a desktop's display, not a sandbox's" >&2; exit 1 ;;
esac
export DISPLAY="$HEYO_TEST_DISPLAY" XDG_DATA_HOME="$H/data" XDG_CONFIG_HOME="$H/config" XDG_CACHE_HOME="$H/cache"
export DBUS_SESSION_BUS_ADDRESS="unix:path=$H/run/bus" XDG_RUNTIME_DIR="$H/run"
# A home with nothing in it: what a program keeps outside the directories
# above (pictures saved, a shell's extensions) stays in the sandbox too.
# REAL_HOME is whose the speech engine is, which the scripts that dictate
# read and never write.
REAL_HOME=${REAL_HOME:-$(getent passwd "$(id -u)" | cut -d: -f6)}
export REAL_HOME HOME="$H/user"
# Only this display, and no file-system service of the desktop's. (And the
# scripts' own Python leaves nothing behind beside them.)
export GDK_BACKEND=x11 GIO_USE_VFS=local PYTHONDONTWRITEBYTECODE=1
unset WAYLAND_DISPLAY XAUTHORITY
# The scale is what the session publishes (sandbox.sh), found as on a real
# desktop: nothing says it in the environment, where it would be believed
# before the session and so hide every fault in finding it.
unset GDK_SCALE GDK_DPI_SCALE WINIT_X11_SCALE_FACTOR QT_SCALE_FACTOR QT_AUTO_SCREEN_SCALE_FACTOR
# An input method's bus is found through the home directories above, unless
# the environment names one: which would be the one of the desktop in use.
unset IBUS_ADDRESS IBUS_ADDRESS_FILE
# The scripts this one starts are told the directory too (ledger.py reads it).
export HEYO_TEST_DIR="$T"
# Where the test window (scripts/wayland-test/in/window.py) writes its text
# and its process number: in the sandbox, so that two sandboxes at once
# never read, or send a signal to, each other's window.
export WINDOW_DIR="$T"
# Where a script that writes down what it saw puts it, with its pictures.
RESULTS=${HEYO_TEST_RESULTS:-$T/results}

w() { python3 -c "import time; time.sleep($1)"; }
now() { date +%s%3N; }

# start COMMAND…: runs it in the background. Every process the scripts
# start in the sandbox is started this way: ledger.py writes its number
# down in $T/pids, with when it was started (which tells it from a later
# process given the same number) and what it is, and ends the processes
# written down there and what those started themselves. It ends nothing
# else: no process is ever looked for by its name or by its environment.
# STARTED is the number. start_as NAME COMMAND… writes it down under a name
# of its own, for `stop NAME` and `alive NAME`.
start() {
  "$@" &
  STARTED=$!
  # One that cannot be written down is not left running: nothing would end it.
  if ! python3 "$GUI/ledger.py" note "$STARTED" "${START_AS:-$(basename "$1")}"; then
    kill "$STARTED" 2>/dev/null
    echo "gui-test: $T/pids cannot be written, so $1 was not started" >&2
    exit 1
  fi
}
start_as() { START_AS=$1; shift; start "$@"; START_AS=; }
# stop NAME…: ends the processes written down under those names, and what
# they started. alive NAME: whether one is still running. pid NAME: its number.
stop() { python3 "$GUI/ledger.py" end "$@"; }
alive() { python3 "$GUI/ledger.py" alive "$1" > /dev/null; }
pid() { python3 "$GUI/ledger.py" alive "$1"; }

# The build under test: debug (which loads the frontend from the dev server),
# or with HEYO_TEST_BUILD another directory under src-tauri/target whose
# build carries its own (release, or one made for the purpose), or the path
# of a program built elsewhere.
BUILD=${HEYO_TEST_BUILD:-debug}
case "$BUILD" in
  /*) HEYO=$BUILD; BUILD=$(basename "$(dirname "$BUILD")") ;;
  *) HEYO="$REPO/src-tauri/target/$BUILD/heyo" ;;
esac
[ "$BUILD" != debug ] && HEYO_TEST_FRONTEND=${HEYO_TEST_FRONTEND:-none}
export HEYO
# heyo ARGUMENTS…: a second Heyo that hands a request to the one running.
heyo() { timeout 10 "$HEYO" "$@"; }

# The scale the session publishes, as a whole number: what a script
# multiplies a logical size by when nothing in the window can be asked.
scale() {
  s=$(xrdb -query 2>/dev/null | awk '/^Xft.dpi:/{print int($2 / 96 + 0.5)}')
  echo "${s:-1}"
}
# window TITLE: the window of that title (a regular expression) that is on
# screen, if one is. wait_window TITLE [TENTHS]: waits for it.
window() { xdotool search --onlyvisible --name "$1" 2>/dev/null | tail -1; }
wait_window() {
  for _ in $(seq "${2:-100}"); do
    id=$(window "$1")
    [ -n "$id" ] && { echo "$id"; return 0; }
    sleep 0.1
  done
  return 1
}
# place ID: X Y W H of a window, in the screen's pixels.
place() {
  xwininfo -id "$1" | awk '/Absolute upper-left X/{x=$4} /Absolute upper-left Y/{y=$4} /Width:/{w=$2} /Height:/{h=$2} END{print x, y, w, h}'
}
viewable() { [ -n "$1" ] && xwininfo -id "$1" 2>/dev/null | grep -q "Map State: IsViewable"; }
# png FILE [X Y W H]: a picture of the screen, or of that part of it.
png() {
  if [ $# -gt 1 ]; then python3 "$GUI/grab.py" "$1" "$2" "$3" $(($2 + $4)) $(($3 + $5)); else python3 "$GUI/grab.py" "$1"; fi
}
# ask …: where a control of one of Heyo's Slint windows is, by its role and
# name, from the window itself (ask.py says how); `ask click` clicks it.
ask() { /usr/bin/python3 "$GUI/ask.py" "$@"; }
# banners off|on: whether the session shows a notification as a banner. A
# script that compares the screen with what it was turns them off for as
# long as it does: "Screenshot copied" is the desktop's to say, and it says
# it over whatever is there. One that is up already stays until the Shell
# has seen the pointer move, and then a few seconds: it is given both.
banners() {
  if [ "$1" = off ]; then
    gsettings set org.gnome.desktop.notifications show-banners false 2> /dev/null
    xdotool mousemove_relative 12 12; sleep 0.3; xdotool mousemove_relative -- -12 -12; sleep 6
  else
    gsettings reset org.gnome.desktop.notifications show-banners 2> /dev/null
  fi
}
# middle X Y W H: the middle of that rectangle.
middle() { [ $# -eq 4 ] && echo $(($1 + $3 / 2)) $(($2 + $4 / 2)); }
# toolbar BUTTON: X Y of the middle of a button of the screenshot's
# toolbar, asked of the overlay, which must be up with something selected.
# BUTTON is rect arrow pen text blur, red yellow green blue, undo, ocr pin
# save copy cancel. (The tools and the colours are buttons of which one is
# chosen, which the accessibility bus calls toggle buttons.)
toolbar() {
  case $1 in
    rect) set -- "toggle button" Rectangle ;; arrow) set -- "toggle button" Arrow ;; pen) set -- "toggle button" Pen ;;
    text) set -- "toggle button" Text ;; blur) set -- "toggle button" Blur ;;
    red) set -- "toggle button" Red ;; yellow) set -- "toggle button" Yellow ;; green) set -- "toggle button" Green ;; blue) set -- "toggle button" Blue ;;
    undo) set -- "push button" Undo ;; ocr) set -- "push button" "Copy Text" ;; pin) set -- "push button" Pin ;;
    save) set -- "push button" Save ;; copy) set -- "push button" Copy ;; cancel) set -- "push button" Cancel ;;
    *) echo "toolbar: no button $1" >&2; return 2 ;;
  esac
  # Waited for: the overlay says where its toolbar is a moment after it shows it.
  middle $(ask wait "$1" "$2" "Heyo Screenshot")
}
# toolbar_box: X Y W H of the toolbar itself.
toolbar_box() { ask wait panel Tools "Heyo Screenshot"; }
# pill: the dictation pill's window, if it is on screen. pill_at
# body|cancel: X Y of the middle of that part of it, asked of the pill.
pill() { window '^Heyo Dictation$'; }
pill_at() {
  case $1 in body) set -- Dictation ;; cancel) set -- Cancel ;; esac
  middle $(ask wait "push button" "$1" "Heyo Dictation")
}

# session_for xfwm|gnome: the session a script tries things in. One that
# runs already is used as it is and left running, so that several scripts
# can share it; else one is started, and `leave` ends it again.
session_for() {
  if alive Xvfb; then OWN_SESSION=; else "$GUI/sandbox.sh" session "${1:-xfwm}" > /dev/null || exit 1; OWN_SESSION=yes; fi
  alive heyo && { echo "$(basename "$0"): Heyo is running in the sandbox already" >&2; exit 1; }
  mkdir -p "$RESULTS"
}
leave() { if [ -n "$OWN_SESSION" ] && [ -z "$KEEP" ]; then "$GUI/sandbox.sh" down > /dev/null; fi; }

# seed [name=value…]: the settings the next Heyo starts with (seed.py).
seed() { mkdir -p "$XDG_CONFIG_HOME/heyo"; python3 "$GUI/seed.py" "$XDG_CONFIG_HOME/heyo/settings.json" "$@"; }
# stored NAME: a setting as Heyo has stored it.
stored() { python3 -c 'import json, sys; print(json.load(open(sys.argv[1])).get(sys.argv[2]))' "$XDG_CONFIG_HOME/heyo/settings.json" "$1"; }
# extensions NAME…: those of extensions/ installed for the next Heyo.
extensions() {
  mkdir -p "$XDG_DATA_HOME/heyo/extensions"
  for extension in "$@"; do
    rm -rf "$XDG_DATA_HOME/heyo/extensions/$extension"
    cp -r "$GUI/extensions/$extension" "$XDG_DATA_HOME/heyo/extensions/"
  done
}
# speech: the speech engine installed for whoever runs this, linked into
# the sandbox: read through the link, never written.
speech() {
  mkdir -p "$XDG_DATA_HOME/heyo"
  ln -sfn "${HEYO_TEST_SPEECH:-$REAL_HOME/.local/share/heyo/dictation}" "$XDG_DATA_HOME/heyo/dictation"
}

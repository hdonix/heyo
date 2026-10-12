# Sourced by the scripts that try the Slint settings window. Everything runs
# in a sandbox of its own (sandbox.sh, beside this file): an X server
# in memory, a private bus, and home directories that are not the user's.
# Every process started is written down by its number, in the sandbox's own
# pids/ (so `sandbox.sh down` ends it too) and in a ledger, and is ended by
# that number: never by a name.
#
#   SLINT_SETTINGS_DISPLAY   the display, :82 unless said
#   SLINT_SETTINGS_DIR       where the sandbox keeps its files (a short path:
#                            sockets are made under it)
#   SLINT_SETTINGS_SCRATCH   where logs and the ledger go
#   HEYO                     the build under test
HERE=$(cd "$(dirname "$0")" && pwd)
W=$(cd "$HERE/../../.." && pwd)
RESULTS=$W/experiments/slint-settings/results
export SLINT_SETTINGS_DISPLAY=${SLINT_SETTINGS_DISPLAY:-:82} SLINT_SETTINGS_DIR=${SLINT_SETTINGS_DIR:-/tmp/heyo-slint}
D=$SLINT_SETTINGS_DIR
S=${SLINT_SETTINGS_SCRATCH:-$D/scratch}
eval "$("$HERE/sandbox.sh" env)"
[ "$DISPLAY" = "$SLINT_SETTINGS_DISPLAY" ] && [ "$DISPLAY" != ":0" ] || { echo "not the sandbox's display" >&2; exit 1; }
# Heyo's own files stay in the sandbox too.
export HOME="$D/home"
# The scale: what the session publishes, where it has a settings daemon
# (sandbox.sh); a program then finds it as on a real desktop, and nothing
# says it in the environment. A bare X server publishes none: there it is
# said to GTK and to winit, each its own way.
if [ -f "$D/pids/xsettings" ]; then
  SCALE=$(xrdb -query 2>/dev/null | awk '/^Xft.dpi/{print int($2 / 96 + 0.5)}')
  SCALE=${SCALE:-1}
else
  SCALE=${GDK_SCALE:-2}
  export WINIT_X11_SCALE_FACTOR="$SCALE"
fi
unset IBUS_ADDRESS IBUS_ADDRESS_FILE
HEYO=${HEYO:-$W/src-tauri/target/release/heyo}
LEDGER=$S/pids.log
mkdir -p "$S" "$RESULTS"

now() { date +%s%3N; }

born() { sed 's/.*) //' "/proc/$1/stat" 2>/dev/null | cut -d' ' -f20; }

# note NAME PID: writes the process down.
note() {
  echo "$2 $(born "$2")" > "$D/pids/$1"
  echo "$(date +%H:%M:%S) $1 $2 $(born "$2")" >> "$LEDGER"
}

# Everything a process started, and what those started, each as
# number:when-started. Found by who started whom, never by a name.
family() {
  for child in $(ps -o pid= --ppid "$1" 2>/dev/null); do
    echo "$child:$(born "$child")"
    family "$child"
  done
}

# end_one PID STARTED WHAT: ends that process if it is still the same one.
end_one() {
  [ -n "$2" ] && [ "$(born "$1")" = "$2" ] || return 0
  kill "${4:--TERM}" "$1" 2>/dev/null
  for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15; do [ "$(born "$1")" = "$2" ] || break; sleep 0.2; done
  [ "$(born "$1")" = "$2" ] && kill -9 "$1" 2>/dev/null
  echo "ended $3 $1"
}

# finish NAME: ends the process written down as NAME, by its number, if it is
# still the same process, and then what it had started itself; says so.
finish() {
  [ -f "$D/pids/$1" ] || return 0
  read -r pid started < "$D/pids/$1"
  if [ -n "$started" ] && [ "$(born "$pid")" = "$started" ]; then
    members=$(family "$pid")
    end_one "$pid" "$started" "$1" "${2:-}"
    for member in $members; do end_one "${member%%:*}" "${member#*:}" "started by $1"; done
  else
    echo "$1 $pid had ended"
  fi
  rm -f "$D/pids/$1"
}

# alive NAME: whether the process written down as NAME is still that process.
alive() {
  [ -f "$D/pids/$1" ] || return 1
  read -r pid started < "$D/pids/$1"
  [ -n "$started" ] && [ "$(born "$pid")" = "$started" ]
}

# The window titled "Heyo Settings" that is on screen, if one is.
settings_window() { xdotool search --onlyvisible --name '^Heyo Settings$' 2>/dev/null | tail -1; }

# wait_window [TENTHS]: waits for it; prints its id.
wait_window() {
  for _ in $(seq "${1:-100}"); do
    id=$(settings_window)
    [ -n "$id" ] && { echo "$id"; return 0; }
    sleep 0.1
  done
  return 1
}

# place ID: X Y W H of a window, in the screen's pixels.
place() {
  xwininfo -id "$1" | awk '/Absolute upper-left X/{x=$4} /Absolute upper-left Y/{y=$4} /Width:/{w=$2} /Height:/{h=$2} END{print x, y, w, h}'
}

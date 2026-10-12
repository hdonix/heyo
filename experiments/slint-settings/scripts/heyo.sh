#!/bin/sh
# The real Heyo in the sandbox, with its settings drawn by Slint or by the
# web view.
#
#   heyo.sh up [on|off|gnome]        the sandbox (experiments/overlay-bench/sandbox.sh)
#   heyo.sh start [slint|web] [VAR=value…]   Heyo, hidden; its log is $S/heyo.log
#   heyo.sh seed [name=value…]       the settings Heyo will start with (seed.py)
#   heyo.sh ibus                     an input method of the sandbox's own, with Pinyin
#   heyo.sh dconf                    a settings service of the sandbox's own, for hotkeys to be kept in
#   heyo.sh open                     asks for the settings, as the palette does
#   heyo.sh shot NAME                the settings window as $RESULTS/NAME.png
#   heyo.sh stop                     ends Heyo
#   heyo.sh down                     ends the sandbox and all it started
#   heyo.sh sh CMD…                  runs a command in the sandbox's environment
. "$(dirname "$0")/lib.sh"
case "${1:-}" in
  up)
    COMPOSITOR=${2:-on} "$OB/sandbox.sh" up
    # Settings the tests start from: a window that stays when another is clicked.
    mkdir -p "$XDG_CONFIG_HOME/heyo"
    [ -f "$XDG_CONFIG_HOME/heyo/settings.json" ] || printf '{ "hideOnBlur": false }\n' > "$XDG_CONFIG_HOME/heyo/settings.json"
    ;;
  start)
    by=${2:-slint}
    shift; [ $# -gt 0 ] && shift
    alive heyo && { echo "Heyo is running already" >&2; exit 1; }
    [ "$by" = slint ] && export HEYO_SETTINGS=slint
    for assignment in "$@"; do export "$assignment"; done
    # PIN names the processors it may use (taskset's list), when it is to
    # be measured on fewer.
    ${PIN:+taskset -c "$PIN"} "$HEYO" --hidden > "$S/heyo.log" 2>&1 &
    note heyo $!
    # It reads the applications before it can open anything by name.
    for _ in $(seq 150); do grep -q "heyo: started" "$S/heyo.log" 2>/dev/null && break; sleep 0.1; done
    sleep "${HEYO_WARMUP:-6}"
    echo "Heyo $(cut -d' ' -f1 "$D/pids/heyo") ($by)"
    ;;
  dconf)
    # Settings of the desktop that stay (GNOME's shortcuts are kept there):
    # the sandbox's own, in its own home, served on its own bus. Heyo is
    # then started with GSETTINGS_BACKEND=dconf.
    alive dconf && exit 0
    GSETTINGS_BACKEND=dconf /usr/libexec/dconf-service > "$S/dconf.log" 2>&1 &
    note dconf $!
    sleep 0.5
    ;;
  seed)
    # The settings Heyo starts with: seed.py's, with name=value changes.
    shift
    alive heyo && { echo "Heyo is running: it would not read them" >&2; exit 1; }
    mkdir -p "$XDG_CONFIG_HOME/heyo"
    python3 -I "$HERE/seed.py" "$XDG_CONFIG_HOME/heyo/settings.json" "$@"
    # Two extensions, so that an extension's page is among the pages: the
    # ones the GUI tests install, one of them with something allowed.
    mkdir -p "$XDG_DATA_HOME/heyo/extensions"
    rm -rf "$XDG_DATA_HOME/heyo/extensions/api-probe" "$XDG_DATA_HOME/heyo/extensions/view-probe"
    cp -r "$W/scripts/gui-test/extensions/api-probe" "$W/scripts/gui-test/extensions/view-probe" "$XDG_DATA_HOME/heyo/extensions/"
    printf '{ "api-probe": ["windows", "screen.capture"] }\n' > "$XDG_CONFIG_HOME/heyo/permissions.json"
    ;;
  ibus)
    # An input method of the sandbox's own, with Pinyin chosen: its
    # directories are the sandbox's, never the desktop's. Heyo is then
    # started with XMODIFIERS=@im=ibus GTK_IM_MODULE=ibus.
    alive ibus && exit 0
    # The daemon ends when the process that started it does: it is kept
    # under a shell that only waits for it, and that shell is what is
    # written down (ending it ends what it started, by number).
    sh -c 'ibus-daemon --xim --panel disable --emoji-extension disable --cache none & wait' > "$S/ibus.log" 2>&1 &
    note ibus $!
    sleep 3
    case "$(ibus address)" in *"$D/"*) ;; *) echo "not the sandbox's own daemon: stopping it" >&2; finish ibus; exit 1 ;; esac
    ibus engine libpinyin; sleep 1
    echo "input method: $(ibus engine) at $(ibus address)"
    ;;
  open)
    timeout 10 "$HEYO" --open heyo:settings
    wait_window 100 > /dev/null || { echo "no settings window came up" >&2; exit 1; }
    ;;
  shot)
    id=$(settings_window); [ -n "$id" ] || { echo "no settings window" >&2; exit 1; }
    set -- "$2" $(place "$id")
    mkdir -p "$(dirname "$RESULTS/$1")"
    python3 -I "$HERE/shot.py" "$DISPLAY" "$RESULTS/$1.png" "$2" "$3" "$4" "$5"
    echo "$RESULTS/$1.png ${4}x${5}"
    ;;
  stop)
    finish heyo
    ;;
  down)
    "$OB/sandbox.sh" down
    ;;
  sh)
    shift
    "$@"
    ;;
  *)
    sed -n '2,15p' "$0" | sed 's/^# \{0,1\}//'
    exit 2
    ;;
esac

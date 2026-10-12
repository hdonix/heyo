#!/bin/sh
# The dictation pill as a window: that it never takes the keyboard from the
# application being dictated into, by appearing or by being clicked; that a
# click on it finishes the recording and its button, which shows when the
# pointer is on it, cancels; that it is above a fullscreen window; and that
# when Slint is lost, dictation goes on without it and nothing stays on
# screen.
#
#   pill.sh NAME [lost]     what was seen goes to $RESULTS/pill-NAME.txt,
#                           pictures beside it as pill-NAME-*.png
#
# In the sandbox (heyo.sh up gnome, scale 1); Heyo must not be running in
# it. Spoken sentences stand in for the microphone (scripts/gui-test/say.py)
# and the speech engine is the one installed for whoever runs this (read,
# never written). The application is a window with a text box
# (scripts/wayland-test/in/window.py), whose text is read back from a file.
. "$(dirname "$0")/lib.sh"
name=$1; lost=${2:-}
out=$RESULTS/pill-$name.txt
alive heyo && { echo "Heyo is running already" >&2; exit 1; }
scale=$SCALE
mkdir -p "$XDG_DATA_HOME/heyo"
ln -sfn "$(getent passwd "$(id -u)" | cut -d: -f6)/.local/share/heyo/dictation" "$XDG_DATA_HOME/heyo/dictation"
"$HERE/heyo.sh" seed dictationLive=true dictationPolish=off dictationLanguage=en > /dev/null
/usr/bin/python3 "$W/scripts/gui-test/say.py" "$D/spoken.wav" "The weather is nice today, so let us walk to the park." "I will bring forty two sandwiches and some coffee." "See you at three thirty." > /dev/null || exit 1
export HEYO_DICTATION_WAV="$D/spoken.wav"
[ "$lost" = lost ] && export HEYO_SLINT_PANIC_AFTER=13000

focus() { xdotool getwindowname "$(xdotool getwindowfocus 2>/dev/null)" 2>/dev/null; }
text() { cat /tmp/window-org.test.Notes.txt 2>/dev/null; }
pill() { xdotool search --onlyvisible --name '^Heyo Dictation$' 2>/dev/null | tail -1; }
# The pill's window, X Y W H; and a picture of the bottom of the screen.
where() { id=$(pill); [ -n "$id" ] && place "$id" || echo none; }
shot() { set -- "$1" $(xdotool getdisplaygeometry); python3 -I "$HERE/shot.py" "$DISPLAY" "$RESULTS/pill-$name-$1.png" $(($2 / 2 - 300 * scale)) $(($3 - $3 / 10 - 110 * scale)) $((600 * scale)) $((130 * scale)); }
dictate() { timeout 10 "$HEYO" --dictate; }
# at DX DY: the pointer there, from the middle of the pill's window.
at() { set -- "$1" "$2" $(where); xdotool mousemove $(($3 + $5 / 2 + $1 * scale)) $(($4 + $6 / 2 + $2 * scale)); }

{
  echo "# $(date +%F), $(basename "$HEYO")${lost:+, Slint lost in the middle}"
  "$HEYO" --hidden > "$S/heyo.log" 2>&1 &
  note heyo $!
  /usr/bin/python3 "$W/scripts/wayland-test/in/window.py" Notes org.test.Notes "Notes: " > /dev/null 2>&1 &
  note notes $!
  sleep "${HEYO_WARMUP:-8}"
  xdotool search --name '^Notes$' | tail -1 | xargs xdotool windowactivate; sleep 1
  echo "before: the keyboard is with [$(focus)]; the pill's window: $(where)"

  if [ "$lost" = lost ]; then
    dictate; sleep 2.5
    echo "recording: the pill's window: $(where); the text so far [$(text)]"
    sleep 7
    echo "after Slint's thread panicked: Heyo $(alive heyo && echo runs || echo "DOES NOT RUN"); the pill's window: $(where); windows of Heyo's on screen: $(xdotool search --onlyvisible --name '^Heyo' 2>/dev/null | wc -l)"
    shot lost
    sleep 4; dictate; sleep 4
    echo "finished without a pill: the text is [$(text)]"
    dictate; sleep 3
    echo "a recording begun after that: the pill's window: $(where); the text so far [$(text)]"
    dictate; sleep 3
    echo "and finished: [$(text)]"
  else
    # A click on the pill finishes; the keyboard stays where it was.
    dictate; sleep 3
    echo "recording: the keyboard is with [$(focus)]; the pill's window: $(where); the text so far [$(text)]"
    shot listening
    sleep 2.5; at -20 0; sleep 0.6; shot hovered
    xdotool click 1; sleep 0.3
    echo "clicked on the pill: the keyboard is with [$(focus)]"
    sleep 4
    echo "after the click: the pill's window: $(where); the text is [$(text)]"

    # Its button cancels. (What a recording has pasted stays, and the
    # pill says so; what was being composed by an input method is gone.)
    xdotool mousemove 100 100; sleep 0.5
    before=$(text)
    dictate; sleep 5
    echo "a second recording: the text so far [$(text)]"
    at 0 0; sleep 0.6; at 49 0; sleep 0.4; shot button
    xdotool click 1; sleep 0.6; shot cancelled
    echo "clicked on its button: the keyboard is with [$(focus)]"
    sleep 4
    echo "after the button: the pill's window: $(where); the text is $([ "$(text)" = "$before" ] && echo "as before the recording" || echo "[$(text)]")"

    # Above a fullscreen window, which has the keyboard and keeps it.
    xdotool mousemove 100 100
    WINDOW_FULLSCREEN=1 /usr/bin/python3 "$W/scripts/wayland-test/in/window.py" Film org.test.Film "Film: " > /dev/null 2>&1 &
    note film $!
    sleep 2.5
    dictate; sleep 3
    set -- $(xdotool search --name '^Film$' | tail -1)
    echo "over a fullscreen window [$(place "$1")]: the keyboard is with [$(focus)]; the pill's window: $(where)"
    shot fullscreen
    dictate; sleep 4
    echo "and its text is [$(cat /tmp/window-org.test.Film.txt 2>/dev/null)]"
    finish film
  fi
  echo "## what Heyo said"
  grep -i "slint\|dictation\|panic" "$S/heyo.log" | grep -v "^ " | cut -c1-200
  finish notes
  finish heyo
} > "$out" 2>&1
cat "$out"

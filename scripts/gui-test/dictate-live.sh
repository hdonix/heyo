#!/bin/sh
# Type as you speak, end to end: a few spoken sentences (say.py, in place of
# a microphone) are dictated into another application's text field, which
# is read back all the while: its text, and apart from it what an input
# method is composing there. The field holds "Notes: " already, with the
# cursor after it.
#
# First by pasting, in a session without an input method (xfwm, no IBus):
#
#   sentences  the setting off: each sentence pasted when it is finished
#   live       the setting on: the words must be there while they are still
#              being spoken, and end up as the same text
#   typed      a character typed by hand in the middle: from then on
#              nothing may be erased, and nothing of the user's is lost
#   clicked    finished in the middle of a sentence with a click on the
#              dictation window, which is not a click in the application:
#              the sentence is still corrected where it stands
#   cancelled  the window's X in the middle: the words are taken back
#   polished   with a model that polishes (llm-stub.py): what was written as
#              it was heard is replaced by the polished text
#
# Then through the input method, in a session that has one (GNOME Shell,
# which runs IBus, with Pinyin chosen as the user's own):
#
#   composed   the sentence in progress is composed in the field and
#              committed when it is finished; the same text; the clipboard
#              is never touched; Pinyin is the session's engine again
#   keyed      a character typed by hand in the middle: it lands behind
#              what was composed by then, and the rest behind it
#   tapped     started and, in the middle of a sentence, finished with the
#              dictation hotkey, which takes the focus from the field for as
#              long as it is down: nothing half-recognized is left behind
#   held       the hotkey held throughout: the words go in when it is let go
#   clicked    finished with a click on the dictation window
#   cancelled  the window's X: what was being composed is gone, and the
#              finished sentence stays
#   talked     with the key held to talk, which is a key of Heyo's and not
#              of the user's typing
#   moved      another window comes to the front in the middle of a
#              sentence: nothing of it is left in the first, and all of it
#              is in the second
#   terminal   into a terminal, which takes the input method like any field
#   switched   the user chooses another input source in the middle: that
#              one is the session's from then on, and the rest is pasted
#   plain      into a field that takes no input method: pasted, as above
#   killed     Heyo killed in the middle of a sentence: the session is left
#              without an engine, and has Pinyin back when Heyo starts again
#   quiet      the setting off: sentences are committed, nothing is composed
#   polished   the whole recording is composed until the polished text is
#              committed in its place
#
# The user's vocabulary turns two of the words into Chinese and an emoji, so
# that what is written is not only ASCII. Uses the speech engine installed
# for the real Heyo (read only); say.py needs Flite (libflite1); the second
# half needs gnome-shell, ibus with ibus-libpinyin, gnome-terminal and, for
# the hotkey, gsd-media-keys. The recordings named "clicked" and "cancelled"
# need the interface (a debug build with HEYO_TEST_FRONTEND=none has none).
#
#   dictate-live.sh [pasted|composed]    one half only; the second compares
#                                        with what the first left behind
. "$(dirname "$0")/env.sh"
HALF=${1:-both}
PORT=18112
SPOKEN="$T/spoken.wav"
HOTKEY="ctrl+alt+d"
mkdir -p "$XDG_DATA_HOME/heyo"
ln -sfn "$HOME/.local/share/heyo/dictation" "$XDG_DATA_HOME/heyo/dictation"
LENGTH=$(/usr/bin/python3 "$HERE/say.py" "$SPOKEN" \
  "The weather is nice today, so let us walk to the park." \
  "I will bring forty two sandwiches and some coffee." \
  "See you at three thirty.") || exit 1
# The key held to talk is pressed by xdotool here, which a debug build can
# be told to take for the keyboard.
export HEYO_DICTATION_WAV="$SPOKEN" HEYO_TALKKEY_SYNTHETIC=1
unset ANTHROPIC_API_KEY ANTHROPIC_BASE_URL ANTHROPIC_AUTH_TOKEN
# What a GNOME session's settings tell the toolkits, and nothing here does.
export GTK_IM_MODULE=ibus QT_IM_MODULE=ibus XMODIFIERS=@im=ibus
[ "$HALF" != composed ] && rm -f "$T"/live-*.json
rm -f "$T"/composed-*.json "$T/term.txt"
INTERFACE=yes
[ "$BUILD" = debug ] && [ "$HEYO_TEST_FRONTEND" = none ] && INTERFACE=no

# configure LIVE POLISH [HOLD-KEY]: the settings of the next Heyo.
configure() {
  python3 - "$XDG_CONFIG_HOME/heyo/settings.json" "$1" "$2" "$PORT" "${3:-}" <<'PY'
import json, os, sys
path, live, polish, port, hold = sys.argv[1:6]
settings = json.load(open(path)) if os.path.exists(path) else {}
settings.update(dictationLive=live == "on", dictationPolish=polish, dictationHoldKey=hold, dictationLanguage="en",
                dictationVocabulary=["sandwiches -> 三明治", "coffee -> 咖啡😀"], clipboardPaused=False,
                aiProvider="local", anthropicApiKey="", localModelUrl=f"http://127.0.0.1:{port}/v1", localModelName="stub-model")
json.dump(settings, open(path, "w"))
PY
}
# notes [none]: the field to dictate into; "none" for one that takes no input method.
notes() {
  (WINDOW_INPUT_METHOD=$1 /usr/bin/python3 "$REPO/scripts/wayland-test/in/window.py" Notes org.test.Notes "Notes: " >/dev/null 2>&1 &); w 2
  front '^Notes$'
}
front() { xdotool search --name "$1" | tail -1 | xargs xdotool windowactivate; w 1; }
close_notes() { xdotool search --name '^Notes$' | tail -1 | xargs xdotool windowclose; w 1; }
engine() { ibus engine 2>/dev/null | grep -v "^No engine" | head -1; }
# The session's own picture of the input source in use: the one used last.
source_in_use() { gsettings get org.gnome.desktop.input-sources mru-sources | sed "s/^[^(]*(\([^)]*\)).*/\1/"; }
# record NAME [SECONDS WHAT [START [FIELD]]]: one recording, with the field
# read every 20 ms. START is how it begins and, unless WHAT ends it, ends:
# "cli" (heyo --dictate), "tap" (the hotkey), "hold" (the hotkey, held) or
# "talk" (the key held to talk). At SECONDS: "click" on the dictation window
# (which finishes), "cancel" on its X, "finish" (as it began, before the
# speech is over), "kill" (Heyo is killed), "move" (the window Other comes
# to the front), "switch" (the next input source is chosen), or any other
# WHAT typed by hand as it is. FIELD is what to read instead of Notes: a
# file that a terminal writes when Enter is pressed.
record() {
  python3 - "$REPO/src-tauri/target/$BUILD/heyo" "$T/$1.json" "$LENGTH" "${2:-0}" "${3:-}" "${4:-cli}" "${5:-}" "$HOTKEY" <<'PY'
import json, os, signal, subprocess, sys, threading, time
heyo, out, length, at, what, begun, terminal, hotkey = sys.argv[1], sys.argv[2], float(sys.argv[3]), float(sys.argv[4]), *sys.argv[5:9]
notes = "/tmp/window-org.test.Notes"
states, stop = [], threading.Event()
def run(*command):
    return subprocess.run(command, capture_output=True, text=True, timeout=15).stdout
def read(path):
    try:
        return open(path).read()
    except OSError:
        return None
def watch():
    start, last = time.time(), None
    while not stop.is_set():
        now = [read(terminal) or ""] if terminal else [read(notes + ".txt"), read(notes + ".composing.txt")]
        if None not in now and now != last:
            states.append([round(time.time() - start, 2), *now, *([""] if terminal else [])]); last = now
        time.sleep(0.02)
def pill():
    """The dictation window where it is on screen: left, top, or nothing."""
    shown = run("xdotool", "search", "--onlyvisible", "--name", "Heyo Dictation").split()
    if not shown: return None
    info = run("xwininfo", "-id", shown[-1])
    at = lambda name: int(next(l.split()[-1] for l in info.splitlines() if name in l))
    return at("Absolute upper-left X"), at("Absolute upper-left Y"), at("Width"), at("Height")
def engine():
    said = run("ibus", "engine").strip()
    return "" if said.startswith("No engine") else said
def toggle():
    run("xdotool", "key", hotkey) if begun == "tap" else run(heyo, "--dictate")
def talk_key():
    """Right Alt, by its keycode: asked for by name, xdotool presses Left Alt with it."""
    return next(line.split()[1] for line in run("xmodmap", "-pke").splitlines() if line.split()[3:4] == ["Alt_R"])
held = hotkey if begun == "hold" else talk_key() if begun == "talk" else None
def kill():
    """The sandbox's Heyo, as nothing it could prepare for: its display says which it is."""
    display = os.environ["DISPLAY"].encode()
    for pid in filter(str.isdigit, os.listdir("/proc")):
        try:
            args = open(f"/proc/{pid}/cmdline", "rb").read().split(b"\0")
            inside = b"DISPLAY=" + display + b"\0" in open(f"/proc/{pid}/environ", "rb").read() + b"\0"
        except OSError:
            continue
        if inside and os.path.basename(args[0]) == b"heyo" and b"--hidden" in args:
            os.kill(int(pid), signal.SIGKILL)
# The field as it is before every recording: some text of the user's own.
# (Not typed: that would go through the user's input method.)
if not terminal:
    os.kill(int(open(notes + ".pid.txt").read()), signal.SIGUSR1); time.sleep(0.7)
before = engine()
watching = threading.Thread(target=watch); watching.start()
start = time.time()
run("xdotool", "keydown", held) if held else toggle()
time.sleep(2.0)
during = engine()
ended = None
if what in ("click", "cancel"):
    time.sleep(max(0, at - (time.time() - start)))
    x, y, width, height = pill()
    # The pill is in the middle of its window; its X is at the pill's right end.
    run("xdotool", "mousemove", str(x + width // 2 + (57 if what == "cancel" else -20)), str(y + height // 2))
    ended = round(time.time() - start, 2)
    run("xdotool", "click", "1")
elif what == "kill":
    time.sleep(max(0, at - (time.time() - start)))
    ended = round(time.time() - start, 2)
    kill()
elif what == "move":
    time.sleep(max(0, at - (time.time() - start)))
    run("xdotool", "windowactivate", run("xdotool", "search", "--name", "^Other$").split()[-1])
elif what == "switch":
    time.sleep(max(0, at - (time.time() - start))); run("xdotool", "key", "super+space")
elif what and what != "finish":
    time.sleep(max(0, at - (time.time() - start))); run("xdotool", "type", what)
if ended is None:
    time.sleep(max(0, (at if what == "finish" else length + 1.0) - (time.time() - start)))
    ended = round(time.time() - start, 2)
    run("xdotool", "keyup", held) if held else toggle()
# Until the dictation window has gone: the text is all there before it goes.
time.sleep(1.5)
deadline = time.time() + 40
while what != "kill" and pill() and time.time() < deadline:
    time.sleep(0.2)
time.sleep(1.0)
if terminal:
    run("xdotool", "key", "Return"); time.sleep(0.7)
stop.set(); watching.join()
json.dump({"ended": ended, "states": states, "engines": [before, during, engine()], "other": read("/tmp/window-org.test.Other.txt")}, open(out, "w"), ensure_ascii=False)
PY
}

# --- By pasting: a session without an input method ---------------------------------
if [ "$HALF" != composed ]; then
  configure off off; "$HERE/up.sh"; notes
  record live-sentences
  python3 "$HERE/down.py" > /dev/null

  configure on off; "$HERE/up.sh"; notes
  record live-live
  record live-typed 3.2 '#'
  if [ "$INTERFACE" = yes ]; then
    record live-clicked 8.6 click
    record live-cancelled 8.6 cancel
  fi
  grep "heyo: dictation" "$T/app.log" | tail -4 | cut -c1-160
  python3 "$HERE/down.py" > /dev/null

  configure on clean; "$HERE/up.sh"; notes
  (python3 "$HERE/llm-stub.py" "$PORT" &); w 1
  record live-polished
  grep "heyo: dictation" "$T/app.log" | tail -3 | cut -c1-160
  python3 "$HERE/down.py" > /dev/null
fi

# --- Through the input method: GNOME Shell and IBus, with Pinyin chosen -------------
# session: the Shell, the user's input sources with Pinyin in use, the
# dictation hotkey as GNOME keeps it, something of the user's own on the
# clipboard, and the field.
session() {
  "$HERE/up.sh" gnome
  mine=$(ibus address)
  # The sandbox's own IBus, never the one of the desktop this is run from.
  case "$mine" in *"$H/"*) ;; *) echo "dictate-live: IBus is at $mine, which is not the sandbox's"; python3 "$HERE/down.py" > /dev/null; exit 1 ;; esac
  gsettings set org.gnome.desktop.input-sources sources "[('xkb', 'us'), ('ibus', 'libpinyin')]"; w 2
  # Chosen as the user chooses it, unless the Shell remembers it as chosen.
  [ "$(engine)" = libpinyin ] || { xdotool key super+space; w 2; }
  K=/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/heyo-dictation/
  gsettings set org.gnome.settings-daemon.plugins.media-keys custom-keybindings "['$K']"
  gsettings set org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:$K name 'Heyo Dictation'
  gsettings set org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:$K command "$REPO/src-tauri/target/$BUILD/heyo --dictate"
  gsettings set org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:$K binding '<Control><Alt>d'
  (/usr/libexec/gsd-media-keys > "$T/media-keys.log" 2>&1 &); w 3
  /usr/bin/python3 "$HERE/clip.py" "the user's own"; w 1.5
  (/usr/bin/python3 "$REPO/scripts/wayland-test/in/window.py" Other org.test.Other >/dev/null 2>&1 &); w 2
  notes
}
# leave: the sandbox as the scripts after this one expect it, with no input
# method chosen and no hotkey, and down.
leave() {
  gsettings reset org.gnome.desktop.input-sources sources
  gsettings reset org.gnome.desktop.input-sources mru-sources
  gsettings reset org.gnome.settings-daemon.plugins.media-keys custom-keybindings
  gsettings reset-recursively org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/heyo-dictation/
  w 1
  python3 "$HERE/down.py" > /dev/null
}
clipboard() { /usr/bin/python3 "$HERE/clip.py"; }
history() {
  python3 - "$XDG_DATA_HOME/heyo/clipboard/history.db" <<'PY'
import json, sqlite3, sys
print(json.dumps([text for (text,) in sqlite3.connect(sys.argv[1]).execute("select text from entries order by copied_at")], ensure_ascii=False))
PY
}
facts() { python3 -c "import json, sys; print(json.dumps(dict(zip(sys.argv[1::2], sys.argv[2::2])), ensure_ascii=False))" "$@" > "$T/composed-facts-$FACTS.json"; }

if [ "$HALF" = pasted ]; then
  :
elif command -v gnome-shell > /dev/null && [ -f /usr/share/ibus/component/libpinyin.xml ]; then
  rm -f "$XDG_DATA_HOME/heyo/clipboard/history.db"*
  configure on off alt_r; session
  ENGINE=$(engine); SOURCE=$(source_in_use); CLIPBOARD=$(clipboard)
  record composed-composed
  record composed-keyed 3.2 '#'
  [ "$BUILD" != release ] && record composed-talked 0 '' talk
  record composed-moved 3.2 move
  front '^Notes$'
  if [ -x /usr/libexec/gsd-media-keys ]; then
    record composed-tapped 8.6 finish tap
    record composed-held 8.6 finish hold
  fi
  if [ "$INTERFACE" = yes ]; then
    record composed-clicked 8.6 click
    record composed-cancelled 8.6 cancel
  fi
  if command -v gnome-terminal > /dev/null; then
    (gnome-terminal --title=ProbeTerm -- "$HERE/term-sink.sh" "$T/term.txt" >/dev/null 2>&1 &); w 4
    front ProbeTerm
    record composed-terminal 0 '' cli "$T/term.txt"
    xdotool search --name ProbeTerm | tail -1 | xargs xdotool windowclose; w 1
    front '^Notes$'
  fi
  FACTS=composed facts engine "$ENGINE" engineAfter "$(engine)" source "$SOURCE" sourceAfter "$(source_in_use)" address "$(ibus address)" \
    clipboard "$CLIPBOARD" clipboardAfter "$(clipboard)" history "$(history)" \
    pasted "$(grep -c "the rest is pasted\|not composed in the application" "$T/app.log")"
  # The user chooses the other input source while Heyo composes; and
  # afterwards Pinyin again, for what follows.
  record composed-switched 3.2 switch
  FACTS=switched facts sourceAfter "$(source_in_use)" said "$(grep "the rest is pasted" "$T/app.log" | tail -1)"
  xdotool key super+space; w 2
  # A field that takes no input method, in the same session.
  close_notes; notes none
  record composed-plain
  REFUSED=$(grep "not composed in the application" "$T/app.log" | tail -1)
  close_notes; notes
  # Heyo is killed while it composes, and started again.
  record composed-killed 3.2 kill
  LEFT=$(engine)
  ("$REPO/src-tauri/target/$BUILD/heyo" --hidden >> "$T/app.log" 2>&1 &)
  BACK=$(python3 - "$ENGINE" <<'PY'
import subprocess, sys, time
start = time.time()
while time.time() - start < 40:
    if subprocess.run(["ibus", "engine"], capture_output=True, text=True).stdout.strip() == sys.argv[1]:
        print(f"{time.time() - start:.1f} s"); break
    time.sleep(0.1)
else:
    print("never")
PY
)
  FACTS=killed facts left "$LEFT" back "$BACK" engineAfter "$(engine)" sourceAfter "$(source_in_use)" refused "$REFUSED" \
    said "$(grep "was given back" "$T/app.log" | tail -1)"
  grep "heyo: dictation\|input method" "$T/app.log" | tail -6 | cut -c1-200
  leave

  configure off off; session
  record composed-quiet
  leave

  configure on clean; session
  (python3 "$HERE/llm-stub.py" "$PORT" &); w 1
  record composed-polished
  FACTS=polished facts engineAfter "$(engine)" clipboardAfter "$(clipboard)"
  grep "heyo: dictation" "$T/app.log" | tail -3 | cut -c1-160
  leave
else
  echo "through the input method: not tried (it takes gnome-shell and ibus with ibus-libpinyin)"
fi
configure on off

python3 - "$T" <<'PY'
import json, os, sys
T = sys.argv[1]
BEFORE = "Notes: "
def there(name):
    return os.path.exists(f"{T}/{name}.json")
def recording(name):
    """When it ended, the field's states as (seconds, text, composing), its last text, the engines before, during and after."""
    r = json.load(open(f"{T}/{name}.json"))
    states = [(t, text, composing) for t, text, composing in r["states"]]
    return r["ended"], states, states[-1][1], r["engines"]
def erasures(states):
    return sum(1 for (_, a, _), (_, b, _) in zip(states, states[1:]) if not b.startswith(a))
def composing(states):
    return [c for _, _, c in states if c]
def other(name):
    """What the window Other held when that recording was over."""
    return json.load(open(f"{T}/{name}.json"))["other"]
def facts(name):
    return json.load(open(f"{T}/composed-facts-{name}.json"))
failed = []
def check(what, ok):
    print(f"  {'ok  ' if ok else 'FAIL'}  {what}")
    if not ok: failed.append(what)

print("BY PASTING")
_, states, plain, _ = recording("live-sentences")
print(f"sentences: the text changed {len(states) - 1} times; it ends as\n  [{plain}]")
check("each sentence went in whole: nothing was ever erased", erasures(states) == 0)
check("three sentences were heard, with Chinese and an emoji from the vocabulary", plain.count(".") >= 3 and "三明治" in plain and "咖啡😀" in plain)
first = plain[:plain.index(".") + 1]

ended, states, live, _ = recording("live-live")
during = [(t, s) for t, s, _ in states if t < ended]
began = next((t for t, s, _ in states if len(s) > len(BEFORE)), None)
print(f"live: the text changed {len(states) - 1} times, {erasures(states)} of them taking something back; the first words after {began} s, the recording ended at {ended} s")
for t, s, _ in states[1:6]:
    print(f"    {t:5.2f}  [{s}]")
check("the same text as sentence by sentence", live == plain)
check("words were there before their sentence was finished", any(len(s) > len(BEFORE) and not s.rstrip().endswith(".") for _, s in during))
check("all of the text was there before the recording ended", bool(during) and during[-1][1] == live)
check("it was corrected where it stood (text taken back and written again)", erasures(states) > 0)
check("what stood before the cursor was never touched", all(s.startswith(BEFORE) for _, s, _ in states))
check("nothing was ever composed: there is no input method here", not composing(states))

ended, states, typed, _ = recording("live-typed")
at = next((i for i, (_, s, _) in enumerate(states) if "#" in s), None)
print(f"typed: a # typed by hand in the middle; it ends as\n  [{typed}]")
check("the character is in the text", at is not None and typed.count("#") == 1)
if at is not None:
    check("nothing was erased after it: every later text begins with the one before", erasures(states[at:]) == 0)
    check("text was still added after it", len(typed) > len(states[at][1]))
    check("every sentence still ended up in the text", typed.replace("#", "").count(".") == plain.count("."))

if there("live-clicked"):
    ended, states, clicked, _ = recording("live-clicked")
    before = [s for t, s, _ in states if t < ended][-1]
    print(f"clicked: finished with a click on the dictation window; at the click the text was\n  [{before}]\nand it ends as\n  [{clicked}]")
    check("the click came in the middle of a sentence", not before.rstrip().endswith("."))
    check("the sentence was finished where it stood", clicked.rstrip().endswith(".") and clicked.startswith(first) and clicked.count(".") == 2)
    ended, states, cancelled, _ = recording("live-cancelled")
    before = [s for t, s, _ in states if t < ended][-1]
    print(f"cancelled: the window's X in the middle; at the click the text was\n  [{before}]\nand it ends as\n  [{cancelled}]")
    check("words had been written", len(before) > len(plain) // 3)
    check("and all of them were taken back, and nothing else", cancelled == BEFORE)
else:
    print("clicked, cancelled: not tried without the interface")

ended, states, polished, _ = recording("live-polished")
print(f"polished: it ends as\n  [{polished[:60]!r} … {polished[-40:]!r}]")
check("what was heard was in the text first", any(s == plain for _, s, _ in states))
check("and was replaced by the polished text", polished.startswith(BEFORE + "You asked: “") and plain[len(BEFORE):] in polished and polished.rstrip().endswith("```") and not polished.startswith(plain))
expected = polished

if not there("composed-composed"):
    print("FAILED: " + "; ".join(failed) if failed else "all as it should be")
    sys.exit(1 if failed else 0)

print("THROUGH THE INPUT METHOD")
session = facts("composed")
ended, states, composed, engines = recording("composed-composed")
shown = composing(states)
began = next((t for t, _, c in states if c), None)
print(f"composed: the field changed {len(states) - 1} times; the first words were composed after {began} s; the engines before, during and after: {engines}")
for t, s, c in states[1:7]:
    print(f"    {t:5.2f}  [{s}] composing [{c}]")
check("the same text as sentence by sentence", composed == plain)
check("the words of a sentence were composed in the field while it was spoken", len(shown) > 5 and any(len(c) > 9 and first.startswith(BEFORE + c) for c in shown))
check("and were not its text until the sentence was finished", all(s.rstrip().endswith(".") or s == BEFORE for _, s, _ in states))
check("nothing of the field's text was ever erased", erasures(states) == 0)
check("when the recording was over nothing was being composed", states[-1][2] == "")
check("the session's engine was Pinyin, Heyo's while it composed, and Pinyin again", engines == ["libpinyin", "heyo", "libpinyin"])

ended, states, keyed, engines = recording("composed-keyed")
at = next((i for i, (_, s, _) in enumerate(states) if "#" in s), None)
print(f"keyed: a # typed by hand in the middle; it ends as\n  [{keyed}]")
check("the character is in the text, once", at is not None and keyed.count("#") == 1)
if at is not None:
    was = next(c for _, _, c in reversed(states[:at]) if c)
    check(f"behind what was composed when it was typed [{was}]", keyed.startswith(BEFORE + was + "#"))
    check("nothing was erased, before it or after", erasures(states) == 0)
    rest = keyed.split("#", 1)[1]
    # The recognizer may have changed its mind about the word the key came
    # after: that word stays as it was heard, and is followed by the word whole.
    from_a_word = plain.endswith(rest) and plain[:-len(rest)].endswith(" ") and len(was) + len(rest) >= len(plain) - len(BEFORE)
    check("and what follows it is the rest of the text: nothing lost, nothing but a word the recognizer gave up twice", keyed.replace("#", "") == plain or from_a_word)
check("Pinyin again", engines[2] == "libpinyin")

if there("composed-tapped"):
    ended, states, tapped, engines = recording("composed-tapped")
    was = [c for t, _, c in states if t < ended and c][-1]
    print(f"tapped: begun with the hotkey and finished with it at {ended} s, when [{was}] was being composed; it ends as\n  [{tapped}]")
    check("the words were composed, though the hotkey that began it had taken the focus", engines[1] == "heyo" and len(composing(states)) > 5)
    check("the hotkey came in the middle of a sentence", not was.rstrip().endswith("."))
    check("the sentence was committed as it was recognized in the end, with nothing of the half-recognized one left before it", tapped.startswith(first) and tapped.count(".") == 2 and tapped.rstrip().endswith(".") and erasures(states) == 0)
    check("Pinyin again", engines[2] == "libpinyin")
    ended, states, held, engines = recording("composed-held")
    print(f"held: the hotkey held for {ended} s; the field changed {len(states) - 1} times, first at {states[1][0] if len(states) > 1 else None} s; it ends as\n  [{held}]")
    check("nothing went in while the hotkey was down", all(s == BEFORE and not c for t, s, c in states if t < ended))
    check("and the sentences went in when it was let go", held.startswith(first) and held.count(".") == 2 and erasures(states) == 0)
    check("Pinyin again", engines[2] == "libpinyin")
else:
    print("tapped, held: not tried without gsd-media-keys")

if there("composed-clicked"):
    ended, states, clicked, engines = recording("composed-clicked")
    was = [c for t, _, c in states if t < ended and c][-1]
    print(f"clicked: finished with a click on the dictation window, when [{was}] was being composed; it ends as\n  [{clicked}]")
    check("the sentence was committed as it was recognized in the end", clicked.startswith(first) and clicked.count(".") == 2 and clicked.rstrip().endswith(".") and states[-1][2] == "")
    ended, states, cancelled, engines = recording("composed-cancelled")
    was = [c for t, _, c in states if t < ended and c][-1]
    print(f"cancelled: the window's X, when [{was}] was being composed; it ends as\n  [{cancelled}]")
    check("what was being composed is gone, and the sentence that was finished stays", cancelled == first and states[-1][2] == "")
    check("Pinyin again", engines[2] == "libpinyin")
else:
    print("clicked, cancelled: not tried without the interface")

if there("composed-talked"):
    ended, states, talked, engines = recording("composed-talked")
    print(f"talked: the key to talk held for {ended} s; the field changed {len(states) - 1} times")
    check("the words were composed while the key was down, and the text is the same", talked == plain and len(composing(states)) > 5 and erasures(states) == 0 and engines == ["libpinyin", "heyo", "libpinyin"])
else:
    print("talked: not tried with a release build, which does not take xdotool's keys for the keyboard")

ended, states, moved, engines = recording("composed-moved")
was = [c for t, _, c in states if t < ended and c][-1]
print(f"moved: another window came to the front when [{was}] was being composed; the first holds\n  [{moved}]\nand the second\n  [{other('composed-moved')}]")
check("nothing of the sentence is left in the field it was being composed in", moved == BEFORE and states[-1][2] == "")
check("and all of the recording is in the one that came to the front", BEFORE + other("composed-moved") == plain)
check("Pinyin again", engines[2] == "libpinyin")

if there("composed-terminal"):
    ended, states, _, engines = recording("composed-terminal")
    got = states[-1][1].rstrip("\n")
    print(f"terminal: the terminal got\n  [{got}]")
    check("the same words, through the input method", BEFORE + got == plain and engines[1] == "heyo")
else:
    print("terminal: not tried without gnome-terminal")

print(f"the session: IBus at {session['address']}")
check("the clipboard still holds what the user copied, and was never taken from them", session["clipboard"] == session["clipboardAfter"] and json.loads(session["clipboard"])["text"] == "the user's own")
check("the clipboard history has that copy and nothing of what was dictated", json.loads(session["history"]) == ["the user's own"])
check("nothing was pasted in any of these recordings", session["pasted"] == "0")
check(f"the engine is the one it was ({session['engine']}), and so is the Shell's input source ({session['source']})", session["engine"] == session["engineAfter"] == "libpinyin" and session["source"] == session["sourceAfter"] == "'ibus', 'libpinyin'")

ended, states, switched, engines = recording("composed-switched")
after = facts("switched")
print(f"switched: the other input source chosen in the middle of a sentence; the engines before, during and after: {engines}; Heyo said\n  {after['said']}")
check("the same text, the rest of it pasted", switched == plain and "the rest is pasted" in after["said"])
check("the engine and the Shell's input source are the ones the user chose, not Heyo's and not the one before", engines[2] == "xkb:us::eng" and after["sourceAfter"] == "'xkb', 'us'")

ended, states, pasted, engines = recording("composed-plain")
killed = facts("killed")
print(f"plain: a field that takes no input method; Heyo said\n  {killed['refused']}")
check("the same text, by pasting", pasted == plain and not composing(states) and "takes no input method" in killed["refused"])
check("with Heyo's engine in the session's place for a moment only", engines == ["libpinyin", "libpinyin", "libpinyin"])

ended, states, _, engines = recording("composed-killed")
was = [c for t, _, c in states if t < ended and c]
print(f"killed: Heyo killed at {ended} s, when [{was[-1] if was else ''}] was being composed; the engine then: [{killed['left']}]; Pinyin was back {killed['back']} after Heyo was started again")
check("words were being composed", bool(was) and engines[1] == "heyo")
check("what was being composed is gone from the field", states[-1][1] == BEFORE and states[-1][2] == "")
check("the session was left without an engine", killed["left"] == "")
check("and has Pinyin again once Heyo has started, and the Shell's input source with it", killed["engineAfter"] == "libpinyin" and killed["sourceAfter"] == "'ibus', 'libpinyin'" and "was given back" in killed["said"])

ended, states, quiet, engines = recording("composed-quiet")
print(f"quiet: the setting off; the field changed {len(states) - 1} times")
check("each sentence was committed whole, and nothing was composed meanwhile", quiet == plain and not composing(states) and erasures(states) == 0 and engines[1] == "heyo")

ended, states, polished, engines = recording("composed-polished")
whole = (composing(states) or [""])[-1]
print(f"polished: {len(whole)} characters were being composed at the end; it ends as\n  [{polished[:60]!r} … {polished[-40:]!r}]")
check("the whole recording was composed, and none of it was the field's text", BEFORE + whole == plain and all(s == BEFORE for t, s, _ in states[:-1] if s != polished))
check("the polished text is what was committed, in one piece", polished == expected and erasures(states) == 0 and states[-1][2] == "")
after = facts("polished")
check("Pinyin again, and the clipboard never taken", after["engineAfter"] == "libpinyin" and json.loads(after["clipboardAfter"])["text"] == "the user's own")
print("FAILED: " + "; ".join(failed) if failed else "all as it should be")
sys.exit(1 if failed else 0)
PY

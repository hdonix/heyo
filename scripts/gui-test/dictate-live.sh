#!/bin/sh
# Type as you speak, end to end: a few spoken sentences (say.py, in place of
# a microphone) are dictated into another application's text field, which
# is read back all the while. The field holds "Notes: " already, with the
# cursor after it. Six recordings:
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
# The user's vocabulary turns two of the words into Chinese and an emoji, so
# that what is corrected in place is not only ASCII. Uses the speech engine
# installed for the real Heyo (read only); say.py needs Flite (libflite1).
# "clicked" and "cancelled" need the interface (not HEYO_TEST_FRONTEND=none).
. "$(dirname "$0")/env.sh"
PORT=18112
SPOKEN="$T/spoken.wav"
mkdir -p "$XDG_DATA_HOME/heyo"
ln -sfn "$HOME/.local/share/heyo/dictation" "$XDG_DATA_HOME/heyo/dictation"
LENGTH=$(/usr/bin/python3 "$HERE/say.py" "$SPOKEN" \
  "The weather is nice today, so let us walk to the park." \
  "I will bring forty two sandwiches and some coffee." \
  "See you at three thirty.") || exit 1
export HEYO_DICTATION_WAV="$SPOKEN"
unset ANTHROPIC_API_KEY ANTHROPIC_BASE_URL ANTHROPIC_AUTH_TOKEN
rm -f "$T"/live-*.json

# configure LIVE POLISH: the settings of the next Heyo.
configure() {
  python3 - "$XDG_CONFIG_HOME/heyo/settings.json" "$1" "$2" "$PORT" <<'PY'
import json, os, sys
path, live, polish, port = sys.argv[1:5]
settings = json.load(open(path)) if os.path.exists(path) else {}
settings.update(dictationLive=live == "on", dictationPolish=polish, dictationHoldKey="", dictationLanguage="en",
                dictationVocabulary=["sandwiches -> 三明治", "coffee -> 咖啡😀"],
                aiProvider="local", anthropicApiKey="", localModelUrl=f"http://127.0.0.1:{port}/v1", localModelName="stub-model")
json.dump(settings, open(path, "w"))
PY
}
notes() {
  (/usr/bin/python3 "$REPO/scripts/wayland-test/in/window.py" Notes org.test.Notes "Notes: " >/dev/null 2>&1 &); w 2
  xdotool search --name '^Notes$' | tail -1 | xargs xdotool windowactivate; w 1
}
# record NAME [SECONDS WHAT]: one recording into Notes, which is read every
# 20 ms. At SECONDS: "click" on the dictation window (which finishes),
# "cancel" on its X, or any other WHAT typed by hand as it is.
record() {
  python3 - "$REPO/src-tauri/target/$BUILD/heyo" "$T/live-$1.json" "$LENGTH" "${2:-0}" "${3:-}" <<'PY'
import json, subprocess, sys, threading, time
heyo, out, length, at, what = sys.argv[1], sys.argv[2], float(sys.argv[3]), float(sys.argv[4]), sys.argv[5]
field = "/tmp/window-org.test.Notes.txt"
states, stop = [], threading.Event()
def run(*command):
    return subprocess.run(command, capture_output=True, text=True, timeout=15).stdout
def watch():
    start, last = time.time(), None
    while not stop.is_set():
        try:
            text = open(field).read()
        except OSError:
            text = last
        if text != last:
            states.append([round(time.time() - start, 2), text]); last = text
        time.sleep(0.02)
def pill():
    """The dictation window where it is on screen: left, top, or nothing."""
    shown = run("xdotool", "search", "--onlyvisible", "--name", "Heyo Dictation").split()
    if not shown: return None
    info = run("xwininfo", "-id", shown[-1])
    at = lambda name: int(next(l.split()[-1] for l in info.splitlines() if name in l))
    return at("Absolute upper-left X"), at("Absolute upper-left Y"), at("Width"), at("Height")
# The field as it is before every recording: some text of the user's own.
run("xdotool", "key", "ctrl+a", "BackSpace"); run("xdotool", "type", "Notes: "); time.sleep(0.7)
watching = threading.Thread(target=watch); watching.start()
start = time.time()
run(heyo, "--dictate")
if what in ("click", "cancel"):
    time.sleep(at)
    x, y, width, height = pill()
    # The pill is in the middle of its window; its X is at the pill's right end.
    run("xdotool", "mousemove", str(x + width // 2 + (57 if what == "cancel" else -20)), str(y + height // 2))
    ended = round(time.time() - start, 2)
    run("xdotool", "click", "1")
else:
    if what:
        time.sleep(at); run("xdotool", "type", what)
    time.sleep(max(0, length + 1.0 - (time.time() - start)))
    ended = round(time.time() - start, 2)
    run(heyo, "--dictate")
# Until the dictation window has gone: the text is all there before it goes.
time.sleep(1.5)
deadline = time.time() + 40
while pill() and time.time() < deadline:
    time.sleep(0.2)
time.sleep(1.0)
stop.set(); watching.join()
json.dump({"ended": ended, "states": states}, open(out, "w"), ensure_ascii=False)
PY
}

configure off off; "$HERE/up.sh"; notes
record sentences
python3 "$HERE/down.py" > /dev/null

configure on off; "$HERE/up.sh"; notes
record live
record typed 3.2 '#'
if [ "$HEYO_TEST_FRONTEND" != none ]; then
  record clicked 8.6 click
  record cancelled 8.6 cancel
fi
grep "heyo: dictation" "$T/app.log" | tail -4 | cut -c1-160
python3 "$HERE/down.py" > /dev/null

configure on clean; "$HERE/up.sh"; notes
(python3 "$HERE/llm-stub.py" "$PORT" &); w 1
record polished
grep "heyo: dictation" "$T/app.log" | tail -3 | cut -c1-160
python3 "$HERE/down.py" > /dev/null
configure on off

python3 - "$T" <<'PY'
import json, os, sys
T = sys.argv[1]
BEFORE = "Notes: "
def recording(name):
    r = json.load(open(f"{T}/live-{name}.json"))
    return r["ended"], [(t, s) for t, s in r["states"]], r["states"][-1][1]
def erasures(states):
    return sum(1 for (_, a), (_, b) in zip(states, states[1:]) if not b.startswith(a))
failed = []
def check(what, ok):
    print(f"  {'ok  ' if ok else 'FAIL'}  {what}")
    if not ok: failed.append(what)

_, states, plain = recording("sentences")
print(f"sentences: the text changed {len(states) - 1} times; it ends as\n  [{plain}]")
check("each sentence went in whole: nothing was ever erased", erasures(states) == 0)
check("three sentences were heard, with Chinese and an emoji from the vocabulary", plain.count(".") >= 3 and "三明治" in plain and "咖啡😀" in plain)

ended, states, live = recording("live")
during = [(t, s) for t, s in states if t < ended]
first = next((t for t, s in states if len(s) > len(BEFORE)), None)
print(f"live: the text changed {len(states) - 1} times, {erasures(states)} of them taking something back; the first words after {first} s, the recording ended at {ended} s")
for t, s in states[1:6]:
    print(f"    {t:5.2f}  [{s}]")
check("the same text as sentence by sentence", live == plain)
check("words were there before their sentence was finished", any(len(s) > len(BEFORE) and not s.rstrip().endswith(".") for _, s in during))
check("all of the text was there before the recording ended", bool(during) and during[-1][1] == live)
check("it was corrected where it stood (text taken back and written again)", erasures(states) > 0)
check("what stood before the cursor was never touched", all(s.startswith(BEFORE) for _, s in states))

ended, states, typed = recording("typed")
at = next((i for i, (_, s) in enumerate(states) if "#" in s), None)
print(f"typed: a # typed by hand in the middle; it ends as\n  [{typed}]")
check("the character is in the text", at is not None and typed.count("#") == 1)
if at is not None:
    check("nothing was erased after it: every later text begins with the one before", erasures(states[at:]) == 0)
    check("text was still added after it", len(typed) > len(states[at][1]))
    check("every sentence still ended up in the text", typed.replace("#", "").count(".") == plain.count("."))

if os.path.exists(f"{T}/live-clicked.json"):
    ended, states, clicked = recording("clicked")
    before = [s for t, s in states if t < ended][-1]
    print(f"clicked: finished with a click on the dictation window; at the click the text was\n  [{before}]\nand it ends as\n  [{clicked}]")
    check("the click came in the middle of a sentence", not before.rstrip().endswith("."))
    check("the sentence was finished where it stood", clicked.rstrip().endswith(".") and clicked.startswith(plain[:plain.index(".") + 1]) and clicked.count(".") == 2)
    ended, states, cancelled = recording("cancelled")
    before = [s for t, s in states if t < ended][-1]
    print(f"cancelled: the window's X in the middle; at the click the text was\n  [{before}]\nand it ends as\n  [{cancelled}]")
    check("words had been written", len(before) > len(plain) // 3)
    check("and all of them were taken back, and nothing else", cancelled == BEFORE)
else:
    print("clicked, cancelled: not tried without the interface")

ended, states, polished = recording("polished")
print(f"polished: it ends as\n  [{polished[:60]!r} … {polished[-40:]!r}]")
check("what was heard was in the text first", any(s == plain for _, s in states))
check("and was replaced by the polished text", polished.startswith(BEFORE + "You asked: “") and plain[len(BEFORE):] in polished and polished.rstrip().endswith("```") and not polished.startswith(plain))
print("FAILED: " + "; ".join(failed) if failed else "all as it should be")
sys.exit(1 if failed else 0)
PY

#!/bin/sh
# Dictation end to end, fed a silent recording instead of a microphone: the
# progress window comes up when recording starts and leaves when it is over.
# Uses the speech engine installed for the real Heyo (read only).
. "$(dirname "$0")/env.sh"
mkdir -p "$XDG_DATA_HOME/heyo"
speech
python3 -c "
import wave,sys
w=wave.open(sys.argv[1],'wb'); w.setnchannels(1); w.setsampwidth(2); w.setframerate(16000); w.writeframes(b'\0\0'*16000*4); w.close()" "$T/silence.wav"
export HEYO_DICTATION_WAV="$T/silence.wav"
"$HERE/sandbox.sh" up
# The window is made when Heyo starts, and is on screen or not.
state() {
  id=$(xwininfo -root -tree | grep '"Heyo Dictation"' | head -1 | awk '{print $1}')
  [ -n "$id" ] && xwininfo -id "$id" | grep "Map State" | awk '{print $3}' || echo none
}
# Seconds until the window is in the state given, or "never" (8 s).
until_state() {
  python3 - "$1" <<'PY'
import subprocess, sys, time
start = time.time()
while time.time() - start < 8:
    tree = subprocess.run(["xwininfo", "-root", "-tree"], capture_output=True, text=True).stdout
    ids = [l.split()[0] for l in tree.splitlines() if '"Heyo Dictation"' in l]
    now = "none"
    if ids:
        info = subprocess.run(["xwininfo", "-id", ids[0]], capture_output=True, text=True).stdout
        now = next((l.split()[2] for l in info.splitlines() if "Map State" in l), "none")
    if now == sys.argv[1]:
        print(f"{time.time() - start:.1f}s"); break
    time.sleep(0.2)
else:
    print("never")
PY
}
for round in 1 2; do
  heyo --dictate; echo "round $round: window up after $(until_state IsViewable)"
  w 2; heyo --dictate; echo "round $round: window gone after $(until_state IsUnMapped)"
done
grep -v "EGL\|dbind\|heyo: started" "$T/app.log" | grep -v DEBUG | tail -3 | cut -c1-200
"$HERE/sandbox.sh" down > /dev/null

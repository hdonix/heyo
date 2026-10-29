#!/bin/sh
# Inside the test session: starts the Heyo built on the host (hidden, as at
# login). HEYO_BUILD picks the build: release (default) or debug.
BIN=/heyo/src-tauri/target/${HEYO_BUILD:-release}/heyo
export HEYO_RUNTIME=/heyo/runtime/dist/heyo-runtime.cjs
cd /tmp
# The speech engine lent to the session (run.sh), where Heyo looks for it.
if [ -d /speech/models ] && [ ! -e "$HOME/.local/share/heyo/dictation" ]; then
    mkdir -p "$HOME/.local/share/heyo"
    ln -s /speech "$HOME/.local/share/heyo/dictation"
fi
# One Heyo at a time: a second would only hand its arguments to the first.
if [ -f /tmp/heyo.pid ] && kill "$(cat /tmp/heyo.pid)" 2>/dev/null; then
    sleep 1
fi
"$BIN" --hidden "$@" > /tmp/heyo.log 2>&1 &
echo $! > /tmp/heyo.pid
echo "heyo started (pid $!)"

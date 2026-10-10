#!/bin/sh
# heyo dev against a running Heyo: an extension in source form is built and
# listed, its command opens, an edit rebuilds it and the open command shows
# the change, and its console output reaches the terminal running heyo dev.
. "$(dirname "$0")/env.sh"
src="$T/dev-probe"; rm -rf "$src"; cp -r "$HERE/extensions/dev-probe" "$src"
# The extension's own esbuild, as one with its dependencies installed has.
mkdir -p "$src/node_modules"; ln -s "$REPO/node_modules/esbuild" "$src/node_modules/esbuild"
ln -s "$REPO/node_modules/@esbuild" "$src/node_modules/@esbuild" 2>/dev/null
"$HERE/up.sh"
export HEYO_BIN="$REPO/src-tauri/target/debug/heyo"
"$HEYO_BIN" dev "$src" > "$T/dev.log" 2>&1 &
dev=$!; w 6
heyo --open "cmd:$XDG_DATA_HOME/heyo/development/dev-probe:hello"; w 4
python3 "$HERE/grab.py" "$T/shots/dev-first.png" 440 200 1480 620
sed -i 's/first build/second build/' "$src/src/hello.tsx"; w 5
python3 "$HERE/grab.py" "$T/shots/dev-second.png" 440 200 1480 620
echo "--- the terminal running heyo dev:"; cut -c1-160 "$T/dev.log" | tail -8
echo "--- development extensions: $("$HEYO_BIN" dev --list 2>&1 | tail -2 | tr '\n' ' ')"
# The tool is a child of the heyo that started it.
pkill -P "$dev"; kill "$dev" 2>/dev/null
python3 "$HERE/down.py" > /dev/null

#!/bin/sh
# Somewhere for a Heyo built with the updater to find a new version, on
# this computer only: no release server exists (packaging/README.md,
# "Updates"). For clicking About's "Check for Updates" in the sandbox.
#
#   updates.sh config            writes $S/updates/updater.json, the configuration to build with:
#                                  pnpm tauri build --no-bundle --features updater --config $S/updates/updater.json
#                                (a key pair made for the purpose is beside it; it signs nothing real)
#   updates.sh serve VERSION     says VERSION is the newest, at http://127.0.0.1:18765/latest.json
#   updates.sh stop              ends that
#
# Heyo is then started with APPIMAGE=$S/updates/heyo.AppImage: only a copy
# that owns its file offers to update. What is served as the new version is
# a few bytes with a signature that is not theirs, so that installing it is
# refused: nothing is ever replaced or started.
. "$(dirname "$0")/lib.sh"
U=$S/updates
PORT=18765
mkdir -p "$U/www"
case "${1:-}" in
  config)
    [ -f "$U/test.key.pub" ] || (cd "$W" && pnpm tauri signer generate --ci -p "" -w "$U/test.key" > "$U/key.log" 2>&1)
    python3 -I - "$U/test.key.pub" "$PORT" > "$U/updater.json" <<'PY'
import json, sys
print(json.dumps({"plugins": {"updater": {"endpoints": [f"http://127.0.0.1:{sys.argv[2]}/latest.json"], "pubkey": open(sys.argv[1]).read().strip(), "dangerousInsecureTransportProtocol": True}}}, indent=2))
PY
    echo "$U/updater.json"
    ;;
  serve)
    alive updates && finish updates
    printf 'not a program\n' > "$U/www/heyo.AppImage"
    # A signature in the right form, of something else.
    printf 'something else\n' > "$U/other"
    (cd "$W" && pnpm tauri signer sign -f "$U/test.key" -p "" "$U/other" > "$U/sign.log" 2>&1)
    python3 -I - "$2" "$PORT" "$U/other.sig" > "$U/www/latest.json" <<'PY'
import json, sys
version, port, signature = sys.argv[1:4]
there = {"signature": open(signature).read().strip(), "url": f"http://127.0.0.1:{port}/heyo.AppImage"}
print(json.dumps({"version": version, "notes": "", "pub_date": "2026-10-12T00:00:00Z", "platforms": {"linux-aarch64": there, "linux-x86_64": there}}))
PY
    python3 -I -m http.server "$PORT" --bind 127.0.0.1 --directory "$U/www" > "$U/server.log" 2>&1 &
    note updates $!
    sleep 0.5
    echo "version $2 at http://127.0.0.1:$PORT/latest.json; start Heyo with APPIMAGE=$U/heyo.AppImage"
    ;;
  stop)
    finish updates
    ;;
  *)
    sed -n '2,15p' "$0" | sed 's/^# \{0,1\}//'
    exit 2
    ;;
esac

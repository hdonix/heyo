#!/bin/sh
# Builds Heyo's Flutter interface (ui/): the `heyo-ui` program and what it
# needs beside it, in ui/build/linux/<arch>/release/bundle/. A debug build
# of Heyo looks for it there; scripts/install.sh installs it from there.
#
#   scripts/build-ui.sh          build
#   scripts/build-ui.sh --test   analyze and run its tests first
#
# Needs Flutter (FLUTTER=/path/to/flutter if it is not on PATH). Heyo
# itself builds and runs without any of this: the web view is its default
# interface, and the only one unless `--ui=flutter` asks for this one.
set -eu
REPO=$(cd "$(dirname "$0")/.." && pwd)
FLUTTER=${FLUTTER:-flutter}
if ! command -v "$FLUTTER" >/dev/null 2>&1; then
  echo "flutter is not on PATH (set FLUTTER=/path/to/flutter): https://docs.flutter.dev/get-started/install/linux" >&2
  exit 1
fi
cd "$REPO/ui"
"$FLUTTER" pub get
if [ "${1:-}" = --test ]; then
  "$FLUTTER" analyze
  "$FLUTTER" test
fi
"$FLUTTER" build linux --release
ARCH=$(uname -m | sed 's/aarch64/arm64/; s/x86_64/x64/')
echo "built $REPO/ui/build/linux/$ARCH/release/bundle/heyo-ui ($("$REPO/ui/build/linux/$ARCH/release/bundle/heyo-ui" --version))"

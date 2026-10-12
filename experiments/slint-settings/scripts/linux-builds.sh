#!/bin/sh
# Heyo's release build for this machine three times over, to weigh what the
# Slint settings add and to measure each: without the feature, with Slint
# and its own renderer (slint-settings), and with Skia as well (slint-skia).
# Each is kept as src-tauri/target/variants/heyo-<plain|slint|skia>, and
# their sizes are written to $RESULTS/sizes.tsv.
#
#   linux-builds.sh [plain] [slint] [skia]     all three when none is named
set -eu
W=$(cd "$(dirname "$0")/../../.." && pwd)
RESULTS=$W/experiments/slint-settings/results
[ $# -gt 0 ] || set -- plain slint skia
mkdir -p "$W/src-tauri/target/variants" "$RESULTS"
cd "$W"
for variant in "$@"; do
  case $variant in
    plain) pnpm tauri build --no-bundle ;;
    slint) pnpm tauri build --no-bundle --features slint-settings ;;
    skia) pnpm tauri build --no-bundle --features slint-skia ;;
    *) echo "$0: no variant $variant" >&2; exit 2 ;;
  esac
  cp src-tauri/target/release/heyo "src-tauri/target/variants/heyo-$variant"
done
{
  printf 'build\tbytes\tfeatures\n'
  for variant in plain slint skia; do
    [ -f "src-tauri/target/variants/heyo-$variant" ] && printf '%s\t%s\t%s\n' "$(uname -m)-linux-gnu $variant" "$(stat -c %s "src-tauri/target/variants/heyo-$variant")" "$(case $variant in plain) echo none ;; slint) echo slint-settings ;; skia) echo slint-skia ;; esac)"
  done
} > "$RESULTS/sizes.tsv"
cat "$RESULTS/sizes.tsv"

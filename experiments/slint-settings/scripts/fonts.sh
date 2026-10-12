#!/bin/sh
# Tries the fonts Heyo could carry for Chinese, one after another: each is
# registered with Slint as Heyo registers its own (put first, the system's
# behind), a window with Chinese in it is drawn by Skia on the CPU, and what
# that took is written down: how long the file took to register, how long
# the first frame took, and how much memory the process then had.
#
#   fonts.sh DIRECTORY     the fonts as fetch-fonts fetched them (inter-var/, notosc-…/)
#
# The window is `slint-overlay showcase` of experiments/overlay-bench/frameworks/slint
# (built with --features skia), which reads the fonts from their files:
# the memory it reports therefore has the whole file in it, which a font
# inside the program does not (see results/fonts.md). In the sandbox only.
. "$(dirname "$0")/lib.sh"
FONTS=$1
BIN=$OB/frameworks/slint/target/release/slint-overlay
PICTURE=${PICTURE:?name the picture the showcase shows (overlay-bench makes one: picture-2x.bgrx)}
OUT=$RESULTS/fonts
mkdir -p "$OUT" "$S/subsets"
INTER="$FONTS/inter-var/Inter[opsz,wght].ttf"
# The subsets, made once.
[ -f "$S/subsets/NotoSansSC-VF-gb2312.otf" ] || python3 -I "$HERE/subset.py" "$FONTS/notosc-var-otf/NotoSansSC-VF.otf" "$S/subsets/NotoSansSC-VF-gb2312.otf"
[ -f "$S/subsets/NotoSansSC-Regular-gb2312.otf" ] || python3 -I "$HERE/subset.py" "$FONTS/notosc-subset-regular/NotoSansSC-Regular.otf" "$S/subsets/NotoSansSC-Regular-gb2312.otf"

try() { # name fonts
  name=$1; fonts=$2
  log=$S/fonts-$name.log
  if [ -n "$fonts" ]; then
    "$BIN" showcase --picture "$PICTURE" --renderer slint-skia-software --seconds 5 --shot "$OUT/$name.png" --fonts "$fonts" > "$log" 2>&1 &
  else
    "$BIN" showcase --picture "$PICTURE" --renderer slint-skia-software --seconds 5 --shot "$OUT/$name.png" > "$log" 2>&1 &
  fi
  pid=$!; note showcase $pid
  sleep 3
  memory=$(awk '/^VmRSS|^RssAnon|^RssFile/ {printf "%s %d kB  ", $1, $2}' "/proc/$pid/status" 2>/dev/null)
  for _ in $(seq 60); do alive showcase || break; sleep 0.1; done
  finish showcase > /dev/null
  redraw=$(awk '/^first-redraw/ {print $3}' "$log"); frame=$(awk '/^first-frame/ {print $3}' "$log")
  echo "## $name"
  grep '^font ' "$log" | sed 's|/.*/||'
  echo "first frame drawn in $(awk -v a="$redraw" -v b="$frame" 'BEGIN { printf "%.1f", b - a }') ms; memory after it: $memory"
}

{
  echo "# The fonts tried, $(date +%F), Slint 1.18.1 with Skia on the CPU, $(uname -m)"
  for round in 1 2; do
    echo; echo "# round $round"
    try system-only ""
    try inter-only "$INTER"
    try variable-otf "$INTER:$FONTS/notosc-var-otf/NotoSansSC-VF.otf"
    try variable-ttf "$INTER:$FONTS/notosc-var-ttf/NotoSansSC-VF.ttf"
    try static-regular-otf "$INTER:$FONTS/notosc-subset-regular/NotoSansSC-Regular.otf"
    try static-three-otf "$INTER:$FONTS/notosc-subset-regular/NotoSansSC-Regular.otf:$FONTS/notosc-subset-medium/NotoSansSC-Medium.otf:$FONTS/notosc-subset-bold/NotoSansSC-Bold.otf"
    try subset-variable-otf "$INTER:$S/subsets/NotoSansSC-VF-gb2312.otf"
    try subset-regular-otf "$INTER:$S/subsets/NotoSansSC-Regular-gb2312.otf"
  done
  echo; echo "# sizes"
  ls -l "$INTER" "$FONTS"/notosc-*/*.otf "$FONTS"/notosc-*/*.ttf "$S"/subsets/*.otf | awk '{n = split($NF, parts, "/"); printf "%10d  %s\n", $5, parts[n]}'
} | tee "$OUT/measured.txt"

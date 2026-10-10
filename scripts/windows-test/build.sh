#!/bin/sh
# Builds Heyo for Windows from Linux and puts what it needs to run in
# src-tauri/target/heyo-windows/, which the test machine sees as
# \\host.lan\Data\src-tauri\target\heyo-windows.
#
# Needs the Rust target (rustup target add aarch64-pc-windows-gnullvm, or
# x86_64-…) and llvm-mingw (https://github.com/mstorsjo/llvm-mingw) unpacked
# in $LLVM_MINGW (default ~/.local/share/llvm-mingw).
#
# INSTALLER=1 also makes the installer (Heyo_<version>_<arch>-setup.exe, in
# the same directory), which needs NSIS's makensis on PATH: see
# packaging/README.md.
set -eu
HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$HERE/../.." && pwd)
ARCH=${ARCH:-aarch64}
TARGET=$ARCH-pc-windows-gnullvm
MINGW=${LLVM_MINGW:-$HOME/.local/share/llvm-mingw}
VAR=$(echo "$TARGET" | tr - _)
UPPER=$(echo "$VAR" | tr a-z A-Z)

export PATH="$MINGW/bin:$PATH"
export "CARGO_TARGET_${UPPER}_LINKER=$ARCH-w64-mingw32-clang"
export "CC_$VAR=$ARCH-w64-mingw32-clang" "CXX_$VAR=$ARCH-w64-mingw32-clang++" "AR_$VAR=llvm-ar"

cd "$REPO"
OUT=src-tauri/target/heyo-windows
mkdir -p "$OUT"

# Heyo's commands and extensions run in Node.js; one beside heyo.exe is used
# without anything having to be installed.
NODE=${NODE_VERSION:-$(node --version)}
case "$ARCH" in aarch64) NODE_ARCH=arm64 ;; *) NODE_ARCH=x64 ;; esac
CACHE=${XDG_CACHE_HOME:-$HOME/.cache}/heyo-build
if [ ! -f "$CACHE/node-$NODE-$NODE_ARCH.exe" ]; then
  mkdir -p "$CACHE"
  curl -fsSL -o "$CACHE/node-$NODE-$NODE_ARCH.exe.part" "https://nodejs.org/dist/$NODE/win-$NODE_ARCH/node.exe"
  mv "$CACHE/node-$NODE-$NODE_ARCH.exe.part" "$CACHE/node-$NODE-$NODE_ARCH.exe"
fi
[ -f "$OUT/node.exe" ] || cp "$CACHE/node-$NODE-$NODE_ARCH.exe" "$OUT/node.exe"
# Replaced rather than written over: the machine may be running the last build.
cp --remove-destination "$MINGW/$ARCH-w64-mingw32/bin/libunwind.dll" "$OUT/"

pnpm tauri build --no-bundle --target "$TARGET"
for file in "src-tauri/target/$TARGET/release/heyo.exe" "src-tauri/target/$TARGET/release/WebView2Loader.dll" runtime/dist/heyo-*.cjs; do
  cp --remove-destination "$file" "$OUT/"
done
echo "built $OUT/heyo.exe"

if [ -n "${INSTALLER:-}" ]; then
  # Tauri puts the program and the scripts in the installer; the three other
  # files heyo.exe needs beside it are added from $OUT, where they are only
  # now. That changes the configuration, so the program is built once more.
  pnpm tauri build --bundles nsis --target "$TARGET" --config packaging/windows/installer.conf.json
  cp --remove-destination "src-tauri/target/$TARGET/release/bundle/nsis/"*-setup.exe "$OUT/"
  echo "built $(ls "$OUT"/*-setup.exe)"
fi

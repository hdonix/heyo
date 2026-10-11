#!/bin/sh
# Builds Heyo for Windows from Linux and puts what it needs to run in
# src-tauri/target/heyo-windows/, which the test machine sees as
# \\host.lan\Data\src-tauri\target\heyo-windows.
#
# The building and the packaging are scripts/package/'s, as for a release:
# see build.sh there for what it needs (the Rust target and llvm-mingw).
#
# INSTALLER=1 also makes the installer (Heyo_<version>_<arch>-setup.exe, in
# the same directory), which needs NSIS's makensis on PATH: see
# packaging/README.md.
set -eu
HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$HERE/../.." && pwd)
ARCH=${ARCH:-aarch64}
TARGET=$ARCH-pc-windows-gnullvm
case "$ARCH" in aarch64) NODE_ARCH=arm64 ;; *) NODE_ARCH=x64 ;; esac

cd "$REPO"
OUT=src-tauri/target/heyo-windows
RELEASE=src-tauri/target/$TARGET/release
mkdir -p "$OUT"

scripts/package/build.sh "$TARGET"
# Heyo's commands and extensions run in Node.js; one beside heyo.exe is used
# without anything having to be installed.
node scripts/package/node.mjs --platform win32 --arch "$NODE_ARCH" --out "$OUT"
# Replaced rather than written over: the machine may be running the last build.
for file in "$RELEASE/heyo.exe" "$RELEASE/WebView2Loader.dll" "$RELEASE/libunwind.dll" runtime/dist/heyo-*.cjs; do
  cp --remove-destination "$file" "$OUT/"
done
echo "built $OUT/heyo.exe"

if [ -n "${INSTALLER:-}" ]; then
  scripts/package/bundle.sh "$TARGET"
  cp --remove-destination "$RELEASE/bundle/nsis/"*-setup.exe "$OUT/"
  echo "built $(ls "$OUT"/*-setup.exe)"
fi

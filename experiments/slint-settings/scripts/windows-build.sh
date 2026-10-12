#!/bin/sh
# Heyo for Windows, cross-compiled on this machine: <arch>-pc-windows-msvc,
# with Microsoft's C runtime and SDK as
# experiments/overlay-bench/frameworks/slint/scripts/msvc-xwin.sh put them in
# ~/.cache/heyo-build/xwin (it asks for their licence to be accepted; this
# script only uses what is there).
#
#   windows-build.sh [aarch64|x86_64]          the program, with its interface
#   windows-build.sh check                     `cargo check` for both, no more
#
# The build has a target directory of its own (src-tauri/target/msvc-slint),
# so that it does not wait on, or hold up, a build for this machine.
set -eu
ARCH=${1:-aarch64}
W=$(cd "$(dirname "$0")/../../.." && pwd)
XWIN=$HOME/.cache/heyo-build/xwin
[ -d "$XWIN/splat/crt" ] || { echo "$0: no $XWIN/splat: run msvc-xwin.sh first" >&2; exit 1; }
export PATH="$XWIN/tools:$PATH"
export CARGO_TARGET_DIR="$W/src-tauri/target/msvc-slint"
# The compiler, the linker and where Microsoft's files are, for one target.
toolchain() {
  TARGET=$1-pc-windows-msvc
  VAR=$(echo "$TARGET" | tr - _)
  UPPER=$(echo "$VAR" | tr a-z A-Z)
  INCLUDES="/imsvc$XWIN/splat/crt/include /imsvc$XWIN/splat/sdk/include/ucrt /imsvc$XWIN/splat/sdk/include/um /imsvc$XWIN/splat/sdk/include/shared"
  export "CC_$VAR=clang-cl" "CXX_$VAR=clang-cl" "AR_$VAR=llvm-lib"
  export "CFLAGS_$VAR=--target=$TARGET -Wno-unused-command-line-argument -fuse-ld=lld-link $INCLUDES"
  export "CXXFLAGS_$VAR=--target=$TARGET -Wno-unused-command-line-argument -fuse-ld=lld-link $INCLUDES"
  export "CARGO_TARGET_${UPPER}_LINKER=lld-link"
  export "CARGO_TARGET_${UPPER}_RUSTFLAGS=-Lnative=$XWIN/splat/crt/lib/$1 -Lnative=$XWIN/splat/sdk/lib/um/$1 -Lnative=$XWIN/splat/sdk/lib/ucrt/$1"
}
if [ "$ARCH" = check ]; then
  cd "$W/src-tauri"
  for arch in aarch64 x86_64; do
    toolchain "$arch"
    cargo check --target "$TARGET"
  done
  exit 0
fi
toolchain "$ARCH"
cd "$W"
pnpm tauri build --no-bundle --target "$TARGET"
ls -la "$CARGO_TARGET_DIR/$TARGET/release/heyo.exe"

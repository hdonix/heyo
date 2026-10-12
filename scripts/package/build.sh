#!/bin/sh
# Compiles Heyo for each target named, from this machine, without making a
# package of it (bundle.sh does that, and need not run where this did):
#
#   scripts/package/build.sh x86_64-unknown-linux-gnu aarch64-pc-windows-gnullvm …
#
# All in one target directory, so that what is compiled for the building
# machine itself (build scripts, macros) is compiled once for all of them.
# The program is left in src-tauri/target/<target>/release/.
#
# Needs each target's standard library (rustup target add <target>) and:
#
#   *-pc-windows-gnullvm   llvm-mingw in $LLVM_MINGW (toolchain.sh fetches it)
#   *-unknown-linux-gnu    for another processor than this machine's: that
#                          architecture's libraries in a sysroot (sysroot.sh
#                          lays one out), and llvm-mingw again, whose clang
#                          compiles and links for any of them
#
# FEATURES names cargo features to build with, for trying what is behind
# one (FEATURES=native-overlay). A package is built with none.
set -eu
REPO=$(cd "$(dirname "$0")/../.." && pwd)
cd "$REPO"
[ $# -gt 0 ] || { echo "usage: $0 <target>…" >&2; exit 2; }
HOST=$(rustc -vV | sed -n 's/^host: //p')
MINGW=${LLVM_MINGW:-$HOME/.local/share/llvm-mingw}

# The interface and the runtime's scripts are the same for every target.
pnpm build

for TARGET in "$@"; do
  ARCH=${TARGET%%-*}
  VAR=$(echo "$TARGET" | tr - _)
  UPPER=$(echo "$VAR" | tr a-z A-Z)
  (
    case "$TARGET" in
      *-pc-windows-gnullvm)
        export PATH="$MINGW/bin:$PATH"
        export "CARGO_TARGET_${UPPER}_LINKER=$ARCH-w64-mingw32-clang"
        export "CC_$VAR=$ARCH-w64-mingw32-clang" "CXX_$VAR=$ARCH-w64-mingw32-clang++" "AR_$VAR=llvm-ar"
        ;;
      *-unknown-linux-gnu)
        if [ "$ARCH" != "${HOST%%-*}" ]; then
          case "$ARCH" in aarch64) DEBIAN=arm64 ;; x86_64) DEBIAN=amd64 ;; esac
          GNU=$ARCH-linux-gnu
          SYSROOT=${HEYO_SYSROOTS:-${XDG_CACHE_HOME:-$HOME/.cache}/heyo-build/sysroots}/$DEBIAN
          [ -f "$SYSROOT/.complete" ] || { echo "$0: no sysroot for $DEBIAN: run scripts/package/sysroot.sh $DEBIAN" >&2; exit 1; }
          # cargo and the C compiler's callers want a program's name, not a
          # command line: clang, told the target and where its libraries are.
          TOOLS=$REPO/src-tauri/target/cross/$GNU
          mkdir -p "$TOOLS"
          # (rustc reads from the linker's name how to speak to it: one
          # called ld would be given a linker's own options, not a compiler's.)
          for tool in cc:clang c++:clang++ link-cc:clang; do
            {
              echo '#!/bin/sh'
              echo "exec \"$MINGW/bin/${tool#*:}\" --target=$GNU --sysroot=\"$SYSROOT\" $([ "${tool%%:*}" = link-cc ] && echo -fuse-ld=lld) \"\$@\""
            } > "$TOOLS/${tool%%:*}"
            chmod +x "$TOOLS/${tool%%:*}"
          done
          export "CARGO_TARGET_${UPPER}_LINKER=$TOOLS/link-cc"
          export "CC_$VAR=$TOOLS/cc" "CXX_$VAR=$TOOLS/c++" "AR_$VAR=$MINGW/bin/llvm-ar"
          export "PKG_CONFIG_SYSROOT_DIR_$VAR=$SYSROOT" "PKG_CONFIG_LIBDIR_$VAR=$SYSROOT/usr/lib/$GNU/pkgconfig:$SYSROOT/usr/share/pkgconfig"
        fi
        ;;
      *) echo "$0: no way to build for $TARGET is written here" >&2; exit 2 ;;
    esac
    pnpm tauri build --no-bundle --target "$TARGET" ${FEATURES:+--features "$FEATURES"} --config '{"build":{"beforeBuildCommand":""}}'
  )
  case "$TARGET" in
    # What heyo.exe built this way needs beside it, besides WebView2Loader.dll
    # which the build leaves there. Replaced rather than written over: a
    # test machine may be running the last build.
    *-pc-windows-gnullvm) cp --remove-destination "$MINGW/$ARCH-w64-mingw32/bin/libunwind.dll" "src-tauri/target/$TARGET/release/" ;;
  esac
done

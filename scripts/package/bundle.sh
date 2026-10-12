#!/bin/sh
# Makes the packages of Heyo for one target, from the program build.sh
# left in src-tauri/target/<target>/release/ and the runtime's scripts in
# runtime/dist/. Nothing is compiled: it can run on another machine than
# the one that built, given those files.
#
#   scripts/package/bundle.sh <target> [bundles]
#
#   *-unknown-linux-gnu    deb,rpm,appimage (the default), or some of them.
#                          An AppImage is made of the libraries of the
#                          machine this runs on: it has to be of the
#                          target's processor.
#   *-pc-windows-msvc      the installer; needs makensis of NSIS 3.09 or newer
#                          on PATH (Ubuntu 24.04's; 22.04's is too old),
#                          llvm-objdump (llvm-mingw's, or one on PATH) to
#                          read which DLLs the program asks for, and,
#                          Tauri's packager being run on Linux, the tray
#                          library it looks for whatever it packages for:
#                          libayatana-appindicator3-dev.
#
# The packages are in src-tauri/target/<target>/release/bundle/, and
# copied together to src-tauri/target/packages/.
set -eu
REPO=$(cd "$(dirname "$0")/../.." && pwd)
cd "$REPO"
TARGET=${1:?usage: $0 <target> [bundles]}
case "${TARGET%%-*}" in aarch64) NODE_ARCH=arm64 ;; x86_64) NODE_ARCH=x64 ;; *) echo "$0: $TARGET?" >&2; exit 2 ;; esac
RELEASE=src-tauri/target/$TARGET/release

case "$TARGET" in
  *-pc-windows-msvc)
    # Tauri packs the program and its scripts; what else heyo.exe needs
    # beside it is gathered in one directory. There is no package manager
    # to bring a Node.js: the installer carries one, which
    # packaging/windows/installer.conf.json names.
    BESIDE=src-tauri/target/heyo-windows
    mkdir -p "$BESIDE"
    node scripts/package/node.mjs --platform win32 --arch "$NODE_ARCH" --out "$BESIDE"
    # And the DLLs this heyo.exe asks for that Windows has not got, which
    # depends on what was compiled into it: named here, once they are known.
    ASKED=$(scripts/package/dlls.sh "$RELEASE/heyo.exe" "$BESIDE")
    DLLS=
    for dll in $ASKED; do
      DLLS="$DLLS${DLLS:+,}\"target/heyo-windows/$dll\":\"$dll\""
    done
    pnpm tauri bundle --bundles nsis --target "$TARGET" --config packaging/windows/installer.conf.json --config "{\"bundle\":{\"resources\":{$DLLS}}}"
    ;;
  *-unknown-linux-gnu)
    BUNDLES=${2:-deb,rpm,appimage}
    # The AppImage is meant to stand alone, so it carries a Node.js; the
    # deb and the rpm leave that to the distribution.
    node scripts/package/node.mjs --platform linux --arch "$NODE_ARCH" --out src-tauri/target/bundled-node
    pnpm tauri bundle --bundles "$BUNDLES" --target "$TARGET" --config packaging/linux/appimage.conf.json
    ;;
  *) echo "$0: no way to package $TARGET is written here" >&2; exit 2 ;;
esac

PACKAGES=src-tauri/target/packages
mkdir -p "$PACKAGES"
find "$RELEASE/bundle" -maxdepth 2 -type f \( -name '*.deb' -o -name '*.rpm' -o -name '*.AppImage' -o -name '*-setup.exe' \) -exec cp {} "$PACKAGES/" \;
ls -l "$PACKAGES"

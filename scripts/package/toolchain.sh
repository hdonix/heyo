#!/bin/sh
# Fetches llvm-mingw into $LLVM_MINGW (default ~/.local/share/llvm-mingw)
# unless it is there: a clang and an lld that compile and link for other
# systems and processors than this machine's, built to run on Ubuntu 22.04
# and newer. build.sh compiles Heyo with them for Windows (with Microsoft's
# headers and libraries, which winsdk.sh fetches) and for Linux on the
# other processor (with that one's libraries, which sysroot.sh lays out).
# The release and what its archive must hash to are written here; an
# archive that hashes differently is not unpacked.
set -eu
RELEASE=20261006
case "$(uname -m)" in
  x86_64) MACHINE=x86_64 SHA256=5f9c6ed95b2d4bdb2869a488c5fd5857fbdabcf288a0aa3eb1da43f6a08d8ab4 ;;
  aarch64) MACHINE=aarch64 SHA256=e77c4acfa44184f6b0c52b814d724ad63b2ecebb3c3f6f842c0846e717cbd23c ;;
  *) echo "no llvm-mingw is pinned for $(uname -m)" >&2; exit 1 ;;
esac
MINGW=${LLVM_MINGW:-$HOME/.local/share/llvm-mingw}
if [ -x "$MINGW/bin/clang" ] && [ -x "$MINGW/bin/lld-link" ]; then
  echo "llvm-mingw is in $MINGW"
  exit 0
fi
NAME=llvm-mingw-$RELEASE-ucrt-ubuntu-22.04-$MACHINE
ARCHIVE=$(mktemp)
trap 'rm -f "$ARCHIVE"' EXIT
curl -fsSL -o "$ARCHIVE" "https://github.com/mstorsjo/llvm-mingw/releases/download/$RELEASE/$NAME.tar.xz"
echo "$SHA256  $ARCHIVE" | sha256sum -c - >/dev/null || { echo "$NAME.tar.xz does not hash to what scripts/package/toolchain.sh says" >&2; exit 1; }
mkdir -p "$MINGW"
tar -xJf "$ARCHIVE" -C "$MINGW" --strip-components=1
echo "llvm-mingw $RELEASE is in $MINGW"

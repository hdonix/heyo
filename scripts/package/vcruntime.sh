#!/bin/sh
# Fetches the DLLs of Microsoft's Visual C++ runtime that a Heyo built with
# Skia (`--features slint-skia`, docs/slint-settings.md) needs beside
# heyo.exe, and writes them to a directory:
#
#   scripts/package/vcruntime.sh aarch64|x86_64 OUT-DIR
#
# Skia comes prebuilt against the C++ library as a DLL (MSVCP140.dll), and
# a Windows that never had the Visual C++ redistributable installed has
# not got it: heyo.exe would not start there. With the DLLs in its own
# directory it does. A Heyo built without Skia needs none of this.
#
# They are taken from the package of Visual Studio's own that holds them
# (the "CRT.Redist" payload, a zip), at the release and by the hashes
# written here: an archive or a DLL that hashes differently is not used.
# Which DLLs: what heyo.exe imports and what those import in turn
# (`llvm-objdump -p` says); on x86_64 msvcp140.dll wants vcruntime140_1.dll
# as well.
#
# These are files Microsoft names as redistributable with a program built
# with Visual Studio's tools, under the licence terms of Visual Studio
# ("Distributable Code": https://learn.microsoft.com/visualstudio/releases/2022/redistribution
# lists the files of Microsoft.VC143.CRT). Whoever distributes Heyo with
# them distributes them under those terms.
set -eu
[ $# -eq 2 ] || { sed -n '2,7p' "$0" | sed 's/^# \{0,1\}//'; exit 2; }
ARCH=$1; OUT=$2
BASE=https://download.visualstudio.microsoft.com/download/pr/45d3b8dd-bced-4b37-9974-142f748d710c
VERSION=14.44.35112
case "$ARCH" in
  aarch64)
    PACKAGE=Microsoft.VC.14.44.17.14.CRT.Redist.ARM64.base.vsix
    SHA256=fc8f37250188509818258582758a3b704810c77b6d25845486d583ea99d9aa1f
    INSIDE=Contents/VC/Redist/MSVC/$VERSION/arm64/Microsoft.VC143.CRT
    DLLS="msvcp140.dll:045faeab0b5710816ae160ea20dae8495ffc11bb51feca15d22aaf7ff3752cb3 vcruntime140.dll:d8a8513921544569837e400d37cc71302819967ae6defa932266a7ecb41dbaf9"
    ;;
  x86_64)
    PACKAGE=Microsoft.VC.14.44.17.14.CRT.Redist.X64.base.vsix
    SHA256=4aaf54db0bfc9435f7c3660e1a00237a4b556042bfeea64bde44c2e0194e6ee5
    INSIDE=Contents/VC/Redist/MSVC/$VERSION/x64/Microsoft.VC143.CRT
    DLLS="msvcp140.dll:0f885b509a685d2bbfa652fed26b5fb31d88fbdab0a978c641d1c7b8aa460aa9 vcruntime140.dll:d5e4d9a3e835fa679450145d6a7d94e36573a509317111904d9b3712c30d9066 vcruntime140_1.dll:1f2d41c4aa5db0bc33ebf7b66d72943a817d7ce6cbe880502a9403823633093f"
    ;;
  *) echo "no Visual C++ runtime is pinned for $ARCH" >&2; exit 1 ;;
esac
ARCHIVE=$(mktemp)
UNPACKED=$(mktemp -d)
trap 'rm -rf "$ARCHIVE" "$UNPACKED"' EXIT
curl -fsSL -o "$ARCHIVE" "$BASE/$SHA256/$PACKAGE"
echo "$SHA256  $ARCHIVE" | sha256sum -c - >/dev/null || { echo "$PACKAGE does not hash to what scripts/package/vcruntime.sh says" >&2; exit 1; }
mkdir -p "$OUT"
for entry in $DLLS; do
  dll=${entry%%:*}; hash=${entry#*:}
  # The archive is a zip. Only the one file is taken out of it, by name.
  python3 -I -c 'import sys, zipfile; open(sys.argv[3], "wb").write(zipfile.ZipFile(sys.argv[1]).read(sys.argv[2]))' "$ARCHIVE" "$INSIDE/$dll" "$UNPACKED/$dll"
  echo "$hash  $UNPACKED/$dll" | sha256sum -c - >/dev/null || { echo "$dll in $PACKAGE does not hash to what scripts/package/vcruntime.sh says" >&2; exit 1; }
  cp "$UNPACKED/$dll" "$OUT/$dll"
done
echo "Visual C++ runtime $VERSION for $ARCH is in $OUT: $(echo "$DLLS" | sed 's/:[0-9a-f]*//g')"

#!/bin/sh
# Puts what the Windows test machine needs to try the Slint settings in one
# directory: the build of windows-build.sh, the test (../windows/), the
# settings it starts from, and Microsoft's C++ runtime where the build
# needs it (scripts/package/vcruntime.sh).
#
#   windows-pack.sh DIRECTORY [aarch64|x86_64]
#
# The machine sees the main checkout's src-tauri/target as
# \\host.lan\Data\src-tauri\target, so DIRECTORY is made there, and must be
# a new one: Windows does not see, for minutes, files added to a directory
# it has listed already. In the machine: \\host.lan\Data\src-tauri\target\DIRECTORY\test.bat
set -eu
[ $# -ge 1 ] || { sed -n '2,11p' "$0" | sed 's/^# \{0,1\}//'; exit 2; }
ARCH=${2:-aarch64}
HERE=$(cd "$(dirname "$0")" && pwd)
W=$(cd "$HERE/../../.." && pwd)
# The main checkout, of which this may be a worktree.
MAIN=$(cd "$(git -C "$W" rev-parse --git-common-dir)/.." && pwd)
TO=$MAIN/src-tauri/target/$1
[ -e "$TO" ] && { echo "$0: $TO is there already: name a new one" >&2; exit 1; }
EXE=$W/src-tauri/target/msvc-slint/$ARCH-pc-windows-msvc/release/heyo.exe
[ -f "$EXE" ] || { echo "$0: no $EXE: run windows-build.sh $ARCH" >&2; exit 1; }
mkdir -p "$TO/out"
cp "$EXE" "$HERE/../windows/test.bat" "$HERE/../windows/test.ps1" "$TO/"
python3 -I "$HERE/seed.py" "$TO/settings.json"
# A build with Skia asks for Microsoft's C++ runtime as a DLL, which a
# Windows without the redistributable has not: it goes beside heyo.exe.
if grep -aqi 'MSVCP140\.dll' "$EXE"; then
  "$W/scripts/package/vcruntime.sh" "$ARCH" "$TO"
fi
ls -la "$TO"
printf 'in the machine: %s\n' "\\\\host.lan\\Data\\src-tauri\\target\\$1\\test.bat"

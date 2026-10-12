#!/bin/sh
# Puts Heyo's installer where the test machine sees it, with the test that
# tries it there (in/installer/: install, start, ask for the settings,
# uninstall, and what was seen written down):
#
#   scripts/windows-test/installer.sh DIRECTORY        ARCH=x86_64 for Intel/AMD
#
# The installer is the one scripts/package/bundle.sh made for that
# processor (INSTALLER=1 scripts/windows-test/build.sh builds and makes
# it). DIRECTORY is made in the src-tauri/target of the main checkout,
# which the machine sees as \\host.lan\Data\src-tauri\target, and must be a
# new one each time: Windows does not see, for minutes, a file added to a
# directory it has listed already. In the machine, run test.bat there; what
# it saw is in DIRECTORY/out once out/done.txt is.
set -eu
[ $# -eq 1 ] || { sed -n '2,6p' "$0" | sed 's/^# \{0,1\}//'; exit 2; }
HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$HERE/../.." && pwd)
ARCH=${ARCH:-aarch64}
# The main checkout, of which this may be a worktree.
MAIN=$(cd "$(git -C "$REPO" rev-parse --git-common-dir)/.." && pwd)
TO=$MAIN/src-tauri/target/$1
[ ! -e "$TO" ] || { echo "$0: $TO is there already: name a new one" >&2; exit 1; }
SETUP=$(ls "$REPO/src-tauri/target/$ARCH-pc-windows-msvc/release/bundle/nsis/"*-setup.exe 2>/dev/null) || true
[ -f "$SETUP" ] || { echo "$0: one installer is looked for in src-tauri/target/$ARCH-pc-windows-msvc/release/bundle/nsis/: run scripts/package/bundle.sh $ARCH-pc-windows-msvc" >&2; exit 1; }
mkdir -p "$TO/out"
cp "$SETUP" "$HERE/in/installer/test.bat" "$HERE/in/installer/test.ps1" "$TO/"
ls -l "$TO"
printf 'in the machine: %s\n' "\\\\host.lan\\Data\\src-tauri\\target\\$1\\test.bat"

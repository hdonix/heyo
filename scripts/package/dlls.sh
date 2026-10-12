#!/bin/sh
# Puts in a directory the DLLs that a program built for Windows asks for
# and Windows has not got by itself, and names them, one a line:
#
#   scripts/package/dlls.sh PROGRAM OUT-DIR
#
# Which they are is read from the program (the DLLs it imports, as
# `llvm-objdump -p` lists them), not from how it was said to be built.
# They are those of Microsoft's Visual C++ runtime: a Heyo with Skia in it
# imports the C++ library, one without imports none of them (vcruntime.sh,
# which fetches them, says why). What the program imports is taken, and
# what those import in turn: msvcp140.dll wants vcruntime140.dll, which
# heyo.exe itself has linked in.
#
# OUT-DIR is one that is used build after build: a DLL of the runtime left
# there that this program does not ask for is removed.
set -eu
[ $# -eq 2 ] || { sed -n '2,5p' "$0" | sed 's/^# \{0,1\}//'; exit 2; }
PROGRAM=$1; OUT=$2
HERE=$(cd "$(dirname "$0")" && pwd)
OBJDUMP=${LLVM_MINGW:-$HOME/.local/share/llvm-mingw}/bin/llvm-objdump
# Where nothing was compiled there need be no llvm-mingw: the system's does.
[ -x "$OBJDUMP" ] || OBJDUMP=$(command -v llvm-objdump) || { echo "$0: no llvm-objdump, in llvm-mingw or on PATH" >&2; exit 1; }

# The files of the runtime (the Microsoft.VC143.CRT of Visual Studio's
# redistributable files), in small letters as imports() gives them.
of_runtime() {
  case "$1" in msvcp140.dll|msvcp140_*.dll|vcruntime140.dll|vcruntime140_*.dll|concrt140.dll|vccorlib140.dll) return 0 ;; esac
  return 1
}
# (Told apart from a file that cannot be read, which is an error and not a
# program that imports nothing.)
imports() {
  listed=$("$OBJDUMP" -p "$1") || { echo "$0: $1 cannot be read" >&2; return 1; }
  echo "$listed" | sed -n 's/^ *DLL Name: //p' | tr A-Z a-z | sort -u
}

case "$("$OBJDUMP" -p "$PROGRAM" | sed -n 's/.*file format coff-//p')" in
  arm64) ARCH=aarch64 ;;
  x86-64) ARCH=x86_64 ;;
  *) echo "$0: $PROGRAM is not a program for Windows on x86_64 or arm64" >&2; exit 1 ;;
esac

FETCHED=$(mktemp -d)
trap 'rm -rf "$FETCHED"' EXIT
NEEDED=
ASKED=$(imports "$PROGRAM")
while [ -n "$ASKED" ]; do
  NEXT=
  for dll in $ASKED; do
    of_runtime "$dll" || continue
    case " $NEEDED " in *" $dll "*) continue ;; esac
    [ -n "$NEEDED" ] || "$HERE/vcruntime.sh" "$ARCH" "$FETCHED" >&2
    [ -f "$FETCHED/$dll" ] || { echo "$0: $dll is asked for, and scripts/package/vcruntime.sh has none for $ARCH" >&2; exit 1; }
    NEEDED="$NEEDED $dll"
    NEXT="$NEXT $(imports "$FETCHED/$dll" | tr '\n' ' ')"
  done
  ASKED=$NEXT
done

mkdir -p "$OUT"
for old in "$OUT"/*.dll; do
  [ -f "$old" ] || continue
  if of_runtime "$(basename "$old")"; then rm -f "$old"; fi
done
for dll in $NEEDED; do
  cp "$FETCHED/$dll" "$OUT/$dll"
  echo "$dll"
done

#!/bin/sh
# Fetches what only Microsoft has of what a program for Windows is compiled
# and linked against: the headers and libraries of its C and C++ runtime
# and of the Windows SDK, for x86_64 and aarch64, into $HEYO_WINSDK
# (default ~/.cache/heyo-build/xwin) unless they are there. build.sh
# compiles the *-pc-windows-msvc targets against them.
#
# FETCHING THEM ACCEPTS MICROSOFT'S LICENCE TERMS for them, those of Visual
# Studio: https://go.microsoft.com/fwlink/?LinkId=2086102. xwin, the
# program that fetches, asks for that before it does anything, and takes
# XWIN_ACCEPT_LICENSE=1 in the environment as the answer. This script does
# not answer for whoever runs it. For Heyo's own builds the project's owner
# accepted the terms on 2026-10-12, and the packaging workflow says so
# where it sets the variable; whoever runs this for themselves accepts them
# for themselves.
#
# What is fetched is the same every time. xwin is at the version written
# here, built from the dependencies its own lock file names. Which files it
# takes is said by a manifest of Visual Studio's, and Microsoft publishes a
# new one every few weeks under one address: so the manifest is taken from
# the address of this one alone, and must hash to what is written here. It
# names the list of packages by an address but by no hash that holds (and
# xwin checks none), so what that list hashes to is written here too; the
# list gives a SHA-256 for every file, and those xwin checks. A version
# number for the SDK and the runtime would not do as much: Microsoft
# publishes changed files under an unchanged number.
#
# To move to a newer one: `curl -sI https://aka.ms/vs/17/release/channel`
# gives the address of the manifest of the day (its Location). Write that
# and what the file hashes to here and run this: it stops at the list of
# packages and says what that one hashes to, which is the third thing to
# write here.
set -eu
XWIN_VERSION=0.10.0
# Visual Studio 2022 17.14.41 (September 2026): the runtime 14.44, the
# Windows SDK 10.0.26100.
MANIFEST_URL=https://download.visualstudio.microsoft.com/download/pr/bc92e2cb-33de-4a0c-995d-efa817f16b16/0dbdfd40c17757e64fc9f72cd9954ec8471c04fd8461a77806e5291e23239ac0/VisualStudio.17.Release.chman
MANIFEST_SHA256=fca418ba94ffbcfb7a2b25f10f16f39dd09660568d21eef4bd3f274cb0b27b8c
PACKAGES_SHA256=f0a50ea157222c29abd5ea6ff01bfc3c33b04e011c5e45ee2ca38ef0778e5643

DIR=${HEYO_WINSDK:-${XDG_CACHE_HOME:-$HOME/.cache}/heyo-build/xwin}
# What is laid out says what it was laid out from: a change above fetches again.
PIN="xwin $XWIN_VERSION, manifest $MANIFEST_SHA256, packages $PACKAGES_SHA256"
if [ "$(cat "$DIR/splat/.complete" 2>/dev/null)" = "$PIN" ]; then
  echo "Microsoft's headers and libraries are in $DIR/splat"
  exit 0
fi
hashes() { echo "$1  $2" | sha256sum -c - >/dev/null 2>&1; }

# A program of this directory's own, not one of the machine's.
cargo install --locked --root "$DIR/tool" xwin --version "$XWIN_VERSION"

mkdir -p "$DIR/cache/dl"
MANIFEST=$DIR/cache/VisualStudio.17.Release.chman
if ! hashes "$MANIFEST_SHA256" "$MANIFEST"; then
  curl -fsSL -o "$MANIFEST.part" "$MANIFEST_URL"
  hashes "$MANIFEST_SHA256" "$MANIFEST.part" || { echo "$0: $MANIFEST_URL does not hash to what is written here" >&2; exit 1; }
  mv "$MANIFEST.part" "$MANIFEST"
fi
# xwin keeps the list of packages in its cache under this name, and uses
# one it finds there as it is.
for list in "$DIR"/cache/dl/pkg_manifest_*.vsman; do
  hashes "$PACKAGES_SHA256" "$list" || rm -f "$list"
done

rm -rf "$DIR/splat"
"$DIR/tool/bin/xwin" --manifest "$MANIFEST" --cache-dir "$DIR/cache" --arch x86_64,aarch64 --http-retry 3 splat --output "$DIR/splat"
for list in "$DIR"/cache/dl/pkg_manifest_*.vsman; do
  if ! hashes "$PACKAGES_SHA256" "$list"; then
    rm -rf "$DIR/splat"
    [ -f "$list" ] || { echo "$0: xwin left no list of packages in $DIR/cache/dl" >&2; exit 1; }
    echo "$0: the list of packages xwin fetched hashes to $(sha256sum "$list" | cut -d' ' -f1), not to what is written here: nothing of it is kept" >&2
    exit 1
  fi
done
echo "$PIN" > "$DIR/splat/.complete"
# What was downloaded has served: only what is laid out is compiled against.
rm -rf "$DIR/cache"
echo "Microsoft's headers and libraries are in $DIR/splat ($(du -sh "$DIR/splat" | cut -f1))"

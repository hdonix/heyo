#!/bin/sh
# Lays out what Heyo links on Linux for another processor than this
# machine's, in a directory of its own (a sysroot):
#
#   scripts/package/sysroot.sh arm64|amd64
#
# Ubuntu's development packages for two architectures cannot be installed
# side by side (WebKitGTK's, among others, conflict), so nothing is
# installed: the packages and all they depend on are downloaded with an apt
# that knows only the other architecture, and unpacked. It needs no root.
# build.sh finds the result in $HEYO_SYSROOTS (default
# ~/.cache/heyo-build/sysroots), under the architecture's name.
#
# From Ubuntu 22.04, the oldest with WebKitGTK 4.1, whatever this machine
# runs: a program runs where the C library is as new as the one it was
# linked against, or newer.
set -eu
ARCH=${1:?usage: $0 arm64|amd64}
RELEASE=jammy
LIBRARIES="libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev libxkbcommon-dev"
# And what any C program is linked with.
TOOLCHAIN="libc6-dev libgcc-11-dev libstdc++-11-dev"
case "$ARCH" in
  arm64) MIRROR=http://ports.ubuntu.com/ubuntu-ports ;;
  amd64) MIRROR=http://archive.ubuntu.com/ubuntu ;;
  *) echo "$0: arm64 or amd64" >&2; exit 2 ;;
esac
ROOT=${HEYO_SYSROOTS:-${XDG_CACHE_HOME:-$HOME/.cache}/heyo-build/sysroots}/$ARCH
if [ -f "$ROOT/.complete" ]; then
  echo "the $ARCH sysroot is in $ROOT"
  exit 0
fi

APT=$(mktemp -d)
trap 'rm -rf "$APT"' EXIT
mkdir -p "$APT/lists/partial" "$APT/archives/partial"
: > "$APT/status"
for pocket in "$RELEASE" "$RELEASE-updates" "$RELEASE-security"; do
  echo "deb [arch=$ARCH signed-by=/usr/share/keyrings/ubuntu-archive-keyring.gpg] $MIRROR $pocket main universe"
done > "$APT/sources.list"
# An apt of its own: nothing of this machine's packages is read or changed.
apt() {
  apt-get -q -o APT::Architecture="$ARCH" -o APT::Architectures="$ARCH" \
    -o Dir::Etc::SourceList="$APT/sources.list" -o Dir::Etc::SourceParts=/dev/null \
    -o Dir::State::Lists="$APT/lists" -o Dir::State::status="$APT/status" \
    -o Dir::Cache::Archives="$APT/archives" -o Dir::Cache::pkgcache= -o Dir::Cache::srcpkgcache= \
    -o Debug::NoLocking=1 -o Acquire::Languages=none -o APT::Install-Recommends=false "$@"
}
apt update
apt install --download-only -y $TOOLCHAIN $LIBRARIES

rm -rf "$ROOT"
mkdir -p "$ROOT/usr/lib" "$ROOT/usr/bin" "$ROOT/usr/sbin"
# As on a running system, where /lib is /usr/lib: a library is looked for
# under either name.
for merged in lib bin sbin; do ln -s "usr/$merged" "$ROOT/$merged"; done
for package in "$APT"/archives/*.deb; do
  dpkg-deb --fsys-tarfile "$package" | tar -x --keep-directory-symlink -C "$ROOT"
done
# A link to /lib/… means the sysroot's /lib, not this machine's.
find "$ROOT" -type l | while read -r link; do
  to=$(readlink "$link")
  case "$to" in /*) ln -sfn "$(realpath -m --relative-to="$(dirname "$link")" "$ROOT$to")" "$link" ;; esac
done
: > "$ROOT/.complete"
echo "the $ARCH sysroot is in $ROOT ($(ls "$APT"/archives/*.deb | wc -l) packages, $(du -sh "$ROOT" | cut -f1))"

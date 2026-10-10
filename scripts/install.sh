#!/bin/sh
# Installs a source build of Heyo for the current user, under ~/.local:
#
#   bin/heyo                                  the program
#   lib/Heyo/heyo-*.cjs                       its helper scripts (where an
#                                             unbundled binary looks for them)
#   lib/Heyo/ui/                              its Flutter interface, if that
#                                             was built (scripts/build-ui.sh);
#                                             `heyo --ui=flutter` uses it
#   share/applications/heyo.desktop           the entry in the applications menu
#   share/icons/hicolor/<size>/apps/heyo.png  its icon
#
# Build first (pnpm tauri build --no-bundle), or pass --build. Running it
# again replaces what is there. --uninstall removes all of it, and the
# entries Heyo itself wrote that point at the program; settings, extensions
# and history are kept.
#
# PREFIX=/somewhere/else installs there instead of ~/.local.
set -eu
HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$HERE/.." && pwd)
PREFIX=${PREFIX:-$HOME/.local}
CONFIG=${XDG_CONFIG_HOME:-$HOME/.config}

BIN=$PREFIX/bin
LIB=$PREFIX/lib/Heyo
APPLICATIONS=$PREFIX/share/applications
ICONS=$PREFIX/share/icons/hicolor
SCRIPTS="heyo-runtime.cjs heyo-dictation.cjs heyo-ocr.cjs heyo-dev.cjs heyo-embed.cjs"
# The size each icon file is installed as.
ICON_FILES="32x32:32x32.png 128x128:128x128.png 256x256:128x128@2x.png 512x512:icon.png"

usage() {
  echo "usage: $0 [--build] | --uninstall" >&2
  exit 2
}

# Desktops read these lazily; telling them makes the entry show up at once.
refresh() {
  if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database -q "$APPLICATIONS" 2>/dev/null || true
  fi
  if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -q -t "$ICONS" 2>/dev/null || true
  fi
}

uninstall() {
  rm -f "$BIN/heyo" "$APPLICATIONS/heyo.desktop"
  for script in $SCRIPTS; do rm -f "$LIB/$script"; done
  rm -rf "$LIB/ui"
  rmdir "$LIB" 2>/dev/null || true
  for icon in $ICON_FILES; do rm -f "$ICONS/${icon%%:*}/apps/heyo.png"; done
  # Written by Heyo when it ran: launch at login, and the handler for the
  # links extensions sign in with. Both name a program that is now gone.
  rm -f "$CONFIG/autostart/heyo.desktop" "$APPLICATIONS/heyo-links.desktop"
  refresh
  echo "removed Heyo from $PREFIX (settings and data are kept: $CONFIG/heyo, ${XDG_DATA_HOME:-$HOME/.local/share}/heyo)"
}

# A file put in place whole: a running Heyo keeps the one it started from.
place() {
  mode=$1 from=$2 to=$3
  mkdir -p "$(dirname "$to")"
  install -m "$mode" "$from" "$to.new"
  mv -f "$to.new" "$to"
}

BUILD=
case "${1:-}" in
  "") ;;
  --build) BUILD=1 ;;
  --uninstall) uninstall; exit 0 ;;
  *) usage ;;
esac
[ $# -le 1 ] || usage

if [ -n "$BUILD" ]; then
  (cd "$REPO" && pnpm tauri build --no-bundle)
fi

PROGRAM=$REPO/src-tauri/target/release/heyo
if [ ! -x "$PROGRAM" ]; then
  echo "$PROGRAM is not built: run pnpm tauri build --no-bundle, or pass --build" >&2
  exit 1
fi
# The program refuses helper scripts of another version, so neither is
# installed unless they agree (see src-tauri/src/helper.rs).
VERSION=$(sed -n 's/^  "version": "\(.*\)",$/\1/p' "$REPO/src-tauri/tauri.conf.json")
for script in $SCRIPTS; do
  built=$REPO/runtime/dist/$script
  if [ ! -f "$built" ]; then
    echo "$built is not built: run pnpm build:runtime" >&2
    exit 1
  fi
  if [ "$(head -n 1 "$built")" != "// heyo-helper $VERSION" ]; then
    echo "$built was not built for Heyo $VERSION: run pnpm build:runtime" >&2
    exit 1
  fi
done
if [ "$PROGRAM" -ot "$REPO/src-tauri/tauri.conf.json" ]; then
  echo "warning: $PROGRAM is older than tauri.conf.json; if the version changed, build again" >&2
fi

# The Flutter interface is installed when it has been built, and only one
# built for this version: the program refuses any other
# (src-tauri/src/shell/flutter.rs). Heyo does not need it: without it,
# `--ui=flutter` says so and the web view is used.
UI=$REPO/ui/build/linux/$(uname -m | sed 's/aarch64/arm64/; s/x86_64/x64/')/release/bundle
if [ -x "$UI/heyo-ui" ] && [ "$("$UI/heyo-ui" --version 2>/dev/null)" != "heyo-ui $VERSION" ]; then
  echo "$UI/heyo-ui was not built for Heyo $VERSION: run scripts/build-ui.sh" >&2
  exit 1
fi

place 755 "$PROGRAM" "$BIN/heyo"
for script in $SCRIPTS; do
  place 644 "$REPO/runtime/dist/$script" "$LIB/$script"
done
# A directory put in place whole, as the files are. One left from an
# earlier build is not kept: it would be of the same version, and not match.
rm -rf "$LIB/ui.new" "$LIB/ui.old"
if [ -x "$UI/heyo-ui" ]; then
  cp -R "$UI" "$LIB/ui.new"
  [ ! -e "$LIB/ui" ] || mv "$LIB/ui" "$LIB/ui.old"
  mv "$LIB/ui.new" "$LIB/ui"
  rm -rf "$LIB/ui.old"
  FLUTTER_UI="with its Flutter interface (heyo --ui=flutter, or HEYO_UI=flutter)"
else
  rm -rf "$LIB/ui"
  FLUTTER_UI="without its Flutter interface, which is not built (scripts/build-ui.sh)"
fi
for icon in $ICON_FILES; do
  place 644 "$REPO/src-tauri/icons/${icon#*:}" "$ICONS/${icon%%:*}/apps/heyo.png"
done
# The same entry the packages carry, with the places filled in. The program
# is named in full: ~/.local/bin is not on every desktop's PATH.
ENTRY=$(mktemp)
sed -e 's|{{name}}|Heyo|' -e "s|{{exec}}|$BIN/heyo|" -e 's|{{icon}}|heyo|' "$REPO/packaging/linux/heyo.desktop" >"$ENTRY"
place 644 "$ENTRY" "$APPLICATIONS/heyo.desktop"
rm -f "$ENTRY"
refresh

echo "installed Heyo $VERSION in $PREFIX, $FLUTTER_UI"
if ! command -v node >/dev/null 2>&1; then
  echo "note: node is not on PATH; extensions need Node.js 22 or newer (or its path in Heyo Settings → Advanced)" >&2
fi
if pgrep -x heyo >/dev/null 2>&1; then
  echo "note: Heyo is running; quit and start it again to use this build" >&2
fi

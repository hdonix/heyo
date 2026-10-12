# What the scripts that take the README's pictures share, sourced by them:
# a sandbox started from nothing, and how a picture is cut out of its
# screen.
#
# The sandbox has a home directory with nothing personal in it, a short list
# of applications and a backdrop drawn for the purpose (readme-backdrop.py),
# so that the pictures show Heyo and not the machine they were taken on. The
# session runs at twice the size (a 3840x2160 screen, GDK_SCALE=2), for
# pictures that stay sharp on a dense display; the README shows them at half
# their width.
#
# The sandbox is in $HEYO_TEST_DIR (by default heyo-readme-shots in the
# temporary directory); what an earlier run left there is removed.
export HEYO_TEST_DIR="${HEYO_TEST_DIR:-${TMPDIR:-/tmp}/heyo-readme-shots}"
. "$(dirname "$0")/env.sh"
python3 "$HERE/down.py" > /dev/null; w 1
OUT="$REPO/docs/images"
mkdir -p "$OUT"

# Twice the size. The web view takes the scale from GTK; the settings
# window is drawn by Slint, which on X11 takes it from the desktop's
# settings, and a bare X server has none: it is told.
export SCREEN=3840x2160 GDK_SCALE=2 WINIT_X11_SCALE_FACTOR=2
# The backdrop kept around a window, in pixels of the picture, and the
# radius the picture's corners are rounded to.
MARGIN=16
RADIUS=28

# The speech engine installed for the Heyo of whoever runs this, which the
# picture of dictation reads (and writes nothing to). In the environment,
# for the script that another of these runs when it is done: by then the
# home directory is no longer theirs.
export HEYO_TEST_SPEECH="${HEYO_TEST_SPEECH:-$HOME/.local/share/heyo/dictation}"
# A home with nothing in it: no path of whoever runs this can show.
export HOME="$T/home/user"
rm -rf "$HOME"; mkdir -p "$HOME"

# The applications in the pictures: a few that most GNOME desktops have,
# instead of everything installed here. The sandbox is given a data
# directory that is the system's with `applications` replaced. (Heyo adds
# the Snap and Flatpak directories itself, which is where Firefox is.)
APPS="org.gnome.Nautilus org.gnome.Terminal org.gnome.TextEditor org.gnome.Calculator
  org.gnome.Calendar org.gnome.Settings org.gnome.SystemMonitor org.gnome.Evince
  org.gnome.eog org.gnome.Rhythmbox3 org.gnome.Totem org.gnome.Characters
  org.gnome.font-viewer org.gnome.DiskUtility org.gnome.Logs org.gnome.baobab
  org.gnome.FileRoller org.gnome.Contacts org.gnome.Shotwell simple-scan
  libreoffice-writer libreoffice-calc libreoffice-impress transmission-gtk"
SHARE="$T/share"
rm -rf "$SHARE"; mkdir -p "$SHARE/applications"
for entry in /usr/share/*; do
  [ "$(basename "$entry")" = applications ] || ln -s "$entry" "$SHARE/"
done
for app in $APPS; do
  [ -f "/usr/share/applications/$app.desktop" ] && ln -s "/usr/share/applications/$app.desktop" "$SHARE/applications/"
done
export XDG_DATA_DIRS="$SHARE"

# home: the sandbox's home directories as they are before anything ran.
home() {
  rm -rf "$T/home/config" "$T/home/data" "$T/home/cache" "$T/dconf.d"
  mkdir -p "$T/home/data" "$T/home/config/heyo" "$T/home/cache" "$T/dconf.d"
}

# session light|dark: the session's own settings, written before it starts
# (there is no bus to ask yet): the backdrop, and the icons every Ubuntu has
# rather than this machine's. Whatever else is in $T/dconf.d goes in too.
session() {
  python3 "$HERE/readme-backdrop.py" "$1" 3840 2160 "$T/backdrop-$1.png"
  mkdir -p "$T/dconf.d" "$XDG_CONFIG_HOME/dconf"
  cat > "$T/dconf.d/readme" <<EOF
[org/gnome/desktop/background]
picture-uri='file://$T/backdrop-$1.png'
picture-uri-dark='file://$T/backdrop-$1.png'
picture-options='stretched'

[org/gnome/desktop/interface]
icon-theme='Yaru'
color-scheme='$([ "$1" = dark ] && echo prefer-dark || echo default)'
enable-animations=false
EOF
  dconf compile "$XDG_CONFIG_HOME/dconf/user" "$T/dconf.d"
}

# size WINDOW TITLE: the width and height of the window, if it is there.
size() {
  id=$(xdotool search --onlyvisible --name "$1" | tail -1)
  [ -n "$id" ] && xwininfo -id "$id" | awk '/Width/{w=$2} /Height/{h=$2} END{print w, h}'
}
# picture NAME LEFT TOP RIGHT BOTTOM: that part of the screen as
# docs/images/NAME-$THEME.png, with its corners rounded.
picture() {
  python3 - "$OUT/$1-$THEME.png" "$2" "$3" "$4" "$5" "$RADIUS" <<'PY'
import os, sys
from PIL import Image, ImageDraw, ImageGrab
path, box, radius = sys.argv[1], tuple(map(int, sys.argv[2:6])), int(sys.argv[6])
image = ImageGrab.grab(xdisplay=os.environ["HEYO_TEST_DISPLAY"]).crop(box).convert("RGBA")
# The corners are cut at four times the size and brought down, for an edge
# that is smooth.
mask = Image.new("L", (image.width * 4, image.height * 4), 0)
ImageDraw.Draw(mask).rounded_rectangle((0, 0, mask.width - 1, mask.height - 1), radius * 4, fill=255)
image.putalpha(mask.resize(image.size, Image.Resampling.LANCZOS))
image.save(path, optimize=True)
print(f"{os.path.basename(path)}: {image.width}x{image.height}, {os.path.getsize(path) // 1024} KB")
PY
}
# shot NAME [WINDOW TITLE]: the window (the palette unless another is named)
# in the middle of the screen, with MARGIN of the backdrop around it.
shot() {
  id=$(xdotool search --onlyvisible --name "${2:-^Heyo$}" | tail -1)
  if [ -z "$id" ]; then echo "$1: no window to take a picture of" >&2; return; fi
  set -- "$1" "$id" $(size "${2:-^Heyo$}")
  x=$(((3840 - $3) / 2)); y=$(((2160 - $4) / 2))
  xdotool windowmove "$2" "$x" "$y"; w 0.8
  picture "$1" $((x - MARGIN)) $((y - MARGIN)) $((x + $3 + MARGIN)) $((y + $4 + MARGIN))
}

#!/bin/sh
# Inside the test session: saves a screenshot of the whole (virtual) screen
# to the given path, through GNOME Shell's own interface. Works only because
# the harness extension has switched the Shell's restrictions off.
OUT=${1:-/tmp/screen.png}
gdbus call --session --dest org.gnome.Shell.Screenshot --object-path /org/gnome/Shell/Screenshot \
  --method org.gnome.Shell.Screenshot.Screenshot false false "$OUT" >/dev/null && echo "$OUT"

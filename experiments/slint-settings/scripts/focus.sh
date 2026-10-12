#!/bin/sh
# Which window the X server sends keys to, by name.
. "$(dirname "$0")/lib.sh"
id=$(xdotool getwindowfocus 2>/dev/null)
echo "focus: $id $(xdotool getwindowname "$id" 2>/dev/null) (settings window: $(settings_window))"

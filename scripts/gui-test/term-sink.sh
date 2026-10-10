#!/bin/sh
# What runs inside the sandbox's terminal: writes each line pasted into the
# terminal to the file given, so a test can read what arrived.
exec cat > "$1"

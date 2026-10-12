#!/bin/sh
# Every process that runs in the sandbox, whoever started it: those whose
# environment names the sandbox's own bus. For seeing that nothing is left
# when the work is done, and what a process that ended left behind. It
# only looks.
. "$(dirname "$0")/lib.sh"
for environment in /proc/[0-9]*/environ; do
  pid=${environment#/proc/}; pid=${pid%/environ}
  [ -r "$environment" ] || continue
  if tr '\0' '\n' < "$environment" 2>/dev/null | grep -q "^DBUS_SESSION_BUS_ADDRESS=unix:path=$D/"; then
    written=no
    for file in "$D"/pids/*; do
      [ -f "$file" ] && [ "$(cut -d' ' -f1 "$file")" = "$pid" ] && written=$(basename "$file")
    done
    echo "$pid $(born "$pid") parent=$(ps -o ppid= -p "$pid" | tr -d ' ') written-down=$written $(ps -o comm= -p "$pid")"
  fi
done

#!/bin/sh
# Checks commit messages against CONTRIBUTING.md.
#
#   scripts/check-commits.sh <range>            every commit of a range (a..b)
#   scripts/check-commits.sh --file <path>      one message in a file (the commit-msg hook)
#
# The first line is `type(scope): subject`: one of the types below, a scope
# in lower case if there is one, at most 72 characters, no full stop. An
# empty line follows it when there is more. What git writes by itself
# (merges, reverts) is let through.
set -eu
TYPES='feat|fix|refactor|perf|test|docs|build|ci|chore|revert'

check() { # $1: where the message is from, stdin: the message
  subject=$(sed -n 1p "$2"); second=$(sed -n 2p "$2")
  case "$subject" in "Merge "*|"Revert \""*|"fixup! "*|"squash! "*) return 0 ;; esac
  problem=
  if ! printf '%s\n' "$subject" | grep -Eq "^($TYPES)(\([a-z0-9][a-z0-9-]*\))?!?: [^ ]"; then
    problem="does not begin with type(scope): where type is one of $(printf %s "$TYPES" | tr '|' ' ')"
  elif [ "${#subject}" -gt 72 ]; then
    problem="is ${#subject} characters long (at most 72)"
  elif printf '%s\n' "$subject" | grep -q '\.$'; then
    problem="ends with a full stop"
  elif [ -n "$second" ]; then
    problem="is not followed by an empty line"
  fi
  [ -z "$problem" ] && return 0
  echo "$1: the first line $problem" >&2
  echo "    $subject" >&2
  return 1
}

message=$(mktemp); trap 'rm -f "$message"' EXIT
case "${1:-}" in
  --file)
    # Lines git adds as comments are not part of the message.
    grep -v '^#' "$2" > "$message" || true
    check "commit message" "$message" || { echo "See CONTRIBUTING.md." >&2; exit 1; }
    ;;
  "") echo "usage: $0 <range> | --file <path>" >&2; exit 2 ;;
  *)
    failed=0
    for commit in $(git rev-list --no-merges "$1"); do
      git log -1 --format=%B "$commit" > "$message"
      check "$(git rev-parse --short "$commit")" "$message" || failed=1
    done
    [ "$failed" = 0 ] || { echo "See CONTRIBUTING.md." >&2; exit 1; }
    ;;
esac

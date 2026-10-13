#!/bin/sh
# notes.sh VERSION: what CHANGELOG.md says of that version, as the notes
# of its release. Fails if it says nothing: a release without a word on
# what is in it is stopped where it is made.
#
# A release's page breaks a line where the text does and has no files
# beside it, so each paragraph and each item is put on one line, and a
# link to a file of the repository is made one to that file at the tag.
version=${1:?usage: notes.sh VERSION}
cd "$(dirname "$0")/../.." || exit 1
notes=$(awk -v version="$version" '
  function out() { if (held != "") print held; held = "" }
  /^## / { out(); on = ($2 == version); next }
  !on { next }
  /^$/ || /^#/ || /^- / { out() }
  /^$/ { print; next }
  { sub(/^  +/, ""); held = (held == "" ? $0 : held " " $0) }
  /^#/ { out() }
  END { out() }
' CHANGELOG.md | sed -E "s#\]\(([A-Za-z0-9_./-]+\.md(\#[^)]*)?)\)#](https://github.com/hdonix/heyo/blob/v$version/\1)#g")
[ -n "$(printf %s "$notes" | tr -d '[:space:]')" ] || { echo "notes.sh: CHANGELOG.md has no section for $version" >&2; exit 1; }
printf '%s\n' "$notes"

#!/bin/sh
# The sandbox's input method: which engine it has, or (with a name) the one
# it is to have.
#
#   ime.sh             says the engine
#   ime.sh libpinyin   chooses Pinyin
. "$(dirname "$0")/lib.sh"
alive ibus || { echo "no input method runs in the sandbox (heyo.sh ibus)" >&2; exit 1; }
case "$(ibus address)" in *"$D/"*) ;; *) echo "not the sandbox's own daemon" >&2; exit 1 ;; esac
[ $# -gt 0 ] && { ibus engine "$1"; sleep 0.5; }
echo "engine: $(ibus engine)"

#!/bin/sh
# Fail unless the given file is a static 32-bit ARM executable.
set -eu
bin=${1:?usage: check-device-binary.sh <binary>}
info=$(file -b "$bin")
echo "$bin: $info"
case "$info" in
    *"ELF 32-bit LSB"*"ARM"*"statically linked"*) ;;
    *) echo "error: not a static 32-bit ARM executable" >&2; exit 1 ;;
esac

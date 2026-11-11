#!/bin/sh
# Copy one file from slot with a provenance header. slot is not vendored:
# point SLOT at a checkout of https://github.com/BrandonKowalski/slot, or
# leave it unset and a shallow clone is made under target/slot.
# usage: copy-from-slot.sh <path in slot> <destination> "<what changed>"
set -eu
src=$1 dst=$2 note=$3
here=$(cd "$(dirname "$0")/.." && pwd)
slot=${SLOT:-$here/target/slot}
if [ ! -d "$slot/.git" ]; then
    git clone --quiet --depth 1 https://github.com/BrandonKowalski/slot "$slot"
fi
commit=$(git -C "$slot" rev-parse --short HEAD)
mkdir -p "$(dirname "$dst")"
{
    echo "// Copied from slot (https://github.com/BrandonKowalski/slot) at $commit:"
    echo "//   $src"
    echo "// Changes: $note"
    echo "// Copyright (C) Brandon Kowalski and slot contributors. GPL-3.0-or-later."
    echo
    cat "$slot/$src"
} > "$dst"

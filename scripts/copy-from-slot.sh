#!/bin/sh
# Copy one file from the slot reference with a provenance header.
# usage: copy-from-slot.sh <path in slot> <destination> "<what changed>"
set -eu
src=$1 dst=$2 note=$3
here=$(cd "$(dirname "$0")/.." && pwd)
commit=$(git -C "$here/reference/slot" rev-parse --short HEAD)
mkdir -p "$(dirname "$dst")"
{
    echo "// Copied from slot (https://github.com/BrandonKowalski/slot) at $commit:"
    echo "//   $src"
    echo "// Changes: $note"
    echo "// Copyright (C) Brandon Kowalski and slot contributors. GPL-3.0-or-later."
    echo
    cat "$here/reference/slot/$src"
} > "$dst"

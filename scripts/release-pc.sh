#!/bin/sh
# Publish the PC zip CI built (dist/slot-mini-pc-<version>-windows-x64.zip)
# on GitHub as pc-v<version>. Run by the `release` job in
# .github/workflows/ci.yml on pushes to main, after scripts/release.sh.
#
# - The version is slot-mini-pc's own (crates/slot-mini-pc/Cargo.toml), a
#   release line apart from the Miyoo's v<version>.
# - If pc-v<version> is already released, nothing happens: bump that
#   version to cut the next one.
# - It is never GitHub's "latest" release. The Miyoo's updater asks for the
#   latest release, and that must stay a v<version> one.
#
# Needs: gh, GH_TOKEN with contents: write, GITHUB_SHA.
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
version=$(sed -n 's/^version = "\(.*\)"/\1/p' crates/slot-mini-pc/Cargo.toml | head -1)
zip=dist/slot-mini-pc-$version-windows-x64.zip
sha=${GITHUB_SHA:?GITHUB_SHA is not set}
[ -f "$zip" ] || { echo "no $zip" >&2; exit 1; }

if gh release view "pc-v$version" > /dev/null 2>&1; then
    echo "pc-v$version is already released; nothing to publish"
    exit 0
fi

notes=$(mktemp)
trap 'rm -f "$notes"' EXIT
cat > "$notes" <<NOTES
slot-mini $version for Windows (64-bit), built from ${sha}.

This is the PC app. For the Miyoo Mini Plus, take the newest release
without "PC" in its name.

**Install:** unzip \`slot-mini-pc-$version-windows-x64.zip\` anywhere and run
\`slot-mini-pc.exe\`. It needs a standalone RetroArch. The exe is not signed:
if Windows says "Windows protected your PC", choose "More info", then
"Run anyway". See README.txt in the zip for the controls and first run.
NOTES

echo "publishing pc-v$version"
gh release create "pc-v$version" "$zip" \
    --target "$sha" \
    --title "slot-mini PC $version" \
    --notes-file "$notes" \
    --latest=false

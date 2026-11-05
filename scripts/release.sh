#!/bin/sh
# Publish the release zip CI built (dist/slot-mini-<version>.zip) on GitHub.
# Run by the `release` job in .github/workflows/ci.yml on pushes to main.
#
# - If v<version> (from Cargo.toml) has no release yet, it becomes a full
#   release, tagged at this commit.
# - Otherwise this commit replaces the rolling "main-latest" pre-release, so
#   every push to main is downloadable without bumping the version. Bump
#   `version` in Cargo.toml to cut the next numbered release.
#
# Needs: gh, GH_TOKEN with contents: write, GITHUB_SHA.
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
zip=dist/slot-mini-$version.zip
sha=${GITHUB_SHA:?GITHUB_SHA is not set}
[ -f "$zip" ] || { echo "no $zip" >&2; exit 1; }

notes=$(mktemp)
trap 'rm -f "$notes"' EXIT
cat > "$notes" <<EOF
slot-mini $version for Onion OS on the Miyoo Mini Plus, built from ${sha}.

**Install:** unzip \`slot-mini-$version.zip\` at the root of your SD card, so that
\`App/slot-mini/\` sits next to Onion's other apps, then open Apps -> slot-mini.
The zip adds that folder and nothing else. See the README for the optional boot
launcher, uninstalling and controls.

**Not yet tested on hardware.** Please follow docs/DEVICE-TESTING.md on a spare
or backed-up card, starting in app mode.
EOF

if gh release view "v$version" > /dev/null 2>&1; then
    tag=main-latest
    echo "v$version is already released; refreshing $tag"
    # Delete the old one; it is fine if there is none yet.
    gh release delete "$tag" --cleanup-tag --yes > /dev/null 2>&1 || true
    gh release create "$tag" "$zip" \
        --target "$sha" \
        --prerelease \
        --title "Latest main build ($version, $(echo "$sha" | cut -c1-7))" \
        --notes-file "$notes"
else
    echo "publishing v$version"
    gh release create "v$version" "$zip" \
        --target "$sha" \
        --title "slot-mini $version" \
        --notes-file "$notes" \
        --generate-notes
fi

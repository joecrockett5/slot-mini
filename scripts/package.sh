#!/bin/sh
# Build the release zip, dist/slot-mini-<version>.zip. Unzip it at the root
# of an Onion SD card: it holds App/slot-mini/ and nothing else.
#
# SLOT_MINI_BIN can point at an already built armv7 binary.
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
target=armv7-unknown-linux-musleabihf
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
bin=${SLOT_MINI_BIN:-$root/target/$target/release/slot-mini}
if [ ! -f "$bin" ]; then
    cargo build --release --target "$target" -p slot-mini
fi
scripts/check-device-binary.sh "$bin"

stage=$root/dist/stage
app=$stage/App/slot-mini
rm -rf "$stage"
mkdir -p "$app/licenses"

cp "$bin" "$app/slot-mini"
cp package/app/launch.sh package/app/config.json package/app/icon.png "$app/"
cp package/app/theme.txt.example package/app/settings.txt.example "$app/"
# The boot shim is built into slot-mini; this copy is for reading.
cp package/shim.sh "$app/boot-shim.sh.txt"
chmod 755 "$app/slot-mini" "$app/launch.sh"

cp LICENSE "$app/LICENSE.txt"
cp licenses/slot-GPL-3.0.txt "$app/licenses/slot-GPL-3.0.txt"
cp crates/slot-mini-ui/assets/label.ttf.OFL.txt "$app/licenses/OpenSans-OFL-1.1.txt"
cp crates/slot-mini-ui/assets/SymbolsNerdFontMono-Regular.ttf.LICENSE.txt "$app/licenses/NerdFontsSymbols-LICENSE.txt"
python3 scripts/third-party.py "$app/licenses/THIRD-PARTY.txt"
cp package/CREDITS.txt "$app/CREDITS.txt"
cp package/README.txt "$app/README.txt"

zip=$root/dist/slot-mini-$version.zip
rm -f "$zip"
(cd "$stage" && zip -q -r -X "$zip" App)
echo "$zip"
unzip -l "$zip"

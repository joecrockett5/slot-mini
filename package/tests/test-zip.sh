#!/bin/sh
# Checks the release zip: it holds App/slot-mini/ and nothing else, the app
# manifest is what Onion reads, and its launch.sh, run the way Onion runs an
# app, queues a game for Onion's runtime. The ARM binary is swapped for a
# host build (SLOT_MINI_BIN) so the app can run here.
#
#   SLOT_MINI_BIN=target/debug/slot-mini sh package/tests/test-zip.sh dist/slot-mini-*.zip
set -eu

ZIP="${1:?usage: test-zip.sh <zip>}"
case "$ZIP" in /*) ;; *) ZIP="$(pwd)/$ZIP" ;; esac
BIN="${SLOT_MINI_BIN:?set SLOT_MINI_BIN to a slot-mini binary that runs here}"
case "$BIN" in /*) ;; *) BIN="$(pwd)/$BIN" ;; esac
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT HUP INT TERM
CARD="$TMP/card"

fail() {
	echo "FAIL: $*" >&2
	exit 1
}

mkdir -p "$CARD"
(cd "$CARD" && unzip -q "$ZIP")

others=$(cd "$CARD" && find . -mindepth 1 -not -path './App' -not -path './App/slot-mini' -not -path './App/slot-mini/*')
[ -z "$others" ] || fail "the zip writes outside App/slot-mini: $others"

app="$CARD/App/slot-mini"
for f in slot-mini launch.sh config.json icon.png LICENSE.txt CREDITS.txt README.txt \
	licenses/slot-GPL-3.0.txt licenses/OpenSans-OFL-1.1.txt licenses/NerdFontsSymbols-LICENSE.txt \
	licenses/THIRD-PARTY.txt; do
	[ -s "$app/$f" ] || fail "missing $f"
done
[ -x "$app/slot-mini" ] && [ -x "$app/launch.sh" ] || fail "binary or launch.sh not executable"
file -b "$app/slot-mini" | grep -q "ELF 32-bit LSB.*ARM.*statically linked" || fail "not a static ARM binary"
python3 -c "
import json, sys
c = json.load(open('$app/config.json'))
assert c['launch'] == 'launch.sh' and c['label'] and c['icon'] == 'icon.png', c
" || fail "config.json is not an Onion app manifest"

# Make the card look like Onion's, with the fixture's games, and run the app
# as Onion's runtime does: cd into the folder and run launch.sh.
"$BIN" --make-fake-sd "$CARD" > /dev/null
mkdir -p "$TMP/tmp"
mv "$app/slot-mini" "$TMP/slot-mini.arm"
cat > "$app/slot-mini" <<-SH
	#!/bin/sh
	exec "$BIN" "\$@" --sd "$CARD" --tmp "$TMP/tmp" --backend headless --sim --frames 200 --input "300:press:A,360:release:A"
SH
chmod 755 "$app/slot-mini"
(cd "$app" && sh ./launch.sh)
grep -q '"/mnt/SDCARD/Emu/GBA/launch.sh" "/mnt/SDCARD/Emu/GBA/../../Roms/GBA/2 Games in 1 (Europe).gba"' \
	"$CARD/.tmp_update/cmd_to_run.sh" || fail "the app did not queue the game for Onion"
[ -f "$TMP/tmp/quick_switch" ] || fail "the app did not ask Onion for a quick switch"
[ -s "$CARD/Roms/recentlist.json" ] || fail "the launch is not in Recents"
[ -f "$app/data/state.txt" ] || fail "slot-mini's state is not kept in its app folder"

echo "test-zip: ok"

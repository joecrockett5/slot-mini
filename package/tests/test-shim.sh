#!/bin/sh
# Tests for the boot shim (package/shim.sh) and the install/restore switch,
# in the style of slot's card/test-*.sh. Everything runs against a fake card
# in a temporary folder:
#   - Onion's MainUI binaries are scripts that log that they ran, and as what;
#   - slot-mini is a script that logs and exits with $FAKE_RC;
#   - detectKey reports L2 held when $FAKE_HELD is 1.
# The real slot-mini binary (SLOT_MINI_BIN) does the installing and restoring.
#
#   SLOT_MINI_BIN=target/debug/slot-mini sh package/tests/test-shim.sh
set -eu

HERE="$(cd "$(dirname "$0")" && pwd)"
BIN="${SLOT_MINI_BIN:?set SLOT_MINI_BIN to a slot-mini binary that runs here}"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT HUP INT TERM
CARD="$TMP/card"
LOG="$TMP/log"
pass=0

fail() {
	echo "FAIL: $*" >&2
	echo "--- log ---" >&2
	cat "$LOG" >&2 2> /dev/null || true
	exit 1
}

ok() {
	pass=$((pass + 1))
}

setup() {
	rm -rf "$CARD" "$TMP/tmp" "$LOG"
	mkdir -p "$CARD/.tmp_update/bin" "$CARD/.tmp_update/config" "$CARD/App/slot-mini" "$TMP/tmp"
	: > "$LOG"
	for name in MainUI-283-clean MainUI-283-expert MainUI-354-clean MainUI-354-expert; do
		cat > "$CARD/.tmp_update/bin/$name" <<-SH
			#!/bin/sh
			# stock $name
			echo "stock $name as \$(basename "\$0")" >> "$LOG"
		SH
		chmod 755 "$CARD/.tmp_update/bin/$name"
	done
	cat > "$CARD/App/slot-mini/slot-mini" <<-SH
		#!/bin/sh
		echo "slot-mini \$* parent=\$(cat /proc/\$PPID/comm)" >> "$LOG"
		exit \${FAKE_RC:-0}
	SH
	chmod 755 "$CARD/App/slot-mini/slot-mini"
	cat > "$CARD/.tmp_update/bin/detectKey" <<-SH
		#!/bin/sh
		[ "\$1" = 15 ] && [ "\${FAKE_HELD:-0}" = 1 ]
	SH
	chmod 755 "$CARD/.tmp_update/bin/detectKey"
}

install() {
	"$BIN" --sd "$CARD" --tmp "$TMP/tmp" --install-launcher
}

restore() {
	"$BIN" --sd "$CARD" --tmp "$TMP/tmp" --restore-launcher
}

# Run the launcher as Onion's runtime does: the selected binary, with
# DEVICE_ID exported. The bind mount is not reproduced; the file is run by
# a copy named MainUI, as the mount makes it look.
launch() {
	mkdir -p "$TMP/miyoo/app"
	cp "$CARD/.tmp_update/bin/MainUI-${DEVICE:-354}-${MODE:-clean}" "$TMP/miyoo/app/MainUI"
	(cd "$TMP/miyoo/app" && DEVICE_ID=${DEVICE:-354} SLOT_MINI_SD="$CARD" SLOT_MINI_TMP="$TMP/tmp" ./MainUI)
}

ran() {
	grep -q "$1" "$LOG"
}

# --- install -------------------------------------------------------------

setup
install || fail "install failed"
grep -q "slot-mini boot shim" "$CARD/.tmp_update/bin/MainUI-354-clean" || fail "354-clean is not the shim"
grep -q "slot-mini boot shim" "$CARD/.tmp_update/bin/MainUI-283-expert" || fail "283-expert is not the shim"
grep -q "# stock MainUI-354-clean" "$CARD/.tmp_update/slot-mini/stock/MainUI-354-clean/MainUI" ||
	fail "354-clean was not backed up"
[ -x "$CARD/.tmp_update/slot-mini/stock/MainUI-354-clean/MainUI" ] || fail "backup is not executable"
[ ! -e "$CARD/.tmp_update/bin/MainUI-285-clean" ] || fail "created a variant that was not there"
ok

# --- normal start: slot-mini runs, not under a MainUI name -----------------

setup
install
launch
ran "slot-mini --mode launcher" || fail "slot-mini did not run"
! ran "stock" || fail "Onion's MainUI ran after a clean exit"
ran "parent=sh" || fail "slot-mini's parent is not named sh, keymon would see MainUI"
ok

# --- crash fallback ------------------------------------------------------

setup
install
FAKE_RC=139 launch
ran "slot-mini --mode launcher" || fail "slot-mini did not run"
ran "stock MainUI-354-clean as MainUI" || fail "a crash did not fall back to Onion's MainUI"
ok

setup
install
FAKE_RC=1 launch
ran "stock MainUI-354-clean" || fail "an error exit did not fall back"
ok

# --- exit to Onion ---------------------------------------------------------

setup
install
FAKE_RC=10 launch
ran "stock MainUI-354-clean as MainUI" || fail "exit code 10 did not start Onion's MainUI"
ok

# --- shutting down: no MainUI ----------------------------------------------

setup
install
touch "$TMP/tmp/.offOrder"
FAKE_RC=143 launch
! ran "stock" || fail "started MainUI during shutdown"
ok

# --- missing binary --------------------------------------------------------

setup
install
rm "$CARD/App/slot-mini/slot-mini"
launch
ran "stock MainUI-354-clean" || fail "missing slot-mini did not fall back"
ok

setup
install
rm -rf "$CARD/App/slot-mini"
launch
ran "stock MainUI-354-clean" || fail "a deleted app folder did not fall back"
ok

setup
install
chmod 644 "$CARD/App/slot-mini/slot-mini"
launch
ran "stock MainUI-354-clean" || fail "a non-executable slot-mini did not fall back"
ok

# --- button held at start ---------------------------------------------------

setup
install
FAKE_HELD=1 launch
! ran "slot-mini --mode" || fail "slot-mini ran with L2 held"
ran "stock MainUI-354-clean" || fail "L2 held did not start Onion's MainUI"
ok

# --- DISABLED marker ---------------------------------------------------------

setup
install
touch "$CARD/.tmp_update/slot-mini/DISABLED"
launch
! ran "slot-mini --mode" || fail "slot-mini ran while DISABLED"
ran "stock MainUI-354-clean" || fail "DISABLED did not start Onion's MainUI"
ok

# --- Expert mode and other models pick their own original ------------------

setup
install
touch "$CARD/.tmp_update/config/.showExpert"
FAKE_RC=10 MODE=expert launch
ran "stock MainUI-354-expert" || fail "Expert mode did not get the expert original"
ok

setup
install
rm -rf "$CARD/.tmp_update/slot-mini/stock/MainUI-354-clean"
FAKE_RC=10 launch
ran "stock MainUI-354-expert" || fail "a missing clean backup did not fall back to the expert one"
ok

setup
install
FAKE_RC=10 DEVICE=283 launch
ran "stock MainUI-283-clean" || fail "the Mini did not get its own original"
ok

# --- double install --------------------------------------------------------

setup
install
install || fail "second install failed"
! grep -q "slot-mini boot shim" "$CARD/.tmp_update/slot-mini/stock/MainUI-354-clean/MainUI" ||
	fail "the second install backed up the shim"
restore || fail "restore after double install failed"
grep -q "# stock MainUI-354-clean" "$CARD/.tmp_update/bin/MainUI-354-clean" ||
	fail "double install lost the original"
ok

# --- restore ---------------------------------------------------------------

setup
cp "$CARD/.tmp_update/bin/MainUI-354-clean" "$TMP/original"
install
restore || fail "restore failed"
cmp -s "$TMP/original" "$CARD/.tmp_update/bin/MainUI-354-clean" || fail "restore did not put the exact original back"
[ -x "$CARD/.tmp_update/bin/MainUI-354-clean" ] || fail "restored original is not executable"
restore || fail "a second restore failed"
cmp -s "$TMP/original" "$CARD/.tmp_update/bin/MainUI-354-clean" || fail "a second restore changed the original"
launch
ran "stock MainUI-354-clean" || fail "the restored original does not run"
ok

# --- Onion updated in between: restore leaves its new binary ---------------

setup
install
printf '#!/bin/sh\necho "newer onion" >> "%s"\n' "$LOG" > "$CARD/.tmp_update/bin/MainUI-354-clean"
restore
grep -q "newer onion" "$CARD/.tmp_update/bin/MainUI-354-clean" || fail "restore overwrote Onion's updated MainUI"
ok

# --- half-finished install -------------------------------------------------

# Power cut after the backup but before the shim: the original still runs.
setup
mkdir -p "$CARD/.tmp_update/slot-mini/stock/MainUI-354-clean"
cp "$CARD/.tmp_update/bin/MainUI-354-clean" "$CARD/.tmp_update/slot-mini/stock/MainUI-354-clean/MainUI"
printf 'half' > "$CARD/.tmp_update/bin/MainUI-354-clean.slot-mini-new"
launch
ran "stock MainUI-354-clean" || fail "a half-finished install broke the launcher"
install || fail "install over a half-finished one failed"
launch
ran "slot-mini --mode launcher" || fail "the completed install does not run slot-mini"
ok

# --- no original to fall back to ---------------------------------------------

# A shim whose originals are all gone: a crash must not loop forever on
# exec; the shim gives up and Onion's loop will try again.
setup
install
rm -rf "$CARD/.tmp_update/slot-mini/stock"
FAKE_RC=139 launch && fail "the shim claimed success with nothing to run" || true
ran "slot-mini --mode launcher" || fail "slot-mini did not run"
ok

# A missing original and a clean exit: nothing else is started.
setup
install
rm -rf "$CARD/.tmp_update/slot-mini/stock"
FAKE_RC=10 launch || true
[ "$(grep -c 'slot-mini --mode launcher' "$LOG")" = 2 ] ||
	fail "with no original, exit to Onion should restart slot-mini once"
ok

# Refuse to install over a card whose shim lost its original.
setup
install
rm -rf "$CARD/.tmp_update/slot-mini/stock/MainUI-354-clean"
install && fail "installed over a shim with no original" || true
ok

# --- not an Onion card ---------------------------------------------------------

setup
rm -rf "$CARD/.tmp_update/bin"/MainUI-*
install && fail "installed on a card without Onion's MainUI" || true
ok

# --- the shim is POSIX sh ---------------------------------------------------------

sh -n "$HERE/../shim.sh" || fail "shim.sh does not parse"
if command -v busybox > /dev/null 2>&1; then
	busybox sh -n "$HERE/../shim.sh" || fail "shim.sh does not parse in busybox sh"
fi
ok

echo "test-shim: $pass passed"

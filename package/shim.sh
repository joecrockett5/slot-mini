#!/bin/sh
# slot-mini boot shim
#
# Installed by slot-mini ("Boot launcher" in its MENU screen) in place of one
# of Onion's MainUI binaries in /mnt/SDCARD/.tmp_update/bin/. Onion's runtime
# bind-mounts that file over /mnt/SDCARD/miyoo/app/MainUI and runs it every
# time the launcher is due, so this runs slot-mini instead and falls back to
# Onion's own MainUI, backed up by the installer in
# /mnt/SDCARD/.tmp_update/slot-mini/stock/<file name>/MainUI, when:
#   - slot-mini is missing or not executable,
#   - L2 is held while it starts, or .tmp_update/slot-mini/DISABLED exists,
#   - slot-mini exits with an error or crashes,
#   - the player picks "Exit to Onion" (slot-mini exits with code 10).
# "Boot launcher: Off" in slot-mini puts the original back. To undo by hand,
# copy stock/<file name>/MainUI over .tmp_update/bin/<file name>.
#
# The whole script is one { } block so that sh has read all of it before
# running any of it: slot-mini may replace this file while it runs.
{
sd=${SLOT_MINI_SD:-/mnt/SDCARD}
sysdir=$sd/.tmp_update
app=$sd/App/slot-mini
stock_dir=$sysdir/slot-mini/stock

# Onion's keymon treats any process named MainUI* as MainUI and takes over
# MENU, SELECT, Y and L1. Run on as plain `sh`, so keymon leaves them alone.
if [ "${SLOT_MINI_SHIM:-}" != 1 ]; then
    SLOT_MINI_SHIM=1 exec /bin/sh "$0" "$@"
    exit 1
fi

# The original for this model and Expert mode, as runtime.sh picks it,
# then the other mode's, then any.
stock() {
    mode=clean
    [ -f "$sysdir/config/.showExpert" ] && mode=expert
    other=expert
    [ "$mode" = expert ] && other=clean
    for name in "MainUI-${DEVICE_ID:-354}-$mode" "MainUI-${DEVICE_ID:-354}-$other"; do
        if [ -x "$stock_dir/$name/MainUI" ] && [ -s "$stock_dir/$name/MainUI" ]; then
            echo "$stock_dir/$name/MainUI"
            return 0
        fi
    done
    for f in "$stock_dir"/*/MainUI; do
        if [ -x "$f" ] && [ -s "$f" ]; then
            echo "$f"
            return 0
        fi
    done
    return 1
}

# Start Onion's MainUI as a file named MainUI, as keymon expects. If there
# is no original to start, slot-mini is better than no launcher at all.
onion() {
    echo "slot-mini shim: starting Onion's MainUI ($1)" >&2
    if s=$(stock); then
        exec "$s"
    fi
    echo "slot-mini shim: no original MainUI backed up" >&2
    if [ "$1" != "slot-mini failed" ] && [ -x "$app/slot-mini" ]; then
        exec "$app/slot-mini" --mode launcher
    fi
    exit 1
}

held() {
    detect=${SLOT_MINI_DETECTKEY:-$sysdir/bin/detectKey}
    [ -x "$detect" ] || return 1
    # detectKey exits 0 while the key is down; 15 is L2 (KEY_TAB).
    "$detect" 15
}

if [ ! -x "$app/slot-mini" ]; then
    onion "slot-mini is missing"
fi
if [ -f "$sysdir/slot-mini/DISABLED" ]; then
    onion "DISABLED"
fi
if held; then
    onion "L2 held"
fi

"$app/slot-mini" --mode launcher
rc=$?

# Shutting down (keymon asked, or Onion's off order): leave quietly.
if [ -f /tmp/.offOrder ] || [ -f /tmp/shutting_down ]; then
    exit 0
fi
case $rc in
    0) exit 0 ;;
    10) onion "exit to Onion" ;;
    *) onion "slot-mini failed" ;;
esac
exit 1
}

#!/bin/sh
# A stand-in for Onion's runtime loop, for desktop integration tests.
#
# Mirrors the parts of Onion's static/build/.tmp_update/runtime.sh (OnionUI/Onion
# at 07505ea) that move a launch along: check_main_ui (run MainUI, then move
# /tmp/cmd_to_run.sh into .tmp_update), launch_game (drop quick_switch, honour
# /tmp/reset_game the way override_game_core does, run the command from the
# RetroArch folder), and check_switcher (keep the command only after a quick
# switch). Device paths in commands are mapped onto the fake card.
#
# usage: fake-runtime.sh <sd> <tmp> <turns> <mainui command...>
set -u
sd=$1 tmp=$2 turns=$3
shift 3
sysdir=$sd/.tmp_update
log=$tmp/runtime.log
mkdir -p "$sysdir" "$sd/RetroArch"

to_card() {
    sed "s|/tmp/|@TMP@/|g; s|/mnt/SDCARD|$sd|g; s|@TMP@|$tmp|g"
}

turn=0
while [ "$turn" -lt "$turns" ]; do
    turn=$((turn + 1))

    # check_main_ui
    if [ ! -f "$sysdir/cmd_to_run.sh" ]; then
        echo "turn $turn: MainUI" >> "$log"
        "$@"
        echo "turn $turn: MainUI exit $?" >> "$log"
        [ -f "$tmp/cmd_to_run.sh" ] && mv -f "$tmp/cmd_to_run.sh" "$sysdir/cmd_to_run.sh"
    fi

    # check_game -> launch_game
    if [ -f "$sysdir/cmd_to_run.sh" ]; then
        cmd=$(cat "$sysdir/cmd_to_run.sh")
        echo "turn $turn: run $cmd" >> "$log"
        rm -f "$tmp/quick_switch"
        if [ -f "$tmp/reset_game" ]; then
            # override_game_core: one run without the auto state.
            echo "turn $turn: appendconfig /tmp/reset.cfg" >> "$log"
            rm -f "$tmp/reset_game"
        fi
        (cd "$sd/RetroArch" && printf '%s\n' "$cmd" | to_card | sh)
        echo "turn $turn: game exit $?" >> "$log"
    fi

    # check_switcher
    if [ -f "$tmp/quick_switch" ]; then
        rm -f "$tmp/quick_switch"
    else
        rm -f "$sysdir/cmd_to_run.sh"
    fi
done

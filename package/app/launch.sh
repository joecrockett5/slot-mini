#!/bin/sh
# slot-mini as an Onion app (Apps -> slot-mini). Picking a game queues it for
# Onion's runtime and exits; after the game Onion returns to its own menu.
# Logs go to .tmp_update/logs/slot-mini.log when Onion's logging is on.
cd "$(dirname "$0")" || exit 1
log=/dev/null
if [ -f /mnt/SDCARD/.tmp_update/config/.logging ] && mkdir -p /mnt/SDCARD/.tmp_update/logs 2> /dev/null; then
    log=/mnt/SDCARD/.tmp_update/logs/slot-mini.log
fi
./slot-mini --mode app >> "$log" 2>&1
exit 0

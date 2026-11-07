# Platform facts: Miyoo Mini Plus on Onion OS

Everything here was checked against source, not a device. No Mini Plus was
available while writing slot-mini, so "confirmed" means "confirmed in the code
that runs on the device", not "observed on hardware". `docs/DEVICE-TESTING.md`
lists what a human should check on a real unit.

Sources, pinned to the commits that were read:

- Onion OS, `OnionUI/Onion` at
  [`07505ea`](https://github.com/OnionUI/Onion/tree/07505ea58c7bba698d6b9220ff43946a43cac76b)
  (2026-01-21). Links below are shortened to `Onion:<path>#L<line>`; they are
  relative to that tree.
- Open MainUI, `robcodedev/onionos-mainui-opensource` at
  [`17fe90d`](https://github.com/robcodedev/onionos-mainui-opensource/tree/17fe90d893358048ed2422481f60a6a932f94ad7)
  (2026-10-24). Shortened to `OMUI:<path>`.

Status key: **confirmed** (matches source), **corrected** (the lead in the brief
was wrong or incomplete, the fix is given), **unresolved** (could not be settled
from source; what slot-mini does about it is given).

## Summary

| Lead | Status |
| --- | --- |
| Display: `/dev/fb0`, 640x480, 32 bpp | confirmed, with notes on 752x560 panels |
| Display: panel rotated 180° | confirmed |
| Display: page flip with `FBIOPAN_DISPLAY` | confirmed (double-height virtual framebuffer) |
| Display: `MI_GFX` blitter optional | confirmed optional; not used |
| Input: evdev, key codes from Onion's key map | confirmed (`/dev/input/event0`) |
| Input: MENU, POWER, L2, R2 | confirmed; **corrected**: keymon also acts on MENU, SELECT, Y, POWER |
| Audio: `MI_AO`, simplest way to play a clip | **corrected**: there is no OSS device; audio goes through `audioserver` via `libpadsp.so`, which a static binary cannot preload. Unresolved on hardware; slot-mini plays clips through Onion's own `ffplay`, off by default |
| Battery level | confirmed: `/tmp/percBat` written by `batmon` |
| Wi-Fi state | confirmed: `wifi` in `/mnt/SDCARD/system.json`, link state in `/sys/class/net/wlan0/operstate` |
| Launch flow: MainUI writes `/tmp/cmd_to_run.sh` and exits | confirmed; **corrected**: the loop moves it to `/mnt/SDCARD/.tmp_update/cmd_to_run.sh` and runs that |
| Recents and Game Switcher | confirmed: the launcher must write the line itself; format below |
| Paths: Roms, Emu launch scripts, RetroArch, saves | confirmed |
| Resume vs fresh start | **corrected** (on a device): released Onion has no `/tmp/reset_game`; slot-mini writes the same direct RetroArch command as Onion's "Reset game", with Onion's `reset.cfg`; on the development build it uses the runtime's `/tmp/reset_game` (0.5.0) |
| Onion versions | 4.3.1-1 and the development build both supported, told apart by feature (Onion versions) |
| Toolchain: static musl armv7 | confirmed feasible; see phase 2 notes in `PROGRESS.md` |
| App manifest | confirmed: `App/<name>/config.json` with `label`, `icon`, `launch`, `description` |
| Boot launcher binaries | confirmed: `.tmp_update/bin/MainUI-{283,354,285}-{clean,expert}` |

## Display

**Framebuffer, size and depth: confirmed.** Onion reads `/dev/fb0` with
`FBIOGET_VSCREENINFO`/`FBIOGET_FSCREENINFO` and maps it with `mmap`
(`Onion:src/common/system/display.h#L109-L142`). The stride is
`xres_virtual * bpp`, and frame `n` starts at `n * yres` rows
(`display_readOrWriteBuffer`, `#L270-L311`). When the runtime resets the mode it
calls `fbset -g <w> <h> <w> <2h> 32` (`Onion:static/build/.tmp_update/runtime.sh`,
`change_resolution`), so the virtual framebuffer is two screens tall at 32 bpp.

Some Mini Plus units have a 752x560 panel (`get_screen_resolution` in
`runtime.sh` reads `/proc/mi_modules/fb/mi_fb0` and touches
`/tmp/new_res_available`). Onion switches to 752x560 only around a game whose
emulator ships a `full_resolution` file and switches back to 640x480 afterwards
(`launch_game`, `change_resolution "640x480"`). The launcher therefore always
starts at 640x480. slot-mini still reads `xres`/`yres` and centres a 640x480
frame in anything larger rather than assuming.

**Rotation: confirmed, 180°.** Onion's screenshot code reads the framebuffer
with `rotate180 = true` (`Onion:src/common/system/screenshot.h#L91-L156`), and
its theme backgrounds are pre-rotated before they are written to the
framebuffer (`Onion:src/common/theme/background.h#L10-L16`). slot-mini writes
its frame into the framebuffer back to front (pixel `(x, y)` goes to
`(w-1-x, h-1-y)`), which costs nothing extra because it is the same copy.

**Page flipping: confirmed.** With `yres_virtual = 2 * yres`, slot-mini draws
into the hidden half and calls `FBIOPAN_DISPLAY` with `yoffset` set to that half.
Onion's own display helper sets `yoffset` through `FBIOPUT_VSCREENINFO`
(`display.h#L75-L77`); `FBIOPAN_DISPLAY` is the standard pan ioctl on the same
`fb_var_screeninfo`. If the pan ioctl fails the backend falls back to drawing
into the visible half (it may tear, but it shows).

**Hand back page 0 on exit.** Onion's own programs assume the first page:
`display_reset()` sets `yoffset = 0` after MainUI is killed, and the in-game
Game Switcher overlay draws through Onion's SDL into page 0. Seen on a device
with 0.1.0: leaving the panel on page 1 made MENU in a game show a black
screen that still took input. slot-mini now copies its last frame to page 0
and pans there before exiting.

**Pixel format.** 32 bpp ARGB8888 little-endian (`0xAARRGGBB` in a `u32`), the
format Onion's display helpers read and write directly as `uint32_t`. slot-mini
writes alpha as `0xFF`.

**`MI_GFX`: optional, not used.** It lives in the proprietary
`/config/lib/libmi_gfx.so`, which a static binary cannot link. Everything is
drawn in software.

## Input

**Device and codes: confirmed.** Onion opens `/dev/input/event0` for buttons
(`Onion:src/detectKey/detectKey.c`, `Onion:src/keymon/input_fd.h`) and the codes
are in `Onion:src/common/system/keymap_hw.h`:

| Button | Linux code | Value |
| --- | --- | --- |
| Up / Down / Left / Right | `KEY_UP` / `KEY_DOWN` / `KEY_LEFT` / `KEY_RIGHT` | 103 / 108 / 105 / 106 |
| A | `KEY_SPACE` | 57 |
| B | `KEY_LEFTCTRL` | 29 |
| X | `KEY_LEFTSHIFT` | 42 |
| Y | `KEY_LEFTALT` | 56 |
| L1 | `KEY_E` | 18 |
| R1 | `KEY_T` | 20 |
| L2 | `KEY_TAB` | 15 |
| R2 | `KEY_BACKSPACE` | 14 |
| SELECT | `KEY_RIGHTCTRL` | 97 |
| START | `KEY_ENTER` | 28 |
| MENU | `KEY_ESC` | 1 |
| POWER | `KEY_POWER` | 116 |
| Volume up / down | `KEY_VOLUMEUP` / `KEY_VOLUMEDOWN` | 115 / 114 |

Events are `struct input_event` with value 0 (released), 1 (pressed) and 2
(repeat). On a 32-bit ARM kernel the struct is 16 bytes: `timeval` (two 32-bit
fields), `u16 type`, `u16 code`, `i32 value`.

**Corrected: keymon reacts to buttons depending on which process it sees.**
Onion's `keymon` runs all the time and classifies the foreground by process name
(`Onion:src/common/system/state.h#L28-L103`, `process_searchpid` in
`Onion:src/common/utils/process.h#L19-L44` is a prefix match on
`/proc/<pid>/comm`). If a process whose name starts with `MainUI` is running and
no command is queued, keymon is in `MODE_MAIN_UI` and:

- MENU kills that process and opens the Game Switcher, or resumes the last game
  (`Onion:src/keymon/menuButtonAction.h`, `action_MainUI_*`);
- every MENU press also injects an L1 press and release ("quietMainUI");
- SELECT injects a MENU press (`Onion:src/keymon/keymon.c#L581-L587`);
- X and Y can run a configured app or tool;
- POWER kills it before shutdown.

Any other process name gives `MODE_APPS`, where keymon leaves MENU, SELECT, X,
Y and L1 alone, short POWER suspends (SIGSTOP), and long POWER deletes the
queued command, sends SIGTERM to everything except `sh`, `runtime.sh`,
`MainUI`, `updater`, `tee`, `audioserver*` and `batmon`, and shuts down
(`keymon.c#L70-L140`, `deepsleep`).

slot-mini needs MENU, SELECT, Y and L1 itself, so **it must never run under a
name starting with `MainUI`**. As an app this is automatic. As the boot
launcher, the shim that sits in MainUI's place re-executes itself through
`/bin/sh` (process name `sh`) before starting slot-mini, so keymon sees `sh`
and `slot-mini` and stays in `MODE_APPS`. The stock MainUI fallback is run as
a file literally named `MainUI`, which keeps keymon's normal behaviour for it
(Open MainUI's wrapper does the same: `OMUI:device/MainUI-test-wrapper.sh`).

## Audio

**Corrected: there is no OSS device to write to.** Stock audio goes through
SigmaStar's `MI_AO`, opened by Miyoo's `audioserver`
(`/mnt/SDCARD/miyoo/app/audioserver`, started by `start_audioserver` in
`runtime.sh`). Programs reach it by preloading
`/mnt/SDCARD/miyoo/lib/libpadsp.so`, which emulates `/dev/dsp`; every launch
command Onion writes starts with `LD_PRELOAD=.../libpadsp.so`. Onion's own
apps and Open MainUI play sounds through SDL_mixer on top of that
(`Onion:src/common/theme/sound.h`, `OMUI:src/platform/audio.c`). `MI_AO`'s
ioctls for volume are known (`Onion:src/common/system/volume.h`), but not the
ones for sending frames, and `audioserver` holds the device anyway.

A static musl binary cannot be preloaded, so slot-mini cannot open the
emulated `/dev/dsp` itself. **Unresolved on hardware.** The simplest path that
uses only what Onion ships is to spawn
`/mnt/SDCARD/.tmp_update/bin/ffplay -nodisp -autoexit -loglevel error <clip>.wav`
with `LD_PRELOAD=/mnt/SDCARD/miyoo/lib/libpadsp.so` (Onion 4.3.1-1 ships
`ffplay` in `.tmp_update/bin/`), from `.tmp_update` as Onion's own video
player app runs it. slot-mini does this for the insert and eject clips, but
it starts with sound **off** (`sounds = off` in its settings) until someone
confirms on a device that it plays promptly and does not hold the audio
device when RetroArch starts. See `DEVICE-TESTING.md`.

**Timing (0.4.7).** slot plays the insert clip `lead` (0.097 s) before the
cart seats (`SEATED_AT`, 0.45 s), so its click lands on the seat, and the
eject clip as the cart starts out; slot-mini now does the same (it used to
play the insert clip as the cart started moving). `ffplay` takes a while to
load its libraries from the card, so a player is started ahead of time and
parked on a FIFO in `/tmp/slot-mini-sfx/` (`-f wav <fifo>`): it loads, then
blocks opening the FIFO, using nothing. A clip is written into it and plays
at once. One is kept ready only while sounds are on and nothing is moving
(shelf, MENU, studio; and at start, for the eject); it runs in its own
process group, is stopped with it when sounds go off or slot-mini exits,
and players left by a crash are stopped the next time one is started.

**Found on a device (0.4.4): `ffplay` could not load its libraries.** The
on-screen sound test read `exit 127: /mnt/SDCARD/.tmp_update/bin/ffplay:
err...`, the dynamic loader's "error while loading shared libraries". In
boot-launcher mode slot-mini runs as MainUI, which `runtime.sh` starts with
`LD_LIBRARY_PATH="$miyoodir/lib:/config/lib:/lib"`; `ffplay`'s ffmpeg
libraries (`libavcodec.so.56` and the rest) are in `.tmp_update/lib/parasyte`,
on the path the runtime exports for everything else
(`/lib:/config/lib:$miyoodir/lib:$sysdir/lib:$sysdir/lib/parasyte`). Since
0.4.5 the player is started with that path (`sound::onion_library_path`).

Earlier notes (0.4.2, 0.4.3: no sound with Sounds on, while games' sound
works): 4.3.1-1's `ffplay` is an old
ffmpeg build (libav*.so.56), dynamically linked against glibc and
`libSDL-1.2.so.0`, so its sound goes through that SDL's audio driver; with
`libpadsp.so` preloaded an OSS (`dsp`) driver would reach `audioserver`.
Since 0.4.4 MENU shows the sound test's outcome on screen. Since 0.4.3, turning Sounds on plays the insert clip at once, and
the player's errors (and a non-zero exit) are written to slot-mini's log, so
the cause can be read off the device. Candidates: the preload or `ffplay`'s
audio output not reaching `audioserver`; in app mode the insert clip racing
slot-mini's exit and RetroArch's start; the eject clip only plays in
boot-launcher mode.

## Power

**Checked against Onion 4.3.1-1 (`keymon.c`, `system.h`, `runtime.sh`).**

- **Screen off (short POWER).** keymon's `suspend_exec` grabs the buttons,
  sends `SIGSTOP` to every user process except shells (slot-mini included),
  mutes, turns the backlight and display off and sets the CPU governor to
  `powersave` with the lowest minimum clock (`system_powersave_on`). It is
  not a hardware sleep: the SoC, RAM and Wi-Fi stay powered. Unless
  `/tmp/stay_awake` exists or Onion's sleep timer is 0, it shuts the
  device down after the sleep timer. Nothing slot-mini does runs while the
  screen is off; Wi-Fi left on is the largest drain then.
- **The governor while the shelf is up.** The runtime sets `ondemand` after
  every game. `cpuclock` (`/dev/mem` and `policy0/scaling_governor`), games
  and the scrapers set `userspace` or `performance` meanwhile. As the boot
  launcher slot-mini can run before anything has put it back; since 0.4.6 it
  sets `ondemand` at start if it finds `performance` or `userspace`
  (`cpufreq::settle`), and logs what it found.
- **slot-mini when idle.** Unchanged frames are not drawn (0.4.2); after
  half a second with no button held (read from the kernel with
  `EVIOCGKEY`, 0.4.6) the loop waits on the button device, 100 ms at a time,
  and after 5 s one second at a time. Measured on the device backend with a
  file framebuffer: about 1 wake-up a second and no measurable CPU.
- **Seeing it.** MENU's bottom line, when it has nothing else to show,
  reads e.g. `CPU ondemand 408 MHz, slot-mini 0.2%` (0.4.6).

## Battery and charging

**Battery level: confirmed.** `batmon` (started at boot) writes the percentage
as a decimal number to `/tmp/percBat` whenever it changes
(`Onion:src/batmon/batmon.c#L85-L125`). Onion's reader treats a missing file as
0 and does not trust values over 100 on the Mini Plus
(`Onion:src/common/system/battery.h#L20-L53`). slot-mini reads the file at most
once every 10 seconds and hides the gauge if it is missing or out of range.

**Charging: from batmon's copy of `axp_test` (0.3.2).** On the Mini Plus,
Onion asks the PMIC by running `/customer/app/axp_test` and parsing its JSON
(`battery.h`, `battery_isCharging`: `"charging":3` means charging). Spawning
that every few seconds would be too costly for the launcher, but batmon already
does it and saves the answer to `/tmp/.axp_result` (`batmon.c`,
`getBatPercMMP`): at every battery check, and straight away when charging
starts or stops (its loop runs every second). slot-mini reads that file with
the battery, every 2 seconds, and shows slot's bolt and a green gauge while it
says `"charging":3`. On the original Mini batmon writes the same file
(`saveFakeAxpResult`), with `"charging":3` while charging. A full battery that
is still plugged in is reported as not charging, so the bolt goes away at
100%.

## Wi-Fi

**Confirmed.** The user's Wi-Fi switch is the `wifi` key (0 or 1) in
`/mnt/SDCARD/system.json` (Onion reads it with `/customer/app/jsonval wifi`,
`runtime.sh`; settings struct in `Onion:src/common/system/settings.h`). The
interface is `wlan0` (`Onion:src/tweaks/tweaks.c`, network scripts under
`.tmp_update/script/network/`). slot-mini shows the Wi-Fi icon when `wifi` is 1
and `/sys/class/net/wlan0/operstate` reads `up`.

## Updates over Wi-Fi

**Written without a device; the end-to-end path is tested against a local
stand-in for GitHub, with the real binary.**

- **Where from.** `GET https://api.github.com/repos/<repo>/releases/latest`,
  which only ever answers the newest full release (never drafts or the
  `main-latest` pre-release). No token: the repo is public, and the
  unauthenticated limit (60 requests an hour per IP) is far more than
  manual checks need. The asset's `browser_download_url` redirects to
  GitHub's asset host; ureq follows it.
- **Trust.** HTTPS with rustls and the bundled webpki roots (as the label
  service), so the device's clock must be roughly right; a certificate
  "expired" or "not yet valid" error is reported as a wrong clock. The zip
  must match the `sha256:` digest GitHub publishes for the asset, and each
  file its zip CRC.
- **Replacing a running binary.** The card is FAT. Renaming a new file over
  the running `slot-mini` drops the old name; the kernel keeps the old file
  alive until the process ends, so nothing breaks. If a rename is refused,
  the old name is removed first. The swap happens on the card, so nothing
  is copied through RAM twice; the zip and its unpacked files (about 10 MB)
  are held in memory once.
- **Restart.** `execv` of `App/slot-mini/slot-mini` with the same arguments:
  same PID, so the boot shim or `launch.sh` waiting on it is none the wiser.
  Descriptors are close-on-exec; the framebuffer is handed back first.
- **The boot shim** lives in the binary. The restarted binary re-installs it
  over `.tmp_update/bin/MainUI-*` if (and only if) the boot launcher is on,
  with the same backup rules as turning it on.

## Launch flow

**Confirmed, with one correction.** `runtime.sh` runs a loop forever
(`main` → `while true; do check_main_ui; check_game_menu; check_game; check_switcher; done`):

1. `check_main_ui`: if `/mnt/SDCARD/.tmp_update/cmd_to_run.sh` does not exist,
   `launch_main_ui` bind-mounts the selected MainUI binary over
   `/mnt/SDCARD/miyoo/app/MainUI` (`mount_main_ui`), `cd`s to
   `/mnt/SDCARD/miyoo/app` and runs `./MainUI` with
   `LD_PRELOAD=libpadsp.so`. Afterwards it **moves `/tmp/cmd_to_run.sh` to
   `/mnt/SDCARD/.tmp_update/cmd_to_run.sh`** (`mv -f`).
2. `check_game`: if that file exists, `launch_game` parses it, handles core
   overrides and the reset/auto-load switches, starts play-activity tracking,
   kills network services, `cd`s to `/mnt/SDCARD/RetroArch` and runs the file.
3. `check_switcher`: unless the Game Switcher or a quick switch was requested,
   deletes the command file, so the next turn starts MainUI again.

The command a launcher writes for a game is one line, exactly as MainUI,
Open MainUI and Onion's random game picker write it
(`OMUI:src/platform/launch.c`, `mainui_launch_command`;
`Onion:src/randomGamePicker/randomGamePicker.c#L425-L433`):

```sh
LD_PRELOAD=/mnt/SDCARD/miyoo/app/../lib/libpadsp.so "/mnt/SDCARD/Emu/GBA/launch.sh" "/mnt/SDCARD/Emu/GBA/../../Roms/GBA/Game.gba"
```

`launch_game` extracts the ROM path as everything after the first `" "`, so
the command must stay on one line and paths may not contain `"`, `` ` ``, `\`,
`:` or control characters. `$` is allowed only inside ROM paths under
`/Roms/`, which the runtime escapes itself. slot-mini refuses to launch a ROM
whose path breaks these rules and says so on screen.

**Launching from an app (not the boot launcher).** When slot-mini runs as an
app, the runtime started it from `cmd_to_run.sh`. An app queues a game the way
AdvanceMENU and the random game picker do: write the command to
`/mnt/SDCARD/.tmp_update/cmd_to_run.sh` and `touch /tmp/quick_switch`, then
exit (`Onion:static/build/.tmp_update/bin/adv/advexec.sh`,
`Onion:static/packages/App/Random Game/App/RandomGamePicker/launch.sh`).
`check_switcher` sees `quick_switch`, keeps the command, and the next turn of
the loop launches it. After that game the loop returns to MainUI, not to the
app: there is no hook to bring an app back. slot-mini remembers the cart it
launched and opens on it the next time it starts.

**As the boot launcher**, slot-mini writes `/tmp/cmd_to_run.sh` like MainUI
and exits; the loop runs the game and then starts the launcher again, so the
player comes straight back to the same cart.

## Recents and the Game Switcher

**Confirmed.** Recents is one JSON object per line, newest first, at
`/mnt/SDCARD/Roms/recentlist.json`, or at `recentlist-hidden.json` when the user
has hidden Recents (`check_hide_recents` in `runtime.sh`). Onion's readers use
the hidden file whenever it exists (`getMiyooRecentFilePath`,
`Onion:src/common/system/state.h#L228-L238`). Nothing in the runtime adds the
launched game: stock MainUI does it before exiting, and Open MainUI does it in
`mainui_recent_add` (`OMUI:src/catalog/saved_actions.c#L296-L399`): put the new
record first, drop older lines for the same ROM, keep at most 50 lines, write
atomically.

The Game Switcher reads `type` (must be 5 or 17), `label`, `rompath`,
`imgpath` and `launch` (`Onion:src/gameSwitcher/gs_history.h#L23-L74`). Stock
records spell paths with the emulator folder and its `rompath`
(`OMUI:docs/STOCK_RECORD_PATHS.md`). slot-mini writes:

```json
{"label":"Game","rompath":"/mnt/SDCARD/Emu/GBA/../../Roms/GBA/Game.gba","imgpath":"/mnt/SDCARD/Emu/GBA/../../Roms/GBA/Imgs/Game.png","launch":"/mnt/SDCARD/Emu/GBA/launch.sh","type":5}
```

to whichever of the two files Onion's readers would use, so the Game Switcher
and "resume last game" see the launch.

## Paths

**Confirmed.**

| What | Path | Source |
| --- | --- | --- |
| ROMs | `/mnt/SDCARD/Roms/{GBA,GB,GBC}` | `rompath` in each `Emu/<sys>/config.json` (`../../Roms/GBA` etc.) |
| Extensions | GBA `bin gba zip 7z`; GB and GBC `bin dmg gb gbc zip 7z` | `extlist` in the same files |
| Launch scripts | `/mnt/SDCARD/Emu/{GBA,GB,GBC}/launch.sh` | `Onion:static/packages/Emu/Nintendo - */Emu/*/launch.sh` |
| RetroArch | `/mnt/SDCARD/RetroArch/retroarch`, cores in `.retroarch/cores/` | the launch scripts |
| Saves | `/mnt/SDCARD/Saves/CurrentProfile/saves/` | GBA `launch.sh` |
| Save states | `/mnt/SDCARD/Saves/CurrentProfile/states/<core>/` | GBA `launch.sh` |
| Onion system dir | `/mnt/SDCARD/.tmp_update` | `runtime.sh` |
| Per-game core override | `Roms/<sys>/.game_config/<stem>.cfg` | `launch_game`, `override_game_core` |

The GB, GBC and GBA emulators are Onion packages and may not be installed. A
platform whose `Emu/<sys>/launch.sh` is missing gets no shelf.

## MENU in a game

**Checked against Onion 4.3.1-1 (`v4.3.1-1`) and its development branch.**

- **What Onion does.** keymon handles MENU while RetroArch runs
  (`src/keymon/menuButtonAction.h`). On released Onion every in-game MENU
  action (Game Switcher, exit to menu, quick switch) saves a screenshot,
  then ends RetroArch with `SIGTERM`; RetroArch writes its auto state on the
  way out. The Game Switcher then starts as a separate program and resuming
  relaunches RetroArch, which is the delay players see. The development
  branch pauses RetroArch with `PAUSE`/`UNPAUSE` network commands its own
  RetroArch build adds, and draws the Game Switcher as an overlay.
- **Taking MENU over.** keymon skips its MENU handling while
  `/tmp/disable_menu_button` exists (`keymon.c`). Its MENU combinations are
  tracked before that check, so they keep working.
- **Hotkeys.** MENU is RetroArch's and Onion's hotkey button in games (MENU
  + SELECT RetroArch's menu, + L rewind, + R fast forward, + L2 load state,
  + R2 save state, + Left/Right save slot, + X FPS, + POWER screenshot;
  Onion's "Global Shortcuts"). The helper treats MENU as a combination when
  any other button (the volume keys included) is down when MENU goes down
  or is pressed before it comes up; on the device it reads which buttons
  are down from the kernel (`EVIOCGKEY`) at the start of each batch of
  events. Tap and hold are both decided when MENU is released (0.4.7; the
  hold used to fire at 0.7 s while MENU was still down, which ejected a
  game when MENU was held a moment before the rest of a hotkey).
- **RetroArch's network commands** are on in Onion's default config
  (`network_cmd_enable = "true"`, UDP port 55355). Standard RetroArch
  commands are used: `GET_STATUS`, `PAUSE_TOGGLE`, `SAVE_STATE`,
  `LOAD_STATE`, `MENU_TOGGLE`. Quitting uses `SIGTERM`, as keymon does.
- **Drawing over the game.** Onion freezes RetroArch with `SIGSTOP` to read
  the framebuffer for its screenshots (`screenshot.h`). slot-mini pauses with
  `PAUSE_TOGGLE` (RetroArch stops its audio), then `SIGSTOP`s it so it
  cannot redraw, and draws on the page `FBIOGET_VSCREENINFO` says is shown.
- **RetroArch may use more than two pages** (seen on a device with 0.2.0:
  the menu sometimes stayed invisible but took input). Its video driver can
  triple-buffer, and the panel may be showing a page past the second, or
  `yoffset` may lag the page about to be shown. slot-mini re-reads the
  geometry, then draws the menu on every page the framebuffer memory holds
  and restores each one when the menu closes.
- **Buttons reach every reader of `/dev/input/event0`.** RetroArch reads the
  same device, so a frozen RetroArch still saw the menu's presses queued
  when it thawed (seen on a device: A on Resume also pressed A in the game).
  While its menu is open slot-mini takes the device with `EVIOCGRAB`, as
  keymon does, and acts on a row only once every button is released, so no
  half of a press is left for the game. While grabbed keymon sees no
  buttons either, so POWER in the menu resumes the game.
- **Not reproduced:** Onion's Game Switcher screenshot (`romScreens/<hash>.png`).
  Its name is a hash that can read past the end of the path string, so it
  cannot be computed exactly. Games left through slot-mini's menu may show an
  older picture in the Game Switcher.

## Cores

**Checked against Onion 4.3.1-1 (tag `v4.3.1-1`; development branch noted).**

- **What is installed.** `RetroArch/.retroarch/cores/` ships both
  `mgba_libretro.so` and `gpsp_libretro.so`.
- **The default.** 4.3.1-1's GBA package is "Nintendo - GBA (mGBA)": its
  `Emu/GBA/launch.sh` runs `mgba_libretro.so` (and once offered to import gpSP
  saves). The development branch's package is "Nintendo - GBA (gpSP)", with
  `default_core=gpsp` in `launch.sh`. Game List Options reads the default
  from a `default_core=` line, else from the `.retroarch/cores/<name>.so` in
  the script (`get_core_info`); slot-mini does the same.
- **A per-game core** is `<rom folder>/.game_config/<rom name>.cfg` with
  `core = "gpsp_libretro"`. `<rom name>` is `basename "$rompath" ".$romext"`
  with the extension lower-cased (so a `.GBA` file keeps it). Game List
  Options' "Game core" writes it (dropping any other `core ` line); picking
  the default, or "Restore default core", deletes the file and, if empty, the
  folder (`rm -d`).
- **When it applies.** The runtime's `launch_game` runs `override_game_core`
  for every game whose `.cfg` exists: unless the command already names a core
  (Game Switcher resumes), it is replaced by RetroArch run directly with that
  core. slot-mini's commands go through `launch.sh`, so the override applies
  to them; Hold A names the core itself (`core_for`).
- **Saves are per core.** 4.3.1-1's `retroarch.cfg` has
  `sort_savefiles_enable` and `sort_savestates_enable`, so saves and states
  go into a folder per core, named after the core
  (`Saves/CurrentProfile/saves/mGBA/<rom>.srm`, `.../gpSP/`, and the same
  under `states/`), `<rom>` being the file name without its extension.
- **Carrying a save.** In-game saves are the same format in both cores:
  4.3.1-1's mGBA `launch.sh` imported gpSP saves by copying them (renaming
  `.sav` to `.srm`), and the development branch's `transfer_mgba_save.sh`
  copies an mGBA `.srm` to gpSP unchanged. That script treats an all-`00`
  or all-`ff` file as empty, keeps a non-empty save it replaces as
  `.bak_<YYYYmmddHHMMSS>`, and sets the target core's `.state.auto` aside,
  because states do not carry over and an old auto state would resume an
  older session. slot-mini does exactly this when its core picker changes a
  game's core (`cores::carry_save`), before writing the new core.

## Resume and fresh start

**Corrected twice; the second time on a device.** Onion's launch scripts load
and write the auto state through RetroArch's own settings. A one-off fresh
start works differently in Onion's releases and on its development branch:

- **Released Onion (4.3.1-1, checked against tag `v4.3.1-1`):** Game List
  Options' "Reset game" writes `savestate_auto_load = "false"` and
  `config_save_on_exit = "false"` to `.tmp_update/reset.cfg`, then replaces
  the command with RetroArch run directly:
  `LD_PRELOAD=/mnt/SDCARD/miyoo/lib/libpadsp.so ./retroarch -v --appendconfig "/mnt/SDCARD/.tmp_update/reset.cfg" -L "<core>" "<rom>"`.
  The core is the per-game override in `<rom folder>/.game_config/<rom name>.cfg`
  (`core = "..."`), else the one named after `.retroarch/cores/` in the
  system's `launch.sh` (`get_core_info`). The runtime has **no**
  `/tmp/reset_game` flag.
- **Development branch (`07505ea`, read first):** `/tmp/reset_game` makes
  `override_game_core` inject `--appendconfig "/tmp/reset.cfg"` into the
  launch script for one run, and its GBA script picks the core at run time
  (`${default_core}`).

slot-mini 0.1.0 used only the development branch's flag, so **Hold A started
from the auto state on released Onion** (reported on a device). Since 0.1.1,
Hold A writes the released "Reset game" command whenever the core can be read
from the launch script and its `.so` is on the card, and falls back to
`/tmp/reset_game` otherwise. Tap A writes the normal command and clears any
stray flag. Since 0.5.0 slot-mini uses the flag whenever the runtime has it
(Onion versions), and the released command otherwise.

slot-mini never touches save files or states. RetroArch's auto-save on exit
is not turned off by `reset.cfg`, so quitting a fresh start writes a new auto
state, just as Onion's own "Reset game" does. The fresh-start command runs
RetroArch directly, so like Onion's "Reset game" it skips anything else the
system's `launch.sh` does (for example the mGBA save-import prompt).

## Apps and the boot launcher

**App manifest: confirmed.** An Onion app is a folder in `/mnt/SDCARD/App/`
with a `config.json` holding `label`, `icon`, `launch` and `description`, and
the launch script beside it
(`Onion:static/packages/App/Random Game/App/RandomGamePicker/config.json`).

**MainUI binaries: confirmed.** `mount_main_ui` bind-mounts
`/mnt/SDCARD/.tmp_update/bin/MainUI-$DEVICE_ID-{clean,expert}` over
`/mnt/SDCARD/miyoo/app/MainUI` (`expert` when
`.tmp_update/config/.showExpert` exists). `DEVICE_ID` is 283 (Mini), 354
(Mini Plus); Open MainUI's guide adds 285 (Mini Flip)
(`OMUI:docs/BUILDING.md`, "Choose the launcher file"). Replacing
`miyoo/app/MainUI` itself does nothing because the mount hides it.
`mount_main_ui` re-mounts when the mounted file's name changes; if a file is
replaced by rename while mounted, `/proc/self/mountinfo` reports it as
deleted, so the next turn of the loop mounts the new file.

**What survives an Onion update.** A fresh Onion install deletes `App/`, `Emu/`,
`RApp/` and `miyoo/` (`Onion:static/dist/miyoo/app/.tmp_update/install.sh#L245-L253`);
an update unpacks over `.tmp_update/` and replaces its own files, including the
MainUI binaries. So the boot shim can disappear (Onion is back, fine), and
`App/slot-mini/` can disappear while the shim stays (the shim must cope).
Since 0.5.0 slot-mini puts the shim back the next time it starts
(Onion versions).

## Onion versions

**Checked against 4.3.1-1 (tag `v4.3.1-1`) and the development build
(`main` at `07505ea`).** slot-mini 0.5.0 supports both on the same card.

- **What the development build is.** Onion's updater (Tweaks, "Check for
  updates") has a stable and a beta channel. Beta installs the GitHub
  release tagged `latest`, which Onion's CI rebuilds from `main`. The update
  is a `7z x -aoa` over the card followed by a reboot: Onion's own files are
  replaced, `App/slot-mini/` and the rest of the card are left alone.
- **Told apart by what they do, not by number.** The development build's
  version string changes with every build, so slot-mini looks for the two
  things it uses (`onion.rs`): `/tmp/reset_game` in `.tmp_update/runtime.sh`
  (the runtime's fresh-start flag) and `--overlay` in
  `.tmp_update/bin/gameSwitcher` (the overlay Game Switcher). 4.3.1-1 has
  neither. The version file is only logged.
- **Hold A (fresh start).** On the development build slot-mini touches
  `/tmp/reset_game` and writes the normal command; the runtime's
  `override_game_core` adds `--appendconfig /tmp/reset.cfg` (auto load off,
  no config save) to that one run, honouring a per-game core, and removes
  the flag afterwards. It needs the launch script to name
  `.retroarch/cores`, which Onion's GBA, GB and GBC scripts do. 4.3.1-1 keeps
  the direct command with `.tmp_update/reset.cfg` (Resume and fresh start).
- **Default GBA core.** The development build's GBA package is gpSP
  (`default_core=gpsp`). slot-mini reads the default from the installed
  `launch.sh`, so games without their own core run on gpSP there and on mGBA
  on 4.3.1-1, with no setting of its own. Onion's GBA script copies an mGBA
  save to gpSP the first time a game runs on gpSP
  (`transfer_mgba_save.sh`), and the core picker carries saves both ways as
  before.
- **In-game Game Switcher.** On the development build keymon's Game
  Switcher pauses RetroArch, touches `.tmp_update/.runGameSwitcher` and runs
  `gameSwitcher --overlay`, which draws over the paused game, resumes it with
  `UNPAUSE`, or quits it to switch games, then removes the flag. slot-mini's
  "Game Switcher" row does the same: it thaws RetroArch (still paused), hands
  the screen and buttons over, and stays out of the way until the overlay
  exits, then touches `/tmp/state_changed` as keymon does. On 4.3.1-1 the row
  still quits the game and lets the runtime start the Game Switcher.
- **Unchanged between the two.** `/tmp/disable_menu_button`, keymon's MENU
  combinations, RetroArch's network commands on port 55355, `batmon`'s
  `/tmp/percBat` and `/tmp/.axp_result`, the MainUI binary names, the
  Recents format, the per-game core file and its location, per-core save
  folders, and the libraries MainUI is started with (which `ffplay` needs).
- **The boot launcher across an Onion update.** The update replaces
  `.tmp_update/bin/MainUI-*`, so Onion's menu is back after it. slot-mini
  records that the boot launcher is on (`boot_launcher` in its settings);
  the next time it starts (Apps -> slot-mini) it finds Onion's new binaries,
  keeps them as the backups and puts the shim back, unless the shim's
  `DISABLED` file says the player turned it off at boot. The old backups are
  replaced, so turning the launcher off afterwards restores the updated
  Onion, not the old one.

## Boot screen

**Checked against 4.3.1-1 and the development build (`07505ea`); the same on
both.** The runtime runs `bootScreen Boot` at start (and again with messages
such as "Syncing time..."), which draws `skin/extra/bootScreen.png` full screen
and keeps it up until the launcher draws. `theme_getImagePath` takes the first
of `Saves/CurrentProfile/theme/skin/extra/bootScreen.png` (the profile's theme
overrides), `<theme>/skin/extra/bootScreen.png` and
`.tmp_update/res/bootScreen.png`. The image is 640x480 (the development build
scales it on larger panels). Over it Onion always writes
`onionVersion/version.txt` bottom left, and any message bottom right, in the
theme's font and colour.

- **Who touches the overrides.** Tweaks' "Reset theme overrides" runs
  `rm -rf Saves/CurrentProfile/theme/*`; Guest Mode swaps the whole profile.
  Onion's updates do not: its default configs (`configs.pak`) are only
  unpacked on a fresh install or a reset.
- **The Miyoo logo before it** is the bootloader's, in the device's flash.
  Changing it means flashing; slot-mini does not.
- **slot-mini** (0.5.1, `bootscreen.rs`) writes slot's RG SP boot logo
  composed as slot composes it (`bootlogo::compose`: the 196x75 wordmark,
  unscaled, centred on its own corner colour `#0b0e11`) as a PNG marked with a
  `tEXt` chunk, to the overrides. A file there that is not its own is copied
  to `.tmp_update/slot-mini/bootScreen.png` first and put back when it is
  turned off; another file there is never removed. With the setting on, a
  missing file is written again at start.

## Toolchain

**Confirmed feasible.** The Rust target `armv7-unknown-linux-musleabihf`
ships its own musl and CRT objects, so a fully static binary can be linked with
`rust-lld` and no ARM C toolchain; C code pulled in by dependencies (`ring`
for TLS) is compiled with `clang --target=armv7-unknown-linux-musleabihf`.
Flags: `-C target-cpu=cortex-a7 -C target-feature=+neon,+vfp4 -C
target-feature=+crt-static`. Every build is checked with `file`. Onion's
Docker toolchain (`aemiii91/miyoomini-toolchain`, glibc) is not needed.

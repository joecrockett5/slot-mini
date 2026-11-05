# Testing slot-mini on a Miyoo Mini Plus

slot-mini was built without a device. This checklist is for the first person
to run it on real hardware. It goes from the safest checks (slot-mini as an
ordinary app, nothing else on the card changed) to the boot switch, which
replaces Onion's main menu. Do it in order and stop at the first failure.

**Before you start**

- [ ] Use a **spare SD card** cloned from a working Onion install, or back up
      the whole card. Keep a computer with a card reader nearby.
- [ ] Note the Onion version (Apps -> Tweaks -> About, or
      `.tmp_update/onionVersion/version.txt`), and whether your Mini Plus has
      the 640x480 or the 752x560 panel.
- [ ] Turn on Onion's logging: create an empty file
      `.tmp_update/config/.logging`. slot-mini then logs to
      `.tmp_update/logs/slot-mini.log` (app mode) and Onion's own logs keep
      going to `.tmp_update/logs/`.
- [ ] Make sure the GBA, GB and GBC emulators are installed (Package Manager),
      and that each of `Roms/GBA`, `Roms/GB`, `Roms/GBC` has a few games,
      including one in a subfolder and one zipped.
- [ ] Unzip the release at the card root. Check that only `App/slot-mini/`
      appeared.

For each failure, note what you did, what you saw, and copy the logs.

## 1. App mode: it starts and draws

Start **Apps -> slot-mini**.

- [ ] It starts within about a second. The log's first line says
      `slot-mini: first frame at N ms`; note N (budget: under 1000 ms).
- [ ] The picture is **the right way up** (shelf at the top, footer at the
      bottom). Upside down means the 180° rotation in `fbdev.rs` is wrong for
      this unit.
- [ ] Colours are right: wood-brown wallpaper, not blue-tinted (red and blue
      swapped means the pixel order is BGRA, not ARGB).
- [ ] No tearing or flicker while idle. (Tearing while scrolling is noted in
      section 3.)
- [ ] On a 752x560 panel: the 640x480 picture is centred with a black border.
- [ ] The footer shows the battery percentage and the right time. If the time
      is off by hours, note the timezone set in Onion.
- [ ] Plug the charger in: within a few seconds a bolt appears left of the
      battery and its fill turns green. Unplug: both go back.
- [ ] With Wi-Fi on and connected, a Wi-Fi icon appears after the battery.

## 2. App mode: buttons

- [ ] Left / Right move one cart; holding scrolls with slot's repeat.
- [ ] Up / Down jump between first letters.
- [ ] L1 / R1 switch GBA, GBC, GB shelves, with the system name shown briefly.
- [ ] **MENU opens slot-mini's About screen**, not Onion's Game Switcher.
      (If Onion's switcher or menu appears, keymon is treating slot-mini as
      MainUI: see PLATFORM.md, Input.)
- [ ] **SELECT opens the cart studio**, and no MENU press is injected.
- [ ] **Y hearts the cart** (a heart on the label); a Favorites shelf appears
      first. Y again removes it.
- [ ] Short POWER press turns the screen off; pressing again comes back to
      slot-mini where it was.
- [ ] Volume buttons still change the volume (Onion's overlay).

## 3. App mode: speed and memory

- [ ] Hold Right across a shelf of 100+ games. Scrolling is smooth (60 fps,
      no stutter when carts come to rest). Note any hitches.
- [ ] With 100+ labels the first start after adding them is slower (labels are
      cached in `App/slot-mini/cache/labels/`); the second start is fast.
- [ ] Check memory while scrolling: from a shell (Onion's terminal or SSH),
      `grep VmHWM /proc/$(pidof slot-mini)/status`. Budget: under 40 MB.

## 4. App mode: launching

- [ ] **Tap A** on a GBA game: the cart drops into the slot, then the game
      starts, resuming its auto save state if it has one.
- [ ] Quit the game (MENU in RetroArch, or Onion's MENU shortcut). Onion
      returns to its own menu (expected in app mode). Open slot-mini again:
      it opens on the same cart.
- [ ] Onion's **Game Switcher** and **Recents** list the game you launched,
      with the right name, and resuming it from there works.
- [ ] **Hold A** (half a second) on a game with an auto save: it starts from
      the beginning. Quit and start it with Onion's own menu: the next normal
      launch auto-loads again (the fresh start was for one run).
- [ ] Repeat tap A for one GB and one GBC game, a game in a subfolder and a
      zipped game.
- [ ] A game whose file name has a `:` in it shows "Rename this file to launch
      it" and does not launch.
- [ ] Save files and states are untouched by slot-mini: compare
      `Saves/CurrentProfile/states/` before and after.

## 5. App mode: studio, labels, sounds

- [ ] SELECT -> Shell Color: pick a colour, B back. The shelf shows the new
      shell and `Labels/cart_shell.txt` has a line for the game.
- [ ] Finish: Clear and Glitter look right on the shelf. For a Game Boy cart,
      Shape switches notched and rounded.
- [ ] Label Printer with the label service off says to use Cart Studio.
- [ ] Copy a label made in Cart Studio to `Labels/GBA/<rom name>.png`; restart
      slot-mini: the cart shows it.
- [ ] MENU -> Sounds: On. Insert a cart: slot's insert sound plays promptly.
      Come back in boot-launcher mode later and check the eject sound.
      **Important:** the game's own sound works normally after an insert sound
      (the sound player must not hold the audio device). If it does not, turn
      Sounds off and report it.
- [ ] (Optional, at your discretion) `label_service = on` in
      `App/slot-mini/data/settings.txt` with Wi-Fi up: carts without labels
      get them in the background, and the printer offers two labels.

## 6. MENU -> Exit to Onion

- [ ] In app mode, Exit to Onion returns to Onion's menu.

Everything above changes nothing outside `App/slot-mini/` (and `Labels/`,
`Roms/recentlist.json` as described). **Stop here if anything failed.**

## 7. The boot switch

Have your computer and card reader ready. If the device ever fails to show a
launcher, the fix is in section 9.

- [ ] MENU -> Boot launcher: On. The screen says it was installed. Check on
      the card later: `.tmp_update/slot-mini/stock/MainUI-354-clean/MainUI`
      (and the other variants Onion has) exist, and the files in
      `.tmp_update/bin/MainUI-*` start with `#!/bin/sh` and
      `# slot-mini boot shim`.
- [ ] Leave the app with MENU -> Exit to Onion. Onion now starts its
      launcher, which is the shim: slot-mini should appear straight away as
      the boot launcher. (If Onion's menu shows once instead, its bind mount
      kept the old file; that is fine, note it. Onion re-mounts when the
      mounted file was replaced, see PLATFORM.md, Apps and the boot launcher.)
- [ ] Power off and on: slot-mini comes up after Onion's boot.
- [ ] **MENU is still slot-mini's About screen** in boot-launcher mode (not
      Onion's Game Switcher). Same for SELECT and Y.
- [ ] Tap A on a game, play, quit: you come straight back to slot-mini on the
      same cart, with the cart rising out of the slot and the eject sound
      (the screen is black for a moment first while RetroArch saves and
      slot-mini starts).
- [ ] Onion's "resume last game at boot" still works: start a game, power off
      from inside it, power on.
- [ ] Hold **L2** while slot-mini is about to start (e.g. while quitting a
      game): Onion's menu appears instead. Its MENU button works as normal
      there (Game Switcher).
- [ ] MENU -> Exit to Onion shows Onion's menu; after its next launch, slot-mini
      is back.
- [ ] Long-press POWER in slot-mini: the device shuts down cleanly (no Onion
      menu flashes up first).
- [ ] Toggle Expert mode in Onion's Tweaks: slot-mini still starts.

## 8. Fallbacks

- [ ] With the card in a computer, create `.tmp_update/slot-mini/DISABLED`.
      Boot: Onion's menu. Delete it: slot-mini again.
- [ ] Rename `App/slot-mini/slot-mini` to something else. Boot: Onion's menu.
      Rename it back.
- [ ] Crash test: replace `App/slot-mini/slot-mini` with a file containing
      `#!/bin/sh` and `kill -SEGV $$`. Boot: Onion's menu. Put the real binary
      back.

## 9. Restore

- [ ] MENU -> Boot launcher: Off. The next launcher start is Onion's menu, and
      every `.tmp_update/bin/MainUI-*` is byte-identical to its backup in
      `.tmp_update/slot-mini/stock/`.
- [ ] Delete `App/slot-mini/`. Onion behaves exactly as before.

**If the device does not show a launcher at any point:** put the card in a
computer and copy each `.tmp_update/slot-mini/stock/<name>/MainUI` back over
`.tmp_update/bin/<name>` (keep the name, no extension). If the backups are
missing, re-copy `.tmp_update/bin/MainUI-*` from Onion's release zip. Then
report exactly what happened before it, with `.tmp_update/logs/`.

## 10. slot's in-game MENU (0.2.0)

Start a game from slot-mini (app mode is fine).

- [ ] Tap MENU: the game freezes and slot's menu appears over it almost
      immediately. Note roughly how long it takes compared with Onion's menu.
- [ ] The game's sound stops while the menu is up (no buzz or loop).
- [ ] The menu stays put: the game does not flicker through it.
- [ ] Open and close the menu ten times in a row, in a couple of games: it
      shows every time (0.2.0 sometimes left it invisible).
- [ ] Resume with A, then Save State with A: the game does not see an A
      press (0.2.0 passed it through). POWER in the menu resumes the game.
- [ ] B (or MENU again) resumes exactly where you were.
- [ ] Save State, then later Load State: RetroArch shows its own "saved" or
      "loaded" message, and the load returns to the saved moment.
- [ ] Settings opens RetroArch's menu; leaving it resumes the game.
- [ ] Game Switcher opens Onion's Game Switcher.
- [ ] Exit Game, and separately holding MENU for about a second: the game
      saves and you are back on the shelf (straight away in boot-launcher
      mode; via Onion's menu in app mode). Tap A resumes from that point.
- [ ] MENU + another button (Onion's shortcuts, such as brightness) still
      does Onion's shortcut and does not open the menu.
- [ ] After leaving the game, MENU in Onion's own menu works as before
      (opens Onion's Game Switcher).
- [ ] A game started from Onion's own menu still has Onion's MENU.
- [ ] MENU -> In-game menu: Off in slot-mini gives MENU back to Onion in
      games launched from slot-mini.

## 11. Updates over Wi-Fi (0.3.0)

Needs a numbered release newer than the version on the card. If there is
none, install the previous release's zip by hand first.

- [ ] Wi-Fi off: MENU -> Check for updates says "No connection. Is Wi-Fi on?"
- [ ] Wi-Fi on: it says "Up to date" or offers "Install update x.y.z" within
      a few seconds.
- [ ] Install: "Downloading" counts up, then "Installing", then "Updated.
      Restarting", and slot-mini comes back on its own showing the new version
      in MENU. Note how long it took.
- [ ] Settings, favourites and the cart you were on are unchanged.
      `App/slot-mini/update/previous/` holds the old binary.
- [ ] Do the same in boot-launcher mode. Afterwards power off and on:
      slot-mini still starts, and `.tmp_update/bin/MainUI-*` still start with
      `# slot-mini boot shim`.
- [ ] If the device clock is far off (e.g. year 2000), the check says the
      clock is wrong instead of failing silently.

## 12. Core picker (0.4.0)

- [ ] START on a GBA game: the other carts dim, the cart opens and the chip
      sits on mGBA (or on the core Onion's Game List Options shows). Smooth?
- [ ] Right, then A: the chip hops to gpSP, the cart closes, "gpSP keeps its
      own saves" shows. Onion's Game List Options for that game says gpSP.
- [ ] Tap A on that game: it runs on gpSP (RetroArch's menu, Information,
      Core Information), and has no mGBA save, as expected.
- [ ] START, Left, A: back to mGBA; `Roms/GBA/.game_config/<game>.cfg` is gone
      and the game has its mGBA save again.
- [ ] START, Right, B: nothing changes.
- [ ] START on a GB or GBC game does nothing.

## What to report

The Onion version, the panel size, which boxes failed, the first-frame time,
VmHWM, anything that looked unlike slot, and the logs.

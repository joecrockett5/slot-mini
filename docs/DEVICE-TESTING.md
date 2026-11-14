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
- [ ] MENU's bottom line shows `CPU <governor> <MHz>, slot-mini <n>%` (0.4.6).
      On an idle shelf expect `ondemand`, a low clock and well under 1%.
      Note what it says if the handheld runs warm.
- [ ] Leave slot-mini on the shelf for 10 minutes: the handheld stays cool
      and the battery barely moves. From a shell, `top` shows slot-mini near
      0% CPU while idle. Scrolling afterwards is as smooth as before, and
      the clock still changes on the minute.
- [ ] Check memory while scrolling: from a shell (Onion's terminal or SSH),
      `grep VmHWM /proc/$(pidof slot-mini)/status`. Budget: under 40 MB.

## 4. App mode: launching

- [ ] **Tap A** on a GBA game: the cart drops into the slot, then the game
      starts, resuming its auto save state if it has one.
- [ ] Quit the game (MENU in RetroArch, or Onion's MENU shortcut). Onion
      returns to its own menu (expected in app mode). Open slot-mini again:
      it opens on the same cart.
- [ ] A **Recents** shelf is now first (L1 from Favorites or GBA), with the
      game you played at the front; X takes a cart off it; **MENU →
      Recents: Off** hides the shelf and **On** brings it back as it was.
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
- [ ] MENU -> Sounds: On plays slot's insert sound at once (0.4.3). Within
      a few seconds MENU's bottom line says how the player ended (0.4.4):
      note it exactly, whether or not you heard anything. With Onion's
      logging on, `.tmp_update/logs/slot-mini.log` has the player's full
      output (`slot-mini: sound:` lines).
- [ ] Insert a cart: slot's insert sound's click lands as the cart seats in
      the slot (0.4.7), not before or well after.
      Come back in boot-launcher mode later and check the eject sound.
      **Important:** the game's own sound works normally after an insert sound
      (the sound player must not hold the audio device). If it does not, turn
      Sounds off and report it.
- [ ] With Wi-Fi up and MENU -> Settings -> Online labels on (the
      default from 0.6.4): carts without labels get them in the background,
      and the printer offers two labels.

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
- [ ] RetroArch hotkeys (0.4.7): MENU + R2 saves, MENU + L2 loads, MENU + R
      fast-forwards, MENU + L rewinds, MENU + SELECT opens RetroArch's menu.
      None opens slot's menu or ejects, also when MENU is held a second or
      two before the other button, or the other button is pressed first.
- [ ] Hold MENU about a second and let go: the game ejects (on release).
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
- [ ] On a game with an in-game save made on mGBA: Right, then A. The chip
      hops to gpSP, the cart closes, "Save carried over to gpSP" shows.
      Onion's Game List Options for that game says gpSP.
- [ ] Tap A on that game: it runs on gpSP (RetroArch's menu, Information,
      Core Information) and starts from the title screen, and Continue loads
      the same save as on mGBA.
- [ ] Save in the game on gpSP, quit. START, Left, A: back to mGBA, "Save
      carried over to mGBA"; `Roms/GBA/.game_config/<game>.cfg` is gone and
      the game has the newer save. `Saves/CurrentProfile/saves/mGBA/` holds
      the older one as `<game>.srm.bak_<time>`.
- [ ] START, Right, B: nothing changes.
- [ ] START on a GB or GBC game does nothing.

## 13. Onion's development build (0.5.0)

Update slot-mini to 0.5.0 on 4.3.1-1 first, then Onion (beta channel).

- [ ] After Onion's update and reboot, Onion's menu shows. Apps -> slot-mini:
      "Boot launcher back on after Onion's update". Quit to Onion's menu
      (MENU -> Exit to Onion) and reboot: slot-mini starts.
- [ ] A GBA game with no core of its own runs on gpSP (RetroArch's menu,
      Information, Core Information), with its mGBA save. START shows the
      chip on gpSP.
- [ ] Hold A: the game starts from the title screen, not where you left it.
      Tap A next time: it resumes as normal.
- [ ] In a game, MENU -> Game Switcher: Onion's Game Switcher draws over the
      game. B (or A on the same game) resumes it where it was; picking
      another game starts that one. MENU works again after resuming.
- [ ] MENU hotkeys, sounds and the core picker as in sections 10 to 12.
- [ ] With Onion's logging on, `.tmp_update/logs/slot-mini.log` has a line
      `Onion ... (reset flag: yes, switcher overlay: yes)`.

## 14. slot's boot and shutdown screens (0.5.1, 0.5.2)

- [ ] MENU -> Splash screens: slot. Reboot: "slot." in the middle of a dark
      screen while Onion starts, with Onion's version bottom left.
- [ ] Hold power on the shelf: "POWERING DOWN" in the middle of a black
      screen, Onion's battery top right, then off.
- [ ] Start a game, hold power: the same screen.
- [ ] Change theme in Onion: the screens stay slot's.
- [ ] MENU -> Splash screens: Onion. Reboot and power off: the theme's
      screens are back.

## 15. Shelf wallpapers (0.6.1)

Put two pictures in `Wallpapers/` at the SD root: any name (say
`beach.png`) and `GBC.png`.

- [ ] The GBA shelf shows `beach.png`; R1 to GBC shows `GBC.png`; back to GBA
      shows `beach.png` again. Restart a few times: `GBC.png` never shows on
      GBA.
- [ ] Leave slot-mini on the GBC shelf, launch a game and quit it: the first
      frame back already has `GBC.png` (no flash of the other wallpaper).
- [ ] MENU on the GBC shelf shows `GBC.png` dimmed behind the rows.
- [ ] With a picture for every shelf (`GBA`, `GBC`, `GB`, `Favorites`,
      `Recents`), the first frame still comes within about a second and
      resident memory (section 3) stays under 40 MB. Note both.

## 16. Label printer animation (0.6.2)

Needs Wi-Fi and MENU -> Settings -> Online labels on (README, label printer).

- [ ] SELECT on a GBA cart, A on the printer row: the light turns blue while
      fetching, then green; the slips print and drop one after the other.
- [ ] Down picks the lower slip; it lifts and zooms smoothly.
- [ ] A: the old label peels off and tumbles away, the new label lifts off its
      slip, flies over and rolls onto the cart. Note whether the curl stutters
      (the strips drawn per frame are decision 59's concern).
- [ ] B during the scene does nothing; after it, B closes the studio and the
      shelf shows the new label.

## 17. The MENU sticker (0.6.3)

- [ ] MENU: slot's sticker peels in from the left at the top, then lies
      flat. Note whether the peel stutters.
- [ ] The sticker shows the battery level and this version; plug in or let
      the battery drop a step and reopen MENU: the number follows.
- [ ] All six Settings rows and the note line clear the footer.

## 18. MENU's Settings page and online labels (0.6.4)

- [ ] MENU shows three rows: Settings, Check for updates, Exit to Onion.
      A on Settings shows Sounds, In-game menu, Recents, Boot launcher,
      Splash screens and Online labels; B goes back to the three rows, and B
      again (or MENU from either page) closes it.
- [ ] Online labels reads On after the update, even on a card whose
      `settings.txt` already says `label_service = off`.
- [ ] With Wi-Fi up, a cart with no file in `Labels/` gets a label within a
      minute or so of starting slot-mini (it shows next time the shelf
      draws it); carts that already have a label file keep it. Note how
      long it took and whether scrolling stuttered meanwhile.
- [ ] SELECT, A on the printer row: two slips print. Apply one.
- [ ] Online labels: Off, restart slot-mini: nothing more is fetched, and
      the printer points to Cart Studio.

## 19. Sync with slot-mini PC (0.7.0)

Needs a Windows PC running slot-mini PC 0.2.0 or later on the same Wi-Fi.
Do these in order: each step risks a little more than the one before.
Before any of it, copy `Saves/CurrentProfile/` off the card.

**Unpaired (nothing may have changed)**

- [ ] After the update, without pairing: MENU has no Sync now row, the
      footer has no new icon, `App/slot-mini/data/sync/` does not exist,
      and a game starts and ejects exactly as before. Note the
      first-frame time and compare it with section 3's.

**Pair**

- [ ] MENU -> Settings: the list scrolls, with View and Sync among the
      rows. A on Sync, then A on "Pair with a PC": the PC is listed by
      its name within a couple of seconds.
- [ ] A on the PC: both screens show the same six-digit code. A on the
      Miyoo, then confirm on the PC: "Paired with <name>".
- [ ] With Wi-Fi off, the page says so instead of searching. With the PC
      app closed: "No PC found", and A looks again.
- [ ] B at each step cancels and leaves the Miyoo unpaired.

**Sync now, with a few games**

- [ ] Start small: a PC library of three or four games, or a Miyoo with
      a few. MENU -> Sync now: the sticker shows the PC's name, the item
      and "n of m", then "Sync finished" with what moved each way.
- [ ] The shelf shows the new carts with no restart, with their labels,
      shells and hearts.
- [ ] Start one of the games that arrived: it runs. Start one that was
      already there: its save is the one you had.
- [ ] The footer icon beside Wi-Fi: two arrows while it syncs, a tick after.
- [ ] Press B during a long transfer: the sync stops, nothing half
      written is left in `Roms/` (every ROM there starts), and Sync now
      again carries on.
- [ ] Note how long a 16 MB and a 32 MB ROM take, and whether the menu
      stayed responsive meanwhile.

**Memory and space**

- [ ] During a Sync now that copies ROMs, read `VmHWM` from
      `/proc/<pid>/status` (over SSH or telnet): under 40 MB.
- [ ] With the card nearly full (under 100 MB free), sync a library
      bigger than the space: the summary says how many games did not
      fit, saves still sync, and at least 64 MB stays free on the card.

**Auto sync**

- [ ] Auto sync: On (the default). Quit slot-mini and start it with the
      PC running: the icon shows its two arrows for a moment after the shelf is up, and
      the first frame is no later than unpaired.
- [ ] Close a game on the PC: within a few seconds the Miyoo, sitting on
      the shelf, syncs by itself (two arrows, then a tick). Sitting in the studio
      or MENU it does not, until you are back on the shelf.
- [ ] Leave the Miyoo on the shelf for ten minutes, paired, PC off: it
      stays cool and the battery falls no faster than before the update.
- [ ] Auto sync: Off: none of the above happens; Sync now still works.

**Play on the PC, then the Miyoo; and the reverse**

- [ ] Play a game on the PC, make progress in its in-game save, close
      it. On the Miyoo start the same game: it starts no more than about
      a second later than usual and has the PC's progress.
- [ ] Carry on, eject, and open the game on the PC: it has the Miyoo's
      progress. In boot-launcher mode the save goes up when the shelf
      comes back; in app mode straight after the game (see below).
- [ ] Hold A to start fresh: still starts fresh.

**A deliberate conflict**

- [ ] Turn Wi-Fi off on the Miyoo. Play the same game on both and save
      in each. Turn Wi-Fi on and Sync now: the summary says "1 save
      conflict", the footer icon shows "!", and the PC's cart wears an
      amber "!".
- [ ] The later save is the one both sides run. On the PC, Sync ->
      Conflicts shows both copies; "Use the other copy", then Sync now on
      the Miyoo: the Miyoo has the other save and the "!" is gone.
- [ ] Nothing was deleted at any point: the copy not in use is on the
      PC, beside the save, as `<game>.srm.conflict-<who>-<when>`.

**App mode and boot-launcher mode**

- [ ] App mode (slot-mini started from Apps): play, eject. Onion's menu
      comes back as promptly as before; the save reaches the PC within
      about ten seconds without opening slot-mini. Check
      `.tmp_update/logs/slot-mini.log` for the push.
- [ ] App mode, In-game menu: Off: MENU in the game is Onion's, and the
      save still reaches the PC after the game.
- [ ] App mode: power off with the power button while in a game. The
      Miyoo powers off as fast as before and the save is intact; it
      syncs at the next start.
- [ ] Boot-launcher mode: play, eject: the shelf comes back at its usual
      speed, then the icon shows its arrows and the save reaches the PC.
- [ ] Start a game from Onion's own menu (app mode), play, quit: nothing
      syncs until slot-mini is next started, and then it does. This is
      the known gap.

**Wi-Fi off, PC off**

- [ ] Wi-Fi off: no icon activity, launches as fast as unpaired, Sync
      now says "Wi-Fi is off."
- [ ] Wi-Fi on, PC off or asleep: a launch waits no more than about half
      a second longer than usual; the icon is a cross;
      Sync now says so within a few seconds.
- [ ] PC back on a different address (restart the router or the PC):
      Sync now finds it again without pairing again.

**Unpair**

- [ ] Sync -> Unpair asks for A twice. Afterwards MENU has no Sync now
      row, the icon is gone, and every game and save is still on the
      card. The PC lists the Miyoo as gone after its next look.

## 20. The grid view (0.7.0)

- [ ] R2 on a shelf turns it into a grid of four rows; the footer shows
      "R2 ROW". R2 again brings the shelf back. Each shelf remembers its
      own choice across a restart (`App/slot-mini/data/views.txt`).
- [ ] MENU -> Settings -> View does the same for the shelf you are on.
- [ ] D-pad in four directions, held Up and Down: smooth, no stutter as
      rows come in. L1 / R1 switch shelves; A inserts from the grid; Y
      hearts; SELECT and START work as on the shelf.
- [ ] With a couple of hundred games on one shelf, hold Down from top to
      bottom: note any hitch, and read `VmHWM` afterwards (under 40 MB).
- [ ] Light theme (`appearance = light`): the legend and the grid read
      clearly.
- [ ] A shelf never switched to the grid looks and moves exactly as in
      0.6.6.

## What to report

The Onion version, the panel size, which boxes failed, the first-frame time,
VmHWM, anything that looked unlike slot, and the logs.

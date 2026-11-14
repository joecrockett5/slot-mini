# slot-mini PC: testing on a real Windows PC

CI builds and tests the app on Windows, but nothing there opens a window,
plays a sound or starts RetroArch. This is the checklist for a person at a
Windows 10 or 11 PC. Tick what works; note anything that does not with the
step number.

To have games without a Miyoo: `cargo run -p slot-mini-pc --example
fake_library -- C:\some\folder` writes a small library of homebrew-named
files (they will not run in RetroArch; use real ROMs for section 4).

## 1. Install and start

- [ ] Unzip `slot-mini-pc-<version>-windows-x64.zip`. It holds one folder
      with `slot-mini-pc.exe`, `README.txt`, `LICENSE.txt`, `CREDITS.txt`
      and `licenses\`.
- [ ] The exe shows the cartridge icon in Explorer; Properties -> Details
      shows the version and "slot-mini".
- [ ] Double-click it. SmartScreen may warn (unsigned): More info -> Run
      anyway. No console window appears.
- [ ] A window opens with the welcome screen, sized as a whole multiple of
      640x480, with the cartridge icon in the title bar and taskbar.
- [ ] Start the exe again: no second window; the first comes to the front
      (also when it was minimised).

## 2. First run and import

- [ ] The welcome screen offers pairing a Miyoo first, then the import,
      and names the library folder (`%USERPROFILE%\slot-mini`).
- [ ] Press START or Space (or click IMPORT): a folder dialog opens.
      Cancel: nothing changes. (A is pairing: section 7.)
- [ ] Choose a folder that is not an Onion card: it says so and copies
      nothing.
- [ ] Choose a Miyoo SD card (or a copy of one): progress shows, then a
      summary; the games appear with their labels, shells and favourites.
- [ ] `App\slot-mini-pc\import.txt` in the library lists what was copied
      and skipped. Nothing on the card has changed (compare dates).
- [ ] Import the same card again: everything is reported as already
      there, nothing is replaced.
- [ ] A card with a file Windows cannot hold (a name ending in a dot, or
      with `:`), if you have one: that file is listed as failed and the
      rest is imported.

## 3. Shelf, grid, input

- [ ] Keyboard: arrows browse, Up/Down and Page Up/Down jump letters,
      Q/W switch system, A hearts, Backspace opens the cart studio, Space
      the core picker, Esc MENU, 2 or Tab the grid.
- [ ] A bar above the footer lists the shortcuts: key caps after a key
      press, button names after a controller press, clickable buttons
      after a mouse click. It steps aside while a shelf's name shows.
- [ ] Controller (Xbox, then any other you have): D-pad and left stick
      browse; A plays (the bottom button on an Xbox pad; MENU -> Settings
      -> Controller switches to the Miyoo's layout); X opens the cart
      studio; START (Menu) the core picker; SELECT (Back / View) opens
      MENU; a stick click takes a cart off Recents. The Guide button is
      not needed. Unplug and replug while running: it keeps working.
      (A config from 0.1.0 keeps its "nintendo" layout until changed in
      MENU; delete `config.txt` to see the new default.)
- [ ] Mouse only, keyboard and controller untouched: click a cart, then
      every button on the bar: PLAY, FAVE, STUDIO, the two SHELF arrows,
      CORE, GRID / ROW, MENU. In MENU, the studio and the core picker the
      legends at the bottom (BACK, SELECT, the arrows...) are buttons
      too. Holding PLAY starts the game from the beginning. Hover picks
      carts in the grid, double-click plays, the wheel scrolls, the right
      button goes back.
- [ ] Insert and eject sounds play; MENU -> Settings -> Sounds turns them
      off and on again without a pause.
- [ ] F11 and Alt+Enter toggle full screen; the choice, the window's size
      and place survive a restart. Resize to odd sizes: the picture is
      never stretched, and what is left over is plain black.
- [ ] Full screen or a large window: cart labels, text and the bar are
      sharp, not blown-up 640x480 pixels (up to four times the size;
      wallpapers and cart shells are only as fine as their artwork).
- [ ] Move the window to a second monitor with different scaling, if you
      have one.

## 4. Playing

- [ ] With RetroArch installed in its default place, MENU -> Folders shows
      it found. With it somewhere unusual, point at `retroarch.exe` there.
- [ ] Remove a core (`mgba_libretro.dll`) from RetroArch's cores: the GBA
      shelf says it is not installed; MENU -> Folders offers it; two
      presses download it; the notice goes.
- [ ] A on a game with slot-mini full screen: the cart goes in,
      slot-mini's window hides, RetroArch starts full screen with the
      game. The Miyoo's save for it (imported) is there.
- [ ] The same with slot-mini in a window: RetroArch opens in a window in
      the same place and about the same size, on the same monitor.
- [ ] Save in the game, close RetroArch (Esc, or its menu): slot-mini
      comes back, the cart ejects. The `.srm` in the library's `Saves`
      folder has a new date.
- [ ] Hold A: the game starts from the beginning rather than its state.
- [ ] Space on a GBA game: choose the other core; it starts with that one
      and keeps its saves apart.
- [ ] Start slot-mini a second time while a game runs: nothing appears
      over the game.
- [ ] A game in a library path longer than 260 characters, and one with
      accents or Japanese in its name: both start.

## 5. MENU

- [ ] Check for updates: says up to date, or names the newer version and
      opens the release page. With the network off: says it could not
      check, and nothing hangs.
- [ ] Folders -> Library folder: move to another folder; the shelf
      reloads from it.
- [ ] Settings -> Sync: opens the Sync page (section 7).
- [ ] Quit, the window's close button and Alt+F4 all end the app at
      once, with no "Not responding"; no `slot-mini-pc.exe` is left in
      Task Manager, and the window's place is remembered.

## 6. Leaving no mess

- [ ] `%APPDATA%\slot-mini-pc\config.txt` is the only thing outside the
      library and the unzipped folder.
- [ ] RetroArch's own `retroarch.cfg` is unchanged after playing (compare
      before and after).

## 7. Sync

This needs a Miyoo Mini Plus running a slot-mini that has Sync in its
MENU, on the same Wi-Fi network as the PC, and a library with a few real
games and saves on the Miyoo. Start with an empty PC library (move the
library folder aside, or point MENU -> Folders at a new one).

The firewall prompt:

- [ ] On a PC where slot-mini has never run (or after removing its rules
      in "Allow an app through firewall"), start it: Windows Firewall
      asks about `slot-mini-pc`. Note what the prompt says.
- [ ] Choose Cancel. slot-mini runs as usual. On the Miyoo, MENU -> Sync
      does not find the PC.
- [ ] MENU -> Settings -> Sync -> "Miyoo can't find this PC?": the steps
      it gives match what Windows shows. Follow them (tick `slot-mini-pc`
      under Private). The Miyoo now finds the PC by its name.
- [ ] Remove the rules and start again; this time choose Allow with
      Private ticked: the Miyoo finds the PC without more ado.

Pairing:

- [ ] The welcome screen says "Pair your Miyoo to bring its library
      over". Press A: "Pair a Miyoo" shows this PC's name and waits.
- [ ] On the Miyoo, MENU -> Sync, pick the PC: both show the same six
      digits. Deny on the PC: the Miyoo says it was not paired.
- [ ] Again, and Confirm on both: the PC says "Paired with <Miyoo>" and
      asks for Sync now on the Miyoo.
- [ ] Sync page -> This PC: type a new name, Enter. The Miyoo shows the
      new name at its next sync and stays paired.

Sync now, from each side:

- [ ] Sync now on the Miyoo: the PC shows "From <Miyoo>: n of m, <game>"
      as files arrive, the status icon left of the clock counts, the
      shelves fill without a restart, and the shelf ends with a line like
      "Synced with <Miyoo>: 14 games and 3 saves in". Labels, shells and
      favourites match the Miyoo's.
- [ ] Sync now on the PC's Sync page with the Miyoo on its shelf: the
      Miyoo starts a sync within a few seconds. With the Miyoo off or in
      a game: after about 8 seconds the PC says there was no answer.
- [ ] Add a ROM to the PC's `Roms\GBA` and a label for it; Sync now: it
      appears on the Miyoo.
- [ ] Change a game's label on the Miyoo (cart studio); sync: the cart on
      the PC's shelf wears the new label without a restart.
- [ ] Turn the Miyoo's Wi-Fi off mid-sync: the PC says the sync stopped
      part way, the icon shows a failure, and the next sync finishes the
      job.

Play on the PC, then the Miyoo:

- [ ] Play a game on the PC, save in the game, close RetroArch: the Miyoo
      (on its shelf) syncs by itself within a few seconds.
- [ ] Start that game on the Miyoo: the save from the PC is there.
- [ ] Save on the Miyoo, exit to its shelf, let it sync; start the game
      on the PC: the Miyoo's save is there, and it resumes from the save,
      not from an old auto state.
- [ ] While a game runs on the PC, Sync now on the Miyoo: everything else
      syncs, that game's save is left alone, and it syncs after the game
      is closed.

A deliberate conflict:

- [ ] Turn the Miyoo's Wi-Fi off. Play and save the same game on both
      (PC first, then the Miyoo). Wi-Fi on, sync.
- [ ] The PC's shelf says "1 conflict", the icon shows it, and the cart
      wears an amber "!" on the shelf and in the grid.
- [ ] Sync -> Conflicts -> the game: "In use" is the Miyoo's copy (the
      later one), "Set aside" is the PC's, each with when it was played.
- [ ] Start the game on the PC: it is the Miyoo's progress.
- [ ] "Use the other copy": the two change places; the game on the PC now
      has the PC's progress, and after a sync so does the Miyoo.
- [ ] "Keep this one": it asks, starting on Cancel. Confirm: the conflict
      and the "!" are gone, and the other copy is in
      `Saves\slot-mini-removed` in the library.

Remove everywhere:

- [ ] Select a game you can spare. Press Delete: a sheet names the game,
      says it goes from this PC and every paired Miyoo and where the save
      is kept, with the cursor on Cancel. Cancel: nothing changes.
- [ ] Right-click the selected cart: the same sheet. (A right click on
      another cart only selects it; off the carts it is Back.)
- [ ] MENU -> Remove this game -> Remove everywhere: the cart leaves the
      shelf at once; its ROM and label are gone from the library and its
      save is in `Saves\slot-mini-removed\<system>`.
- [ ] Sync: the game leaves the Miyoo too, its save kept in
      `Saves/slot-mini-removed` on the card. Another sync does not bring
      it back on either side.

Last:

- [ ] Sync page -> Devices -> the Miyoo -> Unpair (asks first): the Miyoo
      can no longer sync until it is paired again.
- [ ] Quit while the Miyoo is mid-sync: the app closes at once; the next
      sync carries on. No `slot-mini-pc.exe` is left running.

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

- [ ] The welcome screen names the library folder
      (`%USERPROFILE%\slot-mini`) and offers the import.
- [ ] Press A (or click): a folder dialog opens. Cancel: nothing changes.
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
- [ ] The footer shows key caps after a key press and button glyphs after
      a controller press.
- [ ] Controller (Xbox, then any other you have): D-pad and left stick
      browse; A/B are the Miyoo's way round; MENU -> Controller swaps
      them. Unplug and replug while running: it keeps working.
- [ ] Mouse: hover and click carts on the shelf and in the grid,
      double-click plays, wheel scrolls, clicks work in MENU and the
      studio.
- [ ] Insert and eject sounds play; MENU -> Sounds turns them off.
- [ ] F11 and Alt+Enter toggle full screen; the choice, the window's size
      and place survive a restart. Resize to odd sizes: the picture stays
      sharp or evenly scaled over the blurred wallpaper, never stretched.
- [ ] Move the window to a second monitor with different scaling, if you
      have one.

## 4. Playing

- [ ] With RetroArch installed in its default place, MENU -> Folders shows
      it found. With it somewhere unusual, point at `retroarch.exe` there.
- [ ] Remove a core (`mgba_libretro.dll`) from RetroArch's cores: the GBA
      shelf says it is not installed; MENU -> Folders offers it; two
      presses download it; the notice goes.
- [ ] A on a game: the cart goes in, slot-mini's window hides, RetroArch
      starts full screen with the game. The Miyoo's save for it (imported)
      is there.
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
- [ ] Sync: says it is coming.
- [ ] Quit, and the window's close button, both end the app; no
      `slot-mini-pc.exe` is left in Task Manager.

## 6. Leaving no mess

- [ ] `%APPDATA%\slot-mini-pc\config.txt` is the only thing outside the
      library and the unzipped folder.
- [ ] RetroArch's own `retroarch.cfg` is unchanged after playing (compare
      before and after).

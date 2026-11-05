# Decisions

Calls made without asking, and why. Newest last.

1. **Run as an app first, boot launcher by opt-in.** The release zip only adds
   `App/slot-mini/`. The boot switch is a setting inside the app. (Brief.)
2. **Never run under a process name starting with `MainUI`.** Onion's keymon
   treats such a process as MainUI and steals MENU, SELECT, Y and L1 (see
   `PLATFORM.md`, Input). The boot shim re-executes itself through `/bin/sh`
   before starting slot-mini.
3. **Hold A does what Onion's own "Reset game" does.** On released Onion
   that is RetroArch run directly with `--appendconfig .tmp_update/reset.cfg`
   and the core Game List Options would pick; where the core is chosen at
   run time (Onion's development branch) the runtime's `/tmp/reset_game`
   flag is used instead. (0.1.0 used only the flag, which released Onion
   does not have; found on a device.) Either way slot-mini stays out of
   RetroArch's own config.
4. **In app mode a game returns to MainUI, not slot-mini.** Onion has no hook
   to bring an app back after a game. slot-mini remembers the cart and opens on
   it next time. As the boot launcher it returns straight to the cart.
5. **Recents are written by slot-mini** in the stock record format, to the file
   Onion's readers use (`recentlist-hidden.json` if it exists, else
   `recentlist.json`), newest first, deduplicated by ROM, at most 50 lines.
6. **Sound is off by default.** A static binary cannot use `libpadsp.so`;
   the only route found is spawning Onion's `ffplay`. It is unproven on
   hardware, so it is opt-in until `DEVICE-TESTING.md` confirms it.
7. **No charging bolt.** On the Mini Plus charging state needs a process spawn
   (`axp_test`); not worth it on the launcher's hot path.
8. **`theme.txt` uses slot's `key = value` config format.** slot itself has no
   `theme.txt`; its theme is the Dark/Light appearance setting. slot-mini reads
   `appearance` and a few colour overrides from `theme.txt` with slot's own
   `ini.rs` parser.
9. **Reference images come from slot's site media**, not slot's render tests,
   because slot's headless GL surface is macOS-only and the brief forbids
   building slot.
10. **Keep slot's `Draw` list as the drawing seam.** slot-ui already emits
    display lists over CPU-built textures, so copied layout code runs
    unchanged against a software rasteriser. slot's layout `Canvas` (a size)
    is imported as `Stage` so that the pixel buffer can be called `Canvas`,
    as the brief asks.
11. **ROM extensions follow Onion, not slot.** slot only lists `.gba`, `.gb`
    and `.gbc`; slot-mini lists what Onion's `Emu/<sys>/config.json` accepts
    (`bin`, `zip`, `7z`, `dmg` too), so nobody has to unpack or rename games.
    Header-based shell colours only work for unpacked ROMs; zipped ones get the
    platform's default shell.
12. **One level of subfolders is scanned** under `Roms/<sys>/`, skipping
    Onion's `Imgs/`. Labels still match by file name, as in slot.
13. **Faces are pre-scaled in the worker threads.** Every cart face is built
    at full size and at slot's side scale (0.78) off the main thread; the
    renderer draws whichever matches, and only moving carts use
    nearest-neighbour scaling. A smooth resize on the main thread cost ~94 ms
    per cart under qemu-armv7.
14. **Every frame waits for the cart faces it shows**, up to 500 ms, as
    slot does. (0.1.0 waited only on the first frame; on the device, letter
    jumps and shelf switches then flashed every cart as its label colour.)
    A jump now pauses briefly (about 50 ms under qemu-armv7) instead.
15. **Copied slot code is kept rustfmt-clean.** slot formats with default
    rustfmt; renaming `slot_store` to `slot_mini_store` makes a few lines
    too long, and rustfmt re-wraps them. Nothing else in copied code is
    reformatted.
16. **Unsafe file names are refused, not escaped.** Onion's runtime parses
    the command with `awk` and `:`-splitting; a name with `"`, `` ` ``, `\`,
    `:` or control characters cannot round-trip. slot-mini shows "Rename this
    file to launch it" instead of writing a command Onion would mangle.
17. **Recents lines slot-mini cannot parse are kept as they are**, so a
    damaged line is never lost by a launch from slot-mini.
18. **Exit code 10 means "start Onion's MainUI"** for the boot shim (MENU →
    exit to Onion). Any other non-zero exit is treated as a failure by the
    shim, which also falls back to MainUI.
19. **slot's GB house-label size is kept as slot has it (1024x901)**, though
    slot's own test expects the 223x199 label well's shape (Cart Studio's
    1024x914); the test fails in slot at e66715d and is ignored here with
    that note. Labels are scaled to cover and centre-cropped, so either size
    shows correctly.
20. **The on-device studio is flat.** slot's state machine and layout
    (`studio.rs`, `studio_view.rs`) are copied unchanged; the cart is tilted
    once in software, chips are drawn as discs, and printed labels appear as
    plain slips without the print, tear and apply animations.
21. **The label printer row is always shown.** With the service off it says
    labels can be made in Cart Studio on a computer; without Wi-Fi it says
    the printer needs Wi-Fi (slot hides the row without Wi-Fi).
22. **A recoloured text label is saved as a label file** on closing the
    studio, as slot does, so Cart Studio sees the same label. Recolouring a
    logo label needs the logo image, which only exists while the studio has
    fetched it; outside that the Label Color row is slot's but has no effect
    once closed.
23. **MENU opens About** with four rows: Sounds, In-game menu, Boot launcher
    and Exit to Onion. Exit to Onion returns to MainUI as an app, and asks the boot shim
    for Onion's MainUI (exit code 10) as the boot launcher.
24. **The shim falls back on L2 held**, not MENU: Onion already uses MENU
    held at boot to skip auto-resume, and MENU is pressed in games to leave
    them, so it could still be down when the launcher restarts. L2 is
    otherwise unused on the shelf. A `DISABLED` file next to the backups also
    works, as in Open MainUI's wrapper.
25. **Backups live in `.tmp_update/slot-mini/stock/`**, not in the app
    folder, so deleting the app never deletes them. Each is stored as a file
    named `MainUI` so keymon still recognises Onion's menu when it runs.
26. **Installing refreshes a backup from a genuine binary** (one without the
    shim's marker). After an Onion update restores its own MainUI, the next
    install backs up the new one; a restore never replaces a binary that is
    not the shim.
27. **With no original left, the shim runs slot-mini once more and then
    gives up** (exit 1), and Onion's loop tries again, rather than looping on
    `exec` inside one run.
28. **On exit, the last frame is left on framebuffer page 0 and shown
    there.** Onion's programs assume page 0 (`display_reset()` sets
    `yoffset = 0`), and its in-game Game Switcher draws there. 0.1.0 exited
    on whichever page its last frame used, which could leave the Game
    Switcher drawing to a hidden page: a black screen that still took input.
29. **slot's in-game MENU only for games slot-mini launches** (0.2.0, the
    maintainer's choice). A helper started with the game takes MENU through
    Onion's `/tmp/disable_menu_button` flag; nothing of Onion's is replaced,
    and games started elsewhere keep Onion's MENU. "In-game menu: Off" in
    About turns it off.
30. **First version: menu and eject; the polaroid save-state switcher
    later** (also the maintainer's choice). Save and Load State use
    RetroArch's current slot, and RetroArch's own message confirms them.
31. **The game is paused with `PAUSE_TOGGLE` and then frozen with
    `SIGSTOP`.** Pausing alone may let RetroArch redraw over the menu;
    freezing alone may leave audio running. A panic hook, the signal handler
    and the main loop all thaw RetroArch before the helper exits, and
    slot-mini clears any helper or flag left behind each time it starts, so
    MENU can never stay taken from Onion.
32. **The in-game menu is drawn on every framebuffer page** (0.2.1). The
    page RetroArch shows next cannot be known for sure; covering all of
    them (at most a few 640x480 pages) is cheap and always visible.
33. **The input device is grabbed while the in-game menu is open, and rows
    act on release** (0.2.1). Without the grab RetroArch reads the menu's
    presses when it thaws. POWER resumes the game, since keymon cannot see
    it while the device is grabbed.
34. **The eject starts on the first frame shown** (0.2.1). Its clock and
    sound used to start when slot-mini started, so the time spent loading the
    shelf ate the animation and only the last frames, or none, were seen.
35. **Updates only when asked, only numbered releases** (0.3.0). No
    background traffic, and the rolling `main-latest` build is for people
    who unzip it on purpose.
36. **An update must prove itself before it replaces anything.** SHA-256
    against GitHub's digest, CRCs, then the new binary has to run on the
    device and report the promised version. The replaced files are kept in
    `update/previous/`, and a failed swap puts them back. `data/`, `cache/`
    and `update/` can never come from a zip.
37. **Restart by `exec`, not by exiting.** Exiting would drop to Onion's
    menu in app mode and run the shim's fallbacks in launcher mode; `exec`
    keeps the PID and the arguments, so either mode carries on.
38. **Charging is read from batmon's `/tmp/.axp_result`, not by running
    `axp_test`** (0.3.2). batmon already runs it and rewrites the file within
    a second of the charger going in or out; reading a small file in memory
    every 2 seconds costs nothing. The gauge turns green while charging, as
    the maintainer asked; slot only shows the bolt.
39. **The armv7 build runs only on `main`** (maintainer's choice). Pull
    requests and branches get the desktop job (fmt, clippy, every test, the
    shim tests); the slow cross build, qemu tests and zip run where a release
    is published. An armv7-only failure therefore shows up after merging, and
    stops that release; **Run workflow** runs the armv7 job on a branch first.
40. **The core choice is Onion's per-game file, not slot's** (0.4.0). Onion
    applies `.game_config/<rom>.cfg` to every launch through `launch.sh` and
    shows it in Game List Options, so the two can never disagree. mGBA is
    the default, as the maintainer asked and as 4.3.1-1 ships; choosing it
    removes the file, like "Restore default core".
41. **GBA only, START opens it** (0.4.0, the maintainer's request). slot
    also offers mGBA or Gambatte for Game Boy carts; that is a small step
    later. START was free on slot-mini's shelf and is slot's button for it.
42. **Say that saves are per core, move nothing.** RetroArch keeps mGBA and
    gpSP saves apart; copying or converting them is Onion's business (its
    development branch has a transfer script), so slot-mini only tells the
    player when the core changes.

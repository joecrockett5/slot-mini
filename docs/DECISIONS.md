# Decisions

Calls made without asking, and why. Newest last.

1. **Run as an app first, boot launcher by opt-in.** The release zip only adds
   `App/slot-mini/`. The boot switch is a setting inside the app. (Brief.)
2. **Never run under a process name starting with `MainUI`.** Onion's keymon
   treats such a process as MainUI and steals MENU, SELECT, Y and L1 (see
   `PLATFORM.md`, Input). The boot shim re-executes itself through `/bin/sh`
   before starting slot-mini.
3. **Fresh start uses Onion's `/tmp/reset_game` flag** rather than a private
   `--appendconfig` file. It is the same mechanism as Game List Options'
   "Reset game", already handles injection and cleanup in the launch script,
   and keeps slot-mini out of RetroArch's config.
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
14. **Only the first frame waits for cart faces** (up to 250 ms). After that,
    a cart whose face is not ready shows slot's label-colour placeholder for a
    frame or two instead of stalling the scroll.
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
23. **MENU opens About** with three rows: Sounds, Boot launcher and Exit to
    Onion. Exit to Onion returns to MainUI as an app, and asks the boot shim
    for Onion's MainUI (exit code 10) as the boot launcher.

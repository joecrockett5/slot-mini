# slot-mini companion, brief 2 of 4: the PC app

You are working in the `joecrockett5/slot-mini` repository. This file is your whole brief (its file name is `02-pc-app.md`). The shared sections come first, then your own.

## First steps

1. Commit this brief, unchanged, as `docs/briefs/<this brief's file name>` in your first commit, and add one line to `CLAUDE.md` pointing to it, so that a fresh session can pick up from it.
2. Read `CLAUDE.md`, `README.md`, `docs/DECISIONS.md`, `docs/PLATFORM.md` and `docs/PROGRESS.md`. Then read the code your brief touches before you design anything. slot-mini is a mature codebase with strong conventions. Follow them, don't invent new ones.

## The whole project

slot-mini (this repo) is a slot-style launcher for the Miyoo Mini Plus on Onion OS. It is being extended into a pair of apps:

- **slot-mini PC** (`crates/slot-mini-pc`): a Windows frontend for standalone RetroArch with slot-mini's look, built on slot-mini's UI crates. Windows comes first, but it must also build and run on Linux so that it can be tested headlessly in CI and in agent sandboxes.
- **Sync** between the PC and the Miyoo when both are on the same Wi-Fi network.

The work is split across four briefs:

| # | Brief | Starts after |
| --- | --- | --- |
| 1 | `01-sync-core.md`: the shared sync crate, `crates/slot-mini-sync` | – |
| 2 | `02-pc-app.md`: the PC app, without sync | – (runs in parallel with 1) |
| 3 | `03-pc-sync.md`: sync in the PC app | 1 and 2 are merged |
| 4 | `04-device-sync.md`: sync and grid view on the Miyoo | 1 and 2 are merged |

Other agents may be working in the same repo at the same time. Stay inside your brief and rebase on `main` before you merge. Shared files (`CLAUDE.md`, `README.md`, `docs/DECISIONS.md`, CI) are where conflicts happen; resolve them by keeping both sides' changes.

## Agreed decisions

The maintainer made these decisions. Do not reopen them. If one turns out to be impossible, stop and say why.

1. **One library.** The PC's library and the Miyoo's are the same library. The first sync seeds an empty PC library from the Miyoo. After that, games, labels and art added on either side appear on the other.
2. **What syncs, and when.**

   | Kind | Files | Auto sync | Manual sync |
   | --- | --- | --- | --- |
   | ROMs | `Roms/{GBA,GBC,GB}/`, including the one subfolder level slot-mini scans | no | yes |
   | Labels and art | `Labels/{GBA,GBC,GB}/<stem>.png`, `Roms/<sys>/Imgs/<stem>.png` | yes | yes |
   | Shells | `Labels/cart_shell.txt`, merged per game line | yes | yes |
   | Favourites | each side's `favorites.txt`, merged per entry (platform + stem) | yes | yes |
   | In-game saves | `<stem>.srm` and `<stem>.rtc` per game | yes | yes |

   Not synced: save states, Recents (each device keeps its own), settings, theme, wallpapers, the per-game core choice (`.game_config`) and caches.
3. **Save states are not synced.** When a newer in-game save arrives on either side, that side's auto save state for the game is set aside. It is renamed with the `.bak_<stamp>` scheme `cores::carry_save` already uses, never deleted. Otherwise Tap A could resume an older session and overwrite the newer save. The maintainer accepted the consequence: after playing on the other device, a game starts from its title screen and you continue from the in-game save.
4. **Saves belong to games, not cores.** Each side keeps its saves in RetroArch's per-core folders (`Saves/CurrentProfile/saves/<core>/<stem>.srm`). Sync maps "the save for game X" to whichever core folder that game uses on each side. `.srm` files work in both mGBA and gpSP (Onion's transfer script and `cores::carry_save` rely on this), and Gambatte is used for GB/GBC on both sides. A save file that is all `00` or all `ff` counts as no save.
5. **Deletions don't propagate**, except through an explicit **Remove everywhere** on the PC. It removes the game's ROM, labels, art, shell line and favourite on both sides, and moves its saves into a backup folder instead of deleting them.
6. **Conflicts.** If both sides changed the same save since the last sync, keep both. The more recently played copy becomes the save, and the other is kept beside it as a conflict copy that RetroArch will not load. The PC app lets the user swap them. For labels, art and shell lines, the PC's version wins and nothing extra is kept.
7. **Topology.** The PC app runs the sync server and the Miyoo is the client: the Miyoo starts every sync. "Sync now" on the PC sends a UDP poke, and a Miyoo sitting on its shelf answers it by syncing. Same LAN only. **No off-LAN work**: no hub on another server, no relay, no Tailscale. The maintainer will choose that strategy later, so don't build toward one.
8. **Changes are detected by content hash** against a per-device record of what was last synced, never by modification time alone. The Miyoo's clock is unreliable.
9. **Grid view.** Any shelf can show a grid of carts instead of slot's scrolling shelf. The choice is made per shelf and remembered. The Miyoo defaults to the scrolling shelf. The PC defaults to the grid.
10. **PC input** comes from the keyboard, a game controller and the mouse.
11. **PC features:** the shelf and grid, Favourites, Recents, the cart studio, the label printer (online labels), the GBA core picker, wallpapers, the theme, sounds, the About sticker and an update check. Not on the PC: slot's in-game menu (RetroArch's own menu is used), the boot launcher and splash screens.
12. **RetroArch on the PC** is the standalone Windows build (installer or portable zip), not the Steam version.
13. **One repo.** Everything lives in this Cargo workspace.

## Conventions every brief keeps

- **The device comes first.** Unless your brief says otherwise, slot-mini's behaviour on the Miyoo does not change. Existing golden images stay byte-identical. The device budgets still hold: first frame within 1 second, a steady 60 fps, under 40 MB resident memory, and no new timers or polling that wake the CPU while idle (see 0.4.2 and 0.4.6 in `docs/PROGRESS.md` for why this matters). The armv7 CI job stays green.
- **There are two release lines.**
  - The device release is `v<x.y.z>`, from the workspace `version`. It reaches Miyoos over the air when merged to `main`.
  - The PC release is `pc-v<x.y.z>`, from `slot-mini-pc`'s own `version` (brief 2 sets this up).

  Follow `CLAUDE.md`'s standing shipping instruction for each line. Bump the workspace version only when what the Miyoo runs changes, and bump the PC version only when the PC app changes.
- **Networking.** Use blocking I/O on worker threads, with no async runtime. Use `ureq` for HTTP clients, as `update.rs` and the label service already do. Use `ring` for cryptography, which the device binary already links.
- **Copied code.** Anything copied from slot keeps slot-mini's provenance-line convention. Anything copied from elsewhere must be GPL-3.0-or-later compatible and credited.
- **Records.** Append decisions to `docs/DECISIONS.md` under a heading for your brief, with the reason for each. Keep your brief's progress file current at every phase exit and before you stop.
- **Tests.** Write tests as you go. Test network code against local servers, never the internet. Look at rendered PNGs yourself before calling a screen done: a passing golden test only proves the image didn't change.
- **Stopping.** Work autonomously through every phase. Stop and ask only if an agreed decision proves impossible, a change could leave a Miyoo unable to boot or lose a save, or a device budget can't be met without cutting scope.

## Your brief: the PC app (without sync)

**Goal:** build `crates/slot-mini-pc`, a Windows app that:

- shows the library with slot-mini's look;
- launches games in standalone RetroArch;
- has every PC feature in decision 11, plus the grid view.

Sync is brief 3's job, but leave clean seams for it:

- a Sync page in Settings that is a placeholder for now;
- a slot in the HUD for a sync status icon;
- a hook that runs after RetroArch exits;
- the PC's choice of core per game, exposed in a form the sync crate's PC layout can call.

Your progress file is `docs/pc/PROGRESS.md`. Your user docs are a "slot-mini PC" section in the README plus `docs/pc/TESTING.md`.

### Requirements

- **Reuse, don't fork.**
  - Depend on `slot-mini-gfx`, `slot-mini-ui` and `slot-mini-store`.
  - A lot of app-level logic lives in the `slot-mini` crate: library scanning and shelves, the cart studio, the label cache and art jobs, faces, wallpapers, theme, settings, Favourites and Recents. Share it by moving its platform-free parts into a shared crate, or by depending on `slot-mini` with its Onion-only modules kept out. You decide, and record why.
  - The device's behaviour, golden images, binary size and performance must not change.
- **Rendering.**
  - Draw slot's 640x480 frame and scale it to the window. Use an integer scale when one fits, and otherwise the largest fit.
  - Fill the letterbox with something in keeping with the look, such as the current wallpaper blurred and dimmed.
  - Support windowed and fullscreen modes (F11 and Alt+Enter), remember the window state, and be per-monitor DPI-aware so the image stays crisp.
  - minifb has no gamepad support and weak fullscreen handling, so evaluate `winit` + `softbuffer` + `gilrs` (XInput controllers) and record your choice.
- **Input.**
  - Keyboard: keep the desktop build's keys and add PC-natural ones (for example Enter = A), with no clashes.
  - Controller: map by button position in the Miyoo's Nintendo layout (right = A, bottom = B), with a setting to swap to Xbox-style A/B.
    - The Miyoo's MENU button has no direct equivalent, and XInput can't read the Guide button. Pick something sensible, such as View + Menu together or a stick click, and document it.
  - Mouse: hover selects, a click selects, a double-click inserts, and the wheel scrolls the shelf or grid.
  - The footer button legend shows the active input device's buttons: the existing controller glyphs for a controller, and new key caps drawn in slot's style for the keyboard.
- **Library.**
  - The library is a folder in Onion's SD layout:
    - `Roms/{GBA,GBC,GB}/` (with `Imgs/`)
    - `Labels/`
    - `Saves/CurrentProfile/{saves,states}/<core>/`
    - `Wallpapers/`
    - `App/slot-mini-pc/{data,cache}/`
  - Choose its location on first run (default `%USERPROFILE%\slot-mini`, via a native folder dialog), and keep only a pointer to it in `%APPDATA%\slot-mini-pc\`. That keeps the library portable.
  - When the library is empty, show a first-run screen in slot's style. It says sync is coming (brief 3 replaces this with pairing), offers Import from an SD card, and says which folder ROMs can be dropped into.
  - Rescan when the app regains focus.
- **Import from an SD card.** Pick a mounted Onion card's root. Copy its ROMs, labels, `Imgs` art, `cart_shell.txt` lines and favourites into the library, and copy each game's save into the core folder the PC will use for that game.
  - Only add what the library lacks: never overwrite, and report what was skipped.
  - Don't copy states.
  - Show progress.

  This gives the maintainer a fast way to seed the library over a card reader. Brief 1's planner will treat the identical files as already synced.
- **RetroArch.**
  - Find `retroarch.exe`: check the standalone installer's default location, then common portable locations, then ask with a file dialog. Remember the path.
  - Find the cores (`mgba_libretro.dll`, `gambatte_libretro.dll` and the optional `gpsp_libretro.dll`) in the cores folder named by `libretro_directory` in its `retroarch.cfg`, else `cores\`.
  - If a needed core is missing, explain and offer to download it from libretro's buildbot after the user confirms. Verify the URL pattern; it is believed to be `https://buildbot.libretro.com/nightly/windows/x86_64/latest/<core>.dll.zip`.
  - A shelf whose core is missing says so.
- **Launching.**
  - Run `retroarch.exe -L <core> <rom> --appendconfig <temp file>`. The append file sets:
    - `savefile_directory` and `savestate_directory` into the library;
    - `sort_savefiles_enable` and `sort_savestates_enable` to `true`, with the by-content variants off;
    - `savestate_auto_save` to `true`;
    - `savestate_auto_load` to `true` for Tap A and `false` for Hold A;
    - `config_save_on_exit` to `false`;
    - fullscreen from the app's setting.
  - Verify against RetroArch's source that without `config_save_on_exit = false` RetroArch would write these values into the user's own `retroarch.cfg`, and that the per-core folder names (`mGBA`, `gpSP`, `Gambatte`) match Onion's.
  - Play the insert animation and sound, hide the app while the game runs, wait for RetroArch to exit, then return to the same cart with the eject animation and run the post-game hook.
  - Never launch two games at once.
- **Core picker.** START on a GBA cart opens the core picker as on the device: mGBA, and gpSP when its core is installed. It carries the save across cores with the shared `carry_save` logic.
- **Features.** Recents (the PC's own), Favourites, the cart studio (mouse-friendly as well as button-driven), the label printer and online labels, `theme.txt`, wallpapers from the library's `Wallpapers/`, sounds through a small audio crate, the About sticker with the PC version and credits, and a Settings page:
  - sounds
  - the Recents shelf
  - online labels
  - fullscreen
  - controller layout
  - RetroArch path
  - library folder
  - Sync (the placeholder)
- **Grid view.** Build it in `slot-mini-ui` so the Miyoo can adopt it in brief 4.
  - Design it in slot's visual language. One idea is carts standing in a cartridge storage case, labels facing out, with the selected cart lifting and the title plate shown as the shelf shows it. Render your design, look at it, and record what you chose and why.
  - Each shelf has its own view mode, remembered per shelf; the PC defaults to grid.
  - One button toggles the view on a controller and on the Miyoo's buttons. Choose one the device has free (R2 is a candidate; L2 is held at boot for Onion) and use the same one on both sides. The mouse can also toggle it.
  - Navigation: the D-pad moves between cells, L1/R1 switch shelves, and the grid keeps a fast way to jump by letter. A, Y, X, SELECT and START act as on the shelf.
  - Add golden images for one cart, a few carts, a full page, and many carts with scrolling, in both dark and light appearance.
  - The Miyoo will draw it in software on a Cortex-A7, so build it the way the shelf is built (pre-scaled faces composited once; see `docs/DECISIONS.md` #13 and #14) and measure it with `examples/perf.rs` and under qemu-armv7. Record the numbers.
- **Updates.** Check GitHub releases filtered to `pc-v*` tags and offer the download. Updating can be a link at first; an in-place update is optional.
- **Releases and CI.**
  - Give `slot-mini-pc` its own `version`, starting at `0.1.0`.
  - Add a `windows-latest` job (`x86_64-pc-windows-msvc`) that builds and tests the PC app. The desktop Linux job also builds and tests it headlessly. The armv7 job excludes it.
  - On `main`, publish `pc-v<version>` (or refresh a rolling PC pre-release, mirroring `scripts/release.sh`) with `slot-mini-pc-<version>-windows-x64.zip`. The zip holds the exe (Windows subsystem, so no console window, with an icon from slot-mini's and a version resource), a `README.txt`, slot's GPL licence, the font licences and `THIRD-PARTY.txt` from `scripts/third-party.py`.
  - Create PC releases with `--latest=false`. The device's updater asks GitHub for `releases/latest`, and a PC release becoming "latest" would break device updates.
  - Also harden `update.rs` to ignore any tag that isn't `v<x.y.z>`. That is a device change, so bump the workspace version.
  - Update `CLAUDE.md` with the two release lines.
- **Windows details.** Single instance. Long paths. Names that are illegal on Windows are reported, not crashed on. The unsigned exe triggers SmartScreen, so tell the user how to run it in the README.

### Plan

1. **Research and design.** Check the RetroArch facts above against RetroArch's source. Choose the window, input and audio stack. Plan the code sharing. Sketch the grid view as a real render.
   - Exit: `docs/pc/PROGRESS.md` and the `docs/DECISIONS.md` entries record each choice with sources.
2. **Skeleton.** The crate, a window with scaling and fullscreen, keyboard, controller and mouse input, the PC's paths and the shared-code refactor, plus CI on Windows and Linux.
   - Exit: CI is green everywhere, device golden images are unchanged, and the app shows the shelf on a fake library.
3. **Launching.** RetroArch and core discovery, the append config, launch and return, Tap and Hold A, and the post-game hook. Test against a fake `retroarch.exe` (a small test binary that records its arguments and writes a save file) on Linux and Windows CI.
   - Exit: an integration test launches a game, checks the command line and config, and returns to the same cart.
4. **Grid view.**
   - Exit: golden images and perf numbers are recorded, and a per-shelf toggle works on the PC.
5. **Feature parity.** Everything in decision 11.
   - Exit: tests for each feature, and golden images for every new screen.
6. **Import and first run.**
   - Exit: an import test from a fake Onion card, including the save core mapping and skip-don't-overwrite.
7. **Packaging and release.**
   - Exit: CI builds the Windows zip. A PC release does not become GitHub's "latest". The device updater ignores `pc-v*` tags.
8. **Docs.** The README section, `docs/pc/TESTING.md` (a checklist for the maintainer on a real Windows PC with real RetroArch and a controller, safest checks first) and `docs/pc/PROGRESS.md`.

### Done means

- [ ] slot-mini PC builds and tests on Windows and Linux in CI and ships a Windows zip as `pc-v*`
- [ ] Games launch in standalone RetroArch with saves in the library's per-core folders, and RetroArch's own config is untouched
- [ ] The grid view lives in `slot-mini-ui`, per shelf, with golden images and device perf numbers
- [ ] Every feature in decision 11, plus Import from an SD card
- [ ] Seams for brief 3 are in place: the Sync page, the status slot, the post-game hook and the core choice per game
- [ ] Device behaviour, golden images and budgets unchanged; device updates still ignore PC releases

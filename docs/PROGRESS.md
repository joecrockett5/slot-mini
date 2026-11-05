# Progress

## Status

| Phase | State |
| --- | --- |
| 1. Research | done |
| 2. Skeleton and toolchain | done |
| 3. Carousel | done |
| 4. Launching | next |
| 4. Launching | |
| 5. Extras | |
| 6. Device backends | |
| 7. Packaging and the switch | |
| 8. Docs | |

## Phase 1: Research

- slot submodule at `e66715d`. Onion (`07505ea`) and Open MainUI (`17fe90d`)
  were cloned into a scratch folder outside the repo; re-clone them with
  `git clone --depth 1` if needed.
- `docs/PLATFORM.md`: every lead marked confirmed, corrected or unresolved.
- `docs/DESIGN-PARITY.md`: screens and effects, and what slot-mini does.
- Reference images: `docs/reference/slot-*.png`, extracted from slot's site
  media with ffmpeg.

## Phase 2: Skeleton and toolchain

- Workspace with `slot-mini-gfx` (software `Canvas`, sprites, an executor for
  slot's `Draw` list, fontdue text with a glyph cache, headless and desktop
  backends) and the `slot-mini` binary.
- Device build: `cargo build --release --target armv7-unknown-linux-musleabihf -p slot-mini`.
  `.cargo/config.toml` links with `rust-lld`, builds C with clang, and turns
  on NEON (`+neon` is needed: the target spec disables it even with
  `target-cpu=cortex-a7`). `scripts/check-device-binary.sh` checks `file`.
- Tests also run on armv7 under `qemu-arm-static`
  (`CARGO_TARGET_ARMV7_UNKNOWN_LINUX_MUSLEABIHF_RUNNER=qemu-arm-static`).
- CI: `.github/workflows/ci.yml`, desktop (fmt, clippy, tests) and device
  (build, `file` check, tests under qemu).

## Phase 3: Carousel

- `slot-mini-store`: slot-store's scanner, cart_shell.txt parser, header
  readers and ini parser, with slot's own tests (Roms/ instead of Games/).
- `slot-mini-ui`: slot-ui's shelf, cart faces, shells, slot chrome, footer,
  icons and legends, unchanged apart from imports and removed GL-only parts.
- `slot-mini`: the app (`app.rs`), background face workers (`faces.rs`,
  adapted from slot's `cart_faces.rs`), slot's label cache, wallpaper picker,
  `theme.txt`, settings and favourites files, a fake SD card (`fixture.rs`).
- Golden tests: `crates/slot-mini/tests/shelf.rs`, images in
  `crates/slot-mini/tests/golden/`. Regenerate with `UPDATE_GOLDEN=1`.
- Side-by-side with slot in `docs/DESIGN-PARITY.md`.

### Performance (host-side, `cargo run --release --example perf -p slot-mini -- 200`)

Fake card with 207 GBA carts (every 4th with a 1280x661 label), scrolling the
whole shelf with Right held (slot's key repeat), 16 ms simulated frames.

| | x86-64 host | armv7 under qemu-arm-static |
| --- | --- | --- |
| First frame, cold caches | 194 ms | 221 ms |
| First frame, warm caches | 15 ms | 62 ms |
| Scroll frame, average | 0.6 ms | 2.4 ms |
| Scroll frame, worst | 12 ms | 4.0 ms |
| Texture memory | 15.8 MB | 15.8 MB |
| Label decode (cold, 1280x661 PNG, worker thread) | | 60 ms |
| Cart face with a text label (worker thread) | | 8.5 ms |

qemu-user on a fast x86 host is not a Cortex-A7, so treat these as relative.
Assuming the A7 is 3-5x slower than qemu here, scroll frames stay well inside
16.7 ms and the warm first frame inside 1 s. Watch on the device: a cold
first launch with a large label set (labels decode in two worker threads
while placeholders show), and texture memory (~16 MB), which the 40 MB budget
covers but not by a lot.

## Phase 4: Launching

- `launch.rs`: the command line MainUI writes, path checks (Open MainUI's
  rules), Recents in the stock record format, Onion's `/tmp/reset_game` for
  Hold A, and the two hand-offs: `/tmp/cmd_to_run.sh` as the boot launcher,
  `.tmp_update/cmd_to_run.sh` plus `/tmp/quick_switch` as an app.
- `main.rs`: `--mode app|launcher`, scripted input (`--input`) and a
  simulated clock (`--sim`) for tests; exit code 10 asks the boot shim for
  Onion's MainUI.
- `tests/fake-runtime.sh`: a stand-in for Onion's runtime loop.
  `tests/launch_cycle.rs` launches through it in both modes, checks the
  command, Recents and the fresh-start flag, and relaunches onto the same
  cart (golden `relaunch_same_cart.png`); plus insert and eject goldens.

## Phase 5: Extras

- Favourites (Y): slot's list format in `App/slot-mini/data/favorites.txt`,
  slot's Favorites shelf first. Tests: `tests/extras.rs`.
- Cart studio (SELECT): slot's `studio.rs` and `studio_view.rs`, drawn by
  `studio_ui.rs`. Shell colour, finish and shape go to `Labels/cart_shell.txt`
  with slot's own writer; printed and recoloured labels go to
  `Labels/<sys>/<stem>.png`. Tests: `tests/studio.rs`.
- Label service: slot's art client (`art/`), behind `label_service = on`
  (off by default) and an `art_host` setting. Background fetch for carts
  without a label (never replaces a file) and the studio's printer. Tests run
  against a local HTTP server: `tests/label_service.rs`.
- Sounds: slot's insert/eject clips played through Onion's ffplay, off by
  default (`sound.rs`).
- About (MENU): Sounds, Boot launcher (install/restore, `switch.rs`) and
  Exit to Onion.
- Battery and clock: `status.rs`, with tests.
- The boot switch logic (`switch.rs`, `package/shim.sh`) is written; its
  shell tests and packaging are phase 7.

## Phase 6: Device backends

- `slot-mini-gfx/src/fbdev.rs`: `/dev/fb0` mapped, frames written rotated
  180° into the hidden page and flipped with `FBIOPAN_DISPLAY` (falling back
  to `FBIOPUT_VSCREENINFO`, then to single-buffered drawing); a larger panel
  gets the frame centred. Tested into a file sized like the framebuffer.
- `slot-mini-gfx/src/evdev.rs`: `/dev/input/event0`, non-blocking, Onion's
  key codes; tested from recorded 32-bit event bytes.
- `backend::Device` combines them; `--backend device` is the default on
  armv7. `SLOT_MINI_FB` and `SLOT_MINI_INPUT` override the paths (a regular
  file as the framebuffer is mapped with the Mini Plus geometry), which
  `tests/device_backend.rs` uses to run the real binary, also under qemu.
- Audio, battery and Wi-Fi were done in phase 5 (`sound.rs`, `status.rs`).
- Memory: every shelf used to upload its own shadow and backing textures,
  and each restock leaked a set. Now shared, the prefetch radius is 4 and
  slot's cached default ground is not kept. Peak RSS on the x86 build,
  scrolling 207 carts: 42 MB before, 33 MB after (textures 10.6 MB).
  `SLOT_MINI_REPORT_RSS=1` prints it on exit.

## Phase 7: Packaging and the switch

- `scripts/package.sh` builds `dist/slot-mini-<version>.zip`: only
  `App/slot-mini/` (binary, `launch.sh`, `config.json`, icon, examples,
  licences, credits, a readable copy of the shim). CI builds it and uploads it.
- `package/shim.sh`: the boot shim, embedded in the binary. Re-executes as
  `sh` so keymon leaves MENU/SELECT/Y/L1 alone, falls back to Onion's MainUI
  (run as a file named `MainUI`) when slot-mini is missing, fails, is asked to
  (exit 10), L2 is held, or `.tmp_update/slot-mini/DISABLED` exists.
- `switch.rs`: install backs each original up to
  `.tmp_update/slot-mini/stock/<name>/MainUI` and verifies it before swapping
  in the shim (atomic rename); restore copies originals back over shims only.
  `--install-launcher` and `--restore-launcher` on the command line too.
- Tests: `package/tests/test-shim.sh` (23 cases: crash fallback, missing
  binary, L2 held, DISABLED, Expert mode, other models, double install,
  restore, Onion updated in between, half-finished install, no original, not
  an Onion card, keymon's process name, POSIX/busybox parse) and
  `package/tests/test-zip.sh` (zip layout, manifest, and the packaged
  `launch.sh` queueing a game against a fake card).

## Phase 8: Docs

- `README.md`: install, uninstall, controls, labels and Cart Studio (with the
  `Games/` workaround), the boot switch, files written, building, testing,
  roadmap, credits, licence and AI disclosure.
- `docs/DEVICE-TESTING.md`: the hardware checklist, app mode first, the boot
  switch last, with the manual recovery steps.
- Unused copied assets (bezels, link-cable art, the sticker) were removed.

## Done means (from the brief)

- [x] CI builds and tests the desktop build and the armv7 release on every push
- [x] The release zip installs as an Onion app with no other changes to the
      card (`package/tests/test-zip.sh`)
- [x] Every in-scope feature works on the desktop build against a fake Onion SD tree
- [x] The boot switch, its fallback and the restore pass their tests
- [x] `PLATFORM.md`, `DESIGN-PARITY.md`, `PROGRESS.md`, `DEVICE-TESTING.md`
      and the README are complete
- [x] Licences and credits for slot, fonts and every Rust crate ship in the zip

Nothing has run on a real Mini Plus. `DEVICE-TESTING.md` is the next step.

## Next

- Run `docs/DEVICE-TESTING.md` on hardware; fix what it finds (most likely
  areas: framebuffer orientation and pixel order, the pan ioctl, keymon
  interplay, the ffplay sound path, first-start time with many labels).
- Roadmap items in the README.

## 0.1.1 (first device feedback)

Tested on a Mini Plus by the maintainer: it works. Three fixes:

- MENU in a game (app mode) showed a black screen with the Game Switcher
  invisible but working. slot-mini now hands the framebuffer back on page 0
  when it exits (`Fbdev::hand_back`), where Onion's programs draw.
- Big jumps (letter skips, shelf switches) flashed every cart as its label
  colour. Every frame now waits up to 500 ms for the faces it shows, as slot
  does. Regression test: `big_jumps_never_flash_placeholders` in
  `tests/shelf.rs`. Under qemu-armv7: worst frame after a jump about 50 ms
  (cold) / 34 ms (warm); held scrolling worst frame 30 ms cold, 13 ms warm.
  Prefetching one more cart each side only shaved the scroll worst case to
  22 ms and raised peak memory to 37 MB, so it stays at four.

- Hold A loaded the auto state anyway. slot-mini 0.1.0 used the
  development branch's `/tmp/reset_game` flag, which released Onion
  (4.3.1-1) does not have. Hold A now writes released Onion's "Reset game"
  command (RetroArch with `.tmp_update/reset.cfg` appended and the core Game
  List Options would pick), keeping the flag only as a fallback where the
  core is chosen at run time. Tests: `launch.rs` unit tests and
  `hold_a_starts_fresh_for_one_run` / `hold_a_falls_back_to_the_dev_runtime_flag`.
  The in-game MENU problem seen after a Hold A launch is the framebuffer page
  issue above (it depended on which page the last frame used).

Also reported: in boot-launcher mode the in-game menu appears after a delay.
That menu is Onion's Game Switcher, which pauses RetroArch and takes a
screenshot first; it is not slot-mini's code. Worth comparing with Onion's
own MainUI on the same game.

## 0.2.0: slot's in-game MENU

Requested after device testing: replace the in-game MENU for games launched
from slot-mini with something like slot's, and make it open faster.

- `ingame.rs`: the helper (`slot-mini --ingame`), started detached beside
  each game slot-mini launches on the device. Tap MENU: slot's in-game menu
  over the frozen game (Resume, Save State, Load State, Settings, Game
  Switcher, Exit Game). Hold MENU (700 ms): eject. MENU combinations with
  other buttons are ignored.
- `slot-mini-ui/src/quick.rs`: slot's quick-menu type, pointer and layout.
- Framebuffer: read and draw a given page without flipping
  (`Fbdev::capture`, `draw_on`, `displayed_page`).
- About: "In-game menu: On/Off" (`ingame_menu` in settings.txt).
- Tests: `tests/ingame.rs` (gestures, every row's effect, eject, the menu
  golden, RetroArch control against a fake RetroArch with a UDP listener,
  and the real helper process end to end: flag, freeze, draw, clean-up).
- Needs the device: does RetroArch on the Mini Plus answer `GET_STATUS` and
  `PAUSE_TOGGLE`, does the frozen game leave the screen alone, and how fast
  does the menu open. See DEVICE-TESTING.md, section 10.

## 0.2.1: in-game menu fixes (device feedback on 0.2.0)

Save and Load State worked on the device. Fixed:

- **Menu sometimes invisible but taking input.** The helper drew on one of
  two pages; RetroArch can use more, and the geometry was read once. Now it
  re-reads the geometry when MENU opens, draws on every mapped page
  (`Fbdev::refresh`, `mapped_pages`, `Screen::pages`) and restores them all.
- **Menu presses reaching the game on resume.** The helper grabs
  `/dev/input/event0` (`Evdev::grab`, `EVIOCGRAB`) while its menu is open
  and performs the chosen row only when every button is up. POWER resumes.
- **No eject animation.** In boot-launcher mode the eject's clock and sound
  started at startup and the load time hid it. It now starts on the first
  frame shown (`NOT_STARTED` in `app.rs`). In app mode there is no eject by
  design: leaving a game goes back to Onion's menu, as before.
- Tests: `ingame.rs` (three-page fake screen, buttons never reach the game,
  POWER resumes), `launch_cycle.rs` (eject starts on the first frame),
  `extras.rs` (eject sound with the first frame).

## 0.2.2: app icon

- The icon in Onion's Apps list is now a Game Boy cartridge in the style of
  Onion's own icons (white outline, dark line, flat colours), with a blank
  label. `cargo run --example icon -p slot-mini` draws it from shapes.

## 0.3.0: updates over Wi-Fi

- `src/update.rs`: GitHub's `releases/latest`, download with progress,
  SHA-256 (ring, already in the tree through rustls), a small zip reader
  (miniz_oxide inflate, also already in the tree), the version check of the
  new binary, the backup-then-rename swap with roll-back, and a thread-backed
  `Updater` the About screen drives.
- About: "Check for updates" above "Exit to Onion"; it reads "Checking",
  "Up to date", "Install update x.y.z", "Downloading n%", "Installing",
  "Updated. Restarting". B is ignored while an install runs.
- `Exit::Restart` -> `execv` of the new binary with the same arguments.
  After a restart an installed boot shim is refreshed (`after_restart`).
- `update_repo` in settings.txt (default `joecrockett5/slot-mini`).
- Tests: `tests/update.rs` (a local stand-in for GitHub; check, download,
  digest and CRC failures, a binary that is not the promised version, only
  app files taken, the full MENU flow to `Exit::Restart`, the About golden
  with an update offered). Checked by hand: the real binary updating itself
  from a local server and `exec`-ing the new one.
- Needs the device: TLS to GitHub (clock, roots), download speed, renaming
  over the running binary on the card's FAT. DEVICE-TESTING.md section 11.

## Open questions

- Audio: does spawning Onion's `ffplay` with `libpadsp.so` play the clips
  promptly and release the device before RetroArch starts? (Device test.)
- Cart Studio looks for a `Games/` folder beside `Labels/`; Onion has `Roms/`.
  The README documents two workarounds; Cart Studio's source is not in the
  slot repo (its `web/` folder is the Gamebit companion), so this could not
  be checked.
- Does Onion's `mount_main_ui` pick up the shim immediately after an install
  from app mode (the replaced file shows as deleted in mountinfo), or only at
  the next boot? Either is safe; DEVICE-TESTING.md asks.
- slot's `house_label.rs` GB label size (1024x901) disagrees with its label
  well (1024x914); one of slot's own tests fails upstream at e66715d.
- slot's art service (`art.slot-cfw.fyi`) has not approved third-party use;
  the client ships disabled.

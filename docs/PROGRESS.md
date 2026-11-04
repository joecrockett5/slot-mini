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

## Open questions

- Audio: does spawning Onion's `ffplay` with `libpadsp.so` play the clips
  promptly and release the device before RetroArch starts? (Device test.)
- Cart Studio looks for a `Games/` folder beside `Labels/`; Onion has `Roms/`.
- slot's art service (`art.slot-cfw.fyi`) has not approved third-party use;
  the client ships disabled.

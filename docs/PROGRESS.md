# Progress

## Status

| Phase | State |
| --- | --- |
| 1. Research | done |
| 2. Skeleton and toolchain | done (pending CI run) |
| 3. Carousel | next |
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

## Open questions

- Audio: does spawning Onion's `ffplay` with `libpadsp.so` play the clips
  promptly and release the device before RetroArch starts? (Device test.)
- Cart Studio looks for a `Games/` folder beside `Labels/`; Onion has `Roms/`.
- slot's art service (`art.slot-cfw.fyi`) has not approved third-party use;
  the client ships disabled.

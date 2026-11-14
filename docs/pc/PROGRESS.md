# slot-mini PC: progress

Brief: `docs/briefs/02-pc-app.md`. Decisions: `docs/DECISIONS.md`, from #62.

## Phase 1: research and design — done

- Code sharing, app folder, window/input/sound stack, RetroArch facts,
  core defaults: DECISIONS #62 to #66.
- Grid view designed, built and rendered (#67, #68). Goldens:
  `crates/slot-mini/tests/golden/grid_*.png`, tests in
  `crates/slot-mini/tests/grid.rs` (need the `pc` feature).

## Phase 2: skeleton — done (pending CI on Windows)

- `crates/slot-mini-pc` 0.1.0. `PcApp` (src/pcapp.rs) is the whole app
  without a window; `main.rs` is the winit/softbuffer/gilrs shell around it
  and carries out its `Request`s. Tests drive `PcApp` headlessly.
- `present.rs` scales the frame (whole numbers when it fits) over a blurred,
  dimmed wallpaper; `input.rs` maps keys, pads and the stick; `config.rs`
  is `config.txt` under `%APPDATA%\slot-mini-pc` (`SLOT_MINI_PC_CONFIG`
  overrides the folder).
- The shared app's PC seam is `crates/slot-mini/src/app_pc.rs`.
- CI: `windows` job (tests + release build), desktop job installs
  `libudev-dev`, armv7 leaves the crate out and runs the grid tests.

## Phase 3: launching — done

- `retroarch.rs`: discovery, cores folder, appended config, the command.
- `PcApp` takes the app's `Exit::Launch`, runs RetroArch on a waiter thread,
  asks the window to hide and show, ejects the cart, then calls the
  `on_game_exit` hooks with `Played { cart, core, save, ok }` (brief 3).
- `fake-retroarch` (src/bin) stands in for RetroArch in `tests/launch.rs`.

## Next

4. Mouse (hover, click, double-click, wheel; view toggle). Keyboard letter
   jump is in (Page Up / Page Down).
5. Feature parity: About/Settings rows for the PC (fullscreen, controller
   layout, folders, Sync placeholder), legend glyphs per device with key
   caps, sound through tinyaudio, update check on `pc-v*`, core download,
   HUD slot for the sync icon, studio by mouse. Goldens for each screen.
6. First-run screen and Import from SD card.
7. Packaging and release (`pc-v<version>`, `update.rs` tag hardening with
   the workspace bump to 0.6.5, CLAUDE.md release lines, icon, single
   instance, long paths).
8. README section, `docs/pc/TESTING.md`.

## How to check

    cargo test --workspace            # device + PC paths (feature `pc` on)
    cargo test -p slot-mini           # the library as the Miyoo ships it
    cargo check -p slot-mini --lib --features pc --target x86_64-pc-windows-gnu
    cargo test -p slot-mini-pc        # the PC app, headless
    cargo check -p slot-mini-pc --all-targets --target x86_64-pc-windows-gnu
    cargo run -p slot-mini-pc --example fake_library -- /tmp/lib   # games to try it on

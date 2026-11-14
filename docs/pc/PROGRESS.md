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

## Phase 4: grid view and mouse — done

- Grid: DECISIONS #67, #68. Mouse: #69 (`App::pointer` in `app_pc.rs`).

## Phase 5: feature parity — in progress

Done (DECISIONS #70 to #75):

- MENU's PC rows (`crates/slot-mini/src/about_pc.rs`), goldens
  `pc_menu*.png`, tests `crates/slot-mini/tests/pc_menu.rs`; `PcApp` carries
  them out (tests at the bottom of `pcapp.rs`, against a local server).
- Legends per device with key caps (`legend.rs`), goldens `pc_*_keyboard`.
- Sound (`audio.rs`), update check and core download (`net.rs`), dialogs
  and the browser (`desktop.rs`), the status icon slot.

Left in phase 5:

- A shelf whose core is missing should say so before a game is inserted
  (today only the insert notice does).
- The cart studio by mouse.

## Next

6. First-run screen and Import from SD card (`PcApp::chose_card` is a stub).
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

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
- `present.rs` puts the frame in the window between black bars; `input.rs` maps keys, pads and the stick; `config.rs`
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

## Phase 5: feature parity — done

(DECISIONS #70 to #75):

- MENU's PC rows (`crates/slot-mini/src/about_pc.rs`), goldens
  `pc_menu*.png`, tests `crates/slot-mini/tests/pc_menu.rs`; `PcApp` carries
  them out (tests at the bottom of `pcapp.rs`, against a local server).
- Legends per device with key caps (`legend.rs`), goldens `pc_*_keyboard`.
- Sound (`audio.rs`), update check and core download (`net.rs`), dialogs
  and the browser (`desktop.rs`), the status icon slot.

- A shelf whose core is missing says so when it comes up
  (`name_missing_core`), and the studio works by mouse (`studio_view::hit`);
  tests in `crates/slot-mini/tests/mouse.rs`.

## Phase 6: first run and import — done

- DECISIONS #76, #77. `crates/slot-mini-pc/src/import.rs` (tests inside),
  `PcApp::chose_card`, the welcome screen in `app_pc.rs`
  (`pc_first_run*.png`).

## Phase 7: packaging and release — done

- DECISIONS #78 to #81. `build.rs` (icon, version, manifest),
  `main.rs` `instance` (one at a time), `scripts/package-pc.sh`,
  `scripts/release-pc.sh`, `scripts/third-party.py --pc`, CI jobs
  `windows` -> `pc-package` -> `release`. `package/pc/` holds the zip's
  README and credits.
- The Miyoo's updater takes only `v<x.y.z>` tags from 0.6.6
  (`update::device_tag`).

## Phase 8: docs — done

- README "slot-mini PC", `docs/pc/TESTING.md`, CLAUDE.md's release lines.

## After the first Windows test (0.1.1) — done

The maintainer ran `docs/pc/TESTING.md` on Windows and reported seven
things; DECISIONS #82 to #88.

- Closing hung, and so did turning sounds off: both were the audio
  device's thread. Sound is now `PlaySoundW` (`audio.rs`), and quitting
  ends the process once the config is written.
- Controller: Select is MENU, X the studio, a stick click the Miyoo's
  SELECT (`input::pad`, `PcApp::pad_button`); new installs use Xbox labels.
- The shortcut bar and the mouse's buttons: `crates/slot-mini/src/
  app_bar.rs`, `Glyphs::Mouse`, `slot_mini_ui::button_face`; tests and
  goldens in `crates/slot-mini/tests/pc_bar.rs` (`pc_bar_*`, `pc_*_mouse`).
- A game takes slot-mini's place on screen (`retroarch::Screen`).
- Black bars instead of the blurred wallpaper (`present.rs`).
- The frame is drawn at up to four times 640x480 (`Renderer::render_scaled`,
  `PcApp::set_scale`), golden `pc_grid_2x`.
- Not verified here (no Windows): that `PlaySoundW` is heard, and that
  RetroArch honours `-f` and the window position. `docs/pc/TESTING.md`
  has the steps.

## 0.2.1: the grid's row height, once a frame

Brief 4 (the Miyoo's grid) found that the shared grid worked out its row
height by walking the shelf once per cart per frame. It is now worked
out once a walk (`crates/slot-mini-ui/src/grid.rs`). Nothing the PC
shows changed: every PC golden is byte-identical. The Miyoo's pacing
and lean faces are behind `!paths.is_pc()` and do not apply here.

## Not done, and why

- No sync: brief 3. The seams are the MENU Sync page, the status icon
  slot, `PcApp::on_game_exit` (`Played`), and the per-game core choice.
- Never run on a real Windows desktop by a person: CI builds and tests it
  there headlessly. `docs/pc/TESTING.md` is the checklist.
- The exe is unsigned (SmartScreen note in the READMEs).

## How to check

    cargo test --workspace            # device + PC paths (feature `pc` on)
    cargo test -p slot-mini           # the library as the Miyoo ships it
    cargo check -p slot-mini --lib --features pc --target x86_64-pc-windows-gnu
    cargo test -p slot-mini-pc        # the PC app, headless
    cargo check -p slot-mini-pc --all-targets --target x86_64-pc-windows-gnu
    SLOT_MINI_PC_EXE=target/x86_64-pc-windows-gnu/release/slot-mini-pc.exe scripts/package-pc.sh   # after a mingw cross build
    cargo run -p slot-mini-pc --example fake_library -- /tmp/lib   # games to try it on

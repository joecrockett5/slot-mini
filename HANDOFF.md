# Handoff: building out the roadmap

Delete this file before opening the next PR. It is working notes, not part of the project.

## The task

The maintainer asked for every item on the README roadmap to be built, **one PR per item**, each merged once its
CI passes (they gave standing permission to merge). slot may be used as a read-only reference.

Read `CLAUDE.md`, `PROMPT.md` and `docs/PROGRESS.md` first. The rules that matter most:

- Never build or depend on slot. Clone it outside the tree to read it:
  `git clone https://github.com/BrandonKowalski/slot target/slot && git -C target/slot checkout e66715d`
  (`target/` is ignored). Files copied from slot carry a provenance header; `scripts/copy-from-slot.sh
  <path in slot> <dst> "<what changed>"` writes it.
- Shipping a change: bump `version` in the workspace `Cargo.toml`, push, open a PR into `main`, merge when CI is
  green. Merging publishes a release to devices.
- Keep `docs/PROGRESS.md` current (one section per version).

## Status

| # | Roadmap item | Version | State |
| --- | --- | --- | --- |
| 1 | Recents shelf | 0.6.0 | **Merged** ([joecrockett5/slot-mini#22](https://github.com/joecrockett5/slot-mini/pull/22)) |
| 2 | Per-shelf wallpapers | 0.6.1 | **In progress** on this branch: code builds and passes; tests, docs and version bump still to do |
| 3 | Re-tinting logo labels after the studio is closed | 0.6.2 | Not started |
| 4 | Label printer print-and-tear, and the studio cart's apply animation | 0.6.3 | Not started |
| 5 | Sticker on MENU that peels in | 0.6.4 | Not started |

The version numbers are my plan, not a requirement.

## Decisions the maintainer made (asked and answered)

- **Recents** (done): slot-mini's own list of the last 10 games launched from it, `data/recents.txt` in slot's
  format; X removes a cart; a MENU row, on by default.
- **Wallpapers**: automatic by file name. `GBA.png`, `GBC.png`, `GB.png`, `Favorites.png`, `Recents.png` in
  `Wallpapers/` (any case) are used on their shelf and **left out of the random pick**. Shelves without a file keep
  the random wallpaper, or slot's wood. No new setting.
- **MENU sticker**: bring slot's device sticker to MENU, reworded for the Mini Plus (no Anbernic wordmark), holding
  slot-mini's version and credits. It rolls on with slot's sticker-curl maths when MENU opens. The setting rows
  stay below it.

## Item 2: per-shelf wallpapers (where it stands)

Done, in the commit that carries this file:

- `crates/slot-mini/src/wallpaper.rs`: `SHELF_KEYS`, `of_shelf(root, key)`, and `pick` now skips shelf-named
  files. The provenance header's "Changes" line is updated.
- `crates/slot-mini/src/app.rs`: `App::load_shelf_grounds` (called after `restore` in `App::new`) loads the
  wallpaper of the shelf on screen at once and the others on `WallpaperLoader`'s thread;
  `take_shelf_grounds` (in `tick`) uploads them as they land; `App::ground()` returns the current shelf's
  wallpaper, else the random one, and is used by the shelf background, MENU and the studio.
  `App::shelf_wallpapers()` counts those loaded, so tests can `wait_until` on it.
- `cargo fmt --check`, clippy (`-D warnings`) and `cargo test --workspace` pass.

Still to do:

- Tests. Suggested, in `tests/shelf.rs` beside `wallpaper_theme_and_status`: put `blur.png` and a visibly
  different `gbc.png` in `Wallpapers/`; check the GBA shelf shows the random one, R1 to GBC shows `gbc.png`
  (wait with `d.wait_until(.., |d| d.app.shelf_wallpapers() == 1)`), a golden for it, and that a start on a
  shelf with its own file has it on the first frame. Add a unit test in `wallpaper.rs` that `pick` never returns
  a shelf-named file, and that it returns `None` when only shelf-named files exist.
- Check MENU and the studio over a shelf wallpaper look right (they use `ground()` now).
- Docs: README "Customisation → Wallpaper" and the roadmap list (remove the item), `docs/PROGRESS.md` (0.6.1
  section), `docs/DESIGN-PARITY.md`, `docs/DEVICE-TESTING.md`, and a `docs/DECISIONS.md` entry (next number is
  58) for "shelf-named files are not in the shuffle".
- Bump to 0.6.1. The `about*` goldens show the version, so they must be redrawn (see below).

## Notes for items 3 to 5 (from reading slot at `e66715d`)

**3. Logo re-tint.** slot keeps the downloaded logo (`wheel.png`) in its studio cache so a logo label can be
re-tinted later. See `crates/slot/src/art/jobs.rs` (`studio_art_with`, `logo_tag`, `make::relabel`,
`logo_worn`), `crates/slot/src/studio_faces.rs` (`logo_art`: `Label::from_png(wheel, auto, platform, band)`,
`label.set_deep(tint)`), and `crates/slot-ui/src/house_label.rs`, which is already copied into `slot-mini-ui`.
slot-mini's side is `crates/slot-mini/src/studio.rs`, `studio_ui.rs`, `art/`.

**4. Print-and-tear and apply.** slot's `crates/slot/src/studio.rs` around lines 604 to 759 is not in slot-mini's
copy: `ApplyStage {Peel, Toss, Lift, ToCart, Roll}`, `APPLY_STAGES` (240/200/130/220/220 ms), `ApplyScene`,
`Slip`, `tray_step`, `printer_light`, `slip_zoom`, `backing_shift`, `slip_shuffle`. The drawing is in slot's
`crates/slot/src/app.rs` near 6825, 7110 to 7400 (`studio_sticker`) and 7557. It draws `Draw::Quad` strips from
`sticker_curl::strips`. **slot-mini's software renderer does not draw `Draw::Quad` or `Draw::Paper` yet**
(`crates/slot-mini-gfx`), so that comes first. Item 5 needs it too, so doing 4 before 5 saves work. This is the
largest item.

**5. MENU sticker.** `crates/slot-ui/src/sticker.rs` (660x228, `StickerFields`, `sticker_face`, `draw_sticker`;
drop the Anbernic wordmark assets) and `crates/slot-ui/src/sticker_curl.rs` (`Pose`, `Strip`,
`strips(rect, flat, curl_deg, pose, focus)`; 8 px strips, camera at 800). slot draws its About sticker flat; the
peel-in is the maintainer's choice above. MENU (`crates/slot-mini/src/about.rs`) has seven rows now at a 33 px
pitch; the four info lines at the top are what the sticker replaces.

## How to work here

- Branch: `claude/clever-goldberg-slrobc` for every PR. After a PR merges, restart it from `main`:
  `git fetch origin main && git checkout -B claude/clever-goldberg-slrobc origin/main`, then
  `git push --force-with-lease -u origin claude/clever-goldberg-slrobc`.
- Checks CI runs (run them before pushing):
  - `cargo fmt --all --check`
  - `cargo clippy --workspace --all-targets --features slot-mini/desktop -- -D warnings`
  - `cargo test --workspace`
- Goldens: `crates/slot-mini/tests/golden/*.png`. `UPDATE_GOLDEN=1 cargo test -p slot-mini --test <name>`
  rewrites **every** golden those tests touch, including ones whose pixels did not change. Afterwards
  `git checkout` any golden you did not mean to change and rerun the tests without the variable. After a version
  bump, redraw the MENU ones: `UPDATE_GOLDEN=1 cargo test -p slot-mini --test recents --test extras --test update`,
  then restore `shelf_favorites.png` and `shelf_hearted.png`.
- Look at new goldens before committing them.
- `label_service::missing_labels_are_fetched_and_local_files_win` failed once in a full local run and passed on
  every rerun. It waits on a background thread. CI passed.
- MENU rows: tests that walk to a row by counting Down presses (`tests/extras.rs`, `tests/recents.rs`) break if a
  row is added above the one they want.
- PRs: no template in the repo. Say what changes for players, how, and what was tested; say plainly that it was
  not checked on a device. CI has a desktop job and an armv7 (qemu) job; both must be green before merging.

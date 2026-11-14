# slot-mini PC: progress

Brief: `docs/briefs/02-pc-app.md`. Decisions: `docs/DECISIONS.md`, from #62.

## Phase 1: research and design — done

- Code sharing, app folder, window/input/sound stack, RetroArch facts,
  core defaults: DECISIONS #62 to #66.
- Grid view designed, built and rendered (#67, #68). Goldens:
  `crates/slot-mini/tests/golden/grid_*.png`, tests in
  `crates/slot-mini/tests/grid.rs` (need the `pc` feature).

## Phase 2: skeleton — next

- [ ] `crates/slot-mini-pc` (version 0.1.0): window, scaling, input, PC paths
- [ ] CI: Windows job, Linux job builds it headlessly, armv7 excludes it

## Later phases

3 launching, 4 grid (view done; mouse and keyboard letter jump left),
5 feature parity, 6 import and first run, 7 packaging and release, 8 docs.

## How to check

    cargo test --workspace            # device + PC paths (feature `pc` on)
    cargo test -p slot-mini           # the library as the Miyoo ships it
    cargo check -p slot-mini --lib --features pc --target x86_64-pc-windows-gnu

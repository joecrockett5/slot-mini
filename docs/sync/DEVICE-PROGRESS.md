# Brief 4 progress: sync and the grid view on the Miyoo

Brief: `docs/briefs/04-device-sync.md`. Decisions: `docs/DECISIONS.md`, from #101.

## Done

- **Phase 1, client wiring.** `crates/slot-mini/src/sync.rs`: one worker
  thread, the client opened per job, a gate that keeps the worker off the
  card once a launch is on its way. Inert until paired.
- **Phase 2, pairing and Sync now.** MENU -> Settings -> Sync (pair, Auto
  sync, Unpair), MENU -> Sync now with progress and summary, the status
  icon beside Wi-Fi. Tests: `crates/slot-mini/tests/sync.rs`.
- **Phase 3, automatic syncs.** At start, on a poke, beside a launch
  (capped), and after a game: slot-mini's own start in boot-launcher mode,
  the in-game helper's push in app mode (`--push`, `ingame.rs`). Tests with
  the real binary in the fake runtime: `crates/slot-mini/tests/sync_cycle.rs`.

- **Phase 4, the grid on the Miyoo.** R2 and MENU -> Settings -> View,
  kept per shelf in `data/views.txt`, the scrolling shelf by default.
  Made to fit the device: the row height worked out once a frame, the
  next row built ahead, a held Up or Down paced, and only the selected
  cart kept whole (decisions #115, #116). Tests:
  `crates/slot-mini/tests/device_grid.rs`, goldens `miyoo_grid_*.png`;
  the carousel's goldens are byte-identical.
- **Phase 5, budgets and docs.** First frame, idle, memory and binary
  size are in decision #117; `crates/slot-mini/tests/sync_budget.rs`
  holds the real binary under 40 MB while Sync now moves 96 MB of ROMs.
  `docs/DEVICE-TESTING.md` sections 19 to 21, the README's Sync section,
  controls and files table. Shipped as 0.7.0.

## Not done, and why

- Nothing here has run on a Miyoo. Sections 19 to 21 of
  `docs/DEVICE-TESTING.md` are the checks to make, safest first.
- A game started from Onion's own menu is synced at slot-mini's next
  start, not around the game (decision #107).

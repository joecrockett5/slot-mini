# Brief 4 progress: sync and the grid view on the Miyoo

Brief: `docs/briefs/04-device-sync.md`. Decisions: `docs/DECISIONS.md`, from #89.

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

## Next

- Phase 4: the grid view on the device.
- Phase 5: budgets, `docs/DEVICE-TESTING.md`, README, decisions, version.

# Sync core: progress

Brief: `docs/briefs/01-sync-core.md`. Specification: `docs/SYNC.md`.

| Phase | State |
| --- | --- |
| 1. Design | done: `docs/SYNC.md` |
| 2. Core | done: model, hash cache, planner, merges, layouts, hub and client; unit tests and `tests/properties.rs` (seeded random libraries, 300 seeds in CI, 4000 run by hand) |
| 3. Transport | done: HTTP server and client, pairing, signing, discovery and poke; `tests/transport.rs` over localhost |
| 4. Harness | in progress |
| 5. Docs | not started |

## Next

Phase 4: `crates/slot-mini-sync/tests/scenarios.rs`, the twelve scenarios
of the brief over the in-process link (`tests/support`), built on
`slot_mini::fixture`. Then a Windows check of the crate in CI.

Phase 5: bring `docs/SYNC.md` in step with the code (the notes are in
the phase 5 list of the brief, plus: a label, art file or save takes the
receiving side's spelling of the game; orphan saves are listed; a save
already there with the same content is not landed again; what the hub
holds of a removed game is new; the limits found by the property tests),
`docs/DECISIONS.md`, the README row, the armv7 size of the `client`
feature, the version bump and the PR.

`cargo test --workspace` runs everything; on armv7 set
`CARGO_TARGET_ARMV7_UNKNOWN_LINUX_MUSLEABIHF_RUNNER=qemu-arm-static`.
`SYNC_CASES` and `SYNC_SEED` choose how many random libraries the
property test runs and where it starts.

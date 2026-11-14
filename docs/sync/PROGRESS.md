# Sync core: progress

Brief: `docs/briefs/01-sync-core.md`. Specification: `docs/SYNC.md`.

| Phase | State |
| --- | --- |
| 1. Design | done: `docs/SYNC.md` |
| 2. Core | done: model, hash cache, planner, merges, layouts, hub and client; unit tests and `tests/properties.rs` (seeded random libraries, 300 seeds in CI, 4000 run by hand) |
| 3. Transport | done: HTTP server and client, pairing, signing, discovery and poke; `tests/transport.rs` over localhost |
| 4. Harness | done: `tests/scenarios.rs`, the twelve scenarios; they run on armv7 under qemu in the device job, and CI checks the crate for Windows |
| 5. Docs | done: `docs/SYNC.md` in step with the code, `docs/DECISIONS.md`, the README row, version 0.6.5 |

## State

Brief 1 is complete. Nothing in the launcher uses the crate yet: briefs 3
(PC) and 4 (Miyoo) do, from the "Crate" section of `docs/SYNC.md`.

The `client` feature adds 258,624 bytes to the static armv7 release binary
(5,276,836 to 5,535,460, 4.9%), measured by linking a call of every client
entry point into `slot-mini`.

`cargo test --workspace` runs everything; on armv7 set
`CARGO_TARGET_ARMV7_UNKNOWN_LINUX_MUSLEABIHF_RUNNER=qemu-arm-static`.
`SYNC_CASES` and `SYNC_SEED` choose how many random libraries the
property test runs and where it starts.

## Open

- The limits in `docs/SYNC.md`, "Limits and open points"; moves detected
  by hash are the one left for the maintainer to decide.
- Nothing here has run on a real Miyoo or over real Wi-Fi: the transport
  tests use localhost. Brief 4's device checklist should cover a sync.

# Sync in the PC app: progress (brief 3)

The brief is `docs/briefs/03-pc-sync.md`. The contract is `docs/SYNC.md`.
Decisions are in `docs/DECISIONS.md` under "Brief 3", from #89.

## Where things are

| Phase | State |
| --- | --- |
| 0. Hub additions the PC needs (contract PR) | merged, joecrockett5/slot-mini#30 |
| 1. Hub wiring | done |
| 2. The Sync page, pairing and status | done |
| 3. Post-game poke, conflicts, Remove everywhere | the poke is wired (phase 1); the rest to do |
| 4. First run and results | to do |
| 5. Docs and release | to do |

## Phase 0: what the hub tells the PC

Merged on its own so that brief 4 could rebase (decision #89): `Transfer`
events carry the plan's count and the file's size, `Hub::is_syncing_game`,
and a conflict's record says which side its copy came from and when each
copy was last played. Nothing on the wire changed.

## Phase 1: hub wiring

- `crates/slot-mini-pc/src/sync.rs`: `Service` owns the `Hub` over the
  library and its `Server` (TCP and UDP discovery on 24735). Hub events
  come over a channel and are taken once a frame in
  `PcApp::poll_workers`, so there is no new timer.
- The hub's state is in `App/slot-mini-pc/data/sync/` in the library. The
  PC's name is the hub's (`hub.txt`): the computer's name the first time,
  the library's from then on.
- `sync::Cores` is the `CoreChoice`: the cart's `.game_config`, else the
  PC's default, the same rule as `cores::chosen`.
- `PcApp::launch` marks the game busy before RetroArch starts and refuses
  to start a game the sync in flight is moving ("Syncing this game: try
  again in a moment"). `game_over` clears it and pokes paired Miyoos.
- `PcApp::close` stops the server. `PcOptions::sync` is `None` in tests
  that want no network, `Net::local(port)` in the sync tests.
- A `Changed` event asks for a rescan once the shelf is at rest.

Exit test: `crates/slot-mini-pc/tests/sync_hub.rs`, with the real client
over HTTP on loopback (`tests/common/mod.rs` has the rig and the Miyoo):
an empty library is seeded and its shelves fill; a game running in the
stand-in RetroArch is reported busy and its save left alone; the Miyoo's
poke listener hears the poke after the game; discovery answers while the
app runs and stops with it; a game whose save is mid-transfer does not
start.

## Phase 2: the Sync page, pairing and status

- The shared MENU draws a `Page` the host supplies (decision #90):
  `crates/slot-mini/src/about_pc.rs` (`Page`, `PageRow`, `HostRow::Page`,
  `PageTurn`, `PageBack`) and the drawing in `about.rs`. All `pc`-gated.
- `crates/slot-mini-pc/src/sync_ui.rs` is the PC's side: a stack of
  `Screen`s (Main, Name, Pairing, Devices, Device, Unpair, Conflicts,
  Conflict, Dismiss, Remove, Firewall), each turned into a `Page` by
  `sync_page`, and `page_action` for what a row does. `sync_event` turns
  hub events into pairing state, progress and the summary; `sync_tick`
  notices a poke nobody answered and a sync that stalled.
- The status icon is `App::set_status` with `status_face` (decision #93).
- Typing reaches the name editor from `main.rs` (`wants_text`, `text`,
  `backspace`).
- The Conflicts, Conflict, Dismiss and Remove screens exist but nothing
  tests them yet, and nothing opens Remove: phase 3. `App::set_marked`
  stores which carts are in conflict; drawing the badge is phase 3.

Exit tests: `crates/slot-mini-pc/tests/sync_pages.rs` (pairing through
the screens with the real client, Deny, Unpair, the name, Sync now with
and without an answer, the icon), with a picture of each screen in
`crates/slot-mini-pc/tests/golden/sync_*.png`; and
`crates/slot-mini/tests/pc_menu.rs` for the page mechanics.

## How to run

```bash
cargo test -p slot-mini-pc
```

```bash
cargo test -p slot-mini-sync --all-features
```

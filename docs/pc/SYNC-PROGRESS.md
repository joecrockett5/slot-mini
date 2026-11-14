# Sync in the PC app: progress (brief 3)

The brief is `docs/briefs/03-pc-sync.md`. The contract is `docs/SYNC.md`.
Decisions are in `docs/DECISIONS.md` under "Brief 3", from #89.

## Where things are

| Phase | State |
| --- | --- |
| 0. Hub additions the PC needs (contract PR) | merged, joecrockett5/slot-mini#30 |
| 1. Hub wiring | done |
| 2. The Sync page, pairing and status | done |
| 3. Post-game poke, conflicts, Remove everywhere | done |
| 4. First run and results | done |
| 5. Docs and release | done: slot-mini PC 0.2.0 |

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

Exit tests: `crates/slot-mini-pc/tests/sync_pages.rs` (pairing through
the screens with the real client, Deny, Unpair, the name, Sync now with
and without an answer, the icon), with a picture of each screen in
`crates/slot-mini-pc/tests/golden/sync_*.png`; and
`crates/slot-mini/tests/pc_menu.rs` for the page mechanics.

## Phase 3: conflicts and Remove everywhere

- The post-game poke was wired in phase 1 (`game_over`).
- Conflicts: the Conflicts list, a conflict's sheet (both copies, whose
  each is, when each was last played), "Use the other copy" (the hub's
  `swap_conflict`, which keeps the auto-state rule) and "Keep this one",
  which asks first. The badge is `App::draw_marks` in
  `crates/slot-mini/src/app_pc.rs` (decision #97), placed with
  `Shelf::placed` on the shelf and the grid's own cells.
- Remove everywhere (decision #96): `PcApp::remove_current` from the
  Delete key (`main.rs`), `Pointer::Context` for a right click on the
  selected cart, and MENU's "Remove this game" row (`Host::remove`,
  `HostRow::Remove`). The sheet says what goes and where the save is
  kept; afterwards the shelf rescans and a notice says so.
- `PcOptions::sync: None` still opens the hub, off the network, so the
  row is there in the unit tests too.

Exit tests: `crates/slot-mini-pc/tests/sync_games.rs`: a conflict made
with the real client is badged, listed, swapped (the auto state set
aside with the save that leaves, the Miyoo given the chosen save) and
dismissed after a Cancel; Remove everywhere from each of its three ways
in, gone from the PC at once and from the Miyoo at its next sync.

## Phase 4: first run and results

- The first-run screen (decision #98): `Host::offer` (`Offer`: the
  paired Miyoo's name and a line of progress) from `PcApp::offer`;
  `welcome_lines` in `crates/slot-mini/src/app_pc.rs` words it; the bar
  has `Item::Pair` and `Item::Card`; A is `HostRow::Pair`
  (`pair_from_shelf`: the pairing screen, or the Sync page once paired)
  and START is `HostRow::Import`. Without a sync service the old screen
  stands, with the import on A.
- Results (decision #99): `summary` is the shelf's notice after a sync
  that moved something, and the Sync page's last line always.
- Refresh (decision #100): `relabel_due` in `PcApp`, set when a finished
  sync brought labels or shell lines, makes the next rescan restock even
  when the carts are the same.

Exit tests: `crates/slot-mini-pc/tests/sync_first_run.rs` goes from an
empty library to a seeded one through the first-run screen with the real
client (pairing on A, the prompt for Sync now, progress on the screen,
the shelves filling, the summary, a quiet no-op sync, and a label
changed on the Miyoo showing on the PC's shelf without a restart).
`crates/slot-mini/tests/pc_menu.rs` has the pictures of the screen
(`pc_first_run_pair*.png`, `pc_first_run_paired.png`,
`pc_first_run_seeding.png`).

## Phase 5: docs and release

- `README.md` and `package/pc/README.txt` (the zip's) describe sync: the
  firewall prompt and what to do after Cancel, pairing, conflicts,
  Remove everywhere.
- `docs/pc/TESTING.md` section 7 is the checklist for real hardware. None
  of it has been run: it needs a Windows PC and a Miyoo with brief 4's
  side of sync, which does not exist yet.
- `crates/slot-mini-pc` is 0.2.0. The workspace version is unchanged: the
  shared crates' changes are all behind the `pc` feature, the device
  goldens are untouched and the Miyoo's binary is the same.

## What is left

- Brief 4: the Miyoo's side (MENU -> Sync, pairing, Sync now, the poke
  listener). Until it ships, the PC's hub has only the tests' client to
  talk to.
- The real-hardware checklist above, once brief 4 is on a Miyoo. The
  wording of the firewall page is from Windows 11's documentation, not
  from seeing the prompt.

## How to run

```bash
cargo test -p slot-mini-pc
```

```bash
cargo test -p slot-mini-sync --all-features
```

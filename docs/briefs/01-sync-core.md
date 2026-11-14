# slot-mini companion, brief 1 of 4: the sync core

You are working in the `joecrockett5/slot-mini` repository. This file is your whole brief (its file name is `01-sync-core.md`). The shared sections come first, then your own.

## First steps

1. Commit this brief, unchanged, as `docs/briefs/<this brief's file name>` in your first commit, and add one line to `CLAUDE.md` pointing to it, so that a fresh session can pick up from it.
2. Read `CLAUDE.md`, `README.md`, `docs/DECISIONS.md`, `docs/PLATFORM.md` and `docs/PROGRESS.md`. Then read the code your brief touches before you design anything. slot-mini is a mature codebase with strong conventions. Follow them, don't invent new ones.

## The whole project

slot-mini (this repo) is a slot-style launcher for the Miyoo Mini Plus on Onion OS. It is being extended into a pair of apps:

- **slot-mini PC** (`crates/slot-mini-pc`): a Windows frontend for standalone RetroArch with slot-mini's look, built on slot-mini's UI crates. Windows comes first, but it must also build and run on Linux so that it can be tested headlessly in CI and in agent sandboxes.
- **Sync** between the PC and the Miyoo when both are on the same Wi-Fi network.

The work is split across four briefs:

| # | Brief | Starts after |
| --- | --- | --- |
| 1 | `01-sync-core.md`: the shared sync crate, `crates/slot-mini-sync` | – |
| 2 | `02-pc-app.md`: the PC app, without sync | – (runs in parallel with 1) |
| 3 | `03-pc-sync.md`: sync in the PC app | 1 and 2 are merged |
| 4 | `04-device-sync.md`: sync and grid view on the Miyoo | 1 and 2 are merged |

Other agents may be working in the same repo at the same time. Stay inside your brief and rebase on `main` before you merge. Shared files (`CLAUDE.md`, `README.md`, `docs/DECISIONS.md`, CI) are where conflicts happen; resolve them by keeping both sides' changes.

## Agreed decisions

The maintainer made these decisions. Do not reopen them. If one turns out to be impossible, stop and say why.

1. **One library.** The PC's library and the Miyoo's are the same library. The first sync seeds an empty PC library from the Miyoo. After that, games, labels and art added on either side appear on the other.
2. **What syncs, and when.**

   | Kind | Files | Auto sync | Manual sync |
   | --- | --- | --- | --- |
   | ROMs | `Roms/{GBA,GBC,GB}/`, including the one subfolder level slot-mini scans | no | yes |
   | Labels and art | `Labels/{GBA,GBC,GB}/<stem>.png`, `Roms/<sys>/Imgs/<stem>.png` | yes | yes |
   | Shells | `Labels/cart_shell.txt`, merged per game line | yes | yes |
   | Favourites | each side's `favorites.txt`, merged per entry (platform + stem) | yes | yes |
   | In-game saves | `<stem>.srm` and `<stem>.rtc` per game | yes | yes |

   Not synced: save states, Recents (each device keeps its own), settings, theme, wallpapers, the per-game core choice (`.game_config`) and caches.
3. **Save states are not synced.** When a newer in-game save arrives on either side, that side's auto save state for the game is set aside. It is renamed with the `.bak_<stamp>` scheme `cores::carry_save` already uses, never deleted. Otherwise Tap A could resume an older session and overwrite the newer save. The maintainer accepted the consequence: after playing on the other device, a game starts from its title screen and you continue from the in-game save.
4. **Saves belong to games, not cores.** Each side keeps its saves in RetroArch's per-core folders (`Saves/CurrentProfile/saves/<core>/<stem>.srm`). Sync maps "the save for game X" to whichever core folder that game uses on each side. `.srm` files work in both mGBA and gpSP (Onion's transfer script and `cores::carry_save` rely on this), and Gambatte is used for GB/GBC on both sides. A save file that is all `00` or all `ff` counts as no save.
5. **Deletions don't propagate**, except through an explicit **Remove everywhere** on the PC. It removes the game's ROM, labels, art, shell line and favourite on both sides, and moves its saves into a backup folder instead of deleting them.
6. **Conflicts.** If both sides changed the same save since the last sync, keep both. The more recently played copy becomes the save, and the other is kept beside it as a conflict copy that RetroArch will not load. The PC app lets the user swap them. For labels, art and shell lines, the PC's version wins and nothing extra is kept.
7. **Topology.** The PC app runs the sync server and the Miyoo is the client: the Miyoo starts every sync. "Sync now" on the PC sends a UDP poke, and a Miyoo sitting on its shelf answers it by syncing. Same LAN only. **No off-LAN work**: no hub on another server, no relay, no Tailscale. The maintainer will choose that strategy later, so don't build toward one.
8. **Changes are detected by content hash** against a per-device record of what was last synced, never by modification time alone. The Miyoo's clock is unreliable.
9. **Grid view.** Any shelf can show a grid of carts instead of slot's scrolling shelf. The choice is made per shelf and remembered. The Miyoo defaults to the scrolling shelf. The PC defaults to the grid.
10. **PC input** comes from the keyboard, a game controller and the mouse.
11. **PC features:** the shelf and grid, Favourites, Recents, the cart studio, the label printer (online labels), the GBA core picker, wallpapers, the theme, sounds, the About sticker and an update check. Not on the PC: slot's in-game menu (RetroArch's own menu is used), the boot launcher and splash screens.
12. **RetroArch on the PC** is the standalone Windows build (installer or portable zip), not the Steam version.
13. **One repo.** Everything lives in this Cargo workspace.

## Conventions every brief keeps

- **The device comes first.** Unless your brief says otherwise, slot-mini's behaviour on the Miyoo does not change. Existing golden images stay byte-identical. The device budgets still hold: first frame within 1 second, a steady 60 fps, under 40 MB resident memory, and no new timers or polling that wake the CPU while idle (see 0.4.2 and 0.4.6 in `docs/PROGRESS.md` for why this matters). The armv7 CI job stays green.
- **There are two release lines.**
  - The device release is `v<x.y.z>`, from the workspace `version`. It reaches Miyoos over the air when merged to `main`.
  - The PC release is `pc-v<x.y.z>`, from `slot-mini-pc`'s own `version` (brief 2 sets this up).

  Follow `CLAUDE.md`'s standing shipping instruction for each line. Bump the workspace version only when what the Miyoo runs changes, and bump the PC version only when the PC app changes.
- **Networking.** Use blocking I/O on worker threads, with no async runtime. Use `ureq` for HTTP clients, as `update.rs` and the label service already do. Use `ring` for cryptography, which the device binary already links.
- **Copied code.** Anything copied from slot keeps slot-mini's provenance-line convention. Anything copied from elsewhere must be GPL-3.0-or-later compatible and credited.
- **Records.** Append decisions to `docs/DECISIONS.md` under a heading for your brief, with the reason for each. Keep your brief's progress file current at every phase exit and before you stop.
- **Tests.** Write tests as you go. Test network code against local servers, never the internet. Look at rendered PNGs yourself before calling a screen done: a passing golden test only proves the image didn't change.
- **Stopping.** Work autonomously through every phase. Stop and ask only if an agreed decision proves impossible, a change could leave a Miyoo unable to boot or lose a save, or a device budget can't be met without cutting scope.

## Your brief: the sync core

**Goal:** build `crates/slot-mini-sync`, a library both sides use. It holds everything about sync that needs no window and no device:

- the data model and hashing
- the planner and the merges
- the save rules
- the wire protocol
- pairing and authentication
- discovery and poke
- a blocking HTTP server (feature `server`) and client (feature `client`)
- an in-process harness that syncs a fake PC library and a fake Miyoo SD card against each other

Brief 3 embeds the server in the PC app and brief 4 embeds the client in slot-mini. **This brief changes no shipped behaviour** on either side.

Your progress file is `docs/sync/PROGRESS.md`. Your specification is `docs/SYNC.md`: briefs 3 and 4 will build from it, so it must be complete and unambiguous.

### Design (the agreed shape; you settle the details)

- **Roles.** The hub (PC) holds the record of what was last synced with each paired device. The client (Miyoo) starts every exchange: hello, plan, transfers, commit.
- **Items are logical keys, independent of paths.** Use something like `rom:<sys>/<relpath>`, `label:<sys>/<stem>`, `art:<sys>/<stem>`, `shell:<stem>`, `fav:<sys>/<stem>` and `save:<sys>/<stem>.<srm|rtc>`.
  - A `Layout` per side maps keys to files and back.
  - The device layout uses slot-mini's existing rules for which core a game runs on: the `.game_config` override, else the launch script's default, on both Onion 4.3.1-1 and the development build. If those rules live in the `slot-mini` binary crate, move their pure parts into `slot-mini-store` without changing behaviour.
  - The PC layout takes the core choice per game as an input from the PC app. Use a trait or a callback; brief 2 is writing that code in parallel.
  - Favourites live in `App/slot-mini/data/favorites.txt` on the device and `App/slot-mini-pc/data/favorites.txt` on the PC, in the same format.
- **Manifests.**
  - Each item carries its SHA-256 hash (via `ring`) and its size.
  - Saves also carry a "last played" hint. It is used only to pick a conflict's winner, never to detect a change.
  - Each side caches hashes keyed by path, size and mtime so that ROMs aren't re-hashed on every sync. A first full hash of a large library on the Miyoo will be slow, so report progress.
- **Planning is a three-way comparison** of the client's current hash, the hub's current hash and the base (the last synced hash for that device).
  - The result for each item is pull, push, no-op, conflict, or merge (for text items).
  - With no base and equal hashes, just record the base. This makes seeding by copying an SD card idempotent.
  - Tombstones come only from Remove everywhere. They also need to reach devices paired later.
  - Write the full truth table in `docs/SYNC.md`.
- **Scopes.** Every sync names one:
  - `Full`: everything, used for manual syncs.
  - `Auto`: everything except ROMs.
  - `Game(key)`: one game's save and art, used just before a launch.
- **Transfers.**
  - Stream both ways and never buffer a whole file: the Miyoo has 128 MB of RAM, and GBA ROMs reach 32 MB.
  - Write to a temporary file, fsync and rename. Reuse or extend `slot-mini-store`'s `atomic.rs`, which today takes a byte slice.
  - Verify each file's hash before the rename.
  - Commit the base per item once its transfer is verified, so an interrupted sync loses nothing and resumes.
  - Clean up stray temporaries the way `sweep_temps` does.
- **Merges.**
  - `cart_shell.txt` merges per stem as a three-way merge; on a conflict the PC wins.
  - Favourites merge as a set, with three-way adds and removals.
  - Use the existing parsers and writers (`slot-mini-store`'s `cart_shell.rs`, slot-mini's `Favourites`). Move them somewhere shared rather than duplicating them.
- **Save rules.**
  - All-`00` or all-`ff` counts as absent.
  - Receiving a newer save sets aside that side's auto state for the game's core. Provide this as a crate function that both layouts call, matching `cores::carry_save`'s naming. Replace that function's copy so there is one implementation.
  - A conflict keeps the loser as something like `<stem>.srm.conflict-<device>-<stamp>` (RetroArch must not load it) and records the conflict on the hub, so that the PC can list it and swap the two copies.
- **Space and locks.**
  - The client reports its free space. A `Full` plan leaves out ROMs that won't fit and says which ones.
  - The hub can mark a game busy (the game running on the PC). Plans skip it and say so.
- **Names.** Keys use `/`. Compare names case-insensitively wherever Windows or FAT/exFAT would. Report names that one side can't store instead of failing the whole sync. Decide how Unicode normalisation is handled (`slot-mini-store` already depends on `unicode-normalization`) and write the decision down.
- **Protocol.**
  - HTTP/1.1, with JSON for control messages and raw bodies for files. You design the endpoints.
  - Define a `PROTOCOL_VERSION`. On a mismatch, both sides get a clear, user-facing error naming which app to update.
  - The server is small and blocking (for example `tiny_http`), on threads. The client is `ureq`.
  - Measure what the `client` feature adds to the static armv7 device binary and record it.
- **Discovery and poke.**
  - Discovery is UDP: the client broadcasts a request, and hubs reply with their name, id, port and protocol version.
  - The hub can send a poke datagram to a paired client's last address. The client acts on a poke only after verifying it.
  - Pick fixed ports and document them.
- **Pairing and authentication.**
  - Use X25519 key agreement (`ring::agreement`) to get a shared secret. Both screens show the same 6-digit code derived from the exchange, and the user confirms on both.
  - Every request and poke is authenticated with HMAC-SHA256 over the method, path, timestamp, nonce and body hash, with a replay window.
  - Either side can unpair.
  - There is no transport encryption yet, because sync is LAN-only. Say so in `docs/SYNC.md` as a point to revisit with the off-LAN strategy.
- **What each UI must show.** End `docs/SYNC.md` with what briefs 3 and 4 must surface: the progress events, the results summary (added, updated, conflicts, didn't fit, skipped while busy, names that couldn't be stored) and the errors. Expose them as data the apps can render.

### Plan

1. **Design.** Read the code first: `slot-mini-store`, `cores.rs` (`carry_save`, `set_aside`), `settings.rs` (`Favourites`), `cart_shell.rs`, `atomic.rs`, `update.rs` (ureq and TLS setup), `paths.rs`, `library.rs` and `fixture.rs`. Then write `docs/SYNC.md`: the item model, both layouts, the planner truth table, the merge rules, the save rules, the protocol, pairing and authentication, discovery and poke, failure and resume behaviour, and what the UIs must show.
   - Exit: `docs/SYNC.md` is complete, and the truth table covers every combination of present, absent, changed and equal on each side with and without a base.
2. **Core.** The model, the hash cache, the planner, the merges, the save rules and both layouts, including any moves of shared code out of `slot-mini`.
   - Exit: unit tests pass, plus property tests that hold for any history of edits on both sides:
     - a sync converges, so both sides hold the same synced kinds;
     - a second sync is a no-op;
     - no non-empty save is ever lost: every save that existed is still the save, a conflict copy, a `.bak_` or a backup;
     - an auto state is set aside whenever a newer save lands.
3. **Transport.** The server, the client, pairing, authentication, discovery and poke.
   - Exit: localhost tests cover bad HMAC, replay, an expired timestamp, a protocol mismatch, an interrupted transfer that resumes, a hash mismatch that is rejected, unpairing, and a poke with a bad signature that is ignored.
4. **Harness.** In `tests/`, build two fake trees: a Miyoo SD card from slot-mini's fake-SD fixture, and a PC library. Run the server and client in-process, with scenario tests for:
   - seeding a full Miyoo into an empty PC library;
   - a game added on the PC arriving after a `Full` sync, but not after an `Auto` one;
   - a save made on the PC being pulled with the Miyoo's auto state set aside, with the game on gpSP on the Miyoo and mGBA on the PC;
   - both sides playing the same game, producing a conflict copy;
   - Remove everywhere, including a device paired afterwards;
   - an SD card too full for everything;
   - a game running on the PC (busy);
   - an interruption halfway through a ROM;
   - shells edited on both sides;
   - a favourite added on one side and removed on the other;
   - a `.GBA` extension and a subfolder ROM;
   - a name that can't be stored.

   Run the client-side tests on armv7 under qemu in CI as well.
   - Exit: CI is green on the desktop and armv7 jobs.
5. **Docs.** Finish `docs/SYNC.md` and `docs/sync/PROGRESS.md`, add the `docs/DECISIONS.md` entries, and add a row for `docs/SYNC.md` to the README's documentation table. Merge to `main` when CI passes. The device binary doesn't change, so no version bump is needed unless shared code moved and the device build changed. If it did, bump the workspace version per `CLAUDE.md`.

### Done means

- [ ] `crates/slot-mini-sync` with `server` and `client` features, building for desktop Linux, Windows and static armv7 musl
- [ ] `docs/SYNC.md` complete enough for briefs 3 and 4 to implement against without guessing
- [ ] Property tests and scenario tests pass in CI, including the client side on armv7
- [ ] One implementation each of setting saves aside, favourites and shell parsing, shared by both sides
- [ ] Device behaviour, golden images and budgets unchanged; the binary-size delta of the `client` feature recorded

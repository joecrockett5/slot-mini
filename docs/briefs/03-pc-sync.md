# slot-mini companion, brief 3 of 4: sync in the PC app

You are working in the `joecrockett5/slot-mini` repository. This file is your whole brief (its file name is `03-pc-sync.md`). The shared sections come first, then your own.

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

## Your brief: sync in the PC app

**Goal:** make slot-mini PC a sync hub. Embed `slot-mini-sync`'s server with the PC layout, and build the PC-side sync experience on the seams brief 2 left. Briefs 1 and 2 are merged. Read `docs/SYNC.md`, `docs/pc/PROGRESS.md` and both briefs in `docs/briefs/` before starting.

Brief 4 is building the Miyoo side at the same time. Run the sync harness end to end with the real device client often, and treat `docs/SYNC.md` as the contract. If the contract needs to change, change the document and the crate together in a PR of their own, so that brief 4 can rebase onto it.

Your progress file is `docs/pc/SYNC-PROGRESS.md`.

### Requirements

- **The hub.**
  - Start the server and the discovery responder with the app. Stop them cleanly on exit.
  - Store pairing secrets, per-device bases and conflicts under `App/slot-mini-pc/data/sync/` in the library, so that a moved library keeps its sync state.
  - Windows Firewall will prompt the first time the app listens. Explain this on the Sync page and in the README, including what to do if the user clicked "Cancel".
- **The Sync page (Settings).**
  - This PC's name, editable.
  - Pairing: when a Miyoo asks to pair, show its name and the 6-digit code in slot's style, with Confirm and Deny.
  - Paired devices: name, last sync, last address, and Unpair.
  - **Sync now**: poke the paired Miyoo, and say clearly when it doesn't answer (Miyoo asleep, Wi-Fi off or not on its shelf).
  - The conflict list.
- **Status.** A sync status icon in slot's style in the HUD slot brief 2 left: idle, syncing (with progress), conflict needing attention, or last sync failed. While a sync driven by the Miyoo is running, show progress on the PC too (item, count, bytes).
- **After a game.** Use the post-game hook to tell paired Miyoos about the new save (a poke), so that a Miyoo on its shelf picks it up straight away.
- **While a game runs**, mark it busy so that no sync touches its files.
- **Conflicts.**
  - List each conflict: game, both copies, which device each came from, and when each was last played.
  - Swap them (the current save becomes the conflict copy and the other way round, keeping the auto-state rule).
  - Dismiss a conflict, deleting the conflict copy only after an explicit confirmation.
  - Badge the cart in the shelf or grid while a conflict is open.
- **Remove everywhere.** An action on a cart (in the studio or a cart menu, plus right-click for the mouse), with a confirmation in slot's style that says exactly what goes and where the saves are backed up. It creates the tombstones and removes the game locally straight away.
- **First run.** Replace brief 2's placeholder with "Pair your Miyoo to bring its library over". After pairing, prompt the user to run Sync now on the Miyoo, and show the seeding progress. Import from an SD card stays offered as the fast alternative.
- **Results.** After each sync, show a short summary in slot's style. Refresh the shelves for new games and labels without restarting.

### Plan

1. **Hub wiring.** The server, discovery, storage and the busy marking.
   - Exit: a harness test with the real client shows a seeded library and a busy game left alone.
2. **The Sync page, pairing and status.**
   - Exit: golden images for each new screen, and a pairing test through the UI's state machine.
3. **The post-game poke, conflicts and Remove everywhere.**
   - Exit: scenario tests for each, including swapping a conflict and the auto-state rule.
4. **First run and results.**
   - Exit: an end-to-end test from an empty PC library to a seeded one.
5. **Docs and release.** Extend the README and `docs/pc/TESTING.md` with a sync checklist on real hardware: firewall prompt, pairing, Sync now from each side, play on PC then on Miyoo, a deliberate conflict, Remove everywhere. Bump the PC version and merge per `CLAUDE.md`.

### Done means

- [ ] Pairing, Sync now, status, conflicts and Remove everywhere work on the PC against the real device client in the harness
- [ ] A new save on the PC reaches a Miyoo on its shelf without the user doing anything
- [ ] Windows and Linux CI green; a `pc-v*` release shipped
- [ ] Device behaviour unchanged by this brief

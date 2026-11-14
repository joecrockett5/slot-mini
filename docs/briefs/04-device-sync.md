# slot-mini companion, brief 4 of 4: sync and grid view on the Miyoo

You are working in the `joecrockett5/slot-mini` repository. This file is your whole brief (its file name is `04-device-sync.md`). The shared sections come first, then your own.

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

## Your brief: sync and grid view on the Miyoo

**Goal:** make slot-mini a sync client using `slot-mini-sync`'s client with the device layout, and bring brief 2's grid view to the device. Briefs 1 and 2 are merged. Read `docs/SYNC.md`, `docs/pc/PROGRESS.md` (for the grid view and its toggle button) and both briefs in `docs/briefs/` before starting.

Brief 3 is building the PC side at the same time. Treat `docs/SYNC.md` as the contract. If it needs to change, change the document and the crate together in a PR of their own.

Your progress file is `docs/sync/DEVICE-PROGRESS.md`.

**This brief changes what the Miyoo runs, and merging publishes it over the air.** Sync must stay inert until the user pairs, and nothing in this brief may delay a launch or risk a save.

### Requirements

- **The Sync page** in MENU → Settings.
  - **Pair with a PC**: discover hubs on the network and list them. Choose one, show the 6-digit code and confirm with A. B cancels.
  - The paired PC's name, Unpair, and **Auto sync: On/Off** (on by default once paired).
  - Everything is drawn in slot's style, reusing the Settings page's existing parts.
- **MENU → Sync now** runs a `Full` sync, ROMs included, behind a progress screen in slot's style.
  - Show what is happening: hashing (the first full hash of a big card takes a while, so show it), then transferring, with the item, count and bytes.
  - B cancels safely.
  - The results summary shows added, updated, conflicts (resolved on the PC), what didn't fit, and names that couldn't be stored.
  - If Wi-Fi is off or the PC can't be reached, say so plainly.
- **Status.** An icon beside the Wi-Fi icon (`wifi.rs`) shows synced, syncing, PC unreachable or conflict. There is no icon while unpaired.
- **Automatic syncs** (`Auto` scope), only while paired, with Auto sync on and Wi-Fi up. Each fails silently and quickly when the PC isn't there.
  - **When slot-mini starts**, after the first frame. It must not touch the 1-second first-frame budget.
  - **On returning to the shelf after a game** (boot-launcher mode).
  - **When a verified poke arrives** while on the shelf. Use a listener thread blocked on `recv`, with no timers and no polling. Re-check idle power the way 0.4.2 and 0.4.6 did, and record it.
  - **Just before a launch**, a `Game` scope pull of that game's save and art, run alongside the insert animation and capped by a short total timeout (about 1–2 s; tune it and record it).
    - A launch never waits beyond that.
    - A launch never starts over a half-written save.
    - If a newer save lands, the auto state is set aside before RetroArch starts.
    - Check Tap A and Hold A on Onion 4.3.1-1 and the development build, gpSP and mGBA alike.
  - **In app mode**, games return to Onion's menu, not slot-mini. The in-game helper that runs beside each game slot-mini launches pushes that game's save after RetroArch exits, before it finishes. Confirm against Onion's source how long the runtime lets it run, and keep within that.
  - Games started from Onion's own menu are caught at slot-mini's next start. Document that this is the one gap.
- **After any sync**, rescan the library so new ROMs appear, invalidate the label cache for changed labels and art, and reload favourites and shells, all without a restart and without dropping a frame on the shelf.
- **Space.** Never fill the card. Respect the plan's didn't-fit list, and leave a safety margin you choose and record.
- **Budgets.** Measure and record the first frame, idle power, resident memory during a ROM transfer (streams only; stay under 40 MB) and the binary-size delta.
- **Grid view on the Miyoo.**
  - Adopt brief 2's grid from `slot-mini-ui`, with the per-shelf view mode stored in slot-mini's settings. The default is the scrolling shelf for every shelf.
  - Use the toggle button brief 2 chose, plus a row on the Settings page for the current shelf.
  - The footer legend shows the toggle.
  - Add device golden images for the grid. The existing carousel golden images stay byte-identical.
  - Measure grid scrolling and shelf switching against the device budgets with `examples/perf.rs` and under qemu-armv7. If the grid can't hold 60 fps on the device, simplify the device's drawing of it (keeping the silhouette, colours and motion, as the original brief says) and record what you dropped.

### Plan

1. **Client wiring.** The device layout, settings, storage under `App/slot-mini/data/sync/`, and the shared functions in place of local copies.
   - Exit: harness tests pass with the real hub from brief 1, under qemu-armv7 too.
2. **Pairing, the Sync page, Sync now and status.**
   - Exit: golden images for each new screen, and a test through the UI's state machine for pairing and a full sync, including cancelling.
3. **Automatic syncs.** Start, return, poke, pre-launch and the app-mode helper.
   - Exit: integration tests with the fake runtime loop (`tests/fake-runtime.sh`):
     - a pre-launch pull of a newer save sets the auto state aside, then launches;
     - an unreachable PC launches within the cap;
     - the helper pushes after the game exits;
     - a poke triggers a sync.
4. **Grid view.**
   - Exit: golden images and perf numbers, with the carousel golden images unchanged.
5. **Budgets and docs.** Record the measurements. Add a sync section to `docs/DEVICE-TESTING.md` ordered safest first: pair; Sync now on a card with a few games; Auto sync; play on the PC then the Miyoo and the other way round; a deliberate conflict; app mode versus boot-launcher mode; Wi-Fi off. Then the grid view. Update the README's controls, MENU and files tables. Bump the workspace version and merge per `CLAUDE.md`.

### Done means

- [ ] The Miyoo pairs, syncs on demand and syncs saves automatically at every trigger above, against the real hub
- [ ] No launch is delayed past the cap or started over a half-written save, and every pulled save sets the stale auto state aside
- [ ] Unpaired, slot-mini behaves exactly as before
- [ ] Grid view on the device, per shelf, defaulting to the scrolling shelf, within budget
- [ ] Budgets measured and recorded; `docs/DEVICE-TESTING.md` and the README updated; a device release shipped

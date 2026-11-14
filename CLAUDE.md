# slot-mini

A slot-style game launcher for the Miyoo Mini Plus on Onion OS.

- The full brief is in `PROMPT.md`. Read it before doing anything.
- If `docs/PROGRESS.md` exists, read it next and carry on from where it says. Otherwise start at phase 1.
- slot (https://github.com/BrandonKowalski/slot) is the original project. It is no longer vendored: clone it outside the repo when you need to read it (`scripts/copy-from-slot.sh` clones it under `target/slot`), and treat it as read-only. Never build or depend on it. The work this repo copied from it was pinned at slot `e66715d`.
- Keep `docs/PROGRESS.md` current at every phase exit and before you stop.
- Shipping changes: bump `version` in the workspace `Cargo.toml`, push, open a PR into `main`, and merge it once its CI passes. That is the maintainer's standing instruction; hold off only when they say so for a given change. Merging to `main` publishes the release, which reaches devices through MENU -> Check for updates.
- The companion work (slot-mini PC and sync) is split into briefs under `docs/briefs/`. Brief 1, the sync core (`crates/slot-mini-sync`), is `docs/briefs/01-sync-core.md`; its progress file is `docs/sync/PROGRESS.md`.
- There are two release lines. The Miyoo's is `v<version>`, from the workspace `Cargo.toml` (`scripts/release.sh`); it is GitHub's latest release, and the device updater takes only tags that are exactly `v<x.y.z>`. The PC app's is `pc-v<version>`, from `crates/slot-mini-pc/Cargo.toml` (`scripts/release-pc.sh`), always published with `--latest=false`.
- A change to the PC app alone bumps only `crates/slot-mini-pc`'s version; a change the Miyoo runs (anything in its binary, `crates/slot-mini*` other than the PC crate and `pc`-gated code) bumps the workspace version. Bump both when a change touches both.
- The PC app (`crates/slot-mini-pc`) has its own brief in `docs/briefs/02-pc-app.md` and its own progress file, `docs/pc/PROGRESS.md`. Read both before working on it.
- Brief 3, sync in the PC app, is `docs/briefs/03-pc-sync.md`; its progress file is `docs/pc/SYNC-PROGRESS.md`.

# slot-mini

A slot-style game launcher for the Miyoo Mini Plus on Onion OS.

- The full brief is in `PROMPT.md`. Read it before doing anything.
- If `docs/PROGRESS.md` exists, read it next and carry on from where it says. Otherwise start at phase 1.
- `reference/slot` is a read-only git submodule of the original slot project. Run `git submodule update --init` if it is empty. Never edit, build or depend on it.
- Keep `docs/PROGRESS.md` current at every phase exit and before you stop.
- Shipping changes: bump `version` in the workspace `Cargo.toml`, push, open a PR into `main`, and merge it once its CI passes. That is the maintainer's standing instruction; hold off only when they say so for a given change. Merging to `main` publishes the release, which reaches devices through MENU -> Check for updates.

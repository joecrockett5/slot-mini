// Copied from slot (https://github.com/BrandonKowalski/slot) at e66715d:
//   crates/slot-store/tests/common/mod.rs
// Changes: Games/ -> Roms/
// Copyright (C) Brandon Kowalski and slot contributors. GPL-3.0-or-later.

use tempfile::TempDir;

pub fn tmp_root() -> TempDir {
    let d = tempfile::tempdir().expect("tempdir");
    for sub in [
        "Roms", "Roms/GBA", "Roms/GB", "Roms/GBC", "Labels", "Saves", "States", "System", "Config",
    ] {
        std::fs::create_dir_all(d.path().join(sub)).expect("create content dir");
    }
    d
}

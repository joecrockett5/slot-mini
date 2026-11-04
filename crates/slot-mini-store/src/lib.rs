// Adapted from slot (https://github.com/BrandonKowalski/slot) at e66715d:
//   crates/slot-store/src/lib.rs
// Changes: only the modules slot-mini uses; slot-mini's own modules added.
// Copyright (C) Brandon Kowalski and slot contributors. GPL-3.0-or-later.

macro_rules! eprintln {
    ($($arg:tt)*) => {{
        use std::io::Write as _;
        let _ = writeln!(std::io::stderr(), $($arg)*);
    }};
}

mod atomic;
pub mod cart_shell;
pub mod gb;
mod gba;
pub mod header_index;
pub mod ini;
mod platform;
mod scan;

pub use atomic::{atomic_write, sweep_temps};
pub use cart_shell::{Outline, ShellChoice, ShellFinish, CART_SHELL_FILE, LABELS_SHELL_FILE};
pub use gba::{header_clean, header_code, header_title, header_title_code};
pub use platform::Platform;
pub use scan::{initial, is_hidden, listing, scan, scan_platform, sort_key, Cart, StoreError};

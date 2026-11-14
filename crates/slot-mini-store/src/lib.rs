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
mod core;
mod favourites;
pub mod gb;
mod gba;
pub mod header_index;
pub mod ini;
mod platform;
pub mod saves;
mod scan;

/// The app's folder under the card or library root, `App/slot-mini` unless
/// the host says otherwise (the PC app keeps its own beside it, so a library
/// that is also a card's mirror never mixes the two).
pub fn app_dir() -> &'static str {
    APP_DIR.get().copied().unwrap_or("App/slot-mini")
}

/// Name the app's folder for this process. Only the first call counts, and
/// it has to come before anything reads the folder.
pub fn set_app_dir(dir: &'static str) {
    let _ = APP_DIR.set(dir);
}

static APP_DIR: std::sync::OnceLock<&'static str> = std::sync::OnceLock::new();

pub use atomic::{atomic_write, sweep_temps, AtomicFile};
pub use cart_shell::{Outline, ShellChoice, ShellFinish, CART_SHELL_FILE, LABELS_SHELL_FILE};
pub use core::Core;
pub use favourites::{list_entry, list_line, Favourites};
pub use gba::{header_clean, header_code, header_title, header_title_code};
pub use platform::Platform;
pub use scan::{
    initial, is_hidden, listing, rom_files, scan, scan_platform, sort_key, Cart, StoreError,
};

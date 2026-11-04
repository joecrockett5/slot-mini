//! slot-mini: slot's cartridge shelf as a launcher for Onion OS.

macro_rules! eprintln {
    ($($arg:tt)*) => {{
        use std::io::Write as _;
        let _ = writeln!(std::io::stderr(), $($arg)*);
    }};
}

pub mod app;
pub mod faces;
pub mod fixture;
pub mod label_cache;
pub mod library;
pub mod paths;
pub mod settings;
pub mod status;
pub mod theme;
pub mod wallpaper;

pub use app::{App, Exit, Options, Phase};
pub use paths::Paths;

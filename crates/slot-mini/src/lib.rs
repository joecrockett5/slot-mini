//! slot-mini: slot's cartridge shelf as a launcher for Onion OS.

macro_rules! eprintln {
    ($($arg:tt)*) => {{
        use std::io::Write as _;
        let _ = writeln!(std::io::stderr(), $($arg)*);
    }};
}

pub mod about;
pub mod app;
pub mod art;
pub mod core_picker;
pub mod core_scene;
pub mod cores;
pub mod faces;
pub mod fixture;
pub mod ingame;
pub mod label_cache;
pub mod launch;
pub mod library;
pub mod paths;
pub mod settings;
pub mod sound;
pub mod status;
pub mod studio;
pub mod studio_ui;
pub mod studio_view;
pub mod switch;
pub mod theme;
pub mod update;
pub mod wallpaper;

pub use app::{App, Exit, Options, Phase};
pub use paths::Paths;

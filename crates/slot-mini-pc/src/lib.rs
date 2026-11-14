//! slot-mini for the PC: slot's cartridge shelf in front of a standalone
//! RetroArch, on a library folder laid out like an Onion SD card.
//!
//! Everything here runs without a window, so tests drive it: `main.rs`
//! only owns the window, the controllers and the native dialogs.

pub mod audio;
pub mod config;
pub mod desktop;
pub mod import;
pub mod input;
pub mod net;
pub mod pcapp;
pub mod present;
pub mod retroarch;
pub mod sync;

pub use config::Config;
pub use pcapp::{PcApp, Request};

/// The app's folder inside a library (the Miyoo's is `App/slot-mini`).
pub const APP_DIR: &str = "App/slot-mini-pc";

/// This build's version: the PC app's own release line.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

// Copied from slot (https://github.com/BrandonKowalski/slot) at e66715d:
//   crates/slot-store/src/core.rs
// Changes: slot's own Config/selected_core.txt store is not used (slot-mini
//   keeps the choice in Onion's per-game core file, see slot-mini's
//   cores.rs); `libretro` and `from_libretro` added for Onion's core names.
// Copyright (C) Brandon Kowalski and slot contributors. GPL-3.0-or-later.

use crate::Platform;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Core {
    #[default]
    Mgba,
    Gpsp,
    Gambatte,
}

impl Core {
    pub const ALL: [Core; 3] = [Core::Mgba, Core::Gpsp, Core::Gambatte];

    pub fn as_str(&self) -> &'static str {
        match self {
            Core::Mgba => "mgba",
            Core::Gpsp => "gpsp",
            Core::Gambatte => "gambatte",
        }
    }

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn text(self) -> &'static str {
        match self {
            Core::Mgba => "mGBA",
            Core::Gpsp => "gpSP",
            Core::Gambatte => "Gambatte",
        }
    }

    /// The core's file name in Onion's RetroArch (`cores/<name>.so`), as
    /// Onion's per-game `core = "..."` setting names it.
    pub fn libretro(self) -> &'static str {
        match self {
            Core::Mgba => "mgba_libretro",
            Core::Gpsp => "gpsp_libretro",
            Core::Gambatte => "gambatte_libretro",
        }
    }

    /// The core a libretro file name is, if it is one of these.
    pub fn from_libretro(name: &str) -> Option<Core> {
        Core::ALL.into_iter().find(|c| c.libretro() == name.trim())
    }

    pub fn parse(s: &str) -> Option<Core> {
        match s.trim().to_ascii_lowercase().as_str() {
            "mgba" => Some(Core::Mgba),
            "gpsp" => Some(Core::Gpsp),
            "gambatte" => Some(Core::Gambatte),
            _ => None,
        }
    }

    pub fn runs(self, platform: Platform) -> bool {
        match self {
            Core::Mgba => true,
            Core::Gpsp => platform.hardware() == Platform::Gba,
            Core::Gambatte => matches!(platform, Platform::Gb | Platform::Gbc),
        }
    }

    pub fn pair(platform: Platform) -> [Core; 2] {
        match platform {
            Platform::Gba | Platform::Gbav => [Core::Mgba, Core::Gpsp],
            Platform::Gb | Platform::Gbc => [Core::Mgba, Core::Gambatte],
        }
    }
}

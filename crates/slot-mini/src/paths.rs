//! Where things live on an Onion SD card.

use std::path::{Path, PathBuf};

use slot_mini_store::Platform;

/// The SD card and the folder Onion's runtime uses for hand-offs.
#[derive(Clone, Debug)]
pub struct Paths {
    /// `/mnt/SDCARD` on the device; a fake tree in tests.
    pub sd: PathBuf,
    /// `/tmp` on the device: `cmd_to_run.sh`, `reset_game`, `percBat`.
    pub tmp: PathBuf,
}

/// slot-mini's app folder, relative to the SD root.
pub const APP_DIR: &str = "App/slot-mini";

impl Paths {
    pub fn device() -> Paths {
        Paths {
            sd: PathBuf::from("/mnt/SDCARD"),
            tmp: PathBuf::from("/tmp"),
        }
    }

    pub fn new(sd: impl Into<PathBuf>, tmp: impl Into<PathBuf>) -> Paths {
        Paths {
            sd: sd.into(),
            tmp: tmp.into(),
        }
    }

    pub fn app(&self) -> PathBuf {
        self.sd.join(APP_DIR)
    }

    /// Settings, favourites and the last cart.
    pub fn data(&self) -> PathBuf {
        self.app().join("data")
    }

    pub fn cache(&self) -> PathBuf {
        self.app().join("cache")
    }

    /// Onion's system folder.
    pub fn sysdir(&self) -> PathBuf {
        self.sd.join(".tmp_update")
    }

    /// `/mnt/SDCARD/Emu/<sys>` as the device sees it (used inside commands
    /// and Recents records, which must hold device paths).
    pub fn device_emu_dir(platform: Platform) -> String {
        format!("/mnt/SDCARD/Emu/{}", platform.dir_name())
    }

    /// The emulator's launch script on this card.
    pub fn launch_script(&self, platform: Platform) -> PathBuf {
        self.sd
            .join("Emu")
            .join(platform.dir_name())
            .join("launch.sh")
    }

    /// Whether Onion has an emulator installed for this system.
    pub fn has_emulator(&self, platform: Platform) -> bool {
        self.launch_script(platform).is_file()
    }

    /// The path of a file on this card as the device names it.
    pub fn on_device(&self, p: &Path) -> Option<String> {
        let rel = p.strip_prefix(&self.sd).ok()?;
        let rel = rel.to_str()?;
        Some(match rel.is_empty() {
            true => "/mnt/SDCARD".to_string(),
            false => format!("/mnt/SDCARD/{rel}"),
        })
    }
}

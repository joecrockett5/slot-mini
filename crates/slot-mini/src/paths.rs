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
    /// Set by the PC app: `sd` is then its library folder, laid out like a
    /// card, and games run in a standalone RetroArch.
    #[cfg(feature = "pc")]
    pub pc: Option<Pc>,
}

/// What the PC app knows that a card does not say.
#[cfg(feature = "pc")]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pc {
    /// RetroArch's cores folder, once RetroArch has been found.
    pub cores: Option<PathBuf>,
}

/// slot-mini's app folder on a card, relative to the SD root. The PC app
/// names its own with `slot_mini_store::set_app_dir`.
pub const APP_DIR: &str = "App/slot-mini";

impl Paths {
    pub fn device() -> Paths {
        Paths {
            sd: PathBuf::from("/mnt/SDCARD"),
            tmp: PathBuf::from("/tmp"),
            #[cfg(feature = "pc")]
            pc: None,
        }
    }

    pub fn new(sd: impl Into<PathBuf>, tmp: impl Into<PathBuf>) -> Paths {
        Paths {
            sd: sd.into(),
            tmp: tmp.into(),
            #[cfg(feature = "pc")]
            pc: None,
        }
    }

    /// A PC library: `sd` is the library folder.
    #[cfg(feature = "pc")]
    pub fn pc(library: impl Into<PathBuf>, tmp: impl Into<PathBuf>, pc: Pc) -> Paths {
        Paths {
            sd: library.into(),
            tmp: tmp.into(),
            pc: Some(pc),
        }
    }

    /// Whether this is the PC app's library rather than an Onion card. The
    /// device build has no PC side, so this folds to `false` there.
    #[inline]
    pub fn is_pc(&self) -> bool {
        #[cfg(feature = "pc")]
        {
            self.pc.is_some()
        }
        #[cfg(not(feature = "pc"))]
        {
            false
        }
    }

    /// The PC's RetroArch cores folder, if it is known.
    #[inline]
    pub fn pc_cores(&self) -> Option<&Path> {
        #[cfg(feature = "pc")]
        {
            self.pc.as_ref()?.cores.as_deref()
        }
        #[cfg(not(feature = "pc"))]
        {
            None
        }
    }

    pub fn app(&self) -> PathBuf {
        self.sd.join(slot_mini_store::app_dir())
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

    /// Whether Onion has an emulator installed for this system. The PC shows
    /// every system; a shelf whose core is missing says so instead.
    pub fn has_emulator(&self, platform: Platform) -> bool {
        self.is_pc() || self.launch_script(platform).is_file()
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

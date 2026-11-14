//! Which RetroArch core a game runs on, kept where Onion keeps it so that
//! Onion's own Game List Options ("Game core") and slot-mini always agree.
//!
//! Checked against Onion 4.3.1-1 (docs/PLATFORM.md, "Cores"):
//!
//! - A per-game choice is `<rom folder>/.game_config/<rom name>.cfg` holding
//!   `core = "gpsp_libretro"`. Onion's runtime reads it for every game
//!   started through the system's `launch.sh` (`override_game_core`) and runs
//!   RetroArch with that core instead.
//! - Without one, the system's `launch.sh` decides. On 4.3.1-1 the GBA
//!   package ("Nintendo - GBA (mGBA)") runs mGBA. Game List Options reads the
//!   default the same way as [`default_core`].
//! - Picking the default again deletes the file, as "Restore default core"
//!   does, so nothing is left behind for games on their default core.
//! - RetroArch keeps saves and states in a folder per core
//!   (`sort_savefiles_enable`). In-game saves are the same format in mGBA and
//!   gpSP, so [`carry_save`] copies the game's save to the new core when it
//!   changes, as Onion's own `transfer_mgba_save.sh` does. States are not
//!   compatible and stay where they are.

use std::path::PathBuf;

use slot_mini_store::saves::{self, core_line, Incoming};
pub use slot_mini_store::saves::{onion_stem, sort_dir};
use slot_mini_store::{atomic_write, Cart, Core, Platform};

use crate::paths::Paths;

/// `<rom folder>/.game_config/<rom name>.cfg`.
pub fn cfg_path(cart: &Cart) -> Option<PathBuf> {
    saves::game_config_path(&cart.rom)
}

/// The per-game core setting, as Onion's file names it (`gpsp_libretro`).
pub fn per_game(cart: &Cart) -> Option<String> {
    saves::per_game_core(&cart.rom)
}

/// The core the system's `launch.sh` runs (`saves::default_core`).
pub fn default_core(paths: &Paths, platform: Platform) -> Option<String> {
    if paths.is_pc() {
        return Some(pc_default(platform).libretro().to_string());
    }
    saves::default_core(&paths.sd, platform)
}

/// The core the PC runs a system on unless a game says otherwise: mGBA for
/// GBA and Gambatte for GB and GBC, Onion 4.3.1-1's defaults, so a save
/// lands in the folder the Miyoo reads it from.
pub fn pc_default(platform: Platform) -> Core {
    match platform.hardware() {
        Platform::Gba => Core::Mgba,
        _ => Core::Gambatte,
    }
}

/// Whether Onion's RetroArch has this core (on the PC, whether the
/// standalone RetroArch's cores folder does).
pub fn installed(paths: &Paths, core: &str) -> bool {
    if paths.is_pc() {
        return paths.pc_cores().is_some_and(|dir| {
            dir.join(format!("{core}{}", std::env::consts::DLL_SUFFIX))
                .is_file()
        });
    }
    paths
        .sd
        .join("RetroArch/.retroarch/cores")
        .join(format!("{core}.so"))
        .is_file()
}

/// The core this cart runs on now, as one of slot's cores: its own setting,
/// else the system's default, else mGBA (`saves::device_core`).
pub fn chosen(paths: &Paths, cart: &Cart) -> Core {
    if paths.is_pc() {
        // The PC has no launch scripts to read a default from.
        return per_game(cart)
            .and_then(|c| Core::from_libretro(&c))
            .filter(|c| c.runs(cart.platform))
            .unwrap_or(pc_default(cart.platform));
    }
    saves::device_core(&paths.sd, cart.platform, Some(&cart.rom))
}

/// Run this cart on `core` from now on. The system's default removes the
/// per-game file (and its folder, if that leaves it empty), as Onion's
/// "Restore default core" does; any other core is written the way Onion's
/// "Game core" writes it, keeping the file's other settings.
pub fn choose(paths: &Paths, cart: &Cart, core: Core) -> std::io::Result<()> {
    let Some(path) = cfg_path(cart) else {
        return Ok(());
    };
    let old = std::fs::read_to_string(&path).unwrap_or_default();
    let mut lines: Vec<String> = old
        .lines()
        .filter(|l| core_line(l).is_none() && !l.trim().is_empty())
        .map(str::to_string)
        .collect();
    if default_core(paths, cart.platform).as_deref() != Some(core.libretro()) {
        lines.push(format!("core = \"{}\"", core.libretro()));
    }
    if lines.is_empty() {
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        if let Some(dir) = path.parent() {
            // Only goes if empty, like Onion's `rm -d`.
            let _ = std::fs::remove_dir(dir);
        }
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    atomic_write(&path, format!("{}\n", lines.join("\n")).as_bytes())
}

/// The name RetroArch gives a game's save and states: the rom's file name
/// without its extension, as Onion's launch scripts cut it (`${romname%.*}`).
pub fn save_name(cart: &Cart) -> Option<String> {
    Some(cart.rom.file_stem()?.to_str()?.to_string())
}

fn profile(paths: &Paths) -> PathBuf {
    paths.sd.join("Saves/CurrentProfile")
}

pub fn save_path(paths: &Paths, cart: &Cart, core: Core) -> Option<PathBuf> {
    Some(
        profile(paths)
            .join("saves")
            .join(sort_dir(core))
            .join(format!("{}.srm", save_name(cart)?)),
    )
}

pub fn auto_state_path(paths: &Paths, cart: &Cart, core: Core) -> Option<PathBuf> {
    Some(
        profile(paths)
            .join("states")
            .join(sort_dir(core))
            .join(format!("{}.state.auto", save_name(cart)?)),
    )
}

/// What [`carry_save`] did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Carried {
    /// The save was copied to the new core.
    Copied,
    /// The new core already had the same save.
    Same,
    /// The old core had no save (or an empty one): nothing to carry.
    NoSave,
}

/// Carry the game's in-game save from the core it ran on to the one it will
/// run on, the way Onion's "Transfer mGBA save" does:
///
/// - the old core's `<rom>.srm` is copied to the new core's folder;
/// - a different, non-empty save already there is kept as
///   `<rom>.srm.bak_<time>` first;
/// - the new core's auto state is set aside the same way, so the game starts
///   from the save just carried rather than resuming an older session (whose
///   next in-game save would overwrite the newer progress).
///
/// Nothing is deleted, and nothing changes when the old core has no save.
/// The landing itself is `slot_mini_store::saves::receive_save`, which sync
/// uses too.
pub fn carry_save(paths: &Paths, cart: &Cart, from: Core, to: Core) -> std::io::Result<Carried> {
    if from == to {
        return Ok(Carried::Same);
    }
    let (Some(src), Some(dst), Some(state)) = (
        save_path(paths, cart, from),
        save_path(paths, cart, to),
        auto_state_path(paths, cart, to),
    ) else {
        return Ok(Carried::NoSave);
    };
    let save = match std::fs::read(&src) {
        Ok(b) if !saves::is_empty_save(&b) => b,
        Ok(_) => return Ok(Carried::NoSave),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Carried::NoSave),
        Err(e) => return Err(e),
    };
    match std::fs::read(&dst) {
        Ok(old) if old == save => return Ok(Carried::Same),
        _ => {}
    }
    saves::receive_save(&dst, Some(&state), Incoming::Bytes(&save), &saves::stamp())?;
    Ok(Carried::Copied)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn card() -> (tempfile::TempDir, Paths, Cart) {
        let d = tempfile::tempdir().unwrap();
        let paths = Paths::new(d.path().join("sd"), d.path().join("tmp"));
        let emu = paths.sd.join("Emu/GBA");
        std::fs::create_dir_all(&emu).unwrap();
        // Onion 4.3.1-1's GBA launch script, cut down.
        std::fs::write(
            emu.join("launch.sh"),
            "#!/bin/sh\ncd /mnt/SDCARD/RetroArch/\nHOME=/mnt/SDCARD/RetroArch/ ./retroarch -v -L .retroarch/cores/mgba_libretro.so \"$1\"\n",
        )
        .unwrap();
        let cores = paths.sd.join("RetroArch/.retroarch/cores");
        std::fs::create_dir_all(&cores).unwrap();
        for c in ["mgba_libretro", "gpsp_libretro"] {
            std::fs::write(cores.join(format!("{c}.so")), "").unwrap();
        }
        let roms = paths.sd.join("Roms/GBA");
        std::fs::create_dir_all(&roms).unwrap();
        let rom = roms.join("Golden Sun (USA).gba");
        std::fs::write(&rom, "").unwrap();
        let cart = Cart {
            platform: Platform::Gba,
            stem: "Golden Sun (USA)".into(),
            rom,
            label: None,
            title: String::new(),
            code: String::new(),
            shell: Default::default(),
        };
        (d, paths, cart)
    }

    fn put(p: &Path, bytes: &[u8]) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, bytes).unwrap();
    }

    fn baks(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(dir)
            .map(|r| {
                r.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .filter(|n| n.contains(".bak_"))
                    .collect()
            })
            .unwrap_or_default();
        v.sort();
        v
    }

    #[test]
    fn a_save_is_carried_to_the_new_core_and_its_old_resume_set_aside() {
        let (_d, paths, cart) = card();
        let mgba = save_path(&paths, &cart, Core::Mgba).unwrap();
        let gpsp = save_path(&paths, &cart, Core::Gpsp).unwrap();
        assert!(mgba.ends_with("Saves/CurrentProfile/saves/mGBA/Golden Sun (USA).srm"));
        let state = auto_state_path(&paths, &cart, Core::Gpsp).unwrap();
        assert!(state.ends_with("Saves/CurrentProfile/states/gpSP/Golden Sun (USA).state.auto"));
        put(&mgba, b"new progress");
        put(&gpsp, b"old progress");
        put(&state, b"old session");
        assert_eq!(
            carry_save(&paths, &cart, Core::Mgba, Core::Gpsp).unwrap(),
            Carried::Copied
        );
        assert_eq!(std::fs::read(&gpsp).unwrap(), b"new progress");
        assert_eq!(
            std::fs::read(&mgba).unwrap(),
            b"new progress",
            "the source stays"
        );
        assert!(!state.exists());
        // The old save and state are kept, not deleted.
        let saves = baks(gpsp.parent().unwrap());
        assert_eq!(saves.len(), 1);
        assert!(saves[0].starts_with("Golden Sun (USA).srm.bak_"));
        let old = gpsp.parent().unwrap().join(&saves[0]);
        assert_eq!(std::fs::read(old).unwrap(), b"old progress");
        assert_eq!(baks(state.parent().unwrap()).len(), 1);
        // Again: the same save is there, nothing more happens.
        assert_eq!(
            carry_save(&paths, &cart, Core::Mgba, Core::Gpsp).unwrap(),
            Carried::Same
        );
        assert_eq!(baks(gpsp.parent().unwrap()).len(), 1);
    }

    #[test]
    fn no_save_or_an_empty_one_carries_nothing() {
        let (_d, paths, cart) = card();
        let gpsp = save_path(&paths, &cart, Core::Gpsp).unwrap();
        let state = auto_state_path(&paths, &cart, Core::Mgba).unwrap();
        put(&state, b"session");
        assert_eq!(
            carry_save(&paths, &cart, Core::Gpsp, Core::Mgba).unwrap(),
            Carried::NoSave
        );
        put(&gpsp, &[0xff; 64]);
        assert_eq!(
            carry_save(&paths, &cart, Core::Gpsp, Core::Mgba).unwrap(),
            Carried::NoSave
        );
        assert!(state.exists(), "nothing carried, nothing set aside");
        // An empty save at the destination is simply replaced.
        let mgba = save_path(&paths, &cart, Core::Mgba).unwrap();
        put(&gpsp, b"progress");
        put(&mgba, &[0; 64]);
        assert_eq!(
            carry_save(&paths, &cart, Core::Gpsp, Core::Mgba).unwrap(),
            Carried::Copied
        );
        assert_eq!(std::fs::read(&mgba).unwrap(), b"progress");
        assert!(baks(mgba.parent().unwrap()).is_empty());
    }

    #[test]
    fn onion_cuts_only_a_lower_case_extension() {
        assert_eq!(
            onion_stem(Path::new("/r/Game.gba")).as_deref(),
            Some("Game")
        );
        assert_eq!(
            onion_stem(Path::new("/r/Game.zip")).as_deref(),
            Some("Game")
        );
        assert_eq!(
            onion_stem(Path::new("/r/Game.GBA")).as_deref(),
            Some("Game.GBA")
        );
        assert_eq!(onion_stem(Path::new("/r/A.b.gba")).as_deref(), Some("A.b"));
    }

    #[test]
    fn mgba_by_default_gpsp_when_chosen_and_back() {
        let (_d, paths, cart) = card();
        assert_eq!(
            default_core(&paths, Platform::Gba).as_deref(),
            Some("mgba_libretro")
        );
        assert_eq!(chosen(&paths, &cart), Core::Mgba);
        choose(&paths, &cart, Core::Gpsp).unwrap();
        let cfg = cfg_path(&cart).unwrap();
        assert_eq!(
            std::fs::read_to_string(&cfg).unwrap(),
            "core = \"gpsp_libretro\"\n"
        );
        assert_eq!(per_game(&cart).as_deref(), Some("gpsp_libretro"));
        assert_eq!(chosen(&paths, &cart), Core::Gpsp);
        // Back to the default: the file and the empty folder go.
        choose(&paths, &cart, Core::Mgba).unwrap();
        assert!(!cfg.exists());
        assert!(!cfg.parent().unwrap().exists());
        assert_eq!(chosen(&paths, &cart), Core::Mgba);
    }

    #[test]
    fn other_settings_in_the_file_are_kept() {
        let (_d, paths, cart) = card();
        let cfg = cfg_path(&cart).unwrap();
        std::fs::create_dir_all(cfg.parent().unwrap()).unwrap();
        std::fs::write(&cfg, "core = \"mgba_libretro\"\nother = \"1\"\n").unwrap();
        choose(&paths, &cart, Core::Gpsp).unwrap();
        assert_eq!(
            std::fs::read_to_string(&cfg).unwrap(),
            "other = \"1\"\ncore = \"gpsp_libretro\"\n"
        );
        choose(&paths, &cart, Core::Mgba).unwrap();
        assert_eq!(std::fs::read_to_string(&cfg).unwrap(), "other = \"1\"\n");
    }

    #[test]
    fn a_launch_script_that_picks_gpsp_makes_mgba_the_per_game_choice() {
        // Onion's development branch: `default_core=gpsp` in launch.sh.
        let (_d, paths, cart) = card();
        std::fs::write(
            paths.launch_script(Platform::Gba),
            "default_core=gpsp\n./retroarch -v -L .retroarch/cores/${default_core}_libretro.so \"$1\"\n",
        )
        .unwrap();
        assert_eq!(
            default_core(&paths, Platform::Gba).as_deref(),
            Some("gpsp_libretro")
        );
        assert_eq!(chosen(&paths, &cart), Core::Gpsp);
        choose(&paths, &cart, Core::Mgba).unwrap();
        assert_eq!(per_game(&cart).as_deref(), Some("mgba_libretro"));
        assert!(installed(&paths, "gpsp_libretro"));
        assert!(!installed(&paths, "gambatte_libretro"));
    }
}

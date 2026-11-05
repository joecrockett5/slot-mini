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

use std::path::{Path, PathBuf};

use slot_mini_store::{atomic_write, Cart, Core, Platform};

use crate::paths::Paths;

/// The rom's name as Onion's scripts cut it: `basename "$rompath" ".$romext"`
/// with the extension lower-cased, so `Game.GBA` keeps its extension.
pub fn onion_stem(rom: &Path) -> Option<String> {
    let name = rom.file_name()?.to_str()?;
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
    Some(match ext {
        Some(ext) => name
            .strip_suffix(&format!(".{ext}"))
            .filter(|s| !s.is_empty())
            .unwrap_or(name)
            .to_string(),
        None => name.to_string(),
    })
}

/// `<rom folder>/.game_config/<rom name>.cfg`.
pub fn cfg_path(cart: &Cart) -> Option<PathBuf> {
    let dir = cart.rom.parent()?;
    Some(
        dir.join(".game_config")
            .join(format!("{}.cfg", onion_stem(&cart.rom)?)),
    )
}

fn core_line(line: &str) -> Option<String> {
    let (k, v) = line.split_once('=')?;
    (k.trim() == "core").then(|| v.trim().trim_matches('"').trim().to_string())
}

/// The per-game core setting, as Onion's file names it (`gpsp_libretro`).
pub fn per_game(cart: &Cart) -> Option<String> {
    let text = std::fs::read_to_string(cfg_path(cart)?).ok()?;
    text.lines().find_map(core_line).filter(|c| !c.is_empty())
}

/// The core the system's `launch.sh` runs, the way Game List Options works
/// it out (`get_core_info`): a `default_core=` line, else the first
/// `.retroarch/cores/<name>.so`.
pub fn default_core(paths: &Paths, platform: Platform) -> Option<String> {
    let script = std::fs::read_to_string(paths.launch_script(platform)).ok()?;
    if let Some(line) = script.lines().find(|l| l.contains("default_core=")) {
        let v = line.split_once('=')?.1.trim().trim_matches('"');
        return (!v.is_empty() && !v.contains('$')).then(|| format!("{v}_libretro"));
    }
    const AT: &str = ".retroarch/cores/";
    let after = &script[script.find(AT)? + AT.len()..];
    Some(after[..after.find(".so")?].to_string())
}

/// Whether Onion's RetroArch has this core.
pub fn installed(paths: &Paths, core: &str) -> bool {
    paths
        .sd
        .join("RetroArch/.retroarch/cores")
        .join(format!("{core}.so"))
        .is_file()
}

/// The core this cart runs on now, as one of slot's cores: its own setting,
/// else the system's default, else mGBA (the default the maintainer chose,
/// and Onion 4.3.1-1's for GBA).
pub fn chosen(paths: &Paths, cart: &Cart) -> Core {
    per_game(cart)
        .or_else(|| default_core(paths, cart.platform))
        .and_then(|c| Core::from_libretro(&c))
        .filter(|c| c.runs(cart.platform))
        .unwrap_or(Core::Mgba)
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

/// The folder RetroArch sorts a core's saves and states into: the core's
/// name from its `.info` (`Saves/CurrentProfile/saves/mGBA/`).
pub fn sort_dir(core: Core) -> &'static str {
    match core {
        Core::Mgba => "mGBA",
        Core::Gpsp => "gpSP",
        Core::Gambatte => "Gambatte",
    }
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

/// A save that holds nothing: missing, or all `00` or all `ff` bytes (Onion's
/// `is_empty_save`).
fn empty_save(bytes: &[u8]) -> bool {
    bytes.iter().all(|&b| b == 0) || bytes.iter().all(|&b| b == 0xff)
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

/// `YYYYmmddHHMMSS` in local time, as Onion's backups are named.
fn stamp() -> String {
    // SAFETY: localtime_r writes only into `tm`.
    unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&now, &mut tm).is_null() {
            return now.to_string();
        }
        format!(
            "{:04}{:02}{:02}{:02}{:02}{:02}",
            tm.tm_year + 1900,
            tm.tm_mon + 1,
            tm.tm_mday,
            tm.tm_hour,
            tm.tm_min,
            tm.tm_sec
        )
    }
}

/// Set `path` aside as `<path>.bak_<stamp>`, never over an earlier backup.
fn set_aside(path: &Path, stamp: &str) -> std::io::Result<()> {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let mut to = path.with_file_name(format!("{name}.bak_{stamp}"));
    let mut n = 1;
    while to.exists() {
        to = path.with_file_name(format!("{name}.bak_{stamp}_{n}"));
        n += 1;
    }
    std::fs::rename(path, to)
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
        Ok(b) if !empty_save(&b) => b,
        Ok(_) => return Ok(Carried::NoSave),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Carried::NoSave),
        Err(e) => return Err(e),
    };
    let stamp = stamp();
    match std::fs::read(&dst) {
        Ok(old) if old == save => return Ok(Carried::Same),
        Ok(old) if !empty_save(&old) => set_aside(&dst, &stamp)?,
        _ => {}
    }
    if let Some(dir) = dst.parent() {
        std::fs::create_dir_all(dir)?;
    }
    atomic_write(&dst, &save)?;
    if state.is_file() {
        set_aside(&state, &stamp)?;
    }
    Ok(Carried::Copied)
}

#[cfg(test)]
mod tests {
    use super::*;

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

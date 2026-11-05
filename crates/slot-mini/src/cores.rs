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

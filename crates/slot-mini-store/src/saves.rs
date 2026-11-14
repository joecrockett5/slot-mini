//! Where a game's in-game save lives and how one is replaced, shared by
//! slot-mini's core picker and by sync on both the Miyoo and the PC.
//! (Moved from slot-mini's `cores.rs`, which documents where each rule
//! comes from in Onion; see also docs/PLATFORM.md, "Cores".)

use std::path::{Path, PathBuf};

use crate::{atomic_write, AtomicFile, Core, Platform};

/// The folder RetroArch sorts a core's saves and states into: the core's
/// name from its `.info` (`Saves/CurrentProfile/saves/mGBA/`).
pub fn sort_dir(core: Core) -> &'static str {
    match core {
        Core::Mgba => "mGBA",
        Core::Gpsp => "gpSP",
        Core::Gambatte => "Gambatte",
    }
}

/// A save that holds nothing: all `00` or all `ff` bytes, or no bytes
/// (Onion's `is_empty_save`).
pub fn is_empty_save(bytes: &[u8]) -> bool {
    bytes.iter().all(|&b| b == 0) || bytes.iter().all(|&b| b == 0xff)
}

/// `YYYYmmddHHMMSS` in local time, as Onion's backups are named.
pub fn stamp() -> String {
    // SAFETY: both calls write only into `tm`.
    unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        #[cfg(unix)]
        let ok = !libc::localtime_r(&now, &mut tm).is_null();
        #[cfg(windows)]
        let ok = libc::localtime_s(&mut tm, &now) == 0;
        if !ok {
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
/// Returns where it went.
pub fn set_aside(path: &Path, stamp: &str) -> std::io::Result<PathBuf> {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let mut to = path.with_file_name(format!("{name}.bak_{stamp}"));
    let mut n = 1;
    while to.exists() {
        to = path.with_file_name(format!("{name}.bak_{stamp}_{n}"));
        n += 1;
    }
    std::fs::rename(path, &to)?;
    Ok(to)
}

/// A save on its way in.
pub enum Incoming<'a> {
    Bytes(&'a [u8]),
    /// Already written and verified beside its destination.
    File(AtomicFile),
}

/// Put a newer save at `dst`, the way Onion's "Transfer mGBA save" does:
///
/// - a non-empty save already there is kept as `<name>.bak_<stamp>` first
///   (the caller has checked that it differs from the one coming in);
/// - the new save is written atomically;
/// - the auto state at `state` is set aside the same way, so the game
///   starts from the save just received rather than resuming an older
///   session (whose next in-game save would overwrite the newer progress).
///
/// Nothing is deleted.
pub fn receive_save(
    dst: &Path,
    state: Option<&Path>,
    incoming: Incoming,
    stamp: &str,
) -> std::io::Result<()> {
    match std::fs::read(dst) {
        Ok(old) if !is_empty_save(&old) => {
            set_aside(dst, stamp)?;
        }
        _ => {}
    }
    if let Some(dir) = dst.parent() {
        std::fs::create_dir_all(dir)?;
    }
    match incoming {
        Incoming::Bytes(bytes) => atomic_write(dst, bytes)?,
        Incoming::File(file) => file.commit()?,
    }
    if let Some(state) = state.filter(|s| s.is_file()) {
        set_aside(state, stamp)?;
    }
    Ok(())
}

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

/// Onion's per-game settings file: `<rom folder>/.game_config/<rom name>.cfg`.
pub fn game_config_path(rom: &Path) -> Option<PathBuf> {
    let dir = rom.parent()?;
    Some(
        dir.join(".game_config")
            .join(format!("{}.cfg", onion_stem(rom)?)),
    )
}

/// The `core` of a `core = "..."` line.
pub fn core_line(line: &str) -> Option<String> {
    let (k, v) = line.split_once('=')?;
    (k.trim() == "core").then(|| v.trim().trim_matches('"').trim().to_string())
}

/// The per-game core setting, as Onion's file names it (`gpsp_libretro`).
pub fn per_game_core(rom: &Path) -> Option<String> {
    let text = std::fs::read_to_string(game_config_path(rom)?).ok()?;
    text.lines().find_map(core_line).filter(|c| !c.is_empty())
}

/// The emulator's launch script on the card at `sd`.
pub fn launch_script(sd: &Path, platform: Platform) -> PathBuf {
    sd.join("Emu").join(platform.dir_name()).join("launch.sh")
}

/// The core the system's `launch.sh` runs, the way Game List Options works
/// it out (`get_core_info`): a `default_core=` line, else the first
/// `.retroarch/cores/<name>.so`.
pub fn default_core(sd: &Path, platform: Platform) -> Option<String> {
    let script = std::fs::read_to_string(launch_script(sd, platform)).ok()?;
    if let Some(line) = script.lines().find(|l| l.contains("default_core=")) {
        let v = line.split_once('=')?.1.trim().trim_matches('"');
        return (!v.is_empty() && !v.contains('$')).then(|| format!("{v}_libretro"));
    }
    const AT: &str = ".retroarch/cores/";
    let after = &script[script.find(AT)? + AT.len()..];
    Some(after[..after.find(".so")?].to_string())
}

/// The core a game on the card runs on now, as one of slot's cores: its own
/// setting, else the system's default, else mGBA (the default the
/// maintainer chose, and Onion 4.3.1-1's for GBA). `rom` is `None` for a
/// game whose file is not on the card: the system's default.
pub fn device_core(sd: &Path, platform: Platform, rom: Option<&Path>) -> Core {
    rom.and_then(per_game_core)
        .or_else(|| default_core(sd, platform))
        .and_then(|c| Core::from_libretro(&c))
        .filter(|c| c.runs(platform))
        .unwrap_or(Core::Mgba)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stamp_is_fourteen_digits() {
        let s = stamp();
        assert_eq!(s.len(), 14, "{s}");
        assert!(s.bytes().all(|b| b.is_ascii_digit()));
    }

    #[test]
    fn empty_saves() {
        assert!(is_empty_save(&[]));
        assert!(is_empty_save(&[0; 8]));
        assert!(is_empty_save(&[0xff; 8]));
        assert!(!is_empty_save(&[0, 0xff]));
    }

    #[test]
    fn a_received_save_keeps_what_it_replaces() {
        let d = tempfile::tempdir().unwrap();
        let dst = d.path().join("saves/mGBA/Game.srm");
        let state = d.path().join("states/mGBA/Game.state.auto");
        for p in [&dst, &state] {
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        }
        std::fs::write(&dst, b"old").unwrap();
        std::fs::write(&state, b"session").unwrap();
        receive_save(
            &dst,
            Some(&state),
            Incoming::Bytes(b"new"),
            "20260101000000",
        )
        .unwrap();
        assert_eq!(std::fs::read(&dst).unwrap(), b"new");
        assert_eq!(
            std::fs::read(dst.with_file_name("Game.srm.bak_20260101000000")).unwrap(),
            b"old"
        );
        assert!(!state.exists());
        assert!(state
            .with_file_name("Game.state.auto.bak_20260101000000")
            .exists());
        // Same stamp again: a numbered backup, never an overwrite.
        let mut f = AtomicFile::create(&dst).unwrap();
        std::io::Write::write_all(&mut f, b"newer").unwrap();
        receive_save(&dst, Some(&state), Incoming::File(f), "20260101000000").unwrap();
        assert_eq!(std::fs::read(&dst).unwrap(), b"newer");
        assert_eq!(
            std::fs::read(dst.with_file_name("Game.srm.bak_20260101000000_1")).unwrap(),
            b"new"
        );
    }
}

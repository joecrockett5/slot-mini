//! Handing a game to Onion: the command file, the fresh-start flag, Recents,
//! and remembering the cart for the trip back. See docs/PLATFORM.md,
//! "Launch flow" and "Recents and the Game Switcher".

use std::path::{Component, Path, PathBuf};

use slot_mini_store::{atomic_write, Cart, Platform};

use crate::paths::Paths;
use crate::settings::State;

/// How slot-mini was started.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    /// As an Onion app, from `App/slot-mini/launch.sh`.
    App,
    /// In MainUI's place, through the boot shim.
    Launcher,
}

/// Onion's preload for every launch, spelled as MainUI spells it.
pub const PRELOAD: &str = "LD_PRELOAD=/mnt/SDCARD/miyoo/app/../lib/libpadsp.so";
/// Recents keeps at most this many lines (Open MainUI's limit).
pub const RECENTS_MAX: usize = 50;

#[derive(Debug, PartialEq, Eq)]
pub enum LaunchError {
    /// The path would break Onion's command parsing (see `command_safe`).
    UnsafePath(String),
    /// The ROM is not under `Roms/<sys>/` on this card.
    NotOnCard,
    Io(String),
}

impl std::fmt::Display for LaunchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LaunchError::UnsafePath(p) => write!(
                f,
                "cannot launch {p}: rename it without \" ` \\ : or control characters"
            ),
            LaunchError::NotOnCard => write!(f, "the ROM is not in Roms/ on the SD card"),
            LaunchError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl From<std::io::Error> for LaunchError {
    fn from(e: std::io::Error) -> Self {
        LaunchError::Io(e.to_string())
    }
}

/// Onion's runtime splits the command on `" "` and on `:`, and runs it
/// through `sh`; it escapes `$` itself only for ROM paths under `/Roms/`.
/// Same rules as Open MainUI's `protocol_path`.
pub fn command_safe(path: &str, rom: bool) -> bool {
    if !path.starts_with("/mnt/SDCARD/") || path.len() >= 4096 {
        return false;
    }
    let dollar_ok = rom && (path.contains("/mnt/SDCARD/Roms/") || path.contains("/../../Roms/"));
    path.chars().all(|c| {
        !(c.is_control() || matches!(c, '"' | '`' | '\\' | ':') || (c == '$' && !dollar_ok))
    })
}

/// The record paths for a cart, as stock MainUI writes them: the emulator
/// folder plus its `rompath` (`../../Roms/<sys>`) plus the path under it.
pub struct Record {
    pub label: String,
    pub launch: String,
    pub rompath: String,
    pub imgpath: String,
}

pub fn record(paths: &Paths, cart: &Cart) -> Result<Record, LaunchError> {
    let sys = cart.platform.dir_name();
    let roms = paths.sd.join("Roms").join(sys);
    let rel = cart
        .rom
        .strip_prefix(&roms)
        .map_err(|_| LaunchError::NotOnCard)?;
    let rel = rel.to_str().ok_or(LaunchError::NotOnCard)?.to_string();
    let emu = Paths::device_emu_dir(cart.platform);
    let stem = Path::new(&rel)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(&cart.stem)
        .to_string();
    Ok(Record {
        label: cart.stem.clone(),
        launch: format!("{emu}/launch.sh"),
        rompath: format!("{emu}/../../Roms/{sys}/{rel}"),
        imgpath: format!("{emu}/../../Roms/{sys}/Imgs/{stem}.png"),
    })
}

/// The one-line command Onion's runtime runs.
pub fn command(r: &Record) -> Result<String, LaunchError> {
    if !command_safe(&r.launch, false) {
        return Err(LaunchError::UnsafePath(r.launch.clone()));
    }
    if !command_safe(&r.rompath, true) {
        return Err(LaunchError::UnsafePath(r.rompath.clone()));
    }
    Ok(format!("{PRELOAD} \"{}\" \"{}\"\n", r.launch, r.rompath))
}

/// The Recents line for a launch, as stock MainUI and Open MainUI write it.
pub fn recent_line(r: &Record) -> String {
    serde_json::json!({
        "label": r.label,
        "rompath": r.rompath,
        "imgpath": r.imgpath,
        "launch": r.launch,
        "type": 5,
    })
    .to_string()
}

/// `/a/b/../c` to `/a/c`, without touching the file system.
fn lexical(p: &str) -> PathBuf {
    let mut out = PathBuf::new();
    for c in Path::new(p).components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

/// The ROM a Recents line points at, resolved, if it is a game line.
fn line_rom(line: &str) -> Option<PathBuf> {
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    let rom = v.get("rompath")?.as_str()?;
    // A `launch.sh:/path` prefix carries a custom launcher (Onion's format).
    let rom = match rom.find("launch.sh:/") {
        Some(i) => &rom[i + "launch.sh:".len()..],
        None => rom,
    };
    Some(lexical(rom))
}

/// Which file Onion's readers use: the hidden list whenever it exists
/// (`getMiyooRecentFilePath` in Onion's state.h).
pub fn recents_path(paths: &Paths) -> PathBuf {
    let hidden = paths.sd.join("Roms/recentlist-hidden.json");
    match hidden.is_file() {
        true => hidden,
        false => paths.sd.join("Roms/recentlist.json"),
    }
}

/// Put the launch first in Recents, dropping older lines for the same ROM
/// and trimming to `RECENTS_MAX`. Lines slot-mini cannot read are kept as
/// they are.
pub fn add_recent(paths: &Paths, r: &Record) -> std::io::Result<()> {
    let path = recents_path(paths);
    let old = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e),
    };
    let ours = lexical(&r.rompath);
    let mut lines = vec![recent_line(r)];
    for line in old.lines() {
        if line.trim().is_empty() || line_rom(line).as_ref() == Some(&ours) {
            continue;
        }
        if lines.len() >= RECENTS_MAX {
            break;
        }
        lines.push(line.to_string());
    }
    let mut text = lines.join("\n");
    text.push('\n');
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    atomic_write(&path, text.as_bytes())
}

/// Everything that happens when a cart goes in: remember it, add it to
/// Recents, set or clear Onion's fresh-start flag, and queue the command.
pub fn hand_off(
    paths: &Paths,
    mode: Mode,
    shelf: &str,
    cart: &Cart,
    fresh: bool,
) -> Result<String, LaunchError> {
    let r = record(paths, cart)?;
    let cmd = command(&r)?;
    State {
        shelf: Some(shelf.to_string()),
        cart: Some((cart.platform, cart.stem.clone())),
        launched: mode == Mode::Launcher,
    }
    .write(paths)?;
    if let Err(e) = add_recent(paths, &r) {
        // Recents is Onion's history; a failure must not stop the game.
        eprintln!("slot-mini: recents: {e}");
    }
    let reset = paths.tmp.join("reset_game");
    let mut cmd = cmd;
    match fresh {
        // Onion's development build: its runtime's own flag, as its Game
        // List Options' "Reset game" sets it. The runtime adds
        // `/tmp/reset.cfg` for this one launch and takes it out again.
        true if crate::onion::Onion::detect(paths).reset_flag => std::fs::write(&reset, b"")?,
        true => match fresh_command(paths, cart)? {
            // Stable Onion: Game List Options' "Reset game" command.
            Some(direct) => {
                atomic_write(&paths.sysdir().join("reset.cfg"), RESET_CFG.as_bytes())?;
                remove_if_there(&reset)?;
                cmd = direct;
            }
            // A launch script whose core cannot be read (Onion's development
            // branch picks it at run time): that runtime's own flag.
            None => std::fs::write(&reset, b"")?,
        },
        false => remove_if_there(&reset)?,
    }
    match mode {
        // MainUI's hand-off: the runtime moves it into .tmp_update.
        Mode::Launcher => atomic_write(&paths.tmp.join("cmd_to_run.sh"), cmd.as_bytes())?,
        // An app queues the game the way AdvanceMENU and Random Game do.
        Mode::App => {
            atomic_write(&paths.sysdir().join("cmd_to_run.sh"), cmd.as_bytes())?;
            std::fs::write(paths.tmp.join("quick_switch"), b"")?;
        }
    }
    Ok(cmd)
}

fn remove_if_there(p: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(p) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// What Onion's Game List Options writes to `.tmp_update/reset.cfg` for
/// "Reset game" (`reset_game()` in game_list_options.sh, Onion 4.3.1-1):
/// no auto-load for this run, and do not save RetroArch's config on exit.
pub const RESET_CFG: &str = "savestate_auto_load = \"false\"\nconfig_save_on_exit = \"false\"\n";

/// The RetroArch core a cart runs on, the way Onion's Game List Options
/// works it out (`get_core_info`): a per-game override in
/// `<rom folder>/.game_config/<rom name>.cfg` (`core = "..."`), else the core
/// the system's `launch.sh` runs (see `cores.rs`). `None` when neither names
/// a core that is on the card.
pub fn core_for(paths: &Paths, cart: &Cart) -> Option<String> {
    let core = crate::cores::per_game(cart)
        .or_else(|| crate::cores::default_core(paths, cart.platform))?;
    let plain = core
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    (plain && crate::cores::installed(paths, &core)).then_some(core)
}

/// Hold A on stable Onion: the command Game List Options' "Reset game"
/// writes, RetroArch run directly with Onion's `reset.cfg` appended. Stable
/// Onion's runtime has no `/tmp/reset_game` flag (only its development
/// branch does), so this is what works on released Onion.
pub fn fresh_command(paths: &Paths, cart: &Cart) -> Result<Option<String>, LaunchError> {
    let Some(core) = core_for(paths, cart) else {
        return Ok(None);
    };
    let rel = cart
        .rom
        .strip_prefix(&paths.sd)
        .map_err(|_| LaunchError::NotOnCard)?
        .to_str()
        .ok_or(LaunchError::NotOnCard)?
        .to_string();
    let rom = format!("/mnt/SDCARD/{rel}");
    if !command_safe(&rom, true) {
        return Err(LaunchError::UnsafePath(rom));
    }
    Ok(Some(format!(
        "LD_PRELOAD=/mnt/SDCARD/miyoo/lib/libpadsp.so ./retroarch -v --appendconfig \"/mnt/SDCARD/.tmp_update/reset.cfg\" -L \"/mnt/SDCARD/RetroArch/.retroarch/cores/{core}.so\" \"{rom}\"\n"
    )))
}

/// Which emulator folder a Recents `launch` path names, if one of ours.
pub fn platform_of_launch(launch: &str) -> Option<Platform> {
    Platform::ALL
        .into_iter()
        .find(|p| launch == format!("{}/launch.sh", Paths::device_emu_dir(*p)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rig() -> (tempfile::TempDir, Paths) {
        let d = tempfile::tempdir().unwrap();
        let p = Paths::new(d.path().join("sd"), d.path().join("tmp"));
        std::fs::create_dir_all(p.sd.join("Roms/GBA/Sub")).unwrap();
        std::fs::create_dir_all(&p.tmp).unwrap();
        std::fs::create_dir_all(p.sysdir()).unwrap();
        (d, p)
    }

    fn cart(p: &Paths, rel: &str) -> Cart {
        let rom = p.sd.join("Roms/GBA").join(rel);
        Cart {
            platform: Platform::Gba,
            stem: rom.file_stem().unwrap().to_str().unwrap().into(),
            rom,
            label: None,
            title: String::new(),
            code: String::new(),
            shell: None,
        }
    }

    #[test]
    fn command_matches_mainui() {
        let (_d, p) = rig();
        let r = record(&p, &cart(&p, "Golden Sun (USA).gba")).unwrap();
        assert_eq!(
            command(&r).unwrap(),
            "LD_PRELOAD=/mnt/SDCARD/miyoo/app/../lib/libpadsp.so \"/mnt/SDCARD/Emu/GBA/launch.sh\" \"/mnt/SDCARD/Emu/GBA/../../Roms/GBA/Golden Sun (USA).gba\"\n"
        );
        assert_eq!(
            r.imgpath,
            "/mnt/SDCARD/Emu/GBA/../../Roms/GBA/Imgs/Golden Sun (USA).png"
        );
    }

    #[test]
    fn subfolders_keep_their_path() {
        let (_d, p) = rig();
        let r = record(&p, &cart(&p, "Sub/Game.gba")).unwrap();
        assert_eq!(r.rompath, "/mnt/SDCARD/Emu/GBA/../../Roms/GBA/Sub/Game.gba");
    }

    #[test]
    fn unsafe_names_are_refused() {
        let (_d, p) = rig();
        for bad in [
            "Zelda: Minish Cap.gba",
            "Say \"hi\".gba",
            "a`b.gba",
            "a\\b.gba",
        ] {
            let r = record(&p, &cart(&p, bad)).unwrap();
            assert!(
                matches!(command(&r), Err(LaunchError::UnsafePath(_))),
                "{bad}"
            );
        }
        let r = record(&p, &cart(&p, "Cash $$$.gba")).unwrap();
        assert!(command(&r).is_ok(), "$ is fine in a ROM path under Roms/");
        assert!(!command_safe("/mnt/SDCARD/Emu/$X/launch.sh", false));
    }

    #[test]
    fn recents_put_the_launch_first_once() {
        let (_d, p) = rig();
        let old = [
            r#"{"label":"Other","rompath":"/mnt/SDCARD/Emu/GB/../../Roms/GB/Other.gb","imgpath":"","launch":"/mnt/SDCARD/Emu/GB/launch.sh","type":5}"#,
            r#"{"label":"Golden Sun","rompath":"/mnt/SDCARD/Roms/GBA/Golden Sun.gba","launch":"/mnt/SDCARD/Emu/GBA/launch.sh","type":5}"#,
            "not json at all",
        ];
        std::fs::write(p.sd.join("Roms/recentlist.json"), old.join("\n") + "\n").unwrap();
        let r = record(&p, &cart(&p, "Golden Sun.gba")).unwrap();
        add_recent(&p, &r).unwrap();
        let text = std::fs::read_to_string(p.sd.join("Roms/recentlist.json")).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3);
        let first: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(first["label"], "Golden Sun");
        assert_eq!(first["type"], 5);
        assert_eq!(first["launch"], "/mnt/SDCARD/Emu/GBA/launch.sh");
        assert_eq!(lines[1], old[0]);
        assert_eq!(lines[2], old[2], "unreadable lines are left alone");
    }

    #[test]
    fn recents_go_to_the_hidden_list_when_onion_hides_them() {
        let (_d, p) = rig();
        std::fs::write(p.sd.join("Roms/recentlist-hidden.json"), "").unwrap();
        let r = record(&p, &cart(&p, "A.gba")).unwrap();
        add_recent(&p, &r).unwrap();
        assert!(!p.sd.join("Roms/recentlist.json").exists());
        let text = std::fs::read_to_string(p.sd.join("Roms/recentlist-hidden.json")).unwrap();
        assert_eq!(text.lines().count(), 1);
    }

    #[test]
    fn recents_are_capped() {
        let (_d, p) = rig();
        let many: String = (0..80)
            .map(|i| format!("{{\"label\":\"G{i}\",\"rompath\":\"/mnt/SDCARD/Roms/GB/G{i}.gb\",\"type\":5}}\n"))
            .collect();
        std::fs::write(p.sd.join("Roms/recentlist.json"), many).unwrap();
        add_recent(&p, &record(&p, &cart(&p, "A.gba")).unwrap()).unwrap();
        let text = std::fs::read_to_string(p.sd.join("Roms/recentlist.json")).unwrap();
        assert_eq!(text.lines().count(), RECENTS_MAX);
    }

    fn with_core(p: &Paths, script_core: &str, file: &str) {
        let emu = p.sd.join("Emu/GBA");
        std::fs::create_dir_all(&emu).unwrap();
        std::fs::write(
            emu.join("launch.sh"),
            format!("#!/bin/sh\ncd /mnt/SDCARD/RetroArch/\nHOME=/mnt/SDCARD/RetroArch/ ./retroarch -v -L .retroarch/cores/{script_core}.so \"$1\"\n"),
        )
        .unwrap();
        let cores = p.sd.join("RetroArch/.retroarch/cores");
        std::fs::create_dir_all(&cores).unwrap();
        std::fs::write(cores.join(format!("{file}.so")), b"core").unwrap();
    }

    #[test]
    fn the_core_comes_from_the_launch_script_or_a_per_game_override() {
        let (_d, p) = rig();
        with_core(&p, "mgba_libretro", "mgba_libretro");
        let c = cart(&p, "A.gba");
        assert_eq!(core_for(&p, &c).as_deref(), Some("mgba_libretro"));
        // Game List Options' per-game core wins, if that core is installed.
        std::fs::create_dir_all(p.sd.join("Roms/GBA/.game_config")).unwrap();
        std::fs::write(
            p.sd.join("Roms/GBA/.game_config/A.cfg"),
            "core = \"gpsp_libretro\"\n",
        )
        .unwrap();
        assert_eq!(core_for(&p, &c), None, "gpsp is not on the card");
        std::fs::write(
            p.sd.join("RetroArch/.retroarch/cores/gpsp_libretro.so"),
            b"core",
        )
        .unwrap();
        assert_eq!(core_for(&p, &c).as_deref(), Some("gpsp_libretro"));
    }

    #[test]
    fn a_core_picked_at_run_time_cannot_be_read() {
        let (_d, p) = rig();
        // Onion's development branch: `-L .retroarch/cores/${default_core}_libretro.so`.
        with_core(&p, "${default_core}_libretro", "gpsp_libretro");
        assert_eq!(core_for(&p, &cart(&p, "A.gba")), None);
    }

    #[test]
    fn hold_a_writes_onions_reset_game_command() {
        let (_d, p) = rig();
        with_core(&p, "mgba_libretro", "mgba_libretro");
        std::fs::write(p.tmp.join("reset_game"), b"").unwrap();
        let cmd = hand_off(&p, Mode::Launcher, "GBA", &cart(&p, "Sub/Game.gba"), true).unwrap();
        assert_eq!(
            cmd,
            "LD_PRELOAD=/mnt/SDCARD/miyoo/lib/libpadsp.so ./retroarch -v --appendconfig \"/mnt/SDCARD/.tmp_update/reset.cfg\" -L \"/mnt/SDCARD/RetroArch/.retroarch/cores/mgba_libretro.so\" \"/mnt/SDCARD/Roms/GBA/Sub/Game.gba\"\n"
        );
        assert_eq!(
            std::fs::read_to_string(p.tmp.join("cmd_to_run.sh")).unwrap(),
            cmd
        );
        assert_eq!(
            std::fs::read_to_string(p.sysdir().join("reset.cfg")).unwrap(),
            RESET_CFG
        );
        assert!(
            !p.tmp.join("reset_game").exists(),
            "no stray flag for a later launch"
        );
        // Recents still names the emulator's launch script, as Onion's does.
        let recents = std::fs::read_to_string(p.sd.join("Roms/recentlist.json")).unwrap();
        assert!(
            recents.contains("\"launch\":\"/mnt/SDCARD/Emu/GBA/launch.sh\""),
            "{recents}"
        );
    }

    #[test]
    fn hold_a_on_the_development_build_sets_the_runtimes_flag() {
        let (_d, p) = rig();
        // Onion's development build: its GBA script picks the core at run
        // time, and its runtime knows /tmp/reset_game.
        with_core(&p, "${default_core}_libretro", "gpsp_libretro");
        std::fs::write(
            p.sysdir().join("runtime.sh"),
            "    if [ -f /tmp/reset_game ]; then\n",
        )
        .unwrap();
        let c = cart(&p, "Sub/Game.gba");
        let cmd = hand_off(&p, Mode::Launcher, "GBA", &c, true).unwrap();
        assert_eq!(
            cmd,
            command(&record(&p, &c).unwrap()).unwrap(),
            "the normal command"
        );
        assert!(p.tmp.join("reset_game").is_file());
        assert!(!p.sysdir().join("reset.cfg").exists());
        // Also when the core could be read: the flag is what this runtime
        // understands and cleans up after.
        std::fs::remove_file(p.tmp.join("reset_game")).unwrap();
        with_core(&p, "mgba_libretro", "mgba_libretro");
        let cmd = hand_off(&p, Mode::App, "GBA", &c, true).unwrap();
        assert_eq!(cmd, command(&record(&p, &c).unwrap()).unwrap());
        assert!(p.tmp.join("reset_game").is_file());
    }

    #[test]
    fn hand_off_in_each_mode() {
        let (_d, p) = rig();
        let c = cart(&p, "A.gba");
        hand_off(&p, Mode::Launcher, "GBA", &c, true).unwrap();
        assert!(p.tmp.join("cmd_to_run.sh").is_file());
        assert!(p.tmp.join("reset_game").is_file());
        assert!(!p.tmp.join("quick_switch").exists());
        let s = State::read(&p);
        assert!(s.launched);
        assert_eq!(s.cart, Some((Platform::Gba, "A".into())));

        std::fs::remove_file(p.tmp.join("cmd_to_run.sh")).unwrap();
        hand_off(&p, Mode::App, "GBA", &c, false).unwrap();
        assert!(!p.tmp.join("cmd_to_run.sh").exists());
        assert!(p.sysdir().join("cmd_to_run.sh").is_file());
        assert!(p.tmp.join("quick_switch").is_file());
        assert!(
            !p.tmp.join("reset_game").exists(),
            "tap A clears a stale fresh-start flag"
        );
        assert!(
            !State::read(&p).launched,
            "an app does not come back after the game"
        );
    }
}

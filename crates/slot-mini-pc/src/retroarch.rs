//! Finding a standalone RetroArch, its cores, and running a game in it.
//!
//! Facts read from libretro/RetroArch (see docs/DECISIONS.md #65): an
//! `--appendconfig` file's settings are written back into `retroarch.cfg`
//! when RetroArch saves its config on exit, so ours switches that off for
//! the run; with the sort settings on, saves and states go into a folder
//! named after the core (mGBA, gpSP, Gambatte), which is Onion's layout.

use std::path::{Path, PathBuf};
use std::process::Command;

use slot_mini::cores;
use slot_mini_store::Cart;
use slot_mini_store::Core;

/// RetroArch's program file on this system.
pub const EXE: &str = if cfg!(windows) {
    "retroarch.exe"
} else {
    "retroarch"
};

/// Where libretro publishes the cores RetroArch's own downloader fetches.
pub const BUILDBOT: &str = "https://buildbot.libretro.com/nightly/windows/x86_64/latest";

/// A RetroArch on this PC.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetroArch {
    pub exe: PathBuf,
    /// The folder its cores are in (it may not exist yet).
    pub cores: PathBuf,
}

/// Where RetroArch usually is: the installer's default, then the places a
/// portable copy tends to be unpacked, then Steam and Scoop.
pub fn usual_places() -> Vec<PathBuf> {
    let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
    let mut dirs: Vec<PathBuf> = Vec::new();
    if cfg!(windows) {
        dirs.push(PathBuf::from("C:\\RetroArch-Win64"));
        dirs.push(PathBuf::from("C:\\RetroArch"));
        if let Some(home) = env("USERPROFILE") {
            for sub in [
                "RetroArch-Win64",
                "RetroArch",
                "Downloads\\RetroArch-Win64",
                "Desktop\\RetroArch-Win64",
                "Documents\\RetroArch",
                "scoop\\apps\\retroarch\\current",
            ] {
                dirs.push(home.join(sub));
            }
        }
        if let Some(roaming) = env("APPDATA") {
            dirs.push(roaming.join("RetroArch"));
        }
        for key in ["ProgramFiles", "ProgramFiles(x86)"] {
            if let Some(pf) = env(key) {
                dirs.push(pf.join("RetroArch"));
                dirs.push(pf.join("Steam\\steamapps\\common\\RetroArch"));
            }
        }
    } else if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }
    dirs.into_iter().map(|d| d.join(EXE)).collect()
}

/// The RetroArch to use: the remembered one if it is still there, else the
/// first of the usual places that has one.
pub fn find(remembered: Option<&Path>, places: &[PathBuf]) -> Option<RetroArch> {
    remembered
        .into_iter()
        .chain(places.iter().map(PathBuf::as_path))
        .find(|p| p.is_file())
        .map(at)
}

/// The RetroArch whose program file is `exe`.
pub fn at(exe: &Path) -> RetroArch {
    RetroArch {
        exe: exe.to_path_buf(),
        cores: cores_dir(exe),
    }
}

/// The value of `key = "value"` in a RetroArch config.
pub fn cfg_value(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let (k, v) = line.split_once('=')?;
        (k.trim() == key).then(|| v.trim().trim_matches('"').to_string())
    })
}

/// The config files RetroArch may be using for the program at `exe`: the
/// one beside it (a portable copy and the Windows installer), then the
/// user's.
fn cfg_files(exe: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Some(dir) = exe.parent() {
        files.push(dir.join("retroarch.cfg"));
    }
    let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
    if cfg!(windows) {
        if let Some(roaming) = env("APPDATA") {
            files.push(roaming.join("RetroArch\\retroarch.cfg"));
        }
    } else {
        let config = env("XDG_CONFIG_HOME").or_else(|| env("HOME").map(|h| h.join(".config")));
        if let Some(config) = config {
            files.push(config.join("retroarch/retroarch.cfg"));
        }
    }
    files
}

/// Where RetroArch keeps its cores: `libretro_directory` from its config,
/// else `cores` beside the program. RetroArch writes paths inside its own
/// folder with a leading `:` and the home folder as `~`.
pub fn cores_dir(exe: &Path) -> PathBuf {
    let beside = exe.parent().unwrap_or(Path::new("."));
    let set = cfg_files(exe)
        .iter()
        .filter_map(|f| std::fs::read_to_string(f).ok())
        .find_map(|text| cfg_value(&text, "libretro_directory"))
        .filter(|v| !v.is_empty() && v != "default");
    match set {
        Some(v) => expand(&v, beside),
        None => beside.join("cores"),
    }
}

fn expand(value: &str, beside: &Path) -> PathBuf {
    let rest = |s: &str| s.trim_start_matches(['\\', '/']).to_string();
    if let Some(inside) = value.strip_prefix(':') {
        return beside.join(rest(inside));
    }
    if let Some(inside) = value.strip_prefix('~') {
        if let Some(home) = crate::config::home() {
            return home.join(rest(inside));
        }
    }
    PathBuf::from(value)
}

/// A core's file in a cores folder (`mgba_libretro.dll`).
pub fn core_file(cores: &Path, core: Core) -> PathBuf {
    cores.join(format!(
        "{}{}",
        core.libretro(),
        std::env::consts::DLL_SUFFIX
    ))
}

/// Where libretro's buildbot has a core for Windows.
pub fn core_url(core: Core) -> String {
    format!("{BUILDBOT}/{}.dll.zip", core.libretro())
}

/// A path the way RetroArch's config wants it: quoted, with forward
/// slashes (a backslash before a quote would escape it).
fn cfg_path(p: &Path) -> String {
    p.display().to_string().replace('\\', "/")
}

/// The settings for one run, appended to the player's own config: saves
/// and states go to the library in Onion's layout, the game resumes from
/// its auto state unless `fresh`, and nothing here is written back.
pub fn append_config(library: &Path, fresh: bool, fullscreen: bool) -> String {
    let profile = library.join("Saves/CurrentProfile");
    let flag = |b: bool| if b { "true" } else { "false" };
    let mut s = String::new();
    for (key, value) in [
        ("savefile_directory", cfg_path(&profile.join("saves"))),
        ("savestate_directory", cfg_path(&profile.join("states"))),
        ("sort_savefiles_enable", "true".into()),
        ("sort_savestates_enable", "true".into()),
        ("sort_savefiles_by_content_enable", "false".into()),
        ("sort_savestates_by_content_enable", "false".into()),
        ("savefiles_in_content_dir", "false".into()),
        ("savestates_in_content_dir", "false".into()),
        ("savestate_auto_save", "true".into()),
        ("savestate_auto_load", flag(!fresh).into()),
        ("config_save_on_exit", "false".into()),
        ("video_fullscreen", flag(fullscreen).into()),
    ] {
        s.push_str(&format!("{key} = \"{value}\"\n"));
    }
    s
}

/// One game about to run.
#[derive(Clone, Debug)]
pub struct Run {
    pub cart: Cart,
    /// The core it runs on: the PC's choice for this game.
    pub core: Core,
    pub core_file: PathBuf,
    /// Where RetroArch will keep this game's save.
    pub save: Option<PathBuf>,
    pub fresh: bool,
}

/// Work out how `cart` runs: its core and where its save goes.
pub fn plan(paths: &slot_mini::Paths, ra: &RetroArch, cart: &Cart, fresh: bool) -> Run {
    let core = cores::chosen(paths, cart);
    Run {
        cart: cart.clone(),
        core,
        core_file: core_file(&ra.cores, core),
        save: cores::save_path(paths, cart, core),
        fresh,
    }
}

/// Write the run's config to `cfg` and build the command:
/// `retroarch -L <core> <rom> --appendconfig <cfg>`.
pub fn command(
    ra: &RetroArch,
    run: &Run,
    library: &Path,
    cfg: &Path,
    fullscreen: bool,
) -> std::io::Result<Command> {
    let profile = library.join("Saves/CurrentProfile");
    for kind in ["saves", "states"] {
        std::fs::create_dir_all(profile.join(kind).join(cores::sort_dir(run.core)))?;
    }
    if let Some(dir) = cfg.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(cfg, append_config(library, run.fresh, fullscreen))?;
    let mut cmd = Command::new(&ra.exe);
    cmd.arg("-L")
        .arg(&run.core_file)
        .arg(&run.cart.rom)
        .arg("--appendconfig")
        .arg(cfg);
    if let Some(dir) = ra.exe.parent().filter(|d| d.is_dir()) {
        // A portable RetroArch finds its own files from its folder.
        cmd.current_dir(dir);
    }
    Ok(cmd)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cores_folder_comes_from_the_config_else_sits_beside_the_program() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join(EXE);
        std::fs::write(&exe, b"").unwrap();
        assert_eq!(cores_dir(&exe), dir.path().join("cores"));
        std::fs::write(
            dir.path().join("retroarch.cfg"),
            "video_driver = \"gl\"\nlibretro_directory = \":\\my cores\"\n",
        )
        .unwrap();
        assert_eq!(cores_dir(&exe), dir.path().join("my cores"));
        std::fs::write(
            dir.path().join("retroarch.cfg"),
            "libretro_directory = \"D:\\Emu\\cores\"\n",
        )
        .unwrap();
        assert_eq!(cores_dir(&exe), PathBuf::from("D:\\Emu\\cores"));
    }

    #[test]
    fn the_remembered_retroarch_wins_while_it_exists() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (
            dir.path().join("a").join(EXE),
            dir.path().join("b").join(EXE),
        );
        std::fs::create_dir_all(b.parent().unwrap()).unwrap();
        std::fs::write(&b, b"").unwrap();
        let places = vec![dir.path().join("none").join(EXE), b.clone()];
        // Remembered but gone: the usual places are searched.
        assert_eq!(find(Some(&a), &places).unwrap().exe, b);
        std::fs::create_dir_all(a.parent().unwrap()).unwrap();
        std::fs::write(&a, b"").unwrap();
        assert_eq!(find(Some(&a), &places).unwrap().exe, a);
        assert_eq!(find(None, &places[..1]), None);
    }

    #[test]
    fn the_appended_config_keeps_saves_in_the_library_and_writes_nothing_back() {
        let cfg = append_config(Path::new("C:\\Users\\Joe\\slot-mini"), false, true);
        let get = |k: &str| cfg_value(&cfg, k).unwrap();
        assert_eq!(
            get("savefile_directory"),
            "C:/Users/Joe/slot-mini/Saves/CurrentProfile/saves"
        );
        assert_eq!(
            get("savestate_directory"),
            "C:/Users/Joe/slot-mini/Saves/CurrentProfile/states"
        );
        assert_eq!(get("sort_savefiles_enable"), "true");
        assert_eq!(get("sort_savestates_enable"), "true");
        assert_eq!(get("sort_savefiles_by_content_enable"), "false");
        assert_eq!(get("savestate_auto_save"), "true");
        assert_eq!(get("savestate_auto_load"), "true");
        assert_eq!(get("config_save_on_exit"), "false");
        assert_eq!(get("video_fullscreen"), "true");
        // Hold A: start the game from the beginning.
        let fresh = append_config(Path::new("/lib"), true, false);
        assert_eq!(cfg_value(&fresh, "savestate_auto_load").unwrap(), "false");
        assert_eq!(cfg_value(&fresh, "video_fullscreen").unwrap(), "false");
    }

    #[test]
    fn a_core_is_fetched_from_libretros_buildbot() {
        assert_eq!(
            core_url(Core::Mgba),
            "https://buildbot.libretro.com/nightly/windows/x86_64/latest/mgba_libretro.dll.zip"
        );
    }
}

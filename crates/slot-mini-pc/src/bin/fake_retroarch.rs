//! Stands in for RetroArch in the tests (never shipped): notes how it was
//! run, writes a save where the appended config says, and exits.
//!
//! `fake.txt` beside the program sets it up, one `key = value` a line:
//! `log` names a file to write the arguments and the appended config to,
//! `ms` is how long the "game" lasts, `exit` is the exit code. A test
//! copies the program into its own folder, so tests do not share one.

use std::path::{Path, PathBuf};

fn value(cfg: &str, key: &str) -> Option<String> {
    cfg.lines().find_map(|line| {
        let (k, v) = line.split_once('=')?;
        (k.trim() == key).then(|| v.trim().trim_matches('"').to_string())
    })
}

/// The folder a core's saves are sorted into: its library name.
fn sort_dir(core: &Path) -> &'static str {
    let name = core.file_name().and_then(|n| n.to_str()).unwrap_or("");
    match name.split('_').next().unwrap_or("") {
        "gpsp" => "gpSP",
        "gambatte" => "Gambatte",
        _ => "mGBA",
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let after = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
            .map(PathBuf::from)
    };
    let core = after("-L").unwrap_or_default();
    let cfg_file = after("--appendconfig").unwrap_or_default();
    let cfg = std::fs::read_to_string(&cfg_file).unwrap_or_default();
    // The content is the argument that belongs to no flag.
    let rom = args
        .iter()
        .enumerate()
        .find(|(i, a)| {
            !a.starts_with('-')
                && (*i == 0 || !matches!(args[i - 1].as_str(), "-L" | "--appendconfig"))
        })
        .map(|(_, a)| PathBuf::from(a))
        .unwrap_or_default();
    let setup = std::env::current_exe()
        .ok()
        .and_then(|exe| std::fs::read_to_string(exe.with_file_name("fake.txt")).ok())
        .unwrap_or_default();
    if let Some(log) = value(&setup, "log") {
        let text = format!("args: {}\n{cfg}", args.join("\t"));
        std::fs::write(log, text).expect("write the log");
    }
    let ms = value(&setup, "ms")
        .and_then(|v| v.parse().ok())
        .unwrap_or(50);
    std::thread::sleep(std::time::Duration::from_millis(ms));
    if let (Some(dir), Some(stem)) = (value(&cfg, "savefile_directory"), rom.file_stem()) {
        let dir = PathBuf::from(dir).join(sort_dir(&core));
        std::fs::create_dir_all(&dir).expect("make the save folder");
        let save = dir.join(format!("{}.srm", stem.to_string_lossy()));
        std::fs::write(save, b"saved by the fake RetroArch").expect("write the save");
    }
    let code = value(&setup, "exit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    std::process::exit(code);
}

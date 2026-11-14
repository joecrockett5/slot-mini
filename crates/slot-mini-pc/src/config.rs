//! What this PC remembers outside any library: where the library and
//! RetroArch are, and how the window was left. Kept in
//! `%APPDATA%\slot-mini-pc\config.txt` (`~/.config/slot-mini-pc` elsewhere)
//! as `key = value` lines, like the app's other settings.

use std::path::{Path, PathBuf};

pub const FILE: &str = "config.txt";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Config {
    /// The library folder, once chosen.
    pub library: Option<PathBuf>,
    /// `retroarch.exe`, once found or chosen.
    pub retroarch: Option<PathBuf>,
    pub fullscreen: bool,
    /// Controllers by Xbox labels (bottom = A) instead of by position in
    /// the Miyoo's Nintendo layout (right = A).
    pub xbox_layout: bool,
    /// The window's outer position and inner size when it was last closed.
    pub window: Option<[i32; 4]>,
    pub maximized: bool,
}

/// Where the config lives. `SLOT_MINI_PC_CONFIG` overrides it (tests, and
/// a portable copy).
pub fn dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("SLOT_MINI_PC_CONFIG") {
        return PathBuf::from(dir);
    }
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| home().map(|h| h.join(".config")))
    };
    base.unwrap_or_else(|| PathBuf::from("."))
        .join("slot-mini-pc")
}

/// The user's home folder.
pub fn home() -> Option<PathBuf> {
    let key = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(key).map(PathBuf::from)
}

/// The library the first run offers: `%USERPROFILE%\slot-mini`.
pub fn default_library() -> PathBuf {
    home()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("slot-mini")
}

impl Config {
    pub fn read(dir: &Path) -> Config {
        let text = std::fs::read_to_string(dir.join(FILE)).unwrap_or_default();
        Config::parse(&text)
    }

    pub fn parse(text: &str) -> Config {
        let mut c = Config::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim();
            let on = value.eq_ignore_ascii_case("on");
            match key.trim() {
                "library" if !value.is_empty() => c.library = Some(PathBuf::from(value)),
                "retroarch" if !value.is_empty() => c.retroarch = Some(PathBuf::from(value)),
                "fullscreen" => c.fullscreen = on,
                "controller" => c.xbox_layout = value.eq_ignore_ascii_case("xbox"),
                "maximized" => c.maximized = on,
                "window" => {
                    let n: Vec<i32> = value
                        .split(',')
                        .filter_map(|v| v.trim().parse().ok())
                        .collect();
                    if let [x, y, w, h] = n[..] {
                        c.window = (w > 0 && h > 0).then_some([x, y, w, h]);
                    }
                }
                _ => {}
            }
        }
        c
    }

    pub fn text(&self) -> String {
        let on = |b: bool| if b { "on" } else { "off" };
        let path = |p: &Option<PathBuf>| {
            p.as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default()
        };
        let mut s = format!(
            "library = {}\nretroarch = {}\nfullscreen = {}\ncontroller = {}\nmaximized = {}\n",
            path(&self.library),
            path(&self.retroarch),
            on(self.fullscreen),
            if self.xbox_layout { "xbox" } else { "nintendo" },
            on(self.maximized),
        );
        if let Some([x, y, w, h]) = self.window {
            s.push_str(&format!("window = {x},{y},{w},{h}\n"));
        }
        s
    }

    /// Written through a temporary file, so a crash leaves the old one.
    pub fn write(&self, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let tmp = dir.join(format!("{FILE}.tmp"));
        std::fs::write(&tmp, self.text())?;
        std::fs::rename(tmp, dir.join(FILE))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_config_reads_back_as_written() {
        let dir = tempfile::tempdir().unwrap();
        let c = Config {
            library: Some(PathBuf::from("C:\\Users\\Joe Bloggs\\slot-mini")),
            retroarch: Some(PathBuf::from("C:\\RetroArch-Win64\\retroarch.exe")),
            fullscreen: true,
            xbox_layout: true,
            window: Some([-1200, 40, 1280, 960]),
            maximized: false,
        };
        c.write(dir.path()).unwrap();
        assert_eq!(Config::read(dir.path()), c);
    }

    #[test]
    fn a_missing_or_odd_file_is_the_defaults() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(Config::read(dir.path()), Config::default());
        let c = Config::parse("window = 1,2,0,5\nnonsense\ncontroller = sideways\n");
        assert_eq!(c, Config::default());
    }
}

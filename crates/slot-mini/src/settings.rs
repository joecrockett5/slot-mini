//! slot-mini's own files: settings, favourites and where the player was.
//! All live in `App/slot-mini/data/` in slot's `key = value` format.

use std::path::Path;

use slot_mini_store::{atomic_write, ini, Platform};

use crate::paths::Paths;

pub const SETTINGS_FILE: &str = "settings.txt";
pub const STATE_FILE: &str = "state.txt";
pub const FAVOURITES_FILE: &str = "favorites.txt";
pub const RECENTS_FILE: &str = "recents.txt";
/// slot: `RECENTS_MAX`. The Recents shelf holds the last ten games played.
pub const RECENTS_MAX: usize = 10;

/// The default label service, slot's art host. Off unless `label_service = on`.
pub const ART_HOST: &str = "https://art.slot-cfw.fyi";

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// Play slot's insert and eject sounds (see docs/PLATFORM.md, Audio).
    pub sounds: bool,
    /// Fetch labels from slot's art service. Off until its author approves
    /// third-party use.
    pub label_service: bool,
    pub art_host: String,
    pub clock_24h: bool,
    /// slot's in-game MENU (tap: menu, hold: eject) for games slot-mini
    /// launches. Off gives MENU back to Onion.
    pub ingame_menu: bool,
    /// Where MENU -> "Check for updates" looks: `owner/repo` on GitHub.
    pub update_repo: String,
    /// The player turned the boot launcher on. An Onion update puts Onion's
    /// own MainUI back; slot-mini then installs itself again
    /// (`switch::keep`).
    pub boot_launcher: bool,
    /// The player turned slot's boot and shutdown screens on (MENU ->
    /// Splash screens; the key keeps its 0.5.1 name). Put back at start if
    /// Onion's theme overrides were reset (`bootscreen::keep`).
    pub boot_screen: bool,
    /// Show slot's Recents shelf: the last games launched from slot-mini.
    /// (slot's is off until switched on; slot-mini's is on.)
    pub recents: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            sounds: false,
            label_service: false,
            art_host: ART_HOST.to_string(),
            clock_24h: true,
            ingame_menu: true,
            update_repo: crate::update::REPO.to_string(),
            boot_launcher: false,
            boot_screen: false,
            recents: true,
        }
    }
}

fn on(v: Option<&String>, default: bool) -> bool {
    match v.map(|s| s.to_ascii_lowercase()) {
        Some(s) if matches!(s.as_str(), "on" | "1" | "true" | "yes") => true,
        Some(s) if matches!(s.as_str(), "off" | "0" | "false" | "no") => false,
        _ => default,
    }
}

impl Settings {
    pub fn read(paths: &Paths) -> Settings {
        let m = ini::read(&paths.data(), SETTINGS_FILE);
        let d = Settings::default();
        Settings {
            sounds: on(m.get("sounds"), d.sounds),
            label_service: on(m.get("label_service"), d.label_service),
            art_host: m
                .get("art_host")
                .filter(|h| !h.is_empty())
                .cloned()
                .unwrap_or(d.art_host),
            clock_24h: m.get("clock").is_none_or(|c| c != "12h"),
            ingame_menu: on(m.get("ingame_menu"), d.ingame_menu),
            update_repo: m
                .get("update_repo")
                .filter(|r| !r.is_empty())
                .cloned()
                .unwrap_or(d.update_repo),
            boot_launcher: on(m.get("boot_launcher"), d.boot_launcher),
            boot_screen: on(m.get("boot_screen"), d.boot_screen),
            recents: on(m.get("recents"), d.recents),
        }
    }

    pub fn write(&self, paths: &Paths) -> std::io::Result<()> {
        let path = paths.data().join(SETTINGS_FILE);
        let mut text = std::fs::read_to_string(&path).unwrap_or_default();
        let flag = |b: bool| if b { "on" } else { "off" };
        text = ini::set(&text, "sounds", Some(flag(self.sounds)))?;
        text = ini::set(&text, "label_service", Some(flag(self.label_service)))?;
        text = ini::set(&text, "ingame_menu", Some(flag(self.ingame_menu)))?;
        text = ini::set(&text, "boot_launcher", Some(flag(self.boot_launcher)))?;
        text = ini::set(&text, "boot_screen", Some(flag(self.boot_screen)))?;
        text = ini::set(&text, "recents", Some(flag(self.recents)))?;
        text = ini::set(
            &text,
            "clock",
            Some(if self.clock_24h { "24h" } else { "12h" }),
        )?;
        write(&path, &text)
    }
}

fn write(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    if std::fs::read(path).is_ok_and(|old| old == text.as_bytes()) {
        return Ok(());
    }
    atomic_write(path, text.as_bytes())
}

/// What the player was looking at, and what they launched.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    /// `favorites` or a system folder name (`GBA`, `GB`, `GBC`).
    pub shelf: Option<String>,
    pub cart: Option<(Platform, String)>,
    /// Set when a game was launched; cleared when slot-mini next starts, so
    /// the first frame can play the eject.
    pub launched: bool,
}

impl State {
    pub fn read(paths: &Paths) -> State {
        let m = ini::read(&paths.data(), STATE_FILE);
        State {
            shelf: m.get("shelf").filter(|s| !s.is_empty()).cloned(),
            cart: m.get("cart").and_then(|v| entry_value(v)),
            launched: on(m.get("launched"), false),
        }
    }

    pub fn write(&self, paths: &Paths) -> std::io::Result<()> {
        let text = format!(
            "shelf = {}\ncart = {}\nlaunched = {}\n",
            self.shelf.as_deref().unwrap_or(""),
            self.cart
                .as_ref()
                .map(|(p, s)| entry_key(*p, s))
                .unwrap_or_default(),
            if self.launched { "on" } else { "off" },
        );
        write(&paths.data().join(STATE_FILE), &text)
    }
}

// slot's favourites list format (slot-store/src/slot_state.rs): one
// `<platform>/<stem>` per line, platform lowercase.
fn entry_key(platform: Platform, stem: &str) -> String {
    format!("{}/{}", platform.dir_name().to_ascii_lowercase(), stem)
}

fn entry_value(value: &str) -> Option<(Platform, String)> {
    let (platform, stem) = value.split_once('/')?;
    let platform = Platform::ALL
        .into_iter()
        .find(|p| platform.eq_ignore_ascii_case(p.dir_name()))?;
    (!stem.is_empty()).then(|| (platform, stem.to_string()))
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Favourites {
    pub list: Vec<(Platform, String)>,
}

impl Favourites {
    pub fn read(paths: &Paths) -> Favourites {
        let text = std::fs::read_to_string(paths.data().join(FAVOURITES_FILE)).unwrap_or_default();
        let mut list = Vec::new();
        for e in text.lines().filter_map(entry_value) {
            if !list.contains(&e) {
                list.push(e);
            }
        }
        Favourites { list }
    }

    pub fn write(&self, paths: &Paths) -> std::io::Result<()> {
        let text: String = self
            .list
            .iter()
            .map(|(p, s)| entry_key(*p, s) + "\n")
            .collect();
        write(&paths.data().join(FAVOURITES_FILE), &text)
    }

    pub fn contains(&self, platform: Platform, stem: &str) -> bool {
        self.list.iter().any(|(p, s)| *p == platform && s == stem)
    }

    /// Heart or un-heart; returns whether it is now a favourite.
    pub fn toggle(&mut self, platform: Platform, stem: &str) -> bool {
        if self.contains(platform, stem) {
            self.list.retain(|(p, s)| !(*p == platform && s == stem));
            return false;
        }
        self.list.push((platform, stem.to_string()));
        true
    }
}

/// The games last launched from slot-mini, newest first, in slot's list
/// format (`recents.txt`, slot-store/src/slot_state.rs).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Recents {
    pub list: Vec<(Platform, String)>,
}

impl Recents {
    pub fn read(paths: &Paths) -> Recents {
        let text = std::fs::read_to_string(paths.data().join(RECENTS_FILE)).unwrap_or_default();
        let mut list = Vec::new();
        for e in text.lines().filter_map(entry_value) {
            if !list.contains(&e) {
                list.push(e);
            }
        }
        list.truncate(RECENTS_MAX);
        Recents { list }
    }

    pub fn write(&self, paths: &Paths) -> std::io::Result<()> {
        let text: String = self
            .list
            .iter()
            .map(|(p, s)| entry_key(*p, s) + "\n")
            .collect();
        write(&paths.data().join(RECENTS_FILE), &text)
    }

    /// slot's `played`: the game goes to the front; the eleventh drops off.
    pub fn played(&mut self, platform: Platform, stem: &str) {
        self.forget(platform, stem);
        self.list.insert(0, (platform, stem.to_string()));
        self.list.truncate(RECENTS_MAX);
    }

    /// slot's `forget`: take a game off the list.
    pub fn forget(&mut self, platform: Platform, stem: &str) {
        self.list.retain(|(p, s)| !(*p == platform && s == stem));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> (tempfile::TempDir, Paths) {
        let d = tempfile::tempdir().unwrap();
        let p = Paths::new(d.path().join("sd"), d.path().join("tmp"));
        (d, p)
    }

    #[test]
    fn settings_default_off_and_round_trip() {
        let (_d, p) = tmp();
        let s = Settings::read(&p);
        assert!(!s.sounds && !s.label_service);
        let s2 = Settings { sounds: true, ..s };
        s2.write(&p).unwrap();
        assert_eq!(Settings::read(&p), s2);
    }

    #[test]
    fn favourites_toggle_and_persist_in_slots_format() {
        let (_d, p) = tmp();
        let mut f = Favourites::read(&p);
        assert!(f.toggle(Platform::Gba, "Metroid Fusion"));
        assert!(f.toggle(Platform::Gb, "Tetris"));
        assert!(!f.toggle(Platform::Gba, "Metroid Fusion"));
        f.write(&p).unwrap();
        let text = std::fs::read_to_string(p.data().join(FAVOURITES_FILE)).unwrap();
        assert_eq!(text, "gb/Tetris\n");
        assert_eq!(Favourites::read(&p), f);
    }

    #[test]
    fn favourites_skip_junk_and_duplicates() {
        let (_d, p) = tmp();
        std::fs::create_dir_all(p.data()).unwrap();
        std::fs::write(
            p.data().join(FAVOURITES_FILE),
            "gba/A\nnonsense\nnes/B\ngba/\nGBA/A\ngbc/C\n",
        )
        .unwrap();
        let f = Favourites::read(&p);
        assert_eq!(
            f.list,
            vec![(Platform::Gba, "A".into()), (Platform::Gbc, "C".into())]
        );
    }

    #[test]
    fn recents_keep_the_last_ten_newest_first() {
        let (_d, p) = tmp();
        let mut r = Recents::read(&p);
        for i in 0..12 {
            r.played(Platform::Gba, &format!("Game {i}"));
        }
        r.played(Platform::Gb, "Tetris");
        r.played(Platform::Gba, "Game 8");
        assert_eq!(r.list.len(), RECENTS_MAX);
        assert_eq!(r.list[0], (Platform::Gba, "Game 8".to_string()));
        assert_eq!(r.list[1], (Platform::Gb, "Tetris".to_string()));
        assert_eq!(r.list[9], (Platform::Gba, "Game 3".to_string()));
        r.write(&p).unwrap();
        let text = std::fs::read_to_string(p.data().join(RECENTS_FILE)).unwrap();
        assert!(
            text.starts_with("gba/Game 8\ngb/Tetris\ngba/Game 11\n"),
            "{text}"
        );
        assert_eq!(Recents::read(&p), r);
        r.forget(Platform::Gb, "Tetris");
        assert_eq!(r.list.len(), RECENTS_MAX - 1);
        assert_eq!(r.list[1], (Platform::Gba, "Game 11".to_string()));
    }

    #[test]
    fn state_round_trips() {
        let (_d, p) = tmp();
        assert_eq!(State::read(&p), State::default());
        let s = State {
            shelf: Some("GBA".into()),
            cart: Some((Platform::Gba, "Golden Sun".into())),
            launched: true,
        };
        s.write(&p).unwrap();
        assert_eq!(State::read(&p), s);
    }
}

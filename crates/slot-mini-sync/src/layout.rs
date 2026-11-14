//! Where one side keeps each item (docs/SYNC.md, "Layouts"): the scan that
//! lists them, and the few operations that write them.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use slot_mini_store::saves::{self, sort_dir};
use slot_mini_store::{cart_shell, Core, Favourites, Platform, ShellChoice};

use crate::error::SyncError;
use crate::hash::{hash_bytes, hash_file, mtime_secs, HashCache};
use crate::key::{GameId, Key, Kind};
use crate::model::{Item, Manifest};
use crate::names::{fold, NameRules};
use crate::plan::TextEdits;
use crate::report::{Cancel, Scope};

/// Which core a game runs on, on the PC. Brief 2's core picker is one.
pub trait CoreChoice: Send + Sync {
    fn core(&self, platform: Platform, stem: &str) -> Core;
}

/// The same core for every game of a system.
#[derive(Clone, Copy, Debug)]
pub struct FixedCore {
    pub gba: Core,
    pub gb: Core,
}

impl Default for FixedCore {
    fn default() -> Self {
        FixedCore {
            gba: Core::Mgba,
            gb: Core::Gambatte,
        }
    }
}

impl CoreChoice for FixedCore {
    fn core(&self, platform: Platform, _stem: &str) -> Core {
        match platform {
            Platform::Gba | Platform::Gbav => self.gba,
            Platform::Gb | Platform::Gbc => self.gb,
        }
    }
}

#[derive(Clone)]
enum Cores {
    /// Onion's rules, read from the card.
    Device,
    Pc(Arc<dyn CoreChoice>),
}

/// One side's files.
#[derive(Clone)]
pub struct Layout {
    root: PathBuf,
    saves: PathBuf,
    states: PathBuf,
    sync_dir: PathBuf,
    favourites: PathBuf,
    rules: NameRules,
    cores: Cores,
    free: Option<u64>,
}

/// A game on this side: its spelling here and its first ROM file.
#[derive(Clone, Debug)]
pub(crate) struct Game {
    pub id: GameId,
    pub rom: PathBuf,
}

/// What a scan found.
pub(crate) struct Scan {
    pub manifest: Manifest,
    /// Folded key to the file that holds it.
    pub paths: HashMap<String, PathBuf>,
    /// Folded game to the game.
    pub games: HashMap<String, Game>,
    /// Files whose names fold to a key another file already has.
    pub duplicates: Vec<Key>,
}

const RESERVED_NAME: &str = "slot-mini-removed";

impl Layout {
    /// The Miyoo: the SD card at `sd`.
    pub fn device(sd: &Path) -> Layout {
        let data = sd.join("App/slot-mini/data");
        Layout {
            root: sd.to_path_buf(),
            saves: sd.join("Saves/CurrentProfile/saves"),
            states: sd.join("Saves/CurrentProfile/states"),
            sync_dir: data.join("sync"),
            favourites: data.join("favorites.txt"),
            rules: NameRules::Fat,
            cores: Cores::Device,
            free: None,
        }
    }

    /// The PC: the library folder, RetroArch's save and state folders
    /// (sorted by core), and the app's choice of core per game.
    pub fn pc(root: &Path, saves: &Path, states: &Path, cores: Arc<dyn CoreChoice>) -> Layout {
        let data = root.join("App/slot-mini-pc/data");
        Layout {
            root: root.to_path_buf(),
            saves: saves.to_path_buf(),
            states: states.to_path_buf(),
            sync_dir: data.join("sync"),
            favourites: data.join("favorites.txt"),
            rules: NameRules::local(),
            cores: Cores::Pc(cores),
            free: None,
        }
    }

    /// Report this much free space instead of asking the file system
    /// (tests).
    pub fn with_free_space(mut self, bytes: u64) -> Layout {
        self.free = Some(bytes);
        self
    }

    /// Treat names by other rules than the side's own (tests).
    pub fn with_name_rules(mut self, rules: NameRules) -> Layout {
        self.rules = rules;
        self
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Where sync keeps its own files.
    pub fn sync_dir(&self) -> &Path {
        &self.sync_dir
    }

    pub fn name_rules(&self) -> NameRules {
        self.rules
    }

    /// The folder saves are in, sorted by core.
    pub fn saves_dir(&self) -> &Path {
        &self.saves
    }

    pub fn favourites_file(&self) -> &Path {
        &self.favourites
    }

    pub fn shell_file(&self) -> PathBuf {
        self.root.join(cart_shell::LABELS_SHELL_FILE)
    }

    /// Where a removed game's saves go.
    pub fn removed_dir(&self) -> PathBuf {
        self.root.join("Saves").join(RESERVED_NAME)
    }

    fn roms_dir(&self, sys: Platform) -> PathBuf {
        self.root.join("Roms").join(sys.dir_name())
    }

    /// The core a game runs on here.
    pub fn core(&self, sys: Platform, stem: &str, rom: Option<&Path>) -> Core {
        match &self.cores {
            Cores::Device => saves::device_core(&self.root, sys, rom),
            Cores::Pc(choice) => {
                let core = choice.core(sys, stem);
                if core.runs(sys) {
                    core
                } else {
                    FixedCore::default().core(sys, stem)
                }
            }
        }
    }

    /// A game's save file (`ext` is `srm` or `rtc`).
    pub fn save_file(&self, sys: Platform, stem: &str, ext: &str, rom: Option<&Path>) -> PathBuf {
        self.saves
            .join(sort_dir(self.core(sys, stem, rom)))
            .join(format!("{stem}.{ext}"))
    }

    /// A game's auto state, which is set aside when a save arrives.
    pub fn state_file(&self, sys: Platform, stem: &str, rom: Option<&Path>) -> PathBuf {
        self.states
            .join(sort_dir(self.core(sys, stem, rom)))
            .join(format!("{stem}.state.auto"))
    }

    /// The free space sync may plan with.
    pub fn free_space(&self) -> Option<u64> {
        self.free.or_else(|| free_space(&self.root))
    }

    /// Remove temporary files other runs left where sync writes.
    pub fn sweep(&self) {
        for dir in [
            self.root.join("Roms"),
            self.root.join("Labels"),
            self.saves.clone(),
            self.sync_dir.clone(),
        ] {
            slot_mini_store::sweep_temps(&dir);
        }
    }

    /// The name the item's file has here, for the check that this side can
    /// store it.
    pub(crate) fn store_name(key: &Key) -> String {
        key.tree_path().unwrap_or_else(|| key.name.clone())
    }

    /// Where an item that is not here yet goes. Labels, art and saves take
    /// this side's spelling of their game when it has the game, because
    /// that is the name slot-mini and RetroArch look for.
    pub(crate) fn dest(&self, key: &Key, games: &HashMap<String, Game>) -> PathBuf {
        let game = games.get(&key.game().folded());
        let stem = game.map_or(key.stem(), |g| g.id.stem.as_str());
        let sys = key.sys.dir_name();
        match key.kind {
            Kind::Rom => self.roms_dir(key.sys).join(&key.name),
            Kind::Label => self
                .root
                .join("Labels")
                .join(sys)
                .join(format!("{stem}.png")),
            Kind::Art => self
                .roms_dir(key.sys)
                .join("Imgs")
                .join(format!("{stem}.png")),
            Kind::Save => {
                self.save_file(key.sys, stem, key.save_ext(), game.map(|g| g.rom.as_path()))
            }
        }
    }

    /// The auto state that belongs to a save item.
    pub(crate) fn state_for(&self, key: &Key, games: &HashMap<String, Game>) -> PathBuf {
        let game = games.get(&key.game().folded());
        let stem = game.map_or(key.stem(), |g| g.id.stem.as_str());
        self.state_file(key.sys, stem, game.map(|g| g.rom.as_path()))
    }

    /// The games whose ROM is here.
    pub(crate) fn games(&self) -> (HashMap<String, Game>, Vec<(Key, PathBuf)>) {
        let mut games = HashMap::new();
        let mut roms = Vec::new();
        for sys in Platform::ALL {
            let dir = self.roms_dir(sys);
            if !dir.is_dir() {
                continue;
            }
            for path in slot_mini_store::rom_files(&dir, sys) {
                let Some(rel) = relative(&dir, &path) else {
                    continue;
                };
                let key = Key::new(Kind::Rom, sys, rel);
                games.entry(key.game().folded()).or_insert_with(|| Game {
                    id: key.game(),
                    rom: path.clone(),
                });
                roms.push((key, path));
            }
        }
        (games, roms)
    }

    /// List this side's items for `scope`, hashing what the cache does not
    /// know. `progress` gets bytes done and bytes in all.
    pub(crate) fn scan(
        &self,
        scope: &Scope,
        cache: &mut HashCache,
        progress: &mut dyn FnMut(u64, u64),
        cancel: &Cancel,
    ) -> Result<Scan, SyncError> {
        let (games, roms) = self.games();
        let mut files: Vec<(Key, PathBuf)> = Vec::new();
        if *scope == Scope::Full {
            files.extend(roms);
        }
        for sys in Platform::ALL {
            let name = sys.dir_name();
            for (kind, dir) in [
                (Kind::Label, self.root.join("Labels").join(name)),
                (Kind::Art, self.roms_dir(sys).join("Imgs")),
            ] {
                for (stem, path) in pngs(&dir) {
                    files.push((Key::new(kind, sys, stem), path));
                }
            }
        }
        let mut ordered: Vec<&Game> = games.values().collect();
        ordered.sort_by_key(|g| g.id.folded());
        for game in ordered {
            for ext in ["srm", "rtc"] {
                let key = Key::new(Kind::Save, game.id.sys, format!("{}.{ext}", game.id.stem));
                let path = self.save_file(game.id.sys, &game.id.stem, ext, Some(&game.rom));
                files.push((key, path));
            }
        }
        // Saves whose game is not here: its ROM was deleted, or is about
        // to come back. The planner drops those whose game is nowhere.
        let mut dirs: Vec<PathBuf> = std::fs::read_dir(&self.saves)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        dirs.sort();
        // A game here has its save listed; what else carries its name is
        // a leftover from another core.
        let stems: HashSet<String> = games.values().map(|g| fold(&g.id.stem)).collect();
        for dir in dirs {
            for name in names_in(&dir) {
                let Some((stem, ext)) = name.rsplit_once('.') else {
                    continue;
                };
                if (ext != "srm" && ext != "rtc") || stems.contains(&fold(stem)) {
                    continue;
                }
                for sys in Platform::ALL {
                    if self.save_file(sys, stem, ext, None) == dir.join(&name) {
                        files.push((Key::new(Kind::Save, sys, name.clone()), dir.join(&name)));
                    }
                }
            }
        }
        files.retain(|(key, _)| scope.covers(key));

        // Sizes first, so that progress has a total.
        let mut found = Vec::new();
        let mut total = 0;
        for (key, path) in files {
            if let Ok(meta) = std::fs::metadata(&path) {
                if meta.is_file() {
                    total += meta.len();
                    found.push((key, path, meta));
                }
            }
        }
        let mut done = 0;
        progress(done, total);
        let mut manifest = Manifest::empty(self.rules);
        let mut paths = HashMap::new();
        let mut duplicates = Vec::new();
        let mut live = HashSet::new();
        for (key, path, meta) in found {
            cancel.check()?;
            let folded = key.folded();
            if paths.contains_key(&folded) {
                duplicates.push(key);
                done += meta.len();
                continue;
            }
            let item = if key.kind == Kind::Save {
                // Never cached, and an empty one is no save.
                let bytes = std::fs::read(&path)?;
                done += meta.len();
                if saves::is_empty_save(&bytes) {
                    progress(done, total);
                    continue;
                }
                Item {
                    key,
                    size: bytes.len() as u64,
                    hash: hash_bytes(&bytes),
                    played: Some(mtime_secs(&meta)),
                }
            } else {
                let name = relative(&self.root, &path)
                    .unwrap_or_else(|| path.to_string_lossy().into_owned());
                let hash = match cache.get(&name, &meta) {
                    Some(hash) => {
                        done += meta.len();
                        hash
                    }
                    None => {
                        let (hash, size) = hash_file(&path, &mut |n| {
                            done += n;
                            progress(done, total);
                        })?;
                        if size != meta.len() {
                            // Still being written; the next sync sees it.
                            continue;
                        }
                        cache.put(&name, &meta, &hash);
                        hash
                    }
                };
                live.insert(name);
                Item {
                    key,
                    size: meta.len(),
                    hash,
                    played: None,
                }
            };
            progress(done, total);
            paths.insert(folded, path);
            manifest.items.push(item);
        }
        if *scope == Scope::Full {
            cache.keep_only(&live);
        }
        manifest.games = games.values().map(|g| g.id.clone()).collect();
        manifest.games.sort_by_key(|g| g.folded());
        if scope.whole_library() {
            manifest.shells = self.shells();
            manifest.favourites = Favourites::read(&self.favourites)
                .list
                .into_iter()
                .map(|(sys, stem)| GameId::new(sys, stem))
                .collect();
        }
        manifest.now = crate::now_secs();
        manifest.free = self.free_space();
        Ok(Scan {
            manifest,
            paths,
            games,
            duplicates,
        })
    }

    /// The shell lines slot-mini can read, by stem, with the value as
    /// slot-mini would write it.
    pub(crate) fn shells(&self) -> Vec<(String, String)> {
        let text = std::fs::read_to_string(self.shell_file()).unwrap_or_default();
        let mut out: Vec<(String, String)> = slot_mini_store::ini::parse(&text)
            .into_iter()
            .filter_map(|(stem, value)| Some((stem, shell_value(&value)?)))
            .collect();
        out.sort();
        out
    }

    /// Apply the text edits a plan gave this side. Returns whether a file
    /// changed.
    pub(crate) fn apply_text(
        &self,
        edits: &TextEdits,
        games: &HashMap<String, Game>,
    ) -> std::io::Result<bool> {
        let mut changed = false;
        if !edits.shell_set.is_empty() || !edits.shell_remove.is_empty() {
            let path = self.shell_file();
            let old = match std::fs::read_to_string(&path) {
                Ok(text) => text,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
                Err(e) => return Err(e),
            };
            let mut text = old.clone();
            let spelled = |text: &str, stem: &str| -> Vec<String> {
                slot_mini_store::ini::parse(text)
                    .into_keys()
                    .filter(|k| fold(k) == fold(stem))
                    .collect::<std::collections::BTreeSet<_>>()
                    .into_iter()
                    .collect()
            };
            for stem in &edits.shell_remove {
                for line in spelled(&text, stem) {
                    text = cart_shell::set_line(&text, &line, None)?;
                }
            }
            for (stem, value) in &edits.shell_set {
                // This side's spelling: its game's, which is the name
                // slot-mini looks a line up by, else the line it has.
                let lines = spelled(&text, stem);
                let own = games
                    .values()
                    .find(|g| fold(&g.id.stem) == fold(stem))
                    .map(|g| g.id.stem.clone())
                    .or_else(|| lines.first().cloned())
                    .unwrap_or_else(|| stem.clone());
                for line in lines.iter().filter(|l| **l != own) {
                    text = cart_shell::set_line(&text, line, None)?;
                }
                text = cart_shell::set_line(&text, &own, Some(value))?;
            }
            if text != old {
                if let Some(dir) = path.parent() {
                    std::fs::create_dir_all(dir)?;
                }
                slot_mini_store::atomic_write(&path, text.as_bytes())?;
                changed = true;
            }
        }
        if !edits.fav_add.is_empty() || !edits.fav_remove.is_empty() {
            let mut favs = Favourites::read(&self.favourites);
            let old = favs.clone();
            let folded = |sys: Platform, stem: &str| GameId::new(sys, stem).folded();
            for game in &edits.fav_remove {
                favs.list
                    .retain(|(sys, stem)| folded(*sys, stem) != game.folded());
            }
            for game in &edits.fav_add {
                if favs
                    .list
                    .iter()
                    .any(|(sys, stem)| folded(*sys, stem) == game.folded())
                {
                    continue;
                }
                let own = games.get(&game.folded()).map_or(game, |g| &g.id);
                favs.list.push((own.sys, own.stem.clone()));
            }
            if favs != old {
                favs.write(&self.favourites)?;
                changed = true;
            }
        }
        Ok(changed)
    }

    /// Remove a game from this side (docs/SYNC.md, "Remove everywhere"):
    /// its ROM files, label, art, shell line and favourite go; its saves
    /// move to the removed-saves folder. Returns whether anything was here.
    /// The hub removes the shell line and favourite itself (`text`); the
    /// client gets those as text edits from the plan.
    pub(crate) fn remove_game(
        &self,
        game: &GameId,
        stamp: &str,
        text: bool,
    ) -> std::io::Result<bool> {
        let want = game.folded();
        let (games, roms) = self.games();
        let mut any = false;

        // The saves first, while the ROM still says which core they are in.
        if let Some(own) = games.get(&want) {
            let core = self.core(own.id.sys, &own.id.stem, Some(&own.rom));
            // A save file shared with the same stem on another system
            // (Gambatte holds GB and GBC) stays with that game.
            let shared = games.values().any(|g| {
                g.id.folded() != want
                    && fold(&g.id.stem) == fold(&own.id.stem)
                    && self.core(g.id.sys, &g.id.stem, Some(&g.rom)) == core
            });
            let dir = self.saves.join(sort_dir(core));
            if !shared {
                for name in names_in(&dir) {
                    let mine = ["srm", "rtc"].iter().any(|ext| {
                        let save = format!("{}.{ext}", own.id.stem);
                        fold(&name) == fold(&save)
                            || fold(&name).starts_with(&fold(&format!("{save}.conflict-")))
                    });
                    if mine {
                        let to = self.removed_dir().join(own.id.sys.dir_name());
                        std::fs::create_dir_all(&to)?;
                        let mut dst = to.join(format!("{name}.{stamp}"));
                        let mut n = 1;
                        while dst.exists() {
                            dst = to.join(format!("{name}.{stamp}_{n}"));
                            n += 1;
                        }
                        std::fs::rename(dir.join(&name), dst)?;
                        any = true;
                    }
                }
            }
        }
        for (key, path) in &roms {
            if key.game().folded() == want {
                std::fs::remove_file(path)?;
                any = true;
            }
        }
        let sys = game.sys.dir_name();
        for dir in [
            self.root.join("Labels").join(sys),
            self.roms_dir(game.sys).join("Imgs"),
        ] {
            for (stem, path) in pngs(&dir) {
                if fold(&stem) == fold(&game.stem) {
                    std::fs::remove_file(path)?;
                    any = true;
                }
            }
        }
        // The shell line is keyed by stem alone: it stays if another
        // system still has a game of that stem.
        let elsewhere = games
            .values()
            .any(|g| g.id.folded() != want && fold(&g.id.stem) == fold(&game.stem));
        let edits = TextEdits {
            shell_remove: if elsewhere {
                Vec::new()
            } else {
                vec![game.stem.clone()]
            },
            fav_remove: vec![game.clone()],
            ..TextEdits::default()
        };
        if text {
            any |= self.apply_text(&edits, &games)?;
        }
        Ok(any)
    }
}

/// A shell line's value as slot-mini writes it; `None` if it cannot read it.
pub(crate) fn shell_value(value: &str) -> Option<String> {
    if value.trim() == "auto" {
        return Some("auto".to_string());
    }
    ShellChoice::parse(value).map(|c| c.to_value())
}

/// `path` under `dir`, with `/` between its parts.
fn relative(dir: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(dir).ok()?;
    let parts: Option<Vec<&str>> = rel.iter().map(|p| p.to_str()).collect();
    Some(parts?.join("/"))
}

fn names_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| !n.starts_with('.'))
        .collect();
    names.sort();
    names
}

/// The `.png` files in a folder: stem and path, in name order.
fn pngs(dir: &Path) -> Vec<(String, PathBuf)> {
    names_in(dir)
        .into_iter()
        .filter_map(|name| {
            let (stem, ext) = name.rsplit_once('.')?;
            (ext.eq_ignore_ascii_case("png") && !stem.is_empty())
                .then(|| (stem.to_string(), dir.join(&name)))
        })
        .collect()
}

#[cfg(unix)]
fn free_space(path: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    // SAFETY: `c` is a valid C string and `s` is written by statvfs.
    unsafe {
        let mut s: libc::statvfs = std::mem::zeroed();
        (libc::statvfs(c.as_ptr(), &mut s) == 0).then(|| s.f_bavail as u64 * s.f_frsize as u64)
    }
}

/// Only the Miyoo's free space is planned with (docs/SYNC.md, "Space").
#[cfg(not(unix))]
fn free_space(_path: &Path) -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: PathBuf, bytes: &[u8]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    fn card() -> (tempfile::TempDir, Layout) {
        let d = tempfile::tempdir().unwrap();
        let sd = d.path();
        write(
            sd.join("Emu/GBA/launch.sh"),
            b"x .retroarch/cores/gpsp_libretro.so",
        );
        write(sd.join("Roms/GBA/Golden Sun.GBA"), b"rom one");
        write(sd.join("Roms/GBA/Hacks/Emerald.gba"), b"rom two");
        write(sd.join("Roms/GBA/Imgs/Golden Sun.png"), b"art");
        write(sd.join("Roms/GBA/.hidden.gba"), b"no");
        write(sd.join("Roms/GB/Tetris.gb"), b"rom three");
        write(sd.join("Labels/GBA/Golden Sun.png"), b"label");
        write(sd.join("Labels/GBA/notes.txt"), b"no");
        write(
            sd.join("Saves/CurrentProfile/saves/gpSP/Golden Sun.srm"),
            b"save",
        );
        write(
            sd.join("Saves/CurrentProfile/saves/gpSP/Emerald.srm"),
            &[0xff; 64],
        );
        write(
            sd.join("Saves/CurrentProfile/saves/mGBA/Golden Sun.srm"),
            b"other core",
        );
        write(
            sd.join("Saves/CurrentProfile/saves/gpSP/No Rom.srm"),
            b"orphan",
        );
        write(
            sd.join("Labels/cart_shell.txt"),
            b"Tetris = notched 2f5cc0 glitter\nGolden Sun = auto\nbroken line\nBad = nonsense\n",
        );
        write(sd.join("App/slot-mini/data/favorites.txt"), b"gb/Tetris\n");
        let layout = Layout::device(sd);
        (d, layout)
    }

    fn scan(layout: &Layout, scope: Scope) -> Scan {
        layout
            .scan(
                &scope,
                &mut HashCache::default(),
                &mut |_, _| {},
                &Cancel::new(),
            )
            .unwrap()
    }

    fn keys(scan: &Scan) -> Vec<String> {
        let mut keys: Vec<String> = scan
            .manifest
            .items
            .iter()
            .map(|i| i.key.to_string())
            .collect();
        keys.sort();
        keys
    }

    #[test]
    fn a_full_scan_lists_every_item_once() {
        let (_d, layout) = card();
        let s = scan(&layout, Scope::Full);
        assert_eq!(
            keys(&s),
            [
                "art:GBA/Golden Sun",
                "label:GBA/Golden Sun",
                "rom:GB/Tetris.gb",
                "rom:GBA/Golden Sun.GBA",
                "rom:GBA/Hacks/Emerald.gba",
                // gpSP is this card's GBA core; the empty save is not
                // listed, the save whose ROM is gone is.
                "save:GBA/Golden Sun.srm",
                "save:GBA/No Rom.srm",
            ]
        );
        let save = s
            .manifest
            .items
            .iter()
            .find(|i| i.key.to_string() == "save:GBA/Golden Sun.srm")
            .unwrap();
        assert_eq!(save.hash, hash_bytes(b"save"));
        assert!(save.played.is_some());
        assert_eq!(s.manifest.games.len(), 3);
        assert_eq!(
            s.manifest.shells,
            [
                ("Golden Sun".to_string(), "auto".to_string()),
                ("Tetris".to_string(), "notched 2f5cc0 glitter".to_string())
            ]
        );
        assert_eq!(s.manifest.favourites, [GameId::new(Platform::Gb, "Tetris")]);
        assert_eq!(s.manifest.names, NameRules::Fat);
    }

    #[test]
    fn narrower_scopes_list_less_but_always_the_games() {
        let (_d, layout) = card();
        let auto = scan(&layout, Scope::Auto);
        assert_eq!(
            keys(&auto),
            [
                "art:GBA/Golden Sun",
                "label:GBA/Golden Sun",
                "save:GBA/Golden Sun.srm",
                "save:GBA/No Rom.srm",
            ]
        );
        assert_eq!(auto.manifest.games.len(), 3);
        let game = scan(&layout, Scope::Game(GameId::new(Platform::Gb, "tetris")));
        assert!(keys(&game).is_empty());
        assert!(game.manifest.shells.is_empty() && game.manifest.favourites.is_empty());
        assert_eq!(game.manifest.games.len(), 3);
    }

    #[test]
    fn a_per_game_core_moves_where_the_save_is() {
        let (d, layout) = card();
        write(
            d.path().join("Roms/GBA/.game_config/Golden Sun.GBA.cfg"),
            b"core = \"mgba_libretro\"\n",
        );
        // Onion cuts the lower-cased extension, so `.GBA` stays in the name.
        let s = scan(&layout, Scope::Auto);
        let save = s
            .manifest
            .items
            .iter()
            .find(|i| i.key.kind == Kind::Save)
            .unwrap();
        assert_eq!(save.hash, hash_bytes(b"other core"));
    }

    #[test]
    fn new_items_take_this_sides_spelling_of_the_game() {
        let (d, layout) = card();
        let (games, _) = layout.games();
        let sd = d.path();
        let dest = |k: &str| layout.dest(&Key::parse(k).unwrap(), &games);
        assert_eq!(
            dest("rom:GBA/New/Game.gba"),
            sd.join("Roms/GBA/New/Game.gba")
        );
        assert_eq!(
            dest("label:GBA/GOLDEN SUN"),
            sd.join("Labels/GBA/Golden Sun.png")
        );
        assert_eq!(dest("art:GB/New"), sd.join("Roms/GB/Imgs/New.png"));
        assert_eq!(
            dest("save:GBA/golden sun.rtc"),
            sd.join("Saves/CurrentProfile/saves/gpSP/Golden Sun.rtc")
        );
        assert_eq!(
            layout.state_for(&Key::parse("save:GBA/golden sun.srm").unwrap(), &games),
            sd.join("Saves/CurrentProfile/states/gpSP/Golden Sun.state.auto")
        );
    }

    #[test]
    fn text_edits_keep_what_they_do_not_touch() {
        let (d, layout) = card();
        let (games, _) = layout.games();
        let edits = TextEdits {
            shell_set: vec![("golden sun".into(), "notched ff0000 clear".into())],
            shell_remove: vec!["TETRIS".into()],
            fav_add: vec![GameId::new(Platform::Gba, "GOLDEN SUN")],
            fav_remove: vec![GameId::new(Platform::Gb, "tetris")],
        };
        assert!(layout.apply_text(&edits, &games).unwrap());
        assert!(!layout.apply_text(&edits, &games).unwrap());
        let text = std::fs::read_to_string(d.path().join("Labels/cart_shell.txt")).unwrap();
        assert!(text.contains("Golden Sun = notched ff0000 clear"), "{text}");
        assert!(text.contains("broken line") && text.contains("Bad = nonsense"));
        assert!(!text.contains("Tetris"));
        assert_eq!(
            std::fs::read_to_string(layout.favourites_file()).unwrap(),
            "gba/Golden Sun\n"
        );
    }

    #[test]
    fn removing_a_game_keeps_its_saves_aside() {
        let (d, layout) = card();
        let sd = d.path();
        write(
            sd.join("Saves/CurrentProfile/saves/gpSP/Golden Sun.srm.conflict-miyoo-1"),
            b"copy",
        );
        assert!(layout
            .remove_game(&GameId::new(Platform::Gba, "golden sun"), "S", true)
            .unwrap());
        assert!(!sd.join("Roms/GBA/Golden Sun.GBA").exists());
        assert!(!sd.join("Labels/GBA/Golden Sun.png").exists());
        assert!(!sd.join("Roms/GBA/Imgs/Golden Sun.png").exists());
        let removed = sd.join("Saves/slot-mini-removed/GBA");
        assert_eq!(
            std::fs::read(removed.join("Golden Sun.srm.S")).unwrap(),
            b"save"
        );
        assert!(removed.join("Golden Sun.srm.conflict-miyoo-1.S").exists());
        // Another core's save, other games and their text stay.
        assert!(sd
            .join("Saves/CurrentProfile/saves/mGBA/Golden Sun.srm")
            .exists());
        assert!(sd.join("Roms/GBA/Hacks/Emerald.gba").exists());
        let text = std::fs::read_to_string(sd.join("Labels/cart_shell.txt")).unwrap();
        assert!(!text.contains("Golden Sun") && text.contains("Tetris"));
        assert!(!layout
            .remove_game(&GameId::new(Platform::Gba, "golden sun"), "S", true)
            .unwrap());
    }

    #[test]
    fn the_cache_spares_a_second_hash() {
        let (d, layout) = card();
        let rom = d.path().join("Roms/GB/Tetris.gb");
        let old = std::time::SystemTime::now() - std::time::Duration::from_secs(60);
        std::fs::File::options()
            .write(true)
            .open(&rom)
            .unwrap()
            .set_modified(old)
            .unwrap();
        let mut cache = HashCache::default();
        let cancel = Cancel::new();
        layout
            .scan(&Scope::Full, &mut cache, &mut |_, _| {}, &cancel)
            .unwrap();
        // Same size and time, other bytes: only a cache would not notice.
        std::fs::write(&rom, b"ROM THREE").unwrap();
        std::fs::File::options()
            .write(true)
            .open(&rom)
            .unwrap()
            .set_modified(old)
            .unwrap();
        let s = layout
            .scan(&Scope::Full, &mut cache, &mut |_, _| {}, &cancel)
            .unwrap();
        let item = s
            .manifest
            .items
            .iter()
            .find(|i| i.key.name == "Tetris.gb")
            .unwrap();
        assert_eq!(item.hash, hash_bytes(b"rom three"));
        cancel.cancel();
        assert!(layout
            .scan(&Scope::Full, &mut cache, &mut |_, _| {}, &cancel)
            .is_err());
    }
}

//! MENU > Import from SD card: copy an Onion card's games into the library.
//!
//! ROMs, their labels, their box art (`Roms/<system>/Imgs`), cart shell
//! choices, favourites, and each game's save, which goes into the folder of
//! the core the PC runs that game on. Save states are left behind: they are
//! tied to a core's build. Nothing in the library is ever replaced; what was
//! already there is counted and listed in `import.txt` in the app's folder.

use std::path::{Path, PathBuf};

use slot_mini::{cores, Paths};
use slot_mini_store::{cart_shell, ini, Cart, Core};

/// What an import did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub games: usize,
    pub labels: usize,
    pub art: usize,
    pub saves: usize,
    pub shells: usize,
    pub favourites: usize,
    /// Files left alone because the library already had them.
    pub skipped: Vec<String>,
    /// Files that could not be copied, and why.
    pub failed: Vec<String>,
}

impl Report {
    /// One line for the screen.
    pub fn summary(&self) -> String {
        let n = |count: usize, one: &str| match count {
            1 => format!("1 {one}"),
            _ => format!("{count} {one}s"),
        };
        let mut s = format!(
            "Imported {} and {}.",
            n(self.games, "game"),
            n(self.saves, "save")
        );
        if !self.skipped.is_empty() {
            s.push_str(&format!(" {} already here.", self.skipped.len()));
        }
        if !self.failed.is_empty() {
            s.push_str(&format!(" {} failed: see import.txt.", self.failed.len()));
        }
        s
    }

    /// The whole story, for `import.txt`.
    pub fn log(&self, card: &Path) -> String {
        let mut s = format!("Import from {}\n\n", card.display());
        for (what, count) in [
            ("games", self.games),
            ("labels", self.labels),
            ("box art", self.art),
            ("saves", self.saves),
            ("cart shells", self.shells),
            ("favourites", self.favourites),
        ] {
            s.push_str(&format!("{count} {what} copied\n"));
        }
        for (title, list) in [
            ("Already in the library (left as they were)", &self.skipped),
            ("Could not be copied", &self.failed),
        ] {
            if !list.is_empty() {
                s.push_str(&format!("\n{title}:\n"));
                for line in list {
                    s.push_str(&format!("  {line}\n"));
                }
            }
        }
        s
    }
}

/// Why a folder cannot be imported from.
pub fn refuse(card: &Path, library: &Path) -> Option<&'static str> {
    let same = |a: &Path, b: &Path| match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    };
    if same(card, library) {
        return Some("That is the library itself.");
    }
    if !card.join("Roms").is_dir() {
        return Some("No Roms folder there: choose the card's top folder.");
    }
    None
}

enum Copied {
    Yes,
    AlreadyThere,
    Failed(String),
}

/// Copy `from` to `to` unless `to` exists. The copy lands under another
/// name first, so a card pulled half way leaves nothing that looks whole.
fn copy_new(from: &Path, to: &Path) -> Copied {
    if to.exists() {
        return Copied::AlreadyThere;
    }
    let part = {
        let mut name = to.file_name().unwrap_or_default().to_os_string();
        name.push(".importing");
        to.with_file_name(name)
    };
    let copy = || -> std::io::Result<()> {
        if let Some(dir) = to.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::copy(from, &part)?;
        std::fs::rename(&part, to)
    };
    match copy() {
        Ok(()) => Copied::Yes,
        Err(e) => {
            let _ = std::fs::remove_file(&part);
            Copied::Failed(e.to_string())
        }
    }
}

/// The save the card holds for `cart`: the core the Miyoo runs it on
/// first, then the other cores for its system.
fn card_save(card: &Paths, cart: &Cart) -> Option<PathBuf> {
    let first = cores::chosen(card, cart);
    std::iter::once(first)
        .chain(Core::ALL.into_iter().filter(|c| *c != first))
        .filter(|c| c.runs(cart.platform))
        .filter_map(|c| cores::save_path(card, cart, c))
        .find(|p| p.is_file())
}

/// Copy everything from the card at `card` into `library` (the PC app's
/// paths). `progress` hears how many of the games are done.
pub fn run(card: &Path, library: &Paths, mut progress: impl FnMut(usize, usize)) -> Report {
    let mut report = Report::default();
    let card_paths = Paths::new(card, std::env::temp_dir());
    let carts = match slot_mini_store::scan(card) {
        Ok(carts) => carts,
        Err(e) => {
            report.failed.push(format!("{}: {e}", card.display()));
            return report;
        }
    };
    let rel = |p: &Path| p.strip_prefix(card).map(Path::to_path_buf).ok();
    let take = |from: &Path, to: &Path, count: &mut usize, report: &mut Report| {
        let name = rel(from).unwrap_or_else(|| from.to_path_buf());
        match copy_new(from, to) {
            Copied::Yes => *count += 1,
            Copied::AlreadyThere => report.skipped.push(name.display().to_string()),
            Copied::Failed(e) => report.failed.push(format!("{}: {e}", name.display())),
        }
    };
    let mut shells = Vec::new();
    for (i, cart) in carts.iter().enumerate() {
        progress(i, carts.len());
        let Some(rom) = rel(&cart.rom) else {
            continue;
        };
        let (mut games, mut labels, mut art, mut saves) = (0, 0, 0, 0);
        let dest = library.sd.join(&rom);
        take(&cart.rom, &dest, &mut games, &mut report);
        if let Some(label) = cart.label.as_deref() {
            if let Some(to) = rel(label) {
                take(label, &library.sd.join(to), &mut labels, &mut report);
            }
        }
        let img = Path::new("Roms")
            .join(cart.platform.dir_name())
            .join("Imgs")
            .join(format!("{}.png", cart.stem));
        if card.join(&img).is_file() {
            take(
                &card.join(&img),
                &library.sd.join(&img),
                &mut art,
                &mut report,
            );
        }
        if let Some(save) = card_save(&card_paths, cart) {
            // The core this PC runs the game on: its own choice for the
            // game if the library has one, else the system's default.
            let here = Cart {
                rom: dest,
                ..cart.clone()
            };
            let core = cores::chosen(library, &here);
            if let Some(to) = cores::save_path(library, &here, core) {
                take(&save, &to, &mut saves, &mut report);
            }
        }
        report.games += games;
        report.labels += labels;
        report.art += art;
        report.saves += saves;
        if let Some(choice) = cart.shell {
            shells.push((cart.stem.clone(), choice));
        }
    }
    progress(carts.len(), carts.len());
    match merge_shells(&library.sd, &shells) {
        Ok(n) => report.shells = n,
        Err(e) => report.failed.push(format!("cart_shell.txt: {e}")),
    }
    let from = card
        .join(slot_mini::paths::APP_DIR)
        .join("data")
        .join(slot_mini::settings::FAVOURITES_FILE);
    let to = library.data().join(slot_mini::settings::FAVOURITES_FILE);
    match merge_lines(&from, &to) {
        Ok(n) => report.favourites = n,
        Err(e) => report.failed.push(format!("favorites.txt: {e}")),
    }
    let log = library.app().join("import.txt");
    if let Err(e) =
        std::fs::create_dir_all(library.app()).and_then(|()| std::fs::write(&log, report.log(card)))
    {
        eprintln!("slot-mini-pc: {}: {e}", log.display());
    }
    report
}

/// Add the card's shell choices for carts the library has none for.
fn merge_shells(
    library: &Path,
    shells: &[(String, slot_mini_store::ShellChoice)],
) -> std::io::Result<usize> {
    let path = library.join(slot_mini_store::LABELS_SHELL_FILE);
    let mut text = std::fs::read_to_string(&path).unwrap_or_default();
    let have: Vec<String> = ini::parse(&text)
        .into_keys()
        .map(|k| cart_shell::key(&k))
        .collect();
    let mut added = 0;
    for (stem, choice) in shells {
        if have.contains(&cart_shell::key(stem)) {
            continue;
        }
        text = ini::set(&text, stem, Some(&choice.to_value()))?;
        added += 1;
    }
    if added > 0 {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        slot_mini_store::atomic_write(&path, text.as_bytes())?;
    }
    Ok(added)
}

/// Append the lines of `from` that `to` does not have.
fn merge_lines(from: &Path, to: &Path) -> std::io::Result<usize> {
    let Ok(theirs) = std::fs::read_to_string(from) else {
        return Ok(0);
    };
    let mut text = std::fs::read_to_string(to).unwrap_or_default();
    let mut added = 0;
    for line in theirs.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if text.lines().any(|l| l.trim() == line) {
            continue;
        }
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(line);
        text.push('\n');
        added += 1;
    }
    if added > 0 {
        if let Some(dir) = to.parent() {
            std::fs::create_dir_all(dir)?;
        }
        slot_mini_store::atomic_write(to, text.as_bytes())?;
    }
    Ok(added)
}

#[cfg(test)]
mod tests {
    use super::*;
    use slot_mini::paths::Pc;

    fn write(path: PathBuf, bytes: &[u8]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    /// An Onion card with labels, a save per system, art, a state, a shell
    /// line and a favourite; and an empty library.
    fn rig() -> (tempfile::TempDir, PathBuf, Paths) {
        slot_mini_store::set_app_dir(crate::APP_DIR);
        let dir = tempfile::tempdir().unwrap();
        let card = dir.path().join("card");
        slot_mini::fixture::build(&card, true).unwrap();
        let saves = card.join("Saves/CurrentProfile");
        write(saves.join("saves/mGBA/Golden Sun (USA, Europe).srm"), b"gs");
        write(saves.join("saves/Gambatte/Tetris (World).srm"), b"tetris");
        write(
            saves.join("states/mGBA/Golden Sun (USA, Europe).state.auto"),
            b"st",
        );
        write(
            card.join("Roms/GBA/Imgs/Golden Sun (USA, Europe).png"),
            b"png",
        );
        write(
            card.join("App/slot-mini/data/favorites.txt"),
            b"gba/Golden Sun (USA, Europe)\n",
        );
        let library = Paths::pc(
            dir.path().join("library"),
            dir.path().join("tmp"),
            Pc { cores: None },
        );
        (dir, card, library)
    }

    fn files(root: &Path) -> Vec<String> {
        let mut out = Vec::new();
        let mut dirs = vec![root.to_path_buf()];
        while let Some(d) = dirs.pop() {
            for e in std::fs::read_dir(d).unwrap().flatten() {
                match e.path().is_dir() {
                    true => dirs.push(e.path()),
                    false => out.push(
                        e.path()
                            .strip_prefix(root)
                            .unwrap()
                            .display()
                            .to_string()
                            .replace('\\', "/"),
                    ),
                }
            }
        }
        out.sort();
        out
    }

    #[test]
    fn a_card_comes_across_whole_and_states_stay_behind() {
        let (_dir, card, library) = rig();
        let games = slot_mini_store::scan(&card).unwrap();
        assert!(games.len() >= 4, "the fixture has games");
        let mut steps = Vec::new();
        let report = run(&card, &library, |done, of| steps.push((done, of)));
        assert_eq!(report.games, games.len());
        assert_eq!(report.saves, 2);
        assert_eq!(report.art, 1);
        assert_eq!(report.favourites, 1);
        assert!(report.labels > 0 && report.shells > 0, "{report:?}");
        assert_eq!((report.skipped.len(), report.failed.len()), (0, 0));
        assert_eq!(steps.first(), Some(&(0, games.len())));
        assert_eq!(steps.last(), Some(&(games.len(), games.len())));
        // The library now scans to the same carts, shells and labels.
        let here = slot_mini_store::scan(&library.sd).unwrap();
        let key = |c: &Cart| (c.platform, c.stem.clone(), c.shell, c.label.is_some());
        assert_eq!(
            here.iter().map(key).collect::<Vec<_>>(),
            games.iter().map(key).collect::<Vec<_>>()
        );
        let all = files(&library.sd);
        assert!(
            all.contains(&"Saves/CurrentProfile/saves/mGBA/Golden Sun (USA, Europe).srm".into())
        );
        assert!(all.contains(&"Saves/CurrentProfile/saves/Gambatte/Tetris (World).srm".into()));
        assert!(all.contains(&"Roms/GBA/Imgs/Golden Sun (USA, Europe).png".into()));
        assert!(!all.iter().any(|f| f.contains("states")), "{all:?}");
        assert!(!all.iter().any(|f| f.ends_with(".importing")));
        assert!(!all.iter().any(|f| f.starts_with("Emu/")), "only games");
        let favs =
            slot_mini::settings::Favourites::read(&slot_mini::settings::favourites_file(&library));
        assert_eq!(favs.list.len(), 1);
        let log = std::fs::read_to_string(library.app().join("import.txt")).unwrap();
        assert!(log.contains("2 saves copied"), "{log}");
    }

    #[test]
    fn nothing_in_the_library_is_replaced_and_what_was_there_is_reported() {
        let (_dir, card, library) = rig();
        let rom = "Roms/GBA/Golden Sun (USA, Europe).gba";
        let save = "Saves/CurrentProfile/saves/mGBA/Golden Sun (USA, Europe).srm";
        write(library.sd.join(rom), b"mine");
        write(library.sd.join(save), b"my save");
        write(library.data().join("favorites.txt"), b"gb/Tetris (World)\n");
        let first = run(&card, &library, |_, _| {});
        assert_eq!(std::fs::read(library.sd.join(rom)).unwrap(), b"mine");
        assert_eq!(std::fs::read(library.sd.join(save)).unwrap(), b"my save");
        assert_eq!(first.skipped.len(), 2, "{:?}", first.skipped);
        assert!(first
            .skipped
            .contains(&rom.replace('/', std::path::MAIN_SEPARATOR_STR)));
        assert_eq!(first.saves, 1);
        assert!(
            first.summary().contains("2 already here"),
            "{}",
            first.summary()
        );
        let favs = std::fs::read_to_string(library.data().join("favorites.txt")).unwrap();
        assert_eq!(favs, "gb/Tetris (World)\ngba/Golden Sun (USA, Europe)\n");
        // Again: everything is already there, and nothing is added twice.
        let again = run(&card, &library, |_, _| {});
        assert_eq!(
            (again.games, again.saves, again.shells, again.favourites),
            (0, 0, 0, 0)
        );
        assert!(again.failed.is_empty());
        let log = std::fs::read_to_string(library.app().join("import.txt")).unwrap();
        assert!(log.contains("Already in the library"), "{log}");
    }

    #[test]
    fn a_save_follows_the_game_to_the_pcs_core() {
        let (_dir, card, library) = rig();
        // The Miyoo ran this game on gpSP; the PC runs GBA on mGBA.
        let cart = slot_mini_store::scan(&card)
            .unwrap()
            .into_iter()
            .find(|c| c.stem == "Golden Sun (USA, Europe)")
            .unwrap();
        let card_paths = Paths::new(&card, card.join("tmp"));
        std::fs::remove_file(cores::save_path(&card_paths, &cart, Core::Mgba).unwrap()).unwrap();
        cores::choose(&card_paths, &cart, Core::Gpsp).unwrap();
        write(
            cores::save_path(&card_paths, &cart, Core::Gpsp).unwrap(),
            b"gpsp save",
        );
        let report = run(&card, &library, |_, _| {});
        assert!(report.failed.is_empty(), "{report:?}");
        let to = library
            .sd
            .join("Saves/CurrentProfile/saves/mGBA/Golden Sun (USA, Europe).srm");
        assert_eq!(std::fs::read(to).unwrap(), b"gpsp save");
        assert!(!library.sd.join("Saves/CurrentProfile/saves/gpSP").exists());
    }

    #[test]
    fn a_folder_that_is_not_a_card_is_refused() {
        let (dir, card, library) = rig();
        assert_eq!(refuse(&card, &library.sd), None);
        assert!(refuse(dir.path(), &library.sd).unwrap().contains("Roms"));
        std::fs::create_dir_all(library.sd.join("Roms")).unwrap();
        assert!(refuse(&library.sd, &library.sd)
            .unwrap()
            .contains("library"));
    }
}

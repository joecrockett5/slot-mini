//! Property tests of the engine: random libraries, edits and syncs on
//! both sides, some over a connection that drops, checked for what
//! docs/SYNC.md promises.
//!
//! The generator is a seeded xorshift rather than a property-testing
//! crate: the cases are reproducible from the seed in the failure
//! message, and the device build gains no dependency.

mod support;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use slot_mini_store::{cart_shell, rom_files, Favourites, Platform};
use slot_mini_sync::{GameId, Key, Kind, Layout, Report, Scope, Side};
use support::{age, kept_saves, library, write, World};

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn pick<'a, T>(&mut self, list: &'a [T]) -> &'a T {
        &list[self.below(list.len())]
    }
}

/// Stems a GB and a GBC game never share: Gambatte keeps one save file
/// for both (docs/SYNC.md, "Limits").
const STEMS: &[(Platform, &str)] = &[
    (Platform::Gba, "Alpha"),
    (Platform::Gba, "Beta (USA)"),
    (Platform::Gba, "Gamma - The Sequel"),
    (Platform::Gb, "Delta"),
    (Platform::Gb, "Tetris (World)"),
    (Platform::Gbc, "Epsilon"),
    (Platform::Gbc, "Zeta (Europe)"),
];

const SHELLS: &[&str] = &[
    "notched 2f5cc0 glitter",
    "auto 3a7d44 clear",
    "auto ff0000 solid",
];

/// A side's games: system, stem as spelled there, and the ROM.
fn games(layout: &Layout) -> Vec<(Platform, String, PathBuf)> {
    let mut out = Vec::new();
    for sys in [Platform::Gba, Platform::Gb, Platform::Gbc] {
        let dir = layout.root().join("Roms").join(sys.dir_name());
        if !dir.is_dir() {
            continue;
        }
        for rom in rom_files(&dir, sys) {
            let stem = rom.file_stem().unwrap().to_string_lossy().into_owned();
            out.push((sys, stem, rom));
        }
    }
    out.sort_by(|a, b| a.2.cmp(&b.2));
    out
}

fn game_of(layout: &Layout, id: &GameId) -> Option<(Platform, String, PathBuf)> {
    games(layout)
        .into_iter()
        .find(|(sys, stem, _)| GameId::new(*sys, stem.clone()).folded() == id.folded())
}

/// The file in `dir` that FAT would take `name` for, else `name` there.
fn spelled(dir: &std::path::Path, name: &str) -> PathBuf {
    let here = std::fs::read_dir(dir).into_iter().flatten().flatten();
    here.map(|e| e.path())
        .find(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().to_lowercase() == name.to_lowercase())
        })
        .unwrap_or_else(|| dir.join(name))
}

/// One random edit a player or a tool might make to a library.
fn edit(rng: &mut Rng, layout: &Layout) {
    let &(sys, stem) = rng.pick(STEMS);
    let id = GameId::new(sys, stem);
    let dir = sys.dir_name();
    let root = layout.root();
    match rng.below(12) {
        0..=2 => {
            // A ROM, unless the game is here under some spelling.
            if game_of(layout, &id).is_none() {
                let ext = match sys {
                    Platform::Gba => "gba",
                    Platform::Gb => "gb",
                    _ => "gbc",
                };
                // Each side may file a game its own way. The stem keeps
                // its case: the test's card is not FAT, where two
                // spellings of a save are one file (the scenarios and the
                // planner's own tests cover case).
                let name = match rng.below(4) {
                    0 => format!("{stem}.{}", ext.to_uppercase()),
                    1 => format!("Hacks/{stem}.{ext}"),
                    _ => format!("{stem}.{ext}"),
                };
                let fill = rng.below(2) as u8;
                write(
                    &root.join("Roms").join(dir).join(name),
                    &vec![fill + 1; 3000 + 4000 * fill as usize],
                );
            }
        }
        3 => {
            if let Some((_, _, rom)) = game_of(layout, &id) {
                std::fs::remove_file(rom).unwrap();
            }
        }
        4..=6 => {
            // Play: a save and an auto state, some time ago.
            if let Some((sys, stem, rom)) = game_of(layout, &id) {
                let fill = rng.below(5) as u8;
                let bytes = if fill == 0 {
                    vec![0xff; 32]
                } else {
                    support::save(fill)
                };
                let save = layout.save_file(sys, &stem, "srm", Some(&rom));
                write(&save, &bytes);
                age(&save, 60 * rng.below(600) as u64);
                write(&layout.state_file(sys, &stem, Some(&rom)), b"state");
                if rng.below(4) == 0 {
                    write(&layout.save_file(sys, &stem, "rtc", Some(&rom)), &[fill, 7]);
                }
            }
        }
        7 | 8 => {
            let folder = if rng.below(2) == 0 {
                root.join("Labels").join(dir)
            } else {
                root.join("Roms").join(dir).join("Imgs")
            };
            let file = spelled(&folder, &format!("{stem}.png"));
            if rng.below(5) == 0 {
                let _ = std::fs::remove_file(file);
            } else {
                write(&file, &[rng.below(3) as u8; 100]);
            }
        }
        9 | 10 => {
            // slot-mini writes a line under the game's name as it is here.
            let here = game_of(layout, &id).map(|(_, stem, _)| stem);
            let stem = here.as_deref().unwrap_or(stem);
            let file = layout.shell_file();
            let text = std::fs::read_to_string(&file).unwrap_or_default();
            let other: Vec<String> = slot_mini_store::ini::parse(&text)
                .into_keys()
                .filter(|k| k.to_lowercase() == stem.to_lowercase() && k != stem)
                .collect();
            let text = other.iter().fold(text, |text, line| {
                cart_shell::set_line(&text, line, None).unwrap()
            });
            let value = (rng.below(4) != 0).then(|| *rng.pick(SHELLS));
            let text = cart_shell::set_line(&text, stem, value).unwrap();
            write(&file, text.as_bytes());
        }
        _ => {
            let file = layout.favourites_file();
            let mut favourites = Favourites::read(file);
            let here = favourites
                .list
                .iter()
                .find(|(s, name)| GameId::new(*s, name.clone()).folded() == id.folded())
                .map(|(_, name)| name.clone());
            favourites.toggle(sys, here.as_deref().unwrap_or(stem));
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            favourites.write(file).unwrap();
        }
    }
}

/// A save item's file on one side, and its game's auto state.
fn save_and_state(layout: &Layout, key: &str) -> Option<(PathBuf, PathBuf)> {
    let key = Key::parse(key)?;
    if key.kind != Kind::Save {
        return None;
    }
    let ext = if key.name.ends_with(".rtc") {
        "rtc"
    } else {
        "srm"
    };
    let id = key.game();
    Some(match game_of(layout, &id) {
        Some((sys, stem, rom)) => (
            layout.save_file(sys, &stem, ext, Some(&rom)),
            layout.state_file(sys, &stem, Some(&rom)),
        ),
        None => (
            layout.save_file(id.sys, &id.stem, ext, None),
            layout.state_file(id.sys, &id.stem, None),
        ),
    })
}

/// A sync, checked for the two things that must hold however it ends.
fn sync(w: &mut World, scope: Scope, cut: Option<u64>, what: &str) -> Report {
    let before = (kept_saves(w.device()), kept_saves(w.desk()));
    let files = [
        support::tree(w.device().saves_dir()),
        support::tree(w.desk().saves_dir()),
    ];
    let report = match cut {
        Some(budget) => w.sync_cut(scope, budget),
        None => w.sync(scope),
    };
    let after = (kept_saves(w.device()), kept_saves(w.desk()));
    assert!(
        before.0.is_subset(&after.0),
        "{what}: the Miyoo lost a save: {report:?}"
    );
    assert!(
        before.1.is_subset(&after.1),
        "{what}: the PC lost a save: {report:?}"
    );
    // Wherever a newer save landed, the auto state is out of the way.
    let landed = |moved: &slot_mini_sync::Moved, loser: Side| -> Vec<String> {
        moved
            .added
            .iter()
            .chain(&moved.updated)
            .cloned()
            .chain(
                report
                    .conflicts
                    .iter()
                    .filter(|(_, winner)| *winner != loser)
                    .map(|(key, _)| key.clone()),
            )
            .collect()
    };
    for (layout, was, keys) in [
        (
            w.device(),
            &files[0],
            landed(&report.to_client, Side::Client),
        ),
        (w.desk(), &files[1], landed(&report.to_hub, Side::Hub)),
    ] {
        for key in keys {
            let Some((save, state)) = save_and_state(layout, &key) else {
                continue;
            };
            // A save that was already there byte for byte did not land.
            let name = save.strip_prefix(layout.saves_dir()).unwrap();
            let same = was.get(&*name.to_string_lossy()) == std::fs::read(&save).ok().as_ref();
            assert!(
                same || !state.exists(),
                "{what}: {key} landed beside its auto state {} {report:?}",
                state.display()
            );
        }
    }
    report
}

/// What the two sides must agree on, whatever each calls its files.
fn holdings(layout: &Layout) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut put = |name: String, bytes: Vec<u8>| {
        out.insert(name.to_lowercase(), slot_mini_sync::hash_bytes(&bytes));
    };
    let root = layout.root();
    for (name, bytes) in support::tree(&root.join("Roms")) {
        put(format!("roms/{name}"), bytes);
    }
    for (name, bytes) in support::tree(&root.join("Labels")) {
        if name.ends_with(".png") {
            put(format!("labels/{name}"), bytes);
        }
    }
    for (sys, stem, rom) in games(layout) {
        for ext in ["srm", "rtc"] {
            // As FAT would find it, whatever case it was written in.
            let save = layout.save_file(sys, &stem, ext, Some(&rom));
            let name = save.file_name().unwrap().to_string_lossy().into_owned();
            if let Ok(bytes) = std::fs::read(spelled(save.parent().unwrap(), &name)) {
                if !slot_mini_store::saves::is_empty_save(&bytes) {
                    put(format!("save/{}/{stem}.{ext}", sys.dir_name()), bytes);
                }
            }
        }
    }
    let shells = std::fs::read_to_string(layout.shell_file()).unwrap_or_default();
    for (stem, choice) in cart_shell::choices(&shells) {
        put(format!("shell/{stem}"), choice.to_value().into_bytes());
    }
    let favourites: BTreeSet<String> = Favourites::read(layout.favourites_file())
        .list
        .iter()
        .map(|(sys, stem)| format!("fav/{}/{stem}", sys.dir_name()))
        .collect();
    for f in favourites {
        put(f, Vec::new());
    }
    out
}

fn case(seed: u64) {
    let mut rng = Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
    let mut w = World::new();
    let steps = 15 + rng.below(40);
    for step in 0..steps {
        let what = format!("seed {seed} step {step}");
        match rng.below(10) {
            0..=3 => edit(&mut rng, w.device()),
            4..=6 => edit(&mut rng, w.desk()),
            7 => {
                let &(sys, stem) = rng.pick(STEMS);
                let before = kept_saves(w.desk());
                w.hub.remove_everywhere(&GameId::new(sys, stem)).unwrap();
                assert!(
                    before.is_subset(&kept_saves(w.desk())),
                    "{what}: Remove everywhere lost a save"
                );
            }
            _ => {
                let scope = match rng.below(4) {
                    0 => Scope::Full,
                    1 => {
                        let &(sys, stem) = rng.pick(STEMS);
                        Scope::Game(GameId::new(sys, stem))
                    }
                    _ => Scope::Auto,
                };
                let cut = (rng.below(3) == 0).then(|| rng.below(9000) as u64);
                let report = sync(&mut w, scope, cut, &what);
                if cut.is_none() {
                    assert_eq!(report.interrupted, None, "{what}");
                    assert_eq!(report.failed, [], "{what}");
                }
            }
        }
    }

    // However it got here, one whole sync brings the two into step.
    let what = format!("seed {seed}");
    let report = sync(&mut w, Scope::Full, None, &what);
    assert_eq!(report.interrupted, None, "{what}");
    assert_eq!(report.failed, [], "{what}");
    assert_eq!(
        holdings(w.device()),
        holdings(w.desk()),
        "{what}: {report:?}\n{:#?}\n{:#?}",
        library(&w.sd).keys().collect::<Vec<_>>(),
        library(&w.pc).keys().collect::<Vec<_>>()
    );

    // And a second one finds nothing to do, in any scope.
    let before = (library(&w.sd), library(&w.pc));
    for scope in [Scope::Full, Scope::Auto] {
        let again = sync(&mut w, scope, None, &what);
        assert!(
            again.is_noop(),
            "{what}: {again:?} after {report:?}\n{:#?}\n{:#?}",
            library(&w.sd).keys().collect::<Vec<_>>(),
            library(&w.pc).keys().collect::<Vec<_>>()
        );
    }
    assert!(before == (library(&w.sd), library(&w.pc)), "{what}");
}

#[test]
fn random_libraries_converge_and_lose_no_save() {
    // Fewer under qemu, where each case costs far more. SYNC_CASES runs
    // more.
    let cases = std::env::var("SYNC_CASES")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(if cfg!(target_arch = "arm") { 40 } else { 300 });
    let first = std::env::var("SYNC_SEED")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(1);
    for seed in first..=cases.max(first) {
        case(seed);
    }
}

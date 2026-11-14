//! The scenarios of docs/briefs/01-sync-core.md, phase 4: a Miyoo card
//! and a PC library in temporary folders, the hub in this process.

mod support;

use std::path::Path;

use slot_mini::fixture;
use slot_mini_store::{Favourites, Platform};
use slot_mini_sync::{
    hash_bytes, Client, DirectLink, GameId, Layout, Report, Scope, Side, SyncError, RESERVE,
};
use support::{
    age, emulators, kept_saves, library, named, pc_layout, read, save, tree, write, World,
};

fn rom(len: usize, fill: u8) -> Vec<u8> {
    (0..len).map(|n| (n * 31 % 251) as u8 ^ fill).collect()
}

fn none() -> [String; 0] {
    []
}

fn favourites(layout: &Layout) -> Vec<String> {
    let mut list: Vec<String> = std::fs::read_to_string(layout.favourites_file())
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect();
    list.sort();
    list
}

fn favourite(layout: &Layout, sys: Platform, stem: &str) {
    let mut favs = Favourites::read(layout.favourites_file());
    favs.toggle(sys, stem);
    std::fs::create_dir_all(layout.favourites_file().parent().unwrap()).unwrap();
    favs.write(layout.favourites_file()).unwrap();
}

fn shells(layout: &Layout) -> Vec<String> {
    let mut lines: Vec<String> = std::fs::read_to_string(layout.shell_file())
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect();
    lines.sort();
    lines
}

/// A game both sides have, with the same save: the usual starting point.
fn shared(w: &mut World, name: &str, fill: u8) {
    let stem = name.rsplit_once('.').unwrap().0;
    write(&w.sd.join("Roms/GBA").join(name), &rom(50_000, fill));
    let at = w.device().save_file(Platform::Gba, stem, "srm", None);
    write(&at, &save(fill));
    age(&at, 5000);
}

fn clean(report: &Report) {
    assert!(report.is_clean(), "{report:?}");
}

#[test]
fn seeding_a_full_miyoo_into_an_empty_pc() {
    let mut w = World::new();
    fixture::build(&w.sd, true).unwrap();
    let emerald = "Pokemon - Emerald Version (USA, Europe)";
    write(
        &w.device().save_file(Platform::Gba, emerald, "srm", None),
        &save(1),
    );
    write(
        &w.device().save_file(Platform::Gba, emerald, "rtc", None),
        &save(2),
    );
    write(
        &w.device().state_file(Platform::Gba, emerald, None),
        b"state",
    );
    let tetris = w
        .device()
        .save_file(Platform::Gb, "Tetris (World)", "srm", None);
    write(&tetris, &save(3));
    // A save nothing was ever written to stays behind.
    let blank = w
        .device()
        .save_file(Platform::Gba, "Golden Sun (USA, Europe)", "srm", None);
    write(&blank, &[0xff; 4096]);
    favourite(w.device(), Platform::Gba, emerald);
    let before = library(&w.sd);

    let report = w.sync(Scope::Full);
    clean(&report);
    assert!(report.to_client.is_empty() && report.removed.is_empty());
    assert_eq!(report.to_hub.updated, none());
    let count = |kind: &str| {
        report
            .to_hub
            .added
            .iter()
            .filter(|k| k.starts_with(kind))
            .count()
    };
    assert_eq!(count("rom:"), fixture::GAMES.len());
    assert_eq!(count("label:"), 2);
    assert_eq!(count("save:"), 3);
    assert_eq!(report.to_hub.added.len(), fixture::GAMES.len() + 5);

    // Nothing on the card changed.
    assert_eq!(library(&w.sd), before);
    // Every ROM and label is on the PC as it was on the card.
    for (sys, name, ..) in fixture::GAMES {
        let rel = format!("Roms/{}/{name}", sys.dir_name());
        assert_eq!(read(&w.pc.join(&rel)), before[&rel], "{rel}");
    }
    for name in named(&w.sd, "Labels/GBA/") {
        assert_eq!(read(&w.pc.join(&name)), before[&name], "{name}");
    }
    // The saves sit where RetroArch on the PC looks; the state stays.
    let saves = w.pc.join("RetroArch/saves");
    assert_eq!(read(&saves.join(format!("mGBA/{emerald}.srm"))), save(1));
    assert_eq!(read(&saves.join(format!("mGBA/{emerald}.rtc"))), save(2));
    assert_eq!(read(&saves.join("Gambatte/Tetris (World).srm")), save(3));
    assert_eq!(tree(&saves).len(), 3);
    assert!(tree(&w.pc.join("RetroArch/states")).is_empty());
    // Shells and favourites came too.
    assert_eq!(
        shells(w.desk()),
        [
            "Advance Wars (USA) = auto 3a7d44 clear",
            "Tetris (World) = notched 2f5cc0 glitter"
        ]
    );
    assert_eq!(favourites(w.desk()), favourites(w.device()));
    assert_eq!(favourites(w.desk()).len(), 1);

    let again = w.sync(Scope::Full);
    assert!(again.is_noop(), "{again:?}");
    assert_eq!(library(&w.sd), before);
}

#[test]
fn a_game_added_on_the_pc_arrives_with_full_not_auto() {
    let mut w = World::new();
    shared(&mut w, "Old.gba", 1);
    clean(&w.sync(Scope::Full));
    write(&w.pc.join("Roms/GBA/New.gba"), &rom(80_000, 2));
    write(&w.pc.join("Labels/GBA/New.png"), b"label");
    write(&w.pc.join("RetroArch/saves/mGBA/New.srm"), &save(2));
    let before = library(&w.sd);

    // The automatic sync does not move ROMs, nor a save without its game.
    let auto = w.sync(Scope::Auto);
    clean(&auto);
    assert_eq!(auto.to_client.added, ["label:GBA/New"]);
    assert!(!w.sd.join("Roms/GBA/New.gba").exists());
    assert_eq!(named(&w.sd, "New.srm"), none());
    let game = w.sync(Scope::Game(GameId::new(Platform::Gba, "New")));
    assert!(game.is_noop(), "{game:?}");

    let full = w.sync(Scope::Full);
    clean(&full);
    assert_eq!(
        full.to_client.added,
        ["save:GBA/New.srm", "rom:GBA/New.gba"]
    );
    assert!(full.to_hub.is_empty());
    assert_eq!(read(&w.sd.join("Roms/GBA/New.gba")), rom(80_000, 2));
    assert_eq!(
        read(&w.device().save_file(Platform::Gba, "New", "srm", None)),
        save(2)
    );
    assert_eq!(library(&w.sd).len(), before.len() + 3);
    assert!(w.sync(Scope::Auto).is_noop());
}

#[test]
fn a_save_from_the_pc_sets_the_miyoos_auto_state_aside() {
    let mut w = World::new();
    // gpSP on the Miyoo, mGBA on the PC: the same save, another folder.
    emulators(&w.sd, "gpsp");
    shared(&mut w, "Game.gba", 1);
    let mine = w.sd.join("Saves/CurrentProfile/saves/gpSP/Game.srm");
    assert_eq!(read(&mine), save(1));
    clean(&w.sync(Scope::Full));
    let theirs = w.pc.join("RetroArch/saves/mGBA/Game.srm");
    assert_eq!(read(&theirs), save(1));

    // Played on the Miyoo earlier (an auto state is left), then on the PC.
    let state =
        w.sd.join("Saves/CurrentProfile/states/gpSP/Game.state.auto");
    write(&state, b"where the Miyoo left off");
    write(&theirs, &save(9));

    let report = w.sync(Scope::Auto);
    clean(&report);
    assert_eq!(report.to_client.updated, ["save:GBA/Game.srm"]);
    assert!(report.to_hub.is_empty());
    assert_eq!(read(&mine), save(9));
    // The state would have resumed over the new save: it is set aside,
    // and so is the save it belonged to.
    assert!(!state.exists());
    let aside = named(&w.sd.join("Saves/CurrentProfile"), ".bak_");
    assert_eq!(aside.len(), 2, "{aside:?}");
    assert!(aside[0].starts_with("saves/gpSP/Game.srm.bak_"));
    assert!(aside[1].starts_with("states/gpSP/Game.state.auto.bak_"));
    let kept = tree(&w.sd.join("Saves/CurrentProfile"));
    assert_eq!(kept[&aside[0]], save(1));
    assert_eq!(kept[&aside[1]], b"where the Miyoo left off");
    assert!(w.sync(Scope::Auto).is_noop());
}

#[test]
fn a_game_played_on_both_sides_keeps_both_saves() {
    let mut w = World::new();
    shared(&mut w, "Game.gba", 1);
    clean(&w.sync(Scope::Full));
    let mine = w.device().save_file(Platform::Gba, "Game", "srm", None);
    let theirs = w.pc.join("RetroArch/saves/mGBA/Game.srm");
    let state = w.device().state_file(Platform::Gba, "Game", None);
    // The PC last; the Miyoo an hour before.
    write(&mine, &save(20));
    age(&mine, 3600);
    write(&state, b"state");
    write(&theirs, &save(30));

    let report = w.sync(Scope::Auto);
    assert_eq!(
        report.conflicts,
        [("save:GBA/Game.srm".to_string(), Side::Hub)]
    );
    assert!(report.failed.is_empty() && report.interrupted.is_none());
    // The later save is the save on both; the other is kept on both.
    assert_eq!(read(&mine), save(30));
    assert_eq!(read(&theirs), save(30));
    assert!(!state.exists());
    for layout in [w.device(), w.desk()] {
        let kept = kept_saves(layout);
        for fill in [20, 30] {
            assert!(kept.contains(&hash_bytes(&save(fill))), "save {fill} lost");
        }
    }
    let conflicts = w.hub.conflicts();
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].key, "save:GBA/Game.srm");
    assert_eq!(conflicts[0].device, w.client.device_name());
    let copy = w.pc.join("RetroArch/saves").join(&conflicts[0].file);
    assert!(conflicts[0].file.starts_with("mGBA/Game.srm.conflict-"));
    assert_eq!(read(&copy), save(20));
    assert!(w.sync(Scope::Auto).is_noop());

    // The player picks the other one on the PC: it becomes the save on
    // both, and nothing is lost by it.
    w.hub.swap_conflict(&conflicts[0].id).unwrap();
    assert_eq!(read(&theirs), save(20));
    let report = w.sync(Scope::Auto);
    clean(&report);
    assert_eq!(report.to_client.updated, ["save:GBA/Game.srm"]);
    assert_eq!(read(&mine), save(20));
    for layout in [w.device(), w.desk()] {
        let kept = kept_saves(layout);
        assert!(kept.contains(&hash_bytes(&save(20))) && kept.contains(&hash_bytes(&save(30))));
    }

    // The other way round: the Miyoo last.
    write(&theirs, &save(40));
    age(&theirs, 3600);
    write(&mine, &save(50));
    let report = w.sync(Scope::Auto);
    assert_eq!(
        report.conflicts,
        [("save:GBA/Game.srm".to_string(), Side::Client)]
    );
    assert_eq!(read(&mine), save(50));
    assert_eq!(read(&theirs), save(50));
    assert!(kept_saves(w.desk()).contains(&hash_bytes(&save(40))));
    assert!(w.sync(Scope::Auto).is_noop());
}

#[test]
fn remove_everywhere_reaches_every_device_even_one_paired_later() {
    let mut w = World::new();
    let game = GameId::new(Platform::Gba, "Game");
    for name in ["Game.gba", "Stays.gba"] {
        shared(&mut w, name, 1);
    }
    write(&w.sd.join("Labels/GBA/Game.png"), b"label");
    write(
        &w.device().shell_file(),
        b"Game = auto 3a7d44 clear\nStays = auto 2f5cc0 clear\n",
    );
    favourite(w.device(), Platform::Gba, "Game");
    favourite(w.device(), Platform::Gba, "Stays");
    clean(&w.sync(Scope::Full));
    // A second Miyoo with the same game, not paired yet.
    let other = tempfile::tempdir().unwrap();
    let card = other.path();
    emulators(card, "mgba");
    write(&card.join("Roms/GBA/Game.gba"), &rom(50_000, 1));
    let its_save = Layout::device(card).save_file(Platform::Gba, "Game", "srm", None);
    write(&its_save, &save(77));

    w.hub.remove_everywhere(&game).unwrap();
    assert_eq!(w.hub.tombstones(), std::slice::from_ref(&game));
    assert_eq!(
        named(&w.pc, "Game"),
        [named(&w.pc, "slot-mini-removed")[0].clone()]
    );
    assert_eq!(shells(w.desk()), ["Stays = auto 2f5cc0 clear"]);
    assert_eq!(favourites(w.desk()).len(), 1);

    // The automatic sync is enough.
    let report = w.sync(Scope::Auto);
    clean(&report);
    assert_eq!(report.removed, ["GBA/Game"]);
    assert!(report.to_client.is_empty() && report.to_hub.is_empty());
    // The game is gone from the card; its save is in the removed folder.
    let left = named(&w.sd, "Game");
    assert_eq!(left.len(), 1, "{left:?}");
    assert!(left[0].starts_with("Saves/slot-mini-removed/GBA/Game.srm."));
    assert_eq!(read(&w.sd.join(&left[0])), save(1));
    assert_eq!(shells(w.device()), ["Stays = auto 2f5cc0 clear"]);
    assert_eq!(favourites(w.device()), favourites(w.desk()));
    assert!(w.sd.join("Roms/GBA/Stays.gba").exists());
    assert!(w.sync(Scope::Full).is_noop());

    // The device paired afterwards loses the game at its first sync, and
    // keeps its save aside too.
    let mut late = Client::open(Layout::device(card)).unwrap();
    late.set_device_name("Second Miyoo").unwrap();
    support::pair(&w.hub, &mut late);
    let mut link = DirectLink::new(&w.hub);
    let report = late.sync(&mut link, Scope::Auto, &mut |_| {}, &Default::default());
    clean(&report);
    assert_eq!(report.removed, ["GBA/Game"]);
    assert!(!card.join("Roms/GBA/Game.gba").exists() && !its_save.exists());
    let left = named(card, "Game");
    assert_eq!(left.len(), 1, "{left:?}");
    assert_eq!(read(&card.join(&left[0])), save(77));
    // Nothing of it came back to the PC, and the first Miyoo is in step.
    assert_eq!(named(&w.pc, "Game").len(), 1);
    assert!(w.sync(Scope::Full).is_noop());

    // Added again on the PC, it is a new game everywhere.
    write(&w.pc.join("Roms/GBA/Game.gba"), &rom(50_000, 1));
    let report = w.sync(Scope::Full);
    clean(&report);
    assert_eq!(report.to_client.added, ["rom:GBA/Game.gba"]);
    assert!(report.removed.is_empty());
}

#[test]
fn roms_that_do_not_fit_the_card_are_left_and_listed() {
    let mut w = World::with(
        |sd| Layout::device(sd).with_free_space(RESERVE + 1_000_000),
        pc_layout,
    );
    w.pair();
    write(&w.pc.join("Roms/GBA/Small.gba"), &rom(400_000, 1));
    write(&w.pc.join("Roms/GBA/Big.gba"), &rom(4_000_000, 2));
    write(&w.pc.join("Roms/GBC/Other.gbc"), &rom(300_000, 3));
    write(&w.pc.join("Labels/GBA/Big.png"), b"label");
    write(&w.pc.join("RetroArch/saves/mGBA/Big.srm"), &save(2));
    write(&w.pc.join("RetroArch/saves/mGBA/Small.srm"), &save(1));
    write(&w.sd.join("Roms/GB/Mine.gb"), &rom(200_000, 4));

    let report = w.sync(Scope::Full);
    assert_eq!(
        report.didnt_fit,
        [("rom:GBA/Big.gba".to_string(), 4_000_000)]
    );
    assert!(report.failed.is_empty() && report.interrupted.is_none());
    // What fits arrives; the big game's save waits for its game.
    assert_eq!(
        report.to_client.added,
        [
            "label:GBA/Big",
            "save:GBA/Small.srm",
            "rom:GBA/Small.gba",
            "rom:GBC/Other.gbc"
        ]
    );
    assert_eq!(report.to_hub.added, ["rom:GB/Mine.gb"]);
    assert!(!w.sd.join("Roms/GBA/Big.gba").exists());
    assert_eq!(named(&w.sd, "Big.srm"), none());
    assert_eq!(named(&w.sd, ".tmp"), none());
    // It is said again each time, and nothing else moves.
    let again = w.sync(Scope::Full);
    assert_eq!(again.didnt_fit, report.didnt_fit);
    assert!(again.to_client.is_empty() && again.to_hub.is_empty());
    assert!(w.sync(Scope::Auto).is_noop());
}

#[test]
fn a_game_running_on_the_pc_is_skipped_until_it_stops() {
    let mut w = World::new();
    for name in ["Running.gba", "Idle.gba"] {
        shared(&mut w, name, 1);
    }
    clean(&w.sync(Scope::Full));
    let running = GameId::new(Platform::Gba, "Running");
    w.hub.set_busy(Some(running));
    let pc_save = w.pc.join("RetroArch/saves/mGBA/Running.srm");
    // RetroArch on the PC has the save open; the Miyoo played both.
    write(
        &w.device().save_file(Platform::Gba, "Running", "srm", None),
        &save(5),
    );
    write(
        &w.device().save_file(Platform::Gba, "Idle", "srm", None),
        &save(6),
    );
    write(&w.sd.join("Labels/GBA/Running.png"), b"label");

    let report = w.sync(Scope::Auto);
    assert_eq!(report.busy, ["GBA/Running"]);
    assert_eq!(report.to_hub.updated, ["save:GBA/Idle.srm"]);
    assert_eq!(report.to_hub.added, none());
    assert_eq!(read(&pc_save), save(1));
    assert_eq!(named(&w.pc, ".bak_").len(), 1);
    assert!(!w.pc.join("Labels/GBA/Running.png").exists());

    w.hub.set_busy(None);
    let report = w.sync(Scope::Auto);
    clean(&report);
    assert_eq!(report.to_hub.updated, ["save:GBA/Running.srm"]);
    assert_eq!(report.to_hub.added, ["label:GBA/Running"]);
    assert_eq!(read(&pc_save), save(5));
    assert!(w.sync(Scope::Auto).is_noop());
}

#[test]
fn an_interruption_halfway_through_a_rom_resumes_with_what_is_left() {
    let mut w = World::new();
    let big = rom(600_000, 1);
    write(&w.pc.join("Roms/GBA/Big.gba"), &big);
    write(&w.pc.join("Roms/GB/Small.gb"), &rom(40_000, 2));
    write(&w.pc.join("Labels/GBA/Big.png"), b"label");
    write(&w.sd.join("Roms/GBC/Mine.gbc"), &rom(500_000, 3));
    write(
        &w.device().save_file(Platform::Gbc, "Mine", "srm", None),
        &save(3),
    );

    // The Wi-Fi drops with the small ROM and half of the big one across.
    let report = w.sync_cut(Scope::Full, 340_000);
    assert!(
        matches!(report.interrupted, Some(SyncError::Network(_))),
        "{report:?}"
    );
    assert_eq!(report.to_client.added, ["label:GBA/Big", "rom:GB/Small.gb"]);
    // Saves and labels go before any ROM, so they are across already.
    assert_eq!(report.to_hub.added, ["save:GBC/Mine.srm"]);
    assert!(!w.sd.join("Roms/GBA/Big.gba").exists());
    assert_eq!(named(&w.sd, ".tmp"), none());
    assert_eq!(named(&w.pc, ".tmp"), none());

    // Then again halfway through the Miyoo's own ROM.
    let report = w.sync_cut(Scope::Full, 600_000 + 250_000);
    assert!(
        matches!(report.interrupted, Some(SyncError::Network(_))),
        "{report:?}"
    );
    assert_eq!(report.to_client.added, ["rom:GBA/Big.gba"]);
    assert_eq!(report.to_hub.added, none());
    assert!(!w.pc.join("Roms/GBC/Mine.gbc").exists());
    assert_eq!(named(&w.pc, ".tmp"), none());

    // The next sync moves only what is left.
    let report = w.sync(Scope::Full);
    clean(&report);
    assert!(report.to_client.is_empty());
    assert_eq!(report.to_hub.added, ["rom:GBC/Mine.gbc"]);
    assert_eq!(read(&w.sd.join("Roms/GBA/Big.gba")), big);
    assert_eq!(read(&w.pc.join("Roms/GBC/Mine.gbc")), rom(500_000, 3));
    assert!(w.sync(Scope::Full).is_noop());
}

#[test]
fn shells_edited_on_both_sides_merge_line_by_line() {
    let mut w = World::new();
    for name in ["A.gba", "B.gba", "C.gba", "D.gba", "E.gba", "F.gba"] {
        shared(&mut w, name, 1);
    }
    let base = "A = auto 111111 clear\nB = auto 222222 clear\nC = auto 333333 clear\nF = auto 666666 clear\n";
    write(&w.device().shell_file(), base.as_bytes());
    clean(&w.sync(Scope::Full));
    assert_eq!(read(&w.desk().shell_file()), base.as_bytes());

    // The Miyoo recolours A and F, drops B, adds D.
    let mine = "A = auto aaaaaa clear\nC = auto 333333 clear\nD = notched 444444 glitter\nF = auto f0f0f0 clear\n";
    write(&w.device().shell_file(), mine.as_bytes());
    // The PC recolours C and F, drops nothing, adds E.
    let theirs = "A = auto 111111 clear\nB = auto 222222 clear\nC = auto cccccc clear\nE = auto 555555 clear\nF = auto 0f0f0f clear\n";
    write(&w.desk().shell_file(), theirs.as_bytes());

    let report = w.sync(Scope::Auto);
    clean(&report);
    assert!(report.text_changed);
    assert!(report.to_client.is_empty() && report.to_hub.is_empty());
    // Each side's change is kept; where both changed a line, the PC's is.
    let merged = [
        "A = auto aaaaaa clear",
        "C = auto cccccc clear",
        "D = notched 444444 glitter",
        "E = auto 555555 clear",
        "F = auto 0f0f0f clear",
    ];
    assert_eq!(shells(w.device()), merged);
    assert_eq!(shells(w.desk()), merged);
    let again = w.sync(Scope::Auto);
    assert!(again.is_noop(), "{again:?}");
    assert_eq!(shells(w.device()), merged);
}

#[test]
fn a_favourite_added_on_one_side_and_removed_on_the_other() {
    let mut w = World::new();
    for name in ["A.gba", "B.gba", "C.gba", "D.gba"] {
        shared(&mut w, name, 1);
    }
    favourite(w.device(), Platform::Gba, "A");
    favourite(w.device(), Platform::Gba, "B");
    clean(&w.sync(Scope::Full));
    assert_eq!(favourites(w.desk()), favourites(w.device()));
    assert_eq!(favourites(w.desk()).len(), 2);

    // The Miyoo drops A and adds C; the PC adds D, and drops and adds B.
    favourite(w.device(), Platform::Gba, "A");
    favourite(w.device(), Platform::Gba, "C");
    favourite(w.desk(), Platform::Gba, "D");
    favourite(w.desk(), Platform::Gba, "B");
    favourite(w.desk(), Platform::Gba, "B");

    let report = w.sync(Scope::Auto);
    clean(&report);
    assert!(report.text_changed);
    let favs = Favourites::read(w.device().favourites_file());
    for (stem, there) in [("A", false), ("B", true), ("C", true), ("D", true)] {
        assert_eq!(favs.contains(Platform::Gba, stem), there, "{stem}");
    }
    assert_eq!(favourites(w.desk()), favourites(w.device()));
    assert!(w.sync(Scope::Auto).is_noop());

    // A favourite of one game's save scope is left for the next sync.
    favourite(w.device(), Platform::Gba, "A");
    let one = w.sync(Scope::Game(GameId::new(Platform::Gba, "A")));
    assert!(one.is_noop(), "{one:?}");
    assert_ne!(favourites(w.desk()), favourites(w.device()));
    clean(&w.sync(Scope::Auto));
    assert_eq!(favourites(w.desk()), favourites(w.device()));
    assert_eq!(favourites(w.desk()).len(), 4);
}

#[test]
fn an_upper_case_extension_and_a_rom_in_a_subfolder() {
    let mut w = World::new();
    write(&w.sd.join("Roms/GBA/Loud.GBA"), &rom(60_000, 1));
    write(&w.sd.join("Roms/GBA/Hacks/Deep.gba"), &rom(70_000, 2));
    write(&w.sd.join("Roms/GB/Old/Far.GB"), &rom(30_000, 3));
    write(
        &w.device().save_file(Platform::Gba, "Loud", "srm", None),
        &save(1),
    );
    write(
        &w.device().save_file(Platform::Gba, "Deep", "srm", None),
        &save(2),
    );
    write(&w.sd.join("Labels/GBA/Deep.png"), b"label");
    write(&w.device().shell_file(), b"Deep = auto 111111 clear\n");
    favourite(w.device(), Platform::Gb, "Far");

    let report = w.sync(Scope::Full);
    clean(&report);
    assert_eq!(
        report.to_hub.added,
        [
            "label:GBA/Deep",
            "save:GBA/Deep.srm",
            "save:GBA/Loud.srm",
            "rom:GB/Old/Far.GB",
            "rom:GBA/Hacks/Deep.gba",
            "rom:GBA/Loud.GBA"
        ]
    );
    // The PC holds them under the same names, in the same folders.
    assert_eq!(
        named(&w.pc, "Roms/"),
        [
            "Roms/GB/Old/Far.GB",
            "Roms/GBA/Hacks/Deep.gba",
            "Roms/GBA/Loud.GBA"
        ]
    );
    assert_eq!(read(&w.pc.join("Roms/GBA/Loud.GBA")), rom(60_000, 1));
    // Saves, labels, shells and favourites go by the game's name alone.
    assert_eq!(read(&w.pc.join("RetroArch/saves/mGBA/Deep.srm")), save(2));
    assert_eq!(read(&w.pc.join("RetroArch/saves/mGBA/Loud.srm")), save(1));
    assert!(w.pc.join("Labels/GBA/Deep.png").exists());
    assert_eq!(shells(w.desk()), ["Deep = auto 111111 clear"]);
    assert_eq!(favourites(w.desk()), favourites(w.device()));
    assert!(w.sync(Scope::Full).is_noop());

    // And back: played on the PC, the saves return to the same games.
    write(&w.pc.join("RetroArch/saves/mGBA/Deep.srm"), &save(8));
    write(&w.pc.join("RetroArch/saves/Gambatte/Far.srm"), &save(9));
    let report = w.sync(Scope::Auto);
    clean(&report);
    assert_eq!(report.to_client.updated, ["save:GBA/Deep.srm"]);
    assert_eq!(report.to_client.added, ["save:GB/Far.srm"]);
    assert_eq!(
        read(&w.device().save_file(Platform::Gba, "Deep", "srm", None)),
        save(8)
    );
    assert_eq!(
        read(&w.device().save_file(Platform::Gb, "Far", "srm", None)),
        save(9)
    );
    assert!(w.sync(Scope::Full).is_noop());
}

#[test]
fn a_name_the_card_cannot_store_is_left_and_listed() {
    let mut w = World::new();
    let root = w.pc.clone();
    let pc = |rel: &str| -> std::path::PathBuf { Path::new(&root).join(rel) };
    write(&pc("Roms/GBA/What?.gba"), &rom(50_000, 1));
    write(&pc("Labels/GBA/What?.png"), b"label");
    write(&pc("RetroArch/saves/mGBA/What?.srm"), &save(1));
    write(&pc("Roms/GB/Quote \"Me\".gb"), &rom(50_000, 2));
    write(&pc("Roms/GBC/Trailing dot..gbc"), &rom(50_000, 3));
    write(&pc("Roms/GBA/Fine & Well (USA) [!].gba"), &rom(50_000, 4));

    let report = w.sync(Scope::Full);
    let listed: Vec<&str> = report.unstorable.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(
        listed,
        [
            "label:GBA/What?",
            "save:GBA/What?.srm",
            "rom:GB/Quote \"Me\".gb",
            "rom:GBA/What?.gba"
        ]
    );
    assert!(report
        .unstorable
        .iter()
        .all(|(_, side)| *side == Side::Client));
    assert!(report.failed.is_empty() && report.interrupted.is_none());
    // The rest of the library arrives.
    assert_eq!(
        report.to_client.added,
        [
            "rom:GBA/Fine & Well (USA) [!].gba",
            "rom:GBC/Trailing dot..gbc"
        ]
    );
    assert_eq!(named(&w.sd, "What"), none());
    assert_eq!(named(&w.sd, "Quote"), none());
    // The PC keeps what it has, and the next sync says the same.
    assert!(pc("Roms/GBA/What?.gba").exists());
    let again = w.sync(Scope::Full);
    assert_eq!(again.unstorable, report.unstorable);
    assert!(again.to_client.is_empty() && again.to_hub.is_empty() && again.removed.is_empty());
}

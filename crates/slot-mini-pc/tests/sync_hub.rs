//! Brief 3, phase 1: the hub inside the PC app, against the real client.

mod common;

use std::time::Duration;

use common::{at_rest, pair, read, save, write, Miyoo, Rig};
use slot_mini::fixture::GAMES;
use slot_mini_gfx::Button;
use slot_mini_sync::{discover, Scope};

const WARS: &str = "Advance Wars (USA)";
const SUN: &str = "Golden Sun (USA, Europe)";

#[test]
fn a_miyoo_seeds_an_empty_library_and_a_running_game_is_left_alone() {
    let mut miyoo = Miyoo::new();
    write(&miyoo.save(WARS), &save(1));
    write(&miyoo.save(SUN), &save(2));
    // The stand-in RetroArch "plays" for a while, then writes its save.
    let mut rig = Rig::new("ms = 1500\n", miyoo.poke_port());
    assert!(rig.pc.app.current().is_none(), "the library starts empty");
    let address = rig.address();
    pair(&rig, &mut miyoo);

    // The first sync brings the Miyoo's library over.
    let report = miyoo.sync(&address, Scope::Full);
    assert!(report.is_clean(), "{report:?}");
    let roms = report
        .to_hub
        .added
        .iter()
        .filter(|k| k.starts_with("rom:"))
        .count();
    assert_eq!(roms, GAMES.len());
    for (platform, name, ..) in GAMES {
        let rom = rig
            .library()
            .join("Roms")
            .join(platform.dir_name())
            .join(name);
        assert!(rom.is_file(), "{}", rom.display());
    }
    assert!(rig
        .library()
        .join("Labels/GBA/Golden Sun (USA, Europe).png")
        .is_file());
    // Saves land where RetroArch on the PC reads them.
    assert_eq!(read(&rig.save("mGBA", WARS)), save(1));
    assert_eq!(read(&rig.save("mGBA", SUN)), save(2));
    // The hub's state is in the library, so it moves with it.
    let state = rig.library().join("App/slot-mini-pc/data/sync");
    assert!(state.join("hub.txt").is_file());
    assert!(state.join("devices").is_dir());
    // The shelves fill without a restart.
    rig.wait_until("the shelf to fill", |r| r.pc.app.current().is_some());

    // A game runs on the PC; meanwhile it and another were played on the
    // Miyoo.
    rig.select(WARS);
    rig.tap(Button::A);
    rig.wait_until("the game to start", |r| r.pc.in_game());
    write(&miyoo.save(WARS), &save(3));
    write(&miyoo.save(SUN), &save(4));
    let report = miyoo.sync(&address, Scope::Auto);
    assert_eq!(report.busy, ["GBA/Advance Wars (USA)"]);
    assert_eq!(
        report.to_hub.updated,
        ["save:GBA/Golden Sun (USA, Europe).srm"]
    );
    assert_eq!(
        read(&rig.save("mGBA", WARS)),
        save(1),
        "RetroArch has it open"
    );
    assert_eq!(read(&rig.save("mGBA", SUN)), save(4));

    // When the game ends it is an ordinary game again, and the Miyoo on
    // its shelf hears about the new save.
    rig.wait_until("the game to end", |r| !r.pc.in_game());
    let poke = miyoo.ear.wait().expect("a poke after the game");
    assert!(miyoo.client.accept_poke(&poke));
    let report = miyoo.sync(&address, Scope::Full);
    assert!(report.busy.is_empty(), "{report:?}");
    // Both sides changed that save: both copies are kept.
    assert_eq!(report.conflicts.len(), 1);
    rig.wait_until("the cart to come back", at_rest);
}

#[test]
fn the_hub_answers_discovery_while_the_app_runs_and_stops_with_it() {
    let mut rig = Rig::new("", 9);
    let sync = rig.pc.sync.as_ref().unwrap();
    let (server, discovery) = (sync.addr().unwrap(), sync.discovery_addr().unwrap());
    let found = discover(Duration::from_millis(500), &[discovery]).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].name, "Desk");
    assert_eq!(found[0].address.port(), server.port());
    assert!(std::net::TcpStream::connect(server).is_ok());

    rig.pc.close(None);
    assert!(discover(Duration::from_millis(300), &[discovery])
        .unwrap()
        .is_empty());
    assert!(std::net::TcpStream::connect(server).is_err());
}

#[test]
fn a_game_whose_files_are_moving_does_not_start() {
    let mut miyoo = Miyoo::new();
    let mut rig = Rig::new("ms = 50\n", miyoo.poke_port());
    let address = rig.address();
    pair(&rig, &mut miyoo);
    assert!(miyoo.sync(&address, Scope::Full).is_clean());
    rig.wait_until("the shelf to fill", |r| r.pc.app.current().is_some());
    rig.select(WARS);

    // A sync is half way through this game's save when A is pressed.
    write(&miyoo.save(WARS), &save(5));
    let mut pressed = false;
    let mut events = |event| {
        if let (slot_mini_sync::Event::Transfer { .. }, false) = (&event, pressed) {
            pressed = true;
            rig.tap(Button::A);
            rig.wait_until("the cart to come back", at_rest);
            assert!(!rig.pc.in_game());
            assert_eq!(
                rig.pc.app.notice(),
                Some("Syncing this game: try again in a moment")
            );
        }
    };
    let report = miyoo.sync_events(&address, Scope::Auto, &mut events);
    assert!(pressed && report.is_clean(), "{report:?}");
    assert!(report.busy.is_empty());
    assert_eq!(read(&rig.save("mGBA", WARS)), save(5));

    // Once the sync is over the game starts as ever.
    rig.tap(Button::A);
    rig.wait_until("the game to start", |r| r.pc.in_game());
}

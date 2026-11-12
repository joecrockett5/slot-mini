//! slot's Recents shelf: the last games launched from slot-mini, newest
//! first, ahead of Favorites. X takes a cart off it; MENU hides it.

mod common;

use common::{golden, Rig};
use slot_mini::library::ShelfKind;
use slot_mini::settings::{Recents, RECENTS_FILE};
use slot_mini_gfx::Button;
use slot_mini_store::Platform;

fn played(rig: &Rig, games: &[(Platform, &str)]) {
    let mut r = Recents::default();
    for (p, stem) in games.iter().rev() {
        r.played(*p, stem);
    }
    r.write(&rig.paths).unwrap();
}

fn kinds(d: &common::Driver) -> Vec<ShelfKind> {
    d.app.shelf_kinds()
}

#[test]
fn no_recents_shelf_until_something_is_played() {
    let rig = Rig::new();
    let d = rig.app();
    assert!(!kinds(&d).contains(&ShelfKind::Recents));
}

#[test]
fn recents_shelf_comes_first_newest_first() {
    let rig = Rig::new();
    played(
        &rig,
        &[
            (Platform::Gb, "Tetris (World)"),
            (Platform::Gba, "Metroid Fusion (USA)"),
            (Platform::Gba, "No Longer On The Card"),
            (Platform::Gbc, "Wario Land 3 (World)"),
        ],
    );
    let mut d = rig.app();
    assert_eq!(kinds(&d)[0], ShelfKind::Recents);
    assert_eq!(d.app.current().unwrap().0, ShelfKind::Recents);
    assert_eq!(d.selected(), "Tetris (World)");
    d.run(400);
    golden("shelf_recents_named", &d.frame);
    d.tap(Button::Right);
    assert_eq!(d.selected(), "Metroid Fusion (USA)");
    d.tap(Button::Right);
    assert_eq!(d.selected(), "Wario Land 3 (World)");
}

#[test]
fn x_takes_a_cart_off_recents_and_only_there() {
    let rig = Rig::new();
    played(
        &rig,
        &[
            (Platform::Gb, "Tetris (World)"),
            (Platform::Gba, "Metroid Fusion (USA)"),
        ],
    );
    let mut d = rig.app();
    d.tap(Button::Right);
    d.tap(Button::X);
    // The last cart went: the one before it is selected.
    assert_eq!(d.app.current().unwrap().0, ShelfKind::Recents);
    assert_eq!(d.selected(), "Tetris (World)");
    let text = std::fs::read_to_string(rig.paths.data().join(RECENTS_FILE)).unwrap();
    assert_eq!(text, "gb/Tetris (World)\n");
    // The last one gone, the shelf goes and its system's shelf comes up.
    d.tap(Button::X);
    assert!(!kinds(&d).contains(&ShelfKind::Recents));
    assert_eq!(d.app.current().unwrap().0, ShelfKind::System(Platform::Gb));
    assert_eq!(d.selected(), "Tetris (World)");
    // X does nothing on any other shelf.
    let before = kinds(&d);
    d.tap(Button::X);
    assert_eq!(kinds(&d), before);
    assert_eq!(d.selected(), "Tetris (World)");
    assert!(rig.paths.sd.join("Roms/GB/Tetris (World).gb").exists());
}

#[test]
fn menu_hides_the_shelf_and_keeps_the_list() {
    let rig = Rig::new();
    played(&rig, &[(Platform::Gba, "Metroid Fusion (USA)")]);
    let mut d = rig.app();
    assert_eq!(d.app.current().unwrap().0, ShelfKind::Recents);
    d.tap(Button::Menu);
    d.tap(Button::Down);
    d.tap(Button::Down);
    d.run(50);
    golden("about_recents", &d.frame);
    d.tap(Button::A);
    let text = std::fs::read_to_string(rig.paths.data().join("settings.txt")).unwrap();
    assert!(text.contains("recents = off"), "{text}");
    d.tap(Button::B);
    assert!(!kinds(&d).contains(&ShelfKind::Recents));
    // The cart stays selected, on its own system's shelf.
    assert_eq!(d.app.current().unwrap().0, ShelfKind::System(Platform::Gba));
    assert_eq!(d.selected(), "Metroid Fusion (USA)");
    // A fresh start keeps it hidden; switching it on brings the list back.
    let mut d = rig.app();
    assert!(!kinds(&d).contains(&ShelfKind::Recents));
    d.tap(Button::Menu);
    d.tap(Button::Down);
    d.tap(Button::Down);
    d.tap(Button::A);
    d.tap(Button::B);
    assert_eq!(kinds(&d)[0], ShelfKind::Recents);
    assert_eq!(Recents::read(&rig.paths).list.len(), 1);
}

#[test]
fn launching_a_game_puts_it_on_recents() {
    let rig = Rig::new();
    let mut d = rig.app();
    d.go_to("Metroid Fusion (USA)");
    let (kind, cart) = d.app.current().map(|(k, c)| (k, c.clone())).unwrap();
    slot_mini::launch::hand_off(
        &rig.paths,
        slot_mini::launch::Mode::App,
        &kind.key(),
        &cart,
        false,
    )
    .unwrap();
    let d = rig.app();
    assert_eq!(kinds(&d)[0], ShelfKind::Recents);
    // Back on the shelf it was launched from, not on Recents.
    assert_eq!(d.app.current().unwrap().0, ShelfKind::System(Platform::Gba));
    assert_eq!(d.selected(), "Metroid Fusion (USA)");
}

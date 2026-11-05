//! Favourites, the About screen and sounds.

mod common;

use std::sync::{Arc, Mutex};

use common::{golden, Rig};
use slot_mini::library::ShelfKind;
use slot_mini::sound::{Clip, Recorder};
use slot_mini::status::FixedStatus;
use slot_mini::{App, Options, Phase};
use slot_mini_gfx::{Button, Canvas, SCREEN_H, SCREEN_W};
use slot_mini_store::Platform;

#[test]
fn y_hearts_a_cart_and_favorites_get_their_own_shelf() {
    let rig = Rig::new();
    let mut d = rig.app();
    d.go_to("Metroid Fusion (USA)");
    d.tap(Button::Y);
    assert_eq!(d.app.shelf_kinds()[0], ShelfKind::Favourites);
    assert_eq!(
        d.app.current().unwrap().0,
        ShelfKind::System(Platform::Gba),
        "Y keeps you where you are"
    );
    d.run(2500);
    golden("shelf_hearted", &d.frame);
    // Heart a Game Boy cart too (L1 twice: Favorites, then GB, wrapping),
    // then R1 wraps round to the Favorites shelf.
    d.tap(Button::L1);
    d.tap(Button::L1);
    d.go_to("Tetris (World)");
    d.tap(Button::Y);
    d.tap(Button::R1);
    assert_eq!(d.app.current().unwrap().0, ShelfKind::Favourites);
    d.run(300);
    golden("shelf_favorites", &d.frame);
    let saved = std::fs::read_to_string(rig.paths.data().join("favorites.txt")).unwrap();
    assert_eq!(saved, "gba/Metroid Fusion (USA)\ngb/Tetris (World)\n");
    // A fresh start reads them back.
    let d2 = rig.app();
    assert_eq!(d2.app.shelf_kinds()[0], ShelfKind::Favourites);
}

#[test]
fn unhearting_the_last_favourite_removes_the_shelf() {
    let rig = Rig::new();
    let mut d = rig.app();
    d.tap(Button::Y);
    d.tap(Button::L1);
    assert_eq!(d.app.current().unwrap().0, ShelfKind::Favourites);
    d.tap(Button::Y);
    assert!(!d.app.shelf_kinds().contains(&ShelfKind::Favourites));
    assert_eq!(d.app.current().unwrap().0, ShelfKind::System(Platform::Gba));
    assert_eq!(d.selected(), "2 Games in 1 (Europe)");
}

#[test]
fn menu_opens_about_and_settings_stick() {
    let rig = Rig::new();
    let mut d = rig.app();
    d.tap(Button::Menu);
    assert_eq!(d.app.phase, Phase::About);
    d.run(50);
    golden("about", &d.frame);
    // Sounds: Off -> On.
    d.tap(Button::A);
    assert!(d.app.settings.sounds);
    let text = std::fs::read_to_string(rig.paths.data().join("settings.txt")).unwrap();
    assert!(text.contains("sounds = on"), "{text}");
    d.tap(Button::B);
    assert_eq!(d.app.phase, Phase::Shelf);
}

#[test]
fn insert_and_eject_play_slots_clips() {
    let rig = Rig::new();
    let played = Arc::new(Mutex::new(Vec::new()));
    let opts = Options {
        sound: Some(Box::new(Recorder(played.clone()))),
        face_wait: None,
        seed: 1,
    };
    let status = FixedStatus {
        battery: None,
        wifi: false,
        clock: (0, 0),
        charging: false,
    };
    let mut app = App::new(rig.paths.clone(), Box::new(status), opts, 0);
    let mut frame = Canvas::new(SCREEN_W, SCREEN_H);
    app.handle(slot_mini_gfx::Event::Press(Button::A));
    app.tick(16);
    app.handle(slot_mini_gfx::Event::Release(Button::A));
    for t in 2..60 {
        app.tick(t * 16);
        app.render(&mut frame);
    }
    assert_eq!(*played.lock().unwrap(), vec![Clip::Insert]);
    let Some(slot_mini::Exit::Launch { cart, .. }) = app.exit.clone() else {
        panic!("no launch");
    };
    slot_mini::launch::hand_off(
        &rig.paths,
        slot_mini::launch::Mode::Launcher,
        "GBA",
        &cart,
        false,
    )
    .unwrap();
    let opts = Options {
        sound: Some(Box::new(Recorder(played.clone()))),
        face_wait: None,
        seed: 1,
    };
    let status = FixedStatus {
        battery: None,
        wifi: false,
        clock: (0, 0),
        charging: false,
    };
    let mut again = App::new(rig.paths.clone(), Box::new(status), opts, 0);
    // The eject sound starts with the first frame shown, not at startup.
    assert_eq!(*played.lock().unwrap(), vec![Clip::Insert]);
    again.tick(16);
    again.render(&mut frame);
    assert_eq!(*played.lock().unwrap(), vec![Clip::Insert, Clip::Eject]);
}

#[test]
fn hold_a_timing_matches_slot() {
    let rig = Rig::new();
    let mut d = rig.app();
    d.press(Button::A);
    d.run(480);
    assert_eq!(d.app.phase, Phase::Shelf, "still a tap at 480 ms");
    d.run(40);
    assert!(
        matches!(d.app.phase, Phase::Inserting { fresh: true, .. }),
        "slot's 500 ms hold starts fresh without waiting for the release"
    );
    d.release(Button::A);
    d.run(16);
    assert!(matches!(d.app.phase, Phase::Inserting { fresh: true, .. }));

    let mut d = rig.app();
    d.press(Button::A);
    d.run(300);
    d.release(Button::A);
    d.run(16);
    assert!(
        matches!(d.app.phase, Phase::Inserting { fresh: false, .. }),
        "a tap resumes"
    );
}

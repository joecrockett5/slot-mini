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

/// MENU -> Splash screens: slot's boot and shutdown screens go into Onion's
/// theme overrides and the choice is kept; again puts Onion's back.
#[test]
fn menu_puts_slots_screens_in_onions_overrides() {
    use slot_mini::bootscreen::{target, Screen};
    let rig = Rig::new();
    let mut d = rig.app();
    d.tap(Button::Menu);
    for _ in 0..3 {
        d.tap(Button::Down);
    }
    d.tap(Button::A);
    for s in Screen::ALL {
        assert_eq!(std::fs::read(target(&rig.paths, s)).unwrap(), s.png());
    }
    let text = std::fs::read_to_string(rig.paths.data().join("settings.txt")).unwrap();
    assert!(text.contains("boot_screen = on"), "{text}");
    d.run(50);
    golden("about_splash_screens", &d.frame);
    d.tap(Button::A);
    for s in Screen::ALL {
        assert!(!target(&rig.paths, s).exists(), "{s:?}");
    }
    assert!(!d.app.settings.boot_screen);
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

/// Turning Sounds on in MENU plays the insert clip at once (a quick test on
/// the device); turning it off plays nothing.
#[test]
fn turning_sounds_on_plays_one() {
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
    let mut tap = |app: &mut App, b: Button| {
        app.handle(slot_mini_gfx::Event::Press(b));
        app.handle(slot_mini_gfx::Event::Release(b));
        app.tick(16);
        app.render(&mut frame);
    };
    tap(&mut app, Button::Menu);
    assert_eq!(app.phase, Phase::About);
    // Sounds is the first row: Off -> On.
    tap(&mut app, Button::A);
    assert!(app.settings.sounds);
    assert_eq!(*played.lock().unwrap(), vec![Clip::Insert]);
    // On -> Off: silent.
    tap(&mut app, Button::A);
    assert!(!app.settings.sounds);
    assert_eq!(played.lock().unwrap().len(), 1);
}

/// The sound test reports on MENU's note line how the player ended, so a
/// silent device says why on screen.
#[test]
fn the_sound_test_shows_how_the_player_ended() {
    let rig = Rig::new();
    let script = rig.dir.path().join("player.sh");
    std::fs::write(
        &script,
        "#!/bin/sh\necho 'SDL_OpenAudio: No available audio device' >&2\nexit 1\n",
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let player = slot_mini::sound::Player::new(script, vec![], rig.dir.path().join("clips"));
    let opts = Options {
        sound: Some(Box::new(player)),
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
    let mut now = 0;
    let mut step = |app: &mut App, b: Option<Button>| {
        if let Some(b) = b {
            app.handle(slot_mini_gfx::Event::Press(b));
            app.handle(slot_mini_gfx::Event::Release(b));
        }
        now += 16;
        app.tick(now);
        app.render(&mut frame);
    };
    step(&mut app, Some(Button::Menu));
    step(&mut app, Some(Button::A));
    let note = |app: &App| app.about.as_ref().and_then(|a| a.note.clone());
    assert_eq!(note(&app).as_deref(), Some("Sound test: playing..."));
    // The player runs in real time: wait for it, however slow the machine.
    let start = std::time::Instant::now();
    while note(&app).as_deref() == Some("Sound test: playing...") {
        assert!(start.elapsed() < std::time::Duration::from_secs(60));
        std::thread::sleep(std::time::Duration::from_millis(10));
        step(&mut app, None);
    }
    assert_eq!(
        note(&app).as_deref(),
        Some("Sound test: exit 1: SDL_OpenAudio: No available audio device")
    );
    common::golden("about_sound_test", &frame);
}

/// MENU's bottom line shows the CPU's governor, clock and slot-mini's own
/// CPU use when it has nothing else to say.
#[test]
fn menu_shows_the_cpu_state() {
    struct WithCpu;
    impl slot_mini::status::Status for WithCpu {
        fn battery(&mut self) -> Option<u8> {
            Some(80)
        }
        fn wifi(&mut self) -> bool {
            false
        }
        fn clock(&mut self) -> (u32, u32) {
            (12, 0)
        }
        fn cpu(&mut self) -> Option<String> {
            Some("CPU ondemand 408 MHz, slot-mini 0.2%".into())
        }
    }
    let rig = Rig::new();
    let opts = Options {
        sound: Some(Box::new(Recorder::default())),
        face_wait: None,
        seed: 7,
    };
    let mut app = App::new(rig.paths.clone(), Box::new(WithCpu), opts, 0);
    let mut frame = Canvas::new(SCREEN_W, SCREEN_H);
    app.tick(16);
    app.render(&mut frame);
    app.handle(slot_mini_gfx::Event::Press(Button::Menu));
    app.handle(slot_mini_gfx::Event::Release(Button::Menu));
    app.tick(32);
    app.render(&mut frame);
    assert_eq!(
        app.about.as_ref().and_then(|a| a.diag.as_deref()),
        Some("CPU ondemand 408 MHz, slot-mini 0.2%")
    );
    golden("about_cpu", &frame);
}

/// slot's timing: the insert's click lands as the cart seats, so the clip
/// starts its lead before then, not when the cart starts moving.
#[test]
fn the_insert_sound_lands_as_the_cart_seats() {
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
    app.tick(0);
    app.render(&mut frame);
    app.handle(slot_mini_gfx::Event::Press(Button::A));
    app.tick(16);
    app.handle(slot_mini_gfx::Event::Release(Button::A));
    let start = 16;
    let lead = Clip::Insert.lead();
    let due = start + ((slot_mini::app::SEATED_AT - lead) * 1000.0).ceil() as u64;
    app.tick(due - 20);
    assert!(played.lock().unwrap().is_empty(), "not yet");
    app.tick(due + 1);
    assert_eq!(*played.lock().unwrap(), vec![Clip::Insert]);
    app.tick(due + 100);
    assert_eq!(played.lock().unwrap().len(), 1, "once");
}

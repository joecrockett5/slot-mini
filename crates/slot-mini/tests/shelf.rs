//! Golden images of the shelf at 640x480, and the shelf's behaviour.

mod common;

use common::{golden, Rig};
use slot_mini::library::ShelfKind;
use slot_mini_gfx::Button;
use slot_mini_store::Platform;

#[test]
fn first_frame_shows_the_gba_shelf_and_its_name() {
    let rig = Rig::new();
    let mut d = rig.app();
    d.run(400);
    assert_eq!(d.selected(), "2 Games in 1 (Europe)");
    golden("shelf_gba_named", &d.frame);
    d.run(2500);
    golden("shelf_gba", &d.frame);
}

#[test]
fn carts_with_label_art_and_a_text_label() {
    let rig = Rig::new();
    let mut d = rig.app();
    d.go_to("Golden Sun (USA, Europe)");
    d.run(2500);
    golden("shelf_logo_label", &d.frame);
    d.go_to("Pokemon - Emerald Version (USA, Europe)");
    d.run(1000);
    golden("shelf_real_label", &d.frame);
}

#[test]
fn mid_scroll_frame() {
    let rig = Rig::new();
    let mut d = rig.app();
    d.run(2500);
    d.press(Button::Right);
    d.run(64);
    golden("shelf_mid_scroll", &d.frame);
    d.release(Button::Right);
    d.run(1000);
    assert_eq!(d.selected(), "Advance Wars (USA)");
}

#[test]
fn holding_right_repeats() {
    let rig = Rig::new();
    let mut d = rig.app();
    d.press(Button::Right);
    d.run(400 + 110 + 40);
    d.release(Button::Right);
    // One step on press, then two repeats after slot's 400 ms delay.
    assert_eq!(d.selected(), "Mario Kart - Super Circuit (USA)");
}

#[test]
fn l1_and_r1_switch_systems_and_wrap() {
    let rig = Rig::new();
    let mut d = rig.app();
    assert_eq!(
        d.app.shelf_kinds(),
        vec![
            ShelfKind::System(Platform::Gba),
            ShelfKind::System(Platform::Gbc),
            ShelfKind::System(Platform::Gb)
        ]
    );
    d.tap(Button::R1);
    d.run(2500);
    assert_eq!(d.app.current().unwrap().0, ShelfKind::System(Platform::Gbc));
    golden("shelf_gbc", &d.frame);
    d.tap(Button::R1);
    d.run(300);
    assert_eq!(d.app.current().unwrap().0, ShelfKind::System(Platform::Gb));
    golden("shelf_gb_named", &d.frame);
    d.go_to("Tetris (World)");
    d.run(2500);
    golden("shelf_gb_glitter", &d.frame);
    d.tap(Button::R1);
    assert_eq!(d.app.current().unwrap().0, ShelfKind::System(Platform::Gba));
    d.tap(Button::L1);
    assert_eq!(d.app.current().unwrap().0, ShelfKind::System(Platform::Gb));
}

#[test]
fn up_and_down_jump_by_letter() {
    let rig = Rig::new();
    let mut d = rig.app();
    d.tap(Button::Down);
    assert_eq!(d.selected(), "Advance Wars (USA)");
    d.tap(Button::Down);
    assert_eq!(d.selected(), "Golden Sun (USA, Europe)");
    d.tap(Button::Down);
    assert_eq!(d.selected(), "Mario Kart - Super Circuit (USA)");
    d.tap(Button::Up);
    assert_eq!(d.selected(), "Golden Sun (USA, Europe)");
}

#[test]
fn an_empty_card_shows_the_empty_slot() {
    let rig = Rig::with(false);
    std::fs::remove_dir_all(rig.paths.sd.join("Roms")).unwrap();
    let mut d = rig.app();
    d.run(100);
    assert!(d.app.current().is_none());
    golden("empty", &d.frame);
}

#[test]
fn a_system_without_an_emulator_has_no_shelf() {
    let rig = Rig::new();
    std::fs::remove_file(rig.paths.sd.join("Emu/GBC/launch.sh")).unwrap();
    let d = rig.app();
    assert_eq!(
        d.app.shelf_kinds(),
        vec![
            ShelfKind::System(Platform::Gba),
            ShelfKind::System(Platform::Gb)
        ]
    );
}

#[test]
fn wallpaper_theme_and_status() {
    let rig = Rig::new();
    let wp = rig.paths.sd.join("Wallpapers");
    std::fs::create_dir_all(&wp).unwrap();
    std::fs::copy(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/background_default_blur.png"
        ),
        wp.join("blur.png"),
    )
    .unwrap();
    std::fs::create_dir_all(rig.paths.app()).unwrap();
    std::fs::write(
        rig.paths.app().join("theme.txt"),
        "appearance = light\nwallpaper_tint = 20\n",
    )
    .unwrap();
    let mut d = rig.app_with(slot_mini::status::FixedStatus {
        battery: Some(9),
        wifi: true,
        clock: (21, 7),
        charging: false,
    });
    d.run(2500);
    golden("shelf_light_wallpaper", &d.frame);
}

/// A wallpaper named after a shelf is that shelf's own (`gbc.png` here);
/// other shelves keep the random one, which is never the shelf file.
#[test]
fn a_shelf_named_wallpaper_is_shown_on_its_shelf() {
    let rig = Rig::new();
    let wp = rig.paths.sd.join("Wallpapers");
    std::fs::create_dir_all(&wp).unwrap();
    let assets = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/");
    std::fs::copy(
        format!("{assets}background_default_blur.png"),
        wp.join("blur.png"),
    )
    .unwrap();
    std::fs::copy(format!("{assets}board.png"), wp.join("gbc.png")).unwrap();
    // A spot above the carts that only the wallpaper covers.
    let at = |d: &common::Driver| d.frame.px[(40 * d.frame.w + 150) as usize];

    let mut d = rig.app();
    d.run(2500);
    assert_eq!(d.app.current().unwrap().0, ShelfKind::System(Platform::Gba));
    let random = at(&d);
    d.wait_until("the GBC wallpaper", |d| d.app.shelf_wallpapers() == 1);
    d.tap(Button::R1);
    d.run(2500);
    assert_eq!(d.app.current().unwrap().0, ShelfKind::System(Platform::Gbc));
    let own = at(&d);
    assert_ne!(own, random, "GBC shows its own wallpaper");
    golden("shelf_gbc_wallpaper", &d.frame);
    // Back on GBA, the random one again.
    d.tap(Button::L1);
    d.run(2500);
    assert_eq!(at(&d), random);
    // Starting on GBC, its wallpaper is there on the first frame.
    slot_mini::settings::State {
        shelf: Some("GBC".into()),
        cart: None,
        launched: false,
    }
    .write(&rig.paths)
    .unwrap();
    let d = rig.app();
    assert_eq!(d.app.current().unwrap().0, ShelfKind::System(Platform::Gbc));
    assert_eq!(at(&d), own);
}

/// Charging: slot's bolt left of the gauge and a green fill, in both inks.
#[test]
fn charging_shows_a_bolt_and_a_green_gauge() {
    let rig = Rig::new();
    let mut d = rig.app_with(slot_mini::status::FixedStatus {
        battery: Some(42),
        wifi: true,
        clock: (12, 0),
        charging: true,
    });
    d.run(2500);
    golden("shelf_charging", &d.frame);
    // The gauge's fill is green while charging.
    let green = (0..d.frame.h)
        .flat_map(|y| (0..120).map(move |x| (x, y)))
        .filter(|&(x, y)| d.frame.px[(y * d.frame.w + x) as usize] & 0x00ff_ffff == 0x004c_d964)
        .count();
    assert!(green > 20, "{green} green pixels");
}

/// An idle shelf is not drawn again (the device then shows nothing new and
/// sleeps), and what stays on screen is exactly a full redraw.
#[test]
fn an_idle_shelf_is_not_drawn_again() {
    let rig = Rig::new();
    let mut d = rig.app();
    d.run(2500);
    let frame = |d: &mut common::Driver| {
        d.now += common::FRAME_MS;
        d.app.tick(d.now);
        d.app.render(&mut d.frame)
    };
    assert!(!frame(&mut d), "a settled shelf is unchanged");
    assert!(!frame(&mut d));
    d.press(Button::Right);
    assert!(frame(&mut d), "a press moves the shelf");
    d.release(Button::Right);
    let moving = (0..10).filter(|_| frame(&mut d)).count();
    assert!(moving > 5, "the scroll animates ({moving} of 10 frames)");
    d.run(2500);
    assert!(!frame(&mut d), "settled again");
    // The kept frame is what a full redraw gives.
    let mut fresh = slot_mini_gfx::Canvas::new(d.frame.w, d.frame.h);
    assert!(d.app.render(&mut fresh), "another canvas is always drawn");
    assert!(fresh.px == d.frame.px);
}

/// Jumping far along a shelf, or to another shelf, must not show carts as
/// flat label colour while their faces are built (seen on the device). Uses
/// the device's real wait, not the tests' unlimited one.
#[test]
fn big_jumps_never_flash_placeholders() {
    use slot_mini::status::FixedStatus;
    use slot_mini::{App, Options};
    use slot_mini_gfx::{Canvas, Event, SCREEN_H, SCREEN_W};

    let rig = Rig::new();
    // A long shelf, so letter jumps land far from what was built.
    let gba = rig.paths.sd.join("Roms/GBA");
    for c in b'A'..=b'Z' {
        for i in 0..6 {
            std::fs::write(gba.join(format!("{} Game {i}.gba", c as char)), [0u8; 512]).unwrap();
        }
    }
    let opts = Options {
        sound: None,
        face_wait: Some(slot_mini::app::FACE_WAIT),
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
    let mut step = |app: &mut App, ev: Option<Event>| {
        if let Some(ev) = ev {
            app.handle(ev);
        }
        now += 16;
        app.tick(now);
        app.render(&mut frame);
        assert_eq!(
            app.placeholders_on_screen(),
            0,
            "a cart flashed its label colour at {now} ms"
        );
    };
    step(&mut app, None);
    for _ in 0..12 {
        step(&mut app, Some(Event::Press(Button::Down)));
        step(&mut app, Some(Event::Release(Button::Down)));
    }
    for b in [Button::R1, Button::R1, Button::L1, Button::L1, Button::L1] {
        step(&mut app, Some(Event::Press(b)));
        step(&mut app, Some(Event::Release(b)));
    }
    // Held scrolling at slot's fastest repeat.
    step(&mut app, Some(Event::Press(Button::Right)));
    for _ in 0..120 {
        step(&mut app, None);
    }
}

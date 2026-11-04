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
    });
    d.run(2500);
    golden("shelf_light_wallpaper", &d.frame);
}

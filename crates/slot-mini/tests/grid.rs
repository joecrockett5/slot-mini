//! The grid view, as the PC app shows it: golden images at 640x480 and its
//! behaviour. The Miyoo build has no grid yet, so these need the `pc`
//! feature (`cargo test --workspace` turns it on).
#![cfg(feature = "pc")]

mod common;

use common::{golden, Rig};
use slot_mini::app::{Phase, GRID_LIFT_MS};
use slot_mini::fixture::rom_bytes;
use slot_mini::library::ShelfKind;
use slot_mini_gfx::Button;
use slot_mini_store::Platform;

/// Leave `keep` of the fixture's GBA games and add `more` made-up ones.
fn stock(rig: &Rig, keep: usize, more: usize) {
    let dir = rig.paths.sd.join("Roms/GBA");
    let mut roms: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "gba"))
        .collect();
    roms.sort();
    for rom in roms.into_iter().skip(keep) {
        std::fs::remove_file(rom).unwrap();
    }
    const NAMES: [&str; 8] = [
        "Boktai", "Castle", "Drill", "Fire", "Kirby", "Mega", "Rhythm", "Wario",
    ];
    for i in 0..more {
        let name = format!("{} {:02}", NAMES[i % NAMES.len()], i + 1);
        let code = format!("B{:02}E", i % 100);
        std::fs::write(
            dir.join(format!("{name}.gba")),
            rom_bytes(Platform::Gba, &name.to_uppercase(), &code, 0),
        )
        .unwrap();
    }
}

fn light(rig: &Rig) {
    std::fs::create_dir_all(rig.paths.app()).unwrap();
    std::fs::write(rig.paths.app().join("theme.txt"), "appearance = light\n").unwrap();
}

#[test]
fn the_pc_opens_a_shelf_as_a_grid() {
    let rig = Rig::pc();
    let mut d = rig.app();
    assert!(d.app.grid_on());
    d.run(2500);
    assert_eq!(d.app.placeholders_on_screen(), 0);
    golden("grid_gba", &d.frame);
}

#[test]
fn one_cart() {
    let rig = Rig::pc();
    stock(&rig, 1, 0);
    let mut d = rig.app();
    d.run(2500);
    golden("grid_one", &d.frame);
}

#[test]
fn a_full_page_in_dark_and_light() {
    let rig = Rig::pc();
    stock(&rig, 7, 5);
    let mut d = rig.app();
    d.run(2500);
    golden("grid_full_page", &d.frame);
    light(&rig);
    let mut d = rig.app();
    d.run(2500);
    golden("grid_full_page_light", &d.frame);
}

#[test]
fn many_carts_scroll_by_rows() {
    let rig = Rig::pc();
    stock(&rig, 7, 33);
    let mut d = rig.app();
    d.run(2500);
    golden("grid_many", &d.frame);
    for _ in 0..6 {
        d.tap(Button::Down);
    }
    d.tap(Button::Right);
    d.wait_until("the faces", |d| d.app.placeholders_on_screen() == 0);
    d.run(1500);
    golden("grid_many_scrolled", &d.frame);
    light(&rig);
    let mut d = rig.app();
    for _ in 0..9 {
        d.tap(Button::Down);
    }
    d.wait_until("the faces", |d| d.app.placeholders_on_screen() == 0);
    d.run(2500);
    golden("grid_many_end_light", &d.frame);
}

#[test]
fn game_boy_shelves_and_hearts() {
    let rig = Rig::pc();
    let mut d = rig.app();
    d.tap(Button::Y);
    d.tap(Button::Right);
    d.tap(Button::Right);
    d.tap(Button::Y);
    d.tap(Button::R1);
    d.run(2500);
    golden("grid_gbc", &d.frame);
    d.tap(Button::R1);
    d.run(2500);
    golden("grid_gb", &d.frame);
    // Favorites holds carts of more than one shape.
    for _ in 0..6 {
        if d.app.current().unwrap().0 == ShelfKind::Favourites {
            break;
        }
        d.tap(Button::R1);
    }
    assert_eq!(d.app.current().unwrap().0, ShelfKind::Favourites);
    d.run(2500);
    golden("grid_favorites", &d.frame);
}

#[test]
fn the_dpad_moves_between_cells_and_l2_jumps_letters() {
    let rig = Rig::pc();
    let mut d = rig.app();
    let first = d.selected();
    d.tap(Button::Down);
    assert_eq!(d.selected(), "Metroid Fusion (USA)");
    d.tap(Button::Up);
    assert_eq!(d.selected(), first);
    d.tap(Button::Right);
    assert_eq!(d.selected(), "Advance Wars (USA)");
    d.press(Button::L2);
    d.tap(Button::Down);
    assert_eq!(d.selected(), "Golden Sun (USA, Europe)");
    d.tap(Button::Up);
    assert_eq!(d.selected(), "Advance Wars (USA)");
    d.release(Button::L2);
}

#[test]
fn r2_toggles_the_view_per_shelf_and_it_is_remembered() {
    let rig = Rig::pc();
    let mut d = rig.app();
    d.tap(Button::Right);
    d.tap(Button::R2);
    assert!(!d.app.grid_on());
    d.run(2500);
    // The same cart, now on slot's shelf.
    assert_eq!(d.selected(), "Advance Wars (USA)");
    golden("grid_toggled_to_shelf", &d.frame);
    d.tap(Button::R1);
    assert!(d.app.grid_on(), "the next shelf keeps its own view");
    d.tap(Button::L1);
    assert!(!d.app.grid_on());
    drop(d);
    let mut d = rig.app();
    assert!(!d.app.grid_on(), "remembered");
    d.tap(Button::R2);
    assert!(d.app.grid_on());
}

#[test]
fn a_cart_flies_from_its_cell_into_the_slot_and_back() {
    let rig = Rig::pc();
    let mut d = rig.app();
    d.tap(Button::Down);
    d.run(2500);
    d.tap(Button::A);
    assert!(matches!(d.app.phase, Phase::Inserting { .. }));
    d.run(GRID_LIFT_MS / 2);
    golden("grid_lift", &d.frame);
    d.run(GRID_LIFT_MS / 2 + 300);
    golden("grid_insert", &d.frame);
    d.run(700);
    assert!(d.app.exit.is_some(), "the game starts after the insert");
}

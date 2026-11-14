//! The grid view on the Miyoo: every shelf opens as slot's scrolling shelf,
//! R2 (or MENU -> Settings -> View) turns the shelf on screen into a grid
//! and back, and the choice is kept per shelf.

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

fn views(rig: &Rig) -> String {
    std::fs::read_to_string(rig.paths.data().join("views.txt")).unwrap_or_default()
}

#[test]
fn every_shelf_opens_as_the_scrolling_shelf() {
    let rig = Rig::new();
    let mut d = rig.app();
    for _ in 0..4 {
        assert!(!d.app.grid_on());
        d.tap(Button::R1);
    }
    d.run(500);
    assert_eq!(views(&rig), "", "nothing is written until the user chooses");
}

#[test]
fn r2_turns_the_shelf_into_a_grid_and_back_and_it_is_remembered() {
    let rig = Rig::new();
    let mut d = rig.app();
    d.tap(Button::Right);
    d.tap(Button::R2);
    assert!(d.app.grid_on());
    // The same cart, now in its cell.
    assert_eq!(d.selected(), "Advance Wars (USA)");
    d.wait_until("the faces", |d| d.app.placeholders_on_screen() == 0);
    d.run(2500);
    golden("miyoo_grid_gba", &d.frame);
    assert_eq!(views(&rig), "GBA = grid\n");

    d.tap(Button::R1);
    assert!(!d.app.grid_on(), "the next shelf keeps its own view");
    d.tap(Button::L1);
    assert!(d.app.grid_on());
    drop(d);
    let mut d = rig.app();
    assert!(d.app.grid_on(), "remembered");
    d.tap(Button::R2);
    assert!(!d.app.grid_on());
    assert_eq!(views(&rig), "GBA = shelf\n");
}

#[test]
fn a_full_page_in_dark_and_light() {
    let rig = Rig::new();
    stock(&rig, 7, 5);
    let mut d = rig.app();
    d.tap(Button::R2);
    d.wait_until("the faces", |d| d.app.placeholders_on_screen() == 0);
    d.run(2500);
    golden("miyoo_grid_full_page", &d.frame);
    std::fs::create_dir_all(rig.paths.app()).unwrap();
    std::fs::write(rig.paths.app().join("theme.txt"), "appearance = light\n").unwrap();
    let mut d = rig.app();
    assert!(d.app.grid_on());
    d.wait_until("the faces", |d| d.app.placeholders_on_screen() == 0);
    d.run(2500);
    golden("miyoo_grid_full_page_light", &d.frame);
}

#[test]
fn many_carts_scroll_by_rows() {
    let rig = Rig::new();
    stock(&rig, 7, 33);
    let mut d = rig.app();
    d.tap(Button::R2);
    for _ in 0..6 {
        d.tap(Button::Down);
    }
    d.tap(Button::Right);
    d.wait_until("the faces", |d| d.app.placeholders_on_screen() == 0);
    d.run(2500);
    golden("miyoo_grid_many_scrolled", &d.frame);
    // L2 with the D-pad jumps letters, as Up and Down do on the shelf.
    let here = d.selected();
    d.press(Button::L2);
    d.tap(Button::Down);
    d.release(Button::L2);
    assert_ne!(d.selected().chars().next(), here.chars().next());
}

#[test]
fn game_boy_shelves_and_favourites() {
    let rig = Rig::new();
    let mut d = rig.app();
    d.tap(Button::Y);
    d.tap(Button::Right);
    d.tap(Button::Right);
    d.tap(Button::Y);
    d.tap(Button::R1);
    d.tap(Button::R2);
    d.wait_until("the faces", |d| d.app.placeholders_on_screen() == 0);
    d.run(2500);
    golden("miyoo_grid_gbc", &d.frame);
    for _ in 0..6 {
        if d.app.current().unwrap().0 == ShelfKind::Favourites {
            break;
        }
        d.tap(Button::R1);
    }
    assert_eq!(d.app.current().unwrap().0, ShelfKind::Favourites);
    assert!(!d.app.grid_on());
    d.tap(Button::R2);
    d.wait_until("the faces", |d| d.app.placeholders_on_screen() == 0);
    d.run(2500);
    golden("miyoo_grid_favorites", &d.frame);
}

#[test]
fn settings_has_a_row_for_the_shelf_on_screen() {
    let rig = Rig::new();
    let mut d = rig.app();
    d.tap(Button::Menu);
    d.run(700);
    d.tap(Button::A);
    d.tap(Button::Up);
    d.tap(Button::Up);
    d.run(100);
    golden("about_view_row", &d.frame);
    d.tap(Button::A);
    d.run(100);
    golden("about_view_row_grid", &d.frame);
    assert_eq!(views(&rig), "GBA = grid\n");
    d.tap(Button::B);
    d.tap(Button::B);
    assert_eq!(d.app.phase, Phase::Shelf);
    assert!(d.app.grid_on());
    // And back again from the same row.
    d.tap(Button::Menu);
    d.run(700);
    d.tap(Button::A);
    d.tap(Button::Up);
    d.tap(Button::Up);
    d.tap(Button::A);
    d.tap(Button::B);
    d.tap(Button::B);
    assert!(!d.app.grid_on());
}

#[test]
fn a_cart_flies_from_its_cell_into_the_slot() {
    let rig = Rig::new();
    let mut d = rig.app();
    d.tap(Button::R2);
    d.tap(Button::Down);
    d.wait_until("the faces", |d| d.app.placeholders_on_screen() == 0);
    d.run(2500);
    d.tap(Button::A);
    assert!(matches!(d.app.phase, Phase::Inserting { .. }));
    d.run(GRID_LIFT_MS / 2);
    golden("miyoo_grid_lift", &d.frame);
    d.run(GRID_LIFT_MS / 2 + 300);
    golden("miyoo_grid_insert", &d.frame);
    d.run(700);
    assert!(d.app.exit.is_some(), "the game starts after the insert");
}

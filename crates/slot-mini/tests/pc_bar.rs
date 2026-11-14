//! The PC's shortcut bar and the mouse's buttons: the bar names the
//! buttons of the device in hand, its entries and slot's own legends
//! click, and the frame draws sharp at a whole multiple. Needs the `pc`
//! feature.
#![cfg(feature = "pc")]

mod common;

use common::{golden, Driver, Rig};
use slot_mini::bar::{layout, Item};
use slot_mini::legend::Glyphs;
use slot_mini::{Exit, Phase, Pointer};
use slot_mini_gfx::{Button, Canvas, SCREEN_H, SCREEN_W};

/// A driver on the shelf once its name has faded and the bar is up.
fn settled(rig: &Rig) -> Driver {
    let mut d = rig.app();
    d.run(4000);
    d
}

/// The middle of a bar entry's left (or right) half.
fn at(d: &Driver, item: Item, right: bool) -> (f32, f32) {
    for y in (300..480).step_by(2) {
        for x in (0..640).step_by(2) {
            let p = (x as f32, y as f32);
            if d.app.bar_at(p.0, p.1) == Some((item, right)) {
                return (p.0 + 6.0, p.1 + 6.0);
            }
        }
    }
    panic!("{item:?} is not on the bar");
}

fn click(d: &mut Driver, item: Item, right: bool) {
    let (x, y) = at(d, item, right);
    d.app.pointer(Pointer::Click(x, y));
    d.app.pointer(Pointer::Release);
}

#[test]
fn entries_fill_rows_centred_above_the_footer() {
    let one = layout(&[100, 100], 640.0, 400.0);
    assert_eq!(one[0][1], one[1][1]);
    assert_eq!(one[0][0] + (one[1][0] + 100.0), 640.0, "centred");
    assert!(one[0][1] + one[0][3] <= 400.0);
    let two = layout(&[250, 250, 250], 640.0, 400.0);
    assert_eq!(two[0][1], two[1][1]);
    assert!(two[2][1] > two[0][1], "the third wraps");
    assert!(two[2][1] + two[2][3] <= 400.0);
    assert_eq!(two[2][0], 195.0);
    assert!(layout(&[], 640.0, 400.0).is_empty());
}

#[test]
fn the_bar_names_the_buttons_of_the_device_in_hand() {
    let rig = Rig::pc();
    let mut d = settled(&rig);
    assert_eq!(
        d.app.bar_items(),
        [
            Item::Play,
            Item::Fave,
            Item::Studio,
            Item::Shelf,
            Item::Core,
            Item::View(false),
            Item::Menu,
        ]
    );
    golden("pc_bar_grid_pad", &d.frame);
    d.app.set_glyphs(Glyphs::Keyboard);
    d.run(50);
    golden("pc_bar_grid_keyboard", &d.frame);
    d.app.set_glyphs(Glyphs::Mouse);
    d.run(50);
    golden("pc_bar_grid_mouse", &d.frame);
    // The row view offers the grid instead.
    d.tap(Button::R2);
    d.run(4000);
    assert!(d.app.bar_items().contains(&Item::View(true)));
    golden("pc_bar_shelf_mouse", &d.frame);
    d.app.set_glyphs(Glyphs::Pad);
    d.run(50);
    golden("pc_bar_shelf_pad", &d.frame);
    // It is not there while the shelf's name is, or over MENU.
    d.tap(Button::R1);
    d.run(600);
    assert_eq!(d.app.bar_at(320.0, 404.0), None);
    d.run(4000);
    assert!(d.app.bar_at(320.0, 404.0).is_some());
    d.tap(Button::Menu);
    d.run(1000);
    assert_eq!(d.app.bar_at(320.0, 404.0), None);
}

#[test]
fn the_miyoo_has_no_bar() {
    let rig = Rig::new();
    let mut d = settled(&rig);
    for y in (300..480).step_by(2) {
        for x in (0..640).step_by(2) {
            assert_eq!(d.app.bar_at(x as f32, y as f32), None);
        }
    }
    // And its legends are not buttons.
    d.tap(Button::Menu);
    d.run(1000);
    d.app.pointer(Pointer::Click(40.0, 452.0));
    d.run(500);
    assert_eq!(d.app.phase, Phase::About);
}

#[test]
fn every_entry_clicks() {
    let rig = Rig::pc();
    let mut d = settled(&rig);
    d.app.set_glyphs(Glyphs::Mouse);
    d.run(50);
    // SHELF: its right half is the next shelf, its left the one before.
    let shelf = d.app.shelf_key();
    click(&mut d, Item::Shelf, true);
    d.run(4000);
    assert_ne!(d.app.shelf_key(), shelf);
    click(&mut d, Item::Shelf, false);
    d.run(4000);
    assert_eq!(d.app.shelf_key(), shelf);
    // GRID / ROW.
    assert!(d.app.grid_on());
    click(&mut d, Item::View(false), false);
    d.run(4000);
    assert!(!d.app.grid_on());
    click(&mut d, Item::View(true), false);
    d.run(4000);
    assert!(d.app.grid_on());
    // CORE opens the picker, whose legends are buttons too.
    click(&mut d, Item::Core, false);
    assert!(d.app.core.is_some());
    // Its faces are built on a thread in real time: wait for the scene,
    // then slot's 420 ms opening.
    d.wait_until("the core picker", |d| {
        d.app.core.as_ref().is_some_and(|c| c.shown())
    });
    d.run(500);
    golden("pc_core_picker_mouse", &d.frame);
    assert_eq!(d.app.bar_at(320.0, 404.0), None);
    d.app.pointer(Pointer::Click(30.0, 452.0));
    d.run(1500);
    assert!(d.app.core.is_none(), "BACK closes the picker");
    // STUDIO.
    click(&mut d, Item::Studio, false);
    d.run(1500);
    assert_eq!(d.app.phase, Phase::Studio);
    golden("pc_studio_mouse", &d.frame);
    d.app.pointer(Pointer::Click(30.0, 452.0));
    d.run(1500);
    assert_eq!(d.app.phase, Phase::Shelf, "its first legend is the way out");
    d.run(4000);
    // MENU, and its BACK.
    click(&mut d, Item::Menu, false);
    d.run(1000);
    assert_eq!(d.app.phase, Phase::About);
    golden("pc_menu_mouse", &d.frame);
    d.app.pointer(Pointer::Click(30.0, 452.0));
    d.run(1000);
    assert_eq!(d.app.phase, Phase::Shelf);
    d.run(4000);
    // PLAY: a click puts the cart in.
    let stem = d.selected();
    click(&mut d, Item::Play, false);
    assert!(matches!(&d.app.phase, Phase::Inserting { stem: s, .. } if *s == stem));
    d.wait_until("the cart to go in", |d| d.app.exit.is_some());
    assert!(matches!(
        &d.app.exit,
        Some(Exit::Launch { fresh: false, .. })
    ));
}

#[test]
fn holding_play_starts_the_game_from_the_beginning() {
    let rig = Rig::pc();
    let mut d = settled(&rig);
    let (x, y) = at(&d, Item::Play, false);
    d.app.pointer(Pointer::Click(x, y));
    d.run(3000);
    d.app.pointer(Pointer::Release);
    d.wait_until("the cart to go in", |d| d.app.exit.is_some());
    assert!(matches!(
        &d.app.exit,
        Some(Exit::Launch { fresh: true, .. })
    ));
}

#[test]
fn the_first_run_screen_has_its_offer_and_menu() {
    let rig = Rig::pc();
    std::fs::remove_dir_all(rig.paths.sd.join("Roms")).unwrap();
    let mut d = settled(&rig);
    assert_eq!(d.app.bar_items(), [Item::Import, Item::Menu]);
    golden("pc_bar_first_run_pad", &d.frame);
    d.app.set_glyphs(Glyphs::Mouse);
    d.run(50);
    golden("pc_bar_first_run_mouse", &d.frame);
    click(&mut d, Item::Import, false);
    assert_eq!(
        d.app.take_host_action(),
        Some(slot_mini::about::HostRow::Import)
    );
    click(&mut d, Item::Menu, false);
    d.run(1000);
    assert_eq!(d.app.phase, Phase::About);
}

#[test]
fn a_larger_frame_is_drawn_sharp_not_stretched() {
    let rig = Rig::pc();
    let mut d = settled(&rig);
    let mut big = Canvas::new(SCREEN_W * 2, SCREEN_H * 2);
    d.app.render(&mut big);
    golden("pc_grid_2x", &big);
    // Shrunk back it is the small frame, near enough; but it holds detail
    // the small frame doubled would not.
    let small = d.frame.to_rgba();
    let large = big.to_rgba();
    let px = |x: usize, y: usize| {
        let i = (y * (SCREEN_W as usize * 2) + x) * 4;
        [large[i], large[i + 1], large[i + 2]]
    };
    let mut detail = 0;
    let mut far = 0;
    for y in 0..SCREEN_H as usize {
        for x in 0..SCREEN_W as usize {
            let four = [
                px(2 * x, 2 * y),
                px(2 * x + 1, 2 * y),
                px(2 * x, 2 * y + 1),
                px(2 * x + 1, 2 * y + 1),
            ];
            if four.iter().any(|p| p != &four[0]) {
                detail += 1;
            }
            let i = (y * SCREEN_W as usize + x) * 4;
            let mean = |c: usize| four.iter().map(|p| p[c] as u32).sum::<u32>() / 4;
            if (0..3).any(|c| mean(c).abs_diff(small[i + c] as u32) > 48) {
                far += 1;
            }
        }
    }
    let all = (SCREEN_W * SCREEN_H) as usize;
    assert!(
        detail > all / 20,
        "only {detail} of {all} blocks have detail"
    );
    assert!(
        far < all / 20,
        "{far} of {all} blocks are not the small frame's"
    );
}

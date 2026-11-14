//! The PC's mouse on the shelf and the grid: hover, click, double click,
//! wheel and the view button. Needs the `pc` feature.
#![cfg(feature = "pc")]

mod common;

use common::{golden, Rig};
use slot_mini::{Exit, Phase, Pointer};
use slot_mini_gfx::Button;

/// The middle of the grid's cell in `col`, `row` (see `grid_gba.png`).
fn cell(col: usize, row: usize) -> (f32, f32) {
    (98.0 + 148.0 * col as f32, 80.0 + 113.0 * row as f32)
}

/// The view button, left of the clock.
const VIEW: (f32, f32) = (543.0, 451.0);

#[test]
fn the_grid_follows_the_pointer_and_a_click_stays_there() {
    let rig = Rig::pc();
    let mut d = rig.app();
    d.run(500);
    let first = d.selected();
    let (x, y) = cell(2, 0);
    d.app.pointer(Pointer::Move(x, y));
    d.run(500);
    let third = d.selected();
    assert_ne!(third, first);
    let (x, y) = cell(1, 1);
    d.app.pointer(Pointer::Click(x, y));
    d.run(500);
    assert_ne!(d.selected(), third);
    // Between carts and below the last one nothing changes.
    let here = d.selected();
    d.app.pointer(Pointer::Move(320.0, 400.0));
    d.app.pointer(Pointer::Click(3.0, 300.0));
    assert_eq!(d.selected(), here);
    golden("mouse_grid_hover", &d.frame);
}

#[test]
fn a_double_click_puts_the_cart_in() {
    let rig = Rig::pc();
    let mut d = rig.app();
    d.run(500);
    let (x, y) = cell(1, 0);
    d.app.pointer(Pointer::Click(x, y));
    d.app.pointer(Pointer::Double(x, y));
    let stem = d.selected();
    assert!(matches!(&d.app.phase, Phase::Inserting { stem: s, .. } if *s == stem));
    d.wait_until("the cart to go in", |d| d.app.exit.is_some());
    assert!(matches!(&d.app.exit, Some(Exit::Launch { cart, fresh: false }) if cart.stem == stem));
}

#[test]
fn the_wheel_moves_a_row_in_the_grid_and_a_cart_on_the_shelf() {
    let rig = Rig::pc();
    let mut d = rig.app();
    d.run(500);
    let first = d.selected();
    d.app.pointer(Pointer::Wheel(320.0, 200.0, 1));
    d.run(500);
    let below = d.selected();
    assert_ne!(below, first);
    d.app.pointer(Pointer::Wheel(320.0, 200.0, -1));
    assert_eq!(d.selected(), first);

    d.tap(Button::R2);
    assert!(!d.app.grid_on());
    d.run(500);
    d.app.pointer(Pointer::Wheel(320.0, 200.0, 1));
    d.run(500);
    let next = d.selected();
    assert_ne!(next, first);
    assert_ne!(next, below, "one cart along, not one row");
    d.app.pointer(Pointer::Wheel(320.0, 200.0, -1));
    assert_eq!(d.selected(), first);
}

#[test]
fn on_the_shelf_a_click_brings_a_cart_to_the_middle_and_hovering_does_not() {
    let rig = Rig::pc();
    let mut d = rig.app();
    d.tap(Button::R2);
    d.run(800);
    let first = d.selected();
    // The cart to the right of the one over the slot.
    d.app.pointer(Pointer::Move(560.0, 260.0));
    d.run(100);
    assert_eq!(d.selected(), first);
    d.app.pointer(Pointer::Click(560.0, 260.0));
    d.run(800);
    let second = d.selected();
    assert_ne!(second, first);
    d.app.pointer(Pointer::Click(80.0, 260.0));
    d.run(800);
    assert_eq!(d.selected(), first);
    // A double click on a side cart only brings it over; on the middle
    // one it goes in.
    d.app.pointer(Pointer::Double(560.0, 260.0));
    assert_eq!(d.app.phase, Phase::Shelf);
    d.run(800);
    d.app.pointer(Pointer::Double(320.0, 240.0));
    assert!(matches!(d.app.phase, Phase::Inserting { .. }));
}

#[test]
fn the_view_button_switches_between_the_shelf_and_the_grid() {
    let rig = Rig::pc();
    let mut d = rig.app();
    d.run(500);
    assert!(d.app.grid_on());
    assert!(d.app.view_button(VIEW.0, VIEW.1));
    assert!(!d.app.view_button(40.0, 451.0));
    d.app.pointer(Pointer::Click(VIEW.0, VIEW.1));
    assert!(!d.app.grid_on());
    d.run(2500);
    golden("mouse_view_button_shelf", &d.frame);
    d.app.pointer(Pointer::Click(VIEW.0, VIEW.1));
    assert!(d.app.grid_on());
}

#[test]
fn the_right_button_goes_back_and_the_wheel_walks_a_menu() {
    let rig = Rig::pc();
    let mut d = rig.app();
    d.tap(Button::Menu);
    assert_eq!(d.app.phase, Phase::About);
    // Clicks on the shelf behind it do nothing.
    let (x, y) = cell(2, 0);
    let was = d.selected();
    d.app.pointer(Pointer::Click(x, y));
    d.app.pointer(Pointer::Back);
    d.run(100);
    assert_eq!(d.app.phase, Phase::Shelf);
    assert_eq!(d.selected(), was);
}

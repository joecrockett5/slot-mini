//! The cart studio: shell colour, finish and shape written to slot's
//! Labels/cart_shell.txt, and the label printer's states.

mod common;

use common::{golden, Rig};
use slot_mini::Phase;
use slot_mini_gfx::Button;

#[test]
fn select_opens_the_studio_and_b_closes_it() {
    let rig = Rig::new();
    let mut d = rig.app();
    d.go_to("Metroid Fusion (USA)");
    d.tap(Button::Select);
    assert_eq!(d.app.phase, Phase::Studio);
    d.run(100);
    golden("studio_list", &d.frame);
    d.tap(Button::B);
    assert_eq!(d.app.phase, Phase::Shelf);
    assert!(
        !rig.paths.sd.join("Labels/cart_shell.txt").exists()
            || !std::fs::read_to_string(rig.paths.sd.join("Labels/cart_shell.txt"))
                .unwrap()
                .contains("Metroid"),
        "nothing changed, nothing written"
    );
}

#[test]
fn shell_colour_and_finish_are_written_in_slots_format() {
    let rig = Rig::new();
    let mut d = rig.app();
    d.go_to("Metroid Fusion (USA)");
    d.tap(Button::Select);
    // Rows: Label Printer, Label Color (a text label), Shell Color, Finish.
    d.tap(Button::Down);
    d.tap(Button::Down);
    d.tap(Button::A);
    d.run(50);
    golden("studio_chips", &d.frame);
    // Move off Current and Auto onto a preset, and back to the list.
    d.tap(Button::Down);
    d.tap(Button::B);
    d.tap(Button::Down);
    d.tap(Button::Right);
    d.run(50);
    golden("studio_finish", &d.frame);
    d.tap(Button::B);
    assert_eq!(d.app.phase, Phase::Shelf);
    let text = std::fs::read_to_string(rig.paths.sd.join("Labels/cart_shell.txt")).unwrap();
    let line = text
        .lines()
        .find(|l| l.starts_with("Metroid Fusion (USA) = "))
        .unwrap();
    let value = line.split_once(" = ").unwrap().1;
    let choice = slot_mini_store::ShellChoice::parse(value).expect("slot can read it back");
    assert_eq!(choice.finish, slot_mini_store::ShellFinish::Clear);
    // The other lines in the file are untouched.
    assert!(text.contains("Tetris (World) = notched 2f5cc0 glitter"));
    // And the shelf shows the new shell.
    d.run(500);
    assert_eq!(d.app.current().unwrap().1.shell, Some(choice));
    golden("shelf_after_studio", &d.frame);
}

#[test]
fn game_boy_carts_get_a_shape_row() {
    let rig = Rig::new();
    let mut d = rig.app();
    d.tap(Button::L1);
    d.go_to("Tetris (World)");
    d.tap(Button::Select);
    let rows = d.app.studio.as_ref().unwrap().studio.rows();
    assert_eq!(rows.last(), Some(&slot_mini::studio::Row::Shape));
    d.run(50);
    golden("studio_gb", &d.frame);
}

#[test]
fn the_printer_says_where_labels_come_from_when_the_service_is_off() {
    let rig = Rig::new();
    let mut d = rig.app();
    d.tap(Button::Select);
    d.tap(Button::A);
    d.run(50);
    assert!(matches!(
        d.app.studio.as_ref().unwrap().printer,
        slot_mini::studio_ui::Printer::Disabled
    ));
    golden("studio_printer_off", &d.frame);
}

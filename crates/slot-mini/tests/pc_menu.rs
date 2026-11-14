//! MENU on the PC: its own pages and rows, the mouse on them, and the
//! legends naming the keyboard's keys. Needs the `pc` feature.
#![cfg(feature = "pc")]

mod common;

use common::{golden, Rig};
use slot_mini::about::{Host, HostRow, Page, PageRow, Row};
use slot_mini::legend::Glyphs;
use slot_mini::{Phase, Pointer};
use slot_mini_gfx::Button;

fn host() -> Host {
    Host {
        version: "0.1.0".into(),
        fullscreen: false,
        xbox: false,
        retroarch: Some("C:\\RetroArch-Win64\\retroarch.exe".into()),
        library: "C:\\Users\\Joe\\slot-mini".into(),
        update: "Check for updates".into(),
        update_note: None,
        cores: None,
        cores_note: None,
        import: "Import from SD card".into(),
        import_note: None,
        page: None,
    }
}

fn menu(rig: &Rig) -> common::Driver {
    let mut d = rig.app();
    d.app.set_host(host());
    d.tap(Button::Menu);
    d.run(1000);
    assert_eq!(d.app.phase, Phase::About);
    d
}

fn rows(d: &common::Driver) -> Vec<Row> {
    d.app.about.as_ref().unwrap().rows().to_vec()
}

#[test]
fn the_first_page_is_the_pcs_and_the_sticker_is_too() {
    let rig = Rig::pc();
    let mut d = menu(&rig);
    assert_eq!(
        rows(&d),
        [
            Row::Settings,
            Row::Host(HostRow::Import),
            Row::Host(HostRow::Update),
            Row::Host(HostRow::Quit),
        ]
    );
    golden("pc_menu", &d.frame);
    // Nothing of the Miyoo's is offered, and choosing a PC row is handed
    // to the host.
    d.tap(Button::Down);
    d.tap(Button::Down);
    d.tap(Button::Down);
    d.tap(Button::A);
    assert_eq!(d.app.take_host_action(), Some(HostRow::Quit));
    assert_eq!(d.app.take_host_action(), None);
    assert_eq!(d.app.exit, None);
}

#[test]
fn settings_folders_and_sync_are_pages_b_walks_back_through() {
    let rig = Rig::pc();
    let mut d = menu(&rig);
    d.tap(Button::A);
    assert_eq!(
        rows(&d),
        [
            Row::Sounds,
            Row::Recents,
            Row::OnlineLabels,
            Row::Host(HostRow::Fullscreen),
            Row::Host(HostRow::Controller),
            Row::Host(HostRow::Folders),
            Row::Host(HostRow::Sync),
        ]
    );
    for _ in 0..4 {
        d.tap(Button::Down);
    }
    d.run(100);
    golden("pc_menu_settings", &d.frame);
    // A switch turns with Right as well as A; the host does the turning.
    d.tap(Button::Right);
    assert_eq!(d.app.take_host_action(), Some(HostRow::Controller));
    d.app.set_host(Host {
        xbox: true,
        ..host()
    });
    d.tap(Button::Down);
    d.tap(Button::A);
    assert_eq!(
        rows(&d),
        [Row::Host(HostRow::RetroArch), Row::Host(HostRow::Library)]
    );
    d.run(100);
    golden("pc_menu_folders", &d.frame);
    d.tap(Button::A);
    assert_eq!(d.app.take_host_action(), Some(HostRow::RetroArch));
    // A missing core adds its row while the page is open, and takes it
    // away again.
    d.app.set_host(Host {
        cores: Some("Get missing cores".into()),
        cores_note: Some("Not installed: gpSP.".into()),
        ..host()
    });
    assert_eq!(rows(&d).len(), 3);
    d.tap(Button::Up);
    d.run(100);
    golden("pc_menu_cores", &d.frame);
    d.app.set_host(host());
    assert_eq!(rows(&d).len(), 2);
    assert_eq!(d.app.about.as_ref().unwrap().row, 1);
    // B: back to settings, on the row it was left on.
    d.tap(Button::B);
    assert_eq!(rows(&d).len(), 7);
    assert_eq!(d.app.about.as_ref().unwrap().row, 5);
    d.tap(Button::Down);
    d.tap(Button::A);
    // Sync's page is the host's to lay out: MENU says the row was chosen.
    assert_eq!(d.app.take_host_action(), Some(HostRow::Sync));
    let page = Page {
        id: 1,
        rows: vec![
            PageRow::new("This PC: Desk").note("The name your Miyoo shows for this PC."),
            PageRow::new("Sync now"),
        ],
        ..Page::default()
    };
    d.app.set_host(Host {
        page: Some(page.clone()),
        ..host()
    });
    assert_eq!(
        rows(&d),
        [Row::Host(HostRow::Page(0)), Row::Host(HostRow::Page(1))]
    );
    d.run(100);
    golden("pc_menu_sync", &d.frame);
    d.tap(Button::Down);
    d.tap(Button::A);
    assert_eq!(d.app.take_host_action(), Some(HostRow::Page(1)));
    d.tap(Button::Left);
    assert_eq!(d.app.take_host_action(), Some(HostRow::PageTurn(1, false)));
    // A page with a title is a sheet of its own; the same id keeps the row.
    d.app.set_host(Host {
        page: Some(Page {
            title: Some("Pair with Miyoo Mini?".into()),
            big: Some("482913".into()),
            lines: vec!["Check that the Miyoo shows the same code.".into()],
            ..page.clone()
        }),
        ..host()
    });
    assert_eq!(d.app.about.as_ref().unwrap().row, 1);
    d.run(100);
    golden("pc_menu_sheet", &d.frame);
    // The mouse finds the sheet's rows where they are drawn.
    d.app.pointer(Pointer::Move(320.0, 190.0));
    assert_eq!(d.app.about.as_ref().unwrap().row, 0);
    // B is the host's too; it takes the page down and settings is back.
    d.tap(Button::B);
    assert_eq!(d.app.take_host_action(), Some(HostRow::PageBack));
    d.app.set_host(host());
    assert_eq!(rows(&d).len(), 7);
    assert_eq!(d.app.about.as_ref().unwrap().row, 6);
    d.tap(Button::B);
    assert_eq!(rows(&d).len(), 4);
    d.tap(Button::B);
    assert_eq!(d.app.phase, Phase::Shelf);
}

#[test]
fn the_mouse_selects_the_row_under_it_and_a_click_chooses() {
    let rig = Rig::pc();
    let mut d = menu(&rig);
    let about = d.app.about.as_ref().unwrap();
    let y = (0..480)
        .map(|y| y as f32)
        .find(|y| about.row_at(*y) == Some(2))
        .unwrap();
    assert_eq!(about.row_at(y - 28.0), Some(1));
    assert_eq!(about.row_at(10.0), None);
    assert_eq!(about.row_at(470.0), None);
    d.app.pointer(Pointer::Move(320.0, y + 10.0));
    assert_eq!(d.app.about.as_ref().unwrap().row, 2);
    d.app.pointer(Pointer::Click(320.0, y + 10.0));
    assert_eq!(d.app.take_host_action(), Some(HostRow::Update));
    // The wheel walks the rows and the right button goes back.
    d.app.pointer(Pointer::Wheel(320.0, 240.0, 1));
    assert_eq!(d.app.about.as_ref().unwrap().row, 3);
    d.app.pointer(Pointer::Back);
    d.run(50);
    assert_eq!(d.app.phase, Phase::Shelf);
}

#[test]
fn the_legends_name_the_keyboards_keys() {
    let rig = Rig::pc();
    let mut d = rig.app();
    d.app.set_host(host());
    d.app.set_glyphs(Glyphs::Keyboard);
    d.tap(Button::Menu);
    d.run(1000);
    golden("pc_menu_keyboard", &d.frame);
    // Picking up a controller changes them where they stand.
    d.app.set_glyphs(Glyphs::Pad);
    d.run(50);
    golden("pc_menu", &d.frame);
    d.app.set_glyphs(Glyphs::Keyboard);
    d.tap(Button::B);
    d.run(100);
    // The core picker (START on a GBA cart).
    d.tap(Button::Start);
    d.run(1500);
    assert!(d.app.core.is_some(), "the picker did not open");
    golden("pc_core_picker_keyboard", &d.frame);
    d.tap(Button::B);
    d.run(1500);
    // The cart studio.
    d.tap(Button::Select);
    d.run(1500);
    assert_eq!(d.app.phase, Phase::Studio);
    golden("pc_studio_keyboard", &d.frame);
}

#[test]
fn an_empty_library_shows_the_first_run_screen() {
    let rig = Rig::pc();
    std::fs::remove_dir_all(rig.paths.sd.join("Roms")).unwrap();
    let mut d = rig.app();
    d.app.set_host(host());
    d.run(300);
    assert!(d.app.shelf_kinds().is_empty());
    golden("pc_first_run", &d.frame);
    // The keyboard is named when it is the device in hand, and the import's
    // progress takes the place of the offer.
    d.app.set_glyphs(Glyphs::Keyboard);
    d.run(50);
    golden("pc_first_run_keyboard", &d.frame);
    d.tap(Button::A);
    assert_eq!(d.app.take_host_action(), Some(HostRow::Import));
    d.app.set_host(Host {
        import_note: Some("Importing: 12 of 40 games.".into()),
        ..host()
    });
    d.run(50);
    golden("pc_first_run_importing", &d.frame);
    // MENU still opens.
    d.tap(Button::Menu);
    d.run(1000);
    assert_eq!(d.app.phase, Phase::About);
}

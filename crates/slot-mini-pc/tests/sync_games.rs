//! Brief 3, phase 3: conflicts settled on the PC's screens and Remove
//! everywhere, against the real client.

mod common;

use common::{age, at_rest, named, pair, read, save, write, Miyoo, Rig};
use slot_mini::Pointer;
use slot_mini_gfx::Button;
use slot_mini_pc::pcapp::sync_ui::{Screen, Status};
use slot_mini_sync::{Scope, Side};

const WARS: &str = "Advance Wars (USA)";
const SUN: &str = "Golden Sun (USA, Europe)";

/// A PC and a Miyoo, paired, in step, the shelf at rest.
fn seeded(miyoo: &mut Miyoo) -> Rig {
    let rig = Rig::new("", miyoo.poke_port());
    pair(&rig, miyoo);
    let mut rig = rig;
    let address = rig.address();
    assert!(miyoo.sync(&address, Scope::Full).is_clean());
    rig.wait_until("the shelf to fill", |r| r.pc.app.current().is_some());
    rig.wait_until("the shelf to settle", at_rest);
    rig
}

fn marked(rig: &Rig) -> bool {
    let (_, cart) = rig.pc.app.current().expect("a cart");
    rig.pc.app.is_marked(cart)
}

#[test]
fn a_conflict_is_badged_swapped_and_settled_on_the_pc() {
    let mut miyoo = Miyoo::new();
    write(&miyoo.save(WARS), &save(1));
    let mut rig = seeded(&mut miyoo);
    let address = rig.address();
    let mine = rig.save("mGBA", WARS);
    let auto = rig
        .library()
        .join("Saves/CurrentProfile/states/mGBA")
        .join(format!("{WARS}.state.auto"));
    assert_eq!(read(&mine), save(1));

    // Played on both: the PC an hour ago, the Miyoo just now.
    write(&mine, &save(20));
    age(&mine, 3600);
    write(&auto, b"state of 20");
    write(&miyoo.save(WARS), &save(30));
    let report = miyoo.sync(&address, Scope::Full);
    assert_eq!(report.conflicts.len(), 1, "{report:?}");
    // The later save is in use on both; the PC's is set aside, and the
    // auto state that went with it too.
    assert_eq!(read(&mine), save(30));
    assert!(!auto.exists());
    rig.frames(4);
    assert_eq!(rig.pc.sync_status(), Some(Status::Conflict));
    assert!(rig
        .pc
        .app
        .notice()
        .is_some_and(|n| n.contains("1 conflict")));

    // The cart wears a badge, on the shelf and in the grid.
    rig.select(WARS);
    assert!(marked(&rig));
    rig.select(SUN);
    assert!(!marked(&rig));
    rig.select(WARS);
    rig.now += 10_000;
    rig.frames(40);
    let grid = rig.pc.app.grid_on();
    rig.golden(["sync_conflict_shelf", "sync_conflict_grid"][grid as usize]);
    rig.tap(Button::R2);
    rig.frames(60);
    rig.golden(["sync_conflict_shelf", "sync_conflict_grid"][!grid as usize]);
    rig.tap(Button::R2);
    rig.frames(60);

    // Settings, Sync, Conflicts: the game, and both copies.
    rig.open_sync();
    rig.choose("Conflicts: 1");
    assert_eq!(rig.rows(), [WARS]);
    let name = miyoo.client.device_name().to_string();
    assert_eq!(rig.note(), format!("Found syncing with {name}."));
    rig.golden("sync_conflicts");
    rig.choose(WARS);
    let lines = rig.page().lines;
    assert_eq!(lines[2], format!("In use: the copy from {name},"));
    assert_eq!(lines[3], "last played just now");
    assert_eq!(lines[4], "Set aside: the copy from this PC,");
    assert_eq!(lines[5], "last played 1 hour ago");
    rig.golden("sync_conflict");

    // Use the other copy: they change places. The auto state made since
    // belongs to the save that leaves, so it is set aside with it.
    write(&auto, b"state of 30");
    rig.choose("Use the other copy");
    assert_eq!(read(&mine), save(20));
    assert!(!auto.exists());
    let states = named(auto.parent().unwrap(), ".state.auto");
    assert_eq!(states.len(), 2, "{states:?}");
    let conflicts = rig.hub().conflicts();
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].from, Side::Client);
    let lines = rig.page().lines;
    assert_eq!(lines[2], "In use: the copy from this PC,");
    assert_eq!(lines[4], format!("Set aside: the copy from {name},"));
    assert!(rig.note().is_empty() || rig.note().contains("change places"));
    // The Miyoo gets the chosen save at its next sync.
    let report = miyoo.sync(&address, Scope::Full);
    assert!(report.is_clean(), "{report:?}");
    assert_eq!(report.to_client.updated, [format!("save:GBA/{WARS}.srm")]);
    assert_eq!(read(&miyoo.save(WARS)), save(20));
    rig.frames(4);
    assert_eq!(rig.pc.sync_status(), Some(Status::Conflict));

    // Keep this one asks first, on Cancel.
    rig.choose("Keep this one");
    assert_eq!(
        rig.page().title.as_deref(),
        Some("Keep the copy from this PC?")
    );
    assert_eq!(rig.rows(), ["Cancel", "Put the other copy away"]);
    rig.golden("sync_dismiss");
    rig.tap(Button::A);
    assert_eq!(rig.hub().conflicts().len(), 1, "Cancel settles nothing");
    rig.choose("Keep this one");
    rig.choose("Put the other copy away");
    assert!(rig.hub().conflicts().is_empty());
    // The copy is kept, out of the way.
    let away = named(&rig.library().join("Saves/slot-mini-removed"), "conflict-");
    assert_eq!(away.len(), 1, "{away:?}");
    assert_eq!(read(&away[0]), save(30));
    assert_eq!(read(&mine), save(20));
    // With no conflict left the list is gone, and so is the badge.
    assert_eq!(rig.pc.ui.screen(), Some(&Screen::Main));
    assert!(!rig.rows().iter().any(|r| r.starts_with("Conflicts")));
    assert_eq!(rig.pc.sync_status(), Some(Status::Idle));
    rig.tap(Button::Menu);
    rig.frames(4);
    assert!(!marked(&rig));
    assert!(miyoo.sync(&address, Scope::Full).is_noop());
}

#[test]
fn remove_everywhere_asks_then_reaches_the_miyoo() {
    let mut miyoo = Miyoo::new();
    write(&miyoo.save(SUN), &save(2));
    let mut rig = seeded(&mut miyoo);
    let address = rig.address();
    let rom = rig.library().join(format!("Roms/GBA/{SUN}.gba"));
    let label = rig.library().join(format!("Labels/GBA/{SUN}.png"));
    let mine = rig.save("mGBA", SUN);
    assert!(rom.is_file() && label.is_file() && mine.is_file());
    rig.select(SUN);
    rig.wait_until("the shelf to settle", at_rest);

    // MENU has a row for the game it was opened over.
    rig.tap(Button::Menu);
    rig.frames(60);
    rig.taps(&[Button::Down, Button::Down]);
    rig.golden("sync_menu_remove");
    rig.tap(Button::A);
    let page = rig.page();
    let title = page.title.clone().unwrap();
    assert!(
        title.starts_with("Remove Golden Sun") && title.ends_with(" everywhere?"),
        "{title}"
    );
    assert!(page
        .lines
        .contains(&"Saves/slot-mini-removed/GBA".to_string()));
    assert_eq!(rig.rows(), ["Cancel", "Remove everywhere"]);
    rig.golden("sync_remove");
    // Cancel is where the cursor starts, and leads back to MENU.
    rig.tap(Button::A);
    assert!(rom.is_file());
    assert!(rig
        .pc
        .app
        .about
        .as_ref()
        .is_some_and(|a| a.page().is_none()));
    rig.tap(Button::Menu);
    rig.wait_until("MENU to close", at_rest);

    // Delete asks the same, straight from the shelf; B puts it away.
    assert!(rig.pc.remove_current());
    rig.frames(60);
    assert_eq!(rig.rows(), ["Cancel", "Remove everywhere"]);
    rig.tap(Button::B);
    rig.wait_until("the sheet to close", at_rest);
    assert!(rig.pc.ui.screen().is_none());

    // So does a right click on the cart.
    let mut asked = false;
    'find: for y in (40..300).step_by(30) {
        for x in (40..620).step_by(30) {
            let (x, y) = (x as f32, y as f32);
            rig.pc.pointer(Pointer::Move(x, y));
            rig.frame();
            if rig.pc.app.current().is_some_and(|(_, c)| c.stem == SUN) {
                rig.wait_until("the cart to settle", at_rest);
                rig.pc.pointer(Pointer::Context(x, y));
                rig.frames(60);
                asked = rig.pc.ui.screen().is_some();
                if asked {
                    break 'find;
                }
            }
        }
    }
    assert!(asked, "no right click reached the cart");
    rig.choose("Remove everywhere");
    // It goes from the PC at once: the game, its label, and the save to
    // where removed saves are kept.
    assert!(!rom.exists() && !label.exists() && !mine.exists());
    let kept = named(&rig.library().join("Saves/slot-mini-removed/GBA"), SUN);
    assert_eq!(kept.len(), 1, "{kept:?}");
    assert_eq!(read(&kept[0]), save(2));
    assert!(rig
        .pc
        .app
        .notice()
        .is_some_and(|n| n.starts_with("Removed")));
    rig.wait_until("the cart to leave the shelf", |r| {
        at_rest(r) && r.pc.ui.screen().is_none() && !r.pc.app.has_cart(SUN)
    });
    // MENU no longer offers a game that is gone.
    assert!(rig.pc.app.current().is_some_and(|(_, c)| c.stem != SUN));

    // And from the Miyoo at its next sync, the save kept there too.
    let theirs = miyoo.sd.join(format!("Roms/GBA/{SUN}.gba"));
    assert!(theirs.is_file());
    let report = miyoo.sync(&address, Scope::Full);
    assert!(report.is_clean(), "{report:?}");
    assert!(!report.removed.is_empty(), "{report:?}");
    assert!(!theirs.exists());
    assert!(!miyoo.save(SUN).exists());
    let kept = named(&miyoo.sd.join("Saves/slot-mini-removed/GBA"), SUN);
    assert_eq!(kept.len(), 1, "{kept:?}");
    // Nothing brings it back.
    assert!(miyoo.sync(&address, Scope::Full).is_noop());
    assert!(!rom.exists());
}

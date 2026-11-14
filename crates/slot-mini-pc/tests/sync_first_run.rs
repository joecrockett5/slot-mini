//! Brief 3, phase 4: from an empty PC library to a seeded one through the
//! first-run screen, and what a later sync says and refreshes.

mod common;

use common::{at_rest, read, write, Miyoo, Rig};
use slot_mini::app::bar::Item;
use slot_mini_gfx::Button;
use slot_mini_pc::pcapp::sync_ui::{Pairing, Screen};
use slot_mini_pc::pcapp::Request;
use slot_mini_sync::{Event, PairingStatus, Scope};

const WARS: &str = "Advance Wars (USA)";
const SUN: &str = "Golden Sun (USA, Europe)";

fn files(dir: &std::path::Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map(|d| {
            d.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

#[test]
fn an_empty_library_is_seeded_from_the_first_run_screen() {
    let mut miyoo = Miyoo::new();
    write(&miyoo.save(WARS), &common::save(5));
    let mut rig = Rig::new("", miyoo.poke_port());
    let address = rig.address();
    let name = miyoo.client.device_name().to_string();
    rig.frames(60);

    // Nothing to play yet: the first thing offered is pairing, with the
    // import beside it.
    assert!(rig.pc.app.shelf_kinds().is_empty());
    let lines = rig.pc.app.welcome().to_vec();
    assert_eq!(lines[1], "Pair your Miyoo to bring its library over.");
    assert!(lines[2].contains(" to pair: "), "{lines:?}");
    assert!(lines[3].contains("import its SD card"), "{lines:?}");
    assert_eq!(
        rig.pc.app.bar_items(),
        [Item::Pair(false), Item::Card, Item::Menu]
    );
    rig.shot("sync_first_run");
    rig.tap(Button::Start);
    assert_eq!(rig.take(), [Request::PickCard]);
    rig.pc.chose_card(None);

    // A: the pairing screen, straight from the shelf.
    rig.tap(Button::A);
    rig.frames(60);
    assert_eq!(rig.pc.ui.screen(), Some(&Screen::Pairing));
    let attempt = miyoo.ask(&address);
    rig.wait_until("the code", |r| r.page().big.is_some());
    rig.choose("Confirm");
    assert_eq!(miyoo.confirm(&address, &attempt), PairingStatus::Paired);
    rig.wait_until("the pairing", |r| {
        matches!(r.pc.ui.pairing, Pairing::Paired(_))
    });
    // It asks for Sync now on the Miyoo, and Done goes back to the shelf,
    // which says the same.
    assert!(rig.page().lines[0].contains("Sync now on the Miyoo"));
    rig.choose("Done");
    rig.wait_until("MENU to close", at_rest);
    assert!(rig.pc.ui.screen().is_none());
    let lines = rig.pc.app.welcome().to_vec();
    assert_eq!(lines[1], format!("Paired with {name}."));
    assert_eq!(
        lines[2],
        "Choose Sync now on the Miyoo to bring its library over."
    );
    assert_eq!(
        rig.pc.app.bar_items(),
        [Item::Pair(true), Item::Card, Item::Menu]
    );
    rig.shot("sync_first_run_paired");
    // A is the Sync page now; B puts it away.
    rig.tap(Button::A);
    rig.frames(60);
    assert_eq!(rig.pc.ui.screen(), Some(&Screen::Main));
    assert_eq!(rig.rows()[1], "Sync now");
    rig.tap(Button::B);
    rig.wait_until("MENU to close", at_rest);
    assert!(rig.pc.ui.screen().is_none());

    // The Miyoo's first sync: the screen says how far it is.
    let mut seen = Vec::new();
    let report = miyoo.sync_events(&address, Scope::Full, &mut |event| {
        if let Event::Transfer { index, done: 0, .. } = event {
            rig.frames(2);
            if index == 3 {
                seen = rig.pc.app.welcome().to_vec();
                rig.shot("sync_first_run_seeding");
            }
        }
    });
    assert!(report.is_clean(), "{report:?}");
    let line = seen.last().expect("progress while it ran");
    assert!(line.starts_with(&format!("From {name}: ")), "{seen:?}");
    assert!(line.contains(" of "), "{line}");

    // Seeded: the shelves fill without a restart and the shelf says what
    // came.
    rig.wait_until("the summary", |r| {
        r.pc.ui.progress.is_none() && r.pc.ui.last.is_some()
    });
    rig.wait_until("the shelf to fill", |r| r.pc.app.current().is_some());
    rig.wait_until("the shelf to settle", at_rest);
    assert!(rig.pc.app.welcome().is_empty());
    let said = rig.pc.app.notice().expect("a summary").to_string();
    assert!(
        said.starts_with(&format!("Synced with {name}: ")) && said.contains("games"),
        "{said}"
    );
    assert!(said.contains("1 save in"), "{said}");
    rig.frames(4);
    rig.shot("sync_first_run_seeded");
    for system in ["GBA", "GBC", "GB"] {
        let dir = format!("Roms/{system}");
        let theirs: Vec<String> = files(&miyoo.sd.join(&dir))
            .into_iter()
            .filter(|n| n.contains(".gb"))
            .collect();
        let mine: Vec<String> = files(&rig.library().join(&dir))
            .into_iter()
            .filter(|n| theirs.contains(n))
            .collect();
        assert!(!theirs.is_empty());
        assert_eq!(mine, theirs, "{system}");
    }
    assert_eq!(read(&rig.save("mGBA", WARS)), common::save(5));
    assert!(rig.pc.app.has_cart(WARS) && rig.pc.app.has_cart(SUN));

    // A sync with nothing to move says so only on the Sync page.
    rig.now += 10_000;
    rig.frames(30);
    assert!(miyoo.sync(&address, Scope::Full).is_noop());
    rig.frames(4);
    assert_eq!(rig.pc.app.notice(), Some(said.as_str()), "no new notice");
    assert_eq!(
        rig.pc.ui.last.as_deref(),
        Some(format!("In step with {name}").as_str())
    );

    // A label changed on the Miyoo: the cart on the PC's shelf wears the
    // new one after the sync, with no restart.
    rig.select(SUN);
    rig.now += 10_000;
    rig.frames(40);
    rig.pc.render();
    let before = rig.pc.frame.to_rgba();
    let label = |stem: &str| miyoo.sd.join(format!("Labels/GBA/{stem}.png"));
    std::fs::copy(label("Pokemon - Emerald Version (USA, Europe)"), label(SUN)).unwrap();
    common::age(&label(SUN), 60);
    let report = miyoo.sync(&address, Scope::Full);
    assert!(report.is_clean(), "{report:?}");
    assert_eq!(report.to_hub.updated.len(), 1, "{report:?}");
    assert!(report.to_hub.updated[0].starts_with("label:GBA/Golden Sun"));
    rig.frames(4);
    assert_eq!(
        rig.pc.app.notice(),
        Some(format!("Synced with {name}: labels in").as_str())
    );
    rig.now += 10_000;
    rig.frames(60);
    rig.wait_until("the shelf to settle", at_rest);
    assert!(rig.pc.app.current().is_some_and(|(_, c)| c.stem == SUN));
    rig.pc.render();
    let after = rig.pc.frame.to_rgba();
    let changed = before
        .chunks_exact(4)
        .zip(after.chunks_exact(4))
        .filter(|(a, b)| (0..3).any(|i| a[i].abs_diff(b[i]) > 24))
        .count();
    assert!(changed > 2000, "only {changed} pixels changed");
    rig.shot("sync_first_run_relabelled");
}

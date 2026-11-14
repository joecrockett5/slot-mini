//! Brief 3, phase 2: the Sync page, pairing through the PC's screens, and
//! the status icon, against the real client.

mod common;

use common::{at_rest, pair, Miyoo, Rig};
use slot_mini_gfx::Button;
use slot_mini_pc::pcapp::sync_ui::{Pairing, Progress, Screen, Status};
use slot_mini_sync::{PairingStatus, Scope};

/// A port nothing listens on: pokes sent there are never answered.
fn deaf_port() -> u16 {
    let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    socket.local_addr().unwrap().port()
}

/// Show `code` in place of the real one (which is random) for a picture.
fn picture_with_code(rig: &mut Rig, name: &str, code: &str) {
    let real = rig.pc.ui.pairing.clone();
    rig.pc.ui.pairing = match &real {
        Pairing::Code { device, .. } => Pairing::Code {
            device: device.clone(),
            code: code.into(),
        },
        Pairing::Confirmed { device, .. } => Pairing::Confirmed {
            device: device.clone(),
            code: code.into(),
        },
        other => other.clone(),
    };
    rig.pc.refresh_host();
    rig.golden(name);
    rig.pc.ui.pairing = real;
    rig.pc.refresh_host();
    rig.frame();
}

#[test]
fn a_miyoo_pairs_through_the_pcs_screens() {
    let mut miyoo = Miyoo::new();
    let mut rig = Rig::new("", miyoo.poke_port());
    let address = rig.address();

    // Settings, Sync: nothing is paired yet.
    rig.open_sync();
    assert_eq!(
        rig.rows(),
        [
            "This PC: Desk",
            "Sync now",
            "Pair a Miyoo",
            "Miyoo can't find this PC?"
        ]
    );
    rig.tap(Button::Down);
    assert_eq!(rig.note(), "Pair a Miyoo first.");
    rig.golden("sync_page_unpaired");

    // A Miyoo that asks before the PC is on its pairing screen is refused.
    let mut link = slot_mini_sync::HttpLink::new(&address);
    assert!(miyoo.client.pair(&mut link).is_err());

    // Pair a Miyoo: the PC waits, saying which name to look for.
    rig.choose("Pair a Miyoo");
    assert_eq!(rig.pc.ui.screen(), Some(&Screen::Pairing));
    assert_eq!(rig.page().title.as_deref(), Some("Pair a Miyoo"));
    assert!(rig.page().lines.contains(&"Desk".to_string()));
    rig.golden("sync_pairing_waiting");

    // The Miyoo asks: both screens show the same code.
    let attempt = miyoo.ask(&address);
    rig.wait_until("the code", |r| r.page().big.is_some());
    let page = rig.page();
    assert_eq!(page.big.as_deref(), Some(attempt.code.as_str()));
    assert_eq!(
        page.title,
        Some(format!("Pair with {}?", miyoo.client.device_name()))
    );
    assert_eq!(rig.rows(), ["Confirm", "Deny"]);
    picture_with_code(&mut rig, "sync_pairing_code", "482913");
    // Nothing is paired until the PC's player says so.
    assert_eq!(miyoo.confirm(&address, &attempt), PairingStatus::Waiting);
    assert!(rig.hub().devices().is_empty());

    rig.choose("Confirm");
    assert!(matches!(rig.pc.ui.pairing, Pairing::Confirmed { .. }));
    assert_eq!(rig.rows(), ["Cancel"]);
    picture_with_code(&mut rig, "sync_pairing_confirmed", "482913");
    assert_eq!(miyoo.confirm(&address, &attempt), PairingStatus::Paired);
    rig.wait_until("the pairing", |r| {
        matches!(r.pc.ui.pairing, Pairing::Paired(_))
    });
    assert_eq!(rig.hub().devices().len(), 1);
    // The PC asks for the first sync from the Miyoo.
    let page = rig.page();
    assert!(page.lines[0].contains("Sync now on the Miyoo"), "{page:?}");
    assert_eq!(rig.rows(), ["Done"]);
    rig.golden("sync_paired");

    // That sync shows on the same screen as it runs and when it ends.
    let mut seen = Vec::new();
    let mut link = slot_mini_sync::HttpLink::new(&address);
    let report = miyoo.client.sync(
        &mut link,
        Scope::Full,
        &mut |event| {
            if let slot_mini_sync::Event::Transfer { index, .. } = event {
                rig.frames(2);
                if index == 3 {
                    seen = rig.page().lines;
                    rig.golden("sync_paired_seeding");
                }
            }
        },
        &slot_mini_sync::Cancel::new(),
    );
    assert!(report.is_clean(), "{report:?}");
    let line = seen.last().expect("progress while it ran");
    assert!(
        line.starts_with(&format!("From {}: ", miyoo.client.device_name())),
        "{line}"
    );
    assert!(line.contains(" of "), "{line}");
    rig.wait_until("the summary", |r| r.pc.ui.progress.is_none());
    let line = rig.page().lines.last().cloned().unwrap();
    assert!(line.starts_with("Synced with "), "{line}");
    assert!(line.contains("games"), "{line}");
    rig.golden("sync_paired_seeded");

    // Done: the Sync page lists the device now.
    rig.choose("Done");
    assert_eq!(
        rig.rows(),
        [
            "This PC: Desk",
            "Sync now",
            "Pair a Miyoo",
            "Devices: 1",
            "Miyoo can't find this PC?"
        ]
    );
    rig.golden("sync_page");
    // MENU closes; the shelf has the Miyoo's games and says what came.
    rig.tap(Button::Menu);
    rig.wait_until("the shelf to fill", |r| r.pc.app.current().is_some());
    assert_eq!(rig.pc.sync_status(), Some(Status::Idle));
    rig.golden("sync_shelf_idle");
}

#[test]
fn deny_leaves_the_miyoo_unpaired_and_devices_can_be_unpaired() {
    let mut miyoo = Miyoo::new();
    let mut rig = Rig::new("", miyoo.poke_port());
    let address = rig.address();
    rig.open_sync();
    rig.choose("Pair a Miyoo");
    let attempt = miyoo.ask(&address);
    rig.wait_until("the code", |r| r.page().big.is_some());
    rig.choose("Deny");
    assert_eq!(miyoo.confirm(&address, &attempt), PairingStatus::Rejected);
    assert!(rig.hub().devices().is_empty());
    assert_eq!(rig.page().title.as_deref(), Some("Not paired"));
    rig.golden("sync_pairing_denied");
    // Try again: the PC waits once more, and this time B backs out,
    // which ends pairing mode.
    rig.choose("Try again");
    assert_eq!(rig.page().title.as_deref(), Some("Pair a Miyoo"));
    rig.tap(Button::B);
    assert_eq!(rig.pc.ui.screen(), Some(&Screen::Main));
    let mut link = slot_mini_sync::HttpLink::new(&address);
    assert!(miyoo.client.pair(&mut link).is_err());

    // Paired, the device has a page of its own.
    pair(&rig, &mut miyoo);
    assert!(miyoo.sync(&address, Scope::Full).is_clean());
    rig.frames(4);
    rig.choose("Devices: 1");
    let name = miyoo.client.device_name().to_string();
    assert_eq!(rig.rows(), std::slice::from_ref(&name));
    assert_eq!(rig.note(), "Last sync: just now");
    rig.golden("sync_devices");
    rig.choose(&name);
    let page = rig.page();
    assert_eq!(page.title, Some(name.clone()));
    assert_eq!(
        page.lines,
        [
            "Paired: just now",
            "Last sync: just now",
            "Last address: 127.0.0.1"
        ]
    );
    rig.golden("sync_device");
    // Unpair asks first, and Cancel is where the cursor starts.
    rig.choose("Unpair");
    assert_eq!(rig.page().title, Some(format!("Unpair {name}?")));
    assert_eq!(rig.rows(), ["Cancel", "Unpair"]);
    rig.golden("sync_unpair");
    rig.tap(Button::A);
    assert_eq!(rig.hub().devices().len(), 1, "Cancel unpairs nothing");
    rig.choose("Unpair");
    rig.choose("Unpair");
    assert!(rig.hub().devices().is_empty());
    // With nothing left to list, the Sync page is back.
    assert_eq!(rig.pc.ui.screen(), Some(&Screen::Main));
    assert!(!rig.rows().iter().any(|r| r.starts_with("Devices")));
    // The Miyoo's next sync is turned away.
    assert!(miyoo.sync(&address, Scope::Full).interrupted.is_some());
}

#[test]
fn the_pcs_name_is_typed_and_paired_miyoos_stay_paired() {
    let mut miyoo = Miyoo::new();
    let mut rig = Rig::new("", miyoo.poke_port());
    pair(&rig, &mut miyoo);
    let id = rig.hub().id().to_string();
    rig.open_sync();
    rig.choose("This PC: Desk");
    assert!(rig.pc.wants_text());
    assert_eq!(rig.rows(), ["Desk_", "Save", "Cancel"]);
    for _ in 0..4 {
        rig.pc.backspace();
    }
    // An empty name is refused, and the editor stays.
    rig.tap(Button::A);
    assert!(rig.pc.wants_text());
    rig.pc.text("Attic PC\n");
    // Left rubs out, for a controller.
    rig.pc.text("x");
    rig.tap(Button::Left);
    rig.frame();
    assert_eq!(rig.rows()[0], "Attic PC_");
    rig.golden("sync_name");
    rig.tap(Button::A);
    assert!(!rig.pc.wants_text());
    assert_eq!(rig.rows()[0], "This PC: Attic PC");
    assert_eq!(rig.hub().name(), "Attic PC");
    assert_eq!(rig.hub().id(), id);
    // The server came back under the new name, and the Miyoo still syncs.
    let address = rig.address();
    assert!(miyoo.sync(&address, Scope::Full).is_clean());
    assert_eq!(rig.hub().devices().len(), 1);
    // Cancel changes nothing.
    rig.choose("This PC: Attic PC");
    rig.pc.text("!!");
    rig.choose("Cancel");
    assert_eq!(rig.hub().name(), "Attic PC");
}

#[test]
fn sync_now_says_when_no_miyoo_answers_and_shows_one_that_does() {
    let mut miyoo = Miyoo::new();
    // Pokes go where nobody listens: a Miyoo asleep, or in a game.
    let mut rig = Rig::new("", deaf_port());
    let address = rig.address();
    pair(&rig, &mut miyoo);
    assert!(miyoo.sync(&address, Scope::Full).is_clean());
    rig.wait_until("the shelf to fill", |r| r.pc.app.current().is_some());
    rig.wait_until("the shelf to settle", at_rest);
    rig.open_sync();
    rig.choose("Sync now");
    let name = miyoo.client.device_name().to_string();
    assert_eq!(rig.note(), format!("Asking {name}..."));
    rig.now += 9_000;
    rig.frames(2);
    assert!(rig.note().starts_with("No answer."), "{}", rig.note());
    rig.golden("sync_no_answer");
    rig.taps(&[Button::Menu]);

    // A Miyoo on its shelf hears the poke and syncs; the PC shows it.
    let mut rig = Rig::new("", miyoo.poke_port());
    let address = rig.address();
    pair(&rig, &mut miyoo);
    assert!(miyoo.sync(&address, Scope::Full).is_clean());
    rig.wait_until("the shelf to fill", |r| r.pc.app.current().is_some());
    rig.wait_until("the shelf to settle", at_rest);
    rig.open_sync();
    rig.choose("Sync now");
    let poke = miyoo.ear.wait().expect("the poke");
    assert!(miyoo.client.accept_poke(&poke));
    assert!(miyoo.sync(&address, Scope::Full).is_noop());
    rig.frames(4);
    rig.now += 9_000;
    rig.frames(2);
    assert_eq!(rig.note(), format!("In step with {name}"));
}

#[test]
fn the_status_icon_says_what_sync_is_doing() {
    let mut miyoo = Miyoo::new();
    let mut rig = Rig::new("", miyoo.poke_port());
    let address = rig.address();
    // Nothing paired: no icon.
    assert_eq!(rig.pc.sync_status(), None);
    pair(&rig, &mut miyoo);
    assert!(miyoo.sync(&address, Scope::Full).is_clean());
    rig.wait_until("the shelf to fill", |r| r.pc.app.current().is_some());
    rig.wait_until("the shelf to settle", at_rest);
    assert_eq!(rig.pc.sync_status(), Some(Status::Idle));

    // A sync under way counts its items (once the last one's summary
    // has left the shelf).
    rig.now += 10_000;
    rig.frames(30);
    rig.pc.ui.progress = Some(Progress {
        device: "Miyoo".into(),
        item: Some((3, 12)),
        text: String::new(),
        bytes: 0,
        at: rig.now,
    });
    rig.pc.refresh_host();
    assert_eq!(rig.pc.sync_status(), Some(Status::Syncing(Some((3, 12)))));
    rig.golden("sync_shelf_syncing");
    // One that goes quiet is given up on, and says so.
    rig.now += 60_000;
    rig.frames(2);
    assert_eq!(rig.pc.sync_status(), Some(Status::Failed));
    assert!(rig.pc.app.notice().is_some_and(|n| n.contains("stopped")));
    rig.frames(30);
    rig.golden("sync_shelf_failed");
    // The next sync that ends well clears it.
    assert!(miyoo.sync(&address, Scope::Full).is_noop());
    rig.frames(4);
    assert_eq!(rig.pc.sync_status(), Some(Status::Idle));

    // The firewall's story is a page of its own.
    rig.open_sync();
    rig.choose("Miyoo can't find this PC?");
    assert_eq!(rig.rows(), ["Back"]);
    rig.golden("sync_firewall");
    rig.choose("Back");
    assert_eq!(rig.pc.ui.screen(), Some(&Screen::Main));
    // MENU closing takes the pages down with it.
    rig.tap(Button::Menu);
    rig.frames(2);
    assert_eq!(rig.pc.ui.screen(), None);
}

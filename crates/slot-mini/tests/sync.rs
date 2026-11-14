//! Sync on the Miyoo against brief 1's hub, served over localhost in this
//! process: pairing through MENU, Sync now, the automatic syncs and the
//! pull beside a launch. Nothing here leaves the machine.

mod common;

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use common::{golden, Driver, Rig};
use slot_mini::app::Phase;
use slot_mini::status::FixedStatus;
use slot_mini::sync::{self, Icon, Progress, Stage};
use slot_mini::Exit;
use slot_mini_gfx::Button;
use slot_mini_store::{Favourites, Platform};
use slot_mini_sync::{
    Cancel, Client, FixedCore, HttpLink, Hub, Layout, PairingStatus, Scope, Server,
};

const EMERALD: &str = "Pokemon - Emerald Version (USA, Europe)";

fn local() -> SocketAddr {
    "127.0.0.1:0".parse().unwrap()
}

/// slot-mini PC's side: a library, its hub, and the hub's server.
struct Desk {
    root: PathBuf,
    hub: Arc<Hub>,
    server: Option<Server>,
}

impl Desk {
    fn new(rig: &Rig) -> Desk {
        let root = rig.dir.path().join("pc");
        std::fs::create_dir_all(&root).unwrap();
        let layout = Layout::pc(
            &root,
            &root.join("RetroArch/saves"),
            &root.join("RetroArch/states"),
            Arc::new(FixedCore::default()),
        );
        let hub = Arc::new(Hub::open(layout, "Desk").unwrap());
        let server = Server::start(hub.clone(), local(), Some(local())).unwrap();
        Desk {
            root,
            hub,
            server: Some(server),
        }
    }

    fn server(&self) -> &Server {
        self.server.as_ref().unwrap()
    }

    /// Where the Miyoo looks for PCs and listens for pokes.
    fn options(&self) -> sync::Options {
        sync::Options {
            discover: self.server().discovery_addr().into_iter().collect(),
            poke: Some(local()),
        }
    }

    /// The PC goes away.
    fn unplug(&mut self) {
        self.server = None;
    }

    fn layout(&self) -> &Layout {
        self.hub.layout()
    }

    /// Pair the card as the Sync page would, the PC's user saying yes.
    fn pair(&self, rig: &Rig) {
        let mut client = Client::open(sync::layout(&rig.paths.sd)).unwrap();
        let mut link = HttpLink::new(&self.server().addr().to_string());
        self.hub.start_pairing();
        let attempt = client.pair(&mut link).unwrap();
        assert_eq!(
            client.finish_pairing(&mut link, &attempt).unwrap(),
            PairingStatus::Waiting
        );
        assert!(self.hub.confirm_pairing());
        assert_eq!(
            client.finish_pairing(&mut link, &attempt).unwrap(),
            PairingStatus::Paired
        );
        self.hub.stop_pairing();
    }

    /// Pair, then sync everything once, so both sides hold the same games.
    fn pair_and_seed(&self, rig: &Rig) {
        self.pair(rig);
        let mut client = Client::open(sync::layout(&rig.paths.sd)).unwrap();
        let mut link = HttpLink::new(&self.server().addr().to_string());
        let report = client.sync(&mut link, Scope::Full, &mut |_| {}, &Cancel::new());
        assert!(report.is_clean(), "{report:?}");
    }
}

fn online() -> FixedStatus {
    FixedStatus {
        battery: Some(76),
        wifi: true,
        clock: (12, 0),
        charging: false,
    }
}

fn write(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

/// Make a file look written `secs` ago.
fn age(path: &Path, secs: u64) {
    let file = std::fs::File::options().write(true).open(path).unwrap();
    file.set_modified(SystemTime::now() - Duration::from_secs(secs))
        .unwrap();
}

fn save(fill: u8) -> Vec<u8> {
    let mut bytes = vec![fill; 64];
    bytes[0] = fill.wrapping_add(1);
    bytes
}

fn rom(len: usize, fill: u8) -> Vec<u8> {
    (0..len).map(|n| (n * 31 % 251) as u8 ^ fill).collect()
}

fn stage(d: &Driver) -> Stage {
    d.app.sync.view().stage
}

/// Wait for the sync worker to have nothing left to do.
fn settle(d: &mut Driver) {
    d.wait_until("the sync to end", |d| !d.app.sync.busy());
    d.run(32);
}

/// MENU -> Settings -> Sync.
fn open_sync_page(d: &mut Driver) {
    d.tap(Button::Menu);
    d.run(700);
    d.tap(Button::A);
    d.tap(Button::Up);
    d.tap(Button::A);
}

/// A paired card and PC that already agree, the app on its shelf.
fn paired(rig: &Rig, desk: &Desk) -> Driver {
    desk.pair_and_seed(rig);
    let mut d = rig.app_sync(online(), desk.options());
    d.run(32);
    settle(&mut d);
    assert!(d.app.sync.last_report().unwrap().is_clean());
    d
}

#[test]
fn an_unpaired_miyoo_does_nothing() {
    let rig = Rig::new();
    let desk = Desk::new(&rig);
    let mut d = rig.app_sync(online(), desk.options());
    d.run(2000);
    assert!(!d.app.sync.paired() && !d.app.sync.busy());
    assert_eq!(d.app.sync.icon(), None);
    assert_eq!(d.app.sync.poke_port(), None, "no listener while unpaired");
    assert!(!sync::sync_dir(&rig.paths).exists(), "nothing written");
    // A launch is the same launch.
    d.tap(Button::A);
    d.run(800);
    assert!(matches!(d.app.exit, Some(Exit::Launch { .. })));
    assert!(!sync::sync_dir(&rig.paths).exists());
    assert!(desk.hub.devices().is_empty());
}

#[test]
fn pairing_from_the_sync_page() {
    let rig = Rig::new();
    let desk = Desk::new(&rig);
    let mut d = rig.app_sync(online(), desk.options());
    d.tap(Button::Menu);
    d.run(700);
    d.tap(Button::A);
    // The Sync row is the page's last; Up from the first row wraps to it.
    d.tap(Button::Up);
    golden("about_settings_sync_row", &d.frame);
    d.tap(Button::A);
    golden("sync_page_unpaired", &d.frame);

    desk.hub.start_pairing();
    d.tap(Button::A);
    d.wait_until("the PCs found", |d| matches!(stage(d), Stage::Hubs(_)));
    let Stage::Hubs(found) = stage(&d) else {
        unreachable!()
    };
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].name, "Desk");
    golden("sync_page_hubs", &d.frame);

    d.tap(Button::A);
    d.wait_until("the code", |d| matches!(stage(d), Stage::Code { .. }));
    let Stage::Code { hub, code } = stage(&d) else {
        unreachable!()
    };
    assert_eq!((hub.as_str(), code.len()), ("Desk", 6));
    assert!(!d.app.sync.paired(), "not paired before both say yes");

    d.tap(Button::A);
    assert!(matches!(stage(&d), Stage::Confirming { .. }));
    // The PC's user agrees.
    let start = Instant::now();
    while !desk.hub.confirm_pairing() {
        assert!(start.elapsed() < Duration::from_secs(30));
        std::thread::sleep(Duration::from_millis(5));
    }
    d.wait_until("pairing", |d| matches!(stage(d), Stage::Paired(_)));
    assert!(d.app.sync.paired());
    assert_eq!(sync::paired(&rig.paths).as_deref(), Some("Desk"));
    assert_eq!(desk.hub.devices().len(), 1);
    golden("sync_page_paired_done", &d.frame);

    // A returns to the page, which now has the PC, Auto sync and Unpair.
    d.tap(Button::A);
    assert_eq!(stage(&d), Stage::Idle);
    golden("sync_page_paired", &d.frame);
    d.tap(Button::Down);
    assert!(d.app.settings.auto_sync);
    d.tap(Button::A);
    assert!(!d.app.settings.auto_sync);
    let text = std::fs::read_to_string(rig.paths.data().join("settings.txt")).unwrap();
    assert!(text.contains("auto_sync = off"), "{text}");
    d.tap(Button::A);
    assert!(d.app.settings.auto_sync);

    // B, B: the first page has Sync now while paired.
    d.tap(Button::B);
    d.tap(Button::B);
    golden("about_paired", &d.frame);

    // Unpair asks twice.
    d.tap(Button::A);
    d.tap(Button::Up);
    d.tap(Button::A);
    d.tap(Button::Up);
    d.tap(Button::A);
    assert!(d.app.sync.paired());
    golden("sync_page_unpair_armed", &d.frame);
    d.tap(Button::A);
    d.wait_until("unpairing", |d| !d.app.sync.paired());
    d.run(32);
    assert_eq!(sync::paired(&rig.paths), None);
    assert_eq!(d.app.sync.icon(), None);
    d.wait_until("the PC forgetting the Miyoo", |_| {
        desk.hub.devices().is_empty()
    });
}

#[test]
fn the_code_screen_and_b_cancels() {
    let rig = Rig::new();
    let desk = Desk::new(&rig);
    let mut d = rig.app_sync(online(), desk.options());
    open_sync_page(&mut d);
    // Screens that pass in a moment, held still for their pictures.
    d.app.sync.show(Stage::Searching);
    d.run(32);
    golden("sync_page_searching", &d.frame);
    d.app.sync.show(Stage::Code {
        hub: "Desk".into(),
        code: "407913".into(),
    });
    d.run(32);
    golden("sync_page_code", &d.frame);
    d.app.sync.show(Stage::Confirming {
        hub: "Desk".into(),
        code: "407913".into(),
    });
    d.run(32);
    golden("sync_page_confirming", &d.frame);
    d.tap(Button::B);
    assert_eq!(stage(&d), Stage::Idle);

    // No PC answers: say so, and A looks again.
    let lonely = sync::Options {
        discover: vec!["127.0.0.1:9".parse().unwrap()],
        poke: Some(local()),
    };
    let mut d = rig.app_sync(online(), lonely);
    open_sync_page(&mut d);
    d.tap(Button::A);
    d.wait_until("the search to end", |d| stage(d) != Stage::Searching);
    assert_eq!(stage(&d), Stage::Hubs(Vec::new()));
    golden("sync_page_no_pc", &d.frame);
    d.tap(Button::B);
    assert_eq!(stage(&d), Stage::Idle);

    // B on the code leaves the Miyoo unpaired, and the PC with nobody.
    let mut d = rig.app_sync(online(), desk.options());
    open_sync_page(&mut d);
    desk.hub.start_pairing();
    d.tap(Button::A);
    d.wait_until("the PCs found", |d| matches!(stage(d), Stage::Hubs(_)));
    d.tap(Button::A);
    d.wait_until("the code", |d| matches!(stage(d), Stage::Code { .. }));
    d.tap(Button::B);
    assert_eq!(stage(&d), Stage::Idle);
    desk.hub.stop_pairing();
    d.run(200);
    assert!(!d.app.sync.paired());
    assert_eq!(sync::paired(&rig.paths), None);
    assert!(desk.hub.devices().is_empty());

    // Wi-Fi off is said plainly, before anything is tried.
    let offline = FixedStatus {
        wifi: false,
        ..online()
    };
    let mut d = rig.app_sync(offline, desk.options());
    open_sync_page(&mut d);
    d.tap(Button::A);
    assert_eq!(stage(&d), Stage::Failed("Wi-Fi is off.".into()));
    golden("sync_page_wifi_off", &d.frame);
}

#[test]
fn sync_now_moves_games_and_saves_both_ways() {
    let rig = Rig::new();
    let desk = Desk::new(&rig);
    desk.pair(&rig);
    // The PC has a game the Miyoo lacks, and a newer save for one it has;
    // the Miyoo has everything else.
    write(&desk.root.join("Roms/GBA/Desk Quest.gba"), &rom(300_000, 3));
    let device = sync::layout(&rig.paths.sd);
    let mine = device.save_file(Platform::Gba, EMERALD, "srm", None);
    write(&mine, &save(1));
    age(&mine, 9000);
    // No automatic sync here: this is Sync now's own doing.
    std::fs::write(
        rig.paths.data().join("settings.txt"),
        "online_labels = off\nauto_sync = off\n",
    )
    .unwrap();
    let mut d = rig.app_sync(online(), desk.options());
    d.run(500);
    assert!(!d.app.sync.busy(), "Auto sync is off");
    assert_eq!(d.app.sync.icon(), Some(Icon::Synced));
    let carts = d.app.carts().len();

    d.tap(Button::Menu);
    d.run(700);
    d.tap(Button::Down);
    golden("about_sync_now_row", &d.frame);
    d.tap(Button::A);
    assert!(matches!(stage(&d), Stage::Syncing(_)));
    d.wait_until("the summary", |d| matches!(stage(d), Stage::Summary(_)));
    let report = d.app.sync.last_report().unwrap();
    assert!(report.is_clean(), "{report:?}");
    assert_eq!(report.to_client.added.len(), 1, "{report:?}");
    assert!(report.to_hub.added.len() > 5, "{report:?}");
    let Stage::Summary(lines) = stage(&d) else {
        unreachable!()
    };
    assert_eq!(lines[0], "To this Miyoo: 1 added.");
    assert!(lines[1].starts_with("To the PC: "), "{lines:?}");
    golden("sync_summary", &d.frame);

    // The game is on the card, the save on the PC.
    assert_eq!(
        std::fs::read(rig.paths.sd.join("Roms/GBA/Desk Quest.gba")).unwrap(),
        rom(300_000, 3)
    );
    let theirs = desk.layout().save_file(Platform::Gba, EMERALD, "srm", None);
    assert_eq!(std::fs::read(theirs).unwrap(), save(1));

    // Back on the shelf the new cart is there, without a restart.
    d.tap(Button::A);
    d.tap(Button::B);
    assert_eq!(d.app.phase, Phase::Shelf);
    d.run(64);
    assert_eq!(d.app.carts().len(), carts + 1);
    assert!(d.app.carts().iter().any(|c| c.stem == "Desk Quest"));
    assert_eq!(d.app.sync.icon(), Some(Icon::Synced));
    golden("shelf_synced", &d.frame);

    // The screens a sync passes through, held still for their pictures.
    d.tap(Button::Menu);
    d.run(700);
    for (name, progress) in [
        ("sync_progress_checking", Progress::Checking { percent: 42 }),
        (
            "sync_progress_transfer",
            Progress::Transfer {
                index: 3,
                count: 12,
                name: "Pokemon - Emerald Version (USA, Europe).gba".into(),
                to_miyoo: true,
                done: 5_500_000,
                size: 16_777_216,
            },
        ),
    ] {
        d.app.sync.show(Stage::Syncing(progress));
        d.run(32);
        golden(name, &d.frame);
    }
    d.app.sync.show(Stage::Summary(vec![
        "To this Miyoo: 2 added, 1 updated.".into(),
        "To the PC: 3 updated.".into(),
        "1 save conflict: both kept, choose on the PC.".into(),
        "2 games did not fit on the card (31.4 MB).".into(),
        "1 name the card cannot store.".into(),
    ]));
    d.run(32);
    golden("sync_summary_full", &d.frame);
}

#[test]
fn b_stops_a_sync_safely() {
    let rig = Rig::new();
    let desk = Desk::new(&rig);
    desk.pair(&rig);
    // Enough to be busy for a while.
    for n in 0..6u8 {
        write(
            &desk.root.join(format!("Roms/GBA/Big {n}.gba")),
            &rom(4_000_000, n),
        );
    }
    std::fs::write(
        rig.paths.data().join("settings.txt"),
        "online_labels = off\nauto_sync = off\n",
    )
    .unwrap();
    let mut d = rig.app_sync(online(), desk.options());
    d.tap(Button::Menu);
    d.run(700);
    d.tap(Button::Down);
    d.press(Button::A);
    d.press(Button::B);
    d.run(32);
    d.wait_until("the summary", |d| matches!(stage(d), Stage::Summary(_)));
    let Stage::Summary(lines) = stage(&d) else {
        unreachable!()
    };
    assert_eq!(lines[0], "Sync stopped. Nothing is half done.");
    golden("sync_stopped", &d.frame);
    // Whatever arrived is whole, and nothing is left in pieces.
    for n in 0..6u8 {
        let at = rig.paths.sd.join(format!("Roms/GBA/Big {n}.gba"));
        if at.exists() {
            assert_eq!(std::fs::read(&at).unwrap(), rom(4_000_000, n));
        }
    }
    // The next sync finishes the job.
    d.tap(Button::A);
    d.tap(Button::A);
    d.wait_until("the summary", |d| matches!(stage(d), Stage::Summary(_)));
    assert!(d.app.sync.last_report().unwrap().is_clean());
    for n in 0..6u8 {
        let at = rig.paths.sd.join(format!("Roms/GBA/Big {n}.gba"));
        assert_eq!(std::fs::read(&at).unwrap(), rom(4_000_000, n));
    }
}

#[test]
fn a_pc_that_cannot_be_reached_is_said_plainly_and_quickly() {
    let rig = Rig::new();
    let mut desk = Desk::new(&rig);
    let mut d = paired(&rig, &desk);
    assert_eq!(d.app.sync.icon(), Some(Icon::Synced));
    desk.unplug();
    d.tap(Button::Menu);
    d.run(700);
    d.tap(Button::Down);
    let start = Instant::now();
    d.tap(Button::A);
    d.wait_until("the answer", |d| matches!(stage(d), Stage::Failed(_)));
    assert!(start.elapsed() < Duration::from_secs(8), "quickly");
    golden("sync_unreachable", &d.frame);
    d.tap(Button::B);
    d.tap(Button::B);
    d.run(64);
    assert_eq!(d.app.sync.icon(), Some(Icon::Unreachable));
    golden("shelf_sync_unreachable", &d.frame);
    // The mark is remembered across a game.
    drop(d);
    let offline = FixedStatus {
        wifi: false,
        ..online()
    };
    let mut d = rig.app_sync(offline, sync::Options::default());
    d.run(2000);
    assert_eq!(d.app.sync.icon(), Some(Icon::Unreachable));
    assert!(!d.app.sync.busy(), "nothing is tried with Wi-Fi off");
    assert_eq!(d.app.sync.poke_port(), None);
}

#[test]
fn syncs_start_by_themselves_at_start_and_on_a_poke() {
    let rig = Rig::new();
    let desk = Desk::new(&rig);
    let device = sync::layout(&rig.paths.sd);
    let mine = device.save_file(Platform::Gba, EMERALD, "srm", None);
    write(&mine, &save(1));
    age(&mine, 9000);
    desk.pair_and_seed(&rig);
    // Played on the PC since: a newer save there, and a favourite.
    let theirs = desk.layout().save_file(Platform::Gba, EMERALD, "srm", None);
    assert_eq!(std::fs::read(&theirs).unwrap(), save(1));
    write(&theirs, &save(5));
    let mut d = rig.app_sync(online(), desk.options());
    // The first frame is drawn before anything is asked of the network.
    assert!(!d.app.sync.busy());
    d.run(32);
    settle(&mut d);
    assert_eq!(std::fs::read(&mine).unwrap(), save(5));
    assert_eq!(d.app.sync.icon(), Some(Icon::Synced));

    // The PC plays on and hearts the game, then says "sync now".
    write(&theirs, &save(2));
    let mut favs = Favourites::read(desk.layout().favourites_file());
    favs.toggle(Platform::Gba, EMERALD);
    std::fs::create_dir_all(desk.layout().favourites_file().parent().unwrap()).unwrap();
    favs.write(desk.layout().favourites_file()).unwrap();
    assert!(!d.app.is_favourite(Platform::Gba, EMERALD));
    let port = d.app.sync.poke_port().expect("listening while paired");
    assert_eq!(desk.server().poke(port), 1);
    d.wait_until("the poke's sync", |d| {
        d.app.is_favourite(Platform::Gba, EMERALD) && !d.app.sync.busy()
    });
    assert_eq!(std::fs::read(&mine).unwrap(), save(2));

    // A poke from anyone else starts nothing.
    let stranger = std::net::UdpSocket::bind(local()).unwrap();
    stranger
        .send_to(b"slot-mini poke", ("127.0.0.1", port))
        .unwrap();
    write(&theirs, &save(3));
    d.run(500);
    std::thread::sleep(Duration::from_millis(200));
    d.run(500);
    assert_eq!(std::fs::read(&mine).unwrap(), save(2));

    // Auto sync off: pokes are not answered either.
    d.tap(Button::Menu);
    d.run(700);
    d.tap(Button::A);
    d.tap(Button::Up);
    d.tap(Button::A);
    d.tap(Button::Down);
    d.tap(Button::A);
    assert!(!d.app.settings.auto_sync);
    for _ in 0..3 {
        d.tap(Button::B);
    }
    assert_eq!(d.app.phase, Phase::Shelf);
    desk.server().poke(port);
    d.run(500);
    std::thread::sleep(Duration::from_millis(200));
    d.run(500);
    assert_eq!(std::fs::read(&mine).unwrap(), save(2));
}

#[test]
fn a_conflict_shows_until_it_is_settled() {
    let rig = Rig::new();
    let desk = Desk::new(&rig);
    let device = sync::layout(&rig.paths.sd);
    let mine = device.save_file(Platform::Gba, EMERALD, "srm", None);
    write(&mine, &save(1));
    let mut d = paired(&rig, &desk);
    let theirs = desk.layout().save_file(Platform::Gba, EMERALD, "srm", None);
    assert_eq!(std::fs::read(&theirs).unwrap(), save(1));
    // Played on both since.
    write(&mine, &save(2));
    write(&theirs, &save(3));
    let port = d.app.sync.poke_port().unwrap();
    desk.server().poke(port);
    d.wait_until("the conflict", |d| {
        d.app.sync.icon() == Some(Icon::Conflict)
    });
    d.run(64);
    assert_eq!(desk.hub.conflicts().len(), 1);
    golden("shelf_sync_conflict", &d.frame);
}

#[test]
fn a_launch_pulls_the_newer_save_first() {
    let rig = Rig::new();
    let desk = Desk::new(&rig);
    let device = sync::layout(&rig.paths.sd);
    let mine = device.save_file(Platform::Gba, EMERALD, "srm", None);
    write(&mine, &save(1));
    age(&mine, 9000);
    let state = device.state_file(Platform::Gba, EMERALD, None);
    write(&state, b"where the Miyoo left off");
    age(&state, 9000);
    let mut d = paired(&rig, &desk);
    // Played on the PC since.
    let theirs = desk.layout().save_file(Platform::Gba, EMERALD, "srm", None);
    write(&theirs, &save(2));
    d.go_to(EMERALD);
    d.press(Button::A);
    d.run(16);
    d.release(Button::A);
    d.wait_until("the launch", |d| d.app.exit.is_some());
    assert!(matches!(d.app.exit, Some(Exit::Launch { .. })));
    // The PC's save is in place, whole, and the auto state that belonged
    // to the old one is out of RetroArch's way.
    assert_eq!(std::fs::read(&mine).unwrap(), save(2));
    assert!(!state.exists(), "the auto state is set aside");
    let kept: Vec<_> = std::fs::read_dir(state.parent().unwrap())
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(kept.len(), 1, "{kept:?}");
    assert!(kept[0].starts_with(EMERALD), "{kept:?}");
    // Nothing more is written once the launch is on its way.
    write(&theirs, &save(4));
    d.app.sync.auto();
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(std::fs::read(&mine).unwrap(), save(2));
}

#[test]
fn a_launch_does_not_wait_for_a_pc_that_is_gone() {
    let rig = Rig::new();
    let mut desk = Desk::new(&rig);
    let device = sync::layout(&rig.paths.sd);
    let mine = device.save_file(Platform::Gba, EMERALD, "srm", None);
    write(&mine, &save(1));
    let mut d = paired(&rig, &desk);
    desk.unplug();
    d.go_to(EMERALD);
    let start = Instant::now();
    d.press(Button::A);
    d.run(16);
    d.release(Button::A);
    // The cart's own 0.73 s on the test's clock, and no more than the cap
    // of real time.
    d.wait_until("the launch", |d| d.app.exit.is_some());
    assert!(start.elapsed() < sync::LAUNCH_CAP + Duration::from_millis(700));
    assert!(d.now < 2000, "the cart went in as it always does");
    assert_eq!(std::fs::read(&mine).unwrap(), save(1));
}

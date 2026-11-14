//! What the sync tests share: the PC app over a library with its hub on
//! this machine's loopback, a stand-in RetroArch, and a Miyoo that is the
//! real sync client over a card in a temporary folder.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use slot_mini::status::FixedStatus;
use slot_mini::Phase;
use slot_mini_gfx::Button;
use slot_mini_pc::input::Device;
use slot_mini_pc::pcapp::PcOptions;
use slot_mini_pc::sync::Net;
use slot_mini_pc::{PcApp, Request};
use slot_mini_store::{saves, Platform};
use slot_mini_sync::{
    Cancel, Client, Event, HttpLink, Hub, Layout, PairingAttempt, PairingStatus, Poked, Report,
    Scope,
};

pub const FRAME_MS: u64 = 16;

pub struct Rig {
    pub dir: tempfile::TempDir,
    pub pc: PcApp,
    pub now: u64,
}

fn status() -> Box<FixedStatus> {
    Box::new(FixedStatus {
        battery: None,
        wifi: true,
        clock: (12, 0),
        charging: false,
    })
}

impl Rig {
    /// An empty library with its hub listening on loopback, and a
    /// RetroArch folder holding the stand-in (`setup` is its `fake.txt`).
    /// Pokes go to `poke_port`.
    pub fn new(setup: &str, poke_port: u16) -> Rig {
        let dir = tempfile::tempdir().unwrap();
        let library = dir.path().join("library");
        let data = library.join("App/slot-mini-pc/data");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(data.join("settings.txt"), "online_labels = off\n").unwrap();
        let ra = dir.path().join("RetroArch");
        std::fs::create_dir_all(ra.join("cores")).unwrap();
        let exe = ra.join(slot_mini_pc::retroarch::EXE);
        std::fs::copy(env!("CARGO_BIN_EXE_fake-retroarch"), &exe).unwrap();
        for core in ["mgba_libretro", "gambatte_libretro", "gpsp_libretro"] {
            let name = format!("{core}{}", std::env::consts::DLL_SUFFIX);
            std::fs::write(ra.join("cores").join(name), b"not a real core").unwrap();
        }
        let log = dir.path().join("log.txt");
        std::fs::write(
            ra.join("fake.txt"),
            format!("log = {}\n{setup}", log.display()),
        )
        .unwrap();
        let opts = PcOptions {
            config_dir: dir.path().join("config"),
            places: vec![exe],
            status: status(),
            sound: None,
            face_wait: None,
            seed: 7,
            name: "Desk".into(),
            sync: Some(Net::local(poke_port)),
            releases_url: "http://127.0.0.1:9/releases".into(),
            buildbot: "http://127.0.0.1:9".into(),
        };
        let mut pc = PcApp::new(&library, opts, 0);
        pc.tick(0);
        pc.render();
        Rig { dir, pc, now: 0 }
    }

    pub fn library(&self) -> PathBuf {
        self.dir.path().join("library")
    }

    pub fn hub(&self) -> &Hub {
        self.pc.sync.as_ref().expect("the sync service").hub()
    }

    /// Where a Miyoo finds this PC.
    pub fn address(&self) -> String {
        let sync = self.pc.sync.as_ref().expect("the sync service");
        sync.addr().expect("the server").to_string()
    }

    /// The PC's save of a game.
    pub fn save(&self, core: &str, stem: &str) -> PathBuf {
        self.library()
            .join("Saves/CurrentProfile/saves")
            .join(core)
            .join(format!("{stem}.srm"))
    }

    pub fn frame(&mut self) {
        self.now += FRAME_MS;
        self.pc.tick(self.now);
        self.pc.render();
    }

    pub fn frames(&mut self, n: usize) {
        for _ in 0..n {
            self.frame();
        }
    }

    pub fn tap(&mut self, b: Button) {
        self.pc.button(b, true, Device::Keyboard);
        self.frame();
        self.pc.button(b, false, Device::Keyboard);
        self.frame();
    }

    pub fn taps(&mut self, buttons: &[Button]) {
        for b in buttons {
            self.tap(*b);
        }
    }

    pub fn wait_until(&mut self, what: &str, done: impl Fn(&Rig) -> bool) {
        let start = Instant::now();
        while !done(self) {
            assert!(
                start.elapsed() < Duration::from_secs(60),
                "gave up waiting for {what}"
            );
            std::thread::sleep(Duration::from_millis(2));
            self.frame();
        }
    }

    pub fn take(&mut self) -> Vec<Request> {
        std::mem::take(&mut self.pc.requests)
    }

    /// Move along the shelves to the cart named `stem`.
    pub fn select(&mut self, stem: &str) {
        for _ in 0..8 {
            for _ in 0..40 {
                if self.pc.app.current().is_some_and(|(_, c)| c.stem == stem) {
                    return;
                }
                self.tap(Button::Right);
                self.wait_until("the shelf to settle", at_rest);
            }
            self.tap(Button::R1);
            self.wait_until("the shelf to settle", at_rest);
        }
        panic!("no cart named {stem}");
    }

    /// Write the frame to `target/pc-sync/<name>.png`, to look at.
    pub fn shot(&mut self, name: &str) {
        self.pc.render();
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/pc-sync");
        std::fs::create_dir_all(&dir).unwrap();
        let frame = &self.pc.frame;
        let file = std::fs::File::create(dir.join(format!("{name}.png"))).unwrap();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), frame.w, frame.h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut out = enc.write_header().unwrap();
        out.write_image_data(&frame.to_rgba()).unwrap();
    }
}

pub fn at_rest(r: &Rig) -> bool {
    matches!(r.pc.app.phase, Phase::Shelf)
}

/// A Miyoo: a card and the real sync client.
pub struct Miyoo {
    _dir: tempfile::TempDir,
    pub sd: PathBuf,
    pub client: Client,
    /// The poke listener of a Miyoo on its shelf.
    pub ear: Poked,
}

impl Miyoo {
    /// A card with the fixture's games on it.
    pub fn new() -> Miyoo {
        let dir = tempfile::tempdir().unwrap();
        let sd = dir.path().join("sd");
        slot_mini::fixture::build(&sd, true).unwrap();
        let client = Client::open(Layout::device(&sd)).unwrap();
        let ear = Poked::bind(([127, 0, 0, 1], 0).into()).unwrap();
        Miyoo {
            _dir: dir,
            sd,
            client,
            ear,
        }
    }

    /// The port a "Sync now" for this Miyoo goes to.
    pub fn poke_port(&self) -> u16 {
        self.ear.local_addr().unwrap().port()
    }

    pub fn layout(&self) -> &Layout {
        self.client.layout()
    }

    /// The card's save of a GBA game.
    pub fn save(&self, stem: &str) -> PathBuf {
        self.layout().save_file(Platform::Gba, stem, "srm", None)
    }

    /// Ask to pair: the code this Miyoo shows.
    pub fn ask(&mut self, address: &str) -> PairingAttempt {
        let mut link = HttpLink::new(address);
        self.client
            .pair(&mut link)
            .expect("the hub takes the attempt")
    }

    /// The player confirmed the code on the Miyoo.
    pub fn confirm(&mut self, address: &str, attempt: &PairingAttempt) -> PairingStatus {
        let mut link = HttpLink::new(address);
        self.client.finish_pairing(&mut link, attempt).unwrap()
    }

    pub fn sync(&mut self, address: &str, scope: Scope) -> Report {
        self.sync_events(address, scope, &mut |_| {})
    }

    pub fn sync_events(
        &mut self,
        address: &str,
        scope: Scope,
        events: &mut dyn FnMut(Event),
    ) -> Report {
        let mut link = HttpLink::new(address);
        self.client.sync(&mut link, scope, events, &Cancel::new())
    }
}

/// Pair without the PC's screens: the hub is told directly.
pub fn pair(rig: &Rig, miyoo: &mut Miyoo) {
    let address = rig.address();
    rig.hub().start_pairing();
    let attempt = miyoo.ask(&address);
    assert_eq!(miyoo.confirm(&address, &attempt), PairingStatus::Waiting);
    assert!(rig.hub().confirm_pairing());
    assert_eq!(miyoo.confirm(&address, &attempt), PairingStatus::Paired);
    rig.hub().stop_pairing();
}

pub fn write(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

pub fn read(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// A save file that is not empty.
pub fn save(fill: u8) -> Vec<u8> {
    let mut bytes = vec![fill; 512];
    bytes[0] = 1;
    bytes
}

/// Make a file look written `secs` ago.
pub fn age(path: &Path, secs: u64) {
    let then = std::time::SystemTime::now() - Duration::from_secs(secs);
    std::fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(then)
        .unwrap();
}

/// Every file under `dir` whose name contains `part`.
pub fn named(dir: &Path, part: &str) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.contains(part))
            {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

#[allow(unused_imports)]
pub use saves::onion_stem;

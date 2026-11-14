//! slot-mini PC's side of a sync, served over localhost in the test's own
//! process. Nothing here leaves the machine.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use slot_mini::status::FixedStatus;
use slot_mini::sync;
use slot_mini_sync::{
    Cancel, Client, FixedCore, HttpLink, Hub, Layout, PairingStatus, Scope, Server,
};

use super::Rig;

pub fn local() -> SocketAddr {
    "127.0.0.1:0".parse().unwrap()
}

/// slot-mini PC's side: a library, its hub, and the hub's server.
pub struct Desk {
    pub root: PathBuf,
    pub hub: Arc<Hub>,
    pub server: Option<Server>,
}

impl Desk {
    pub fn new(rig: &Rig) -> Desk {
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

    pub fn server(&self) -> &Server {
        self.server.as_ref().unwrap()
    }

    /// Where the Miyoo looks for PCs and listens for pokes.
    pub fn options(&self) -> sync::Options {
        sync::Options {
            discover: self.server().discovery_addr().into_iter().collect(),
            poke: Some(local()),
        }
    }

    /// The PC goes away.
    pub fn unplug(&mut self) {
        self.server = None;
    }

    pub fn layout(&self) -> &Layout {
        self.hub.layout()
    }

    /// Pair the card as the Sync page would, the PC's user saying yes.
    pub fn pair(&self, rig: &Rig) {
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
    pub fn pair_and_seed(&self, rig: &Rig) {
        self.pair(rig);
        let mut client = Client::open(sync::layout(&rig.paths.sd)).unwrap();
        let mut link = HttpLink::new(&self.server().addr().to_string());
        let report = client.sync(&mut link, Scope::Full, &mut |_| {}, &Cancel::new());
        assert!(report.is_clean(), "{report:?}");
    }
}

pub fn online() -> FixedStatus {
    FixedStatus {
        battery: Some(76),
        wifi: true,
        clock: (12, 0),
        charging: false,
    }
}

pub fn write(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

/// Make a file look written `secs` ago.
pub fn age(path: &Path, secs: u64) {
    let file = std::fs::File::options().write(true).open(path).unwrap();
    file.set_modified(SystemTime::now() - Duration::from_secs(secs))
        .unwrap();
}

pub fn save(fill: u8) -> Vec<u8> {
    let mut bytes = vec![fill; 64];
    bytes[0] = fill.wrapping_add(1);
    bytes
}

pub fn rom(len: usize, fill: u8) -> Vec<u8> {
    (0..len).map(|n| (n * 31 % 251) as u8 ^ fill).collect()
}

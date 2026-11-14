//! What the engine tests share: a Miyoo card and a PC library in a
//! temporary folder, paired, with the hub in this process.
#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use serde_json::Value;
use slot_mini_store::{saves, Platform};
use slot_mini_sync::{
    hash_bytes, Cancel, Client, DirectLink, Event, FixedCore, Hub, Layout, Link, PairingStatus,
    Report, Scope, SecretKey, SyncError,
};

pub struct World {
    _dir: tempfile::TempDir,
    pub sd: PathBuf,
    pub pc: PathBuf,
    pub hub: Arc<Hub>,
    pub client: Client,
}

pub fn pc_layout(pc: &Path) -> Layout {
    Layout::pc(
        pc,
        &pc.join("RetroArch/saves"),
        &pc.join("RetroArch/states"),
        Arc::new(FixedCore::default()),
    )
}

/// Onion's launch scripts, so that the card says which core runs what.
pub fn emulators(sd: &Path, gba: &str) {
    for (sys, core) in [
        (Platform::Gba, gba),
        (Platform::Gb, "gambatte"),
        (Platform::Gbc, "gambatte"),
    ] {
        write(
            &saves::launch_script(sd, sys),
            slot_mini::fixture::launch_script(core).as_bytes(),
        );
    }
}

impl World {
    /// Two empty libraries, not paired.
    pub fn unpaired() -> World {
        World::with(Layout::device, pc_layout)
    }

    /// Two empty libraries, paired.
    pub fn new() -> World {
        let mut w = World::unpaired();
        w.pair();
        w
    }

    pub fn with(device: impl Fn(&Path) -> Layout, pc: impl Fn(&Path) -> Layout) -> World {
        let dir = tempfile::tempdir().unwrap();
        let (sd, root) = (dir.path().join("sd"), dir.path().join("pc"));
        std::fs::create_dir_all(&sd).unwrap();
        std::fs::create_dir_all(&root).unwrap();
        emulators(&sd, "mgba");
        let hub = Arc::new(Hub::open(pc(&root), "Desk").unwrap());
        let client = Client::open(device(&sd)).unwrap();
        World {
            _dir: dir,
            sd,
            pc: root,
            hub,
            client,
        }
    }

    pub fn pair(&mut self) {
        pair(&self.hub, &mut self.client);
    }

    pub fn sync(&mut self, scope: Scope) -> Report {
        let mut link = DirectLink::new(&self.hub);
        self.client
            .sync(&mut link, scope, &mut |_| {}, &Cancel::new())
    }

    /// Sync over a connection that drops after `budget` bytes.
    pub fn sync_cut(&mut self, scope: Scope, budget: u64) -> Report {
        let mut link = Flaky {
            inner: DirectLink::new(&self.hub),
            left: budget,
        };
        self.client
            .sync(&mut link, scope, &mut |_| {}, &Cancel::new())
    }

    pub fn sync_events(&mut self, scope: Scope) -> (Report, Vec<Event>) {
        let mut seen = Vec::new();
        let mut link = DirectLink::new(&self.hub);
        let report = self
            .client
            .sync(&mut link, scope, &mut |e| seen.push(e), &Cancel::new());
        (report, seen)
    }

    pub fn device(&self) -> &Layout {
        self.client.layout()
    }

    pub fn desk(&self) -> &Layout {
        self.hub.layout()
    }
}

/// Pair a device with a hub in this process, the PC's user saying yes.
pub fn pair(hub: &Hub, client: &mut Client) {
    hub.start_pairing();
    let mut link = DirectLink::new(hub);
    let attempt = client.pair(&mut link).unwrap();
    assert_eq!(attempt.code.len(), 6);
    assert_eq!(
        client.finish_pairing(&mut link, &attempt).unwrap(),
        PairingStatus::Waiting
    );
    assert!(hub.confirm_pairing());
    assert_eq!(
        client.finish_pairing(&mut link, &attempt).unwrap(),
        PairingStatus::Paired
    );
    hub.stop_pairing();
}

/// A link that fails once `left` bytes (one per request, one per byte of
/// a file) have gone through, and stays down.
pub struct Flaky<'a> {
    pub inner: DirectLink<'a>,
    pub left: u64,
}

impl Flaky<'_> {
    fn spend(&mut self, n: u64) -> Result<(), SyncError> {
        if self.left < n {
            self.left = 0;
            return Err(SyncError::Network("cut".into()));
        }
        self.left -= n;
        Ok(())
    }
}

impl Link for Flaky<'_> {
    fn info(&mut self) -> Result<Value, SyncError> {
        self.spend(1)?;
        self.inner.info()
    }

    fn call(&mut self, path: &str, body: &Value) -> Result<Value, SyncError> {
        self.spend(1)?;
        self.inner.call(path, body)
    }

    fn call_open(&mut self, path: &str, body: &Value) -> Result<Value, SyncError> {
        self.spend(1)?;
        self.inner.call_open(path, body)
    }

    fn upload(
        &mut self,
        session: &str,
        action: usize,
        path: &Path,
        size: u64,
        hash: &str,
        progress: &mut dyn FnMut(u64) -> Result<(), SyncError>,
    ) -> Result<(), SyncError> {
        self.spend(1)?;
        let start = self.left;
        let mut left = start;
        let result = self
            .inner
            .upload(session, action, path, size, hash, &mut |done| {
                if done > start {
                    left = 0;
                    return Err(SyncError::Network("cut".into()));
                }
                left = start - done;
                progress(done)
            });
        self.left = left;
        result
    }

    fn download(
        &mut self,
        session: &str,
        action: usize,
        sink: &mut dyn FnMut(&[u8]) -> Result<(), SyncError>,
    ) -> Result<(), SyncError> {
        self.spend(1)?;
        let mut left = self.left;
        let result = self.inner.download(session, action, &mut |piece| {
            if left < piece.len() as u64 {
                let part = left as usize;
                left = 0;
                sink(&piece[..part])?;
                return Err(SyncError::Network("cut".into()));
            }
            left -= piece.len() as u64;
            sink(piece)
        });
        self.left = left;
        result
    }

    fn set_auth(&mut self, device: &str, key: SecretKey) {
        self.inner.set_auth(device, key);
    }

    fn address(&self) -> String {
        self.inner.address()
    }
}

pub fn write(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

pub fn read(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Make a file look written `secs` ago: a save's time is when the game
/// was last played.
pub fn age(path: &Path, secs: u64) {
    let file = std::fs::File::options().write(true).open(path).unwrap();
    file.set_modified(SystemTime::now() - Duration::from_secs(secs))
        .unwrap();
}

/// A save that is not empty, `fill` telling one from another.
pub fn save(fill: u8) -> Vec<u8> {
    let mut bytes = vec![fill; 64];
    bytes[0] = fill.wrapping_add(1);
    bytes
}

/// Every file under `dir`, by its path from there.
pub fn tree(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut todo = vec![dir.to_path_buf()];
    while let Some(d) = todo.pop() {
        for entry in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                todo.push(path);
            } else {
                let rel = path.strip_prefix(dir).unwrap().to_string_lossy();
                out.insert(rel.replace('\\', "/"), read(&path));
            }
        }
    }
    out
}

/// The files of a library that are not sync's own bookkeeping.
pub fn library(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut all = tree(root);
    all.retain(|name, _| !name.starts_with("App/"));
    all
}

/// The hashes of every save a side holds in any form: the saves, their
/// backups and conflict copies, and what Remove everywhere set aside.
pub fn kept_saves(layout: &Layout) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for dir in [layout.saves_dir().to_path_buf(), layout.removed_dir()] {
        for bytes in tree(&dir).values() {
            if !saves::is_empty_save(bytes) {
                out.insert(hash_bytes(bytes));
            }
        }
    }
    out
}

/// The names of the files under `dir` that contain `part`.
pub fn named(dir: &Path, part: &str) -> Vec<String> {
    tree(dir)
        .into_keys()
        .filter(|name| name.contains(part))
        .collect()
}

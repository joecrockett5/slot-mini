//! The client: the Miyoo's side of a sync (docs/SYNC.md, "The exchange").

use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;

use serde_json::{json, Value};
use slot_mini_store::saves::{self, Incoming};
use slot_mini_store::AtomicFile;

use crate::auth::{self, PairKeys, SecretKey};
use crate::discovery::Poke;
use crate::error::SyncError;
use crate::hash::{hex, unhex, HashCache, Hasher};
use crate::key::{Key, Kind};
use crate::layout::{Game, Layout};
use crate::link::Link;
use crate::plan::{Op, TextEdits};
use crate::report::{Cancel, Event, Report, Scope, Side};
use crate::state::ClientStore;
use crate::PROTOCOL_VERSION;

pub use crate::state::HubRecord;

/// A pairing that is waiting for the player: show `code`.
pub struct PairingAttempt {
    pairing: String,
    hub_id: String,
    pub hub_name: String,
    key: SecretKey,
    /// The six digits both screens show.
    pub code: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PairingStatus {
    /// The PC's user has not confirmed yet: ask again in a second.
    Waiting,
    Paired,
    Rejected,
}

/// The device's sync state: who it is and the hub it is paired with.
pub struct Client {
    layout: Layout,
    store: ClientStore,
    device_id: String,
    device_name: String,
    hub: Option<HubRecord>,
}

fn text<'a>(body: &'a Value, name: &str) -> Result<&'a str, SyncError> {
    body[name]
        .as_str()
        .ok_or_else(|| SyncError::Bad(format!("the PC's reply has no {name}")))
}

fn check_protocol(info: &Value) -> Result<(), SyncError> {
    let hub = info["protocol"].as_u64().unwrap_or(0) as u32;
    if hub != PROTOCOL_VERSION {
        return Err(SyncError::Protocol {
            hub,
            client: PROTOCOL_VERSION,
        });
    }
    Ok(())
}

/// Whether the file at `path` has this hash.
pub(crate) fn same_file(path: &std::path::Path, hash: &str) -> bool {
    std::fs::read(path).is_ok_and(|bytes| crate::hash::hash_bytes(&bytes) == hash)
}

/// One sync under way.
struct Run<'a> {
    layout: &'a Layout,
    link: &'a mut dyn Link,
    events: &'a mut dyn FnMut(Event),
    cancel: &'a Cancel,
    session: String,
    paths: HashMap<String, PathBuf>,
    games: HashMap<String, Game>,
    sizes: HashMap<String, u64>,
    count: usize,
}

impl Run<'_> {
    fn commit(&mut self, id: usize) -> Result<(), SyncError> {
        self.link
            .call(
                "/sync/commit",
                &json!({"session": self.session, "action": id}),
            )
            .map(|_| ())
    }

    fn upload(&mut self, id: usize, key: &Key, hash: &str) -> Result<(), SyncError> {
        let folded = key.folded();
        let path = self
            .paths
            .get(&folded)
            .ok_or_else(|| SyncError::Io(format!("{key} is no longer here")))?;
        let size = self.sizes.get(&folded).copied().unwrap_or(0);
        let (events, cancel, count) = (&mut *self.events, self.cancel, self.count);
        let mut progress = |done: u64| {
            cancel.check()?;
            events(Event::Transfer {
                index: id,
                count,
                key: key.to_string(),
                to_client: false,
                done,
                size,
            });
            Ok(())
        };
        progress(0)?;
        self.link
            .upload(&self.session, id, path, size, hash, &mut progress)
    }

    /// Fetch an action's file and put it in place.
    fn download(&mut self, id: usize, key: &Key, size: u64, hash: &str) -> Result<(), SyncError> {
        let folded = key.folded();
        let dest = match self.paths.get(&folded) {
            Some(path) => path.clone(),
            None => self.layout.dest(key, &self.games),
        };
        if key.kind == Kind::Save && same_file(&dest, hash) {
            // Already here: Gambatte keeps one save for a GB and a GBC
            // game of the same name, and the first of the two brought it.
            self.paths.insert(folded, dest);
            return Ok(());
        }
        if let Some(dir) = dest.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut file = AtomicFile::create(&dest)?;
        let mut hasher = Hasher::new();
        let mut done = 0u64;
        let (events, cancel, count) = (&mut *self.events, self.cancel, self.count);
        let mut show = |done: u64| {
            events(Event::Transfer {
                index: id,
                count,
                key: key.to_string(),
                to_client: true,
                done,
                size,
            });
        };
        show(0);
        self.link.download(&self.session, id, &mut |piece| {
            cancel.check()?;
            hasher.update(piece);
            file.write_all(piece)?;
            done += piece.len() as u64;
            show(done);
            Ok(())
        })?;
        if done != size || hasher.finish() != hash {
            return Err(SyncError::HashMismatch);
        }
        if key.kind == Kind::Save {
            let state = self.layout.state_for(key, &self.games);
            saves::receive_save(&dest, Some(&state), Incoming::File(file), &saves::stamp())?;
        } else {
            file.commit()?;
        }
        if key.kind == Kind::Rom {
            self.games
                .entry(key.game().folded())
                .or_insert_with(|| Game {
                    id: key.game(),
                    rom: dest.clone(),
                });
        }
        self.paths.insert(folded, dest);
        Ok(())
    }

    fn act(&mut self, id: usize, op: &Op, report: &mut Report) -> Result<(), SyncError> {
        match op {
            Op::Remove { game } => {
                if self.layout.remove_game(game, &saves::stamp(), false)? {
                    report.removed.push(game.to_string());
                    // What the scan knew of the game is gone.
                    let folded = game.folded();
                    self.games.remove(&folded);
                }
                self.commit(id)
            }
            Op::Pull {
                key,
                size,
                hash,
                new,
            } => {
                self.download(id, key, *size, hash)?;
                let list = match new {
                    true => &mut report.to_client.added,
                    false => &mut report.to_client.updated,
                };
                list.push(key.to_string());
                // A commit that is lost is made up for by the next sync.
                self.commit(id)
            }
            Op::Push { key, hash, new, .. } => {
                self.upload(id, key, hash)?;
                let list = match new {
                    true => &mut report.to_hub.added,
                    false => &mut report.to_hub.updated,
                };
                list.push(key.to_string());
                Ok(())
            }
            Op::Conflict {
                key,
                winner,
                size,
                hash,
                mine,
            } => {
                self.upload(id, key, mine)?;
                if *winner == Side::Hub {
                    self.download(id, key, *size, hash)?;
                    report.conflicts.push((key.to_string(), *winner));
                    return self.commit(id);
                }
                report.conflicts.push((key.to_string(), *winner));
                Ok(())
            }
        }
    }
}

impl Client {
    /// The device whose card is `layout`.
    pub fn open(layout: Layout) -> Result<Client, SyncError> {
        let store = ClientStore::new(layout.sync_dir());
        let (id, name) = store.identity();
        let name = name.unwrap_or_else(|| "Miyoo Mini Plus".to_string());
        let device_id = match id {
            Some(id) => id,
            None => {
                let id = auth::random_hex(8)?;
                store.set_identity(&id, &name)?;
                id
            }
        };
        Ok(Client {
            hub: store.hub(),
            layout,
            store,
            device_id,
            device_name: name,
        })
    }

    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    pub fn set_device_name(&mut self, name: &str) -> Result<(), SyncError> {
        self.store.set_identity(&self.device_id, name)?;
        self.device_name = name.to_string();
        Ok(())
    }

    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    /// The hub this device is paired with.
    pub fn hub(&self) -> Option<&HubRecord> {
        self.hub.as_ref()
    }

    fn set_hub(&mut self, hub: Option<HubRecord>) -> Result<(), SyncError> {
        self.store.set_hub(hub.as_ref())?;
        self.hub = hub;
        Ok(())
    }

    /// Start pairing with the hub at `link`, which must be in pairing
    /// mode. Show the attempt's code; when the player confirms it, call
    /// [`Client::finish_pairing`].
    pub fn pair(&mut self, link: &mut dyn Link) -> Result<PairingAttempt, SyncError> {
        let info = link.info()?;
        check_protocol(&info)?;
        if info["pairing"].as_bool() != Some(true) {
            return Err(SyncError::NotPairing);
        }
        let keys = PairKeys::new()?;
        let nonce = auth::random(16)?;
        let public = keys.public.clone();
        let start = link.call_open(
            "/pair/start",
            &json!({
                "device_id": self.device_id,
                "device_name": self.device_name,
                "commit": auth::commitment(&public, &nonce),
            }),
        )?;
        let pairing = text(&start, "pairing")?.to_string();
        let hub_id = text(&start, "hub_id")?.to_string();
        let hub_public = unhex(text(&start, "public")?)
            .ok_or_else(|| SyncError::Bad("the PC's pairing key is not valid".into()))?;
        link.call_open(
            "/pair/reveal",
            &json!({"pairing": pairing, "public": hex(&public), "nonce": hex(&nonce)}),
        )?;
        let (key, code) =
            keys.finish(&hub_public, &public, &hub_public, &self.device_id, &hub_id)?;
        Ok(PairingAttempt {
            pairing,
            hub_id,
            hub_name: start["hub_name"].as_str().unwrap_or("").to_string(),
            key,
            code,
        })
    }

    /// The player confirmed the code here: tell the hub, and hear whether
    /// the PC's user has confirmed too. Call once a second while
    /// [`PairingStatus::Waiting`].
    pub fn finish_pairing(
        &mut self,
        link: &mut dyn Link,
        attempt: &PairingAttempt,
    ) -> Result<PairingStatus, SyncError> {
        link.set_auth(&self.device_id, attempt.key);
        let reply = link.call("/pair/confirm", &json!({"pairing": attempt.pairing}))?;
        match reply["status"].as_str() {
            Some("paired") => {
                self.set_hub(Some(HubRecord {
                    id: attempt.hub_id.clone(),
                    name: attempt.hub_name.clone(),
                    address: link.address(),
                    key: attempt.key,
                    hub_time: 0,
                }))?;
                Ok(PairingStatus::Paired)
            }
            Some("waiting") => Ok(PairingStatus::Waiting),
            _ => Ok(PairingStatus::Rejected),
        }
    }

    /// Forget the hub. With a link, tell the hub first (best effort).
    pub fn unpair(&mut self, link: Option<&mut dyn Link>) -> Result<(), SyncError> {
        if let (Some(link), Some(hub)) = (link, &self.hub) {
            link.set_auth(&self.device_id, hub.key);
            let _ = link.call("/unpair", &json!({}));
        }
        self.set_hub(None)
    }

    /// Whether a poke is from this device's hub and new (docs/SYNC.md,
    /// "Discovery and poke"). An accepted poke is remembered, with the
    /// address it came from; answer it with a `Full` sync.
    pub fn accept_poke(&mut self, poke: &Poke) -> bool {
        let Some(hub) = &self.hub else {
            return false;
        };
        if poke.hub != hub.id
            || poke.time <= hub.hub_time
            || !auth::verify_poke(&hub.key, &poke.hub, poke.time, poke.port, &poke.mac)
        {
            return false;
        }
        let mut hub = hub.clone();
        hub.hub_time = poke.time;
        hub.address = std::net::SocketAddr::new(poke.from.ip(), poke.port).to_string();
        self.set_hub(Some(hub)).is_ok()
    }

    /// Sync with the hub at `link`. Always returns a report; what stopped
    /// a sync early is in its `interrupted`.
    pub fn sync(
        &mut self,
        link: &mut dyn Link,
        scope: Scope,
        events: &mut dyn FnMut(Event),
        cancel: &Cancel,
    ) -> Report {
        let mut report = Report::new(scope.clone());
        if let Err(e) = self.run(link, &scope, events, cancel, &mut report) {
            if e == SyncError::Unpaired {
                // The PC forgot this device.
                let _ = self.set_hub(None);
            }
            report.interrupted = Some(e);
        }
        events(Event::Finished(Box::new(report.clone())));
        report
    }

    fn run(
        &mut self,
        link: &mut dyn Link,
        scope: &Scope,
        events: &mut dyn FnMut(Event),
        cancel: &Cancel,
        report: &mut Report,
    ) -> Result<(), SyncError> {
        let mut hub = self.hub.clone().ok_or(SyncError::NotPaired)?;
        events(Event::Connecting);
        link.set_auth(&self.device_id, hub.key);
        let info = link.info()?;
        check_protocol(&info)?;
        if info["id"].as_str() != Some(&hub.id) {
            // Another PC answers at that address.
            return Err(SyncError::NoHub);
        }
        hub.hub_time = hub
            .hub_time
            .max(info["now"].as_u64().unwrap_or(0).saturating_mul(1000));
        hub.address = link.address();
        if let Some(name) = info["name"].as_str() {
            hub.name = name.to_string();
        }
        if self.hub.as_ref() != Some(&hub) {
            self.set_hub(Some(hub))?;
        }
        let hello = link.call(
            "/sync/hello",
            &json!({"scope": scope.to_string(), "device_name": self.device_name}),
        )?;
        let session = text(&hello, "session")?.to_string();

        self.layout.sweep();
        let hashes = self.store.hashes();
        let mut cache = HashCache::load(&hashes);
        let scan = self.layout.scan(
            scope,
            &mut cache,
            &mut |done, total| events(Event::Scanning { done, total }),
            cancel,
        )?;
        let _ = cache.save(&hashes);
        report.duplicates.extend(
            scan.duplicates
                .iter()
                .map(|k| (k.to_string(), Side::Client)),
        );

        events(Event::Planning);
        let mut body = scan.manifest.to_json();
        body["session"] = json!(session);
        let plan = link.call("/sync/plan", &body)?;
        let actions: Vec<Op> = plan["actions"]
            .as_array()
            .map(|a| a.iter().map(Op::from_json).collect::<Option<_>>())
            .unwrap_or_default()
            .ok_or_else(|| SyncError::Bad("the PC's plan is not well formed".into()))?;
        let notes = &plan["notes"];
        let keyed = |v: &Value| -> Vec<Value> { v.as_array().cloned().unwrap_or_default() };
        for e in keyed(&notes["didnt_fit"]) {
            if let Some(key) = e["key"].as_str() {
                report
                    .didnt_fit
                    .push((key.to_string(), e["size"].as_u64().unwrap_or(0)));
            }
        }
        report.busy = crate::report::strings(&notes["busy"]);
        for (name, list) in [
            ("unstorable", &mut report.unstorable),
            ("duplicates", &mut report.duplicates),
        ] {
            for e in keyed(&notes[name]) {
                if let Some(key) = e["key"].as_str() {
                    list.push((
                        key.to_string(),
                        Side::parse(e["side"].as_str().unwrap_or("")),
                    ));
                }
            }
        }

        let sizes = scan
            .manifest
            .items
            .iter()
            .map(|i| (i.key.folded(), i.size))
            .collect();
        let mut run = Run {
            layout: &self.layout,
            link,
            events,
            cancel,
            session: session.clone(),
            paths: scan.paths,
            games: scan.games,
            sizes,
            count: actions.len(),
        };
        for (id, op) in actions.iter().enumerate() {
            cancel.check()?;
            match run.act(id, op, report) {
                Ok(()) => {}
                Err(e) if e.stops_sync() => return Err(e),
                Err(e) => {
                    let name = match op {
                        Op::Remove { game } => game.to_string(),
                        _ => op.key().map(Key::to_string).unwrap_or_default(),
                    };
                    report.failed.push((name, e.to_string()));
                }
            }
        }
        if scope.whole_library() {
            let edits = TextEdits::from_json(&plan["text"]);
            report.text_changed = run.layout.apply_text(&edits, &run.games)?;
            run.link
                .call("/sync/commit", &json!({"session": session, "text": true}))?;
        }
        // The sync is done either way; the report is for the PC's screen.
        let _ = run.link.call(
            "/sync/done",
            &json!({"session": session, "report": report.to_json()}),
        );
        Ok(())
    }
}

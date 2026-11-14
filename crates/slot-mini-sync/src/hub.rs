//! The hub: the PC's side of a sync, and the state it keeps (docs/SYNC.md,
//! "The exchange", "Save rules", "Remove everywhere", "Pairing").

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use slot_mini_store::saves::{self, Incoming};
use slot_mini_store::AtomicFile;

use crate::auth::{self, Nonces, PairKeys, SecretKey, REPLAY_WINDOW_SECS};
use crate::error::SyncError;
use crate::hash::{hash_bytes, hex, unhex, HashCache, Hasher, PIECE};
use crate::key::{GameId, Key, Kind};
use crate::layout::{Game, Layout};
use crate::model::Manifest;
use crate::plan::{plan, Input, Op};
use crate::report::{Cancel, Event, Report, Scope, Side};
use crate::state::{Device, HubStore};
use crate::{now_secs, PROTOCOL_VERSION};

pub use crate::state::Conflict;

/// A session nobody has used for this long gives way to another device.
const SESSION_IDLE: Duration = Duration::from_secs(30);
/// How long a pairing attempt may take.
const PAIRING_TIME: Duration = Duration::from_secs(120);

/// A paired device, for the PC's UI.
#[derive(Clone, Debug, PartialEq)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    /// Where it last connected from.
    pub address: String,
    /// When it was paired and when it last finished a sync (Unix seconds,
    /// 0 for never).
    pub paired: u64,
    pub last_sync: u64,
}

/// What the PC's UI hears from the hub.
#[derive(Clone, Debug, PartialEq)]
pub enum HubEvent {
    /// A device's sync is at this stage.
    Sync {
        device: String,
        event: Event,
    },
    /// This item changed in the PC's library.
    Changed(String),
    /// A device wants to pair: show the code and ask.
    PairingCode {
        device_name: String,
        code: String,
    },
    Paired(DeviceInfo),
    Unpaired(String),
}

type Events = Arc<dyn Fn(HubEvent) + Send + Sync>;

struct Session {
    id: String,
    device: String,
    scope: Scope,
    used: Instant,
    /// Set by `/sync/plan`.
    actions: Vec<Op>,
    text_records: Vec<(String, Option<String>)>,
    /// The hub's files and games as the plan saw them, kept up as files
    /// land.
    paths: HashMap<String, PathBuf>,
    games: HashMap<String, Game>,
}

struct Attempt {
    id: String,
    device_id: String,
    device_name: String,
    commit: String,
    keys: Option<PairKeys>,
    public: Vec<u8>,
    key: Option<SecretKey>,
    confirmed: bool,
    rejected: bool,
    started: Instant,
}

#[derive(Default)]
struct State {
    devices: HashMap<String, Device>,
    nonces: Nonces,
    session: Option<Session>,
    pairing: bool,
    attempt: Option<Attempt>,
    /// The attempt that last succeeded, so that a repeated confirm still
    /// hears "paired".
    paired_attempt: Option<String>,
    busy: Option<GameId>,
}

pub struct Hub {
    layout: Layout,
    store: HubStore,
    id: String,
    name: String,
    state: Mutex<State>,
    cache: Mutex<HashCache>,
    events: Mutex<Option<Events>>,
}

fn text<'a>(body: &'a Value, name: &str) -> Result<&'a str, SyncError> {
    body[name]
        .as_str()
        .ok_or_else(|| SyncError::Bad(format!("the request has no {name}")))
}

fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-');
    if out.is_empty() {
        "device".to_string()
    } else {
        out.chars().take(24).collect()
    }
}

fn info_of(d: &Device) -> DeviceInfo {
    DeviceInfo {
        id: d.id.clone(),
        name: d.name.clone(),
        address: d.address.clone(),
        paired: d.paired,
        last_sync: d.last_sync,
    }
}

impl Hub {
    /// The hub whose library is `layout`. `name` is what devices show.
    pub fn open(layout: Layout, name: &str) -> Result<Hub, SyncError> {
        let store = HubStore::new(layout.sync_dir());
        let (id, old_name) = store.identity();
        let id = match id {
            Some(id) => id,
            None => auth::random_hex(8)?,
        };
        if old_name.as_deref() != Some(name) {
            store.set_identity(&id, name)?;
        }
        let state = State {
            devices: store.devices(),
            ..State::default()
        };
        Ok(Hub {
            cache: Mutex::new(HashCache::load(&store.hashes())),
            layout,
            store,
            id,
            name: name.to_string(),
            state: Mutex::new(state),
            events: Mutex::new(None),
        })
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    /// Where to send what happens. The function is called on whichever
    /// thread the hub is working on, and must not call back into the hub.
    pub fn set_events(&self, events: impl Fn(HubEvent) + Send + Sync + 'static) {
        *self.events.lock().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(events));
    }

    fn emit(&self, event: HubEvent) {
        let events = self
            .events
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if let Some(events) = events {
            events(event);
        }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The reply to `GET /info`.
    pub fn info(&self) -> Value {
        json!({
            "name": self.name,
            "id": self.id,
            "protocol": PROTOCOL_VERSION,
            "now": now_secs(),
            "pairing": self.state().pairing,
        })
    }

    pub fn devices(&self) -> Vec<DeviceInfo> {
        let mut list: Vec<DeviceInfo> = self.state().devices.values().map(info_of).collect();
        list.sort_by(|a, b| (&a.name, &a.id).cmp(&(&b.name, &b.id)));
        list
    }

    /// Forget a device, and what was last synced with it.
    pub fn unpair(&self, device: &str) -> Result<(), SyncError> {
        {
            let mut state = self.state();
            if state.devices.remove(device).is_none() {
                return Ok(());
            }
            state.nonces.forget(device);
            if state.session.as_ref().is_some_and(|s| s.device == device) {
                state.session = None;
            }
            self.store.delete_device(device)?;
        }
        self.emit(HubEvent::Unpaired(device.to_string()));
        Ok(())
    }

    /// Mark the game running on the PC; plans leave it alone.
    pub fn set_busy(&self, game: Option<GameId>) {
        self.state().busy = game;
    }

    /// Whether a device is syncing right now.
    pub fn is_syncing(&self) -> bool {
        self.state()
            .session
            .as_ref()
            .is_some_and(|s| s.used.elapsed() < SESSION_IDLE)
    }

    // ---- Authentication ----

    /// Check a signed request (docs/SYNC.md, "Authentication") and return
    /// the key to sign the reply with. `address` is where it came from.
    #[allow(clippy::too_many_arguments)]
    pub fn authenticate(
        &self,
        device: &str,
        method: &str,
        target: &str,
        time: u64,
        nonce: &str,
        body_hash: &str,
        auth: &str,
        address: Option<&str>,
    ) -> Result<SecretKey, SyncError> {
        let mut state = self.state();
        let paired = state.devices.get(device).map(|d| d.key);
        let key = match paired {
            Some(key) => key,
            // A device that is pairing signs its confirmation with the
            // key the exchange just made.
            None => state
                .attempt
                .as_ref()
                .filter(|a| a.device_id == device && target == "/pair/confirm")
                .and_then(|a| a.key)
                .ok_or(SyncError::Unpaired)?,
        };
        if !auth::verify_request(&key, method, target, time, nonce, body_hash, auth) {
            return Err(SyncError::Auth("bad-auth".into()));
        }
        let now = now_secs();
        if time.abs_diff(now) > REPLAY_WINDOW_SECS {
            return Err(SyncError::Auth("stale".into()));
        }
        if !state.nonces.fresh(device, nonce, time, now) {
            return Err(SyncError::Auth("replay".into()));
        }
        if let (Some(address), Some(d)) = (address, state.devices.get_mut(device)) {
            if d.address != address {
                d.address = address.to_string();
                let _ = self.store.save_device(d);
            }
        }
        Ok(key)
    }

    /// Each paired device's last address and key, for pokes.
    pub fn poke_targets(&self) -> Vec<(String, SecretKey)> {
        self.state()
            .devices
            .values()
            .filter(|d| !d.address.is_empty())
            .map(|d| (d.address.clone(), d.key))
            .collect()
    }

    /// Ask every paired device to sync now: a signed datagram to its last
    /// address. `hub_port` is where the server listens. Returns how many
    /// were sent.
    pub fn poke(&self, hub_port: u16, poke_port: u16) -> usize {
        let mut sent = 0;
        for (address, key) in self.poke_targets() {
            let Ok(ip) = address.parse::<std::net::IpAddr>() else {
                continue;
            };
            let target = std::net::SocketAddr::new(ip, poke_port);
            if crate::discovery::poke(&self.id, &key, hub_port, target).is_ok() {
                sent += 1;
            }
        }
        sent
    }

    // ---- Pairing ----

    /// Enter pairing mode (the PC's "Pair a device" screen).
    pub fn start_pairing(&self) {
        let mut state = self.state();
        state.pairing = true;
        state.attempt = None;
    }

    /// Leave pairing mode; an attempt under way is rejected.
    pub fn stop_pairing(&self) {
        let mut state = self.state();
        state.pairing = false;
        if let Some(a) = &mut state.attempt {
            a.rejected = true;
        }
    }

    /// The PC's user agrees that the codes match.
    pub fn confirm_pairing(&self) -> bool {
        match &mut self.state().attempt {
            Some(a) if a.key.is_some() && !a.rejected => {
                a.confirmed = true;
                true
            }
            _ => false,
        }
    }

    fn pair_start(&self, body: &Value) -> Result<Value, SyncError> {
        let device_id = text(body, "device_id")?;
        let ok = (8..=64).contains(&device_id.len())
            && device_id
                .bytes()
                .all(|b| b.is_ascii_digit() || b.is_ascii_lowercase() || b == b'-');
        if !ok {
            return Err(SyncError::Bad("the device id is not valid".into()));
        }
        let commit = text(body, "commit")?;
        let keys = PairKeys::new()?;
        let id = auth::random_hex(8)?;
        let mut state = self.state();
        if !state.pairing {
            return Err(SyncError::NotPairing);
        }
        let public = keys.public.clone();
        state.attempt = Some(Attempt {
            id: id.clone(),
            device_id: device_id.to_string(),
            device_name: text(body, "device_name")?.to_string(),
            commit: commit.to_string(),
            keys: Some(keys),
            public: public.clone(),
            key: None,
            confirmed: false,
            rejected: false,
            started: Instant::now(),
        });
        Ok(
            json!({"pairing": id, "hub_id": self.id, "hub_name": self.name,
                  "public": hex(&public)}),
        )
    }

    fn pair_reveal(&self, body: &Value) -> Result<Value, SyncError> {
        let bad = || SyncError::Bad("the pairing key does not match what was promised".into());
        let public = unhex(text(body, "public")?).ok_or_else(bad)?;
        let nonce = unhex(text(body, "nonce")?).ok_or_else(bad)?;
        let (name, code) = {
            let mut state = self.state();
            let attempt = state
                .attempt
                .as_mut()
                .filter(|a| a.id == body["pairing"].as_str().unwrap_or(""))
                .ok_or(SyncError::Rejected)?;
            if auth::commitment(&public, &nonce) != attempt.commit {
                return Err(bad());
            }
            let keys = attempt.keys.take().ok_or(SyncError::Rejected)?;
            let (key, code) = keys.finish(
                &public,
                &public,
                &attempt.public,
                &attempt.device_id,
                &self.id,
            )?;
            attempt.key = Some(key);
            (attempt.device_name.clone(), code)
        };
        self.emit(HubEvent::PairingCode {
            device_name: name,
            code,
        });
        Ok(json!({}))
    }

    fn pair_confirm(&self, device: &str, body: &Value) -> Result<Value, SyncError> {
        let pairing = text(body, "pairing")?;
        let status = |s: &str| Ok(json!({"status": s}));
        let paired = {
            let mut state = self.state();
            if state.paired_attempt.as_deref() == Some(pairing) {
                return status("paired");
            }
            let Some(a) = state
                .attempt
                .as_ref()
                .filter(|a| a.id == pairing && a.device_id == device)
            else {
                return status("rejected");
            };
            let Some(key) = a.key else {
                return status("rejected");
            };
            if a.rejected || a.started.elapsed() > PAIRING_TIME {
                state.attempt = None;
                return status("rejected");
            }
            if !a.confirmed {
                return status("waiting");
            }
            let d = Device {
                id: a.device_id.clone(),
                name: a.device_name.clone(),
                key,
                address: String::new(),
                paired: now_secs(),
                last_sync: 0,
            };
            // Pairing again starts from no base.
            self.store.delete_device(&d.id)?;
            self.store.save_device(&d)?;
            state.devices.insert(d.id.clone(), d.clone());
            state.paired_attempt = Some(pairing.to_string());
            state.attempt = None;
            state.pairing = false;
            info_of(&d)
        };
        self.emit(HubEvent::Paired(paired));
        status("paired")
    }

    // ---- Requests ----

    /// An unsigned JSON request: the first two steps of pairing.
    pub fn open_request(&self, path: &str, body: &Value) -> Result<Value, SyncError> {
        match path {
            "/pair/start" => self.pair_start(body),
            "/pair/reveal" => self.pair_reveal(body),
            _ => Err(SyncError::NoHub),
        }
    }

    /// A JSON request from `device`, which the caller has authenticated.
    pub fn request(&self, device: &str, path: &str, body: &Value) -> Result<Value, SyncError> {
        if path == "/pair/confirm" {
            return self.pair_confirm(device, body);
        }
        if !self.state().devices.contains_key(device) {
            return Err(SyncError::Unpaired);
        }
        match path {
            "/sync/hello" => self.hello(device, body),
            "/sync/plan" => self.plan(device, body),
            "/sync/commit" => self.commit(device, body),
            "/sync/done" => self.done(device, body),
            "/unpair" => self.unpair(device).map(|()| json!({})),
            _ => Err(SyncError::NoHub),
        }
    }

    fn hello(&self, device: &str, body: &Value) -> Result<Value, SyncError> {
        let scope = Scope::parse(text(body, "scope")?)
            .ok_or_else(|| SyncError::Bad("the scope is not one this PC knows".into()))?;
        let id = auth::random_hex(8)?;
        {
            let mut state = self.state();
            if let Some(s) = &state.session {
                if s.device != device && s.used.elapsed() < SESSION_IDLE {
                    return Err(SyncError::HubBusy);
                }
            }
            if let (Some(name), Some(d)) =
                (body["device_name"].as_str(), state.devices.get_mut(device))
            {
                if d.name != name && !name.is_empty() {
                    d.name = name.to_string();
                    self.store.save_device(d)?;
                }
            }
            state.session = Some(Session {
                id: id.clone(),
                device: device.to_string(),
                scope,
                used: Instant::now(),
                actions: Vec::new(),
                text_records: Vec::new(),
                paths: HashMap::new(),
                games: HashMap::new(),
            });
        }
        self.sync_event(device, Event::Connecting);
        Ok(json!({"session": id, "hub_name": self.name}))
    }

    fn sync_event(&self, device: &str, event: Event) {
        let name = self
            .state()
            .devices
            .get(device)
            .map_or(String::new(), |d| d.name.clone());
        self.emit(HubEvent::Sync {
            device: name,
            event,
        });
    }

    /// The session `id` if it is `device`'s, marked as used.
    fn session<'a>(
        state: &'a mut State,
        device: &str,
        id: &str,
    ) -> Result<&'a mut Session, SyncError> {
        let s = state
            .session
            .as_mut()
            .filter(|s| s.id == id && s.device == device)
            .ok_or(SyncError::NoSession)?;
        s.used = Instant::now();
        Ok(s)
    }

    fn plan(&self, device: &str, body: &Value) -> Result<Value, SyncError> {
        let session = text(body, "session")?;
        let client = Manifest::from_json(body)
            .ok_or_else(|| SyncError::Bad("the list of files is not well formed".into()))?;
        let scope = Self::session(&mut self.state(), device, session)?
            .scope
            .clone();
        self.sync_event(device, Event::Planning);

        // Scan without the state lock: other requests must not wait for
        // the hashing.
        self.layout.sweep();
        let scan = {
            let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
            let scan = self
                .layout
                .scan(&scope, &mut cache, &mut |_, _| {}, &Cancel::new())?;
            let _ = cache.save(&self.store.hashes());
            scan
        };

        let mut state = self.state();
        Self::session(&mut state, device, session)?;
        // A removed game that is back on the PC is an ordinary game again.
        let mut tombstones = self.store.tombstones();
        let back: Vec<GameId> = tombstones
            .iter()
            .filter(|t| scan.games.contains_key(&t.folded()))
            .cloned()
            .collect();
        for game in &back {
            self.lift(&state, &mut tombstones, game)?;
        }
        let acked: Vec<String> = self
            .store
            .acked(device)
            .iter()
            .map(|g| g.folded())
            .collect();
        tombstones.retain(|t| !acked.contains(&t.folded()));

        let base = self.store.base(device);
        let mut plan = plan(&Input {
            scope: &scope,
            client: &client,
            hub: &scan.manifest,
            base: &base,
            tombstones: &tombstones,
            busy: state.busy.as_ref(),
        });
        plan.notes
            .duplicates
            .extend(scan.duplicates.iter().map(|k| (k.clone(), Side::Hub)));
        let text_changed = self.layout.apply_text(&plan.hub_text, &scan.games)?;
        self.store.append_base(device, &plan.records)?;

        let reply = plan.to_json();
        let s = Self::session(&mut state, device, session)?;
        s.actions = plan.actions;
        s.text_records = plan.text_records;
        s.paths = scan.paths;
        s.games = scan.games;
        drop(state);
        if text_changed {
            self.emit(HubEvent::Changed("text".into()));
        }
        Ok(reply)
    }

    /// Lift a tombstone: the game is back.
    fn lift(
        &self,
        state: &State,
        tombstones: &mut Vec<GameId>,
        game: &GameId,
    ) -> Result<(), SyncError> {
        let folded = game.folded();
        tombstones.retain(|t| t.folded() != folded);
        self.store.set_tombstones(tombstones)?;
        for id in state.devices.keys() {
            let mut acked = self.store.acked(id);
            let before = acked.len();
            acked.retain(|t| t.folded() != folded);
            if acked.len() != before {
                self.store.set_acked(id, &acked)?;
            }
        }
        Ok(())
    }

    fn action(
        &self,
        device: &str,
        session: &str,
        action: usize,
    ) -> Result<(Op, Option<PathBuf>, HashMap<String, Game>), SyncError> {
        let mut state = self.state();
        let s = Self::session(&mut state, device, session)?;
        let op = s
            .actions
            .get(action)
            .cloned()
            .ok_or_else(|| SyncError::Bad("the plan has no such action".into()))?;
        let path = op.key().and_then(|k| s.paths.get(&k.folded()).cloned());
        // Only saves, labels and art look their game up.
        let games = match op.key().map(|k| k.game().folded()) {
            Some(game) => s
                .games
                .get(&game)
                .map(|g| (game, g.clone()))
                .into_iter()
                .collect(),
            None => HashMap::new(),
        };
        Ok((op, path, games))
    }

    fn device_name(&self, device: &str) -> String {
        self.state()
            .devices
            .get(device)
            .map_or(String::new(), |d| d.name.clone())
    }

    /// Receive the file of a `push` or `conflict` action (`PUT
    /// /sync/file`). It is checked against the plan before it lands.
    pub fn upload(
        &self,
        device: &str,
        session: &str,
        action: usize,
        reader: &mut dyn Read,
    ) -> Result<(), SyncError> {
        let (op, have, games) = self.action(device, session, action)?;
        let (key, want, winner) = match &op {
            Op::Push { key, hash, .. } => (key, hash, None),
            Op::Conflict {
                key, mine, winner, ..
            } => (key, mine, Some(*winner)),
            _ => return Err(SyncError::Bad("that action sends no file to the PC".into())),
        };
        let dest = have.unwrap_or_else(|| self.layout.dest(key, &games));
        let stamp = saves::stamp();
        // A conflict the hub wins stores the upload as the conflict copy.
        let copy_name = |who: &str| -> PathBuf {
            let name = dest.file_name().and_then(|n| n.to_str()).unwrap_or("save");
            let mut to = dest.with_file_name(format!("{name}.conflict-{}-{stamp}", slug(who)));
            let mut n = 1;
            while to.exists() {
                to = dest.with_file_name(format!("{name}.conflict-{}-{stamp}_{n}", slug(who)));
                n += 1;
            }
            to
        };
        let target = match winner {
            Some(Side::Hub) => copy_name(&self.device_name(device)),
            _ => dest.clone(),
        };
        if let Some(dir) = target.parent() {
            std::fs::create_dir_all(dir)?;
        }
        self.sync_event(
            device,
            Event::Transfer {
                index: action,
                count: 0,
                key: key.to_string(),
                to_client: false,
                done: 0,
                size: 0,
            },
        );
        let mut file = AtomicFile::create(&target)?;
        let mut hasher = Hasher::new();
        let mut buf = vec![0u8; PIECE];
        loop {
            let n = reader.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
            file.write_all(&buf[..n])?;
            // Keep the session while a long file arrives.
            let _ = Self::session(&mut self.state(), device, session)?;
        }
        if hasher.finish() != *want {
            return Err(SyncError::HashMismatch);
        }

        let state_file = self.layout.state_for(key, &games);
        let folded = key.folded();
        // The save is already here when Gambatte's one file for a GB and
        // a GBC game of the same name came with the other of the two.
        let here = key.kind == Kind::Save && crate::client::same_file(&dest, want);
        match winner {
            _ if here => drop(file),
            None if key.kind == Kind::Save => {
                saves::receive_save(&dest, Some(&state_file), Incoming::File(file), &stamp)?;
            }
            None => file.commit()?,
            Some(Side::Client) => {
                // The PC's save becomes the conflict copy, then the
                // device's save comes in.
                if let Ok(old) = std::fs::read(&dest) {
                    let copy = copy_name(&self.name);
                    std::fs::rename(&dest, &copy)?;
                    self.record_conflict(device, key, &stamp, &copy, &hash_bytes(&old), &dest)?;
                }
                saves::receive_save(&dest, Some(&state_file), Incoming::File(file), &stamp)?;
            }
            Some(Side::Hub) => {
                // The same copy sent again (a sync that stopped half way)
                // is kept once.
                let beside = dest.parent().map(|d| d.to_path_buf());
                let known = self.store.conflicts().iter().any(|c| {
                    let copy = self.layout.saves_dir().join(&c.file);
                    c.hash == *want
                        && copy.is_file()
                        && copy.parent().map(|d| d.to_path_buf()) == beside
                        && copy.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                            let save = dest.file_name().and_then(|n| n.to_str()).unwrap_or("");
                            crate::names::fold(n)
                                .starts_with(&crate::names::fold(&format!("{save}.conflict-")))
                        })
                });
                if !known {
                    file.commit()?;
                    self.record_conflict(device, key, &stamp, &target, want, &dest)?;
                }
            }
        }

        let mut state = self.state();
        if winner != Some(Side::Hub) || here {
            self.store
                .append_base(device, &[(folded.clone(), Some(want.clone()))])?;
            if let Ok(s) = Self::session(&mut state, device, session) {
                s.paths.insert(folded, dest.clone());
                if key.kind == Kind::Rom {
                    s.games.entry(key.game().folded()).or_insert_with(|| Game {
                        id: key.game(),
                        rom: dest.clone(),
                    });
                }
            }
            if key.kind == Kind::Rom {
                let mut tombstones = self.store.tombstones();
                if tombstones.iter().any(|t| t.folded() == key.game().folded()) {
                    self.lift(&state, &mut tombstones, &key.game())?;
                }
            }
        }
        drop(state);
        self.emit(HubEvent::Changed(key.to_string()));
        Ok(())
    }

    fn record_conflict(
        &self,
        device: &str,
        key: &Key,
        stamp: &str,
        copy: &std::path::Path,
        hash: &str,
        save: &std::path::Path,
    ) -> Result<(), SyncError> {
        let mut list = self.store.conflicts();
        let file = copy
            .strip_prefix(self.layout.saves_dir())
            .unwrap_or(copy)
            .to_string_lossy()
            .replace('\\', "/");
        // The key in the PC's spelling: the save's own file name.
        let name = save
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&key.name);
        list.push(Conflict {
            id: auth::random_hex(4)?,
            key: Key::new(Kind::Save, key.sys, name).to_string(),
            device: self.device_name(device),
            stamp: stamp.to_string(),
            file,
            hash: hash.to_string(),
        });
        self.store.set_conflicts(&list)?;
        Ok(())
    }

    /// Open the file of a `pull` action, or of a `conflict` the hub wins
    /// (`GET /sync/file`): the file, and the size and hash the plan gave.
    pub fn download(
        &self,
        device: &str,
        session: &str,
        action: usize,
    ) -> Result<(File, u64, String), SyncError> {
        let (op, path, _) = self.action(device, session, action)?;
        let (key, size, hash) = match &op {
            Op::Pull {
                key, size, hash, ..
            }
            | Op::Conflict {
                key,
                size,
                hash,
                winner: Side::Hub,
                ..
            } => (key, *size, hash.clone()),
            _ => {
                return Err(SyncError::Bad(
                    "that action sends no file to the device".into(),
                ))
            }
        };
        let path = path.ok_or_else(|| SyncError::Io(format!("{key} is no longer on the PC")))?;
        self.sync_event(
            device,
            Event::Transfer {
                index: action,
                count: 0,
                key: key.to_string(),
                to_client: true,
                done: 0,
                size,
            },
        );
        Ok((File::open(path)?, size, hash))
    }

    fn commit(&self, device: &str, body: &Value) -> Result<Value, SyncError> {
        let mut state = self.state();
        let s = Self::session(&mut state, device, text(body, "session")?)?;
        if body["text"].as_bool() == Some(true) {
            let records = std::mem::take(&mut s.text_records);
            self.store.append_base(device, &records)?;
            return Ok(json!({}));
        }
        let action = body["action"].as_u64().unwrap_or(u64::MAX) as usize;
        match s.actions.get(action) {
            Some(Op::Pull { key, hash, .. })
            | Some(Op::Conflict {
                key,
                hash,
                winner: Side::Hub,
                ..
            }) => {
                self.store
                    .append_base(device, &[(key.folded(), Some(hash.clone()))])?;
            }
            Some(Op::Remove { game }) => {
                let mut acked = self.store.acked(device);
                if !acked.iter().any(|g| g.folded() == game.folded()) {
                    acked.push(game.clone());
                    self.store.set_acked(device, &acked)?;
                }
            }
            Some(_) => {}
            None => return Err(SyncError::Bad("the plan has no such action".into())),
        }
        Ok(json!({}))
    }

    fn done(&self, device: &str, body: &Value) -> Result<Value, SyncError> {
        let report = Report::from_json(&body["report"]);
        let name = {
            let mut state = self.state();
            Self::session(&mut state, device, text(body, "session")?)?;
            state.session = None;
            let now = now_secs();
            let base = self.store.base(device);
            self.store.rewrite_base(device, &base)?;
            let Some(d) = state.devices.get_mut(device) else {
                return Err(SyncError::Unpaired);
            };
            d.last_sync = now;
            self.store.save_device(d)?;
            if let Some(report) = &report {
                self.store.set_last_report(
                    &json!({"device": d.name, "time": now, "report": report.to_json()}),
                )?;
            }
            d.name.clone()
        };
        if let Some(report) = report {
            self.emit(HubEvent::Sync {
                device: name,
                event: Event::Finished(Box::new(report)),
            });
        }
        Ok(json!({}))
    }

    /// The last finished sync: the device's name, when, and its summary.
    pub fn last_report(&self) -> Option<(String, u64, Report)> {
        let v = self.store.last_report()?;
        Some((
            v["device"].as_str()?.to_string(),
            v["time"].as_u64()?,
            Report::from_json(&v["report"])?,
        ))
    }

    // ---- The PC's own actions ----

    /// Remove a game from the PC and, at their next sync, from every
    /// device (docs/SYNC.md, "Remove everywhere").
    pub fn remove_everywhere(&self, game: &GameId) -> Result<(), SyncError> {
        let state = self.state();
        self.layout.remove_game(game, &saves::stamp(), true)?;
        let folded = game.folded();
        let mut tombstones = self.store.tombstones();
        if !tombstones.iter().any(|t| t.folded() == folded) {
            tombstones.push(game.clone());
            self.store.set_tombstones(&tombstones)?;
        }
        // Nobody has applied this removal yet, whatever came before.
        for id in state.devices.keys() {
            let mut acked = self.store.acked(id);
            let before = acked.len();
            acked.retain(|t| t.folded() != folded);
            if acked.len() != before {
                self.store.set_acked(id, &acked)?;
            }
        }
        let mut conflicts = self.store.conflicts();
        let before = conflicts.len();
        conflicts.retain(|c| Key::parse(&c.key).is_none_or(|k| k.game().folded() != folded));
        if conflicts.len() != before {
            self.store.set_conflicts(&conflicts)?;
        }
        drop(state);
        self.emit(HubEvent::Changed(game.to_string()));
        Ok(())
    }

    /// The games removed everywhere that are still gone.
    pub fn tombstones(&self) -> Vec<GameId> {
        self.store.tombstones()
    }

    /// The saves kept twice. A record whose copy is gone is dropped.
    pub fn conflicts(&self) -> Vec<Conflict> {
        let _state = self.state();
        let all = self.store.conflicts();
        let live: Vec<Conflict> = all
            .iter()
            .filter(|c| self.layout.saves_dir().join(&c.file).is_file())
            .cloned()
            .collect();
        if live.len() != all.len() {
            let _ = self.store.set_conflicts(&live);
        }
        live
    }

    fn conflict(&self, id: &str) -> Result<(Vec<Conflict>, usize, PathBuf, PathBuf), SyncError> {
        let list = self.store.conflicts();
        let gone = || SyncError::Bad("that conflict is no longer listed".into());
        let n = list.iter().position(|c| c.id == id).ok_or_else(gone)?;
        let copy = self.layout.saves_dir().join(&list[n].file);
        // The save is the copy without its `.conflict-…` ending.
        let name = copy.file_name().and_then(|n| n.to_str()).ok_or_else(gone)?;
        let save = name
            .rfind(".conflict-")
            .map(|at| &name[..at])
            .ok_or_else(gone)?;
        let save = copy.with_file_name(save);
        Ok((list, n, copy, save))
    }

    /// Make the conflict copy the save, and the save the conflict copy.
    /// The next sync sends the new save to the device.
    pub fn swap_conflict(&self, id: &str) -> Result<(), SyncError> {
        let state = self.state();
        let (mut list, n, copy, save) = self.conflict(id)?;
        let key = Key::parse(&list[n].key)
            .ok_or_else(|| SyncError::Bad("that conflict is no longer listed".into()))?;
        let chosen = std::fs::read(&copy)?;
        let (games, _) = self.layout.games();
        let state_file = self.layout.state_for(&key, &games);
        let stamp = saves::stamp();
        match std::fs::read(&save) {
            Ok(old) if !saves::is_empty_save(&old) => {
                // Move the save out of the way under a name of its own,
                // land the chosen copy, then let the old save take the
                // copy's name.
                let parked = copy.with_file_name(format!(
                    "{}.swap",
                    copy.file_name().and_then(|n| n.to_str()).unwrap_or("save")
                ));
                std::fs::rename(&save, &parked)?;
                saves::receive_save(&save, Some(&state_file), Incoming::Bytes(&chosen), &stamp)?;
                std::fs::rename(&parked, &copy)?;
                list[n].hash = hash_bytes(&old);
            }
            _ => {
                saves::receive_save(&save, Some(&state_file), Incoming::Bytes(&chosen), &stamp)?;
                std::fs::remove_file(&copy)?;
                list.remove(n);
            }
        }
        self.store.set_conflicts(&list)?;
        drop(state);
        self.emit(HubEvent::Changed(key.to_string()));
        Ok(())
    }

    /// Stop listing a conflict: its copy moves to the removed-saves
    /// folder.
    pub fn dismiss_conflict(&self, id: &str) -> Result<(), SyncError> {
        let _state = self.state();
        let (mut list, n, copy, _) = self.conflict(id)?;
        if copy.is_file() {
            let sys = Key::parse(&list[n].key).map_or("other", |k| k.sys.dir_name());
            let to = self.layout.removed_dir().join(sys);
            std::fs::create_dir_all(&to)?;
            let name = copy.file_name().and_then(|n| n.to_str()).unwrap_or("save");
            let stamp = saves::stamp();
            let mut dst = to.join(format!("{name}.{stamp}"));
            let mut i = 1;
            while dst.exists() {
                dst = to.join(format!("{name}.{stamp}_{i}"));
                i += 1;
            }
            std::fs::rename(&copy, dst)?;
        }
        list.remove(n);
        self.store.set_conflicts(&list)?;
        Ok(())
    }
}

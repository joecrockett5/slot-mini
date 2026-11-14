//! Sync with slot-mini PC (docs/SYNC.md, docs/briefs/04-device-sync.md).
//!
//! The Miyoo is a client of `slot-mini-sync`. Nothing here runs until the
//! player pairs: an unpaired start reads one small file and that is all.
//! Every network call and every hash runs on one worker thread that sleeps
//! on a channel; the app reads a shared [`View`] of what it should draw.
//!
//! A launch must never wait on the network or start over a half-written
//! save, so the worker's link to the PC goes through a [`Gate`]: the app
//! closes it before it hands the game to Onion, which lets a local write
//! under way finish and stops the worker before its next one.

use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde_json::Value;
use slot_mini_store::{atomic_write, ini, Cart};
use slot_mini_sync::{
    discover, discover_broadcast, Cancel, Client, Event, Found, GameId, HttpLink, Layout, Link,
    PairingAttempt, PairingStatus, Poke, Poked, PokedStop, Report, Scope, SecretKey, Side,
    SyncError, POKE_PORT,
};

use crate::paths::Paths;
use crate::settings::Favourites;

/// Kept free on the card on top of the sync crate's own reserve
/// (`slot_mini_sync::RESERVE`, 16 MB): 64 MB in all. Onion writes save
/// states, screenshots and its own logs to the same card, and a FAT card
/// that fills up mid-game loses the save being written.
pub const MARGIN: u64 = 48 * 1024 * 1024;
/// How long a launch may wait for the pull of its game's save, counted
/// from the press that started the insert. The insert itself takes 0.73 s,
/// so a pull that answers in time costs nothing and the slowest case adds
/// about three quarters of a second.
pub const LAUNCH_CAP: Duration = Duration::from_millis(1500);
/// How long to wait for the PC to accept a connection before calling it
/// unreachable. A PC on the same network answers in a few milliseconds.
const PROBE_QUIET: Duration = Duration::from_millis(1000);
const PROBE_LAUNCH: Duration = Duration::from_millis(400);
const PROBE_SHOWN: Duration = Duration::from_millis(3000);
/// How long hubs are listened for.
const DISCOVER: Duration = Duration::from_millis(1500);
const REDISCOVER: Duration = Duration::from_millis(800);
/// How long the in-game helper may spend pushing a save after a game.
pub const AFTER_GAME_CAP: Duration = Duration::from_secs(8);

const STATUS_FILE: &str = "status.txt";

/// The footer's sync icon. Nothing is drawn while unpaired.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Synced,
    Syncing,
    Unreachable,
    Conflict,
}

/// How the last sync ended, kept in `data/sync/status.txt` so the icon
/// survives a game.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Last {
    Never,
    Synced,
    Unreachable,
    Conflict,
}

impl Last {
    fn name(self) -> &'static str {
        match self {
            Last::Never => "never",
            Last::Synced => "synced",
            Last::Unreachable => "unreachable",
            Last::Conflict => "conflict",
        }
    }

    fn parse(s: &str) -> Last {
        match s {
            "synced" => Last::Synced,
            "unreachable" => Last::Unreachable,
            "conflict" => Last::Conflict,
            _ => Last::Never,
        }
    }
}

/// A sync under way, for the progress screen.
#[derive(Clone, Debug, PartialEq)]
pub enum Progress {
    Connecting,
    /// Hashing this card's files.
    Checking {
        percent: u8,
    },
    Planning,
    Transfer {
        /// Counts from 1.
        index: usize,
        count: usize,
        name: String,
        to_miyoo: bool,
        done: u64,
        size: u64,
    },
}

/// What the Sync page shows beyond its rows.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum Stage {
    #[default]
    Idle,
    Searching,
    /// The PCs that answered; empty when none did.
    Hubs(Vec<Found>),
    Connecting(String),
    /// The code both screens show. A confirms.
    Code {
        hub: String,
        code: String,
    },
    /// Waiting for the PC to confirm too.
    Confirming {
        hub: String,
        code: String,
    },
    Paired(String),
    Syncing(Progress),
    /// Sync now ended: the lines to show.
    Summary(Vec<String>),
    Failed(String),
}

/// The library as it is after a sync changed it.
pub struct Fresh {
    pub carts: Vec<Cart>,
    pub favourites: Favourites,
}

/// The pull that runs beside the insert animation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Launch {
    None,
    Running,
    Done,
}

struct Inner {
    hub: Option<String>,
    last: Last,
    /// Sync jobs queued or running.
    jobs: usize,
    stage: Stage,
    /// Bumped when the app leaves a stage, so a worker's late answer to
    /// the old one is dropped.
    epoch: u64,
    fresh: Option<Fresh>,
    launch: Launch,
    poked: Option<Poke>,
    /// What the last sync reported (tests and the log).
    report: Option<Report>,
}

/// What the app draws from.
#[derive(Clone, Debug, PartialEq)]
pub struct View {
    /// The paired PC's name.
    pub hub: Option<String>,
    pub icon: Option<Icon>,
    pub stage: Stage,
}

// ---- the gate ---------------------------------------------------------------

#[derive(Default)]
struct GateState {
    /// The worker is between network calls, where it may write files.
    local: bool,
    /// A launch is going ahead: the worker stays where it is.
    closed: bool,
}

/// Keeps a launch and the worker's writes apart.
#[derive(Default)]
struct Gate {
    state: Mutex<GateState>,
    turn: Condvar,
}

impl Gate {
    fn lock(&self) -> MutexGuard<'_, GateState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The worker is about to work on local files. Never returns once the
    /// gate is closed.
    fn enter_local(&self) {
        let mut s = self.lock();
        while s.closed {
            s = self.turn.wait(s).unwrap_or_else(|e| e.into_inner());
        }
        s.local = true;
    }

    /// The worker is about to wait on the network, or is done.
    fn leave_local(&self) {
        self.lock().local = false;
        self.turn.notify_all();
    }

    /// Run a network wait outside the local section.
    fn net<T>(&self, f: impl FnOnce() -> T) -> T {
        self.leave_local();
        let out = f();
        self.enter_local();
        out
    }

    /// Close the gate for a launch: wait out the local work under way (a
    /// rename or two, or one piece of hashing), then hold the worker at
    /// its next step for good. `false` if the worker did not let go.
    fn close(&self, patience: Duration) -> bool {
        let until = Instant::now() + patience;
        let mut s = self.lock();
        while s.local {
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }
            s = self
                .turn
                .wait_timeout(s, left)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
        s.closed = true;
        !s.local
    }

    fn is_closed(&self) -> bool {
        self.lock().closed
    }
}

/// The worker's link to the PC: every call is a network wait.
struct Gated<'a> {
    link: HttpLink,
    gate: &'a Gate,
}

impl Gated<'_> {
    fn pass<T>(
        &mut self,
        f: impl FnOnce(&mut HttpLink) -> Result<T, SyncError>,
    ) -> Result<T, SyncError> {
        if self.gate.is_closed() {
            return Err(SyncError::Cancelled);
        }
        let link = &mut self.link;
        self.gate.net(|| f(link))
    }
}

impl Link for Gated<'_> {
    fn info(&mut self) -> Result<Value, SyncError> {
        self.pass(|l| l.info())
    }

    fn call(&mut self, path: &str, body: &Value) -> Result<Value, SyncError> {
        self.pass(|l| l.call(path, body))
    }

    fn call_open(&mut self, path: &str, body: &Value) -> Result<Value, SyncError> {
        self.pass(|l| l.call_open(path, body))
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
        self.pass(|l| l.upload(session, action, path, size, hash, progress))
    }

    fn download(
        &mut self,
        session: &str,
        action: usize,
        sink: &mut dyn FnMut(&[u8]) -> Result<(), SyncError>,
    ) -> Result<(), SyncError> {
        // The pieces land in a temporary file, so the sink needs no gate.
        self.pass(|l| l.download(session, action, sink))
    }

    fn set_auth(&mut self, device: &str, key: SecretKey) {
        self.link.set_auth(device, key);
    }

    fn address(&self) -> String {
        self.link.address()
    }
}

// ---- files ------------------------------------------------------------------

pub fn sync_dir(paths: &Paths) -> PathBuf {
    paths.data().join("sync")
}

/// The card as sync sees it: Onion's tree, less the margin kept free.
pub fn layout(sd: &Path) -> Layout {
    let layout = Layout::device(sd);
    match layout.free_space() {
        Some(free) => layout.with_free_space(free.saturating_sub(MARGIN)),
        None => layout,
    }
}

/// The paired PC's name, read straight from the client's record: no
/// identity is made and nothing is written.
pub fn paired(paths: &Paths) -> Option<String> {
    let text = std::fs::read_to_string(sync_dir(paths).join("client.txt")).ok()?;
    let v = ini::parse(&text);
    v.get("hub_id")?;
    v.get("key")?;
    Some(v.get("hub_name").cloned().unwrap_or_default())
}

fn read_last(paths: &Paths) -> Last {
    std::fs::read_to_string(sync_dir(paths).join(STATUS_FILE))
        .ok()
        .and_then(|t| ini::parse(&t).get("last").map(|s| Last::parse(s)))
        .unwrap_or(Last::Never)
}

fn write_last(paths: &Paths, last: Last) {
    let file = sync_dir(paths).join(STATUS_FILE);
    if last == Last::Never {
        let _ = std::fs::remove_file(file);
        return;
    }
    let _ = atomic_write(&file, format!("last = {}\n", last.name()).as_bytes());
}

// ---- the worker -------------------------------------------------------------

/// How a sync was asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum How {
    /// In the background: no screen, failures only change the icon.
    Quiet,
    /// MENU -> Sync now.
    Shown,
    /// Beside the insert animation.
    Launch,
}

enum Job {
    Discover(u64),
    Pair(Found, u64),
    Confirm(Cancel, u64),
    Unpair,
    Sync {
        scope: Scope,
        how: How,
        cancel: Cancel,
        epoch: u64,
        poke: Option<Poke>,
    },
}

/// Where hubs are looked for and pokes listened for. The defaults are the
/// network's; tests name local addresses.
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// Ask these addresses instead of broadcasting.
    pub discover: Vec<SocketAddr>,
    /// Listen for pokes here instead of on `POKE_PORT`.
    pub poke: Option<SocketAddr>,
}

impl Options {
    /// `SLOT_MINI_SYNC_DISCOVER` and `SLOT_MINI_SYNC_POKE`, for tests that
    /// run the binary.
    pub fn from_env() -> Options {
        let addr = |name: &str| std::env::var(name).ok().and_then(|v| v.parse().ok());
        Options {
            discover: addr("SLOT_MINI_SYNC_DISCOVER").into_iter().collect(),
            poke: addr("SLOT_MINI_SYNC_POKE"),
        }
    }
}

struct Worker {
    paths: Paths,
    opts: Options,
    inner: Arc<Mutex<Inner>>,
    gate: Arc<Gate>,
    pairing: Option<(Client, HttpLink, PairingAttempt)>,
}

fn lock(inner: &Mutex<Inner>) -> MutexGuard<'_, Inner> {
    inner.lock().unwrap_or_else(|e| e.into_inner())
}

fn reachable(address: &str, patience: Duration) -> bool {
    address
        .parse::<SocketAddr>()
        .is_ok_and(|a| TcpStream::connect_timeout(&a, patience).is_ok())
}

fn find_hubs(opts: &Options, patience: Duration) -> std::io::Result<Vec<Found>> {
    match opts.discover.is_empty() {
        true => discover_broadcast(patience),
        false => discover(patience, &opts.discover),
    }
}

/// A key as the player knows it: `save:GBA/Golden Sun.srm` -> `Golden Sun.srm`.
fn item_name(key: &str) -> String {
    key.rsplit('/').next().unwrap_or(key).to_string()
}

fn plural(n: usize, one: &str, many: &str) -> String {
    match n {
        1 => format!("1 {one}"),
        _ => format!("{n} {many}"),
    }
}

/// The results screen's lines, most important first.
pub fn summary(report: &Report) -> Vec<String> {
    let mut lines = Vec::new();
    match &report.interrupted {
        Some(SyncError::Cancelled) => lines.push("Sync stopped. Nothing is half done.".into()),
        Some(SyncError::Network(_)) => {
            lines.push("Lost the connection to the PC.".into());
        }
        Some(e) => lines.push(format!("{e}.")),
        None => {}
    }
    let moved = |m: &slot_mini_sync::Moved, to: &str| -> Option<String> {
        let (a, u) = (m.added.len(), m.updated.len());
        match (a, u) {
            (0, 0) => None,
            (a, 0) => Some(format!("{to}: {a} added.")),
            (0, u) => Some(format!("{to}: {u} updated.")),
            (a, u) => Some(format!("{to}: {a} added, {u} updated.")),
        }
    };
    lines.extend(moved(&report.to_client, "To this Miyoo"));
    lines.extend(moved(&report.to_hub, "To the PC"));
    if !report.removed.is_empty() {
        lines.push(format!(
            "{} removed everywhere.",
            plural(report.removed.len(), "game", "games")
        ));
    }
    if !report.conflicts.is_empty() {
        lines.push(format!(
            "{}: both kept, choose on the PC.",
            plural(report.conflicts.len(), "save conflict", "save conflicts")
        ));
    }
    if !report.didnt_fit.is_empty() {
        let bytes: u64 = report.didnt_fit.iter().map(|(_, n)| n).sum();
        lines.push(format!(
            "{} did not fit on the card ({}).",
            plural(report.didnt_fit.len(), "game", "games"),
            size_text(bytes)
        ));
    }
    let here = report
        .unstorable
        .iter()
        .filter(|(_, s)| *s == Side::Client)
        .count();
    let there = report.unstorable.len() - here;
    if here > 0 {
        lines.push(format!(
            "{} the card cannot store.",
            plural(here, "name", "names")
        ));
    }
    if there > 0 {
        lines.push(format!(
            "{} the PC cannot store.",
            plural(there, "name", "names")
        ));
    }
    if !report.duplicates.is_empty() {
        lines.push(format!(
            "{} differ only by case: skipped.",
            plural(report.duplicates.len(), "name", "names")
        ));
    }
    if !report.busy.is_empty() {
        lines.push(format!(
            "{} running on the PC: skipped.",
            plural(report.busy.len(), "game is", "games are")
        ));
    }
    if !report.failed.is_empty() {
        lines.push(format!(
            "{} failed and will be tried again.",
            plural(report.failed.len(), "file", "files")
        ));
    }
    if report.text_changed && lines.is_empty() {
        lines.push("Favourites and shells updated.".into());
    }
    if lines.is_empty() {
        lines.push("Everything is in sync.".into());
    }
    lines
}

/// Bytes as the progress screen prints them.
pub fn size_text(bytes: u64) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    match bytes {
        0..=1023 => format!("{bytes} B"),
        1024..=1_048_575 => format!("{} KB", bytes / 1024),
        _ => format!("{:.1} MB", bytes as f64 / MB),
    }
}

impl Worker {
    fn stage(&self, epoch: u64, stage: Stage) {
        let mut g = lock(&self.inner);
        if g.epoch == epoch {
            g.stage = stage;
        }
    }

    fn discover(&mut self, epoch: u64) {
        let stage = match find_hubs(&self.opts, DISCOVER) {
            Ok(found) => Stage::Hubs(found),
            Err(e) => {
                eprintln!("slot-mini: sync: discovery: {e}");
                Stage::Failed("The network cannot be reached.".into())
            }
        };
        self.stage(epoch, stage);
    }

    fn pair(&mut self, found: Found, epoch: u64) {
        self.pairing = None;
        let begun = Client::open(layout(&self.paths.sd)).and_then(|mut client| {
            let mut link = HttpLink::new(&found.address.to_string());
            let attempt = client.pair(&mut link)?;
            Ok((client, link, attempt))
        });
        match begun {
            Ok((client, link, attempt)) => {
                self.stage(
                    epoch,
                    Stage::Code {
                        hub: attempt.hub_name.clone(),
                        code: attempt.code.clone(),
                    },
                );
                self.pairing = Some((client, link, attempt));
            }
            Err(e) => self.stage(epoch, Stage::Failed(format!("{e}."))),
        }
    }

    fn confirm(&mut self, cancel: Cancel, epoch: u64) {
        let Some((mut client, mut link, attempt)) = self.pairing.take() else {
            return;
        };
        let outcome = loop {
            if cancel.is_cancelled() {
                return;
            }
            match client.finish_pairing(&mut link, &attempt) {
                Ok(PairingStatus::Waiting) => {}
                Ok(PairingStatus::Paired) => break Ok(()),
                Ok(PairingStatus::Rejected) => break Err(SyncError::Rejected),
                Err(e) => break Err(e),
            }
            // docs/SYNC.md: ask once a second.
            for _ in 0..10 {
                if cancel.is_cancelled() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        };
        let mut g = lock(&self.inner);
        match outcome {
            Ok(()) => {
                let name = client.hub().map(|h| h.name.clone()).unwrap_or_default();
                g.hub = Some(name.clone());
                g.last = Last::Never;
                if g.epoch == epoch {
                    g.stage = Stage::Paired(name);
                }
            }
            Err(e) => {
                if g.epoch == epoch {
                    g.stage = Stage::Failed(format!("{e}."));
                }
            }
        }
    }

    fn unpair(&mut self) {
        if let Ok(mut client) = Client::open(layout(&self.paths.sd)) {
            let address = client.hub().map(|h| h.address.clone()).unwrap_or_default();
            let mut link = reachable(&address, PROBE_QUIET).then(|| HttpLink::new(&address));
            let link = link.as_mut().map(|l| l as &mut dyn Link);
            if let Err(e) = client.unpair(link) {
                eprintln!("slot-mini: sync: unpair: {e}");
            }
        }
        write_last(&self.paths, Last::Never);
        let mut g = lock(&self.inner);
        g.hub = paired(&self.paths);
        g.last = Last::Never;
    }

    /// One sync, start to finish. The gate is held local throughout
    /// except inside network waits.
    fn sync(
        &mut self,
        scope: Scope,
        how: How,
        cancel: &Cancel,
        epoch: u64,
        poke: Option<Poke>,
    ) -> Report {
        let fail = |e: SyncError| {
            let mut r = Report::new(scope.clone());
            r.interrupted = Some(e);
            r
        };
        if cancel.is_cancelled() {
            return fail(SyncError::Cancelled);
        }
        let mut client = match Client::open(layout(&self.paths.sd)) {
            Ok(c) => c,
            Err(e) => return fail(e),
        };
        if let Some(poke) = poke {
            if !client.accept_poke(&poke) {
                return fail(SyncError::Cancelled);
            }
        }
        let Some(hub) = client.hub().cloned() else {
            return fail(SyncError::NotPaired);
        };
        if how == How::Shown {
            self.stage(epoch, Stage::Syncing(Progress::Connecting));
        }
        let patience = match how {
            How::Quiet => PROBE_QUIET,
            How::Shown => PROBE_SHOWN,
            How::Launch => PROBE_LAUNCH,
        };
        let gate = self.gate.clone();
        let mut address = hub.address.clone();
        if !gate.net(|| reachable(&address, patience)) {
            // The PC may have a new address: ask the network, unless a
            // launch is waiting.
            let found = match how {
                How::Launch => None,
                _ => gate
                    .net(|| find_hubs(&self.opts, REDISCOVER))
                    .ok()
                    .and_then(|l| l.into_iter().find(|f| f.id == hub.id)),
            };
            match found {
                Some(f) => address = f.address.to_string(),
                None => return fail(SyncError::NoHub),
            }
        }
        if cancel.is_cancelled() || gate.is_closed() {
            return fail(SyncError::Cancelled);
        }
        let mut link = Gated {
            link: HttpLink::new(&address),
            gate: &gate,
        };
        let inner = self.inner.clone();
        let mut shown: Option<Progress> = None;
        let mut events = |ev: Event| {
            if how != How::Shown {
                return;
            }
            let next = match ev {
                Event::Connecting => Progress::Connecting,
                Event::Scanning { done, total } => Progress::Checking {
                    percent: (done.saturating_mul(100) / total.max(1)).min(100) as u8,
                },
                Event::Planning => Progress::Planning,
                Event::Transfer {
                    index,
                    count,
                    key,
                    to_client,
                    done,
                    size,
                } => Progress::Transfer {
                    index: index + 1,
                    count,
                    name: item_name(&key),
                    to_miyoo: to_client,
                    done,
                    size,
                },
                Event::Finished(_) => return,
            };
            if shown.as_ref() != Some(&next) {
                shown = Some(next.clone());
                let mut g = lock(&inner);
                if g.epoch == epoch {
                    g.stage = Stage::Syncing(next);
                }
            }
        };
        client.sync(&mut link, scope, &mut events, cancel)
    }

    fn run_sync(&mut self, scope: Scope, how: How, cancel: Cancel, epoch: u64, poke: Option<Poke>) {
        self.gate.enter_local();
        let report = self.sync(scope, how, &cancel, epoch, poke);
        let last = {
            let g = lock(&self.inner);
            match &report.interrupted {
                Some(SyncError::Cancelled | SyncError::HubBusy) => g.last,
                Some(_) => Last::Unreachable,
                None if !report.conflicts.is_empty() => Last::Conflict,
                // A conflict is settled on the PC, which this side cannot
                // see: the mark stays until a sync moves something or the
                // player runs one by hand.
                None if g.last == Last::Conflict && how != How::Shown && report.is_noop() => {
                    Last::Conflict
                }
                None => Last::Synced,
            }
        };
        let hub = paired(&self.paths);
        if hub.is_some() {
            write_last(&self.paths, last);
        }
        // A launch is about to replace this process: the next start reads
        // the library itself.
        let fresh = (how != How::Launch && report.changed_client()).then(|| Fresh {
            carts: crate::library::scan(&self.paths),
            favourites: Favourites::read(&crate::settings::favourites_file(&self.paths)),
        });
        self.gate.leave_local();
        if let Some(e) = &report.interrupted {
            if *e != SyncError::Cancelled {
                eprintln!("slot-mini: sync ({}): {e}", report.scope);
            }
        }
        let mut g = lock(&self.inner);
        g.jobs = g.jobs.saturating_sub(1);
        g.last = match hub {
            Some(_) => last,
            None => Last::Never,
        };
        g.hub = hub;
        if fresh.is_some() {
            g.fresh = fresh;
        }
        if how == How::Launch {
            g.launch = Launch::Done;
        }
        if how == How::Shown && g.epoch == epoch {
            g.stage = match &report.interrupted {
                Some(SyncError::NoHub) => Stage::Failed(
                    "The PC cannot be reached. Is slot-mini PC running on this network?".into(),
                ),
                _ => Stage::Summary(summary(&report)),
            };
        }
        g.report = Some(report);
    }

    fn run(mut self, jobs: std::sync::mpsc::Receiver<Job>) {
        while let Ok(job) = jobs.recv() {
            match job {
                Job::Discover(epoch) => self.discover(epoch),
                Job::Pair(found, epoch) => self.pair(found, epoch),
                Job::Confirm(cancel, epoch) => self.confirm(cancel, epoch),
                Job::Unpair => self.unpair(),
                Job::Sync {
                    scope,
                    how,
                    cancel,
                    epoch,
                    poke,
                } => self.run_sync(scope, how, cancel, epoch, poke),
            }
        }
    }
}

// ---- the app's side ---------------------------------------------------------

/// The app's handle on sync. Cheap until something is asked of it.
pub struct Sync {
    paths: Paths,
    opts: Options,
    inner: Arc<Mutex<Inner>>,
    gate: Arc<Gate>,
    tx: Option<Sender<Job>>,
    /// The syncs asked for and not known to be over.
    cancels: Vec<Cancel>,
    confirming: Option<Cancel>,
    listener: Option<(PokedStop, u16)>,
    launch_at: Option<Instant>,
}

impl Sync {
    /// Reads whether this Miyoo is paired; starts nothing.
    pub fn new(paths: &Paths, opts: Options) -> Sync {
        let hub = match paths.is_pc() {
            true => None,
            false => paired(paths),
        };
        let last = match hub {
            Some(_) => read_last(paths),
            None => Last::Never,
        };
        Sync {
            paths: paths.clone(),
            opts,
            inner: Arc::new(Mutex::new(Inner {
                hub,
                last,
                jobs: 0,
                stage: Stage::Idle,
                epoch: 0,
                fresh: None,
                launch: Launch::None,
                poked: None,
                report: None,
            })),
            gate: Arc::new(Gate::default()),
            tx: None,
            cancels: Vec::new(),
            confirming: None,
            listener: None,
            launch_at: None,
        }
    }

    pub fn view(&self) -> View {
        let g = lock(&self.inner);
        let icon = g.hub.as_ref().map(|_| match (g.jobs, g.last) {
            (1.., _) => Icon::Syncing,
            (_, Last::Unreachable) => Icon::Unreachable,
            (_, Last::Conflict) => Icon::Conflict,
            (_, Last::Synced | Last::Never) => Icon::Synced,
        });
        View {
            hub: g.hub.clone(),
            icon,
            stage: g.stage.clone(),
        }
    }

    pub fn paired(&self) -> bool {
        lock(&self.inner).hub.is_some()
    }

    /// The footer's icon alone.
    pub fn icon(&self) -> Option<Icon> {
        self.view().icon
    }

    /// Whether a sync is queued or running.
    pub fn busy(&self) -> bool {
        lock(&self.inner).jobs > 0
    }

    /// The last sync's report (tests).
    pub fn last_report(&self) -> Option<Report> {
        lock(&self.inner).report.clone()
    }

    fn send(&mut self, job: Job) -> bool {
        if self.tx.is_none() {
            let (tx, rx) = channel();
            let worker = Worker {
                paths: self.paths.clone(),
                opts: self.opts.clone(),
                inner: self.inner.clone(),
                gate: self.gate.clone(),
                pairing: None,
            };
            let spawned = std::thread::Builder::new()
                .name("slot-mini-sync".into())
                .spawn(move || worker.run(rx));
            match spawned {
                Ok(_) => self.tx = Some(tx),
                Err(e) => {
                    eprintln!("slot-mini: sync: {e}");
                    return false;
                }
            }
        }
        self.tx.as_ref().is_some_and(|tx| tx.send(job).is_ok())
    }

    /// Leave whatever the Sync page was showing.
    fn fresh_stage(&mut self, stage: Stage) -> u64 {
        if let Some(c) = self.confirming.take() {
            c.cancel();
        }
        let mut g = lock(&self.inner);
        g.epoch += 1;
        g.stage = stage;
        g.epoch
    }

    /// Put the page on `stage` (the tests' pictures of screens that
    /// otherwise pass in a moment).
    #[doc(hidden)]
    pub fn show(&mut self, stage: Stage) {
        self.fresh_stage(stage);
    }

    /// B, or the page closing: back to the rows.
    pub fn dismiss(&mut self) {
        self.fresh_stage(Stage::Idle);
    }

    // ---- pairing ----

    /// "Pair with a PC": look for hubs.
    pub fn search(&mut self, wifi: bool) {
        if !wifi {
            self.fresh_stage(Stage::Failed("Wi-Fi is off.".into()));
            return;
        }
        let epoch = self.fresh_stage(Stage::Searching);
        if !self.send(Job::Discover(epoch)) {
            self.fresh_stage(Stage::Failed("Sync could not start.".into()));
        }
    }

    /// A PC was chosen from the list.
    pub fn choose(&mut self, found: Found) {
        let epoch = self.fresh_stage(Stage::Connecting(found.name.clone()));
        self.send(Job::Pair(found, epoch));
    }

    /// A on the code: wait for the PC to confirm too.
    pub fn confirm(&mut self) {
        let Stage::Code { hub, code } = self.view().stage else {
            return;
        };
        let epoch = self.fresh_stage(Stage::Confirming { hub, code });
        let cancel = Cancel::new();
        self.confirming = Some(cancel.clone());
        self.send(Job::Confirm(cancel, epoch));
    }

    pub fn unpair(&mut self) {
        self.stop_syncs();
        self.stop_listening();
        self.fresh_stage(Stage::Idle);
        self.send(Job::Unpair);
    }

    // ---- syncs ----

    fn stop_syncs(&mut self) {
        for c in self.cancels.drain(..) {
            c.cancel();
        }
    }

    fn start(&mut self, scope: Scope, how: How, epoch: u64, poke: Option<Poke>) -> bool {
        if !self.busy() {
            self.cancels.clear();
        }
        let cancel = Cancel::new();
        self.cancels.push(cancel.clone());
        lock(&self.inner).jobs += 1;
        let sent = self.send(Job::Sync {
            scope,
            how,
            cancel,
            epoch,
            poke,
        });
        if !sent {
            let mut g = lock(&self.inner);
            g.jobs = g.jobs.saturating_sub(1);
        }
        sent
    }

    /// MENU -> Sync now: a `Full` sync behind the progress screen. A quiet
    /// sync under way is stopped to make room.
    pub fn sync_now(&mut self, wifi: bool) {
        if !self.paired() {
            return;
        }
        if !wifi {
            self.fresh_stage(Stage::Failed("Wi-Fi is off.".into()));
            return;
        }
        self.stop_syncs();
        let epoch = self.fresh_stage(Stage::Syncing(Progress::Connecting));
        if !self.start(Scope::Full, How::Shown, epoch, None) {
            self.fresh_stage(Stage::Failed("Sync could not start.".into()));
        }
    }

    /// B on the progress screen: stop at the next safe point. The summary
    /// follows.
    pub fn cancel(&mut self) {
        self.stop_syncs();
    }

    /// A quiet `Auto` sync, unless one is already on its way.
    pub fn auto(&mut self) {
        if self.paired() && !self.busy() {
            self.start(Scope::Auto, How::Quiet, 0, None);
        }
    }

    /// Start listening for the PC's pokes (once, while paired). The thread
    /// sleeps in `recv`; [`Sync::poked`] hands the poke to the app.
    pub fn listen(&mut self) {
        if self.listener.is_some() || !self.paired() {
            return;
        }
        let addr = self
            .opts
            .poke
            .unwrap_or_else(|| SocketAddr::from(([0, 0, 0, 0], POKE_PORT)));
        let poked = match Poked::bind(addr) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("slot-mini: sync: poke listener: {e}");
                return;
            }
        };
        let (Ok(stop), Ok(at)) = (poked.stopper(), poked.local_addr()) else {
            return;
        };
        let inner = self.inner.clone();
        let spawned = std::thread::Builder::new()
            .name("slot-mini-poke".into())
            .spawn(move || {
                while let Some(poke) = poked.wait() {
                    lock(&inner).poked = Some(poke);
                }
            });
        if spawned.is_ok() {
            self.listener = Some((stop, at.port()));
        }
    }

    /// Where pokes are listened for (tests).
    pub fn poke_port(&self) -> Option<u16> {
        self.listener.as_ref().map(|(_, port)| *port)
    }

    fn stop_listening(&mut self) {
        if let Some((stop, _)) = self.listener.take() {
            stop.stop();
        }
    }

    /// A poke that arrived: answer it with a quiet `Auto` sync. The worker
    /// checks it is the paired PC's and new.
    pub fn poked(&mut self) -> bool {
        let Some(poke) = lock(&self.inner).poked.take() else {
            return false;
        };
        if self.busy() {
            // The sync under way will see what changed.
            return false;
        }
        self.start(Scope::Auto, How::Quiet, 0, Some(poke))
    }

    /// Whether a poke is waiting for the shelf to be at rest.
    pub fn has_poke(&self) -> bool {
        lock(&self.inner).poked.is_some()
    }

    /// The library after a sync changed it, once.
    pub fn take_fresh(&mut self) -> Option<Fresh> {
        lock(&self.inner).fresh.take()
    }

    // ---- launching ----

    /// The insert began: pull this game's save and art beside it. Other
    /// syncs give way.
    pub fn launching(&mut self, game: GameId) {
        if !self.paired() {
            return;
        }
        self.stop_syncs();
        lock(&self.inner).launch = Launch::Running;
        self.launch_at = Some(Instant::now());
        if !self.start(Scope::Game(game), How::Launch, 0, None) {
            lock(&self.inner).launch = Launch::None;
        }
    }

    /// Whether the launch should wait a little longer for its pull.
    pub fn launch_waits(&self) -> bool {
        let running = lock(&self.inner).launch == Launch::Running;
        running && self.launch_at.is_some_and(|at| at.elapsed() < LAUNCH_CAP)
    }

    /// The game is about to start: stop every sync and keep the worker
    /// away from the card. Returns once nothing is half written.
    pub fn close(&mut self) {
        self.stop_syncs();
        if self.tx.is_none() {
            return;
        }
        if !self.gate.close(Duration::from_secs(3)) {
            eprintln!("slot-mini: sync: the worker did not let go before the launch");
        }
    }
}

impl Drop for Sync {
    fn drop(&mut self) {
        self.stop_syncs();
        if let Some(c) = self.confirming.take() {
            c.cancel();
        }
        self.stop_listening();
    }
}

// ---- after a game, in app mode ------------------------------------------------

/// Push `game`'s save to the PC (and take what the PC has for it). The
/// in-game helper calls this once RetroArch has gone; it runs beside
/// Onion's menu and gives up at `cap` or when `cancel` is set.
pub fn after_game(paths: &Paths, game: GameId, cap: Duration, cancel: &Cancel) -> Option<Report> {
    paired(paths)?;
    let mut client = Client::open(layout(&paths.sd)).ok()?;
    let address = client.hub()?.address.clone();
    if !reachable(&address, PROBE_QUIET) {
        write_last(paths, Last::Unreachable);
        return None;
    }
    let timer = cancel.clone();
    let done = Arc::new((Mutex::new(false), Condvar::new()));
    let done2 = done.clone();
    let watch = std::thread::spawn(move || {
        let (flag, cv) = &*done2;
        let g = flag.lock().unwrap_or_else(|e| e.into_inner());
        let (g, _) = cv
            .wait_timeout_while(g, cap, |over| !*over)
            .unwrap_or_else(|e| e.into_inner());
        if !*g {
            timer.cancel();
        }
    });
    let mut link = HttpLink::new(&address);
    let report = client.sync(&mut link, Scope::Game(game), &mut |_| {}, cancel);
    {
        let (flag, cv) = &*done;
        *flag.lock().unwrap_or_else(|e| e.into_inner()) = true;
        cv.notify_all();
    }
    let _ = watch.join();
    let last = match &report.interrupted {
        Some(SyncError::Cancelled | SyncError::HubBusy) => None,
        Some(_) => Some(Last::Unreachable),
        None if !report.conflicts.is_empty() => Some(Last::Conflict),
        None => match read_last(paths) {
            Last::Conflict => None,
            _ => Some(Last::Synced),
        },
    };
    if let (Some(last), Some(_)) = (last, paired(paths)) {
        write_last(paths, last);
    }
    Some(report)
}

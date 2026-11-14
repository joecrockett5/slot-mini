//! The PC as the sync hub (docs/SYNC.md): `slot-mini-sync`'s hub over this
//! library, its server and discovery responder, and what they report,
//! handed to the app's own thread.
//!
//! The hub's state (pairing secrets, per-device bases, conflicts) lives in
//! the library, under `App/slot-mini-pc/data/sync/`, so a library that
//! moves keeps it.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};

use slot_mini::cores;
use slot_mini_store::{saves, Core, Platform};
use slot_mini_sync::{
    CoreChoice, GameId, Hub, HubEvent, Layout, Server, DISCOVERY_PORT, HUB_PORT, POKE_PORT,
};

/// Where the hub listens and where it pokes.
#[derive(Clone, Copy, Debug)]
pub struct Net {
    /// The server (TCP).
    pub bind: SocketAddr,
    /// The discovery responder (UDP).
    pub discovery: SocketAddr,
    /// The port a Miyoo on its shelf listens on for "Sync now".
    pub poke_port: u16,
}

impl Net {
    /// The ports docs/SYNC.md fixes, on every interface.
    pub fn lan() -> Net {
        Net {
            bind: ([0, 0, 0, 0], HUB_PORT).into(),
            discovery: ([0, 0, 0, 0], DISCOVERY_PORT).into(),
            poke_port: POKE_PORT,
        }
    }

    /// This machine only, on ports the system picks: tests.
    pub fn local(poke_port: u16) -> Net {
        Net {
            bind: ([127, 0, 0, 1], 0).into(),
            discovery: ([127, 0, 0, 1], 0).into(),
            poke_port,
        }
    }
}

/// Which core a game runs on here: the cart's own setting, else the PC's
/// default (`cores::chosen`). Sync asks so that "the save for this game"
/// lands in the folder RetroArch reads it from.
pub struct Cores {
    library: PathBuf,
}

impl Cores {
    pub fn new(library: &Path) -> Cores {
        Cores {
            library: library.to_path_buf(),
        }
    }
}

impl CoreChoice for Cores {
    fn core(&self, platform: Platform, stem: &str) -> Core {
        let roms = self.library.join("Roms").join(platform.dir_name());
        // The setting is beside the ROM, which may be one folder down.
        let folders = std::iter::once(roms.clone()).chain(
            std::fs::read_dir(&roms)
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir()),
        );
        for folder in folders {
            // Any extension: only the stem names the setting's file.
            let rom = folder.join(format!("{stem}.rom"));
            if let Some(core) = saves::per_game_core(&rom)
                .and_then(|c| Core::from_libretro(&c))
                .filter(|c| c.runs(platform))
            {
                return core;
            }
        }
        cores::pc_default(platform)
    }
}

/// The game a cart is, as sync names it.
pub fn game_of(cart: &slot_mini_store::Cart) -> Option<GameId> {
    Some(GameId::new(cart.platform, saves::onion_stem(&cart.rom)?))
}

/// The hub's layout over `library`: RetroArch keeps this library's saves
/// and states inside it (`retroarch::append_config`).
pub fn layout(library: &Path) -> Layout {
    let profile = library.join("Saves").join("CurrentProfile");
    Layout::pc(
        library,
        &profile.join("saves"),
        &profile.join("states"),
        Arc::new(Cores::new(library)),
    )
}

/// The name this PC goes by when the library has none yet.
pub fn computer_name() -> String {
    let named = ["COMPUTERNAME", "HOSTNAME"]
        .iter()
        .find_map(|key| std::env::var(key).ok())
        .or_else(|| std::fs::read_to_string("/etc/hostname").ok());
    clean_name(named.as_deref().unwrap_or("")).unwrap_or_else(|| "PC".to_string())
}

/// The longest name a PC can take: it has to fit the Miyoo's screen.
pub const NAME_MAX: usize = 20;

/// A name fit to show and to store: one line, not empty, not too long.
pub fn clean_name(name: &str) -> Option<String> {
    let name: String = name
        .chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .chars()
        .take(NAME_MAX)
        .collect();
    let name = name.trim().to_string();
    (!name.is_empty()).then_some(name)
}

/// The name the library's hub was last opened with.
fn stored_name(library: &Path) -> Option<String> {
    let file = layout(library).sync_dir().join("hub.txt");
    let text = std::fs::read_to_string(file).ok()?;
    clean_name(slot_mini_store::ini::parse(&text).get("name")?)
}

/// The hub and its server.
pub struct Service {
    hub: Arc<Hub>,
    server: Option<Server>,
    events: Receiver<HubEvent>,
    library: PathBuf,
    net: Option<Net>,
    /// Why the server is not listening, when it should be.
    pub offline: Option<String>,
}

impl Service {
    /// Open the library's hub under its stored name (`default_name` the
    /// first time) and, with `net`, start serving it.
    pub fn open(library: &Path, default_name: &str, net: Option<Net>) -> Result<Service, String> {
        let name = stored_name(library)
            .or_else(|| clean_name(default_name))
            .unwrap_or_else(|| "PC".to_string());
        Service::named(library, &name, net)
    }

    fn named(library: &Path, name: &str, net: Option<Net>) -> Result<Service, String> {
        let hub = Hub::open(layout(library), name).map_err(|e| e.to_string())?;
        let hub = Arc::new(hub);
        // The hub speaks on its request threads; the app listens on its
        // own, once a frame.
        let (tx, events) = mpsc::channel();
        let tx = Mutex::new(tx);
        hub.set_events(move |event| {
            let _ = tx.lock().unwrap_or_else(|e| e.into_inner()).send(event);
        });
        let mut service = Service {
            hub,
            server: None,
            events,
            library: library.to_path_buf(),
            net,
            offline: None,
        };
        service.listen();
        Ok(service)
    }

    fn listen(&mut self) {
        let Some(net) = self.net else {
            return;
        };
        match Server::start(self.hub.clone(), net.bind, Some(net.discovery)) {
            Ok(server) => {
                self.server = Some(server);
                self.offline = None;
            }
            Err(e) => {
                eprintln!("slot-mini-pc: sync server: {e}");
                self.offline = Some(e.to_string());
            }
        }
    }

    pub fn hub(&self) -> &Arc<Hub> {
        &self.hub
    }

    /// Whether Miyoos can reach this PC.
    pub fn listening(&self) -> bool {
        self.server.is_some()
    }

    /// Where the server listens.
    pub fn addr(&self) -> Option<SocketAddr> {
        self.server.as_ref().map(|s| s.addr())
    }

    /// Where discovery answers.
    pub fn discovery_addr(&self) -> Option<SocketAddr> {
        self.server.as_ref().and_then(|s| s.discovery_addr())
    }

    /// What the hub has said since the last call, oldest first.
    pub fn poll(&self) -> Vec<HubEvent> {
        self.events.try_iter().collect()
    }

    /// "Sync now": ask every paired Miyoo to sync. Returns how many were
    /// asked; one that is asleep or away from its shelf will not answer.
    pub fn poke(&self) -> usize {
        match (&self.server, self.net) {
            (Some(server), Some(net)) => server.poke(net.poke_port),
            _ => 0,
        }
    }

    /// Go by `name` from now on. The hub keeps its id, so paired devices
    /// stay paired; they learn the name at their next sync.
    pub fn rename(&mut self, name: &str) -> Result<(), String> {
        let name = clean_name(name).ok_or("A name needs at least one letter")?;
        if name == self.hub.name() {
            return Ok(());
        }
        if self.hub.is_syncing() {
            return Err("Syncing: try again in a moment".to_string());
        }
        // The old server lets go of its ports before the new one binds.
        self.server = None;
        let next = Service::named(&self.library, &name, self.net)?;
        *self = next;
        Ok(())
    }

    /// Stop serving. Requests under way end with their connection.
    pub fn stop(&mut self) {
        self.server = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_is_one_short_line() {
        assert_eq!(clean_name("  Desk\n"), Some("Desk".to_string()));
        assert_eq!(clean_name(" \t "), None);
        assert_eq!(
            clean_name("A very long name for a small screen"),
            Some("A very long name for".to_string())
        );
        assert!(!computer_name().is_empty());
    }

    #[test]
    fn a_game_s_core_is_the_cart_s_own_or_the_pc_s_default() {
        let dir = tempfile::tempdir().unwrap();
        let cores = Cores::new(dir.path());
        assert_eq!(cores.core(Platform::Gba, "Game"), Core::Mgba);
        assert_eq!(cores.core(Platform::Gbc, "Game"), Core::Gambatte);
        for folder in ["Roms/GBA", "Roms/GBA/Hacks"] {
            let config = dir.path().join(folder).join(".game_config");
            std::fs::create_dir_all(&config).unwrap();
            let stem = if folder.ends_with("Hacks") {
                "Deep"
            } else {
                "Game v1.1"
            };
            std::fs::write(
                config.join(format!("{stem}.cfg")),
                "core = \"gpsp_libretro\"\n",
            )
            .unwrap();
        }
        assert_eq!(cores.core(Platform::Gba, "Game v1.1"), Core::Gpsp);
        assert_eq!(cores.core(Platform::Gba, "Deep"), Core::Gpsp);
        assert_eq!(cores.core(Platform::Gba, "Other"), Core::Mgba);
        // A core that cannot run the system is no choice at all.
        assert_eq!(cores.core(Platform::Gbc, "Game v1.1"), Core::Gambatte);
    }

    #[test]
    fn the_hub_keeps_its_name_and_id_in_the_library() {
        let dir = tempfile::tempdir().unwrap();
        let first = Service::open(dir.path(), "Desk", None).unwrap();
        let id = first.hub().id().to_string();
        assert_eq!(first.hub().name(), "Desk");
        assert!(dir
            .path()
            .join("App/slot-mini-pc/data/sync/hub.txt")
            .is_file());
        drop(first);
        // The library's name wins over this PC's from then on.
        let mut again = Service::open(dir.path(), "Laptop", None).unwrap();
        assert_eq!(
            (again.hub().name(), again.hub().id()),
            ("Desk", id.as_str())
        );
        again.rename("  Attic ").unwrap();
        assert_eq!(
            (again.hub().name(), again.hub().id()),
            ("Attic", id.as_str())
        );
        assert!(again.rename("\n").is_err());
        drop(again);
        let last = Service::open(dir.path(), "Laptop", None).unwrap();
        assert_eq!(last.hub().name(), "Attic");
    }
}

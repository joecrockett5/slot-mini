//! Sync between slot-mini PC (the hub) and slot-mini on the Miyoo (the
//! client). The specification is `docs/SYNC.md`; read it first.
//!
//! Nothing here needs a window or a device. Without features the crate
//! holds the model, the planner and both engines, joined in-process by
//! [`DirectLink`]. `client` adds [`HttpLink`] and `server` adds [`Server`].

mod auth;
mod client;
mod discovery;
mod error;
mod hash;
mod hub;
mod key;
mod layout;
mod link;
mod model;
pub mod names;
pub mod plan;
mod report;
mod state;

#[cfg(feature = "client")]
mod http_client;
#[cfg(feature = "server")]
mod http_server;

pub use auth::{sign_reply, sign_request, SecretKey, REPLAY_WINDOW_SECS};
pub use client::{Client, HubRecord, PairingAttempt, PairingStatus};
pub use discovery::{
    discover, discover_broadcast, poke, Found, Poke, Poked, PokedStop, Responder, DISCOVERY_PORT,
    HUB_PORT, POKE_PORT,
};
pub use error::SyncError;
pub use hash::{hash_bytes, hash_file, HashCache};
pub use hub::{Conflict, DeviceInfo, Hub, HubEvent};
pub use key::{GameId, Key, Kind};
pub use layout::{CoreChoice, FixedCore, Layout};
pub use link::{DirectLink, Link};
pub use model::{Item, Manifest};
pub use names::NameRules;
pub use report::{Cancel, Event, Moved, Report, Scope, Side};

#[cfg(feature = "client")]
pub use http_client::HttpLink;
#[cfg(feature = "server")]
pub use http_server::Server;

/// The wire protocol's version. Both ends must have the same one.
pub const PROTOCOL_VERSION: u32 = 1;

/// Seconds since the Unix epoch on this machine's clock.
pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

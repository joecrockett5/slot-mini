//! What can go wrong, as the apps show it (docs/SYNC.md, "What each UI
//! must show").

use std::fmt;

use serde_json::{json, Value};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SyncError {
    /// This device has no hub.
    NotPaired,
    /// No hub answered.
    NoHub,
    /// The hub no longer knows this device.
    Unpaired,
    /// The two apps speak different protocol versions.
    Protocol {
        hub: u32,
        client: u32,
    },
    /// The hub is syncing with another device.
    HubBusy,
    /// A signature did not verify (`bad-auth`, `stale`, `replay`).
    Auth(String),
    /// The session is gone: the hub restarted or the sync was replaced.
    NoSession,
    /// A file was not what the plan said.
    HashMismatch,
    /// The PC is not in pairing mode.
    NotPairing,
    /// Pairing was cancelled or ran out of time.
    Rejected,
    Network(String),
    Io(String),
    /// A request the other side could not understand.
    Bad(String),
    Cancelled,
}

impl SyncError {
    /// The wire code and HTTP status.
    pub fn code(&self) -> (&'static str, u16) {
        match self {
            SyncError::NotPaired | SyncError::Unpaired => ("unknown-device", 401),
            SyncError::Auth(kind) => match kind.as_str() {
                "stale" => ("stale", 401),
                "replay" => ("replay", 401),
                _ => ("bad-auth", 401),
            },
            SyncError::Protocol { .. } => ("protocol", 426),
            SyncError::HubBusy => ("busy", 409),
            SyncError::NoSession => ("no-session", 409),
            SyncError::HashMismatch => ("hash-mismatch", 422),
            SyncError::NotPairing => ("not-pairing", 403),
            SyncError::Rejected => ("rejected", 403),
            SyncError::Bad(_) => ("bad-request", 400),
            SyncError::NoHub => ("not-found", 404),
            SyncError::Network(_) | SyncError::Io(_) | SyncError::Cancelled => ("io", 500),
        }
    }

    /// A name and detail that survive a report's JSON.
    pub(crate) fn tag(&self) -> (&'static str, String) {
        match self {
            SyncError::NotPaired => ("not-paired", String::new()),
            SyncError::NoHub => ("no-hub", String::new()),
            SyncError::Network(e) => ("network", e.clone()),
            SyncError::Io(e) => ("io", e.clone()),
            SyncError::Bad(e) => ("bad-request", e.clone()),
            SyncError::Cancelled => ("cancelled", String::new()),
            SyncError::Protocol { hub, .. } => ("protocol", hub.to_string()),
            SyncError::Auth(kind) => ("auth", kind.clone()),
            other => (other.code().0, String::new()),
        }
    }

    pub(crate) fn from_tag(tag: &str, detail: &str) -> SyncError {
        match tag {
            "not-paired" => SyncError::NotPaired,
            "no-hub" => SyncError::NoHub,
            "network" => SyncError::Network(detail.into()),
            "cancelled" => SyncError::Cancelled,
            "auth" => SyncError::Auth(detail.into()),
            "protocol" => SyncError::Protocol {
                hub: detail.parse().unwrap_or(0),
                client: crate::PROTOCOL_VERSION,
            },
            _ => SyncError::from_json(&json!({"error": {"code": tag, "message": detail}})),
        }
    }

    /// Whether the sync cannot go on after this (a failed item can).
    pub(crate) fn stops_sync(&self) -> bool {
        !matches!(
            self,
            SyncError::Io(_) | SyncError::HashMismatch | SyncError::Bad(_)
        )
    }

    /// The error body the hub sends.
    pub fn to_json(&self) -> Value {
        let mut v = json!({"error": {"code": self.code().0, "message": self.to_string()}});
        if let SyncError::Protocol { hub, .. } = self {
            v["error"]["protocol"] = json!(hub);
        }
        v
    }

    /// The error a reply's body stands for, as the client sees it.
    pub fn from_json(v: &Value) -> SyncError {
        let e = &v["error"];
        let message = e["message"].as_str().unwrap_or("").to_string();
        match e["code"].as_str().unwrap_or("") {
            "unknown-device" => SyncError::Unpaired,
            "bad-auth" => SyncError::Auth("bad-auth".into()),
            "stale" => SyncError::Auth("stale".into()),
            "replay" => SyncError::Auth("replay".into()),
            "protocol" => SyncError::Protocol {
                hub: e["protocol"].as_u64().unwrap_or(0) as u32,
                client: crate::PROTOCOL_VERSION,
            },
            "busy" => SyncError::HubBusy,
            "no-session" => SyncError::NoSession,
            "hash-mismatch" => SyncError::HashMismatch,
            "not-pairing" => SyncError::NotPairing,
            "rejected" => SyncError::Rejected,
            "bad-request" | "not-found" => SyncError::Bad(message),
            _ => SyncError::Io(message),
        }
    }
}

impl fmt::Display for SyncError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SyncError::NotPaired => write!(f, "Pair with slot-mini PC first"),
            SyncError::NoHub => write!(f, "The PC was not found on this network"),
            SyncError::Unpaired => write!(f, "This device was unpaired on the PC"),
            SyncError::Protocol { hub, client } if client < hub => {
                write!(f, "Update slot-mini on the Miyoo to sync with this PC")
            }
            SyncError::Protocol { .. } => {
                write!(f, "Update slot-mini PC to sync with this Miyoo")
            }
            SyncError::HubBusy => write!(f, "The PC is syncing with another device"),
            SyncError::Auth(_) => {
                write!(f, "The PC did not accept this device's key. Pair again")
            }
            SyncError::NoSession => write!(f, "The sync was interrupted on the PC"),
            SyncError::HashMismatch => write!(f, "A file changed while it was being sent"),
            SyncError::NotPairing => write!(f, "Open Pair a device on the PC first"),
            SyncError::Rejected => write!(f, "The PC declined"),
            SyncError::Network(_) => write!(f, "Lost the connection to the PC"),
            SyncError::Io(e) => write!(f, "Could not read or write a file: {e}"),
            SyncError::Bad(e) => write!(f, "The PC did not understand the request: {e}"),
            SyncError::Cancelled => write!(f, "Sync stopped"),
        }
    }
}

impl std::error::Error for SyncError {}

impl From<std::io::Error> for SyncError {
    fn from(e: std::io::Error) -> Self {
        SyncError::Io(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_protocol_mismatch_names_the_app_to_update() {
        let old_miyoo = SyncError::Protocol { hub: 2, client: 1 };
        assert_eq!(
            old_miyoo.to_string(),
            "Update slot-mini on the Miyoo to sync with this PC"
        );
        let old_pc = SyncError::Protocol { hub: 1, client: 2 };
        assert_eq!(
            old_pc.to_string(),
            "Update slot-mini PC to sync with this Miyoo"
        );
    }

    #[test]
    fn errors_survive_the_wire() {
        for e in [
            SyncError::Unpaired,
            SyncError::HubBusy,
            SyncError::NoSession,
            SyncError::HashMismatch,
            SyncError::NotPairing,
            SyncError::Rejected,
            SyncError::Auth("stale".into()),
            SyncError::Auth("replay".into()),
            SyncError::Auth("bad-auth".into()),
        ] {
            assert_eq!(SyncError::from_json(&e.to_json()), e);
        }
        let p = SyncError::Protocol { hub: 7, client: 1 };
        assert_eq!(
            SyncError::from_json(&p.to_json()),
            SyncError::Protocol {
                hub: 7,
                client: crate::PROTOCOL_VERSION
            }
        );
    }
}

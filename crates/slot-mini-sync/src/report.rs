//! What the apps pass in and render: scopes, progress events and the
//! summary of a sync (docs/SYNC.md, "What each UI must show").

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde_json::{json, Value};

use crate::error::SyncError;
use crate::key::{GameId, Key, Kind};

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Scope {
    /// Every item.
    Full,
    /// Everything except ROMs.
    Auto,
    /// One game's saves, label and art.
    Game(GameId),
}

impl Scope {
    pub fn parse(s: &str) -> Option<Scope> {
        match s {
            "full" => Some(Scope::Full),
            "auto" => Some(Scope::Auto),
            _ => GameId::parse(s.strip_prefix("game:")?).map(Scope::Game),
        }
    }

    /// Whether a file item is part of this scope.
    pub fn covers(&self, key: &Key) -> bool {
        match self {
            Scope::Full => true,
            Scope::Auto => key.kind != Kind::Rom,
            Scope::Game(g) => key.kind != Kind::Rom && key.game().folded() == g.folded(),
        }
    }

    /// Whether shells, favourites and Remove everywhere are handled.
    pub fn whole_library(&self) -> bool {
        !matches!(self, Scope::Game(_))
    }
}

impl std::fmt::Display for Scope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Scope::Full => write!(f, "full"),
            Scope::Auto => write!(f, "auto"),
            Scope::Game(g) => write!(f, "game:{g}"),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side {
    Client,
    Hub,
}

impl Side {
    pub fn as_str(self) -> &'static str {
        match self {
            Side::Client => "client",
            Side::Hub => "hub",
        }
    }

    pub fn parse(s: &str) -> Side {
        if s == "client" {
            Side::Client
        } else {
            Side::Hub
        }
    }
}

/// Stops a sync from another thread. Checked between pieces of a transfer
/// and between actions.
#[derive(Clone, Default)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    pub fn new() -> Cancel {
        Cancel::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }

    pub(crate) fn check(&self) -> Result<(), SyncError> {
        if self.is_cancelled() {
            Err(SyncError::Cancelled)
        } else {
            Ok(())
        }
    }
}

/// Progress, in the order it happens.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Connecting,
    /// Hashing this side's files, in bytes.
    Scanning {
        done: u64,
        total: u64,
    },
    /// Waiting for the hub's plan.
    Planning,
    /// One file on its way, in bytes. `index` counts from 0.
    Transfer {
        index: usize,
        count: usize,
        key: String,
        to_client: bool,
        done: u64,
        size: u64,
    },
    Finished(Box<Report>),
}

/// The keys one side received.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Moved {
    pub added: Vec<String>,
    pub updated: Vec<String>,
}

impl Moved {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.updated.is_empty()
    }

    fn to_json(&self) -> Value {
        json!({"added": self.added, "updated": self.updated})
    }

    fn from_json(v: &Value) -> Moved {
        Moved {
            added: strings(&v["added"]),
            updated: strings(&v["updated"]),
        }
    }
}

pub(crate) fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|s| s.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn pairs<T>(v: &Value, a: &str, b: impl Fn(&Value) -> T) -> Vec<(String, T)> {
    v.as_array()
        .map(|list| {
            list.iter()
                .filter_map(|e| Some((e["key"].as_str()?.to_string(), b(&e[a]))))
                .collect()
        })
        .unwrap_or_default()
}

/// The summary of one sync, the same value on both sides.
#[derive(Clone, Debug, PartialEq)]
pub struct Report {
    pub scope: Scope,
    pub to_client: Moved,
    pub to_hub: Moved,
    /// Saves kept twice: the key, and whose copy is now the save.
    pub conflicts: Vec<(String, Side)>,
    /// Games removed by Remove everywhere.
    pub removed: Vec<String>,
    /// ROMs left off the card: key and size.
    pub didnt_fit: Vec<(String, u64)>,
    /// Games skipped because they were running on the PC.
    pub busy: Vec<String>,
    /// Names a side cannot store.
    pub unstorable: Vec<(String, Side)>,
    /// Names that differ only by case on one side.
    pub duplicates: Vec<(String, Side)>,
    /// Items whose transfer failed: key and why.
    pub failed: Vec<(String, String)>,
    /// Whether shells or favourites changed on this side.
    pub text_changed: bool,
    /// What stopped the sync early.
    pub interrupted: Option<SyncError>,
}

impl Report {
    pub fn new(scope: Scope) -> Report {
        Report {
            scope,
            to_client: Moved::default(),
            to_hub: Moved::default(),
            conflicts: Vec::new(),
            removed: Vec::new(),
            didnt_fit: Vec::new(),
            busy: Vec::new(),
            unstorable: Vec::new(),
            duplicates: Vec::new(),
            failed: Vec::new(),
            text_changed: false,
            interrupted: None,
        }
    }

    /// Nothing here needs the player's attention.
    pub fn is_clean(&self) -> bool {
        self.conflicts.is_empty()
            && self.didnt_fit.is_empty()
            && self.busy.is_empty()
            && self.unstorable.is_empty()
            && self.duplicates.is_empty()
            && self.failed.is_empty()
            && self.interrupted.is_none()
    }

    /// Nothing moved, merged or went wrong: the two sides were in step.
    pub fn is_noop(&self) -> bool {
        self.is_clean()
            && self.to_client.is_empty()
            && self.to_hub.is_empty()
            && self.removed.is_empty()
            && !self.text_changed
    }

    /// Whether this side's library changed (the Miyoo reloads its shelf).
    pub fn changed_client(&self) -> bool {
        !self.to_client.is_empty() || !self.removed.is_empty() || self.text_changed
    }

    pub fn to_json(&self) -> Value {
        let sided = |list: &[(String, Side)], name: &str| -> Value {
            list.iter()
                .map(|(k, s)| json!({"key": k, name: s.as_str()}))
                .collect()
        };
        json!({
            "scope": self.scope.to_string(),
            "to_client": self.to_client.to_json(),
            "to_hub": self.to_hub.to_json(),
            "conflicts": sided(&self.conflicts, "winner"),
            "removed": self.removed,
            "didnt_fit": self.didnt_fit.iter()
                .map(|(k, n)| json!({"key": k, "size": n})).collect::<Value>(),
            "busy": self.busy,
            "unstorable": sided(&self.unstorable, "side"),
            "duplicates": sided(&self.duplicates, "side"),
            "failed": self.failed.iter()
                .map(|(k, e)| json!({"key": k, "error": e})).collect::<Value>(),
            "text_changed": self.text_changed,
            "interrupted": self.interrupted.as_ref().map(|e| {
                let (tag, detail) = e.tag();
                json!({"tag": tag, "detail": detail})
            }),
        })
    }

    pub fn from_json(v: &Value) -> Option<Report> {
        let side = |v: &Value| Side::parse(v.as_str().unwrap_or(""));
        let text = |v: &Value| v.as_str().unwrap_or("").to_string();
        Some(Report {
            scope: Scope::parse(v["scope"].as_str()?)?,
            to_client: Moved::from_json(&v["to_client"]),
            to_hub: Moved::from_json(&v["to_hub"]),
            conflicts: pairs(&v["conflicts"], "winner", side),
            removed: strings(&v["removed"]),
            didnt_fit: pairs(&v["didnt_fit"], "size", |n| n.as_u64().unwrap_or(0)),
            busy: strings(&v["busy"]),
            unstorable: pairs(&v["unstorable"], "side", side),
            duplicates: pairs(&v["duplicates"], "side", side),
            failed: pairs(&v["failed"], "error", text),
            text_changed: v["text_changed"].as_bool().unwrap_or(false),
            interrupted: v["interrupted"]["tag"].as_str().map(|tag| {
                SyncError::from_tag(tag, v["interrupted"]["detail"].as_str().unwrap_or(""))
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scopes_round_trip_and_cover() {
        let game = Scope::parse("game:GBA/Golden Sun").unwrap();
        for s in [Scope::Full, Scope::Auto, game.clone()] {
            assert_eq!(Scope::parse(&s.to_string()), Some(s));
        }
        assert!(Scope::parse("game:NES/x").is_none());
        let rom = Key::parse("rom:GBA/Golden Sun.gba").unwrap();
        let save = Key::parse("save:GBA/golden sun.srm").unwrap();
        let other = Key::parse("save:GBA/Other.srm").unwrap();
        assert!(Scope::Full.covers(&rom));
        assert!(!Scope::Auto.covers(&rom) && Scope::Auto.covers(&save));
        assert!(game.covers(&save) && !game.covers(&other) && !game.covers(&rom));
    }

    #[test]
    fn a_report_survives_json() {
        let mut r = Report::new(Scope::Full);
        assert!(r.is_clean() && r.is_noop());
        r.to_client.added.push("rom:GBA/A.gba".into());
        r.to_hub.updated.push("save:GBA/A.srm".into());
        r.conflicts.push(("save:GBA/B.srm".into(), Side::Client));
        r.removed.push("GBA/Old".into());
        r.didnt_fit.push(("rom:GBA/Big.gba".into(), 1 << 25));
        r.busy.push("GBA/Running".into());
        r.unstorable
            .push(("rom:GBA/What?.gba".into(), Side::Client));
        r.duplicates.push(("rom:GBA/game.gba".into(), Side::Hub));
        r.failed.push(("label:GBA/A".into(), "disk full".into()));
        r.text_changed = true;
        r.interrupted = Some(SyncError::Network("reset".into()));
        assert!(!r.is_clean());
        assert_eq!(Report::from_json(&r.to_json()), Some(r));
    }
}

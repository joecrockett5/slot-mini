//! A side's list of items (docs/SYNC.md, "Manifests and hashing").

use serde_json::{json, Value};

use crate::key::{GameId, Key};
use crate::names::NameRules;
use crate::report::strings;

/// One file item as a side has it.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    /// In this side's spelling.
    pub key: Key,
    pub size: u64,
    /// SHA-256, hex.
    pub hash: String,
    /// Saves only: when the game was last played here (the save's
    /// modification time, Unix seconds).
    pub played: Option<u64>,
}

/// What one side holds, for one scope.
#[derive(Clone, Debug, PartialEq)]
pub struct Manifest {
    /// This side's clock, to turn `played` into an age.
    pub now: u64,
    /// Free space, where it is known and matters (the Miyoo's card).
    pub free: Option<u64>,
    pub names: NameRules,
    /// Every game whose ROM is here, whatever the scope.
    pub games: Vec<GameId>,
    pub items: Vec<Item>,
    /// Shell lines: stem and value.
    pub shells: Vec<(String, String)>,
    pub favourites: Vec<GameId>,
}

impl Manifest {
    pub fn empty(names: NameRules) -> Manifest {
        Manifest {
            now: 0,
            free: None,
            names,
            games: Vec::new(),
            items: Vec::new(),
            shells: Vec::new(),
            favourites: Vec::new(),
        }
    }

    pub fn to_json(&self) -> Value {
        json!({
            "now": self.now,
            "free": self.free,
            "names": self.names.as_str(),
            "games": self.games.iter().map(|g| g.to_string()).collect::<Vec<_>>(),
            "items": self.items.iter().map(|i| {
                let mut v = json!({"key": i.key.to_string(), "size": i.size, "hash": i.hash});
                if let Some(p) = i.played {
                    v["played"] = json!(p);
                }
                v
            }).collect::<Vec<_>>(),
            "shells": self.shells.iter().map(|(s, v)| json!([s, v])).collect::<Vec<_>>(),
            "favourites": self.favourites.iter().map(|g| g.favourite_line()).collect::<Vec<_>>(),
        })
    }

    /// `None` for a manifest that is not well formed. Entries that do not
    /// parse are refused rather than skipped: a side must not plan from
    /// half a list.
    pub fn from_json(v: &Value) -> Option<Manifest> {
        let mut items = Vec::new();
        for i in v["items"].as_array()? {
            let hash = i["hash"].as_str()?;
            if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
                return None;
            }
            items.push(Item {
                key: Key::parse(i["key"].as_str()?)?,
                size: i["size"].as_u64()?,
                hash: hash.to_ascii_lowercase(),
                played: i["played"].as_u64(),
            });
        }
        let mut shells = Vec::new();
        for s in v["shells"].as_array()? {
            shells.push((s[0].as_str()?.to_string(), s[1].as_str()?.to_string()));
        }
        Some(Manifest {
            now: v["now"].as_u64()?,
            free: v["free"].as_u64(),
            names: NameRules::parse(v["names"].as_str()?),
            games: strings(&v["games"])
                .iter()
                .map(|g| GameId::parse(g))
                .collect::<Option<_>>()?,
            items,
            shells,
            favourites: strings(&v["favourites"])
                .iter()
                .map(|line| {
                    slot_mini_store::list_entry(line).map(|(sys, stem)| GameId::new(sys, stem))
                })
                .collect::<Option<_>>()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slot_mini_store::Platform;

    #[test]
    fn a_manifest_survives_json() {
        let m = Manifest {
            now: 1_700_000_000,
            free: Some(1 << 30),
            names: NameRules::Fat,
            games: vec![GameId::new(Platform::Gba, "Golden Sun (USA)")],
            items: vec![
                Item {
                    key: Key::parse("save:GBA/Golden Sun (USA).srm").unwrap(),
                    size: 65536,
                    hash: crate::hash_bytes(b"save"),
                    played: Some(1_699_990_000),
                },
                Item {
                    key: Key::parse("rom:GBA/Hacks/Golden Sun (USA).GBA").unwrap(),
                    size: 1,
                    hash: crate::hash_bytes(b"rom"),
                    played: None,
                },
            ],
            shells: vec![("Tetris (World)".into(), "notched 2f5cc0 glitter".into())],
            favourites: vec![GameId::new(Platform::Gb, "Tetris (World)")],
        };
        assert_eq!(Manifest::from_json(&m.to_json()), Some(m.clone()));
        let mut bad = m.to_json();
        bad["items"][0]["key"] = json!("save:NES/x.srm");
        assert_eq!(Manifest::from_json(&bad), None);
        let mut bad = m.to_json();
        bad["items"][0]["hash"] = json!("abc");
        assert_eq!(Manifest::from_json(&bad), None);
    }
}

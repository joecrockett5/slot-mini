//! The files sync keeps for itself (docs/SYNC.md, "Stored state").

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use slot_mini_store::{atomic_write, ini};

use crate::auth::SecretKey;
use crate::hash::{hex, unhex};
use crate::key::GameId;
use crate::plan::Base;

fn read_ini(path: &Path) -> HashMap<String, String> {
    ini::parse(&std::fs::read_to_string(path).unwrap_or_default())
}

fn write_ini(path: &Path, entries: &[(&str, Option<String>)]) -> std::io::Result<()> {
    let mut text = std::fs::read_to_string(path).unwrap_or_default();
    for (key, value) in entries {
        let value = value.as_ref().map(|v| v.replace(['\n', '\r'], " "));
        text = ini::set(&text, key, value.as_deref())?;
    }
    write(path, text.as_bytes())
}

fn write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    atomic_write(path, bytes)
}

fn key_of(text: Option<&String>) -> Option<SecretKey> {
    unhex(text?)?.try_into().ok()
}

fn lines(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

fn games(path: &Path) -> Vec<GameId> {
    lines(path)
        .iter()
        .filter_map(|l| GameId::parse(l))
        .collect()
}

fn write_games(path: &Path, games: &[GameId]) -> std::io::Result<()> {
    let text: String = games.iter().map(|g| format!("{g}\n")).collect();
    write(path, text.as_bytes())
}

/// A paired device, as the hub remembers it.
#[derive(Clone)]
pub(crate) struct Device {
    pub id: String,
    pub name: String,
    pub key: SecretKey,
    /// Where its last signed request came from.
    pub address: String,
    pub paired: u64,
    pub last_sync: u64,
}

/// A save kept twice (docs/SYNC.md, "Save rules").
#[derive(Clone, Debug, PartialEq)]
pub struct Conflict {
    pub id: String,
    /// The save's key, as the hub spells it.
    pub key: String,
    /// The device whose sync found it.
    pub device: String,
    pub stamp: String,
    /// The conflict copy, under the hub's save folder.
    pub file: String,
    /// The copy's hash.
    pub hash: String,
}

pub(crate) struct HubStore {
    dir: PathBuf,
}

impl HubStore {
    pub fn new(dir: &Path) -> HubStore {
        HubStore {
            dir: dir.to_path_buf(),
        }
    }

    pub fn identity(&self) -> (Option<String>, Option<String>) {
        let v = read_ini(&self.dir.join("hub.txt"));
        (v.get("id").cloned(), v.get("name").cloned())
    }

    pub fn set_identity(&self, id: &str, name: &str) -> std::io::Result<()> {
        write_ini(
            &self.dir.join("hub.txt"),
            &[("id", Some(id.into())), ("name", Some(name.into()))],
        )
    }

    fn device_dir(&self, id: &str) -> PathBuf {
        self.dir.join("devices").join(id)
    }

    pub fn devices(&self) -> HashMap<String, Device> {
        let mut out = HashMap::new();
        let Ok(entries) = std::fs::read_dir(self.dir.join("devices")) else {
            return out;
        };
        for entry in entries.flatten() {
            let Ok(id) = entry.file_name().into_string() else {
                continue;
            };
            let v = read_ini(&entry.path().join("device.txt"));
            let Some(key) = key_of(v.get("key")) else {
                continue;
            };
            let number = |k: &str| v.get(k).and_then(|n| n.parse().ok()).unwrap_or(0);
            out.insert(
                id.clone(),
                Device {
                    id,
                    name: v.get("name").cloned().unwrap_or_default(),
                    key,
                    address: v.get("address").cloned().unwrap_or_default(),
                    paired: number("paired"),
                    last_sync: number("last_sync"),
                },
            );
        }
        out
    }

    pub fn save_device(&self, d: &Device) -> std::io::Result<()> {
        write_ini(
            &self.device_dir(&d.id).join("device.txt"),
            &[
                ("name", Some(d.name.clone())),
                ("key", Some(hex(&d.key))),
                ("address", Some(d.address.clone())),
                ("paired", Some(d.paired.to_string())),
                ("last_sync", Some(d.last_sync.to_string())),
            ],
        )
    }

    /// Forget a device and its base.
    pub fn delete_device(&self, id: &str) -> std::io::Result<()> {
        match std::fs::remove_dir_all(self.device_dir(id)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }

    fn base_file(&self, id: &str) -> PathBuf {
        self.device_dir(id).join("base.tsv")
    }

    /// The base is a journal: later lines win, and an empty value removes.
    pub fn base(&self, id: &str) -> Base {
        let mut base = Base::new();
        for line in lines(&self.base_file(id)) {
            match line.split_once('\t') {
                Some((key, "")) => base.remove(key),
                Some((key, value)) => base.insert(key.to_string(), value.to_string()),
                None => None,
            };
        }
        base
    }

    pub fn append_base(
        &self,
        id: &str,
        entries: &[(String, Option<String>)],
    ) -> std::io::Result<()> {
        if entries.is_empty() {
            return Ok(());
        }
        let path = self.base_file(id);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut text = String::new();
        for (key, value) in entries {
            let value = value.as_deref().unwrap_or("");
            if !key.contains(['\t', '\n']) && !value.contains(['\t', '\n']) {
                text += &format!("{key}\t{value}\n");
            }
        }
        let mut file = std::fs::File::options()
            .create(true)
            .append(true)
            .open(path)?;
        file.write_all(text.as_bytes())?;
        file.sync_data()
    }

    /// Replace the journal with the base as it stands.
    pub fn rewrite_base(&self, id: &str, base: &Base) -> std::io::Result<()> {
        let mut entries: Vec<String> = base
            .iter()
            .filter(|(k, v)| !k.contains(['\t', '\n']) && !v.contains(['\t', '\n']))
            .map(|(k, v)| format!("{k}\t{v}\n"))
            .collect();
        entries.sort();
        write(&self.base_file(id), entries.concat().as_bytes())
    }

    pub fn tombstones(&self) -> Vec<GameId> {
        games(&self.dir.join("tombstones.txt"))
    }

    pub fn set_tombstones(&self, list: &[GameId]) -> std::io::Result<()> {
        write_games(&self.dir.join("tombstones.txt"), list)
    }

    /// The tombstones a device has applied.
    pub fn acked(&self, id: &str) -> Vec<GameId> {
        games(&self.device_dir(id).join("acked.txt"))
    }

    pub fn set_acked(&self, id: &str, list: &[GameId]) -> std::io::Result<()> {
        write_games(&self.device_dir(id).join("acked.txt"), list)
    }

    pub fn conflicts(&self) -> Vec<Conflict> {
        lines(&self.dir.join("conflicts.tsv"))
            .iter()
            .filter_map(|line| {
                let mut f = line.split('\t').map(str::to_string);
                Some(Conflict {
                    id: f.next()?,
                    key: f.next()?,
                    device: f.next()?,
                    stamp: f.next()?,
                    file: f.next()?,
                    hash: f.next()?,
                })
            })
            .collect()
    }

    pub fn set_conflicts(&self, list: &[Conflict]) -> std::io::Result<()> {
        let clean = |s: &str| s.replace(['\t', '\n'], " ");
        let text: String = list
            .iter()
            .map(|c| {
                format!(
                    "{}\t{}\t{}\t{}\t{}\t{}\n",
                    c.id,
                    clean(&c.key),
                    clean(&c.device),
                    c.stamp,
                    clean(&c.file),
                    c.hash
                )
            })
            .collect();
        write(&self.dir.join("conflicts.tsv"), text.as_bytes())
    }

    pub fn last_report(&self) -> Option<serde_json::Value> {
        serde_json::from_slice(&std::fs::read(self.dir.join("last_report.json")).ok()?).ok()
    }

    pub fn set_last_report(&self, report: &serde_json::Value) -> std::io::Result<()> {
        write(
            &self.dir.join("last_report.json"),
            report.to_string().as_bytes(),
        )
    }

    pub fn hashes(&self) -> PathBuf {
        self.dir.join("hashes.tsv")
    }
}

/// The hub a device is paired with.
#[derive(Clone, Debug, PartialEq)]
pub struct HubRecord {
    pub id: String,
    pub name: String,
    /// `host:port` of its server, as last seen.
    pub address: String,
    pub(crate) key: SecretKey,
    /// The hub's clock (milliseconds) at the last sync or accepted poke.
    pub(crate) hub_time: u64,
}

pub(crate) struct ClientStore {
    file: PathBuf,
}

impl ClientStore {
    pub fn new(dir: &Path) -> ClientStore {
        ClientStore {
            file: dir.join("client.txt"),
        }
    }

    pub fn hashes(&self) -> PathBuf {
        self.file.with_file_name("hashes.tsv")
    }

    pub fn identity(&self) -> (Option<String>, Option<String>) {
        let v = read_ini(&self.file);
        (v.get("device_id").cloned(), v.get("device_name").cloned())
    }

    pub fn set_identity(&self, id: &str, name: &str) -> std::io::Result<()> {
        write_ini(
            &self.file,
            &[
                ("device_id", Some(id.into())),
                ("device_name", Some(name.into())),
            ],
        )
    }

    pub fn hub(&self) -> Option<HubRecord> {
        let v = read_ini(&self.file);
        Some(HubRecord {
            id: v.get("hub_id")?.clone(),
            name: v.get("hub_name").cloned().unwrap_or_default(),
            address: v.get("address").cloned().unwrap_or_default(),
            key: key_of(v.get("key"))?,
            hub_time: v.get("hub_time").and_then(|n| n.parse().ok()).unwrap_or(0),
        })
    }

    pub fn set_hub(&self, hub: Option<&HubRecord>) -> std::io::Result<()> {
        let field = |f: &dyn Fn(&HubRecord) -> String| hub.map(f);
        write_ini(
            &self.file,
            &[
                ("hub_id", field(&|h| h.id.clone())),
                ("hub_name", field(&|h| h.name.clone())),
                ("key", field(&|h| hex(&h.key))),
                ("address", field(&|h| h.address.clone())),
                ("hub_time", field(&|h| h.hub_time.to_string())),
            ],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slot_mini_store::Platform;

    #[test]
    fn the_hub_store_remembers() {
        let d = tempfile::tempdir().unwrap();
        let store = HubStore::new(d.path());
        assert_eq!(store.identity(), (None, None));
        store.set_identity("abc", "Joe's PC").unwrap();
        assert_eq!(
            store.identity(),
            (Some("abc".into()), Some("Joe's PC".into()))
        );

        let device = Device {
            id: "dev1".into(),
            name: "Miyoo = mine\nreally".into(),
            key: [9; 32],
            address: "192.168.1.5".into(),
            paired: 5,
            last_sync: 7,
        };
        store.save_device(&device).unwrap();
        let back = &HubStore::new(d.path()).devices()["dev1"];
        assert_eq!(back.key, [9; 32]);
        assert_eq!((back.paired, back.last_sync), (5, 7));
        assert_eq!(back.address, "192.168.1.5");

        // The base journal: later lines win, and it survives a rewrite.
        let set = |k: &str, v: Option<&str>| (k.to_string(), v.map(str::to_string));
        store
            .append_base("dev1", &[set("a", Some("1")), set("b", Some("2"))])
            .unwrap();
        store
            .append_base(
                "dev1",
                &[
                    set("a", Some("3")),
                    set("b", None),
                    set("bad\tkey", Some("x")),
                ],
            )
            .unwrap();
        let base = store.base("dev1");
        assert_eq!(base, Base::from([("a".to_string(), "3".to_string())]));
        store.rewrite_base("dev1", &base).unwrap();
        assert_eq!(store.base("dev1"), base);
        assert_eq!(
            std::fs::read_to_string(d.path().join("devices/dev1/base.tsv")).unwrap(),
            "a\t3\n"
        );

        let game = GameId::new(Platform::Gba, "Golden Sun");
        store.set_tombstones(std::slice::from_ref(&game)).unwrap();
        store
            .set_acked("dev1", std::slice::from_ref(&game))
            .unwrap();
        assert_eq!(store.tombstones(), std::slice::from_ref(&game));
        assert_eq!(store.acked("dev1"), [game]);
        let conflict = Conflict {
            id: "1".into(),
            key: "save:GBA/X.srm".into(),
            device: "Miyoo".into(),
            stamp: "20260101000000".into(),
            file: "mGBA/X.srm.conflict-miyoo-20260101000000".into(),
            hash: "00".into(),
        };
        store
            .set_conflicts(std::slice::from_ref(&conflict))
            .unwrap();
        assert_eq!(store.conflicts(), [conflict]);

        store.delete_device("dev1").unwrap();
        store.delete_device("dev1").unwrap();
        assert!(store.devices().is_empty() && store.base("dev1").is_empty());
    }

    #[test]
    fn the_client_store_remembers() {
        let d = tempfile::tempdir().unwrap();
        let store = ClientStore::new(&d.path().join("sync"));
        assert!(store.hub().is_none());
        store.set_identity("id1", "Miyoo Mini Plus").unwrap();
        let hub = HubRecord {
            id: "hub1".into(),
            name: "PC".into(),
            address: "10.0.0.2:24735".into(),
            key: [3; 32],
            hub_time: 99,
        };
        store.set_hub(Some(&hub)).unwrap();
        assert_eq!(store.hub(), Some(hub));
        assert_eq!(store.identity().0.as_deref(), Some("id1"));
        store.set_hub(None).unwrap();
        assert!(store.hub().is_none());
        assert_eq!(store.identity().0.as_deref(), Some("id1"));
    }
}

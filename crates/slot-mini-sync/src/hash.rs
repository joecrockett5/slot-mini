//! SHA-256 of files, and the cache that saves hashing ROMs again
//! (docs/SYNC.md, "Manifests and hashing").

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

use ring::digest::{Context, SHA256};

/// How much of a file is read, sent or written at a time.
pub const PIECE: usize = 64 * 1024;

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn unhex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) || !s.is_ascii() {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

pub fn hash_bytes(bytes: &[u8]) -> String {
    hex(ring::digest::digest(&SHA256, bytes).as_ref())
}

/// A running SHA-256.
pub struct Hasher(Context);

impl Hasher {
    pub fn new() -> Hasher {
        Hasher(Context::new(&SHA256))
    }

    pub fn update(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }

    pub fn finish(self) -> String {
        hex(self.0.finish().as_ref())
    }
}

/// The hash and size of a file, read a piece at a time. `seen` is told how
/// many bytes each piece held.
pub fn hash_file(path: &Path, seen: &mut dyn FnMut(u64)) -> std::io::Result<(String, u64)> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Hasher::new();
    let mut buf = vec![0u8; PIECE];
    let mut size = 0u64;
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        size += n as u64;
        seen(n as u64);
    }
    Ok((hasher.finish(), size))
}

fn mtime_ns(meta: &std::fs::Metadata) -> u128 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

/// Modification time in Unix seconds.
pub fn mtime_secs(meta: &std::fs::Metadata) -> u64 {
    (mtime_ns(meta) / 1_000_000_000) as u64
}

struct Cached {
    size: u64,
    mtime: u128,
    /// When it was hashed, in this machine's nanoseconds.
    at: u128,
    hash: String,
}

/// A file may have been written again inside its time stamp's resolution
/// (2 s on FAT) after it was hashed; such an entry is not trusted.
const RACY_NS: u128 = 3_000_000_000;

/// Hashes by path, size and modification time.
#[derive(Default)]
pub struct HashCache {
    map: HashMap<String, Cached>,
    dirty: bool,
}

impl HashCache {
    pub fn load(file: &Path) -> HashCache {
        let mut map = HashMap::new();
        for line in std::fs::read_to_string(file).unwrap_or_default().lines() {
            let mut f = line.splitn(5, '\t');
            let (Some(size), Some(mtime), Some(at), Some(hash), Some(path)) =
                (f.next(), f.next(), f.next(), f.next(), f.next())
            else {
                continue;
            };
            let (Ok(size), Ok(mtime), Ok(at)) = (size.parse(), mtime.parse(), at.parse()) else {
                continue;
            };
            if hash.len() == 64 {
                map.insert(
                    path.to_string(),
                    Cached {
                        size,
                        mtime,
                        at,
                        hash: hash.to_string(),
                    },
                );
            }
        }
        HashCache { map, dirty: false }
    }

    /// Write the cache if it changed.
    pub fn save(&mut self, file: &Path) -> std::io::Result<()> {
        if !self.dirty {
            return Ok(());
        }
        let mut lines: Vec<String> = self
            .map
            .iter()
            .filter(|(path, _)| !path.contains(['\t', '\n']))
            .map(|(path, c)| format!("{}\t{}\t{}\t{}\t{path}\n", c.size, c.mtime, c.at, c.hash))
            .collect();
        lines.sort();
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        slot_mini_store::atomic_write(file, lines.concat().as_bytes())?;
        self.dirty = false;
        Ok(())
    }

    /// The cached hash of the file named `name`, if it still stands.
    pub fn get(&self, name: &str, meta: &std::fs::Metadata) -> Option<String> {
        let c = self.map.get(name)?;
        let mtime = mtime_ns(meta);
        (c.size == meta.len() && c.mtime == mtime && mtime + RACY_NS < c.at).then(|| c.hash.clone())
    }

    pub fn put(&mut self, name: &str, meta: &std::fs::Metadata, hash: &str) {
        let at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        self.map.insert(
            name.to_string(),
            Cached {
                size: meta.len(),
                mtime: mtime_ns(meta),
                at,
                hash: hash.to_string(),
            },
        );
        self.dirty = true;
    }

    /// Drop entries for files that are no longer listed.
    pub fn keep_only(&mut self, live: &std::collections::HashSet<String>) {
        let before = self.map.len();
        self.map.retain(|k, _| live.contains(k));
        self.dirty |= self.map.len() != before;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_match_known_values() {
        assert_eq!(
            hash_bytes(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("f");
        std::fs::write(&p, vec![7u8; PIECE * 2 + 5]).unwrap();
        let mut seen = 0;
        let (hash, size) = hash_file(&p, &mut |n| seen += n).unwrap();
        assert_eq!(size, (PIECE * 2 + 5) as u64);
        assert_eq!(seen, size);
        assert_eq!(hash, hash_bytes(&vec![7u8; PIECE * 2 + 5]));
        assert_eq!(unhex(&hex(&[0, 255, 16])).unwrap(), vec![0, 255, 16]);
        assert!(unhex("xyz").is_none());
    }

    #[test]
    fn the_cache_trusts_only_settled_unchanged_files() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("rom.gba");
        std::fs::write(&p, b"rom").unwrap();
        let old = std::time::SystemTime::now() - std::time::Duration::from_secs(60);
        std::fs::File::options()
            .write(true)
            .open(&p)
            .unwrap()
            .set_modified(old)
            .unwrap();
        let meta = std::fs::metadata(&p).unwrap();
        let mut cache = HashCache::default();
        assert!(cache.get("rom.gba", &meta).is_none());
        cache.put("rom.gba", &meta, &hash_bytes(b"rom"));
        assert_eq!(cache.get("rom.gba", &meta), Some(hash_bytes(b"rom")));
        // It survives a save and a load.
        let file = d.path().join("sync/hashes.tsv");
        cache.save(&file).unwrap();
        let cache = HashCache::load(&file);
        assert_eq!(cache.get("rom.gba", &meta), Some(hash_bytes(b"rom")));
        // A new size or time is a miss.
        std::fs::write(&p, b"rom!").unwrap();
        let meta = std::fs::metadata(&p).unwrap();
        assert!(cache.get("rom.gba", &meta).is_none());
        // Just written: its time is too close to the hashing to trust.
        let mut cache = cache;
        cache.put("rom.gba", &meta, &hash_bytes(b"rom!"));
        assert!(cache.get("rom.gba", &meta).is_none());
    }
}

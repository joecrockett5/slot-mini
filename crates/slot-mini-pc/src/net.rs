//! What the PC app asks the internet for: whether a newer PC release is
//! out, and a core RetroArch is missing. Both block, so they are called on
//! worker threads (see `PcApp`).

use std::path::Path;
use std::time::Duration;

use slot_mini::update::{is_newer, version};
use slot_mini_store::Core;

/// Where the releases are.
pub const REPO: &str = "joecrockett5/slot-mini";

/// PC releases are tagged `pc-v<version>`; the Miyoo's are `v<version>`.
pub const TAG_PREFIX: &str = "pc-v";

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const API_TIMEOUT: Duration = Duration::from_secs(20);
const CORE_TIMEOUT: Duration = Duration::from_secs(180);
/// No core is anywhere near this large.
const CORE_LIMIT: u64 = 64 << 20;

/// GitHub's list of the repository's releases.
pub fn releases_url(repo: &str) -> String {
    format!("https://api.github.com/repos/{repo}/releases?per_page=100")
}

/// A PC release newer than this one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PcRelease {
    pub version: String,
    /// Its page on GitHub, where the zip is.
    pub page: String,
}

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(CONNECT_TIMEOUT))
        .timeout_global(Some(timeout))
        .user_agent(format!("slot-mini-pc/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

/// The newest published `pc-v<x.y.z>` release in GitHub's list, whatever
/// else (the Miyoo's releases, drafts) is in it.
pub fn newest_pc(json: &str) -> Option<PcRelease> {
    let list: serde_json::Value = serde_json::from_str(json).ok()?;
    list.as_array()?
        .iter()
        .filter(|r| r["draft"].as_bool() != Some(true) && r["prerelease"].as_bool() != Some(true))
        .filter_map(|r| {
            let v = r["tag_name"].as_str()?.strip_prefix(TAG_PREFIX)?;
            // Exactly x.y.z: nothing before or after it.
            let parsed = version(v).filter(|_| !v.contains(['-', '+', 'v', ' ']))?;
            Some((parsed, v.to_string(), r["html_url"].as_str()?.to_string()))
        })
        .max_by_key(|(parsed, ..)| *parsed)
        .map(|(_, version, page)| PcRelease { version, page })
}

/// Ask `url` (see [`releases_url`]) for a PC release newer than `current`.
pub fn check(url: &str, current: &str) -> Result<Option<PcRelease>, String> {
    let response = agent(API_TIMEOUT)
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| match e {
            ureq::Error::StatusCode(code) => format!("GitHub answered {code}."),
            _ => "No connection.".to_string(),
        })?;
    let text = response
        .into_body()
        .into_with_config()
        .limit(8 << 20)
        .read_to_string()
        .map_err(|_| "No connection.".to_string())?;
    Ok(newest_pc(&text).filter(|r| is_newer(&r.version, current)))
}

fn u16_at(b: &[u8], at: usize) -> Option<usize> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?) as usize)
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// The file called `name` (in any folder) from a zip, checked against its
/// CRC. Stored and deflated entries only, which is what the buildbot
/// writes.
pub fn unzip_one(zip: &[u8], name: &str) -> Option<Vec<u8>> {
    let floor = zip.len().saturating_sub(22 + 0xffff);
    let eocd = (floor..=zip.len().checked_sub(22)?)
        .rev()
        .find(|&i| u32_at(zip, i) == Some(0x0605_4b50))?;
    let count = u16_at(zip, eocd + 10)?;
    let mut at = u32_at(zip, eocd + 16)? as usize;
    for _ in 0..count {
        if u32_at(zip, at)? != 0x0201_4b50 {
            return None;
        }
        let flags = u16_at(zip, at + 8)?;
        let method = u16_at(zip, at + 10)?;
        let crc = u32_at(zip, at + 16)?;
        let packed = u32_at(zip, at + 20)? as usize;
        let size = u32_at(zip, at + 24)? as usize;
        let (name_len, extra, comment) = (
            u16_at(zip, at + 28)?,
            u16_at(zip, at + 30)?,
            u16_at(zip, at + 32)?,
        );
        let local = u32_at(zip, at + 42)? as usize;
        let entry = String::from_utf8_lossy(zip.get(at + 46..at + 46 + name_len)?).into_owned();
        at += 46 + name_len + extra + comment;
        if entry.rsplit(['/', '\\']).next() != Some(name) {
            continue;
        }
        if flags & 1 != 0 || size as u64 > CORE_LIMIT || u32_at(zip, local)? != 0x0403_4b50 {
            return None;
        }
        let skip = 30 + u16_at(zip, local + 26)? + u16_at(zip, local + 28)?;
        let data = zip.get(local + skip..local + skip + packed)?;
        let bytes = match method {
            0 => data.to_vec(),
            8 => miniz_oxide::inflate::decompress_to_vec_with_limit(data, size).ok()?,
            _ => return None,
        };
        return (bytes.len() == size && crc32fast::hash(&bytes) == crc).then_some(bytes);
    }
    None
}

/// Download `core` from a buildbot folder (`base`, see
/// `retroarch::BUILDBOT`) into `cores`. A core that is already there is
/// left alone.
pub fn fetch_core(base: &str, core: Core, cores: &Path) -> Result<(), String> {
    let dest = crate::retroarch::core_file(cores, core);
    if dest.exists() {
        return Ok(());
    }
    let dll = format!("{}.dll", core.libretro());
    let url = format!("{}/{dll}.zip", base.trim_end_matches('/'));
    let failed = || format!("{} could not be downloaded.", core.text());
    let response = agent(CORE_TIMEOUT).get(&url).call().map_err(|e| {
        eprintln!("slot-mini-pc: {url}: {e}");
        failed()
    })?;
    let zip = response
        .into_body()
        .into_with_config()
        .limit(CORE_LIMIT)
        .read_to_vec()
        .map_err(|_| failed())?;
    let bytes = unzip_one(&zip, &dll)
        .ok_or_else(|| format!("{}: the download was damaged.", core.text()))?;
    let write = || -> std::io::Result<()> {
        std::fs::create_dir_all(cores)?;
        // Whole or not at all: RetroArch must never load half a core.
        let part = dest.with_extension("part");
        std::fs::write(&part, &bytes)?;
        std::fs::rename(&part, &dest)
    };
    write().map_err(|e| {
        eprintln!("slot-mini-pc: {}: {e}", dest.display());
        format!("Could not write to {}.", cores.display())
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;

    /// A stand-in for GitHub and the buildbot on this machine: fixed
    /// answers by path. Returns its base URL.
    pub fn serve(files: HashMap<String, Vec<u8>>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                if reader.read_line(&mut line).is_err() {
                    continue;
                }
                let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
                loop {
                    let mut h = String::new();
                    if reader.read_line(&mut h).unwrap_or(0) == 0 || h == "\r\n" {
                        break;
                    }
                }
                let reply = match files.get(&path) {
                    Some(body) => {
                        let mut r = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        )
                        .into_bytes();
                        r.extend_from_slice(body);
                        r
                    }
                    None => {
                        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                            .to_vec()
                    }
                };
                let _ = stream.write_all(&reply);
            }
        });
        base
    }

    /// A zip holding one deflated file.
    pub fn zip_of(name: &str, bytes: &[u8]) -> Vec<u8> {
        let packed = miniz_oxide::deflate::compress_to_vec(bytes, 6);
        let header = |sig: u32, central: bool| {
            let mut h = Vec::new();
            h.extend(sig.to_le_bytes());
            if central {
                h.extend(20u16.to_le_bytes());
            }
            h.extend(20u16.to_le_bytes());
            h.extend(0u16.to_le_bytes());
            h.extend(8u16.to_le_bytes());
            h.extend(0u32.to_le_bytes());
            h.extend(crc32fast::hash(bytes).to_le_bytes());
            h.extend((packed.len() as u32).to_le_bytes());
            h.extend((bytes.len() as u32).to_le_bytes());
            h.extend((name.len() as u16).to_le_bytes());
            h.extend(0u16.to_le_bytes());
            if central {
                h.extend([0u8; 10]);
                h.extend(0u32.to_le_bytes());
            }
            h.extend(name.as_bytes());
            h
        };
        let mut out = header(0x0403_4b50, false);
        out.extend(&packed);
        let dir = header(0x0201_4b50, true);
        let dir_at = out.len() as u32;
        out.extend(&dir);
        out.extend(0x0605_4b50u32.to_le_bytes());
        out.extend([0u8; 4]);
        out.extend(1u16.to_le_bytes());
        out.extend(1u16.to_le_bytes());
        out.extend((dir.len() as u32).to_le_bytes());
        out.extend(dir_at.to_le_bytes());
        out.extend(0u16.to_le_bytes());
        out
    }

    pub const RELEASES: &str = r#"[
        {"tag_name": "v0.7.0", "html_url": "https://example.test/v0.7.0"},
        {"tag_name": "pc-v0.2.0", "html_url": "https://example.test/pc-v0.2.0"},
        {"tag_name": "pc-v0.10.1", "html_url": "https://example.test/pc-v0.10.1"},
        {"tag_name": "pc-v0.11.0", "draft": true, "html_url": "https://example.test/draft"},
        {"tag_name": "pc-v9.0.0-rc1", "html_url": "https://example.test/rc"},
        {"tag_name": "pc-vnext", "html_url": "https://example.test/next"}
    ]"#;

    #[test]
    fn only_published_pc_tags_count_and_the_highest_wins() {
        let newest = newest_pc(RELEASES).unwrap();
        assert_eq!(newest.version, "0.10.1");
        assert_eq!(newest.page, "https://example.test/pc-v0.10.1");
        assert_eq!(
            newest_pc(r#"[{"tag_name": "v0.7.0", "html_url": "x"}]"#),
            None
        );
        assert_eq!(newest_pc("not json"), None);
    }

    #[test]
    fn the_check_offers_only_a_newer_release() {
        let base = serve(HashMap::from([(
            "/releases".to_string(),
            RELEASES.as_bytes().to_vec(),
        )]));
        let url = format!("{base}/releases");
        assert_eq!(check(&url, "0.1.0").unwrap().unwrap().version, "0.10.1");
        assert_eq!(check(&url, "0.10.1").unwrap(), None);
        assert_eq!(check(&url, "1.0.0").unwrap(), None);
        assert!(check(&format!("{base}/nothing"), "0.1.0").is_err());
    }

    #[test]
    fn a_core_is_unzipped_into_the_cores_folder_and_never_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let cores = dir.path().join("cores");
        let base = serve(HashMap::from([
            (
                "/mgba_libretro.dll.zip".to_string(),
                zip_of("mgba_libretro.dll", b"a core"),
            ),
            ("/gpsp_libretro.dll.zip".to_string(), b"not a zip".to_vec()),
        ]));
        fetch_core(&base, Core::Mgba, &cores).unwrap();
        let file = crate::retroarch::core_file(&cores, Core::Mgba);
        assert_eq!(std::fs::read(&file).unwrap(), b"a core");
        // Already there: left alone, and nothing is asked for.
        std::fs::write(&file, b"the player's own").unwrap();
        fetch_core("http://127.0.0.1:9", Core::Mgba, &cores).unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), b"the player's own");
        // Damaged, and missing from the server: said, and nothing written.
        assert!(fetch_core(&base, Core::Gpsp, &cores).is_err());
        assert!(fetch_core(&base, Core::Gambatte, &cores).is_err());
        assert_eq!(std::fs::read_dir(&cores).unwrap().count(), 1);
    }
}

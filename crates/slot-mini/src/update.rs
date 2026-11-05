//! Over-the-air updates from the project's GitHub releases.
//!
//! MENU -> "Check for updates" asks GitHub for the latest full release (the
//! rolling `main-latest` pre-release is never offered). Installing it:
//!
//! 1. Downloads `slot-mini-<version>.zip` and checks it against the SHA-256
//!    GitHub publishes for the asset, then every file against its zip CRC.
//! 2. Unpacks `App/slot-mini/` from it into `App/slot-mini/update/new/`.
//!    `data/`, `cache/` and `update/` are never taken from a zip.
//! 3. Runs the new binary with `--version`: it must start on this device and
//!    report the version the release names.
//! 4. Copies each file it is about to replace to `update/previous/`, then
//!    renames the new files over the old ones, the binary last. A failure
//!    part-way puts the previous files back.
//!
//! slot-mini then restarts itself (`Exit::Restart`). After the restart a
//! boot shim that is installed is refreshed from the new binary
//! ([`after_restart`]). Only files in the zip change: settings, favourites,
//! labels and the cache stay as they are.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::paths::Paths;

/// Where releases come from, as `owner/repo`. `update_repo` in
/// settings.txt overrides it (a fork, or a full URL for tests).
pub const REPO: &str = "joecrockett5/slot-mini";
const API: &str = "https://api.github.com/repos";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const API_TIMEOUT: Duration = Duration::from_secs(20);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(600);
/// A release zip is about 5 MB; anything far bigger is not ours.
const ZIP_LIMIT: u64 = 64 << 20;
/// Everything unpacked from one zip.
const UNPACKED_LIMIT: u64 = 128 << 20;
/// How long the new binary gets to answer `--version`.
const VERSION_TIMEOUT: Duration = Duration::from_secs(20);
/// The prefix of every file slot-mini takes from a release zip.
const ZIP_ROOT: &str = "App/slot-mini/";
/// The binary, swapped in last.
const BIN: &str = "slot-mini";
/// Left by an install; the restarted binary refreshes the boot shim.
const REFRESH_MARK: &str = "refresh-shim";

/// A release that can be installed.
#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub version: String,
    pub url: String,
    pub size: u64,
    pub sha256: Option<[u8; 32]>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Fail {
    Offline,
    Clock,
    NoRelease,
    Download,
    Corrupt(String),
    Broken(String),
    Disk(String),
}

impl std::fmt::Display for Fail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Fail::Offline => write!(f, "No connection. Is Wi-Fi on?"),
            Fail::Clock => write!(f, "The clock is wrong. Set the date in Onion."),
            Fail::NoRelease => write!(f, "No release found."),
            Fail::Download => write!(f, "The download failed."),
            Fail::Corrupt(_) => write!(f, "The download was damaged."),
            Fail::Broken(_) => write!(f, "The new version did not start."),
            Fail::Disk(_) => write!(f, "Could not write to the SD card."),
        }
    }
}

impl Fail {
    /// The detail, for the log.
    pub fn detail(&self) -> String {
        match self {
            Fail::Corrupt(d) | Fail::Broken(d) | Fail::Disk(d) => format!("{self} ({d})"),
            _ => self.to_string(),
        }
    }
}

fn disk(what: &str, p: &Path, e: std::io::Error) -> Fail {
    Fail::Disk(format!("{what} {}: {e}", p.display()))
}

/// The release API URL for `repo` (`owner/repo`, or a full URL).
pub fn latest_url(repo: &str) -> String {
    match repo.starts_with("http://") || repo.starts_with("https://") {
        true => repo.to_string(),
        false => format!("{API}/{}/releases/latest", repo.trim_matches('/')),
    }
}

/// `major.minor.patch`, with or without a leading `v`.
pub fn version(s: &str) -> Option<(u32, u32, u32)> {
    let s = s.trim().trim_start_matches('v');
    let s = s.split(['-', '+']).next()?;
    let mut it = s.split('.').map(|p| p.parse::<u32>().ok());
    let v = (it.next()??, it.next()??, it.next()??);
    it.next().is_none().then_some(v)
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (version(candidate), version(current)) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

fn hex32(s: &str) -> Option<[u8; 32]> {
    let s = s.trim();
    if s.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(s.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some(out)
}

/// The installable release in GitHub's `releases/latest` answer.
pub fn parse_release(json: &str) -> Option<Release> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    if v["draft"].as_bool() == Some(true) || v["prerelease"].as_bool() == Some(true) {
        return None;
    }
    let tag = v["tag_name"].as_str()?;
    version(tag)?;
    let ver = tag.trim().trim_start_matches('v').to_string();
    let assets = v["assets"].as_array()?;
    let wanted = format!("slot-mini-{ver}.zip");
    let asset = assets
        .iter()
        .find(|a| a["name"].as_str() == Some(wanted.as_str()))
        .or_else(|| {
            assets.iter().find(|a| {
                a["name"]
                    .as_str()
                    .is_some_and(|n| n.starts_with("slot-mini-") && n.ends_with(".zip"))
            })
        })?;
    Some(Release {
        version: ver,
        url: asset["browser_download_url"].as_str()?.to_string(),
        size: asset["size"].as_u64().unwrap_or(0),
        sha256: asset["digest"]
            .as_str()
            .and_then(|d| d.strip_prefix("sha256:"))
            .and_then(hex32),
    })
}

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(CONNECT_TIMEOUT))
        .timeout_global(Some(timeout))
        .user_agent(format!("slot-mini/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

fn request_fail(e: &ureq::Error) -> Fail {
    let text = format!("{e:?}");
    match text.contains("Expired") || text.contains("NotValidYet") {
        true => Fail::Clock,
        false => Fail::Offline,
    }
}

/// The latest release, if it is newer than this binary.
pub fn check(repo: &str) -> Result<Option<Release>, Fail> {
    check_against(repo, env!("CARGO_PKG_VERSION"))
}

pub fn check_against(repo: &str, current: &str) -> Result<Option<Release>, Fail> {
    let response = match agent(API_TIMEOUT)
        .get(latest_url(repo))
        .header("Accept", "application/vnd.github+json")
        .call()
    {
        Ok(r) => r,
        Err(ureq::Error::StatusCode(404)) => return Err(Fail::NoRelease),
        Err(ureq::Error::StatusCode(_)) => return Err(Fail::Download),
        Err(e) => return Err(request_fail(&e)),
    };
    let text = response
        .into_body()
        .into_with_config()
        .limit(4 << 20)
        .read_to_string()
        .map_err(|_| Fail::Offline)?;
    let release = parse_release(&text).ok_or(Fail::NoRelease)?;
    Ok(is_newer(&release.version, current).then_some(release))
}

/// Download a release zip, reporting `(bytes so far, total)`, and check it
/// against the published SHA-256.
pub fn download(release: &Release, mut progress: impl FnMut(u64, u64)) -> Result<Vec<u8>, Fail> {
    let response = match agent(DOWNLOAD_TIMEOUT).get(&release.url).call() {
        Ok(r) => r,
        Err(ureq::Error::StatusCode(_)) => return Err(Fail::Download),
        Err(e) => return Err(request_fail(&e)),
    };
    let total = match release.size {
        0 => response
            .headers()
            .get("content-length")
            .and_then(|v| v.to_str().ok()?.parse().ok())
            .unwrap_or(0),
        n => n,
    };
    let mut reader = response
        .into_body()
        .into_with_config()
        .limit(ZIP_LIMIT)
        .reader();
    let mut bytes = Vec::with_capacity(total.min(ZIP_LIMIT) as usize);
    let mut chunk = vec![0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut chunk).map_err(|_| Fail::Download)?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..n]);
        progress(bytes.len() as u64, total);
    }
    if release.size != 0 && bytes.len() as u64 != release.size {
        return Err(Fail::Corrupt(format!(
            "{} bytes, expected {}",
            bytes.len(),
            release.size
        )));
    }
    if let Some(want) = release.sha256 {
        let got = ring::digest::digest(&ring::digest::SHA256, &bytes);
        if got.as_ref() != want {
            return Err(Fail::Corrupt("SHA-256 does not match".into()));
        }
    }
    Ok(bytes)
}

/// One file from a release zip, relative to `App/slot-mini/`.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub name: String,
    pub bytes: Vec<u8>,
    pub exec: bool,
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// Whether a path inside `App/slot-mini/` may come from a release.
fn allowed(rel: &str) -> bool {
    !rel.is_empty()
        && !rel.starts_with('/')
        && !rel.contains('\\')
        && rel
            .split('/')
            .all(|c| !c.is_empty() && c != "." && c != "..")
        && !["data", "cache", "update"].contains(&rel.split('/').next().unwrap_or(""))
}

/// The files under `App/slot-mini/` in a release zip, checked against their
/// CRCs. Stored and deflated entries are supported, as `zip` writes them.
pub fn unzip(zip: &[u8]) -> Result<Vec<Entry>, Fail> {
    let bad = |what: &str| Fail::Corrupt(format!("zip: {what}"));
    // The end of central directory record, searched from the end.
    let floor = zip.len().saturating_sub(22 + 0xffff);
    let eocd = (floor..=zip.len().saturating_sub(22))
        .rev()
        .find(|&i| u32_at(zip, i) == Some(0x0605_4b50))
        .ok_or_else(|| bad("no directory"))?;
    let count = u16_at(zip, eocd + 10).ok_or_else(|| bad("directory"))? as usize;
    let mut at = u32_at(zip, eocd + 16).ok_or_else(|| bad("directory"))? as usize;
    let mut out = Vec::new();
    let mut unpacked = 0u64;
    for _ in 0..count {
        if u32_at(zip, at) != Some(0x0201_4b50) {
            return Err(bad("directory entry"));
        }
        let field = |o: usize| u16_at(zip, at + o).ok_or_else(|| bad("directory entry"));
        let field32 = |o: usize| u32_at(zip, at + o).ok_or_else(|| bad("directory entry"));
        let flags = field(8)?;
        let method = field(10)?;
        let crc = field32(16)?;
        let packed = field32(20)? as usize;
        let size = field32(24)? as u64;
        let name_len = field(28)? as usize;
        let extra_len = field(30)? as usize;
        let comment_len = field(32)? as usize;
        let mode = field32(38)? >> 16;
        let local = field32(42)? as usize;
        let name = zip
            .get(at + 46..at + 46 + name_len)
            .ok_or_else(|| bad("name"))?;
        let name = String::from_utf8_lossy(name).into_owned();
        at += 46 + name_len + extra_len + comment_len;

        let Some(rel) = name.strip_prefix(ZIP_ROOT) else {
            continue;
        };
        if rel.is_empty() || rel.ends_with('/') {
            continue;
        }
        if !allowed(rel) {
            continue;
        }
        if flags & 1 != 0 {
            return Err(bad("encrypted entry"));
        }
        unpacked += size;
        if unpacked > UNPACKED_LIMIT {
            return Err(bad("too large"));
        }
        if u32_at(zip, local) != Some(0x0403_4b50) {
            return Err(bad("local header"));
        }
        let skip = 30
            + u16_at(zip, local + 26).ok_or_else(|| bad("local header"))? as usize
            + u16_at(zip, local + 28).ok_or_else(|| bad("local header"))? as usize;
        let data = zip
            .get(local + skip..local + skip + packed)
            .ok_or_else(|| bad("truncated"))?;
        let bytes = match method {
            0 => data.to_vec(),
            8 => miniz_oxide::inflate::decompress_to_vec_with_limit(data, size as usize)
                .map_err(|_| bad("inflate"))?,
            m => return Err(bad(&format!("method {m}"))),
        };
        if bytes.len() as u64 != size || crc32fast::hash(&bytes) != crc {
            return Err(Fail::Corrupt(format!("{rel}: CRC does not match")));
        }
        let exec = mode & 0o111 != 0 || rel == BIN || rel == "launch.sh";
        out.push(Entry {
            name: rel.to_string(),
            bytes,
            exec,
        });
    }
    if !out.iter().any(|e| e.name == BIN) {
        return Err(bad("no slot-mini binary"));
    }
    Ok(out)
}

fn update_dir(paths: &Paths) -> PathBuf {
    paths.app().join("update")
}

fn set_mode(p: &Path, exec: bool) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mode = if exec { 0o755 } else { 0o644 };
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(mode))
}

fn sync() {
    // SAFETY: sync(2) takes no arguments and cannot fail.
    unsafe { libc::sync() };
}

/// Run `bin --version` with a time limit and return what it printed.
fn version_of(bin: &Path) -> Result<String, Fail> {
    use std::process::{Command, Stdio};
    let mut child = Command::new(bin)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| Fail::Broken(format!("start: {e}")))?;
    let started = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() < VERSION_TIMEOUT => {
                std::thread::sleep(Duration::from_millis(50));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(Fail::Broken("no answer to --version".into()));
            }
        }
    }
    let mut text = String::new();
    if let Some(mut out) = child.stdout.take() {
        let _ = out.read_to_string(&mut text);
    }
    Ok(text.trim().to_string())
}

/// Swap the release's files into `App/slot-mini/`. See the module notes.
pub fn install(paths: &Paths, files: &[Entry], version: &str) -> Result<(), Fail> {
    let app = paths.app();
    let dir = update_dir(paths);
    let new = dir.join("new");
    let previous = dir.join("previous");
    for d in [&new, &previous] {
        if d.exists() {
            std::fs::remove_dir_all(d).map_err(|e| disk("clear", d, e))?;
        }
    }
    // 2. Unpack beside the app.
    for f in files {
        let p = new.join(&f.name);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).map_err(|e| disk("create", parent, e))?;
        }
        std::fs::write(&p, &f.bytes).map_err(|e| disk("write", &p, e))?;
        // Some cards are mounted without permissions; that is fine.
        let _ = set_mode(&p, f.exec);
    }
    sync();
    // 3. The new binary must run here and be the version promised.
    let said = version_of(&new.join(BIN))?;
    if said != format!("slot-mini {version}") {
        return Err(Fail::Broken(format!(
            "it says {said:?}, expected {version}"
        )));
    }
    // 4. Back up, then swap: everything else first, the binary last.
    let mut order: Vec<&Entry> = files.iter().filter(|f| f.name != BIN).collect();
    order.extend(files.iter().filter(|f| f.name == BIN));
    for f in &order {
        let live = app.join(&f.name);
        if live.is_file() {
            let keep = previous.join(&f.name);
            if let Some(parent) = keep.parent() {
                std::fs::create_dir_all(parent).map_err(|e| disk("create", parent, e))?;
            }
            std::fs::copy(&live, &keep).map_err(|e| disk("back up", &live, e))?;
        }
    }
    sync();
    let mut swapped: Vec<&Entry> = Vec::new();
    for f in &order {
        match swap_in(&new.join(&f.name), &app.join(&f.name)) {
            Ok(()) => swapped.push(f),
            Err(e) => {
                roll_back(&app, &previous, &swapped);
                return Err(e);
            }
        }
    }
    sync();
    let _ = std::fs::remove_dir_all(&new);
    let _ = std::fs::write(dir.join(REFRESH_MARK), version);
    sync();
    Ok(())
}

fn swap_in(from: &Path, to: &Path) -> Result<(), Fail> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent).map_err(|e| disk("create", parent, e))?;
    }
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    // Some file systems refuse to rename over a file in use; the running
    // binary keeps its own copy once its name is gone.
    let _ = std::fs::remove_file(to);
    std::fs::rename(from, to).map_err(|e| disk("replace", to, e))
}

fn roll_back(app: &Path, previous: &Path, swapped: &[&Entry]) {
    for f in swapped {
        let live = app.join(&f.name);
        let keep = previous.join(&f.name);
        let result = match keep.is_file() {
            true => {
                let tmp = live.with_extension("rollback");
                std::fs::copy(&keep, &tmp)
                    .and_then(|_| set_mode(&tmp, f.exec))
                    .and_then(|_| std::fs::rename(&tmp, &live))
            }
            false => std::fs::remove_file(&live),
        };
        if let Err(e) = result {
            eprintln!(
                "slot-mini: update: could not restore {}: {e}",
                live.display()
            );
        }
    }
    sync();
}

/// Called once at startup: after an update, refresh an installed boot shim
/// from the new binary. Returns the version that was installed.
pub fn after_restart(paths: &Paths) -> Option<String> {
    let mark = update_dir(paths).join(REFRESH_MARK);
    let version = std::fs::read_to_string(&mark).ok()?;
    let _ = std::fs::remove_file(&mark);
    if crate::switch::installed(paths) {
        if let Err(e) = crate::switch::install(paths) {
            eprintln!("slot-mini: update: boot shim not refreshed: {e}");
        }
    }
    Some(version.trim().to_string())
}

/// Where an update is, for the About screen.
#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Idle,
    Checking,
    UpToDate,
    Available(Release),
    /// Percent in steps of ten, so few distinct labels are drawn.
    Downloading {
        version: String,
        percent: u8,
    },
    Installing(String),
    Installed(String),
    Failed(Fail),
}

/// Runs checks and installs on a thread; the UI reads [`Updater::status`].
pub struct Updater {
    status: Arc<Mutex<Status>>,
}

impl Default for Updater {
    fn default() -> Self {
        Updater {
            status: Arc::new(Mutex::new(Status::Idle)),
        }
    }
}

impl Updater {
    pub fn status(&self) -> Status {
        self.status
            .lock()
            .map(|s| s.clone())
            .unwrap_or(Status::Idle)
    }

    fn set(status: &Mutex<Status>, s: Status) {
        if let Ok(mut g) = status.lock() {
            *g = s;
        }
    }

    pub fn busy(&self) -> bool {
        matches!(
            self.status(),
            Status::Checking
                | Status::Downloading { .. }
                | Status::Installing(_)
                | Status::Installed(_)
        )
    }

    /// A press on the update row: check, or install what the check found.
    pub fn press(&mut self, paths: &Paths, repo: &str) {
        let status = self.status.clone();
        match self.status() {
            Status::Idle | Status::UpToDate | Status::Failed(_) => {
                Updater::set(&status, Status::Checking);
                let repo = repo.to_string();
                std::thread::spawn(move || {
                    let s = match check(&repo) {
                        Ok(Some(r)) => Status::Available(r),
                        Ok(None) => Status::UpToDate,
                        Err(e) => {
                            eprintln!("slot-mini: update check: {}", e.detail());
                            Status::Failed(e)
                        }
                    };
                    Updater::set(&status, s);
                });
            }
            Status::Available(release) => {
                let paths = paths.clone();
                Updater::set(
                    &status,
                    Status::Downloading {
                        version: release.version.clone(),
                        percent: 0,
                    },
                );
                std::thread::spawn(move || {
                    let s = match fetch_and_install(&paths, &release, &status) {
                        Ok(()) => {
                            eprintln!("slot-mini: updated to {}", release.version);
                            Status::Installed(release.version.clone())
                        }
                        Err(e) => {
                            eprintln!("slot-mini: update: {}", e.detail());
                            Status::Failed(e)
                        }
                    };
                    Updater::set(&status, s);
                });
            }
            _ => {}
        }
    }
}

fn fetch_and_install(paths: &Paths, release: &Release, status: &Mutex<Status>) -> Result<(), Fail> {
    let zip = download(release, |got, total| {
        let percent = match total {
            0 => 0,
            t => ((got * 10 / t).min(10) * 10) as u8,
        };
        Updater::set(
            status,
            Status::Downloading {
                version: release.version.clone(),
                percent,
            },
        );
    })?;
    Updater::set(status, Status::Installing(release.version.clone()));
    let files = unzip(&zip)?;
    drop(zip);
    install(paths, &files, &release.version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_numerically() {
        assert_eq!(version("v0.2.10"), Some((0, 2, 10)));
        assert_eq!(version("1.2"), None);
        assert_eq!(version("1.2.3.4"), None);
        assert_eq!(version("1.2.3-rc1"), Some((1, 2, 3)));
        assert!(is_newer("v0.2.10", "0.2.9"));
        assert!(!is_newer("0.2.2", "0.2.2"));
        assert!(!is_newer("main-latest", "0.2.2"));
    }

    #[test]
    fn a_github_release_is_read() {
        let json = r#"{"tag_name":"v0.3.0","draft":false,"prerelease":false,
            "assets":[{"name":"other.txt","browser_download_url":"x"},
            {"name":"slot-mini-0.3.0.zip","size":123,
             "digest":"sha256:00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
             "browser_download_url":"https://example/slot-mini-0.3.0.zip"}]}"#;
        let r = parse_release(json).unwrap();
        assert_eq!(r.version, "0.3.0");
        assert_eq!(r.url, "https://example/slot-mini-0.3.0.zip");
        assert_eq!(r.size, 123);
        assert_eq!(r.sha256.unwrap()[1], 0x11);
        let pre = json.replace(r#""prerelease":false"#, r#""prerelease":true"#);
        assert_eq!(parse_release(&pre), None);
    }

    #[test]
    fn only_app_files_may_come_from_a_zip() {
        assert!(allowed("slot-mini"));
        assert!(allowed("licenses/slot-GPL-3.0.txt"));
        for bad in [
            "",
            "../x",
            "a/../b",
            "/etc/x",
            "data/settings.txt",
            "cache/x",
            "update/x",
            "a//b",
        ] {
            assert!(!allowed(bad), "{bad}");
        }
    }

    #[test]
    fn repo_or_url() {
        assert_eq!(
            latest_url("joecrockett5/slot-mini"),
            "https://api.github.com/repos/joecrockett5/slot-mini/releases/latest"
        );
        assert_eq!(latest_url("http://127.0.0.1:9/x"), "http://127.0.0.1:9/x");
    }
}

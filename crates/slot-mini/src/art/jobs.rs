// Adapted from slot (https://github.com/BrandonKowalski/slot) at e66715d:
//   crates/slot/src/art/jobs.rs (load_index, other, path_of, make_one, lost,
//     got, image, label_for, fetch_art, get_index)
//   crates/slot/src/art/rom.rs (rom_crc)
//   crates/slot/src/update.rs (UpdateFail, agent, request_fail, timeouts)
// Changes: the art host is a parameter (tests point it at a local server and
//   a settings key can move it); the index cache lives in
//   App/slot-mini/cache/art; slot's background job queue, bulk fetching and
//   studio save are replaced by `studio_prints`, which fetches the two labels
//   the on-device studio offers. Requests run on the caller's thread; the app
//   calls them from a worker thread.
// Copyright (C) Brandon Kowalski and slot contributors. GPL-3.0-or-later.

use std::io::Read;
use std::path::Path;
use std::time::Duration;

use slot_mini_store::Cart;

use super::index::{ArtIndex, Entry, Look};
use super::make::{self, Kind};

const CACHE: &str = "App/slot-mini/cache/art";

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const API_TIMEOUT: Duration = Duration::from_secs(15);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(60);
const DOWNLOAD_LIMIT: u64 = 16 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateFail {
    NoWifi,
    Offline,
    Clock,
    Download,
    Bad,
}

pub enum IndexFetch {
    Fresh { body: Vec<u8>, etag: Option<String> },
    NotModified,
}

/// The index, from the host when it changed (ETag) or from the cache.
pub fn load_index(
    root: &Path,
    get: &mut dyn FnMut(Option<&str>) -> Result<IndexFetch, UpdateFail>,
) -> Result<ArtIndex, UpdateFail> {
    let dir = root.join(CACHE);
    let (json, tag) = (dir.join("index.json"), dir.join("index.etag"));
    let cached = std::fs::read_to_string(&json)
        .ok()
        .and_then(|text| ArtIndex::parse(&text));
    let etag = cached
        .as_ref()
        .and_then(|_| std::fs::read_to_string(&tag).ok());
    let from_cache = |fail: UpdateFail| cached.ok_or(fail);
    match get(etag.as_deref()) {
        Ok(IndexFetch::Fresh { body, etag }) => {
            let Some(index) = std::str::from_utf8(&body).ok().and_then(ArtIndex::parse) else {
                return from_cache(UpdateFail::Bad);
            };
            let _ = std::fs::remove_file(&tag);
            let saved = make::write_replacing(&json, &body).is_ok();
            if let (true, Some(t)) = (saved, etag) {
                let _ = make::write_replacing(&tag, t.as_bytes());
            }
            Ok(index)
        }
        Ok(IndexFetch::NotModified) => from_cache(UpdateFail::Bad),
        Err(e) => from_cache(e),
    }
}

fn other(kind: Kind) -> Kind {
    match kind {
        Kind::Real => Kind::Logo,
        Kind::Logo => Kind::Real,
    }
}

fn path_of(entry: &Entry, kind: Kind) -> Option<&str> {
    match kind {
        Kind::Real => entry.real.as_deref(),
        Kind::Logo => entry.logo.as_deref(),
    }
}

fn make_one(kind: Kind, bytes: &[u8], look: Option<&Look>, cart: &Cart) -> Option<Vec<u8>> {
    match kind {
        Kind::Real => make::real_label(bytes, cart.platform),
        Kind::Logo => make::logo_label(bytes, look, &cart.stem, cart.platform),
    }
}

fn lost(e: &UpdateFail) -> bool {
    matches!(
        e,
        UpdateFail::Offline | UpdateFail::Clock | UpdateFail::NoWifi
    )
}

type Fetch<'a> = dyn FnMut(&str) -> Result<Vec<u8>, UpdateFail> + 'a;

fn got(fetch: &mut Fetch, url: &str) -> Result<Option<Vec<u8>>, UpdateFail> {
    match fetch(url) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if lost(&e) => Err(e),
        Err(_) => Ok(None),
    }
}

/// slot's index names PNGs; the host serves WebP beside each one.
fn image(base: &str, fetch: &mut Fetch, path: &str) -> Result<Option<Vec<u8>>, UpdateFail> {
    if let Some(stem) = path.strip_suffix(".png") {
        if let Some(bytes) = got(fetch, &format!("{base}{stem}.webp"))? {
            return Ok(Some(bytes));
        }
    }
    got(fetch, &format!("{base}{path}"))
}

pub fn rom_crc(path: &Path) -> std::io::Result<u32> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = crc32fast::Hasher::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            return Ok(hasher.finalize());
        }
        hasher.update(&buf[..n]);
    }
}

fn entry_for<'a>(cart: &Cart, index: &'a ArtIndex) -> Option<&'a Entry> {
    index.get(rom_crc(&cart.rom).ok()?)
}

/// One label of `kind` (or, failing that, the other kind), as a PNG.
pub fn label_for(
    base: &str,
    cart: &Cart,
    kind: Kind,
    index: &ArtIndex,
    fetch: &mut Fetch,
) -> Result<Option<Vec<u8>>, UpdateFail> {
    let Some(entry) = entry_for(cart, index) else {
        return Ok(None);
    };
    for k in [kind, other(kind)] {
        let Some(path) = path_of(entry, k) else {
            continue;
        };
        let Some(bytes) = image(base, fetch, path)? else {
            continue;
        };
        if let Some(png) = make_one(k, &bytes, entry.look.as_ref(), cart) {
            return Ok(Some(png));
        }
    }
    Ok(None)
}

/// What the on-device label printer offers for a cart: the scanned label
/// and the logo label, each if the service has it.
#[derive(Default, Debug)]
pub struct Prints {
    pub real: Option<Vec<u8>>,
    pub logo: Option<Vec<u8>>,
}

pub fn studio_prints(root: &Path, base: &str, cart: &Cart) -> Result<Prints, UpdateFail> {
    let agent = agent(DOWNLOAD_TIMEOUT);
    let index = load_index(root, &mut |etag| get_index(base, etag))?;
    let Some(entry) = entry_for(cart, &index) else {
        return Ok(Prints::default());
    };
    let mut fetch = |url: &str| fetch_art(&agent, url);
    let mut one = |kind: Kind| -> Result<Option<Vec<u8>>, UpdateFail> {
        let Some(path) = path_of(entry, kind) else {
            return Ok(None);
        };
        let Some(bytes) = image(base, &mut fetch, path)? else {
            return Ok(None);
        };
        Ok(make_one(kind, &bytes, entry.look.as_ref(), cart))
    };
    Ok(Prints {
        real: one(Kind::Real)?,
        logo: one(Kind::Logo)?,
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

fn request_fail(e: &ureq::Error) -> UpdateFail {
    let text = format!("{e:?}");
    match text.contains("Expired") || text.contains("NotValidYet") {
        true => UpdateFail::Clock,
        false => UpdateFail::Offline,
    }
}

fn fetch_art(agent: &ureq::Agent, url: &str) -> Result<Vec<u8>, UpdateFail> {
    let response = match agent.get(url).call() {
        Ok(r) => r,
        Err(ureq::Error::StatusCode(_)) => return Err(UpdateFail::Download),
        Err(e) => return Err(request_fail(&e)),
    };
    let mut body = Vec::new();
    response
        .into_body()
        .into_with_config()
        .limit(DOWNLOAD_LIMIT)
        .reader()
        .read_to_end(&mut body)
        .map_err(|_| UpdateFail::Offline)?;
    Ok(body)
}

fn get_index(base: &str, etag: Option<&str>) -> Result<IndexFetch, UpdateFail> {
    let mut request = agent(API_TIMEOUT).get(format!("{base}index.json"));
    if let Some(t) = etag {
        request = request.header("If-None-Match", t);
    }
    let response = match request.call() {
        Ok(r) => r,
        Err(ureq::Error::StatusCode(304)) => return Ok(IndexFetch::NotModified),
        Err(e) => return Err(request_fail(&e)),
    };
    if response.status() == 304 {
        return Ok(IndexFetch::NotModified);
    }
    let etag = response
        .headers()
        .get("etag")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let mut body = Vec::new();
    response
        .into_body()
        .into_with_config()
        .limit(DOWNLOAD_LIMIT)
        .reader()
        .read_to_end(&mut body)
        .map_err(|_| UpdateFail::Download)?;
    Ok(IndexFetch::Fresh { body, etag })
}

/// The host with exactly one trailing slash.
pub fn base_url(host: &str) -> String {
    format!("{}/", host.trim_end_matches('/'))
}

/// Fetch labels for carts that have no local label, one at a time, writing
/// each new file with `make::write_new` (which never replaces a file).
/// Every label that lands is sent back. Stops at the first sign the network
/// is gone. Local label files always win: carts with one are skipped.
pub fn fetch_missing(
    root: &Path,
    base: &str,
    carts: Vec<Cart>,
    landed: &std::sync::mpsc::Sender<(Cart, std::path::PathBuf)>,
) -> Result<usize, UpdateFail> {
    let carts: Vec<Cart> = carts.into_iter().filter(|c| c.label.is_none()).collect();
    if carts.is_empty() {
        return Ok(0);
    }
    let index = load_index(root, &mut |etag| get_index(base, etag))?;
    let agent = agent(DOWNLOAD_TIMEOUT);
    let mut fetch = |url: &str| fetch_art(&agent, url);
    let mut n = 0;
    for cart in carts {
        let path = make::label_path(root, &cart);
        if path.exists() {
            continue;
        }
        let Some(png) = label_for(base, &cart, Kind::Real, &index, &mut fetch)? else {
            continue;
        };
        if let Ok(true) = make::write_new(&path, &png) {
            n += 1;
            if landed.send((cart, path)).is_err() {
                break;
            }
        }
    }
    Ok(n)
}

//! The hub over HTTP (docs/SYNC.md, "Protocol"). Blocking: one thread
//! accepts and each request gets a thread of its own, so a long transfer
//! never holds up a pairing screen.

use std::io::Read;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde_json::Value;
use tiny_http::{Header, Method, Request, Response, StatusCode};

use crate::auth::{sign_reply, SecretKey};
use crate::discovery::Responder;
use crate::error::SyncError;
use crate::hash::hash_bytes;
use crate::hub::Hub;
use crate::{now_secs, PROTOCOL_VERSION};

/// The largest JSON body the hub reads (a manifest of a very big library
/// is a few megabytes).
const MAX_JSON: u64 = 32 * 1024 * 1024;

/// Serves a [`Hub`] until dropped.
pub struct Server {
    hub: Arc<Hub>,
    http: Arc<tiny_http::Server>,
    addr: SocketAddr,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
    responder: Option<Responder>,
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes())
        .unwrap_or_else(|()| Header::from_bytes(&b"X-Slot"[..], &b""[..]).expect("a plain header"))
}

fn get<'a>(request: &'a Request, name: &'static str) -> Option<&'a str> {
    request
        .headers()
        .iter()
        .find(|h| h.field.equiv(name))
        .map(|h| h.value.as_str())
}

/// The value of `name` in a query string.
fn query<'a>(query: &'a str, name: &str) -> Option<&'a str> {
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v)
}

struct Signed {
    device: String,
    nonce: String,
    key: SecretKey,
}

impl Server {
    /// Listen on `bind` (port 0 picks one). With `discovery`, also answer
    /// `find` datagrams there.
    pub fn start(
        hub: Arc<Hub>,
        bind: SocketAddr,
        discovery: Option<SocketAddr>,
    ) -> std::io::Result<Server> {
        let http = tiny_http::Server::http(bind).map_err(std::io::Error::other)?;
        let addr = http
            .server_addr()
            .to_ip()
            .ok_or_else(|| std::io::Error::other("the server has no address"))?;
        let responder = match discovery {
            Some(at) => Some(Responder::start(at, hub.id(), hub.name(), addr.port())?),
            None => None,
        };
        let http = Arc::new(http);
        let stop = Arc::new(AtomicBool::new(false));
        let (listener, stopped, served) = (http.clone(), stop.clone(), hub.clone());
        let thread = std::thread::Builder::new()
            .name("sync-server".into())
            .spawn(move || {
                while !stopped.load(Ordering::SeqCst) {
                    // `unblock` (on drop) ends this wait at once.
                    let Ok(request) = listener.recv() else {
                        break;
                    };
                    let hub = served.clone();
                    let _ = std::thread::Builder::new()
                        .name("sync-request".into())
                        .spawn(move || handle(&hub, request));
                }
            })?;
        Ok(Server {
            hub,
            http,
            addr,
            stop,
            thread: Some(thread),
            responder,
        })
    }

    pub fn hub(&self) -> &Arc<Hub> {
        &self.hub
    }

    /// Where the server listens.
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Where discovery answers, if it does.
    pub fn discovery_addr(&self) -> Option<SocketAddr> {
        self.responder.as_ref().and_then(|r| r.local_addr().ok())
    }

    /// "Sync now": poke every paired device on `poke_port`.
    pub fn poke(&self, poke_port: u16) -> usize {
        self.hub.poke(self.addr.port(), poke_port)
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.http.unblock();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn json_body(request: &mut Request) -> Result<Vec<u8>, SyncError> {
    let mut body = Vec::new();
    request
        .as_reader()
        .take(MAX_JSON + 1)
        .read_to_end(&mut body)?;
    if body.len() as u64 > MAX_JSON {
        return Err(SyncError::Bad("the request is too large".into()));
    }
    Ok(body)
}

fn parse(body: &[u8]) -> Result<Value, SyncError> {
    serde_json::from_slice(body).map_err(|_| SyncError::Bad("the request is not JSON".into()))
}

/// Check the signature headers of a request.
fn authenticate(hub: &Hub, request: &Request) -> Result<(Signed, String), SyncError> {
    let missing = || SyncError::Auth("bad-auth".into());
    let device = get(request, "X-Slot-Device").ok_or(SyncError::Unpaired)?;
    let time: u64 = get(request, "X-Slot-Time")
        .and_then(|t| t.parse().ok())
        .ok_or_else(missing)?;
    let nonce = get(request, "X-Slot-Nonce").ok_or_else(missing)?;
    let body_hash = get(request, "X-Slot-Body").ok_or_else(missing)?;
    let auth = get(request, "X-Slot-Auth").ok_or_else(missing)?;
    let address = request.remote_addr().map(|a| a.ip().to_string());
    let key = hub.authenticate(
        device,
        request.method().as_str(),
        request.url(),
        time,
        nonce,
        body_hash,
        auth,
        address.as_deref(),
    )?;
    Ok((
        Signed {
            device: device.to_string(),
            nonce: nonce.to_string(),
            key,
        },
        body_hash.to_string(),
    ))
}

enum Reply {
    Json(Value),
    File(std::fs::File, u64, String),
}

fn route(
    hub: &Hub,
    request: &mut Request,
    signed: &mut Option<Signed>,
) -> Result<Reply, SyncError> {
    let url = request.url().to_string();
    let (path, q) = url.split_once('?').unwrap_or((&url, ""));
    let method = request.method().clone();
    if method == Method::Get && path == "/info" {
        return Ok(Reply::Json(hub.info()));
    }
    let version = get(request, "X-Slot-Protocol").and_then(|v| v.parse::<u32>().ok());
    if version != Some(PROTOCOL_VERSION) {
        return Err(SyncError::Protocol {
            hub: PROTOCOL_VERSION,
            client: version.unwrap_or(0),
        });
    }
    if method == Method::Post && (path == "/pair/start" || path == "/pair/reveal") {
        let body = json_body(request)?;
        return hub.open_request(path, &parse(&body)?).map(Reply::Json);
    }
    let (who, body_hash) = authenticate(hub, request)?;
    let device = who.device.clone();
    *signed = Some(who);
    if path == "/sync/file" {
        let session = query(q, "s").unwrap_or("");
        let action: usize = query(q, "a")
            .and_then(|a| a.parse().ok())
            .ok_or_else(|| SyncError::Bad("the request names no action".into()))?;
        return match method {
            Method::Put => {
                hub.upload(&device, session, action, request.as_reader())?;
                Ok(Reply::Json(serde_json::json!({})))
            }
            Method::Get => hub
                .download(&device, session, action)
                .map(|(file, size, hash)| Reply::File(file, size, hash)),
            _ => Err(SyncError::NoHub),
        };
    }
    if method != Method::Post {
        return Err(SyncError::NoHub);
    }
    let body = json_body(request)?;
    if hash_bytes(&body) != body_hash {
        return Err(SyncError::Auth("bad-auth".into()));
    }
    hub.request(&device, path, &parse(&body)?).map(Reply::Json)
}

fn handle(hub: &Hub, mut request: Request) {
    let mut signed = None;
    let reply = route(hub, &mut request, &mut signed);
    let sign = |status: u16, body_hash: &str| -> Vec<Header> {
        let mut headers = vec![header("X-Slot-Protocol", &PROTOCOL_VERSION.to_string())];
        if let Some(s) = &signed {
            headers.push(header("X-Slot-Body", body_hash));
            headers.push(header(
                "X-Slot-Auth",
                &sign_reply(&s.key, &s.nonce, status, body_hash),
            ));
        }
        headers
    };
    let (status, body) = match reply {
        Ok(Reply::File(file, size, hash)) => {
            let response = Response::new(
                StatusCode(200),
                sign(200, &hash),
                file.take(size),
                Some(size as usize),
                None,
            );
            let _ = request.respond(response);
            return;
        }
        Ok(Reply::Json(v)) => (200, v),
        Err(e) => {
            let mut v = e.to_json();
            if e == SyncError::Auth("stale".into()) {
                v["error"]["now"] = serde_json::json!(now_secs());
            }
            (e.code().1, v)
        }
    };
    let body = body.to_string().into_bytes();
    let mut headers = sign(status, &hash_bytes(&body));
    headers.push(header("Content-Type", "application/json"));
    let len = body.len();
    let response = Response::new(
        StatusCode(status),
        headers,
        std::io::Cursor::new(body),
        Some(len),
        None,
    );
    let _ = request.respond(response);
}

//! The client's connection to a hub over HTTP (docs/SYNC.md, "Protocol"
//! and "Authentication"). Blocking; call it from a worker thread.

use std::io::Read;
use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::auth::{self, sign_request, verify_reply, SecretKey};
use crate::error::SyncError;
use crate::hash::{hash_bytes, Hasher, PIECE};
use crate::link::{Counted, Link};
use crate::PROTOCOL_VERSION;

const CONNECT: Duration = Duration::from_secs(5);
/// The hub may hash its whole library before it answers `/sync/plan`.
const ANSWER: Duration = Duration::from_secs(180);
/// A transfer that stalls this long has lost its network.
const STALL: Duration = Duration::from_secs(20);
const MAX_JSON: u64 = 32 * 1024 * 1024;

/// A hub at `host:port`.
pub struct HttpLink {
    agent: ureq::Agent,
    address: String,
    device: String,
    key: Option<SecretKey>,
    /// The hub's clock when `/info` was read, and when that was here.
    hub_time: Option<(u64, Instant)>,
}

fn network(e: ureq::Error) -> SyncError {
    SyncError::Network(e.to_string())
}

struct Signature {
    nonce: String,
    headers: Vec<(&'static str, String)>,
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(CONNECT))
        .timeout_recv_response(Some(ANSWER))
        .timeout_per_read(Some(STALL))
        .timeout_per_write(Some(STALL))
        .http_status_as_error(false)
        .max_redirects(0)
        .proxy(None)
        .build()
        .into()
}

impl HttpLink {
    pub fn new(address: &str) -> HttpLink {
        HttpLink {
            agent: agent(),
            address: address.to_string(),
            device: String::new(),
            key: None,
            hub_time: None,
        }
    }

    fn url(&self, target: &str) -> String {
        format!("http://{}{target}", self.address)
    }

    /// The hub's clock now: the device's own cannot be trusted.
    fn hub_now(&mut self) -> Result<u64, SyncError> {
        if self.hub_time.is_none() {
            self.info()?;
        }
        let (then, at) = self.hub_time.ok_or(SyncError::NoHub)?;
        Ok(then + at.elapsed().as_secs())
    }

    fn sign(
        &mut self,
        method: &str,
        target: &str,
        body_hash: &str,
    ) -> Result<Signature, SyncError> {
        let key = self.key.ok_or(SyncError::NotPaired)?;
        let time = self.hub_now()?;
        let nonce = auth::random_hex(16)?;
        let auth = sign_request(&key, method, target, time, &nonce, body_hash);
        Ok(Signature {
            headers: vec![
                ("X-Slot-Device", self.device.clone()),
                ("X-Slot-Time", time.to_string()),
                ("X-Slot-Nonce", nonce.clone()),
                ("X-Slot-Body", body_hash.to_string()),
                ("X-Slot-Auth", auth),
            ],
            nonce,
        })
    }

    /// Read a JSON reply, checking its signature when the request was
    /// signed.
    fn reply(
        &mut self,
        response: ureq::http::Response<ureq::Body>,
        signed: Option<&Signature>,
    ) -> Result<Value, SyncError> {
        let status = response.status().as_u16();
        let auth = response
            .headers()
            .get("X-Slot-Auth")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let body = response
            .into_body()
            .into_with_config()
            .limit(MAX_JSON)
            .read_to_vec()
            .map_err(network)?;
        let verified = match (signed, self.key, &auth) {
            (Some(s), Some(key), Some(auth)) => {
                verify_reply(&key, &s.nonce, status, &hash_bytes(&body), auth)
            }
            _ => false,
        };
        let value: Value = serde_json::from_slice(&body)
            .map_err(|_| SyncError::Bad("the PC's reply is not JSON".into()))?;
        if (200..300).contains(&status) {
            if signed.is_some() && !verified {
                return Err(SyncError::Auth("bad-auth".into()));
            }
            return Ok(value);
        }
        if auth.is_some() && signed.is_some() && !verified {
            return Err(SyncError::Auth("bad-auth".into()));
        }
        let error = SyncError::from_json(&value);
        if error == SyncError::Auth("stale".into()) {
            // Our idea of the hub's clock is off: take the one it sent.
            if let Some(now) = value["error"]["now"].as_u64() {
                self.hub_time = Some((now, Instant::now()));
            }
        }
        Err(error)
    }

    fn post(&mut self, path: &str, body: &Value, signed: bool) -> Result<Value, SyncError> {
        let body = body.to_string().into_bytes();
        let signature = match signed {
            true => Some(self.sign("POST", path, &hash_bytes(&body))?),
            false => None,
        };
        let mut request = self
            .agent
            .post(self.url(path))
            .header("X-Slot-Protocol", PROTOCOL_VERSION.to_string())
            .header("Content-Type", "application/json");
        for (name, value) in signature.iter().flat_map(|s| &s.headers) {
            request = request.header(*name, value);
        }
        let response = request.send(&body[..]).map_err(network)?;
        self.reply(response, signature.as_ref())
    }
}

impl Link for HttpLink {
    fn info(&mut self) -> Result<Value, SyncError> {
        let response = self
            .agent
            .get(self.url("/info"))
            .header("X-Slot-Protocol", PROTOCOL_VERSION.to_string())
            .call()
            .map_err(network)?;
        let info = self.reply(response, None)?;
        if let Some(now) = info["now"].as_u64() {
            self.hub_time = Some((now, Instant::now()));
        }
        Ok(info)
    }

    fn call(&mut self, path: &str, body: &Value) -> Result<Value, SyncError> {
        match self.post(path, body, true) {
            // The clock was corrected from the reply: once more.
            Err(SyncError::Auth(kind)) if kind == "stale" => self.post(path, body, true),
            other => other,
        }
    }

    fn call_open(&mut self, path: &str, body: &Value) -> Result<Value, SyncError> {
        self.post(path, body, false)
    }

    fn upload(
        &mut self,
        session: &str,
        action: usize,
        path: &Path,
        size: u64,
        hash: &str,
        progress: &mut dyn FnMut(u64) -> Result<(), SyncError>,
    ) -> Result<(), SyncError> {
        let target = format!("/sync/file?s={session}&a={action}");
        let signature = self.sign("PUT", &target, hash)?;
        let mut request = self
            .agent
            .put(self.url(&target))
            .header("X-Slot-Protocol", PROTOCOL_VERSION.to_string())
            .header("Content-Type", "application/octet-stream")
            .header("Content-Length", size.to_string());
        for (name, value) in &signature.headers {
            request = request.header(*name, value);
        }
        let mut reader = Counted {
            inner: std::fs::File::open(path)?.take(size),
            done: 0,
            progress,
            stopped: None,
        };
        let sent = request.send(ureq::SendBody::from_reader(&mut reader));
        let stopped = reader.stopped.take();
        if stopped.is_some() || reader.done != size {
            // Half a body went out: close the connection, so that the hub
            // stops waiting for the rest and drops what it has.
            self.agent = agent();
        }
        if let Some(e) = stopped {
            return Err(e);
        }
        if reader.done != size {
            // The file shrank since the scan.
            return Err(SyncError::HashMismatch);
        }
        self.reply(sent.map_err(network)?, Some(&signature))
            .map(|_| ())
    }

    fn download(
        &mut self,
        session: &str,
        action: usize,
        sink: &mut dyn FnMut(&[u8]) -> Result<(), SyncError>,
    ) -> Result<(), SyncError> {
        let target = format!("/sync/file?s={session}&a={action}");
        let signature = self.sign("GET", &target, &hash_bytes(b""))?;
        let mut request = self
            .agent
            .get(self.url(&target))
            .header("X-Slot-Protocol", PROTOCOL_VERSION.to_string());
        for (name, value) in &signature.headers {
            request = request.header(*name, value);
        }
        let response = request.call().map_err(network)?;
        if !response.status().is_success() {
            return self.reply(response, Some(&signature)).map(|_| ());
        }
        let head = |name: &str| -> Option<String> {
            Some(response.headers().get(name)?.to_str().ok()?.to_string())
        };
        let (claimed, auth) = (head("X-Slot-Body"), head("X-Slot-Auth"));
        let key = self.key.ok_or(SyncError::NotPaired)?;
        let (Some(claimed), Some(auth)) = (claimed, auth) else {
            return Err(SyncError::Auth("bad-auth".into()));
        };
        if !verify_reply(&key, &signature.nonce, 200, &claimed, &auth) {
            return Err(SyncError::Auth("bad-auth".into()));
        }
        let mut reader = response.into_body().into_reader();
        let mut hasher = Hasher::new();
        let mut buf = vec![0u8; PIECE];
        loop {
            let n = reader
                .read(&mut buf)
                .map_err(|e| SyncError::Network(e.to_string()))?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
            sink(&buf[..n])?;
        }
        if hasher.finish() != claimed {
            return Err(SyncError::HashMismatch);
        }
        Ok(())
    }

    fn set_auth(&mut self, device: &str, key: SecretKey) {
        self.device = device.to_string();
        self.key = Some(key);
    }

    fn address(&self) -> String {
        self.address.clone()
    }
}

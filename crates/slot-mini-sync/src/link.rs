//! How the client reaches the hub. `HttpLink` (feature `client`) is the
//! real one; `DirectLink` calls a `Hub` in the same process.

use std::io::Read;
use std::path::Path;

use serde_json::Value;

use crate::auth::SecretKey;
use crate::error::SyncError;
use crate::hash::PIECE;
use crate::hub::Hub;

/// A connection to a hub, as the client engine uses it.
pub trait Link {
    /// `GET /info`.
    fn info(&mut self) -> Result<Value, SyncError>;

    /// A signed JSON request.
    fn call(&mut self, path: &str, body: &Value) -> Result<Value, SyncError>;

    /// An unsigned JSON request (the first two steps of pairing).
    fn call_open(&mut self, path: &str, body: &Value) -> Result<Value, SyncError>;

    /// Send the file at `path` for an action. `progress` gets the bytes
    /// sent so far and can stop the transfer.
    fn upload(
        &mut self,
        session: &str,
        action: usize,
        path: &Path,
        size: u64,
        hash: &str,
        progress: &mut dyn FnMut(u64) -> Result<(), SyncError>,
    ) -> Result<(), SyncError>;

    /// Fetch an action's file, a piece at a time.
    fn download(
        &mut self,
        session: &str,
        action: usize,
        sink: &mut dyn FnMut(&[u8]) -> Result<(), SyncError>,
    ) -> Result<(), SyncError>;

    /// Who this device is and the key it signs with.
    fn set_auth(&mut self, device: &str, key: SecretKey);

    /// The hub's `host:port`, to remember.
    fn address(&self) -> String;
}

/// A link to a hub in this process: no network and no signatures. The
/// harness and the property tests use it.
pub struct DirectLink<'a> {
    hub: &'a Hub,
    device: String,
}

impl<'a> DirectLink<'a> {
    pub fn new(hub: &'a Hub) -> DirectLink<'a> {
        DirectLink {
            hub,
            device: String::new(),
        }
    }
}

/// A reader that reports how far it has got.
pub(crate) struct Counted<'a, R> {
    pub inner: R,
    pub done: u64,
    pub progress: &'a mut dyn FnMut(u64) -> Result<(), SyncError>,
    /// The error `progress` stopped the transfer with.
    pub stopped: Option<SyncError>,
}

impl<R: Read> Read for Counted<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let room = buf.len().min(PIECE);
        let n = self.inner.read(&mut buf[..room])?;
        self.done += n as u64;
        if let Err(e) = (self.progress)(self.done) {
            self.stopped = Some(e);
            return Err(std::io::Error::other("stopped"));
        }
        Ok(n)
    }
}

impl Link for DirectLink<'_> {
    fn info(&mut self) -> Result<Value, SyncError> {
        Ok(self.hub.info())
    }

    fn call(&mut self, path: &str, body: &Value) -> Result<Value, SyncError> {
        self.hub.request(&self.device, path, body)
    }

    fn call_open(&mut self, path: &str, body: &Value) -> Result<Value, SyncError> {
        self.hub.open_request(path, body)
    }

    fn upload(
        &mut self,
        session: &str,
        action: usize,
        path: &Path,
        _size: u64,
        _hash: &str,
        progress: &mut dyn FnMut(u64) -> Result<(), SyncError>,
    ) -> Result<(), SyncError> {
        let mut reader = Counted {
            inner: std::fs::File::open(path)?,
            done: 0,
            progress,
            stopped: None,
        };
        let result = self.hub.upload(&self.device, session, action, &mut reader);
        match reader.stopped {
            Some(e) => Err(e),
            None => result,
        }
    }

    fn download(
        &mut self,
        session: &str,
        action: usize,
        sink: &mut dyn FnMut(&[u8]) -> Result<(), SyncError>,
    ) -> Result<(), SyncError> {
        let (mut file, _, _) = self.hub.download(&self.device, session, action)?;
        let mut buf = vec![0u8; PIECE];
        loop {
            let n = file.read(&mut buf)?;
            if n == 0 {
                return Ok(());
            }
            sink(&buf[..n])?;
        }
    }

    fn set_auth(&mut self, device: &str, _key: SecretKey) {
        self.device = device.to_string();
    }

    fn address(&self) -> String {
        "direct".to_string()
    }
}

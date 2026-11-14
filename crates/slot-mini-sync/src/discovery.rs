//! Finding the hub, and the hub's "sync now" (docs/SYNC.md, "Discovery
//! and poke"). Plain UDP; nothing here waits in a loop that costs CPU.

use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::auth::{self, SecretKey};
use crate::PROTOCOL_VERSION;

/// The hub's server (TCP).
pub const HUB_PORT: u16 = 24735;
/// Where hubs answer `find` (UDP).
pub const DISCOVERY_PORT: u16 = 24735;
/// Where devices hear pokes (UDP).
pub const POKE_PORT: u16 = 24736;

const DATAGRAM: usize = 512;

/// A hub that answered.
#[derive(Clone, Debug, PartialEq)]
pub struct Found {
    pub id: String,
    pub name: String,
    /// Its server.
    pub address: SocketAddr,
    pub protocol: u32,
}

/// Ask `targets` who is a hub and collect the answers for `timeout`.
pub fn discover(timeout: Duration, targets: &[SocketAddr]) -> std::io::Result<Vec<Found>> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
    socket.set_broadcast(true)?;
    let ask = json!({"t": "find", "protocol": PROTOCOL_VERSION}).to_string();
    let mut sent = false;
    for target in targets {
        sent |= socket.send_to(ask.as_bytes(), target).is_ok();
    }
    if !sent {
        return Err(std::io::Error::other("no network to look for the PC on"));
    }
    let until = Instant::now() + timeout;
    let mut found: Vec<Found> = Vec::new();
    let mut buf = [0u8; DATAGRAM];
    loop {
        let left = until.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        socket.set_read_timeout(Some(left))?;
        let Ok((n, from)) = socket.recv_from(&mut buf) else {
            break;
        };
        let Ok(v) = serde_json::from_slice::<Value>(&buf[..n]) else {
            continue;
        };
        let (Some("hub"), Some(id), Some(port)) =
            (v["t"].as_str(), v["id"].as_str(), v["port"].as_u64())
        else {
            continue;
        };
        if found.iter().any(|f| f.id == id) {
            continue;
        }
        found.push(Found {
            id: id.to_string(),
            name: v["name"].as_str().unwrap_or("").to_string(),
            address: SocketAddr::new(from.ip(), port as u16),
            protocol: v["protocol"].as_u64().unwrap_or(0) as u32,
        });
    }
    Ok(found)
}

/// Ask the whole network.
pub fn discover_broadcast(timeout: Duration) -> std::io::Result<Vec<Found>> {
    discover(
        timeout,
        &[SocketAddr::from((Ipv4Addr::BROADCAST, DISCOVERY_PORT))],
    )
}

fn stopper(socket: &UdpSocket) {
    // Wake the blocking `recv` with a datagram to itself.
    if let (Ok(wake), Ok(mut addr)) = (
        UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)),
        socket.local_addr(),
    ) {
        if addr.ip().is_unspecified() {
            addr.set_ip(Ipv4Addr::LOCALHOST.into());
        }
        let _ = wake.send_to(b"{}", addr);
    }
}

/// The hub's answer to `find`, on a thread of its own.
pub struct Responder {
    socket: UdpSocket,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Responder {
    /// Answer on `bind` for the hub `id`, whose server is on `port`.
    pub fn start(bind: SocketAddr, id: &str, name: &str, port: u16) -> std::io::Result<Responder> {
        let socket = UdpSocket::bind(bind)?;
        let answer = json!({"t": "hub", "id": id, "name": name, "port": port,
                            "protocol": PROTOCOL_VERSION})
        .to_string();
        let stop = Arc::new(AtomicBool::new(false));
        let (listener, stopped) = (socket.try_clone()?, stop.clone());
        let thread = std::thread::Builder::new()
            .name("sync-discovery".into())
            .spawn(move || {
                let mut buf = [0u8; DATAGRAM];
                while let Ok((n, from)) = listener.recv_from(&mut buf) {
                    if stopped.load(Ordering::SeqCst) {
                        break;
                    }
                    let find = serde_json::from_slice::<Value>(&buf[..n])
                        .is_ok_and(|v| v["t"].as_str() == Some("find"));
                    if find {
                        let _ = listener.send_to(answer.as_bytes(), from);
                    }
                }
            })?;
        Ok(Responder {
            socket,
            stop,
            thread: Some(thread),
        })
    }

    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.socket.local_addr()
    }
}

impl Drop for Responder {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        stopper(&self.socket);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Send a device the hub's "sync now".
pub fn poke(
    hub_id: &str,
    key: &SecretKey,
    hub_port: u16,
    target: SocketAddr,
) -> std::io::Result<()> {
    let time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let body = json!({"t": "poke", "hub": hub_id, "time": time, "port": hub_port,
                      "mac": auth::sign_poke(key, hub_id, time, hub_port)})
    .to_string();
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
    socket.send_to(body.as_bytes(), target).map(|_| ())
}

/// A poke as it arrived. `Client::accept_poke` says whether it counts.
#[derive(Clone, Debug, PartialEq)]
pub struct Poke {
    pub hub: String,
    /// The hub's clock, milliseconds.
    pub time: u64,
    /// The hub's server port.
    pub port: u16,
    pub(crate) mac: String,
    pub from: SocketAddr,
}

/// The device's ear for pokes.
pub struct Poked {
    socket: UdpSocket,
    stop: Arc<AtomicBool>,
}

/// Stops a [`Poked::wait`] from another thread.
#[derive(Clone)]
pub struct PokedStop {
    socket: Arc<UdpSocket>,
    stop: Arc<AtomicBool>,
}

impl PokedStop {
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
        stopper(&self.socket);
    }
}

impl Poked {
    pub fn bind(addr: SocketAddr) -> std::io::Result<Poked> {
        Ok(Poked {
            socket: UdpSocket::bind(addr)?,
            stop: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.socket.local_addr()
    }

    pub fn stopper(&self) -> std::io::Result<PokedStop> {
        Ok(PokedStop {
            socket: Arc::new(self.socket.try_clone()?),
            stop: self.stop.clone(),
        })
    }

    /// Block until a poke arrives; `None` once stopped. Datagrams that
    /// are not pokes are dropped. This is a blocking `recv`: it costs
    /// nothing while it waits.
    pub fn wait(&self) -> Option<Poke> {
        let mut buf = [0u8; DATAGRAM];
        loop {
            let (n, from) = self.socket.recv_from(&mut buf).ok()?;
            if self.stop.load(Ordering::SeqCst) {
                return None;
            }
            let Ok(v) = serde_json::from_slice::<Value>(&buf[..n]) else {
                continue;
            };
            let (Some("poke"), Some(hub), Some(time), Some(port), Some(mac)) = (
                v["t"].as_str(),
                v["hub"].as_str(),
                v["time"].as_u64(),
                v["port"].as_u64(),
                v["mac"].as_str(),
            ) else {
                continue;
            };
            return Some(Poke {
                hub: hub.to_string(),
                time,
                port: port as u16,
                mac: mac.to_string(),
                from,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local() -> SocketAddr {
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0))
    }

    #[test]
    fn a_hub_answers_find() {
        let responder = Responder::start(local(), "hub1", "Joe's PC", 4321).unwrap();
        let at = responder.local_addr().unwrap();
        let found = discover(Duration::from_millis(500), &[at]).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(
            (found[0].id.as_str(), found[0].name.as_str()),
            ("hub1", "Joe's PC")
        );
        assert_eq!(
            found[0].address,
            SocketAddr::from((Ipv4Addr::LOCALHOST, 4321))
        );
        assert_eq!(found[0].protocol, PROTOCOL_VERSION);
        drop(responder);
        // Nobody there now.
        assert!(discover(Duration::from_millis(100), &[at])
            .unwrap()
            .is_empty());
    }

    #[test]
    fn a_poke_arrives_and_junk_does_not() {
        let poked = Poked::bind(local()).unwrap();
        let at = poked.local_addr().unwrap();
        let junk = UdpSocket::bind(local()).unwrap();
        junk.send_to(b"not json", at).unwrap();
        junk.send_to(br#"{"t":"find"}"#, at).unwrap();
        poke("hub1", &[1; 32], 4321, at).unwrap();
        let got = poked.wait().unwrap();
        assert_eq!((got.hub.as_str(), got.port), ("hub1", 4321));
        assert!(auth::verify_poke(
            &[1; 32], "hub1", got.time, 4321, &got.mac
        ));
        let stop = poked.stopper().unwrap();
        let waiter = std::thread::spawn(move || poked.wait());
        stop.stop();
        assert_eq!(waiter.join().unwrap(), None);
    }
}

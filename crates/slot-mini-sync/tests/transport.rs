//! The hub and the client over real sockets on localhost: what the
//! server refuses, what the client refuses, and what an interruption
//! leaves behind (docs/SYNC.md, "Protocol", "Authentication",
//! "Discovery and poke", "Failure and resume").

mod support;

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::time::Duration;

use serde_json::{json, Value};
use slot_mini_store::Platform;
use slot_mini_sync::{
    discover, hash_bytes, now_secs, poke, sign_request, Cancel, Event, GameId, HttpLink, Item, Key,
    Link, Manifest, NameRules, PairingStatus, Poked, Report, Scope, SecretKey, Server, SyncError,
    PROTOCOL_VERSION,
};
use support::{named, read, save, write, World};

fn local() -> SocketAddr {
    "127.0.0.1:0".parse().unwrap()
}

/// A paired Miyoo and PC, the PC behind its HTTP server.
struct Net {
    w: World,
    server: Server,
}

impl Net {
    fn new() -> Net {
        let mut w = World::unpaired();
        let server = Server::start(w.hub.clone(), local(), Some(local())).unwrap();
        let mut link = HttpLink::new(&server.addr().to_string());
        w.hub.start_pairing();
        let attempt = w.client.pair(&mut link).unwrap();
        assert_eq!(
            w.client.finish_pairing(&mut link, &attempt).unwrap(),
            PairingStatus::Waiting
        );
        assert!(w.hub.confirm_pairing());
        assert_eq!(
            w.client.finish_pairing(&mut link, &attempt).unwrap(),
            PairingStatus::Paired
        );
        // A first sync, so that the hub knows where the device is.
        let mut net = Net { w, server };
        assert!(net.sync(Scope::Auto).is_noop());
        net
    }

    fn link(&self) -> HttpLink {
        HttpLink::new(&self.server.addr().to_string())
    }

    fn sync(&mut self, scope: Scope) -> Report {
        let mut link = self.link();
        self.w
            .client
            .sync(&mut link, scope, &mut |_| {}, &Cancel::new())
    }

    /// The key the device and the hub share.
    fn key(&self) -> SecretKey {
        self.w.hub.poke_targets()[0].1
    }

    /// A request signed as the device would, with what a test changes.
    fn signed(&self, method: &str, target: &str, body: &[u8]) -> Signed {
        Signed {
            addr: self.server.addr(),
            method: method.to_string(),
            target: target.to_string(),
            device: self.w.client.device_id().to_string(),
            key: self.key(),
            time: now_secs(),
            nonce: format!("{:032x}", rand()),
            body_hash: hash_bytes(body),
            body: body.to_vec(),
            protocol: Some(PROTOCOL_VERSION.to_string()),
        }
    }

    fn post(&self, path: &str, body: &Value) -> (u16, Value) {
        self.signed("POST", path, body.to_string().as_bytes())
            .send()
    }
}

fn rand() -> u128 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(1);
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    t ^ ((N.fetch_add(1, Ordering::SeqCst) as u128) << 64)
}

struct Signed {
    addr: SocketAddr,
    method: String,
    target: String,
    device: String,
    key: SecretKey,
    time: u64,
    nonce: String,
    body_hash: String,
    body: Vec<u8>,
    protocol: Option<String>,
}

impl Signed {
    fn send(&self) -> (u16, Value) {
        let auth = sign_request(
            &self.key,
            &self.method,
            &self.target,
            self.time,
            &self.nonce,
            &self.body_hash,
        );
        let mut headers = vec![
            ("X-Slot-Device", self.device.clone()),
            ("X-Slot-Time", self.time.to_string()),
            ("X-Slot-Nonce", self.nonce.clone()),
            ("X-Slot-Body", self.body_hash.clone()),
            ("X-Slot-Auth", auth),
        ];
        if let Some(p) = &self.protocol {
            headers.push(("X-Slot-Protocol", p.clone()));
        }
        raw(self.addr, &self.method, &self.target, &headers, &self.body)
    }
}

/// One HTTP request by hand: the status and the JSON body.
fn raw(
    addr: SocketAddr,
    method: &str,
    target: &str,
    headers: &[(&str, String)],
    body: &[u8],
) -> (u16, Value) {
    let mut stream = TcpStream::connect(addr).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .unwrap();
    let mut head = format!("{method} {target} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n");
    for (name, value) in headers {
        head += &format!("{name}: {value}\r\n");
    }
    head += &format!("Content-Length: {}\r\n\r\n", body.len());
    stream.write_all(head.as_bytes()).unwrap();
    stream.write_all(body).unwrap();
    let mut reply = Vec::new();
    let _ = stream.read_to_end(&mut reply);
    let text = String::from_utf8_lossy(&reply);
    let status = text
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| panic!("no status in {text:?}"));
    let body = text.split_once("\r\n\r\n").map_or("", |(_, b)| b);
    (status, serde_json::from_str(body).unwrap_or(Value::Null))
}

fn code(reply: &(u16, Value)) -> (u16, &str) {
    (reply.0, reply.1["error"]["code"].as_str().unwrap_or(""))
}

fn rom(len: usize) -> Vec<u8> {
    (0..len).map(|n| (n * 31 % 251) as u8).collect()
}

fn hello(net: &Net) -> String {
    let reply = net.post(
        "/sync/hello",
        &json!({"scope": "auto", "device_name": "Miyoo"}),
    );
    assert_eq!(reply.0, 200, "{reply:?}");
    reply.1["session"].as_str().unwrap().to_string()
}

#[test]
fn a_sync_over_http_moves_files_both_ways() {
    let mut net = Net::new();
    write(&net.w.sd.join("Roms/GBA/Mine.gba"), &rom(300_000));
    write(&net.w.pc.join("Roms/GB/Yours.gb"), &rom(70_000));
    write(&net.w.pc.join("Labels/GB/Yours.png"), b"label");
    let mine = net.w.device().save_file(Platform::Gba, "Mine", "srm", None);
    write(&mine, &save(1));
    let report = net.sync(Scope::Full);
    assert!(report.is_clean(), "{report:?}");
    assert_eq!(
        report.to_client.added,
        ["label:GB/Yours", "rom:GB/Yours.gb"]
    );
    assert_eq!(
        report.to_hub.added,
        ["save:GBA/Mine.srm", "rom:GBA/Mine.gba"]
    );
    assert_eq!(read(&net.w.pc.join("Roms/GBA/Mine.gba")), rom(300_000));
    assert_eq!(read(&net.w.sd.join("Roms/GB/Yours.gb")), rom(70_000));
    assert_eq!(
        read(&net.w.pc.join("RetroArch/saves/mGBA/Mine.srm")),
        save(1)
    );
    assert!(net.sync(Scope::Full).is_noop());
    // The hub remembers where the device was, and what the sync did.
    let (device, _, last) = net.w.hub.last_report().unwrap();
    assert_eq!(device, net.w.client.device_name());
    assert!(last.is_noop());
    assert_eq!(
        net.w.client.hub().unwrap().address,
        net.server.addr().to_string()
    );
}

#[test]
fn a_bad_signature_is_refused() {
    let net = Net::new();
    let body = json!({"scope": "auto", "device_name": "Miyoo"}).to_string();
    // Signed with the key of some other pairing.
    let other = Net::new();
    let mut request = net.signed("POST", "/sync/hello", body.as_bytes());
    request.key = other.key();
    assert_eq!(code(&request.send()), (401, "bad-auth"));
    // A body that is not the one that was signed.
    let mut request = net.signed("POST", "/sync/hello", body.as_bytes());
    request.body = body.replace("auto", "full").into_bytes();
    assert_eq!(code(&request.send()), (401, "bad-auth"));
    // Signed for another path.
    let mut request = net.signed("POST", "/sync/done", body.as_bytes());
    request.target = "/sync/hello".into();
    let signed_for_done = sign_request(
        &request.key,
        "POST",
        "/sync/done",
        request.time,
        &request.nonce,
        &request.body_hash,
    );
    let reply = raw(
        request.addr,
        "POST",
        "/sync/hello",
        &[
            ("X-Slot-Protocol", PROTOCOL_VERSION.to_string()),
            ("X-Slot-Device", request.device.clone()),
            ("X-Slot-Time", request.time.to_string()),
            ("X-Slot-Nonce", request.nonce.clone()),
            ("X-Slot-Body", request.body_hash.clone()),
            ("X-Slot-Auth", signed_for_done),
        ],
        body.as_bytes(),
    );
    assert_eq!(code(&reply), (401, "bad-auth"));
    // No signature at all, and a device the hub never met.
    let reply = raw(
        net.server.addr(),
        "POST",
        "/sync/hello",
        &[("X-Slot-Protocol", PROTOCOL_VERSION.to_string())],
        body.as_bytes(),
    );
    assert_eq!(code(&reply), (401, "unknown-device"));
    let mut request = net.signed("POST", "/sync/hello", body.as_bytes());
    request.device = "0123456789abcdef0123456789abcdef".into();
    assert_eq!(code(&request.send()), (401, "unknown-device"));
    // None of that opened a session; the real thing still works.
    assert!(!net.w.hub.is_syncing());
    assert_eq!(
        net.signed("POST", "/sync/hello", body.as_bytes()).send().0,
        200
    );
}

#[test]
fn a_replayed_request_is_refused() {
    let net = Net::new();
    let body = json!({"scope": "auto", "device_name": "Miyoo"}).to_string();
    let request = net.signed("POST", "/sync/hello", body.as_bytes());
    assert_eq!(request.send().0, 200);
    assert_eq!(code(&request.send()), (401, "replay"));
}

#[test]
fn an_expired_timestamp_is_refused_and_the_hub_says_the_time() {
    let net = Net::new();
    let body = json!({"scope": "auto", "device_name": "Miyoo"}).to_string();
    for skew in [-1000i64, 1000] {
        let mut request = net.signed("POST", "/sync/hello", body.as_bytes());
        request.time = (now_secs() as i64 + skew) as u64;
        let reply = request.send();
        assert_eq!(code(&reply), (401, "stale"));
        let now = reply.1["error"]["now"].as_u64().unwrap();
        assert!(now.abs_diff(now_secs()) < 60);
    }
    assert!(!net.w.hub.is_syncing());
}

#[test]
fn another_protocol_version_is_refused_by_both_sides() {
    let mut net = Net::new();
    let body = json!({"scope": "auto", "device_name": "Miyoo"}).to_string();
    for theirs in [Some("2".to_string()), None] {
        let mut request = net.signed("POST", "/sync/hello", body.as_bytes());
        request.protocol = theirs;
        let reply = request.send();
        assert_eq!(code(&reply), (426, "protocol"));
        assert_eq!(reply.1["error"]["protocol"], json!(PROTOCOL_VERSION));
    }
    // `/info` answers anyone, so that each side can say who must update.
    let info = raw(net.server.addr(), "GET", "/info", &[], b"");
    assert_eq!(info.0, 200);
    assert_eq!(info.1["protocol"], json!(PROTOCOL_VERSION));
    assert_eq!(info.1["id"], json!(net.w.hub.id()));

    // A newer PC: the client stops before it sends anything of its own.
    let newer = TcpListener::bind(local()).unwrap();
    let addr = newer.local_addr().unwrap();
    let served = std::thread::spawn(move || {
        let (mut stream, _) = newer.accept().unwrap();
        let mut seen = Vec::new();
        let mut buf = [0u8; 1024];
        while !seen.windows(4).any(|w| w == b"\r\n\r\n") {
            let n = stream.read(&mut buf).unwrap();
            seen.extend_from_slice(&buf[..n]);
        }
        let body = json!({"id": "x", "name": "Newer", "protocol": 2, "now": now_secs()});
        let body = body.to_string();
        let reply = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(reply.as_bytes()).unwrap();
        String::from_utf8_lossy(&seen).into_owned()
    });
    let mut link = HttpLink::new(&addr.to_string());
    let report = net
        .w
        .client
        .sync(&mut link, Scope::Auto, &mut |_| {}, &Cancel::new());
    let error = report.interrupted.unwrap();
    assert_eq!(error, SyncError::Protocol { hub: 2, client: 1 });
    assert_eq!(
        error.to_string(),
        "Update slot-mini on the Miyoo to sync with this PC"
    );
    assert!(served.join().unwrap().starts_with("GET /info "));
    // It is still paired with its own PC.
    assert!(net.sync(Scope::Auto).is_noop());
}

#[test]
fn an_interrupted_transfer_leaves_nothing_and_the_next_sync_resumes() {
    let mut net = Net::new();
    let (down, up) = (rom(900_000), rom(700_001));
    write(&net.w.pc.join("Roms/GBA/Down.gba"), &down);
    write(&net.w.pc.join("Labels/GBA/Down.png"), b"label");
    write(&net.w.sd.join("Roms/GBC/Up.gbc"), &up);

    // The player stops the sync (or the Wi-Fi does) halfway through each
    // ROM in turn.
    for (name, to_client) in [("rom:GBA/Down.gba", true), ("rom:GBC/Up.gbc", false)] {
        let cancel = Cancel::new();
        let stop = cancel.clone();
        let mut link = net.link();
        let report = net.w.client.sync(
            &mut link,
            Scope::Full,
            &mut |event| {
                if let Event::Transfer {
                    key, done, size, ..
                } = event
                {
                    if key == name && done * 2 > size {
                        stop.cancel();
                    }
                }
            },
            &cancel,
        );
        assert_eq!(report.interrupted, Some(SyncError::Cancelled), "{name}");
        let (there, tree) = match to_client {
            true => (net.w.sd.join("Roms/GBA/Down.gba"), &net.w.sd),
            false => (net.w.pc.join("Roms/GBC/Up.gbc"), &net.w.pc),
        };
        assert!(!there.exists(), "{name} landed half-way");
        // The server may still be reading what the socket had buffered:
        // wait until nothing temporary has been there for a while.
        let mut quiet = 0;
        for _ in 0..500 {
            quiet = if named(tree, ".tmp").is_empty() {
                quiet + 1
            } else {
                0
            };
            if quiet == 25 {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(named(tree, ".tmp"), [] as [String; 0]);
        if to_client {
            // What came before the ROM is in place and is not sent again.
            assert_eq!(report.to_client.added, ["label:GBA/Down"]);
            // The next attempt stops in the other ROM instead.
            std::fs::remove_file(net.w.pc.join("Roms/GBA/Down.gba")).unwrap();
        } else {
            assert_eq!(report.to_client.added, [] as [String; 0]);
            write(&net.w.pc.join("Roms/GBA/Down.gba"), &down);
        }
    }

    let report = net.sync(Scope::Full);
    assert!(report.is_clean(), "{report:?}");
    assert_eq!(report.to_client.added, ["rom:GBA/Down.gba"]);
    assert_eq!(report.to_hub.added, ["rom:GBC/Up.gbc"]);
    assert_eq!(read(&net.w.sd.join("Roms/GBA/Down.gba")), down);
    assert_eq!(read(&net.w.pc.join("Roms/GBC/Up.gbc")), up);
    assert!(net.sync(Scope::Full).is_noop());
}

#[test]
fn a_file_that_is_not_what_the_plan_said_is_rejected_by_the_hub() {
    let net = Net::new();
    let session = hello(&net);
    let good = b"the label the device listed".to_vec();
    let mut manifest = Manifest::empty(NameRules::Fat);
    manifest.now = now_secs();
    manifest.games = vec![GameId::new(Platform::Gba, "Game")];
    manifest.items = vec![Item {
        key: Key::parse("label:GBA/Game").unwrap(),
        size: good.len() as u64,
        hash: hash_bytes(&good),
        played: None,
    }];
    let mut body = manifest.to_json();
    body["session"] = json!(session);
    let plan = net.post("/sync/plan", &body);
    assert_eq!(plan.0, 200, "{plan:?}");
    assert_eq!(plan.1["actions"][0]["op"], json!("push"));

    let target = format!("/sync/file?s={session}&a=0");
    let landed = net.w.pc.join("Labels/GBA/Game.png");
    // Other bytes of the same length, then too few, then too many.
    let mut wrong = good.clone();
    wrong[3] ^= 1;
    for bytes in [wrong, good[..5].to_vec(), [&good[..], b"more"].concat()] {
        let mut request = net.signed("PUT", &target, &bytes);
        request.body_hash = hash_bytes(&good);
        assert_eq!(code(&request.send()), (422, "hash-mismatch"));
        assert!(!landed.exists());
        assert_eq!(named(&net.w.pc, ".tmp"), [] as [String; 0]);
    }
    // An action the plan does not have.
    let request = net.signed("PUT", &format!("/sync/file?s={session}&a=7"), &good);
    assert_eq!(request.send().0, 400);
    // The right bytes land.
    assert_eq!(net.signed("PUT", &target, &good).send().0, 200);
    assert_eq!(read(&landed), good);
}

/// A link whose downloads arrive damaged.
struct Damaged(HttpLink);

impl Link for Damaged {
    fn info(&mut self) -> Result<Value, SyncError> {
        self.0.info()
    }
    fn call(&mut self, path: &str, body: &Value) -> Result<Value, SyncError> {
        self.0.call(path, body)
    }
    fn call_open(&mut self, path: &str, body: &Value) -> Result<Value, SyncError> {
        self.0.call_open(path, body)
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
        self.0.upload(session, action, path, size, hash, progress)
    }
    fn download(
        &mut self,
        session: &str,
        action: usize,
        sink: &mut dyn FnMut(&[u8]) -> Result<(), SyncError>,
    ) -> Result<(), SyncError> {
        let mut first = true;
        self.0.download(session, action, &mut |piece| {
            let mut piece = piece.to_vec();
            if std::mem::take(&mut first) {
                piece[0] ^= 0x80;
            }
            sink(&piece)
        })
    }
    fn set_auth(&mut self, device: &str, key: SecretKey) {
        self.0.set_auth(device, key);
    }
    fn address(&self) -> String {
        self.0.address()
    }
}

#[test]
fn a_file_that_is_not_what_the_plan_said_is_rejected_by_the_client() {
    let mut net = Net::new();
    write(&net.w.pc.join("Roms/GBA/Game.gba"), &rom(100_000));
    write(&net.w.pc.join("Labels/GBA/Game.png"), b"label");
    let mut link = Damaged(net.link());
    let report = net
        .w
        .client
        .sync(&mut link, Scope::Full, &mut |_| {}, &Cancel::new());
    // Each file is refused on its own; the sync goes on to the next.
    assert_eq!(report.interrupted, None);
    let failed: Vec<&str> = report.failed.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(failed, ["label:GBA/Game", "rom:GBA/Game.gba"]);
    assert!(report.to_client.is_empty());
    assert!(!net.w.sd.join("Roms/GBA/Game.gba").exists());
    assert!(!net.w.sd.join("Labels/GBA/Game.png").exists());
    assert_eq!(named(&net.w.sd, ".tmp"), [] as [String; 0]);
    // Over a sound connection they arrive.
    let report = net.sync(Scope::Full);
    assert!(report.is_clean(), "{report:?}");
    assert_eq!(report.to_client.added.len(), 2);
}

#[test]
fn unpairing_on_either_side_ends_it_for_both() {
    // On the PC: the device hears of it at its next sync and forgets.
    let mut net = Net::new();
    assert!(net.sync(Scope::Auto).is_noop());
    let device = net.w.client.device_id().to_string();
    assert_eq!(net.w.hub.devices().len(), 1);
    net.w.hub.unpair(&device).unwrap();
    assert!(net.w.hub.devices().is_empty());
    let report = net.sync(Scope::Auto);
    assert_eq!(report.interrupted, Some(SyncError::Unpaired));
    assert!(net.w.client.hub().is_none());
    assert_eq!(
        net.sync(Scope::Auto).interrupted,
        Some(SyncError::NotPaired)
    );
    // Its old key no longer signs anything.
    let body = json!({"scope": "auto", "device_name": "Miyoo"}).to_string();
    let mut request = Net::new().signed("POST", "/sync/hello", body.as_bytes());
    request.addr = net.server.addr();
    request.device = device;
    assert_eq!(code(&request.send()), (401, "unknown-device"));

    // On the Miyoo: the PC is told, and forgets the device.
    let mut net = Net::new();
    let mut link = net.link();
    net.w.client.unpair(Some(&mut link)).unwrap();
    assert!(net.w.client.hub().is_none());
    assert!(net.w.hub.devices().is_empty());

    // On the Miyoo with the PC out of reach: the device forgets anyway.
    let mut net = Net::new();
    let mut nowhere = HttpLink::new("127.0.0.1:1");
    net.w.client.unpair(Some(&mut nowhere)).unwrap();
    assert!(net.w.client.hub().is_none());
    assert_eq!(net.w.hub.devices().len(), 1);
}

#[test]
fn a_poke_with_a_bad_signature_is_ignored() {
    let mut net = Net::new();
    // The hub pokes the address the device last synced from.
    assert!(net.sync(Scope::Auto).is_noop());
    let poked = Poked::bind(local()).unwrap();
    let port = poked.local_addr().unwrap();
    let hub_id = net.w.hub.id().to_string();

    // Someone else's key, under this hub's name.
    let other = Net::new();
    poke(&hub_id, &other.key(), net.server.addr().port(), port).unwrap();
    let forged = poked.wait().unwrap();
    assert_eq!(forged.hub, hub_id);
    assert!(!net.w.client.accept_poke(&forged));
    // Another hub altogether.
    poke(other.w.hub.id(), &other.key(), 1, port).unwrap();
    assert!(!net.w.client.accept_poke(&poked.wait().unwrap()));

    // The real one counts, once.
    assert_eq!(net.server.poke(port.port()), 1);
    let real = poked.wait().unwrap();
    assert!(net.w.client.accept_poke(&real));
    assert!(!net.w.client.accept_poke(&real), "a poke replayed");
    assert_eq!(
        net.w.client.hub().unwrap().address,
        net.server.addr().to_string()
    );
    // And the sync it asks for runs.
    assert!(net.sync(Scope::Auto).is_noop());

    // Stopping the ear ends its wait.
    let stop = poked.stopper().unwrap();
    let waiting = std::thread::spawn(move || poked.wait());
    std::thread::sleep(Duration::from_millis(50));
    stop.stop();
    assert_eq!(waiting.join().unwrap(), None);
}

#[test]
fn discovery_finds_the_hub_and_its_port() {
    let net = Net::new();
    let at = net.server.discovery_addr().unwrap();
    let found = discover(Duration::from_millis(500), &[at]).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].id, net.w.hub.id());
    assert_eq!(found[0].name, "Desk");
    assert_eq!(found[0].address.port(), net.server.addr().port());
    // Nobody home.
    let quiet = std::net::UdpSocket::bind(local()).unwrap();
    let nobody = discover(Duration::from_millis(200), &[quiet.local_addr().unwrap()]).unwrap();
    assert!(nobody.is_empty());
}

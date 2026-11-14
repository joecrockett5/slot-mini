//! Request signing and the pairing key exchange (docs/SYNC.md, "Pairing"
//! and "Authentication").

use std::collections::HashMap;

use ring::agreement::{agree_ephemeral, EphemeralPrivateKey, UnparsedPublicKey, X25519};
use ring::rand::{SecureRandom, SystemRandom};
use ring::{digest, hkdf, hmac};

use crate::error::SyncError;
use crate::hash::{hex, unhex};

/// How far a request's time may be from the hub's clock.
pub const REPLAY_WINDOW_SECS: u64 = 120;

/// The key a hub and a device share.
pub type SecretKey = [u8; 32];

fn mac(key: &SecretKey, message: &str) -> String {
    let key = hmac::Key::new(hmac::HMAC_SHA256, key);
    hex(hmac::sign(&key, message.as_bytes()).as_ref())
}

fn verify(key: &SecretKey, message: &str, mac: &str) -> bool {
    let key = hmac::Key::new(hmac::HMAC_SHA256, key);
    unhex(mac).is_some_and(|mac| hmac::verify(&key, message.as_bytes(), &mac).is_ok())
}

fn request_message(method: &str, target: &str, time: u64, nonce: &str, body_hash: &str) -> String {
    format!("slot-mini-sync/1\n{method}\n{target}\n{time}\n{nonce}\n{body_hash}")
}

/// The `X-Slot-Auth` of a request. `target` is the path and query.
pub fn sign_request(
    key: &SecretKey,
    method: &str,
    target: &str,
    time: u64,
    nonce: &str,
    body_hash: &str,
) -> String {
    mac(
        key,
        &request_message(method, target, time, nonce, body_hash),
    )
}

pub(crate) fn verify_request(
    key: &SecretKey,
    method: &str,
    target: &str,
    time: u64,
    nonce: &str,
    body_hash: &str,
    auth: &str,
) -> bool {
    verify(
        key,
        &request_message(method, target, time, nonce, body_hash),
        auth,
    )
}

fn reply_message(nonce: &str, status: u16, body_hash: &str) -> String {
    format!("slot-mini-sync/1 reply\n{nonce}\n{status}\n{body_hash}")
}

/// The `X-Slot-Auth` of the reply to the request that carried `nonce`.
pub fn sign_reply(key: &SecretKey, nonce: &str, status: u16, body_hash: &str) -> String {
    mac(key, &reply_message(nonce, status, body_hash))
}

pub(crate) fn verify_reply(
    key: &SecretKey,
    nonce: &str,
    status: u16,
    body_hash: &str,
    auth: &str,
) -> bool {
    verify(key, &reply_message(nonce, status, body_hash), auth)
}

fn poke_message(hub: &str, time: u64, port: u16) -> String {
    format!("slot-mini-sync/1 poke\n{hub}\n{time}\n{port}")
}

pub(crate) fn sign_poke(key: &SecretKey, hub: &str, time: u64, port: u16) -> String {
    mac(key, &poke_message(hub, time, port))
}

pub(crate) fn verify_poke(key: &SecretKey, hub: &str, time: u64, port: u16, auth: &str) -> bool {
    verify(key, &poke_message(hub, time, port), auth)
}

fn crypto<E>(_: E) -> SyncError {
    SyncError::Io("the system's random numbers or key exchange failed".into())
}

pub(crate) fn random(n: usize) -> Result<Vec<u8>, SyncError> {
    let mut out = vec![0u8; n];
    SystemRandom::new().fill(&mut out).map_err(crypto)?;
    Ok(out)
}

/// `n` random bytes as hex.
pub(crate) fn random_hex(n: usize) -> Result<String, SyncError> {
    Ok(hex(&random(n)?))
}

/// What the client sends first: `SHA-256(public ‖ nonce)`, which fixes its
/// key before it has seen the hub's.
pub(crate) fn commitment(public: &[u8], nonce: &[u8]) -> String {
    let mut ctx = digest::Context::new(&digest::SHA256);
    ctx.update(public);
    ctx.update(nonce);
    hex(ctx.finish().as_ref())
}

/// One side's ephemeral X25519 key pair.
pub(crate) struct PairKeys {
    private: EphemeralPrivateKey,
    pub public: Vec<u8>,
}

struct Len(usize);

impl hkdf::KeyType for Len {
    fn len(&self) -> usize {
        self.0
    }
}

impl PairKeys {
    pub fn new() -> Result<PairKeys, SyncError> {
        let private =
            EphemeralPrivateKey::generate(&X25519, &SystemRandom::new()).map_err(crypto)?;
        let public = private.compute_public_key().map_err(crypto)?;
        Ok(PairKeys {
            public: public.as_ref().to_vec(),
            private,
        })
    }

    /// The shared key and the 6-digit code both screens show.
    pub fn finish(
        self,
        their_public: &[u8],
        client_public: &[u8],
        hub_public: &[u8],
        device_id: &str,
        hub_id: &str,
    ) -> Result<(SecretKey, String), SyncError> {
        let theirs = UnparsedPublicKey::new(&X25519, their_public);
        agree_ephemeral(self.private, &theirs, |shared| {
            let prk = hkdf::Salt::new(hkdf::HKDF_SHA256, b"slot-mini-sync pair v1").extract(shared);
            let info = [
                client_public,
                hub_public,
                device_id.as_bytes(),
                hub_id.as_bytes(),
            ];
            let mut out = [0u8; 36];
            prk.expand(&info, Len(36))
                .and_then(|okm| okm.fill(&mut out))
                .map_err(crypto)?;
            let mut key = [0u8; 32];
            key.copy_from_slice(&out[..32]);
            let n = u32::from_be_bytes([out[32], out[33], out[34], out[35]]) % 1_000_000;
            Ok((key, format!("{n:06}")))
        })
        .map_err(|_| SyncError::Bad("the other side's pairing key is not valid".into()))?
    }
}

/// The nonces each device has used inside the window.
#[derive(Default)]
pub(crate) struct Nonces {
    seen: HashMap<String, Vec<(u64, String)>>,
}

impl Nonces {
    /// Note a nonce; false if this device already used it.
    pub fn fresh(&mut self, device: &str, nonce: &str, time: u64, now: u64) -> bool {
        let list = self.seen.entry(device.to_string()).or_default();
        list.retain(|(t, _)| t + 2 * REPLAY_WINDOW_SECS >= now);
        if list.iter().any(|(_, n)| n == nonce) {
            return false;
        }
        list.push((time, nonce.to_string()));
        true
    }

    pub fn forget(&mut self, device: &str) {
        self.seen.remove(device);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signatures_cover_every_part_of_a_request() {
        let key = [7u8; 32];
        let auth = sign_request(&key, "POST", "/sync/plan", 100, "aa", "bb");
        assert!(verify_request(
            &key,
            "POST",
            "/sync/plan",
            100,
            "aa",
            "bb",
            &auth
        ));
        assert!(!verify_request(
            &[8u8; 32],
            "POST",
            "/sync/plan",
            100,
            "aa",
            "bb",
            &auth
        ));
        assert!(!verify_request(
            &key,
            "GET",
            "/sync/plan",
            100,
            "aa",
            "bb",
            &auth
        ));
        assert!(!verify_request(
            &key,
            "POST",
            "/sync/done",
            100,
            "aa",
            "bb",
            &auth
        ));
        assert!(!verify_request(
            &key,
            "POST",
            "/sync/plan",
            101,
            "aa",
            "bb",
            &auth
        ));
        assert!(!verify_request(
            &key,
            "POST",
            "/sync/plan",
            100,
            "ab",
            "bb",
            &auth
        ));
        assert!(!verify_request(
            &key,
            "POST",
            "/sync/plan",
            100,
            "aa",
            "bc",
            &auth
        ));
        assert!(!verify_request(
            &key,
            "POST",
            "/sync/plan",
            100,
            "aa",
            "bb",
            "not hex"
        ));
        let reply = sign_reply(&key, "aa", 200, "bb");
        assert!(verify_reply(&key, "aa", 200, "bb", &reply));
        assert!(!verify_reply(&key, "aa", 500, "bb", &reply));
        assert_ne!(reply, auth);
        let poke = sign_poke(&key, "hub", 5, 24735);
        assert!(verify_poke(&key, "hub", 5, 24735, &poke));
        assert!(!verify_poke(&key, "hub", 6, 24735, &poke));
    }

    #[test]
    fn both_sides_of_a_pairing_get_the_same_key_and_code() {
        let (client, hub) = (PairKeys::new().unwrap(), PairKeys::new().unwrap());
        let (cp, hp) = (client.public.clone(), hub.public.clone());
        let a = client.finish(&hp, &cp, &hp, "device", "hub").unwrap();
        let b = hub.finish(&cp, &cp, &hp, "device", "hub").unwrap();
        assert_eq!(a, b);
        assert_eq!(a.1.len(), 6);
        assert!(a.1.bytes().all(|b| b.is_ascii_digit()));
        // Another exchange, another key.
        let (c2, h2) = (PairKeys::new().unwrap(), PairKeys::new().unwrap());
        let other = c2.finish(&h2.public, &cp, &hp, "device", "hub").unwrap();
        assert_ne!(other.0, a.0);
        // The commitment fixes the key and the nonce.
        let nonce = random(16).unwrap();
        assert_eq!(commitment(&cp, &nonce), commitment(&cp, &nonce));
        assert_ne!(commitment(&cp, &nonce), commitment(&hp, &nonce));
        assert!(PairKeys::new()
            .unwrap()
            .finish(&[1, 2, 3], &cp, &hp, "d", "h")
            .is_err());
    }

    #[test]
    fn a_nonce_works_once() {
        let mut n = Nonces::default();
        assert!(n.fresh("a", "x", 100, 100));
        assert!(!n.fresh("a", "x", 100, 101));
        assert!(n.fresh("b", "x", 100, 101));
        assert!(n.fresh("a", "y", 100, 101));
        // Long after the window the list is pruned; the time check has
        // already refused the request by then.
        assert!(n.fresh("a", "x", 100, 100 + 3 * REPLAY_WINDOW_SECS));
    }
}

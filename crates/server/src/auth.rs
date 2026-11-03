//! Who a player is: an Elo Pros pass, checked offline (docs/DESIGN.md §11).
//! A port of Gates' `crates/server/src/auth.rs` `verify_pass`, against
//! Elo Pros' own vector (`tests/testdata/account_token.json`).
//!
//! A pass is `b64url(text) "." b64url(ed25519 signature over text)`, signed
//! by Elo Pros' account key (`GET /api/account/pubkey`). The text is exactly
//! seven lines: `elo account`, `id: acct_…`, `for: play:<slug>`,
//! `shard: <the page's host>`, `nonce: <ours, once>`, `wallet: 0x…|none`,
//! `exp: <unix>`. The nonce is one this server (or its other slot, holding
//! the same key) handed out (`Nonces`), so a pass can't be replayed at
//! another join.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine;

/// Who signed in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Passed {
    /// `acct_` and 20 lower-case hex.
    pub account: String,
    /// A linked wallet, lower-case `0x…`: what a rating belongs to.
    pub wallet: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PassError {
    Malformed,
    WrongSigner,
    /// For another game, or handed to another host.
    Elsewhere,
    /// A nonce we didn't issue, or already used.
    Replayed,
    Expired,
}

pub fn now_unix() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// `n` lower-case hex digits.
fn hexn(s: &str, n: usize) -> bool {
    s.len() == n && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Checks a pass. `nonce_ok` says whether the pass's nonce is one this
/// server issued and hasn't seen used (and marks it used).
pub fn verify_pass(
    pass: &str,
    key: &[u8; 32],
    slug: &str,
    domains: &[String],
    nonce_ok: impl FnOnce(&str) -> bool,
    now: u64,
) -> Result<Passed, PassError> {
    let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let (body, sig) = pass.split_once('.').ok_or(PassError::Malformed)?;
    let body = b64.decode(body).map_err(|_| PassError::Malformed)?;
    let sig = b64.decode(sig).map_err(|_| PassError::Malformed)?;
    ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, key)
        .verify(&body, &sig)
        .map_err(|_| PassError::WrongSigner)?;
    let text = core::str::from_utf8(&body).map_err(|_| PassError::Malformed)?;
    let mut lines = text.split('\n');
    if lines.next() != Some("elo account") {
        return Err(PassError::Malformed);
    }
    let mut take = |name: &str| {
        lines
            .next()
            .and_then(|l| l.strip_prefix(name))
            .and_then(|l| l.strip_prefix(": "))
            .ok_or(PassError::Malformed)
    };
    let id = take("id")?;
    let game = take("for")?;
    let shard = take("shard")?;
    let nonce = take("nonce")?;
    let wallet = take("wallet")?;
    let exp = take("exp")?;
    if lines.next().is_some() {
        return Err(PassError::Malformed);
    }
    if !id.strip_prefix("acct_").is_some_and(|h| hexn(h, 20)) {
        return Err(PassError::Malformed);
    }
    let wallet = match wallet {
        "none" => None,
        w if w.strip_prefix("0x").is_some_and(|h| hexn(h, 40)) => Some(w.to_string()),
        _ => return Err(PassError::Malformed),
    };
    let exp: u64 = exp.parse().map_err(|_| PassError::Malformed)?;
    if game.strip_prefix("play:") != Some(slug) || !domains.iter().any(|d| d == shard) {
        return Err(PassError::Elsewhere);
    }
    if exp <= now {
        return Err(PassError::Expired);
    }
    if !nonce_ok(nonce) {
        return Err(PassError::Replayed);
    }
    Ok(Passed {
        account: id.to_string(),
        wallet,
    })
}

/// The nonces this server hands the page (`GET /nonce`), each good once for
/// ten minutes.
///
/// A nonce carries its own proof: `hex(issued unix seconds, 8 bytes)`,
/// `hex(8 random bytes)`, then the first 16 bytes of their HMAC-SHA256 under
/// the key, in hex: 64 characters, inside Elo Pros' `[0-9A-Za-z]{8,64}`.
/// So it needs no list of what was issued, and two servers holding one key
/// (the two slots, `ops/oneshot@.service`) each take the other's: a page
/// handed a nonce by the slot going down signs in at the slot coming up.
/// Each server still takes a nonce once, remembering it until it expires.
pub struct Nonces {
    key: ring::hmac::Key,
    used: Mutex<BTreeMap<String, u64>>,
}

pub const NONCE_TTL: Duration = Duration::from_secs(600);
const NONCE_CAP: usize = 100_000;

impl Default for Nonces {
    /// A key of its own: nonces only this process takes.
    fn default() -> Self {
        let mut k = [0u8; 32];
        let _ = getrandom::fill(&mut k);
        Nonces::with_key(&k)
    }
}

impl Nonces {
    /// Nonces any server holding `key` takes.
    pub fn with_key(key: &[u8; 32]) -> Nonces {
        Nonces {
            key: ring::hmac::Key::new(ring::hmac::HMAC_SHA256, key),
            used: Mutex::new(BTreeMap::new()),
        }
    }

    fn mac(&self, head: &str) -> String {
        crate::hex(&ring::hmac::sign(&self.key, head.as_bytes()).as_ref()[..16])
    }

    /// A fresh nonce: 64 lower-case hex.
    pub fn issue(&self) -> String {
        self.issue_at(now_unix())
    }

    pub fn issue_at(&self, now: u64) -> String {
        let mut r = [0u8; 8];
        let _ = getrandom::fill(&mut r);
        let head = format!("{}{}", crate::hex(&now.to_be_bytes()), crate::hex(&r));
        let mac = self.mac(&head);
        head + &mac
    }

    /// Whether `n` was issued under our key in the last [`NONCE_TTL`] and
    /// hasn't been taken here before. Takes it.
    pub fn take(&self, n: &str) -> bool {
        self.take_at(n, now_unix())
    }

    pub fn take_at(&self, n: &str, now: u64) -> bool {
        if !hexn(n, 64) {
            return false;
        }
        let (head, mac) = n.split_at(32);
        // Compared whole, in constant time.
        if !constant_eq(self.mac(head).as_bytes(), mac.as_bytes()) {
            return false;
        }
        let Ok(at) = u64::from_str_radix(&head[..16], 16) else {
            return false;
        };
        // Issued in the window (a little ahead allows the two slots' clocks).
        if at > now + 5 || now.saturating_sub(at) >= NONCE_TTL.as_secs() {
            return false;
        }
        let mut used = self.used.lock().unwrap_or_else(|e| e.into_inner());
        used.retain(|_, t| now.saturating_sub(*t) < NONCE_TTL.as_secs());
        if used.contains_key(n) || used.len() >= NONCE_CAP {
            return false;
        }
        used.insert(n.to_string(), at);
        true
    }
}

fn constant_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    const VECTOR: &str = include_str!("../tests/testdata/account_token.json");

    fn field<'a>(json: &'a str, after: &str, key: &str) -> &'a str {
        let at = json.find(after).unwrap();
        let tail = &json[at..];
        let k = tail.find(&format!("\"{key}\": \"")).unwrap() + key.len() + 5;
        let end = tail[k..].find('"').unwrap();
        &tail[k..k + end]
    }

    fn key() -> [u8; 32] {
        let b = base64::engine::general_purpose::STANDARD
            .decode(field(VECTOR, "\"pubkey\"", "pubkey"))
            .unwrap();
        b.try_into().unwrap()
    }

    const NONCE: &str = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";

    fn check(pass: &str, slug: &str, domain: &str, now: u64) -> Result<Passed, PassError> {
        verify_pass(pass, &key(), slug, &[domain.to_string()], |n| n == NONCE, now)
    }

    #[test]
    fn elo_pros_vector_checks_out() {
        let with = field(VECTOR, "with_wallet", "pass");
        let p = check(with, "gates", "elopros.com", 1_700_000_000).unwrap();
        assert_eq!(p.account, "acct_0123456789abcdef0123");
        assert_eq!(p.wallet.as_deref(), Some("0x24445efddb08d4938e3e3627042b2cf4063d9092"));
        let without = field(VECTOR, "no_wallet", "pass");
        assert_eq!(
            check(without, "gates", "elopros.com", 1_700_000_000).unwrap().wallet,
            None
        );
    }

    #[test]
    fn what_is_refused() {
        let pass = field(VECTOR, "with_wallet", "pass");
        assert_eq!(check(pass, "oneshot", "elopros.com", 0), Err(PassError::Elsewhere));
        assert_eq!(check(pass, "gates", "evil.example", 0), Err(PassError::Elsewhere));
        assert_eq!(
            check(pass, "gates", "elopros.com", 1_800_000_000),
            Err(PassError::Expired)
        );
        let replayed = verify_pass(pass, &key(), "gates", &["elopros.com".into()], |_| false, 0);
        assert_eq!(replayed, Err(PassError::Replayed));
        // One character of the signature changed.
        let mut bad = pass.to_string();
        let last = bad.pop().unwrap();
        bad.push(if last == 'A' { 'B' } else { 'A' });
        assert!(matches!(
            check(&bad, "gates", "elopros.com", 0),
            Err(PassError::WrongSigner | PassError::Malformed)
        ));
        assert_eq!(
            check("not a pass", "gates", "elopros.com", 0),
            Err(PassError::Malformed)
        );
    }

    #[test]
    fn nonces_are_good_once() {
        let n = Nonces::default();
        let a = n.issue();
        assert_eq!(a.len(), 64);
        assert!(a.bytes().all(|b| b.is_ascii_alphanumeric()), "Elo Pros' nonce shape");
        assert!(n.take(&a));
        assert!(!n.take(&a));
        assert!(!n.take("00000000000000000000000000000000"));
        assert!(!n.take(&"0".repeat(64)));
    }

    #[test]
    fn a_nonce_is_taken_by_any_server_with_the_key_and_no_other() {
        let k = [9u8; 32];
        let (a, b, other) = (Nonces::with_key(&k), Nonces::with_key(&k), Nonces::default());
        let n = a.issue_at(1000);
        // The slot coming up takes what the slot going down handed out.
        assert!(b.take_at(&n, 1001));
        assert!(!b.take_at(&n, 1002), "once per server");
        assert!(a.take_at(&n, 1003), "each server keeps its own");
        assert!(!other.take_at(&a.issue_at(1000), 1001), "another key");
        // Ten minutes, and not from the future.
        assert!(!b.take_at(&a.issue_at(1000), 1000 + 600));
        assert!(b.take_at(&a.issue_at(1000), 1000 + 599));
        assert!(!b.take_at(&a.issue_at(2000), 1000));
        // A changed character anywhere spoils it.
        let n = a.issue_at(1000);
        for i in [0, 20, 40, 63] {
            let mut bad = n.clone().into_bytes();
            bad[i] = if bad[i] == b'0' { b'1' } else { b'0' };
            assert!(!b.take_at(std::str::from_utf8(&bad).unwrap(), 1001));
        }
    }
}

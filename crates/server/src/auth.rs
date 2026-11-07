//! Who a player is: an Elo Pros pass, checked offline (docs/DESIGN.md §11).
//! A port of Gates' `crates/server/src/auth.rs` `verify_pass`, against
//! Elo Pros' own vector (`tests/testdata/account_token.json`).
//!
//! A pass is `b64url(text) "." b64url(ed25519 signature over text)`, signed
//! by Elo Pros' account key (`GET /api/account/pubkey`). The text is exactly
//! seven lines: `elo account`, `id: acct_…`, `for: play:<slug>`,
//! `shard: <the page's host>`, `nonce: <ours, once>`, `wallet: 0x…|base58|none`,
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
    /// A linked wallet, lower-case `0x…` or a Solana wallet as base58 spells
    /// it: what a rating belongs to.
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
        w if !w.starts_with("0x") && proto::party::is_wallet(w) => Some(w.to_string()),
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
        getrandom::fill(&mut k).expect("the OS gave no randomness for the nonce key");
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
        getrandom::fill(&mut r).expect("the OS gave no randomness for a nonce");
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

/// How long a ticket is good for: twelve hours' play without signing in
/// again.
pub const TICKET_TTL: Duration = Duration::from_secs(12 * 3600);
/// What a ticket starts with: a pass never does (a pass is base64url).
pub use proto::TICKET_PREFIX;

/// Who a session is: an Elo Pros player (a pass, or a ticket a pass
/// earned), or a guest of ours with an id we made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Who {
    /// The player id Elo Pros uses (the wallet if there is one, else the
    /// account), or `guest_` and 16 hex for a guest.
    pub id: String,
    pub account: Option<String>,
    pub wallet: Option<String>,
}

impl Who {
    pub fn guest() -> Who {
        let mut b = [0u8; 8];
        getrandom::fill(&mut b).expect("the OS gave no randomness for a guest id");
        Who {
            id: format!("guest_{}", crate::hex(&b)),
            account: None,
            wallet: None,
        }
    }

    /// Signed in on Elo Pros (not a guest).
    pub fn signed(&self) -> bool {
        self.account.is_some()
    }
}

impl From<Passed> for Who {
    fn from(p: Passed) -> Who {
        Who {
            id: p.wallet.clone().unwrap_or_else(|| p.account.clone()),
            account: Some(p.account),
            wallet: p.wallet,
        }
    }
}

/// A ticket's id, account or wallet: what may sit in one.
fn ticket_field(s: &str) -> bool {
    !s.is_empty() && s.len() <= 64 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

impl Nonces {
    fn ticket_mac(&self, text: &str) -> String {
        let mut m = String::from("oneshot ticket\n");
        m.push_str(text);
        crate::hex(&ring::hmac::sign(&self.key, m.as_bytes()).as_ref()[..16])
    }

    /// A ticket for `who`: `os1.` b64url(text) `.` its MAC under our key,
    /// so any server holding the key (the other slot) takes it, until it
    /// expires. Printable ASCII and under a pass's length, so it rides
    /// where a pass does.
    pub fn ticket(&self, who: &Who) -> String {
        self.ticket_at(who, now_unix())
    }

    pub fn ticket_at(&self, who: &Who, now: u64) -> String {
        let text = format!(
            "{}\n{}\n{}\n{}",
            who.id,
            who.account.as_deref().unwrap_or("none"),
            who.wallet.as_deref().unwrap_or("none"),
            now + TICKET_TTL.as_secs()
        );
        let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
        format!("{TICKET_PREFIX}{}.{}", b64.encode(&text), self.ticket_mac(&text))
    }

    /// Who a ticket of ours names, if it is ours and still good.
    pub fn check_ticket(&self, t: &str) -> Option<Who> {
        self.check_ticket_at(t, now_unix())
    }

    pub fn check_ticket_at(&self, t: &str, now: u64) -> Option<Who> {
        let (body, mac) = t.strip_prefix(TICKET_PREFIX)?.split_once('.')?;
        let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
        let text = String::from_utf8(b64.decode(body).ok()?).ok()?;
        if !constant_eq(self.ticket_mac(&text).as_bytes(), mac.as_bytes()) {
            return None;
        }
        let mut f = text.split('\n');
        let (id, account, wallet, exp) = (f.next()?, f.next()?, f.next()?, f.next()?);
        if f.next().is_some() || ![id, account, wallet].iter().all(|s| ticket_field(s)) {
            return None;
        }
        if exp.parse::<u64>().ok()? <= now {
            return None;
        }
        let some = |s: &str| (s != "none").then(|| s.to_string());
        Some(Who {
            id: id.to_string(),
            account: some(account),
            wallet: some(wallet),
        })
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
    fn a_pass_may_name_a_solana_wallet() {
        let pass = field(VECTOR, "solana_wallet", "pass");
        let p = check(pass, "gates", "elopros.com", 1_700_000_000).unwrap();
        assert_eq!(
            p.wallet.as_deref(),
            Some("4uV4Yccau69gBh88Z8d7TbQJYCVgqsugJQuhQ1LCqYSd")
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
    fn a_ticket_names_who_earned_it_until_it_expires() {
        let k = [3u8; 32];
        let (a, b, other) = (Nonces::with_key(&k), Nonces::with_key(&k), Nonces::default());
        let who = Who {
            id: "0x24445efddb08d4938e3e3627042b2cf4063d9092".into(),
            account: Some("acct_0123456789abcdef0123".into()),
            wallet: Some("0x24445efddb08d4938e3e3627042b2cf4063d9092".into()),
        };
        let t = a.ticket_at(&who, 1000);
        assert!(proto::Pass::new(&t).is_some(), "rides where a pass does: {t}");
        assert_eq!(
            b.check_ticket_at(&t, 1001),
            Some(who.clone()),
            "either slot, again and again"
        );
        assert_eq!(b.check_ticket_at(&t, 1002), Some(who.clone()));
        assert_eq!(other.check_ticket_at(&t, 1001), None, "another key");
        assert_eq!(b.check_ticket_at(&t, 1000 + TICKET_TTL.as_secs()), None, "expired");
        // A guest's, and a tampered one.
        let g = Who::guest();
        assert!(!g.signed());
        assert_eq!(a.check_ticket_at(&a.ticket_at(&g, 5), 6), Some(g));
        let mut bad = t.clone().into_bytes();
        bad[6] = if bad[6] == b'A' { b'B' } else { b'A' };
        assert_eq!(a.check_ticket_at(std::str::from_utf8(&bad).unwrap(), 1001), None);
        // A nonce's MAC is no ticket's.
        assert_eq!(a.check_ticket_at(&a.issue_at(1000), 1001), None);
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

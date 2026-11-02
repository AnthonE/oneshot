//! Who a player is: an Elo Pros pass, checked offline (docs/DESIGN.md §11).
//! A port of Gates' `crates/server/src/auth.rs` `verify_pass`, against
//! Elo Pros' own vector (`tests/testdata/account_token.json`).
//!
//! A pass is `b64url(text) "." b64url(ed25519 signature over text)`, signed
//! by Elo Pros' account key (`GET /api/account/pubkey`). The text is exactly
//! seven lines: `elo account`, `id: acct_…`, `for: play:<slug>`,
//! `shard: <the page's host>`, `nonce: <ours, once>`, `wallet: 0x…|none`,
//! `exp: <unix>`. The nonce is one this server handed out (`Nonces`), so a
//! pass can't be replayed at another join.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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
#[derive(Default)]
pub struct Nonces {
    live: Mutex<BTreeMap<String, Instant>>,
}

pub const NONCE_TTL: Duration = Duration::from_secs(600);
const NONCE_CAP: usize = 100_000;

impl Nonces {
    /// A fresh nonce: 32 lower-case hex.
    pub fn issue(&self) -> String {
        let mut b = [0u8; 16];
        let _ = getrandom::fill(&mut b);
        let n = crate::hex(&b);
        let mut live = self.live.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        live.retain(|_, t| now.duration_since(*t) < NONCE_TTL);
        if live.len() < NONCE_CAP {
            live.insert(n.clone(), now);
        }
        n
    }

    /// Whether `n` was issued here, in time, and not used before. Uses it.
    pub fn take(&self, n: &str) -> bool {
        let mut live = self.live.lock().unwrap_or_else(|e| e.into_inner());
        live.remove(n).is_some_and(|t| t.elapsed() < NONCE_TTL)
    }
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
        assert_eq!(a.len(), 32);
        assert!(n.take(&a));
        assert!(!n.take(&a));
        assert!(!n.take("00000000000000000000000000000000"));
    }
}

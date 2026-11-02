//! Elo Pros: ratings in, results out (docs/DESIGN.md §11).
//!
//! A result is `POST /api/ratings/<slug>/report`, signed with this server's
//! Ed25519 key the way Elo Pros' `meter/serverkeys.py` checks it, and as its
//! Rust SDK (`sdk/rust/src/server.rs`) builds it: the text `elo server`,
//! the method, the path without `/api`, `<slug>/<key id>`, unix seconds, a
//! nonce and the body's SHA-256 in hex, one a line; the signature in the
//! `X-Elo-Signature` header, standard base64.

use std::time::Duration;

use base64::Engine;
use ring::signature::Ed25519KeyPair;
use sha2::{Digest, Sha256};

/// PKCS#8 v1 for an Ed25519 key is this prefix and the 32-byte seed
/// (`openssl genpkey -algorithm ed25519`).
const PKCS8_ED25519: [u8; 16] = [
    0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20,
];

pub struct ServerKey {
    pub slug: String,
    pub id: String,
    pair: Ed25519KeyPair,
}

impl ServerKey {
    pub fn from_seed(slug: &str, id: &str, seed: &[u8]) -> Result<ServerKey, String> {
        Ok(ServerKey {
            slug: slug.into(),
            id: id.into(),
            pair: Ed25519KeyPair::from_seed_unchecked(seed).map_err(|_| "not an Ed25519 seed")?,
        })
    }

    /// A PKCS#8 PEM, as `openssl genpkey -algorithm ed25519` writes it.
    pub fn from_pem(slug: &str, id: &str, pem: &str) -> Result<ServerKey, String> {
        let b64: String = pem.lines().filter(|l| !l.starts_with("-----")).collect();
        let der = base64::engine::general_purpose::STANDARD
            .decode(b64.trim())
            .map_err(|_| "the key file isn't PEM")?;
        if der.len() < 48 || der[..16] != PKCS8_ED25519 {
            return Err("the key file isn't an Ed25519 private key".into());
        }
        ServerKey::from_seed(slug, id, &der[16..48])
    }

    pub fn public(&self) -> [u8; 32] {
        use ring::signature::KeyPair;
        self.pair.public_key().as_ref().try_into().unwrap_or([0; 32])
    }

    /// The headers for one request. `path` is without `/api`.
    pub fn headers(&self, method: &str, path: &str, body: &[u8], at: u64, nonce: &str) -> Vec<(&'static str, String)> {
        let who = format!("{}/{}", self.slug, self.id);
        let text = server_text(method, path, &who, at, body, nonce);
        let sig = self.pair.sign(text.as_bytes());
        vec![
            ("X-Elo-Server", who),
            ("X-Elo-Time", at.to_string()),
            ("X-Elo-Nonce", nonce.to_string()),
            (
                "X-Elo-Signature",
                base64::engine::general_purpose::STANDARD.encode(sig.as_ref()),
            ),
        ]
    }
}

pub fn sha256_hex(b: &[u8]) -> String {
    crate::hex(&Sha256::digest(b))
}

/// The text a server signs (Elo Pros' `serverkeys.message`).
pub fn server_text(method: &str, path: &str, who: &str, at: u64, body: &[u8], nonce: &str) -> String {
    format!(
        "elo server\n{}\n{path}\n{who}\n{at}\n{nonce}\n{}",
        method.to_ascii_uppercase(),
        sha256_hex(body)
    )
}

fn new_nonce() -> String {
    let mut b = [0u8; 16];
    let _ = getrandom::fill(&mut b);
    crate::hex(&b)
}

/// One match's result: each side's wallets and place (1 best; a draw shares
/// a place).
#[derive(Clone, Debug)]
pub struct Report {
    pub id: String,
    pub ladder: String,
    pub sides: Vec<(Vec<String>, u32)>,
    /// What the replay hashes to and the salt's commitment: kept beside the
    /// report until Elo Pros' ratings take an `evidence` key.
    pub replay_sha256: String,
    pub salt_commit: String,
}

pub fn report_body(r: &Report) -> String {
    let teams: Vec<serde_json::Value> = r
        .sides
        .iter()
        .map(|(players, place)| serde_json::json!({"players": players, "place": place}))
        .collect();
    serde_json::json!({"matches": [{"id": r.id, "ladder": r.ladder, "teams": teams}]}).to_string()
}

pub struct Elo {
    pub origin: String,
    pub slug: String,
    pub key: Option<ServerKey>,
    http: reqwest::Client,
}

impl Elo {
    pub fn new(origin: &str, slug: &str, key: Option<ServerKey>) -> Elo {
        Elo {
            origin: origin.trim_end_matches('/').into(),
            slug: slug.into(),
            key,
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
        }
    }

    /// A wallet's rating on a ladder; 1500 (a new player's) if it has none
    /// or Elo Pros can't be reached.
    pub async fn rating(&self, wallet: &str, ladder: &str) -> f64 {
        let url = format!("{}/api/ratings/{}/of/{}", self.origin, self.slug, wallet);
        let got = async {
            let text = self.http.get(&url).send().await.ok()?.text().await.ok()?;
            let v: serde_json::Value = serde_json::from_str(&text).ok()?;
            v["ladders"][ladder]["rating"].as_f64()
        };
        got.await.unwrap_or(1500.0)
    }

    /// Posts a result, retrying what may be passing (5xx, 429).
    pub async fn report(&self, r: &Report) -> Result<(), String> {
        let key = self.key.as_ref().ok_or("no server key: nothing is reported")?;
        let body = report_body(r);
        let path = format!("/ratings/{}/report", self.slug);
        let mut wait = Duration::from_secs(1);
        for _ in 0..5 {
            let mut req = self
                .http
                .post(format!("{}/api{}", self.origin, path))
                .header("Content-Type", "application/json")
                .body(body.clone());
            for (k, v) in key.headers("POST", &path, body.as_bytes(), crate::auth::now_unix(), &new_nonce()) {
                req = req.header(k, v);
            }
            match req.send().await {
                Ok(res) if res.status().is_success() => return Ok(()),
                Ok(res) if res.status().is_server_error() || res.status().as_u16() == 429 => {}
                Ok(res) => {
                    let code = res.status();
                    return Err(format!("{code}: {}", res.text().await.unwrap_or_default()));
                }
                Err(_) => {}
            }
            tokio::time::sleep(wait).await;
            wait *= 2;
        }
        Err("Elo Pros didn't take the report after five tries".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_text_is_the_sdk_s() {
        // Elo Pros' sdk/testdata/server_texts.json shape: seven lines, no
        // trailing newline, the body's hash last.
        let t = server_text(
            "post",
            "/ratings/oneshot/report",
            "oneshot/eu-1",
            1700000000,
            b"{}",
            "abcdefgh",
        );
        let lines: Vec<&str> = t.split('\n').collect();
        assert_eq!(lines.len(), 7);
        assert_eq!(lines[0], "elo server");
        assert_eq!(lines[1], "POST");
        assert_eq!(lines[6], sha256_hex(b"{}"));
    }

    #[test]
    fn a_pem_key_signs_and_verifies() {
        let seed = [7u8; 32];
        let mut der = PKCS8_ED25519.to_vec();
        der.extend_from_slice(&seed);
        let pem = format!(
            "-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----\n",
            base64::engine::general_purpose::STANDARD.encode(&der)
        );
        let k = ServerKey::from_pem("oneshot", "eu-1", &pem).unwrap();
        let h = k.headers("POST", "/ratings/oneshot/report", b"x", 5, "nonce123");
        let sig = base64::engine::general_purpose::STANDARD.decode(&h[3].1).unwrap();
        let text = server_text("POST", "/ratings/oneshot/report", "oneshot/eu-1", 5, b"x", "nonce123");
        ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, k.public())
            .verify(text.as_bytes(), &sig)
            .unwrap();
    }
}

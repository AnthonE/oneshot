//! Elo Pros: ratings in, results out (docs/DESIGN.md §11).
//!
//! A result is `POST /api/ratings/<slug>/report`, signed with this server's
//! Ed25519 key the way Elo Pros' `meter/serverkeys.py` checks it, and as its
//! Rust SDK (`sdk/rust/src/server.rs`) builds it: the text `elo server`,
//! the method, the path without `/api`, `<slug>/<key id>`, unix seconds, a
//! nonce and the body's SHA-256 in hex, one a line; the signature in the
//! `X-Elo-Signature` header, standard base64.
//!
//! Bans are the game's own list on Elo Pros (`meter/gamebans.py`): written
//! with the same signature by `oneshot-review`, read by anyone.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

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
    /// What the replay hashes to and the salt's commitment, sent as the
    /// match's `evidence`, kept in Elo Pros' hash-chained match log.
    pub replay_sha256: String,
    pub salt_commit: String,
}

pub fn report_body(r: &Report, evidence: bool) -> String {
    let teams: Vec<serde_json::Value> = r
        .sides
        .iter()
        .map(|(players, place)| serde_json::json!({"players": players, "place": place}))
        .collect();
    // A ranked room seats people only (no bots, no agents' keys): saying so
    // puts every player on the people's side of the boards.
    let mut m = serde_json::json!({"id": r.id, "ladder": r.ladder, "teams": teams, "agents": []});
    if evidence {
        m["evidence"] = serde_json::json!({"replay_sha256": r.replay_sha256, "salt_commit": r.salt_commit});
    }
    serde_json::json!({ "matches": [m] }).to_string()
}

/// A ban or a lift, as `POST /api/bans/<slug>` takes it.
pub fn ban_body(wallet: &str, banned: bool, reason: &str, match_id: Option<&str>) -> String {
    let mut b = serde_json::json!({
        "wallet": wallet,
        "status": if banned { "banned" } else { "lifted" },
        "reason": reason,
    });
    if let Some(m) = match_id {
        b["match"] = m.into();
    }
    b.to_string()
}

/// The skin names in an items answer that are skins of this game (`slug`).
///
/// Elo Pros answers two shapes. The EVM collection
/// (`GET /api/items/of/<wallet>?game=<slug>&title=<slug>`, `meter/items.py`)
/// lists tokens already filtered to the title, each named by its catalog
/// row's `catalog_name`. The Solana one (`GET /api/items/sol/of/<wallet>`,
/// `meter/solana_items.py`) lists every title's items, each with its
/// `title`, its `item` key and its `name`. A row of another title is
/// dropped; a name counts if it, or its first word, is one of
/// `proto::SKINS`.
pub fn owned_skins(v: &serde_json::Value, slug: &str) -> Vec<String> {
    let rows = v["items"].as_array().into_iter().flatten();
    let names = rows.filter_map(|i| {
        if let Some(t) = i["title"].as_str() {
            if !t.eq_ignore_ascii_case(slug) {
                return None;
            }
        }
        i.as_str()
            .or_else(|| i["skin"].as_str())
            .or_else(|| i["item"].as_str())
            .or_else(|| i["catalog_name"].as_str())
            .or_else(|| i["name"].as_str())
            .map(|s| s.trim().to_ascii_lowercase())
    });
    let mut out: Vec<String> = names
        .filter_map(|n| {
            let first = n.split(|c: char| !c.is_ascii_alphanumeric()).next().unwrap_or("");
            [n.as_str(), first]
                .into_iter()
                .find(|c| proto::SKINS.contains(c))
                .map(str::to_string)
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

/// Where Elo Pros lists `wallet`'s items of game `slug`. A `0x` wallet's
/// are in two places: the EVM collection, and Solana at the wallet linked to
/// it (where the free Pill is sent; Elo Pros reads the link itself). A
/// base58 wallet's are on Solana.
pub fn items_urls(origin: &str, slug: &str, wallet: &str) -> Vec<String> {
    let sol = format!("{origin}/api/items/sol/of/{wallet}");
    if wallet.starts_with("0x") || wallet.starts_with("0X") {
        vec![format!("{origin}/api/items/of/{wallet}?game={slug}&title={slug}"), sol]
    } else {
        vec![sol]
    }
}

/// How long a ban answer is kept before it is asked again.
pub const BAN_CACHE: Duration = Duration::from_secs(60);
/// How long a friends list is kept: a friend added on Elo Pros shows up
/// within this.
pub const FRIENDS_CACHE: Duration = Duration::from_secs(30);

/// One friend as Elo Pros lists them (`GET /api/friends/<player>`,
/// `meter/friends.py`): their player id, the name they go by, and whether
/// Elo Pros sees them online anywhere.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FriendRow {
    pub id: String,
    pub name: String,
    pub online: bool,
}

/// The rows of a friends answer, in its order. A friend with no name goes
/// by their id, shortened as Elo Pros shows one (`0x1234…abcd`).
pub fn friend_rows(v: &serde_json::Value) -> Vec<FriendRow> {
    v["friends"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|f| {
            let id = f["wallet"].as_str()?.trim().to_ascii_lowercase();
            proto::party::PlayerId::new(&id).filter(|p| !p.is_empty())?;
            let name = match f["name"].as_str().map(str::trim) {
                Some(n) if !n.is_empty() => n.to_string(),
                _ if id.len() > 12 => format!("{}..{}", &id[..6], &id[id.len() - 4..]),
                _ => id.clone(),
            };
            Some(FriendRow {
                id,
                name,
                online: f["online"].as_bool().unwrap_or(false),
            })
        })
        .collect()
}

pub struct Elo {
    pub origin: String,
    pub slug: String,
    pub key: Option<ServerKey>,
    http: reqwest::Client,
    bans: Mutex<HashMap<String, (Instant, bool)>>,
    skins: Mutex<HashMap<String, (Instant, Vec<String>)>>,
    friends: Mutex<HashMap<String, (Instant, Vec<FriendRow>)>>,
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
            bans: Mutex::new(HashMap::new()),
            skins: Mutex::new(HashMap::new()),
            friends: Mutex::new(HashMap::new()),
        }
    }

    /// `player`'s friends on Elo Pros (their list is public), or None if
    /// Elo Pros can't be asked. `fresh`: ask again even if a list is kept.
    /// Answers are kept [`FRIENDS_CACHE`].
    pub async fn friends(&self, player: &str, fresh: bool) -> Option<Vec<FriendRow>> {
        let key = player.to_ascii_lowercase();
        if !fresh {
            if let Ok(m) = self.friends.lock() {
                if let Some((at, f)) = m.get(&key) {
                    if at.elapsed() < FRIENDS_CACHE {
                        return Some(f.clone());
                    }
                }
            }
        }
        let url = format!("{}/api/friends/{}", self.origin, key);
        let res = self.http.get(&url).send().await.ok()?;
        if !res.status().is_success() {
            return None;
        }
        let v: serde_json::Value = serde_json::from_str(&res.text().await.ok()?).ok()?;
        let got = friend_rows(&v);
        if let Ok(mut m) = self.friends.lock() {
            if m.len() > 10_000 {
                m.retain(|_, (at, _)| at.elapsed() < FRIENDS_CACHE);
            }
            m.insert(key, (Instant::now(), got.clone()));
        }
        Some(got)
    }

    /// The skins `wallet` owns, by name (`proto::SKINS`): the game's items
    /// on Elo Pros ([`items_urls`], read by [`owned_skins`]). Empty if Elo
    /// Pros can't be asked: a skin is cosmetic, so not knowing costs the
    /// look, never the game. Answers are kept [`BAN_CACHE`].
    pub async fn skins(&self, wallet: &str) -> Vec<String> {
        let key = wallet.to_ascii_lowercase();
        if let Ok(m) = self.skins.lock() {
            if let Some((at, s)) = m.get(&key) {
                if at.elapsed() < BAN_CACHE {
                    return s.clone();
                }
            }
        }
        let one = |url: String| async move {
            let res = self.http.get(&url).send().await.ok()?;
            if !res.status().is_success() {
                return None;
            }
            let v: serde_json::Value = serde_json::from_str(&res.text().await.ok()?).ok()?;
            Some(owned_skins(&v, &self.slug))
        };
        let mut lists = Vec::new();
        for url in items_urls(&self.origin, &self.slug, wallet) {
            lists.push(one(url).await);
        }
        // What any list answered; none answering is "couldn't ask", not kept.
        if lists.iter().all(Option::is_none) {
            return Vec::new();
        }
        let mut got: Vec<String> = lists.into_iter().flatten().flatten().collect();
        got.sort();
        got.dedup();
        if let Ok(mut m) = self.skins.lock() {
            if m.len() > 10_000 {
                m.retain(|_, (at, _)| at.elapsed() < BAN_CACHE);
            }
            m.insert(key, (Instant::now(), got.clone()));
        }
        got
    }

    /// Whether the game has banned `wallet`: None if Elo Pros couldn't be
    /// asked, which never refuses anyone. Answers are kept [`BAN_CACHE`].
    pub async fn banned(&self, wallet: &str) -> Option<bool> {
        let key = wallet.to_ascii_lowercase();
        if let Ok(m) = self.bans.lock() {
            if let Some(&(at, b)) = m.get(&key) {
                if at.elapsed() < BAN_CACHE {
                    return Some(b);
                }
            }
        }
        let url = format!("{}/api/bans/{}/of/{}", self.origin, self.slug, wallet);
        let got = async {
            let res = self.http.get(&url).send().await.ok()?;
            if !res.status().is_success() {
                return None;
            }
            let v: serde_json::Value = serde_json::from_str(&res.text().await.ok()?).ok()?;
            v["banned"].as_bool()
        }
        .await?;
        if let Ok(mut m) = self.bans.lock() {
            if m.len() > 10_000 {
                m.retain(|_, (at, _)| at.elapsed() < BAN_CACHE);
            }
            m.insert(key, (Instant::now(), got));
        }
        Some(got)
    }

    /// Bans or lifts a wallet on the game's list. A person's call, made on a
    /// reviewed replay (`oneshot-review`).
    pub async fn ban(
        &self,
        wallet: &str,
        banned: bool,
        reason: &str,
        match_id: Option<&str>,
    ) -> Result<String, String> {
        let key = self.key.as_ref().ok_or("no server key: a ban is signed with it")?;
        let body = ban_body(wallet, banned, reason, match_id);
        let path = format!("/bans/{}", self.slug);
        let mut req = self
            .http
            .post(format!("{}/api{}", self.origin, path))
            .header("Content-Type", "application/json")
            .body(body.clone());
        for (k, v) in key.headers("POST", &path, body.as_bytes(), crate::auth::now_unix(), &new_nonce()) {
            req = req.header(k, v);
        }
        let res = req
            .send()
            .await
            .map_err(|e| format!("Elo Pros couldn't be reached: {e}"))?;
        let code = res.status();
        let text = res.text().await.unwrap_or_default();
        if code.is_success() {
            Ok(text)
        } else {
            Err(format!("{code}: {text}"))
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
        let path = format!("/ratings/{}/report", self.slug);
        match self.post_signed(&path, report_body(r, true)).await {
            // An Elo Pros from before ratings took `evidence`: the result
            // still counts, so send it without.
            Err(e) if e.starts_with("422") && e.contains("evidence") => {
                self.post_signed(&path, report_body(r, false)).await.map(|_| ())
            }
            other => other.map(|_| ()),
        }
    }

    /// Posts what players did (`stats::report_body`) to the game's stats,
    /// achievements and boards. The answer names what it unlocked.
    pub async fn stats(&self, body: String) -> Result<String, String> {
        self.post_signed(&format!("/gamestats/{}/report", self.slug), body)
            .await
    }

    /// A POST signed with the server key, retrying what may be passing
    /// (5xx, 429, no answer) five times, a second then doubling. `path` is
    /// without `/api`. Another refusal comes back as `"<code>: <text>"`.
    async fn post_signed(&self, path: &str, body: String) -> Result<String, String> {
        let key = self.key.as_ref().ok_or("no server key: nothing is reported")?;
        let mut wait = Duration::from_secs(1);
        for _ in 0..5 {
            let mut req = self
                .http
                .post(format!("{}/api{}", self.origin, path))
                .header("Content-Type", "application/json")
                .body(body.clone());
            for (k, v) in key.headers("POST", path, body.as_bytes(), crate::auth::now_unix(), &new_nonce()) {
                req = req.header(k, v);
            }
            match req.send().await {
                Ok(res) if res.status().is_success() => return Ok(res.text().await.unwrap_or_default()),
                Ok(res) if res.status().is_server_error() || res.status().as_u16() == 429 => {}
                Ok(res) => {
                    let code = res.status();
                    let text = res.text().await.unwrap_or_default();
                    return Err(format!("{code}: {text}"));
                }
                Err(_) => {}
            }
            tokio::time::sleep(wait).await;
            wait *= 2;
        }
        Err(format!("Elo Pros didn't take {path} after five tries"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn friends_are_read_as_elo_pros_lists_them() {
        // `meter/friends.py` `view_of`: a wallet friend with a name, an
        // account friend without one, and rows that aren't ids at all.
        let v = serde_json::json!({"wallet": "0xaa", "friends": [
            {"wallet": "0x24445EFDDB08D4938E3E3627042B2CF4063D9092", "name": "Chief", "online": true,
             "playing": {"game": "gates"}},
            {"wallet": "acct_0123456789abcdef0123", "name": null, "online": false},
            {"wallet": "not an id!"}, {"name": "no id"}
        ]});
        let rows = friend_rows(&v);
        assert_eq!(rows.len(), 2);
        assert_eq!(
            rows[0],
            FriendRow {
                id: "0x24445efddb08d4938e3e3627042b2cf4063d9092".into(),
                name: "Chief".into(),
                online: true
            }
        );
        assert_eq!((rows[1].name.as_str(), rows[1].online), ("acct_0..0123", false));
        assert!(friend_rows(&serde_json::json!({"error": "x"})).is_empty());
    }

    #[test]
    fn skins_are_read_from_names_or_objects_and_unknown_ones_dropped() {
        let v = serde_json::json!({"items": ["Ember", {"skin": "gilded"}, {"name": "pill"}, {"name": "a hat"}, 7]});
        assert_eq!(owned_skins(&v, "oneshot"), vec!["ember", "gilded", "pill"]);
        assert!(owned_skins(&serde_json::json!({}), "oneshot").is_empty());
    }

    #[test]
    fn skins_are_read_from_both_of_elo_pros_item_lists() {
        // `meter/items.py`: tokens of the game's EloItem, named by catalog.
        let evm = serde_json::json!({"wallet": "0xab", "items": [
            {"token_id": 4, "catalog_id": 101, "catalog_name": "Glacier"},
            {"token_id": 5, "catalog_id": 102, "catalog_name": "Ember Trim"},
            {"token_id": 6, "derived": null, "derived_error": "no schema"},
        ]});
        assert_eq!(owned_skins(&evm, "oneshot"), vec!["ember", "glacier"]);
        // `meter/solana_items.py`: every title's items; only ours count.
        let sol = serde_json::json!({"items": [
            {"title": "oneshot", "item": "verdant", "name": "Verdant"},
            {"title": "gates", "item": "ember", "name": "Ember Bat"},
        ]});
        assert_eq!(owned_skins(&sol, "oneshot"), vec!["verdant"]);
    }

    #[test]
    fn a_wallet_is_asked_where_its_chain_keeps_items() {
        let o = "https://elopros.com";
        // An EVM wallet: its RH items, and its linked Solana wallet's.
        assert_eq!(
            items_urls(o, "oneshot", "0xAbC"),
            vec![
                "https://elopros.com/api/items/of/0xAbC?game=oneshot&title=oneshot".to_string(),
                "https://elopros.com/api/items/sol/of/0xAbC".to_string(),
            ]
        );
        assert_eq!(
            items_urls(o, "oneshot", "7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU"),
            vec!["https://elopros.com/api/items/sol/of/7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU".to_string()]
        );
    }

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

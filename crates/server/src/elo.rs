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

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
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
    getrandom::fill(&mut b).expect("the OS gave no randomness for a nonce");
    crate::hex(&b)
}

/// Tries a signed POST gets ([`Elo::post_signed`]), and how long one may
/// take.
pub const TRIES: u32 = 5;
pub const POST_TIMEOUT: Duration = Duration::from_secs(10);
/// The longest a signed POST takes before it gives up: every try timing
/// out, and the waits between them (1, 2, 4, 8 s).
pub const RETRY_BUDGET: Duration = Duration::from_secs(TRIES as u64 * POST_TIMEOUT.as_secs() + (1 << (TRIES - 1)) - 1);
/// A lookup (a ban, a floor, a skin list, friends, a rating) waits this
/// long, and Elo Pros failing one (no answer, a 5xx) is taken as down for
/// [`DOWN_FOR`]: lookups then answer "couldn't ask" at once, so a hello
/// that asks several never sits at "Connecting..." for a minute.
pub const GET_TIMEOUT: Duration = Duration::from_secs(3);
pub const DOWN_FOR: Duration = Duration::from_secs(30);
/// Where, under the replays, a report Elo Pros didn't take is kept until
/// `oneshot-review resend` sends it again.
pub const UNREPORTED: &str = "unreported";
/// Answers a lookup cache holds at most: past it, what has aged out goes,
/// and a still-full cache takes no more (a flood of fresh wallets, each
/// its own sign-in, fills no memory).
pub const CACHE_CAP: usize = 10_000;

/// Keeps `v` under `key` in a lookup cache, within [`CACHE_CAP`].
fn keep<V>(m: &mut HashMap<String, (Instant, V)>, key: String, v: V, ttl: Duration) {
    if m.len() >= CACHE_CAP && !m.contains_key(&key) {
        m.retain(|_, (at, _)| at.elapsed() < ttl);
        if m.len() >= CACHE_CAP {
            return;
        }
    }
    m.insert(key, (Instant::now(), v));
}

/// A signed POST Elo Pros didn't take: where it went (without `/api`) and
/// its body. It is signed afresh when it is sent again: a signature is
/// only good for a few minutes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unsent {
    pub path: String,
    pub body: String,
}

impl Unsent {
    pub fn to_json(&self) -> Vec<u8> {
        serde_json::to_vec_pretty(&serde_json::json!({"path": self.path, "body": self.body})).unwrap_or_default()
    }

    pub fn from_json(b: &[u8]) -> Option<Unsent> {
        let v: serde_json::Value = serde_json::from_slice(b).ok()?;
        Some(Unsent {
            path: v["path"].as_str()?.to_string(),
            body: v["body"].as_str()?.to_string(),
        })
    }

    /// Its file's name: what it is (`ratings`, `gamestats`) and the match
    /// it reports, else its body's hash, so the same one saved twice is
    /// one file.
    pub fn file_name(&self) -> String {
        let kind = self.path.split('/').find(|p| !p.is_empty()).unwrap_or("post");
        let id = serde_json::from_str::<serde_json::Value>(&self.body)
            .ok()
            .and_then(|v| v["matches"][0]["id"].as_str().map(str::to_string))
            .filter(|_| kind == "ratings")
            .unwrap_or_else(|| sha256_hex(self.body.as_bytes())[..16].to_string());
        let clean = |s: &str| -> String {
            s.chars()
                .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
                .take(80)
                .collect()
        };
        format!("{}-{}.json", clean(kind), clean(&id))
    }

    /// Writes it whole into `dir`; returns where.
    pub fn save(&self, dir: &Path) -> std::io::Result<PathBuf> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join(self.file_name());
        let tmp = path.with_extension("part");
        std::fs::write(&tmp, self.to_json())?;
        std::fs::rename(&tmp, &path)?;
        Ok(path)
    }
}

/// How one signed POST went.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Posted {
    /// Taken: the answer.
    Taken(String),
    /// Refused for good (a 4xx but 429): `"<code>: <text>"`.
    Refused(String),
    /// Not answered, or an error that may pass (5xx, 429).
    Passing(String),
}

/// A match from Elo Pros' queue, as the server seats it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueueMatch {
    pub id: String,
    pub game: String,
    pub ladder: String,
    pub teams: usize,
    pub size: usize,
    /// The server it was launched onto, `host:port` as the game's list has it.
    pub addr: String,
    /// Each side's wallets, as Elo Pros spells them.
    pub sides: Vec<Vec<String>>,
}

impl QueueMatch {
    /// The side `wallet` plays on, if any (EVM wallets in any case).
    pub fn side_of(&self, wallet: &str) -> Option<usize> {
        let same = |w: &str| {
            if w.starts_with("0x") {
                w.eq_ignore_ascii_case(wallet)
            } else {
                w == wallet
            }
        };
        self.sides.iter().position(|s| s.iter().any(|w| same(w)))
    }

    /// Players it seats.
    pub fn players(&self) -> usize {
        self.sides.iter().map(Vec::len).sum()
    }
}

/// Where a player stands in Elo Pros' queue, as its game's server is told.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QueueState {
    pub player: String,
    /// queued, launched (matched), open or closed (out of the queue), or
    /// None: in no lobby of this game.
    pub state: Option<String>,
    pub match_id: Option<String>,
    /// The server its match was launched onto.
    pub server: Option<String>,
    pub waited_s: Option<u64>,
    pub unqueued_why: Option<String>,
}

pub fn parse_queue_states(text: &str) -> Result<Vec<QueueState>, String> {
    let v: serde_json::Value = serde_json::from_str(text).map_err(|e| format!("a queue answer: {e}"))?;
    let s = |x: &serde_json::Value, k: &str| x[k].as_str().map(str::to_string);
    Ok(v["players"]
        .as_array()
        .ok_or("a queue answer names no players")?
        .iter()
        .map(|p| QueueState {
            player: s(p, "player").unwrap_or_default(),
            state: s(p, "state"),
            match_id: s(p, "match"),
            server: s(p, "server"),
            waited_s: p["waited_s"].as_u64(),
            unqueued_why: s(p, "unqueued_why"),
        })
        .collect())
}

/// `GET /api/lobbies/match/{id}`'s body (`meter/lobbies.py` `_match`).
pub fn parse_queue_match(text: &str) -> Result<QueueMatch, String> {
    let v: serde_json::Value = serde_json::from_str(text).map_err(|e| format!("a queue match: {e}"))?;
    let s = |k: &str| {
        v[k].as_str()
            .map(str::to_string)
            .ok_or(format!("a queue match has no {k}"))
    };
    let n = |k: &str| {
        v[k].as_u64()
            .map(|x| x as usize)
            .ok_or(format!("a queue match has no {k}"))
    };
    let sides: Vec<Vec<String>> = v["sides"]
        .as_array()
        .ok_or("a queue match has no sides")?
        .iter()
        .map(|side| {
            side["players"]
                .as_array()
                .map(|ps| ps.iter().filter_map(|p| p.as_str().map(str::to_string)).collect())
                .unwrap_or_default()
        })
        .collect();
    Ok(QueueMatch {
        id: s("id")?,
        game: s("game")?,
        ladder: s("ladder")?,
        teams: n("teams")?,
        size: n("size")?,
        addr: v["server"]["addr"].as_str().unwrap_or("").to_string(),
        sides,
    })
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

/// The names in an items answer that are skins or gear of this game
/// (`slug`).
///
/// Elo Pros answers two shapes. The EVM collection
/// (`GET /api/items/of/<wallet>?game=<slug>&title=<slug>`, `meter/items.py`)
/// lists tokens already filtered to the title, each named by its catalog
/// row's `catalog_name`. The Solana one (`GET /api/items/sol/of/<wallet>`,
/// `meter/solana_items.py`) lists every title's items, each with its
/// `title`, its `item` key and its `name`. A row of another title is
/// dropped; a name counts if it, or its first word, is one of
/// `proto::SKINS`, `proto::HEADWEAR` or `proto::FOOTWEAR` (but "none").
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
                .find(|c| {
                    proto::SKINS.contains(c) || proto::HEADWEAR[1..].contains(c) || proto::FOOTWEAR[1..].contains(c)
                })
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

/// A wallet's best floor across the game's ladders in an Elo Pros ratings
/// answer (`GET /api/ratings/<slug>/of/<wallet>`): what a rank-locked skin
/// reads (`proto::SKIN_FLOOR`). None if it was never rated.
pub fn best_floor(v: &serde_json::Value) -> Option<f64> {
    v["ladders"]
        .as_object()?
        .values()
        .filter_map(|l| l["floor"].as_f64())
        .reduce(f64::max)
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
            let id = proto::party::spell(f["wallet"].as_str()?.trim());
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
    floors: Mutex<HashMap<String, (Instant, Option<f64>)>>,
    friends: Mutex<HashMap<String, (Instant, Vec<FriendRow>)>>,
    /// Where a signed POST that never got through is saved ([`UNREPORTED`]).
    unreported: Option<PathBuf>,
    /// The signed POSTs still being tried, so a drain that can't wait for
    /// them saves them ([`Elo::save_pending`]).
    pending: Mutex<BTreeMap<u64, Unsent>>,
    next_pending: AtomicU64,
    /// Until when lookups take Elo Pros as down ([`DOWN_FOR`]).
    down_until: Mutex<Option<Instant>>,
    /// What became of the signed POSTs, for `/health.json` ([`Elo::counts`]).
    sent: AtomicU64,
    refused: AtomicU64,
    failed: AtomicU64,
    unkeyed: AtomicU64,
}

/// What became of the signed POSTs (results and stats) since start: taken,
/// refused (a 4xx Elo Pros meant), never got through after every try, not
/// sent for want of a server key; and how many are being tried now.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub sent: u64,
    pub refused: u64,
    pub failed: u64,
    pub unkeyed: u64,
    pub held: u64,
}

impl Elo {
    pub fn new(origin: &str, slug: &str, key: Option<ServerKey>) -> Elo {
        Elo {
            origin: origin.trim_end_matches('/').into(),
            slug: slug.into(),
            key,
            http: reqwest::Client::builder()
                .timeout(POST_TIMEOUT)
                .build()
                .unwrap_or_default(),
            bans: Mutex::new(HashMap::new()),
            skins: Mutex::new(HashMap::new()),
            floors: Mutex::new(HashMap::new()),
            friends: Mutex::new(HashMap::new()),
            unreported: None,
            pending: Mutex::new(BTreeMap::new()),
            next_pending: AtomicU64::new(0),
            down_until: Mutex::new(None),
            sent: AtomicU64::new(0),
            refused: AtomicU64::new(0),
            failed: AtomicU64::new(0),
            unkeyed: AtomicU64::new(0),
        }
    }

    /// [`Counts`], read now. Never waits on the network: `held` is the
    /// pending map's length, under a lock no POST holds across an await.
    pub fn counts(&self) -> Counts {
        let g = |a: &AtomicU64| a.load(Ordering::Relaxed);
        Counts {
            sent: g(&self.sent),
            refused: g(&self.refused),
            failed: g(&self.failed),
            unkeyed: g(&self.unkeyed),
            held: self.pending().len() as u64,
        }
    }

    /// A lookup's GET: None at once while Elo Pros is taken as down, or if
    /// it fails (which takes it as down for [`DOWN_FOR`], said once).
    async fn get(&self, url: &str) -> Option<reqwest::Response> {
        let down = self
            .down_until
            .lock()
            .ok()
            .is_some_and(|g| g.is_some_and(|t| Instant::now() < t));
        if down {
            return None;
        }
        let res = self.http.get(url).timeout(GET_TIMEOUT).send().await;
        let failed = match &res {
            Ok(r) => r.status().is_server_error(),
            Err(_) => true,
        };
        if failed {
            if let Ok(mut g) = self.down_until.lock() {
                if g.is_none_or(|t| Instant::now() >= t) {
                    eprintln!(
                        "oneshot: Elo Pros didn't answer a lookup; taking it as down for {} s",
                        DOWN_FOR.as_secs()
                    );
                }
                *g = Some(Instant::now() + DOWN_FOR);
            }
            return None;
        }
        res.ok()
    }

    /// The same, saving a report or stats it couldn't send into `dir`
    /// (`<replays>/unreported`), for `oneshot-review resend`.
    pub fn saving_unsent_to(mut self, dir: Option<PathBuf>) -> Elo {
        self.unreported = dir;
        self
    }

    fn pending(&self) -> std::sync::MutexGuard<'_, BTreeMap<u64, Unsent>> {
        self.pending.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Saves every signed POST still being tried (a drain that can't wait
    /// any longer) and forgets them: what was saved, where, and how many
    /// were lost (no directory, or it couldn't be written).
    pub fn save_pending(&self) -> (usize, usize, Option<PathBuf>) {
        let mut p = self.pending();
        let (mut saved, mut lost) = (0, 0);
        for u in std::mem::take(&mut *p).into_values() {
            match self.unreported.as_deref().map(|d| u.save(d)) {
                Some(Ok(_)) => saved += 1,
                Some(Err(e)) => {
                    eprintln!("oneshot: {path} unsaved: {e}", path = u.path);
                    lost += 1;
                }
                None => lost += 1,
            }
        }
        (saved, lost, self.unreported.clone())
    }

    /// Takes back the file [`Elo::save_pending`] wrote for a POST that has
    /// since got through.
    fn forget_saved(&self, path: &str, body: &str) {
        let Some(dir) = self.unreported.as_deref() else {
            return;
        };
        let u = Unsent {
            path: path.to_string(),
            body: body.to_string(),
        };
        let at = dir.join(u.file_name());
        if std::fs::remove_file(&at).is_ok() {
            eprintln!(
                "oneshot: {path} got through after the drain saved it: {} removed",
                at.display()
            );
        }
    }

    /// `wallet`'s best floor on Elo Pros ([`best_floor`]): None if it was
    /// never rated or Elo Pros can't be asked, which wears the standard look.
    /// Answers are kept [`BAN_CACHE`].
    pub async fn floor(&self, wallet: &str) -> Option<f64> {
        let key = proto::party::spell(wallet);
        if let Ok(m) = self.floors.lock() {
            if let Some((at, f)) = m.get(&key) {
                if at.elapsed() < BAN_CACHE {
                    return *f;
                }
            }
        }
        let url = format!("{}/api/ratings/{}/of/{}", self.origin, self.slug, wallet);
        let got = async {
            let res = self.get(&url).await?;
            if !res.status().is_success() {
                return None;
            }
            let v: serde_json::Value = serde_json::from_str(&res.text().await.ok()?).ok()?;
            Some(best_floor(&v))
        }
        .await?;
        if let Ok(mut m) = self.floors.lock() {
            keep(&mut m, key, got, BAN_CACHE);
        }
        got
    }

    /// `player`'s friends on Elo Pros (their list is public), or None if
    /// Elo Pros can't be asked. `fresh`: ask again even if a list is kept.
    /// Answers are kept [`FRIENDS_CACHE`].
    pub async fn friends(&self, player: &str, fresh: bool) -> Option<Vec<FriendRow>> {
        let key = proto::party::spell(player);
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
        let res = self.get(&url).await?;
        if !res.status().is_success() {
            return None;
        }
        let v: serde_json::Value = serde_json::from_str(&res.text().await.ok()?).ok()?;
        let got = friend_rows(&v);
        if let Ok(mut m) = self.friends.lock() {
            keep(&mut m, key, got.clone(), FRIENDS_CACHE);
        }
        Some(got)
    }

    /// The skins and gear `wallet` owns, by name (`proto::SKINS`,
    /// `proto::HEADWEAR`, `proto::FOOTWEAR`): the game's items
    /// on Elo Pros ([`items_urls`], read by [`owned_skins`]). Empty if Elo
    /// Pros can't be asked: a skin is cosmetic, so not knowing costs the
    /// look, never the game. Answers are kept [`BAN_CACHE`].
    pub async fn skins(&self, wallet: &str) -> Vec<String> {
        self.owned(wallet).await.unwrap_or_default()
    }

    /// As [`Elo::skins`], but None when Elo Pros couldn't be asked (the
    /// armory says so, rather than that the wallet owns nothing).
    pub async fn owned(&self, wallet: &str) -> Option<Vec<String>> {
        let key = proto::party::spell(wallet);
        if let Ok(m) = self.skins.lock() {
            if let Some((at, s)) = m.get(&key) {
                if at.elapsed() < BAN_CACHE {
                    return Some(s.clone());
                }
            }
        }
        let one = |url: String| async move {
            let res = self.get(&url).await?;
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
            return None;
        }
        let mut got: Vec<String> = lists.into_iter().flatten().flatten().collect();
        got.sort();
        got.dedup();
        if let Ok(mut m) = self.skins.lock() {
            keep(&mut m, key, got.clone(), BAN_CACHE);
        }
        Some(got)
    }

    /// Whether the game has banned `wallet`: None if Elo Pros couldn't be
    /// asked and never answered for this wallet, which refuses nobody.
    /// Answers are kept [`BAN_CACHE`]; when Elo Pros can't be asked, the
    /// last answer stands however old, so a ban already seen holds
    /// through a hiccup rather than lapsing for whoever retries into it.
    pub async fn banned(&self, wallet: &str) -> Option<bool> {
        let key = proto::party::spell(wallet);
        let kept = self.bans.lock().ok().and_then(|m| m.get(&key).copied());
        if let Some((at, b)) = kept {
            if at.elapsed() < BAN_CACHE {
                return Some(b);
            }
        }
        let url = format!("{}/api/bans/{}/of/{}", self.origin, self.slug, wallet);
        let got = async {
            let res = self.get(&url).await?;
            if !res.status().is_success() {
                return None;
            }
            let v: serde_json::Value = serde_json::from_str(&res.text().await.ok()?).ok()?;
            v["banned"].as_bool()
        }
        .await;
        let Some(got) = got else {
            return kept.map(|(_, b)| b);
        };
        if let Ok(mut m) = self.bans.lock() {
            keep(&mut m, key, got, BAN_CACHE);
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

    /// One call on Elo Pros' queue for this game's own players
    /// (`POST /api/lobbies/serve/{slug}`, signed with the server key):
    /// where each player in it now stands.
    pub async fn serve(&self, body: &serde_json::Value) -> Result<Vec<QueueState>, String> {
        let key = self
            .key
            .as_ref()
            .ok_or("no server key: Elo Pros' queue is called with it")?;
        let body = body.to_string();
        let path = format!("/lobbies/serve/{}", self.slug);
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
        if !code.is_success() {
            return Err(format!("{code}: {text}"));
        }
        parse_queue_states(&text)
    }

    /// A match Elo Pros' queue formed (`GET /api/lobbies/match/{id}`): who
    /// plays it on which side, on which ladder and server.
    pub async fn queue_match(&self, id: &str) -> Result<QueueMatch, String> {
        let url = format!("{}/api/lobbies/match/{}", self.origin, id);
        let res = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("Elo Pros couldn't be reached: {e}"))?;
        let code = res.status();
        let text = res.text().await.unwrap_or_default();
        if !code.is_success() {
            return Err(format!("{code}: {text}"));
        }
        parse_queue_match(&text)
    }

    /// A wallet's rating on a ladder; 1500 (a new player's) if it has none,
    /// and also if Elo Pros can't be asked (said, so a queue pairing a
    /// stranger by it is no mystery).
    pub async fn rating(&self, wallet: &str, ladder: &str) -> f64 {
        let url = format!("{}/api/ratings/{}/of/{}", self.origin, self.slug, wallet);
        let res = self.get(&url).await;
        if res
            .as_ref()
            .is_some_and(|r| r.status() == reqwest::StatusCode::NOT_FOUND)
        {
            return 1500.0;
        }
        let Some(res) = res.filter(|r| r.status().is_success()) else {
            eprintln!("oneshot: {wallet}'s {ladder} rating couldn't be read; queued as 1500");
            return 1500.0;
        };
        let v: Option<serde_json::Value> = async { serde_json::from_str(&res.text().await.ok()?).ok() }.await;
        match v {
            Some(v) => v["ladders"][ladder]["rating"].as_f64().unwrap_or(1500.0),
            None => {
                eprintln!("oneshot: {wallet}'s {ladder} rating came back unreadable; queued as 1500");
                1500.0
            }
        }
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
    /// (5xx, 429, no answer) [`TRIES`] times, a second then doubling. `path`
    /// is without `/api`. Another refusal comes back as `"<code>: <text>"`.
    /// One that never got through is saved for `oneshot-review resend`
    /// (with `saving_unsent_to`), and so is one a drain stops waiting for.
    async fn post_signed(&self, path: &str, body: String) -> Result<String, String> {
        let Some(key) = self.key.as_ref() else {
            self.unkeyed.fetch_add(1, Ordering::Relaxed);
            return Err("no server key: nothing is reported".into());
        };
        let held = self.next_pending.fetch_add(1, Ordering::Relaxed);
        self.pending().insert(
            held,
            Unsent {
                path: path.to_string(),
                body: body.clone(),
            },
        );
        let mut wait = Duration::from_secs(1);
        let mut last = String::new();
        for attempt in 0..TRIES {
            match self.post_once(key, path, &body).await {
                Posted::Taken(text) => {
                    self.sent.fetch_add(1, Ordering::Relaxed);
                    // A drain that stopped waiting saved it meanwhile: it
                    // got through after all, so `resend` mustn't post it
                    // twice.
                    if self.pending().remove(&held).is_none() {
                        self.forget_saved(path, &body);
                    }
                    return Ok(text);
                }
                Posted::Refused(e) => {
                    self.refused.fetch_add(1, Ordering::Relaxed);
                    let u = self.pending().remove(&held);
                    // Refused for who we are (a key rotated, a clock off),
                    // not for what was sent: kept for `resend` once that
                    // is put right, rather than a result lost to stderr.
                    let ours = e.starts_with("401") || e.starts_with("403");
                    if let (true, Some(u), Some(dir)) = (ours, u, self.unreported.as_deref()) {
                        let saved = match u.save(dir) {
                            Ok(at) => format!("saved to {} (oneshot-review resend)", at.display()),
                            Err(e) => format!("not saved: {e}"),
                        };
                        return Err(format!("{e}; {saved}"));
                    }
                    return Err(e);
                }
                Posted::Passing(e) => last = e,
            }
            if attempt + 1 < TRIES {
                tokio::time::sleep(wait).await;
                wait *= 2;
            }
        }
        self.failed.fetch_add(1, Ordering::Relaxed);
        // Saved under the lock a drain saves under: never lost between.
        let mut p = self.pending();
        let saved = match (p.remove(&held), self.unreported.as_deref()) {
            (None, _) => "saved by the drain".to_string(),
            (Some(u), Some(dir)) => match u.save(dir) {
                Ok(at) => format!("saved to {} (oneshot-review resend)", at.display()),
                Err(e) => format!("not saved: {e}"),
            },
            (Some(_), None) => "not saved (no --replays)".to_string(),
        };
        drop(p);
        Err(format!(
            "Elo Pros didn't take {path} after {TRIES} tries ({last}); {saved}"
        ))
    }

    /// One signed POST, no retry.
    async fn post_once(&self, key: &ServerKey, path: &str, body: &str) -> Posted {
        // One key for one body, whatever the try (and a `resend`): a
        // report that got through but whose answer didn't is taken once.
        let mut req = self
            .http
            .post(format!("{}/api{}", self.origin, path))
            .header("Content-Type", "application/json")
            .header("Idempotency-Key", sha256_hex(body.as_bytes()))
            .body(body.to_string());
        for (k, v) in key.headers("POST", path, body.as_bytes(), crate::auth::now_unix(), &new_nonce()) {
            req = req.header(k, v);
        }
        match req.send().await {
            Ok(res) if res.status().is_success() => Posted::Taken(res.text().await.unwrap_or_default()),
            Ok(res) => {
                let code = res.status();
                let text = res.text().await.unwrap_or_default();
                if code.is_server_error() || code.as_u16() == 429 {
                    Posted::Passing(format!("{code}: {text}"))
                } else {
                    Posted::Refused(format!("{code}: {text}"))
                }
            }
            Err(e) => Posted::Passing(format!("no answer: {e}")),
        }
    }

    /// Sends a saved report or stats again, signed now, once.
    pub async fn resend(&self, u: &Unsent) -> Result<Posted, String> {
        let key = self.key.as_ref().ok_or("no server key: a report is signed with it")?;
        Ok(self.post_once(key, &u.path, &u.body).await)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_best_floor_is_read_across_ladders_and_never_rated_is_none() {
        let v = serde_json::json!({"ladders": {
            "duel": {"rated": true, "rating": 1710, "floor": 1612},
            "doubles": {"rated": true, "rating": 1650, "floor": 1580},
            "cup": {"rated": false, "rating": null, "floor": null}}});
        assert_eq!(best_floor(&v), Some(1612.0));
        let none = serde_json::json!({"ladders": {"duel": {"rated": false, "floor": null}}});
        assert_eq!(best_floor(&none), None);
        assert_eq!(best_floor(&serde_json::json!({"error": "x"})), None);
    }

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
        let v = serde_json::json!({"items": ["Ember", {"skin": "gilded"}, {"name": "pill"}, {"name": "a hat"}, "Shades", "Loggers", "none", 7]});
        assert_eq!(
            owned_skins(&v, "oneshot"),
            vec!["ember", "gilded", "loggers", "pill", "shades"]
        );
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
    fn an_unsent_report_is_one_file_a_match_and_reads_back() {
        let r = Report {
            id: "os-00ab".into(),
            ladder: "duel".into(),
            sides: vec![(vec!["0xa".into()], 1), (vec!["0xb".into()], 2)],
            replay_sha256: "00".into(),
            salt_commit: "11".into(),
        };
        let u = Unsent {
            path: "/ratings/oneshot/report".into(),
            body: report_body(&r, true),
        };
        assert_eq!(u.file_name(), "ratings-os-00ab.json");
        assert_eq!(Unsent::from_json(&u.to_json()), Some(u.clone()));
        let s = Unsent {
            path: "/gamestats/oneshot/report".into(),
            body: "{\"reports\":[]}".into(),
        };
        assert!(s.file_name().starts_with("gamestats-") && s.file_name().len() == "gamestats-".len() + 16 + 5);
        let dir = std::env::temp_dir().join(format!("oneshot-unsent-{}", std::process::id()));
        let at = u.save(&dir).unwrap();
        assert_eq!(Unsent::from_json(&std::fs::read(&at).unwrap()), Some(u));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(RETRY_BUDGET, Duration::from_secs(65));
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

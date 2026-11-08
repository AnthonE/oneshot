//! Today's and this week's challenges, as Elo Pros has them
//! (`/api/challenges/oneshot`, its `challenges.py`): our own goals over our
//! own stats (`server/src/stats.rs`, `CHALLENGES`), picked there each UTC day
//! and ISO week, counted from the reports the server already sends, and paid
//! (in our trading cards) by Elo Pros' rules. The title's CHALLENGES tab
//! reads them, with the signed-in player's progress. Beside them, the daily
//! pot (`/api/pot/oneshot`, its `daily_pot.py`): a fixed sum a day split by
//! the challenges each player finished, shown only while Elo Pros says it is
//! on. Nothing here counts or pays anything: it only shows what Elo Pros said.

use std::sync::{Arc, Mutex};

use bevy::prelude::*;
use serde::Deserialize;

/// Where Elo Pros answers.
pub const API: &str = "https://elopros.com/api/challenges/oneshot";
/// The daily pot's answer.
pub const POT_API: &str = "https://elopros.com/api/pot/oneshot";
/// How often the tab asks again while the title is up.
const AGAIN_S: f64 = 90.0;

#[derive(Deserialize, Clone, Debug, Default, PartialEq)]
pub struct One {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub desc: String,
    pub goal: u64,
    /// Cards it pays when done (0: this game pays none).
    #[serde(default)]
    pub cards: u32,
    #[serde(default)]
    pub progress: Option<u64>,
    #[serde(default)]
    pub done: Option<bool>,
}

#[derive(Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Period {
    pub period: String,
    pub ends_at: u64,
    pub challenges: Vec<One>,
}

#[derive(Deserialize, Clone, Debug, Default, PartialEq)]
pub struct View {
    #[serde(default)]
    pub pays: bool,
    #[serde(default)]
    pub rules: String,
    pub day: Period,
    pub week: Period,
}

/// A player's part of today's pot.
#[derive(Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Mine {
    #[serde(default)]
    pub shares: u64,
    #[serde(default)]
    pub earns: u64,
    /// Why this player can't be paid yet (an account with no wallet).
    #[serde(default)]
    pub why_not: Option<String>,
    /// What the pot has sent this player, in all.
    #[serde(default)]
    pub paid_total: u64,
}

/// The daily pot, as Elo Pros says it. Drawn only when `on`.
#[derive(Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Pot {
    #[serde(default)]
    pub on: bool,
    #[serde(default)]
    pub coin: String,
    /// The sentence to show, verbatim.
    #[serde(default)]
    pub say: String,
    #[serde(default)]
    pub mine: Option<Mine>,
}

impl Pot {
    /// What the tab draws: the pot's own sentence, and the player's part.
    /// None unless it is on and has something to say.
    pub fn lines(&self) -> Option<(String, Option<String>)> {
        if !self.on || self.say.trim().is_empty() {
            return None;
        }
        let mine = self.mine.as_ref().map(|m| {
            let s = if m.shares == 1 { "" } else { "S" };
            let today = match &m.why_not {
                Some(why) => why.trim().trim_end_matches('.').to_uppercase(),
                None => format!("{} {}", thousands(m.earns), self.coin),
            };
            let sent = if m.paid_total > 0 {
                format!(" · SENT SO FAR: {} {}", thousands(m.paid_total), self.coin)
            } else {
                String::new()
            };
            format!("YOU: {} SHARE{s} · {today}{sent}", m.shares)
        });
        Some((self.say.clone(), mine))
    }
}

/// 25000 as "25,000".
fn thousands(n: u64) -> String {
    let d = n.to_string();
    let mut out = String::with_capacity(d.len() + d.len() / 3);
    for (i, c) in d.chars().enumerate() {
        if i > 0 && (d.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

type Pending = Arc<Mutex<Option<Result<String, String>>>>;

/// What Elo Pros last said, and the ask in flight.
#[derive(Resource, Default)]
pub struct Challenges {
    pub view: Option<View>,
    pub failed: bool,
    /// Bumped when `view` or `failed` moved, so the title redraws.
    pub gen: u32,
    /// The player last asked for (None: nobody signed in), and when.
    asked: Option<(Option<String>, f64)>,
    pending: Option<Pending>,
    /// The daily pot, while Elo Pros says it is on; None otherwise, and
    /// when it couldn't be read (the tab shows nothing then).
    pub pot: Option<Pot>,
    pot_pending: Option<Pending>,
}

impl Challenges {
    /// Asks Elo Pros for `wallet`'s view (or the plain one), unless the same
    /// ask is fresh or one is in flight.
    pub fn want(&mut self, wallet: Option<String>, now: f64) {
        if self.pending.is_some() {
            return;
        }
        if let Some((w, at)) = &self.asked {
            if *w == wallet && now - at < AGAIN_S {
                return;
            }
        }
        let (url, pot_url) = match &wallet {
            Some(w) => {
                let w = proto::party::spell(w);
                (format!("{API}/{w}"), format!("{POT_API}/{w}"))
            }
            None => (API.to_string(), POT_API.to_string()),
        };
        self.asked = Some((wallet, now));
        let slot: Pending = Arc::default();
        self.pending = Some(slot.clone());
        get(url, slot);
        if self.pot_pending.is_none() {
            let slot: Pending = Arc::default();
            self.pot_pending = Some(slot.clone());
            get(pot_url, slot);
        }
    }

    /// Takes the answers, if they came.
    pub fn poll(&mut self) {
        self.poll_pot();
        let Some(p) = &self.pending else { return };
        let got = p.lock().ok().and_then(|mut g| g.take());
        let Some(got) = got else { return };
        self.pending = None;
        match got.ok().and_then(|t| serde_json::from_str::<View>(&t).ok()) {
            Some(v) => {
                if self.view.as_ref() != Some(&v) || self.failed {
                    self.view = Some(v);
                    self.failed = false;
                    self.gen = self.gen.wrapping_add(1);
                }
            }
            None if !self.failed => {
                self.failed = true;
                self.gen = self.gen.wrapping_add(1);
            }
            None => {}
        }
    }
}

impl Challenges {
    fn poll_pot(&mut self) {
        let Some(p) = &self.pot_pending else { return };
        let got = p.lock().ok().and_then(|mut g| g.take());
        let Some(got) = got else { return };
        self.pot_pending = None;
        // Off, unreadable or unanswered all read as nothing to show.
        let pot = got
            .ok()
            .and_then(|t| serde_json::from_str::<Pot>(&t).ok())
            .filter(|p| p.lines().is_some());
        if pot != self.pot {
            self.pot = pot;
            self.gen = self.gen.wrapping_add(1);
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn get(url: String, slot: Pending) {
    std::thread::spawn(move || {
        let r = crate::door::http_get(&url);
        if let Ok(mut g) = slot.lock() {
            *g = Some(r);
        }
    });
}

#[cfg(target_arch = "wasm32")]
fn get(url: String, slot: Pending) {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;
    wasm_bindgen_futures::spawn_local(async move {
        let r = async {
            let w = web_sys::window().ok_or("no window")?;
            let resp = JsFuture::from(w.fetch_with_str(&url)).await.map_err(|_| "fetch")?;
            let resp: web_sys::Response = resp.dyn_into().map_err(|_| "response")?;
            if resp.status() != 200 {
                return Err("status");
            }
            let text = JsFuture::from(resp.text().map_err(|_| "body")?)
                .await
                .map_err(|_| "body")?;
            text.as_string().ok_or("body")
        }
        .await
        .map_err(|e: &str| e.to_string());
        if let Ok(mut g) = slot.lock() {
            *g = Some(r);
        }
    });
}

/// Unix seconds now, for how long a period has left.
pub fn unix_now() -> u64 {
    #[cfg(target_arch = "wasm32")]
    {
        (js_sys::Date::now() / 1000.0) as u64
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs())
    }
}

/// "12 / 25", "DONE", or the goal alone for a player we don't know.
pub fn progress_words(c: &One) -> String {
    match (c.done, c.progress) {
        (Some(true), _) => "DONE".into(),
        (_, Some(p)) => format!("{p} / {}", c.goal),
        _ => format!("GOAL {}", c.goal),
    }
}

/// How long until `ends_at` (unix seconds), from `now`: "5H", "2D 3H".
pub fn left_words(ends_at: u64, now: u64) -> String {
    let s = ends_at.saturating_sub(now);
    let (d, h, m) = (s / 86_400, s % 86_400 / 3600, s % 3600 / 60);
    match (d, h) {
        (0, 0) => format!("{m}M"),
        (0, _) => format!("{h}H"),
        _ => format!("{d}D {h}H"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elo_pros_view_reads_and_says_progress() {
        let t = r#"{"game":"oneshot","pays":true,"rules":"r","pool":16,
            "day":{"period":"2026-11-05","ends_at":1762387200,"challenges":[
              {"id":"kills_25","name":"Twenty-five down","desc":"","goal":25,"stat":"kills","cards":1,"progress":12,"done":false},
              {"id":"win_1","name":"Take one","goal":1,"stat":"wins","cards":1,"progress":1,"done":true}]},
            "week":{"period":"2026-W45","ends_at":1762732800,"challenges":[]}}"#;
        let v: View = serde_json::from_str(t).unwrap();
        assert!(v.pays);
        assert_eq!(progress_words(&v.day.challenges[0]), "12 / 25");
        assert_eq!(progress_words(&v.day.challenges[1]), "DONE");
        let plain = One {
            goal: 3,
            ..One::default()
        };
        assert_eq!(progress_words(&plain), "GOAL 3");
        assert_eq!(left_words(1000 + 2 * 86_400 + 3 * 3600, 1000), "2D 3H");
        assert_eq!(left_words(1000 + 5 * 3600 + 59, 1000), "5H");
    }

    #[test]
    fn the_pot_shows_only_when_on() {
        let t = r#"{"game":"oneshot","on":true,"why_not":null,"coin":"ONESHOT","per_day":100000,
            "cap_wallet":5000,"day":"2026-11-08","ends_at":1762646400,"shares":40,"wallets":15,
            "per_share":2500,"say":"Finish today's challenges: tomorrow 100,000 ONESHOT is shared out.",
            "wallet":"0xaa","mine":{"shares":2,"played":true,"earns":5000}}"#;
        let p: Pot = serde_json::from_str(t).unwrap();
        let (say, mine) = p.lines().unwrap();
        assert!(say.starts_with("Finish today's"));
        assert_eq!(mine.as_deref(), Some("YOU: 2 SHARES · 5,000 ONESHOT"));
        let paid: Pot = serde_json::from_str(
            r#"{"on":true,"coin":"ONESHOT","say":"s","mine":{"shares":1,"earns":2500,"paid_total":12500}}"#,
        )
        .unwrap();
        assert_eq!(
            paid.lines().unwrap().1.as_deref(),
            Some("YOU: 1 SHARE · 2,500 ONESHOT · SENT SO FAR: 12,500 ONESHOT")
        );
        let acct: Pot = serde_json::from_str(
            r#"{"on":true,"coin":"ONESHOT","say":"s","mine":{"shares":3,"earns":0,"why_not":"Link a wallet to be paid your share."}}"#,
        )
        .unwrap();
        assert_eq!(
            acct.lines().unwrap().1.as_deref(),
            Some("YOU: 3 SHARES · LINK A WALLET TO BE PAID YOUR SHARE")
        );
        assert_eq!(
            (thousands(0), thousands(999), thousands(1000), thousands(100000)),
            ("0".into(), "999".into(), "1,000".into(), "100,000".into())
        );
        let off: Pot = serde_json::from_str(r#"{"on":false,"why_not":"not yet","say":"x"}"#).unwrap();
        assert!(off.lines().is_none());
        let plain: Pot = serde_json::from_str(r#"{"on":true,"coin":"ONESHOT","say":"s"}"#).unwrap();
        assert_eq!(plain.lines(), Some(("s".to_string(), None)));
    }
}

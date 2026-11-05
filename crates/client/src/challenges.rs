//! Today's and this week's challenges, as Elo Pros has them
//! (`/api/challenges/oneshot`, its `challenges.py`): our own goals over our
//! own stats (`server/src/stats.rs`, `CHALLENGES`), picked there each UTC day
//! and ISO week, counted from the reports the server already sends, and paid
//! (in our trading cards) by Elo Pros' rules. The title's CHALLENGES tab
//! reads them, with the signed-in player's progress. Nothing here counts or
//! pays anything: it only shows what Elo Pros said.

use std::sync::{Arc, Mutex};

use bevy::prelude::*;
use serde::Deserialize;

/// Where Elo Pros answers.
pub const API: &str = "https://elopros.com/api/challenges/oneshot";
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
        let url = match &wallet {
            Some(w) => format!("{API}/{}", w.to_ascii_lowercase()),
            None => API.to_string(),
        };
        self.asked = Some((wallet, now));
        let slot: Pending = Arc::default();
        self.pending = Some(slot.clone());
        get(url, slot);
    }

    /// Takes the answer, if it came.
    pub fn poll(&mut self) {
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
}

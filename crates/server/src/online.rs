//! Who is playing, for Elo Pros (`meter/playtime.py`): `/online.json` and
//! `/status.json`, the files a title's shard list points at.
//!
//! Elo Pros reads `online.json` once a minute and credits each player listed
//! the time between two reads: hours on record, playing now and its one-line
//! status (Steam's rich presence), card drops, and the free skin owed for
//! ten minutes of play. `status.json` is the server list's player count.
//!
//! Two slots run at once during a deploy (`ops/README.md`), and nginx asks
//! either. So each writes what it holds to `<roster dir>/online-<port>.json`
//! every few seconds, and either one answers for both: its own roster, and
//! every other slot's file still fresh ([`FRESH_S`]). A slot that stopped
//! stops being fresh and drops out.

use serde_json::{json, Value};

/// A slot's file older than this is a slot that stopped.
pub const FRESH_S: u64 = 30;
/// Elo Pros' cap on a row's status (`playtime.STATUS_MAX`).
pub const STATUS_MAX: usize = 64;

/// One signed-in player.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    /// The wallet, or the account's `acct_` id.
    pub player: String,
    /// Unix seconds they connected.
    pub since: u64,
    pub status: String,
}

/// What one slot holds.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Slot {
    pub at: u64,
    /// Everyone connected, guests too.
    pub players: usize,
    pub rooms: usize,
    pub online: Vec<Row>,
}

impl Slot {
    pub fn to_json(&self) -> Value {
        let rows: Vec<Value> = self
            .online
            .iter()
            .map(|r| json!({"wallet": r.player, "since": r.since, "status": r.status}))
            .collect();
        json!({"at": self.at, "players": self.players, "rooms": self.rooms, "online": rows})
    }

    pub fn from_json(v: &Value) -> Option<Slot> {
        let online = v["online"]
            .as_array()?
            .iter()
            .filter_map(|r| {
                Some(Row {
                    player: r["wallet"].as_str()?.to_string(),
                    since: r["since"].as_u64()?,
                    status: r["status"].as_str().unwrap_or("").to_string(),
                })
            })
            .collect();
        Some(Slot {
            at: v["at"].as_u64()?,
            players: v["players"].as_u64().unwrap_or(0) as usize,
            rooms: v["rooms"].as_u64().unwrap_or(0) as usize,
            online,
        })
    }
}

/// A slot's file in the roster dir: by the game port it plays on.
pub fn file_name(port: u16) -> String {
    format!("online-{port}.json")
}

/// This slot and the others still fresh, as one: each player once (the
/// earlier connection), counts summed.
pub fn merge(own: &Slot, others: &[Slot], now: u64) -> Slot {
    let mut out = Slot {
        at: now,
        ..Slot::default()
    };
    let fresh = std::iter::once(own).chain(others.iter().filter(|s| now.saturating_sub(s.at) <= FRESH_S));
    for s in fresh {
        out.players += s.players;
        out.rooms += s.rooms;
        for r in &s.online {
            match out.online.iter_mut().find(|o| o.player == r.player) {
                Some(o) if r.since < o.since => *o = r.clone(),
                Some(_) => {}
                None => out.online.push(r.clone()),
            }
        }
    }
    out.online
        .sort_by(|a, b| (a.since, &a.player).cmp(&(b.since, &b.player)));
    out
}

/// `online.json`, as Elo Pros' `playtime.clean_roster` reads it.
pub fn online_json(s: &Slot) -> String {
    let rows: Vec<Value> = s
        .online
        .iter()
        .map(|r| json!({"wallet": r.player, "since": r.since, "status": r.status}))
        .collect();
    json!({"game": "oneshot", "at": s.at, "online": rows}).to_string()
}

/// `status.json`, as Elo Pros' `launcher._live_row` reads it. Bots are
/// never counted; nobody here is an agent.
pub fn status_json(s: &Slot, capacity: usize, proto: u16) -> String {
    json!({"at": s.at, "players": s.players, "agents": 0, "max_players": capacity,
           "rooms": s.rooms, "proto": proto})
    .to_string()
}

/// A status line cut to [`STATUS_MAX`] characters.
pub fn clip(s: String) -> String {
    if s.chars().count() <= STATUS_MAX {
        s
    } else {
        s.chars().take(STATUS_MAX).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(p: &str, since: u64) -> Row {
        Row {
            player: p.into(),
            since,
            status: "Free for all on Bulkhead".into(),
        }
    }

    #[test]
    fn two_slots_answer_as_one() {
        let own = Slot {
            at: 1000,
            players: 3,
            rooms: 2,
            online: vec![row("0xa", 900), row("acct_b", 950)],
        };
        let other = Slot {
            at: 990,
            players: 2,
            rooms: 1,
            online: vec![row("0xa", 800), row("0xc", 700)],
        };
        let stopped = Slot {
            at: 900,
            players: 9,
            rooms: 9,
            online: vec![row("0xd", 100)],
        };
        let m = merge(&own, &[other.clone(), stopped], 1000);
        assert_eq!((m.players, m.rooms), (5, 3), "a stopped slot drops out");
        let who: Vec<(&str, u64)> = m.online.iter().map(|r| (r.player.as_str(), r.since)).collect();
        assert_eq!(who, vec![("0xc", 700), ("0xa", 800), ("acct_b", 950)]);
        // What a slot writes, it reads back.
        assert_eq!(Slot::from_json(&other.to_json()), Some(other));
        let v: Value = serde_json::from_str(&online_json(&m)).unwrap();
        assert_eq!(v["at"], 1000);
        assert_eq!(v["online"][0]["wallet"], "0xc");
        let st: Value = serde_json::from_str(&status_json(&m, 256, 7)).unwrap();
        assert_eq!(
            (st["players"].as_u64(), st["max_players"].as_u64()),
            (Some(5), Some(256))
        );
    }

    #[test]
    fn a_status_fits_elo_pros_line() {
        assert_eq!(clip("x".repeat(80)).len(), STATUS_MAX);
        assert_eq!(clip("Big Team on Dry Fork".into()), "Big Team on Dry Fork");
    }
}

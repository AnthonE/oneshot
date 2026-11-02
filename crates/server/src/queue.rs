//! The ranked queue: signed-in players with a wallet wait here until enough
//! of them are close enough in rating. The rule is Elo Pros' own
//! (`meter/lobbies.py`): the longest waiter anchors a match, and takes the
//! nearest ratings inside a window of 100 that grows by 5 every second it
//! has waited. Doubles puts the best and the worst on one side.

use std::time::Instant;

use proto::{MODE_DOUBLES, MODE_DUEL};

pub const WINDOW_BASE: f64 = 100.0;
pub const WINDOW_GROW: f64 = 5.0;

#[derive(Clone, Debug)]
pub struct Entry {
    pub session: u64,
    pub wallet: String,
    pub name: proto::Name,
    pub mode: u8,
    pub rating: f64,
    pub since: Instant,
}

/// Players a match needs.
pub fn size(mode: u8) -> usize {
    match mode {
        MODE_DOUBLES => 4,
        _ => 2,
    }
}

#[derive(Default)]
pub struct Queue {
    pub entries: Vec<Entry>,
}

impl Queue {
    pub fn push(&mut self, e: Entry) {
        self.entries.push(e);
    }

    pub fn remove(&mut self, session: u64) -> Option<Entry> {
        let i = self.entries.iter().position(|e| e.session == session)?;
        Some(self.entries.remove(i))
    }

    pub fn has_wallet(&self, w: &str) -> bool {
        self.entries.iter().any(|e| e.wallet == w)
    }

    /// The next match that can form, as its sides: for a duel two sides of
    /// one, for doubles two sides of two with the ratings balanced.
    pub fn form(&mut self, now: Instant) -> Option<Vec<Vec<Entry>>> {
        for mode in [MODE_DUEL, MODE_DOUBLES] {
            let need = size(mode);
            let mut waiting: Vec<&Entry> = self.entries.iter().filter(|e| e.mode == mode).collect();
            if waiting.len() < need {
                continue;
            }
            waiting.sort_by_key(|e| e.since);
            let anchor = waiting[0];
            let waited = now.duration_since(anchor.since).as_secs_f64();
            let window = WINDOW_BASE + WINDOW_GROW * waited;
            let mut near: Vec<&Entry> = waiting[1..]
                .iter()
                .copied()
                .filter(|e| (e.rating - anchor.rating).abs() <= window)
                .collect();
            if near.len() < need - 1 {
                continue;
            }
            near.sort_by(|a, b| {
                let da = (a.rating - anchor.rating).abs();
                let db = (b.rating - anchor.rating).abs();
                da.total_cmp(&db).then(a.since.cmp(&b.since))
            });
            let mut picked: Vec<u64> = vec![anchor.session];
            picked.extend(near.iter().take(need - 1).map(|e| e.session));
            let mut players: Vec<Entry> = picked.iter().filter_map(|&s| self.remove(s)).collect();
            let sides = if mode == MODE_DOUBLES {
                players.sort_by(|a, b| b.rating.total_cmp(&a.rating));
                let (a, b, c, d) = (
                    players[0].clone(),
                    players[1].clone(),
                    players[2].clone(),
                    players[3].clone(),
                );
                vec![vec![a, d], vec![b, c]]
            } else {
                players.into_iter().map(|p| vec![p]).collect()
            };
            return Some(sides);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn e(session: u64, mode: u8, rating: f64, since: Instant) -> Entry {
        Entry {
            session,
            wallet: format!("0x{session:040x}"),
            name: proto::Name::new("p"),
            mode,
            rating,
            since,
        }
    }

    #[test]
    fn a_duel_forms_inside_the_window_and_the_window_grows() {
        let t0 = Instant::now();
        let mut q = Queue::default();
        q.push(e(1, MODE_DUEL, 1500.0, t0));
        q.push(e(2, MODE_DUEL, 1750.0, t0 + Duration::from_secs(1)));
        assert!(q.form(t0 + Duration::from_secs(2)).is_none(), "250 apart, window 110");
        let m = q.form(t0 + Duration::from_secs(31)).expect("window 255 by now");
        assert_eq!(m.len(), 2);
        assert!(q.entries.is_empty());
    }

    #[test]
    fn the_nearest_is_picked_and_doubles_balance() {
        let t0 = Instant::now();
        let mut q = Queue::default();
        q.push(e(1, MODE_DUEL, 1500.0, t0));
        q.push(e(2, MODE_DUEL, 1580.0, t0));
        q.push(e(3, MODE_DUEL, 1510.0, t0));
        let m = q.form(t0).unwrap();
        assert_eq!(m[1][0].session, 3);
        assert_eq!(q.entries.len(), 1);

        let mut q = Queue::default();
        for (s, r) in [(1, 1500.0), (2, 1560.0), (3, 1440.0), (4, 1520.0)] {
            q.push(e(s, MODE_DOUBLES, r, t0));
        }
        let m = q.form(t0).unwrap();
        let side = |i: usize| m[i].iter().map(|p| p.rating).sum::<f64>();
        assert_eq!(side(0), 1560.0 + 1440.0);
        assert_eq!(side(1), 1520.0 + 1500.0);
    }
}

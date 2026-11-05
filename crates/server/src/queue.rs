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
    /// A skin and emblem checked against the wallet.
    pub look: proto::Look,
    /// Doubles: the party it queues with, kept on one side.
    pub party: Option<crate::party::Seating>,
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
    /// one, for doubles two sides of two with the ratings balanced and a
    /// party of two never split.
    pub fn form(&mut self, now: Instant) -> Option<Vec<Vec<Entry>>> {
        if let Some(m) = self.form_duel(now) {
            return Some(m);
        }
        self.form_doubles(now)
    }

    fn form_duel(&mut self, now: Instant) -> Option<Vec<Vec<Entry>>> {
        for mode in [MODE_DUEL] {
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

    /// Doubles by units: a player alone, or a party of two once both are
    /// here. The longest-waiting unit anchors; the nearest units by mean
    /// rating inside its window fill four seats without splitting a pair.
    fn form_doubles(&mut self, now: Instant) -> Option<Vec<Vec<Entry>>> {
        let waiting: Vec<&Entry> = self.entries.iter().filter(|e| e.mode == MODE_DOUBLES).collect();
        let mut units: Vec<Vec<&Entry>> = Vec::new();
        for e in &waiting {
            match e.party.filter(|p| p.size == 2) {
                None => units.push(vec![e]),
                Some(p) => {
                    if units.iter().any(|u| u[0].party.is_some_and(|q| q.code == p.code)) {
                        continue;
                    }
                    let pair: Vec<&Entry> = waiting
                        .iter()
                        .copied()
                        .filter(|o| o.party.is_some_and(|q| q.code == p.code))
                        .take(2)
                        .collect();
                    // A pair waits for its other half.
                    if pair.len() == 2 {
                        units.push(pair);
                    }
                }
            }
        }
        let since = |u: &Vec<&Entry>| u.iter().map(|e| e.since).min().unwrap_or(now);
        let rating = |u: &Vec<&Entry>| u.iter().map(|e| e.rating).sum::<f64>() / u.len() as f64;
        units.sort_by_key(since);
        let anchor = units.first()?;
        let window = WINDOW_BASE + WINDOW_GROW * now.duration_since(since(anchor)).as_secs_f64();
        let mut near: Vec<&Vec<&Entry>> = units[1..]
            .iter()
            .filter(|u| (rating(u) - rating(anchor)).abs() <= window)
            .collect();
        near.sort_by(|a, b| {
            let (da, db) = ((rating(a) - rating(anchor)).abs(), (rating(b) - rating(anchor)).abs());
            da.total_cmp(&db).then(since(a).cmp(&since(b)))
        });
        let mut picked = vec![anchor];
        let mut n = anchor.len();
        for u in near {
            if n + u.len() <= 4 {
                n += u.len();
                picked.push(u);
            }
            if n == 4 {
                break;
            }
        }
        if n < 4 {
            return None;
        }
        let ids: Vec<Vec<u64>> = picked.iter().map(|u| u.iter().map(|e| e.session).collect()).collect();
        let mut take = |ids: &[u64]| -> Vec<Entry> { ids.iter().filter_map(|&s| self.remove(s)).collect() };
        let (pairs, singles): (Vec<&Vec<u64>>, Vec<&Vec<u64>>) = ids.iter().partition(|u| u.len() == 2);
        let sides = match pairs.len() {
            2 => vec![take(pairs[0]), take(pairs[1])],
            1 => {
                let rest: Vec<u64> = singles.iter().flat_map(|u| u.iter().copied()).collect();
                vec![take(pairs[0]), take(&rest)]
            }
            _ => {
                let all: Vec<u64> = ids.iter().flatten().copied().collect();
                let mut players = take(&all);
                players.sort_by(|a, b| b.rating.total_cmp(&a.rating));
                let (a, b, c, d) = (
                    players[0].clone(),
                    players[1].clone(),
                    players[2].clone(),
                    players[3].clone(),
                );
                vec![vec![a, d], vec![b, c]]
            }
        };
        Some(sides)
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
            look: proto::Look::default(),
            party: None,
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
    fn a_party_of_two_waits_for_its_half_and_sits_together() {
        let t0 = Instant::now();
        let pair = Some(crate::party::Seating {
            code: 9,
            size: 2,
            private: false,
            custom: Default::default(),
        });
        let mut q = Queue::default();
        // Half of a pair and three singles: no match; the pair waits whole.
        q.push(Entry {
            party: pair,
            ..e(1, MODE_DOUBLES, 1800.0, t0)
        });
        for s in 2..=4 {
            q.push(e(s, MODE_DOUBLES, 1500.0, t0));
        }
        assert!(q.form(t0).is_none(), "the pair's other half isn't here yet");
        // Its other half: the pair (far apart in rating, which alone would
        // split them) against two of the singles.
        q.push(Entry {
            party: pair,
            ..e(5, MODE_DOUBLES, 1400.0, t0)
        });
        let m = q.form(t0).expect("a pair and two singles");
        let side_of = |s: u64| m.iter().position(|side| side.iter().any(|p| p.session == s)).unwrap();
        assert_eq!(side_of(1), side_of(5), "the party sits together");
        assert_eq!(m[side_of(1)].len(), 2);
        assert_eq!(q.entries.len(), 1, "one single still waits");
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

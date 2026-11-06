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

/// How long a match from Elo Pros' queue waits for everyone on its sides
/// before it is called off (its players are told to queue again).
pub const GATHER_S: u64 = 30;
/// Matches remembered as started or called off, so one is never played
/// twice.
const DONE_KEPT: usize = 4096;

/// A match Elo Pros' queue formed (`meter/lobbies.py`), as its players come
/// for it: who is on which side, who is here.
#[derive(Clone, Debug)]
pub struct Gather {
    pub id: String,
    pub mode: u8,
    pub sides: Vec<Vec<String>>,
    /// Each with its side.
    pub here: Vec<(usize, Entry)>,
    pub first: Instant,
}

/// The matches being gathered, and those already started or called off.
#[derive(Default)]
pub struct Gathers {
    pub list: Vec<Gather>,
    done: std::collections::VecDeque<String>,
}

/// What [`Gathers::due`] hands back.
pub struct Due {
    /// Everyone came: the match's id and its sides, in Elo Pros' order.
    pub start: Vec<(String, Vec<Vec<Entry>>)>,
    /// Not everyone came in time: those who did, to be told.
    pub called_off: Vec<Entry>,
}

impl Gathers {
    /// Whether match `id` was already started or called off.
    pub fn done(&self, id: &str) -> bool {
        self.done.iter().any(|d| d == id)
    }

    pub fn has_wallet(&self, w: &str) -> bool {
        self.list.iter().any(|g| g.here.iter().any(|(_, e)| e.wallet == w))
    }

    /// `e` comes for match `id` (`sides` its wallets), on side `side`. A
    /// wallet already here for it is replaced (a reconnect): its old
    /// session is returned to be closed.
    pub fn arrive(&mut self, id: &str, sides: Vec<Vec<String>>, side: usize, e: Entry, now: Instant) -> Option<u64> {
        let g = match self.list.iter().position(|g| g.id == id) {
            Some(i) => &mut self.list[i],
            None => {
                self.list.push(Gather {
                    id: id.to_string(),
                    mode: e.mode,
                    sides,
                    here: Vec::new(),
                    first: now,
                });
                self.list.last_mut().unwrap()
            }
        };
        let old = g
            .here
            .iter()
            .position(|(_, x)| x.wallet == e.wallet)
            .map(|i| g.here.remove(i).1.session);
        g.here.push((side, e));
        old
    }

    /// A session gone: out of whatever it waited for.
    pub fn leave(&mut self, session: u64) {
        for g in &mut self.list {
            g.here.retain(|(_, e)| e.session != session);
        }
        self.list.retain(|g| !g.here.is_empty());
    }

    /// Everyone in every gathered match, let go (a drain).
    pub fn take_all(&mut self) -> Vec<Entry> {
        let all = self
            .list
            .drain(..)
            .flat_map(|g| g.here.into_iter().map(|(_, e)| e))
            .collect();
        all
    }

    /// The matches everyone has come for, and those whose wait is up.
    pub fn due(&mut self, now: Instant) -> Due {
        let mut due = Due {
            start: Vec::new(),
            called_off: Vec::new(),
        };
        let mut keep = Vec::new();
        for g in self.list.drain(..) {
            let want: usize = g.sides.iter().map(Vec::len).sum();
            if g.here.len() >= want {
                let mut sides: Vec<Vec<Entry>> = vec![Vec::new(); g.sides.len()];
                for (s, wallets) in g.sides.iter().enumerate() {
                    for w in wallets {
                        if let Some((_, e)) = g.here.iter().find(|(_, e)| e.wallet.eq_ignore_ascii_case(w)) {
                            sides[s].push(e.clone());
                        }
                    }
                }
                due.start.push((g.id.clone(), sides));
            } else if now.duration_since(g.first).as_secs() >= GATHER_S {
                due.called_off.extend(g.here.iter().map(|(_, e)| e.clone()));
            } else {
                keep.push(g);
                continue;
            }
            // Started or called off: never again.
            let id = g.id;
            if self.done.len() >= DONE_KEPT {
                self.done.pop_front();
            }
            self.done.push_back(id);
        }
        self.list = keep;
        due
    }
}

#[cfg(test)]
mod gather_tests {
    use super::*;
    use std::time::Duration;

    fn entry(session: u64, wallet: &str) -> Entry {
        Entry {
            session,
            wallet: wallet.into(),
            name: proto::Name::new("x"),
            mode: MODE_DUEL,
            rating: 1500.0,
            since: Instant::now(),
            look: proto::Look::default(),
            party: None,
        }
    }

    #[test]
    fn a_match_starts_once_everyone_is_here_and_never_again() {
        let t0 = Instant::now();
        let sides = vec![vec!["0xaa".to_string()], vec!["0xbb".to_string()]];
        let mut g = Gathers::default();
        assert_eq!(g.arrive("q-1", sides.clone(), 1, entry(2, "0xbb"), t0), None);
        assert!(g.due(t0).start.is_empty(), "one of two");
        // A reconnect replaces the session.
        assert_eq!(g.arrive("q-1", sides.clone(), 1, entry(3, "0xbb"), t0), Some(2));
        g.arrive("q-1", sides, 0, entry(1, "0xAA"), t0);
        let due = g.due(t0);
        assert_eq!(due.start.len(), 1);
        let (id, s) = &due.start[0];
        assert_eq!(id, "q-1");
        assert_eq!((s[0][0].session, s[1][0].session), (1, 3), "in Elo Pros' side order");
        assert!(g.done("q-1") && g.list.is_empty());
    }

    #[test]
    fn a_match_nobody_else_comes_for_is_called_off() {
        let t0 = Instant::now();
        let sides = vec![vec!["0xaa".to_string()], vec!["0xbb".to_string()]];
        let mut g = Gathers::default();
        g.arrive("q-2", sides, 0, entry(1, "0xaa"), t0);
        assert!(g.due(t0 + Duration::from_secs(GATHER_S - 1)).called_off.is_empty());
        let due = g.due(t0 + Duration::from_secs(GATHER_S));
        assert_eq!(due.called_off.len(), 1);
        assert!(g.done("q-2"));
        // Someone leaving takes them out.
        g.arrive(
            "q-3",
            vec![vec!["0xaa".into()], vec!["0xbb".into()]],
            0,
            entry(5, "0xaa"),
            t0,
        );
        g.leave(5);
        assert!(g.list.is_empty());
    }
}

//! Cups (docs/DESIGN.md §1, §13): free-for-all heats of up to eight, points
//! by placement, Swiss rounds (each round's heats drawn from players close
//! in points), then the top eight into a final. The bracket only: who plays
//! whom and what they scored. The sim thread runs the heats as ranked rooms
//! and feeds the results back.

/// Points for first to eighth in a heat.
pub const POINTS: [u32; 8] = [10, 7, 5, 4, 3, 2, 1, 0];
/// The most in a heat and in the final.
pub const HEAT: usize = 8;

#[derive(Clone, Debug)]
pub struct Entrant {
    pub session: u64,
    pub wallet: String,
    pub name: proto::Name,
    pub look: proto::Look,
    /// Points over the Swiss rounds.
    pub points: u32,
    /// Still connected: one who left plays no more heats but keeps points.
    pub here: bool,
    /// Place in the final, for those in it.
    pub final_place: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stage {
    /// Swiss round `n` (1-based) is being played.
    Swiss(u8),
    Final,
    Done,
}

#[derive(Clone, Debug)]
pub struct Cup {
    pub id: String,
    pub entrants: Vec<Entrant>,
    pub rounds: u8,
    pub stage: Stage,
    /// The heats of the stage being played: entrants by index, and whether
    /// the heat has reported.
    pub heats: Vec<(Vec<usize>, bool)>,
    /// Every heat played: (stage, match id, entrants, their places).
    pub played: Vec<(Stage, String, Vec<usize>, Vec<u32>)>,
}

/// Places from scores: 1 the best, a tie shares the better place.
pub fn places(scores: &[i32]) -> Vec<u32> {
    scores
        .iter()
        .map(|s| 1 + scores.iter().filter(|o| *o > s).count() as u32)
        .collect()
}

impl Cup {
    pub fn new(id: String, entrants: Vec<Entrant>, rounds: u8) -> Cup {
        let mut c = Cup {
            id,
            entrants,
            rounds: rounds.max(1),
            stage: Stage::Swiss(1),
            heats: Vec::new(),
            played: Vec::new(),
        };
        c.draw();
        c
    }

    /// Ranks entrants: finalists by their place in the final, then everyone
    /// by points, then by when they entered.
    pub fn standings(&self) -> Vec<usize> {
        let mut order: Vec<usize> = (0..self.entrants.len()).collect();
        order.sort_by_key(|&i| {
            let e = &self.entrants[i];
            (e.final_place.unwrap_or(u32::MAX), std::cmp::Reverse(e.points), i)
        });
        order
    }

    /// Each entrant's place in the standings, a tie (same final place or,
    /// outside the final, same points) sharing the better.
    pub fn place_of(&self, i: usize) -> u32 {
        let e = &self.entrants[i];
        let key = |o: &Entrant| (o.final_place.unwrap_or(u32::MAX), std::cmp::Reverse(o.points));
        1 + self.entrants.iter().filter(|o| key(o) < key(e)).count() as u32
    }

    /// Draws the heats of the current stage: the entrants still here, in
    /// standings order, cut into as few heats as fit and as even as can be,
    /// so each heat plays people close in points. The final is the top
    /// [`HEAT`].
    fn draw(&mut self) {
        let here: Vec<usize> = self
            .standings()
            .into_iter()
            .filter(|&i| self.entrants[i].here)
            .collect();
        self.heats.clear();
        if here.len() < 2 {
            self.stage = Stage::Done;
            return;
        }
        if self.stage == Stage::Final {
            self.heats.push((here.into_iter().take(HEAT).collect(), false));
            return;
        }
        let n = here.len();
        let k = n.div_ceil(HEAT);
        let mut at = 0;
        for h in 0..k {
            let size = n / k + usize::from(h < n % k);
            self.heats.push((here[at..at + size].to_vec(), false));
            at += size;
        }
    }

    /// Heat `h` of this stage ended (match `id`): `scores` in the order of
    /// its entrants. Returns true when the stage is over and the next has
    /// been drawn (or the cup is done).
    pub fn record(&mut self, h: usize, id: &str, scores: &[i32]) -> bool {
        let Some((members, done)) = self.heats.get_mut(h) else {
            return false;
        };
        if *done || scores.len() != members.len() {
            return false;
        }
        *done = true;
        let members = members.clone();
        let placed = places(scores);
        for (&i, &p) in members.iter().zip(&placed) {
            let e = &mut self.entrants[i];
            match self.stage {
                Stage::Final => e.final_place = Some(p),
                _ => e.points += POINTS.get(p as usize - 1).copied().unwrap_or(0),
            }
        }
        self.played.push((self.stage.clone(), id.to_string(), members, placed));
        if self.heats.iter().any(|(_, d)| !d) {
            return false;
        }
        self.stage = match self.stage {
            Stage::Swiss(r) if r < self.rounds => Stage::Swiss(r + 1),
            Stage::Swiss(_) => Stage::Final,
            _ => Stage::Done,
        };
        if self.stage != Stage::Done {
            self.draw();
        }
        true
    }

    /// An entrant left: no more heats for them. A heat they are in plays on
    /// without them (their fighter stands where it was).
    pub fn leave(&mut self, session: u64) {
        for e in self.entrants.iter_mut().filter(|e| e.session == session) {
            e.here = false;
        }
    }

    /// The round being played as the wire counts it: 1.. for Swiss, one
    /// past the last for the final, 0 once done.
    pub fn round_no(&self) -> u8 {
        match self.stage {
            Stage::Swiss(r) => r,
            Stage::Final => self.rounds + 1,
            Stage::Done => 0,
        }
    }

    /// The cup as a document: standings and every heat, for the cup's file
    /// beside the replays.
    pub fn doc(&self) -> serde_json::Value {
        let standings: Vec<serde_json::Value> = self
            .standings()
            .into_iter()
            .map(|i| {
                let e = &self.entrants[i];
                serde_json::json!({
                    "place": self.place_of(i),
                    "wallet": e.wallet,
                    "name": e.name.as_str(),
                    "points": e.points,
                    "final_place": e.final_place,
                })
            })
            .collect();
        let heats: Vec<serde_json::Value> = self
            .played
            .iter()
            .map(|(stage, id, members, placed)| {
                let stage = match stage {
                    Stage::Swiss(r) => format!("round {r}"),
                    _ => "final".to_string(),
                };
                serde_json::json!({
                    "stage": stage,
                    "match": id,
                    "replay": format!("{id}.osr"),
                    "places": members.iter().zip(placed).map(|(&i, &p)| serde_json::json!({
                        "wallet": self.entrants[i].wallet, "place": p
                    })).collect::<Vec<_>>(),
                })
            })
            .collect();
        serde_json::json!({
            "cup": self.id,
            "rounds": self.rounds,
            "points": POINTS,
            "standings": standings,
            "heats": heats,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cup(n: usize, rounds: u8) -> Cup {
        let entrants = (0..n)
            .map(|i| Entrant {
                session: i as u64,
                wallet: format!("0x{i:040x}"),
                name: proto::Name::new("p"),
                look: proto::Look::default(),
                points: 0,
                here: true,
                final_place: None,
            })
            .collect();
        Cup::new("cup-1".into(), entrants, rounds)
    }

    /// Plays the current stage: in each heat, the entrant with the lowest
    /// index wins, and so on down.
    fn play(c: &mut Cup) {
        for h in 0..c.heats.len() {
            let members = c.heats[h].0.clone();
            let scores: Vec<i32> = members.iter().map(|&i| 100 - i as i32).collect();
            c.record(h, &format!("m{h}"), &scores);
        }
    }

    #[test]
    fn heats_are_as_even_as_can_be() {
        let sizes = |n| cup(n, 1).heats.iter().map(|h| h.0.len()).collect::<Vec<_>>();
        assert_eq!(sizes(8), vec![8]);
        assert_eq!(sizes(9), vec![5, 4]);
        assert_eq!(sizes(20), vec![7, 7, 6]);
        assert_eq!(sizes(3), vec![3]);
        assert_eq!(cup(1, 1).stage, Stage::Done, "a cup of one is no cup");
    }

    #[test]
    fn places_share_ties() {
        assert_eq!(places(&[5, 9, 5, 0]), vec![2, 1, 2, 4]);
        assert_eq!(places(&[0, 0]), vec![1, 1]);
    }

    #[test]
    fn swiss_rounds_then_a_final_of_the_top_eight() {
        let mut c = cup(16, 2);
        assert_eq!(c.heats.len(), 2);
        play(&mut c);
        assert_eq!(c.stage, Stage::Swiss(2));
        // Round two draws by points: the two heat winners (0 and 8) are in
        // the same heat now.
        let top = &c.heats.iter().find(|h| h.0.contains(&0)).unwrap().0;
        assert!(top.contains(&8), "{top:?}");
        play(&mut c);
        assert_eq!(c.stage, Stage::Final);
        assert_eq!(c.heats.len(), 1);
        assert_eq!(c.heats[0].0.len(), HEAT);
        assert!(c.heats[0].0.contains(&0));
        play(&mut c);
        assert_eq!(c.stage, Stage::Done);
        let s = c.standings();
        assert_eq!(c.place_of(s[0]), 1);
        assert!(c.entrants[s[0]].final_place == Some(1));
        // Everyone outside the final ranks below everyone in it.
        let finalists = c.entrants.iter().filter(|e| e.final_place.is_some()).count();
        assert_eq!(finalists, HEAT);
        assert!(s[HEAT..].iter().all(|&i| c.entrants[i].final_place.is_none()));
        assert_eq!(c.played.len(), 2 + 2 + 1);
        assert_eq!(c.doc()["heats"].as_array().unwrap().len(), 5);
    }

    #[test]
    fn a_heat_reports_once_and_whoever_leaves_plays_no_more() {
        let mut c = cup(4, 1);
        c.leave(3);
        assert!(!c.record(0, "m", &[1, 2]), "wrong count");
        assert!(c.record(0, "m", &[4, 3, 2, 1]));
        assert!(!c.record(0, "m", &[4, 3, 2, 1]), "once");
        assert_eq!(c.stage, Stage::Final);
        assert_eq!(c.heats[0].0, vec![0, 1, 2], "3 left: not in the final");
        assert_eq!(c.entrants[3].points, POINTS[3], "but kept their points");
    }
}

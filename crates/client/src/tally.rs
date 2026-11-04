//! Who killed whom, as the stream tells every client (`Msg::Kill`): each
//! seat's kills, deaths and spree for the scoreboard, and the medals our
//! own kills earn. Drawing only: the score is the server's.

use std::collections::BTreeMap;

use sim::events::Cause;

/// Kills this close together chain (a double, a triple...).
pub const CHAIN_S: f32 = 4.0;
/// A spree ended at this many or more is a killjoy.
const KILLJOY_AT: u32 = 5;

/// Medals for a chain of kills, from the second on.
const CHAINS: [&str; 3] = ["DOUBLE KILL", "TRIPLE KILL", "QUAD KILL"];
const CHAIN_MORE: &str = "MAYHEM";
/// Medals for kills without dying.
const SPREES: [(u32, &str); 4] = [
    (5, "KILLING SPREE"),
    (10, "RAMPAGE"),
    (15, "UNSTOPPABLE"),
    (20, "UNTOUCHABLE"),
];

#[derive(Default)]
pub struct Tally {
    /// Kills of the other side (or of anyone, everyone for themselves) and
    /// deaths, by seat.
    pub kd: BTreeMap<u8, (u32, u32)>,
    spree: BTreeMap<u8, u32>,
    /// Our chain: how many, the last one's time.
    chain: (u32, f32),
    blooded: bool,
    /// Who last killed us, till we pay them back.
    nemesis: Option<u8>,
}

impl Tally {
    /// A kill as the stream told it (`foe`: across sides), at `now` in
    /// seconds; the medals it earns `me`, in the order they show.
    #[allow(clippy::too_many_arguments)]
    pub fn kill(
        &mut self,
        me: u8,
        killer: u8,
        victim: u8,
        cause: u16,
        head: bool,
        foe: bool,
        now: f32,
    ) -> Vec<&'static str> {
        self.kd.entry(victim).or_default().1 += 1;
        let ended = self.spree.insert(victim, 0).unwrap_or(0);
        let mut medals = Vec::new();
        if victim == me {
            self.chain = (0, 0.0);
            if killer != me {
                self.nemesis = Some(killer);
            }
        }
        if killer == victim || !foe {
            return medals;
        }
        self.kd.entry(killer).or_default().0 += 1;
        let spree = self.spree.entry(killer).or_default();
        *spree += 1;
        let spree = *spree;
        let first = !self.blooded;
        self.blooded = true;
        if killer != me {
            return medals;
        }
        if first {
            medals.push("FIRST BLOOD");
        }
        self.chain = if self.chain.0 > 0 && now - self.chain.1 <= CHAIN_S {
            (self.chain.0 + 1, now)
        } else {
            (1, now)
        };
        if self.chain.0 >= 2 {
            medals.push(CHAINS.get(self.chain.0 as usize - 2).copied().unwrap_or(CHAIN_MORE));
        }
        if let Some(&(_, m)) = SPREES.iter().find(|&&(n, _)| n == spree) {
            medals.push(m);
        }
        if head {
            medals.push("HEADSHOT");
        }
        let how = [
            (Cause::Behind, "ASSASSINATION"),
            (Cause::Melee, "BEATDOWN"),
            (Cause::Splatter, "SPLATTER"),
            (Cause::Sticky, "STUCK"),
        ];
        if let Some(&(_, m)) = how.iter().find(|(c, _)| *c as u16 == cause) {
            medals.push(m);
        }
        if self.nemesis == Some(victim) {
            self.nemesis = None;
            medals.push("REVENGE");
        }
        if ended >= KILLJOY_AT {
            medals.push("KILLJOY");
        }
        medals
    }
}

/// How a kill was made, for the feed: the gun's name, or the cause's.
pub fn how(cause: u16) -> &'static str {
    if (cause as usize) < sim::arms::COUNT {
        return sim::arms::arm(cause as u8).name;
    }
    let causes = [
        (Cause::Grenade, "GRENADE"),
        (Cause::Melee, "MELEE"),
        (Cause::Behind, "ASSASSINATION"),
        (Cause::Fall, "FALL"),
        (Cause::Splatter, "SPLATTER"),
        (Cause::Wreck, "WRECK"),
        (Cause::Sticky, "STICKY"),
    ];
    causes.iter().find(|(c, _)| *c as u16 == cause).map_or("", |&(_, n)| n)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BR: u16 = sim::arms::BR as u16;

    #[test]
    fn a_quick_pair_is_a_double_and_a_slow_one_is_not() {
        let mut t = Tally::default();
        assert_eq!(t.kill(0, 0, 1, BR, false, true, 0.0), vec!["FIRST BLOOD"]);
        assert_eq!(t.kill(0, 0, 2, BR, true, true, 3.0), vec!["DOUBLE KILL", "HEADSHOT"]);
        assert_eq!(t.kill(0, 0, 3, BR, false, true, 6.5), vec!["TRIPLE KILL"]);
        assert!(t.kill(0, 0, 1, BR, false, true, 20.0).is_empty());
        assert_eq!(t.kd[&0], (4, 0));
        assert_eq!(t.kd[&1], (0, 2));
    }

    #[test]
    fn a_spree_counts_till_death_and_ending_one_is_a_killjoy() {
        let mut t = Tally::default();
        for k in 0..5 {
            t.kill(0, 1, 2 + k, BR, false, true, k as f32 * 10.0);
        }
        // Seat 1's spree of five, ended by us; they had killed us before.
        t.kill(0, 1, 0, BR, false, true, 55.0);
        let m = t.kill(0, 0, 1, sim::events::Cause::Behind as u16, false, true, 60.0);
        assert_eq!(m, vec!["ASSASSINATION", "REVENGE", "KILLJOY"]);
    }

    #[test]
    fn a_teammate_or_ourselves_earns_nothing() {
        let mut t = Tally::default();
        assert!(t.kill(0, 0, 1, BR, false, false, 0.0).is_empty());
        assert!(t.kill(0, 0, 0, BR, false, false, 1.0).is_empty());
        assert_eq!(t.kd[&0], (0, 1));
        assert_eq!(how(BR), "BATTLE RIFLE");
        assert_eq!(how(sim::events::Cause::Sticky as u16), "STICKY");
    }
}

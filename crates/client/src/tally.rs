//! Who killed whom, as the stream tells every client (`Msg::Kill`): each
//! seat's kills and deaths for the scoreboard. Medals are the server's
//! (`room/src/medals.rs`, `Msg::Medal`). Drawing only: the score is the
//! server's.

use std::collections::BTreeMap;

use sim::events::Cause;

#[derive(Default)]
pub struct Tally {
    /// Kills of the other side (or of anyone, everyone for themselves) and
    /// deaths, by seat.
    pub kd: BTreeMap<u8, (u32, u32)>,
}

impl Tally {
    /// A kill as the stream told it (`foe`: across sides).
    pub fn kill(&mut self, killer: u8, victim: u8, foe: bool) {
        self.kd.entry(victim).or_default().1 += 1;
        if killer != victim && foe {
            self.kd.entry(killer).or_default().0 += 1;
        }
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

/// What a death screen says was done to us by `cause`, after Halo 2's
/// ("You were sniped by ...").
pub fn verb(cause: u16) -> &'static str {
    use sim::arms::{BEAM_RIFLE, CANNON, LAUNCHER, ROCKET, SHOTGUN, SNIPER, SWORD};
    if (cause as usize) < sim::arms::COUNT {
        return match cause as u8 {
            SNIPER | BEAM_RIFLE => "sniped",
            SHOTGUN => "shotgunned",
            ROCKET | LAUNCHER | CANNON => "blown up",
            SWORD => "sliced",
            _ => "killed",
        };
    }
    let causes = [
        (Cause::Grenade, "blown up"),
        (Cause::Melee, "beaten down"),
        (Cause::Behind, "assassinated"),
        (Cause::Splatter, "splattered"),
        (Cause::Wreck, "blown up"),
        (Cause::Sticky, "stuck"),
    ];
    causes
        .iter()
        .find(|(c, _)| *c as u16 == cause)
        .map_or("killed", |&(_, n)| n)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BR: u16 = sim::arms::BR as u16;

    #[test]
    fn kills_count_across_sides_and_deaths_always() {
        let mut t = Tally::default();
        t.kill(0, 1, true);
        t.kill(0, 2, true);
        // A teammate, and ourselves: deaths, no kills.
        t.kill(0, 1, false);
        t.kill(0, 0, false);
        assert_eq!(t.kd[&0], (2, 1));
        assert_eq!(t.kd[&1], (0, 2));
        assert_eq!(how(BR), "BATTLE RIFLE");
        assert_eq!(how(Cause::Sticky as u16), "STICKY");
    }
}

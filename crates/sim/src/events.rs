//! What happened this tick, for the server to filter and send (shots heard,
//! hits, kills) and for tests to read. A fixed array: no allocation.

use crate::consts::{EVENTS_KEPT, MAX_EVENTS};
use crate::geom::V3;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum Kind {
    #[default]
    None,
    /// A round left `a`'s gun `c` (an arm id, or a turret's [`Cause`]:
    /// `arms::fired`): from `pos` to `end`.
    Shot,
    /// `a` hurt `b` for `c` points; `part` 0 shields, 1 health, 2 head (to
    /// health), 3 head (the shields took it all).
    Hit,
    /// `a` killed `b` with `c` (an arm id or a [`Cause`] past them);
    /// `part` 2 if the blow was to the head.
    Kill,
    Spawn,
    /// `a` took arm `c` from a pad, or the power-up `c`
    /// (`arms::OVERSHIELD`, `arms::CAMO`, `arms::FRAGS`, `arms::STICKIES`);
    /// walking over a drop, rounds for arm `c`, or only grenades:
    /// `arms::FRAGS` (frags, or both kinds) or `arms::STICKIES`.
    Pickup,
    /// A blast at `pos`; `c` is the arm id (rocket) or [`Cause::Grenade`]
    /// or [`Cause::Sticky`].
    Blast,
    Melee,
    /// `a` threw a grenade; `c` 1 for a sticky.
    Throw,
    /// `a` hurt vehicle `part` (its slot) for `c` points.
    VehicleHit,
    /// Vehicle `part` blew up at `pos`, `a` the one who did it, or
    /// `world::NO_WINNER` (255): nobody (it fell into the void).
    Wreck,
    /// `a` got into vehicle `part`'s seat `b` (0 driver, 1 passenger), or
    /// out of it when `c` is 1.
    Ride,
    /// Capture the Flag: side `b`'s flag, at `pos`; `part` 0 taken by `a`,
    /// 1 dropped by `a`, 2 returned (by `a`, or 255 on its own), 3
    /// captured by `a`.
    Flag,
    /// Siege: node `b` (`world::World::nodes`); `part` 0 broken (`a` the
    /// first of the side that broke it in its ring, or 255), 1 a core's
    /// shield down (every generator of its side broken), 2 sudden death
    /// (every core's shield down).
    Node,
    /// Siege: minion `b` (`world::World::minions`); `part` 0 killed by seat
    /// `a` (255: by a tower or a minion), 1 hit by seat `a`, 2 came out of
    /// its base, at `pos`; 3 fired from `pos` to `end` at seat `a`, 4 at
    /// minion `a` (`c` 1 if it hit).
    Minion,
    /// Siege: node `b` fired a needle from `pos` to `end`, at seat `a`
    /// (`part` 0) or at minion `a` (`part` 1).
    Tower,
}

impl Kind {
    /// An event that must arrive: it changes what a player knows of the
    /// match (the feed, the score, the flags, a hull, a seat), so a crowded
    /// tick never drops it for a shot or a hit ([`EVENTS_KEPT`]).
    pub fn must(self) -> bool {
        matches!(self, Kind::Kill | Kind::Flag | Kind::Wreck | Kind::Spawn | Kind::Ride)
    }
}

/// Kill causes past the arm ids.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum Cause {
    Grenade = 100,
    Melee = 101,
    Behind = 102,
    Fall = 103,
    /// Run down by a vehicle.
    Splatter = 104,
    /// In or by a vehicle when it blew up.
    Wreck = 105,
    /// A sticky grenade: stuck to, or in its blast.
    Sticky = 106,
    /// A truck's minigun (`arms::MINIGUN`): its shots carry it as their
    /// gun.
    Minigun = 107,
    /// Siege: a tower's needles, a minion's shots.
    Tower = 108,
    Minion = 109,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Event {
    pub kind: Kind,
    pub a: u8,
    pub b: u8,
    pub part: u8,
    pub c: u16,
    pub pos: V3,
    pub end: V3,
}

#[derive(Clone, Debug)]
pub struct Events {
    list: [Event; MAX_EVENTS],
    len: usize,
    /// Events past the cap since the match began (never reset by
    /// [`Events::clear`]): the room reads it to say a tick was crowded.
    pub dropped: u32,
}

impl Default for Events {
    fn default() -> Self {
        Events {
            list: [Event::default(); MAX_EVENTS],
            len: 0,
            dropped: 0,
        }
    }
}

impl Events {
    pub fn clear(&mut self) {
        self.len = 0;
    }

    /// The first `n` gone, the rest moved up (no allocation).
    pub fn drop_first(&mut self, n: usize) {
        let n = n.min(self.len);
        self.list.copy_within(n..self.len, 0);
        self.len -= n;
    }

    /// Adds `e`, or counts it dropped: past the cap, or for an event that
    /// need not arrive, past the cap less the room kept for those that must
    /// ([`Kind::must`]).
    pub fn push(&mut self, e: Event) {
        let cap = if e.kind.must() {
            MAX_EVENTS
        } else {
            MAX_EVENTS - EVENTS_KEPT
        };
        if self.len < cap {
            self.list[self.len] = e;
            self.len += 1;
        } else {
            self.dropped += 1;
        }
    }

    pub fn as_slice(&self) -> &[Event] {
        &self.list[..self.len]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_crowded_tick_keeps_room_for_the_kills() {
        let mut ev = Events::default();
        let shot = Event {
            kind: Kind::Shot,
            ..Event::default()
        };
        for _ in 0..MAX_EVENTS {
            ev.push(shot);
        }
        assert_eq!(ev.as_slice().len(), MAX_EVENTS - EVENTS_KEPT);
        assert_eq!(ev.dropped as usize, EVENTS_KEPT);
        for _ in 0..EVENTS_KEPT + 1 {
            ev.push(Event {
                kind: Kind::Kill,
                ..Event::default()
            });
        }
        assert_eq!(ev.as_slice().len(), MAX_EVENTS);
        assert_eq!(ev.dropped as usize, EVENTS_KEPT + 1);
        ev.clear();
        assert!(ev.as_slice().is_empty());
        assert_eq!(ev.dropped as usize, EVENTS_KEPT + 1, "counted over the match");
    }
}

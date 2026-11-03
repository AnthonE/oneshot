//! What happened this tick, for the server to filter and send (shots heard,
//! hits, kills) and for tests to read. A fixed array: no allocation.

use crate::consts::MAX_EVENTS;
use crate::geom::V3;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum Kind {
    #[default]
    None,
    /// A round left `a`'s gun `c` (an arm id): from `pos` to `end`.
    Shot,
    /// `a` hurt `b` for `c` points; `part` 0 shields, 1 health, 2 head (to
    /// health), 3 head (the shields took it all).
    Hit,
    /// `a` killed `b` with `c` (an arm id or a [`Cause`] past them).
    Kill,
    Spawn,
    /// `a` took arm `c` from a pad.
    Pickup,
    /// A blast at `pos`; `c` is the arm id (rocket) or [`Cause::Grenade`].
    Blast,
    Melee,
    Throw,
    /// `a` hurt vehicle `part` (its slot) for `c` points.
    VehicleHit,
    /// Vehicle `part` blew up at `pos`, `a` the one who did it (or a
    /// slot past the seats: nobody).
    Wreck,
    /// `a` got into vehicle `part`'s seat `b` (0 driver, 1 passenger), or
    /// out of it when `c` is 1.
    Ride,
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
    /// Events past the cap since the match began.
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

    pub fn push(&mut self, e: Event) {
        if self.len < MAX_EVENTS {
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

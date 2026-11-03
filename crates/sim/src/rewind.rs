//! Where every fighter stood at the end of each of the last
//! [`REWIND_TICKS`] ticks, for lag compensation (docs/DESIGN.md §7). After
//! Gates' `sim-core/src/rewind.rs`, with sub-tick interpolation. The depth
//! of a rewind is the server's to choose, never the shooter's.

use crate::consts::REWIND_TICKS;
use crate::geom::V3;

/// What a rewind ring keeps of a thing each tick, and how it reads between
/// two ticks.
pub trait Posed: Copy + Default {
    fn alive(&self) -> bool;
    /// The same pose, not there: what a respawn writes over its past.
    fn gone(self) -> Self;
    /// `frac` of the way from `a` to `b`, both alive.
    fn between(a: &Self, b: &Self, frac: f32) -> Self;
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pose {
    pub pos: V3,
    pub crouched: bool,
    pub alive: bool,
}

impl Posed for Pose {
    fn alive(&self) -> bool {
        self.alive
    }
    fn gone(self) -> Self {
        Pose { alive: false, ..self }
    }
    fn between(a: &Self, b: &Self, frac: f32) -> Self {
        Pose {
            pos: a.pos + (b.pos - a.pos) * frac,
            crouched: if frac < 0.5 { a.crouched } else { b.crouched },
            alive: true,
        }
    }
}

/// `N` things' poses (`P`) for each of the last [`REWIND_TICKS`] ticks.
#[derive(Clone, Debug)]
pub struct Rewind<P, const N: usize> {
    rows: [[P; N]; REWIND_TICKS],
    /// The tick each row holds; a row whose stamp doesn't match is stale.
    stamps: [u32; REWIND_TICKS],
}

impl<P: Posed, const N: usize> Default for Rewind<P, N> {
    fn default() -> Self {
        Rewind {
            rows: [[P::default(); N]; REWIND_TICKS],
            stamps: [u32::MAX; REWIND_TICKS],
        }
    }
}

/// The deepest rewind, in 1/256 tick: the ring's length less the two rows
/// an interpolation reads.
pub const MAX_DEPTH_Q8: u32 = ((REWIND_TICKS as u32) - 2) << 8;

impl<P: Posed, const N: usize> Rewind<P, N> {
    /// Records the poses at the end of `tick`.
    pub fn write(&mut self, tick: u32, poses: &[P; N]) {
        let i = tick as usize % REWIND_TICKS;
        self.rows[i] = *poses;
        self.stamps[i] = tick;
    }

    /// A fighter (re)spawned: nothing before now may be hit at its old place.
    pub fn forget(&mut self, slot: usize) {
        for r in self.rows.iter_mut() {
            r[slot] = r[slot].gone();
        }
    }

    fn row(&self, tick: u32, slot: usize, live: &P) -> P {
        let i = tick as usize % REWIND_TICKS;
        if self.stamps[i] == tick {
            self.rows[i][slot]
        } else {
            *live
        }
    }

    /// Where `slot` was `depth_q8 / 256` ticks before `now`, the tick being
    /// stepped (whose poses are `live`, not yet written). Between two ticks
    /// the position is interpolated; a row where it was dead wins over one
    /// where it was alive, so a rewind never resurrects anyone.
    pub fn pose_at(&self, now: u32, slot: usize, depth_q8: u32, live: &P) -> P {
        let d = depth_q8.min(MAX_DEPTH_Q8);
        let k = d >> 8;
        let frac = (d & 255) as f32 * (1.0 / 256.0);
        let at = |k: u32| {
            if k == 0 {
                *live
            } else {
                self.row(now.wrapping_sub(k), slot, live)
            }
        };
        let a = at(k);
        if frac == 0.0 {
            return a;
        }
        let b = at(k + 1);
        if !a.alive() || !b.alive() {
            return a.gone();
        }
        P::between(&a, &b, frac)
    }
}

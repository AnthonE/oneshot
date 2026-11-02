//! Where every fighter stood at the end of each of the last
//! [`REWIND_TICKS`] ticks, for lag compensation (docs/DESIGN.md §7). After
//! Gates' `sim-core/src/rewind.rs`, with sub-tick interpolation. The depth
//! of a rewind is the server's to choose, never the shooter's.

use crate::consts::{MAX_FIGHTERS, REWIND_TICKS};
use crate::geom::V3;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pose {
    pub pos: V3,
    pub crouched: bool,
    pub alive: bool,
}

#[derive(Clone, Debug)]
pub struct Rewind {
    rows: [[Pose; MAX_FIGHTERS]; REWIND_TICKS],
    /// The tick each row holds; a row whose stamp doesn't match is stale.
    stamps: [u32; REWIND_TICKS],
}

impl Default for Rewind {
    fn default() -> Self {
        Rewind {
            rows: [[Pose::default(); MAX_FIGHTERS]; REWIND_TICKS],
            stamps: [u32::MAX; REWIND_TICKS],
        }
    }
}

/// The deepest rewind, in 1/256 tick: the ring's length less the two rows
/// an interpolation reads.
pub const MAX_DEPTH_Q8: u32 = ((REWIND_TICKS as u32) - 2) << 8;

impl Rewind {
    /// Records the poses at the end of `tick`.
    pub fn write(&mut self, tick: u32, poses: &[Pose; MAX_FIGHTERS]) {
        let i = tick as usize % REWIND_TICKS;
        self.rows[i] = *poses;
        self.stamps[i] = tick;
    }

    /// A fighter (re)spawned: nothing before now may be hit at its old place.
    pub fn forget(&mut self, slot: usize) {
        for r in self.rows.iter_mut() {
            r[slot].alive = false;
        }
    }

    fn row(&self, tick: u32, slot: usize, live: &Pose) -> Pose {
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
    pub fn pose_at(&self, now: u32, slot: usize, depth_q8: u32, live: &Pose) -> Pose {
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
        if !a.alive || !b.alive {
            return Pose { alive: false, ..a };
        }
        Pose {
            pos: a.pos + (b.pos - a.pos) * frac,
            crouched: if frac < 0.5 { a.crouched } else { b.crouched },
            alive: true,
        }
    }
}

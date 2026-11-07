//! The gun in our own view, its light alive: what its model paints glowing
//! (`gun_skin.wgsl`) flares as each round goes, runs hotter as a hot gun
//! heats and as a charge fills, and dims as a battery drains; and where the
//! model paints a row of lights, the row is the magazine. The shard gun's
//! crest goes out from the front a crystal at a time, the cannon's strip
//! shortens, the launcher's four eyes close one a round, the carbine's slot
//! and slashes empty, the battle rifle's side slots and the beam rifle's
//! lines run back. A reload fills it again with a sweep; an empty gun's
//! lights beat until it is reloaded, and a spent one's are all but out.
//!
//! Each row is a box in the model's own frame (measured off its glow map,
//! as `counter.rs`' places are), and the shader is told the model's frame
//! every frame, after the transforms, so the row stays on the gun however
//! it moves. Drawing only: it reads the predicted fighter's magazines.

use bevy::prelude::*;
use sim::arms::{self, arm, HEAT_FULL, NONE};
use sim::consts::ticks;

use super::gun_skin::{CoatParams, GunMaterial};

/// A row of lights that is the magazine: a run along a box, from the end
/// lit longest to the end out first, its edge a share of its length; and
/// cells, each a box, the first out last.
struct Row {
    run: Option<(Vec3, Vec3, Vec3, Vec3, f32)>,
    cells: &'static [(Vec3, Vec3)],
}

/// A row's cells at most (`gun_skin.wgsl`'s `CELLS`, each a pair of boxes
/// after the run's).
const CELLS: usize = 4;

const fn v(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(x, y, z)
}

/// Each gun's row, in its model's frame and units (`guns.rs`): x right, y
/// from the stock along the barrel, z up.
const ROWS: [(u8, Row); 6] = [
    (
        arms::SHARDS,
        // The crest along its spine: the front crystals go first.
        Row {
            run: Some((
                v(-0.03, 0.055, 0.06),
                v(0.03, 0.325, 0.3),
                v(0.0, 0.065, 0.1),
                v(0.0, 0.32, 0.2),
                0.08,
            )),
            cells: &[],
        },
    ),
    (
        arms::CANNON,
        // The strip along the barrel's top, shortening toward the breech.
        Row {
            run: Some((
                v(-0.03, 0.69, 0.17),
                v(0.03, 0.97, 0.22),
                v(0.0, 0.70, 0.19),
                v(0.0, 0.96, 0.19),
                0.06,
            )),
            cells: &[],
        },
    ),
    (
        arms::LAUNCHER,
        // Two eyes a side: the right front shuts first, the left rear last.
        Row {
            run: None,
            cells: &[
                (v(-0.07, 0.52, 0.14), v(-0.048, 0.575, 0.17)),
                (v(0.048, 0.52, 0.14), v(0.07, 0.575, 0.17)),
                (v(-0.08, 0.58, 0.135), v(-0.05, 0.655, 0.175)),
                (v(0.05, 0.58, 0.135), v(0.08, 0.655, 0.175)),
            ],
        },
    ),
    (
        arms::CARBINE,
        // The slot at its back, facing the eye, empties from the top; the
        // three slashes a side go front first.
        Row {
            run: Some((
                v(-0.02, 0.005, -0.04),
                v(0.02, 0.05, 0.035),
                v(0.0, 0.03, -0.031),
                v(0.0, 0.03, 0.025),
                0.1,
            )),
            cells: &[
                (v(-0.07, 0.41, -0.03), v(0.07, 0.453, 0.02)),
                (v(-0.07, 0.453, -0.03), v(0.07, 0.491, 0.02)),
                (v(-0.07, 0.491, -0.03), v(0.07, 0.535, 0.02)),
            ],
        },
    ),
    (
        arms::BR,
        // A slot a side under the receiver, running back.
        Row {
            run: Some((
                v(-0.03, 0.298, -0.005),
                v(0.03, 0.35, 0.03),
                v(0.0, 0.303, 0.01),
                v(0.0, 0.345, 0.01),
                0.08,
            )),
            cells: &[],
        },
    ),
    (
        arms::BEAM_RIFLE,
        // Its lines along the whole gun, drawing back to the stock.
        Row {
            run: Some((
                v(-0.2, 0.0, -0.2),
                v(0.2, 1.1, 0.2),
                v(0.0, 0.05, 0.0),
                v(0.0, 1.08, 0.0),
                0.12,
            )),
            cells: &[],
        },
    ),
];

/// What of the glow a row leaves where it is out.
const OUT: f32 = 0.06;
/// The glow's gain the frame a round goes, over 1, and how fast that falls
/// (a share a second).
const FLARE: f32 = 2.2;
const FLARE_FALL_PER_S: f32 = 9.0;
/// How fast a row fills again as a reload puts rounds back, a share of it a
/// second.
const REFILL_PER_S: f32 = 2.6;
/// An empty gun's beat: times a second, and the glow it beats up to where
/// the row is out.
const EMPTY_HZ: f32 = 1.6;
const EMPTY_BEAT: f32 = 0.5;
/// A spent gun's glow, and a drained battery's (it dims toward it as it
/// drains).
const SPENT: f32 = 0.15;
const BATTERY_LOW: f32 = 0.3;
/// A hot gun's glow at full heat, over 1, and a full charge's, with its
/// flicker once full (times a second, a share).
const HEAT_GAIN: f32 = 1.6;
const CHARGE_GAIN: f32 = 2.0;
const CHARGE_FLICKER_HZ: f32 = 13.0;
const CHARGE_FLICKER: f32 = 0.25;

/// A gun posed in our own view whose glow lives: which hand's, its own
/// materials and their standing lanes, and what it last showed.
#[derive(Component)]
pub struct Alive {
    arm: u8,
    left: bool,
    coat: CoatParams,
    mats: Vec<Handle<GunMaterial>>,
    /// Whether it has a row (one without dims as its battery drains).
    rowed: bool,
    /// The row's lit share as drawn, the flare left, the magazine last seen.
    shown: f32,
    flare: f32,
    mag: Option<u16>,
}

impl Alive {
    pub fn new(arm: u8, left: bool, coat: CoatParams, mats: Vec<Handle<GunMaterial>>) -> Self {
        Alive {
            arm,
            left,
            coat: with_row(coat, arm),
            mats,
            rowed: ROWS.iter().any(|(a, _)| *a == arm),
            shown: 1.0,
            flare: 0.0,
            mag: None,
        }
    }
}

/// `coat` with `arm`'s row laid into it, if it has one.
fn with_row(mut coat: CoatParams, arm: u8) -> CoatParams {
    let Some((_, row)) = ROWS.iter().find(|(a, _)| *a == arm) else {
        return coat;
    };
    coat.glow.z = OUT;
    if let Some((lo, hi, from, to, edge)) = row.run {
        coat.boxes[0] = lo.extend(0.0);
        coat.boxes[1] = hi.extend(0.0);
        coat.stays = from.extend(edge);
        coat.goes = to.extend(0.0);
    }
    for (k, (lo, hi)) in row.cells.iter().take(CELLS).enumerate() {
        coat.boxes[2 + 2 * k] = lo.extend(0.0);
        coat.boxes[3 + 2 * k] = hi.extend(0.0);
    }
    coat.goes.w = row.cells.len().min(CELLS) as f32;
    coat
}

/// How full a hand's charge is, 0..=1.
fn charge_of(f: &sim::fighter::Fighter, left: bool, id: u8) -> f32 {
    let a = arm(id);
    let t = if left { f.left_charge } else { f.charge };
    if a.charge_ms == 0 || t == 0 {
        return 0.0;
    }
    (f32::from(t) / ticks(a.charge_ms) as f32).min(1.0)
}

/// Every frame, once the transforms are set: each live gun's lanes.
pub fn live(
    link: NonSend<crate::LinkRes>,
    time: Res<Time>,
    mut guns: Query<(&mut Alive, &GlobalTransform, &InheritedVisibility)>,
    mut mats: ResMut<Assets<GunMaterial>>,
) {
    let f = link.0.session().core.own().0;
    let dt = time.delta_secs().min(0.1);
    let now = time.elapsed_secs();
    for (mut a, at, shown) in &mut guns {
        let g = if a.left { &f.left } else { f.gun() };
        if !shown.get() || !f.alive || g.arm != a.arm || a.arm == NONE {
            a.mag = None;
            continue;
        }
        let full = sim::fighter::Gun::full(a.arm).mag.max(1);
        let share = (f32::from(g.mag) / f32::from(full)).min(1.0);
        match a.mag {
            Some(m) if g.mag < m => {
                a.flare = 1.0;
                a.shown = share;
            }
            None => a.shown = share,
            _ => {}
        }
        a.mag = Some(g.mag);
        a.shown = if a.shown < share {
            (a.shown + REFILL_PER_S * dt).min(share)
        } else {
            share
        };
        a.flare = (a.flare - FLARE_FALL_PER_S * dt).max(0.0);

        let kind = arm(a.arm);
        let mut gain = 1.0 + FLARE * a.flare * a.flare;
        if kind.hot() {
            if !a.rowed {
                gain *= BATTERY_LOW + (1.0 - BATTERY_LOW) * share;
            }
            gain *= 1.0 + HEAT_GAIN * f32::from(g.heat.min(HEAT_FULL)) / f32::from(HEAT_FULL);
        }
        let charge = charge_of(f, a.left, a.arm);
        if charge > 0.0 {
            let flicker = if charge >= 1.0 {
                1.0 + CHARGE_FLICKER * (now * CHARGE_FLICKER_HZ * core::f32::consts::TAU).sin()
            } else {
                1.0
            };
            gain *= (1.0 + CHARGE_GAIN * charge) * flicker;
        }
        let mut out = a.coat.glow.z;
        if g.spent() {
            gain *= SPENT;
        } else if g.mag == 0 {
            let beat = 0.5 - 0.5 * (now * EMPTY_HZ * core::f32::consts::TAU).cos();
            out = OUT + (EMPTY_BEAT - OUT) * beat;
        }

        let mut p = a.coat;
        p.local = at.affine().inverse().into();
        p.glow = Vec4::new(gain, p.glow.y, out, a.shown);
        for h in &a.mats {
            // Asking for it to change marks it changed: only when it has.
            if mats.get(h).is_some_and(|m| m.extension.params != p) {
                if let Some(m) = mats.get_mut(h) {
                    m.extension.params = p;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every row is on a gun there is, inside its length, its boxes the right
    /// way round and its cells no more than the shader reads.
    #[test]
    fn every_row_lies_on_its_gun() {
        for (id, row) in &ROWS {
            let g = super::super::guns::gun(*id).expect("a gun");
            let boxes = row.run.iter().map(|r| (r.0, r.1)).chain(row.cells.iter().copied());
            for (lo, hi) in boxes {
                assert!(lo.cmplt(hi).all(), "{}", g.file);
                assert!(lo.y >= 0.0 && hi.y <= g.height * 1.05, "{}", g.file);
            }
            assert!(row.cells.len() <= CELLS, "{}", g.file);
            if let Some((lo, hi, from, to, edge)) = row.run {
                assert!(from.distance(to) > 0.01 && edge > 0.0 && edge < 0.5, "{}", g.file);
                let _ = (lo, hi);
            }
        }
    }

    /// A full gun shows its whole row; the standing lanes keep the coat.
    #[test]
    fn a_row_keeps_the_coat() {
        let c = with_row(super::super::gun_skin::BARE, arms::LAUNCHER);
        assert_eq!(c.goes.w, 4.0);
        assert_eq!(c.glow.y, 0.0);
        assert!(c.boxes[0].x > c.boxes[1].x, "no run");
        let c = with_row(super::super::gun_skin::BARE, arms::SNIPER);
        assert_eq!(c, super::super::gun_skin::BARE);
    }
}

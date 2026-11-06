//! The world's own small motions: what moves on a map when nobody does.
//! Spray where the swell meets the rock that stands in the sea, motes in
//! the sun riding the wind past the eye, and over an island (`level::Land::Isle`)
//! wisps of the sea of cloud rolling up over its rim and sand blown off
//! its dunes. Drawn only, from the map and its mood: nothing here knows
//! where anyone is but the camera.
//!
//! **Nothing here hides anyone.** The spray is thrown off rock that stands
//! out of bounds (`boulders::scenery`, the stones and the stacks' feet),
//! so it is never between two fighters; a mote or a grain of sand is
//! additive light a few millimetres across, and adds a few percent to a
//! pixel it crosses; a wisp is born off the island's edge at under a
//! third of a pixel's alpha and only thins from there. All are few
//! ([`MOTES`], [`SPRAY_HZ`], [`WISPS_HZ`], [`SAND_HZ`]) beside the pools
//! they draw in, so a firefight's sparks and smoke are never short of room
//! for them.

use bevy::prelude::*;
use sim::terrain::TerrainDef;

use super::table::Layer;
use super::Fx;
use crate::look::boulders;
use crate::look::mood::MoodNow;

/// How many motes drift round the eye at once, about, and the box they
/// are born in: this far out to each side, from this far under the eye to
/// this far over it, metres.
pub const MOTES: f32 = 70.0;
const MOTE_REACH_M: f32 = 16.0;
const MOTE_DOWN_M: f32 = 1.4;
const MOTE_UP_M: f32 = 3.0;
/// A mote's mean life, seconds (`table::built(Layer::Mote)`'s).
const MOTE_LIFE_S: f32 = 5.0;
/// A mote's colour over the recipe's: pale gold in the sun.
const MOTE_TINT: [f32; 3] = [1.0, 0.95, 0.8];
/// Waves a second that break on any one rock, and how far from the eye a
/// rock's spray is still thrown, metres.
pub const SPRAY_HZ: f32 = 0.3;
const SPRAY_DRAW_M: f32 = 220.0;
/// Puffs a break throws on a stone a metre across; a larger one more, to
/// this many.
const SPRAY_PUFFS: f32 = 1.6;
const SPRAY_PUFFS_MAX: f32 = 7.0;
/// The spray's colour: the sea's white, a little blue in it.
const SPRAY_TINT: [f32; 3] = [0.92, 0.96, 1.0];
/// An island's wisps: how many a second are born within [`WISP_REACH_M`]
/// of the eye, this far under its rim and this far out from its coast,
/// in the sea of cloud's grey white.
pub const WISPS_HZ: f32 = 2.2;
const WISP_REACH_M: f32 = 45.0;
const WISP_UNDER_M: (f32, f32) = (1.5, 4.0);
const WISP_OUT_M: (f32, f32) = (0.5, 3.0);
const WISP_TINT: [f32; 3] = [0.9, 0.92, 0.96];
/// Its blown sand: how many grains a second within [`SAND_REACH_M`], off
/// ground that stands this far over the island's floor (its dunes), in
/// the sand's own tan.
pub const SAND_HZ: f32 = 18.0;
const SAND_REACH_M: f32 = 22.0;
const SAND_DUNE_M: f32 = 0.6;
const SAND_TINT: [f32; 3] = [0.8, 0.66, 0.46];

/// What the ambient step carries from frame to frame.
#[derive(Resource, Default)]
pub struct Ambient {
    motes: f32,
    wisps: f32,
    sand: f32,
}

/// Every frame, before the pools are drawn: the motes topped up round the
/// eye and the waves that broke this frame thrown up.
pub fn drive(time: Res<Time>, mood: Res<MoodNow>, mut state: ResMut<Ambient>, fx: Option<ResMut<Fx>>) {
    let Some(mut fx) = fx else {
        return;
    };
    let fx = &mut *fx;
    let dt = time.delta_secs().min(0.1);
    if mood.roofed || dt <= 0.0 {
        return;
    }
    let eye = fx.eye;
    let m = &mood.mood;
    // Toward where the wind blows (`rig::to_sun_at`'s bearing), level.
    let wind = Vec3::new(m.wind_azim.sin(), 0.05, m.wind_azim.cos());

    // Motes: only where there is open ground under an open sky, which is
    // a map with a sea or a terrain's light; an arena of boxes has none.
    let props = boulders::scenery(mood.map);
    if !props.is_empty() {
        state.motes += dt * MOTES / MOTE_LIFE_S;
        while state.motes >= 1.0 {
            state.motes -= 1.0;
            let at = eye
                + Vec3::new(
                    fx.range(-MOTE_REACH_M, MOTE_REACH_M),
                    fx.range(-MOTE_DOWN_M, MOTE_UP_M),
                    fx.range(-MOTE_REACH_M, MOTE_REACH_M),
                );
            let tint = Color::linear_rgb(MOTE_TINT[0], MOTE_TINT[1], MOTE_TINT[2]);
            let scale = fx.range(0.7, 1.4);
            fx.emit_tint(Layer::Mote, 1, at, wind, tint, scale);
        }
    }

    // An island's air: wisps over its rim, sand off its dunes. Its sea is
    // cloud, which breaks on nothing.
    let def = sim::maps::ALL.iter().find(|d| d.id == mood.map);
    if let Some(d) = def.filter(|d| d.void > f32::NEG_INFINITY) {
        if let Some(t) = d.terrain.as_ref() {
            isle(fx, &mut state, dt, t, d.void, wind);
        }
        return;
    }

    // Spray: each rock the sea washes takes a wave now and then, each on
    // its own roll, so the shore never breaks in step.
    let sea = def.map_or(f32::NEG_INFINITY, |d| d.light.sea);
    if !sea.is_finite() {
        return;
    }
    let tint = Color::linear_rgb(SPRAY_TINT[0], SPRAY_TINT[1], SPRAY_TINT[2]);
    for p in props {
        let at = Vec3::new(p.at[0], sea, p.at[2]);
        if at.distance_squared(eye) > SPRAY_DRAW_M * SPRAY_DRAW_M || fx.rand() >= dt * SPRAY_HZ {
            continue;
        }
        // Its footprint at the waterline: a stack's is its height's share.
        let (rx, rz) = if p.model == boulders::STACK {
            (p.size[1] * 0.3, p.size[1] * 0.17)
        } else {
            (p.size[0] * 0.5, p.size[2] * 0.5)
        };
        let r = rx.max(rz);
        let n = (SPRAY_PUFFS * r).clamp(2.0, SPRAY_PUFFS_MAX) as usize;
        // One side of it takes the wave.
        let a0 = fx.range(0.0, core::f32::consts::TAU);
        for _ in 0..n {
            let a = a0 + fx.range(-0.6, 0.6);
            let edge = at + Vec3::new(a.cos() * rx, 0.1, a.sin() * rz);
            let out = Vec3::new(a.cos() * 0.35, 1.0, a.sin() * 0.35).normalize();
            let scale = (0.6 + 0.22 * r).min(2.4) * fx.range(0.8, 1.25);
            fx.emit_tint(Layer::Spray, 1, edge, out, tint, scale);
        }
    }
}

/// An island's own motions, over terrain `t` whose ground ends at `void`:
/// wisps of the sea of cloud born off its coast under the rim and lifted
/// over it, drifting inward and downwind; grains of sand lifted off its
/// dunes and carried down the wind.
fn isle(fx: &mut Fx, state: &mut Ambient, dt: f32, t: &TerrainDef, void: f32, wind: Vec3) {
    let eye = fx.eye;
    let ground = |x: f32, z: f32| crate::look::grass::ground(t, x, z).filter(|&y| y > void);
    // The island's middle, for which way is in.
    let mid = Vec2::new(
        t.x0 + (t.nx - 1) as f32 * t.cell * 0.5,
        t.z0 + (t.nz - 1) as f32 * t.cell * 0.5,
    );
    let tint = Color::linear_rgb(WISP_TINT[0], WISP_TINT[1], WISP_TINT[2]);
    state.wisps += dt * WISPS_HZ;
    while state.wisps >= 1.0 {
        state.wisps -= 1.0;
        // A point off the island within reach whose way in meets the
        // coast within a few metres: a few tries, or none this time.
        for _ in 0..6 {
            let p = Vec2::new(
                eye.x + fx.range(-WISP_REACH_M, WISP_REACH_M),
                eye.z + fx.range(-WISP_REACH_M, WISP_REACH_M),
            );
            if ground(p.x, p.y).is_some() {
                continue;
            }
            let inward = (mid - p).normalize_or_zero();
            let Some(rim) = [2.0f32, 4.0, 6.0].into_iter().find_map(|k| {
                let q = p + inward * k;
                ground(q.x, q.y)
            }) else {
                continue;
            };
            let out = fx.range(WISP_OUT_M.0, WISP_OUT_M.1);
            let under = fx.range(WISP_UNDER_M.0, WISP_UNDER_M.1);
            let at = Vec3::new(p.x - inward.x * out, rim - under, p.y - inward.y * out);
            let dir = (Vec3::new(inward.x, 0.0, inward.y) * 0.7 + wind * 0.5 + Vec3::Y * 0.3).normalize_or(Vec3::Y);
            let scale = fx.range(0.8, 1.3);
            fx.emit_tint(Layer::Mist, 1, at, dir, tint, scale);
            break;
        }
    }
    let tint = Color::linear_rgb(SAND_TINT[0], SAND_TINT[1], SAND_TINT[2]);
    state.sand += dt * SAND_HZ;
    while state.sand >= 1.0 {
        state.sand -= 1.0;
        for _ in 0..4 {
            let (x, z) = (
                eye.x + fx.range(-SAND_REACH_M, SAND_REACH_M),
                eye.z + fx.range(-SAND_REACH_M, SAND_REACH_M),
            );
            let Some(y) = ground(x, z).filter(|&y| y > SAND_DUNE_M) else {
                continue;
            };
            let lift = fx.range(0.05, 0.4);
            let scale = fx.range(0.7, 1.3);
            fx.emit_tint(Layer::Sand, 1, Vec3::new(x, y + lift, z), wind, tint, scale);
            break;
        }
    }
}

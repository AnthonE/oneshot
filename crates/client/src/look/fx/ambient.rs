//! The world's own small motions: what moves on a map when nobody does.
//! Spray where the swell meets the rock that stands in the sea, and motes
//! in the sun riding the wind past the eye. Drawn only, from the map and
//! its mood: nothing here knows where anyone is but the camera.
//!
//! **Nothing here hides anyone.** The spray is thrown off rock that stands
//! out of bounds (`boulders::scenery`, the stones and the stacks' feet),
//! so it is never between two fighters; a mote is additive light a few
//! millimetres across, and adds a few percent to a pixel it crosses. Both
//! are few ([`MOTES`], [`SPRAY_HZ`]) beside the pools they draw in, so a
//! firefight's sparks and smoke are never short of room for them.

use bevy::prelude::*;

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

/// What the ambient step carries from frame to frame.
#[derive(Resource, Default)]
pub struct Ambient {
    motes: f32,
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

    // Spray: each rock the sea washes takes a wave now and then, each on
    // its own roll, so the shore never breaks in step.
    let sea = sim::maps::ALL
        .iter()
        .find(|d| d.id == mood.map)
        .map_or(f32::NEG_INFINITY, |d| d.light.sea);
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

//! What lies about the map for the taking: each pad's gun, turning over its
//! disc and gone while the pad waits (the snapshot's `empty_s`), the disc
//! lit while it holds one, and the line that says what USE would take
//! here. All read from the session; what USE takes is the sim's
//! (`World::pickup`).

use bevy::prelude::*;
use client_net::core::NetCore;
use sim::arms::{arm, NONE};
use sim::consts::{ENTER_R, PAD_REACH};
use sim::fighter::Fighter;
use sim::vehicle::DRIVER;
use sim::world::Phase;

use crate::look::guns::{self, Models};

/// How high a pad's gun floats over its disc, how far and how fast it
/// bobs, and how fast it turns (radians a second).
const FLOAT_Y: f32 = 0.9;
const BOB_M: f32 = 0.06;
const BOB_HZ: f32 = 0.45;
const SPIN: f32 = 1.1;
/// A loaded pad's disc glows this, a waiting one's not at all.
const DISC_GLOW: f32 = 1.6;

/// Pad `i`'s gun.
#[derive(Component)]
pub struct PadGun(pub usize);

/// Pad `i`'s disc (`level::build`), its material its own.
#[derive(Component)]
pub struct PadDisc(pub usize);

/// The welcome the pads' guns were made for, and which discs are lit.
#[derive(Resource, Default)]
pub struct Items {
    built: u32,
    lit: Vec<Option<bool>>,
}

/// Whether pad `i` holds its gun, as the last snapshot says.
fn loaded(core: &NetCore, i: usize) -> bool {
    core.latest().pads().get(i).is_none_or(|p| p.empty_s == 0)
}

#[allow(clippy::too_many_arguments)]
pub fn draw(
    link: NonSend<crate::LinkRes>,
    mut items: ResMut<Items>,
    mut commands: Commands,
    models: Res<Models>,
    time: Res<Time>,
    mut held: Query<(&PadGun, &mut Transform, &mut Visibility)>,
    discs: Query<(&PadDisc, &MeshMaterial3d<StandardMaterial>)>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    let s = link.0.session();
    if !s.ready {
        return;
    }
    let core = &s.core;
    if s.welcomes != items.built {
        // The level's parts go with the level (`level::build`).
        items.built = s.welcomes;
        items.lit = vec![None; core.map.pads.len()];
        for (i, p) in core.map.pads.iter().enumerate() {
            let root = commands
                .spawn((
                    crate::level::LevelPart,
                    PadGun(i),
                    Transform::from_xyz(p.pos.x, p.pos.y + FLOAT_Y, p.pos.z),
                    Visibility::Hidden,
                ))
                .id();
            if let (Some(g), Some((pivot, _))) = (
                guns::gun(p.arm),
                guns::spawn_posed(&mut commands, &models, p.arm, guns::body_basis(), root),
            ) {
                // Turned about its middle, not its grip.
                let half = g.height * g.scale * (0.5 - g.grip);
                commands.entity(pivot).insert(Transform {
                    translation: Vec3::new(-half, 0.0, 0.0),
                    rotation: guns::body_basis(),
                    ..default()
                });
            }
        }
        return;
    }
    let t = time.elapsed_secs();
    for (pad, mut tr, mut vis) in held.iter_mut() {
        let Some(def) = core.map.pads.get(pad.0) else {
            continue;
        };
        let on = loaded(core, pad.0);
        let want = if on { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
        // Each pad a little out of step with the next.
        let phase = pad.0 as f32 * 1.7;
        tr.translation.y = def.pos.y + FLOAT_Y + BOB_M * (t * BOB_HZ * core::f32::consts::TAU + phase).sin();
        tr.rotation = Quat::from_rotation_y(t * SPIN + phase);
    }
    for (disc, mat) in discs.iter() {
        let on = loaded(core, disc.0);
        let Some(was) = items.lit.get_mut(disc.0) else {
            continue;
        };
        if *was == Some(on) {
            continue;
        }
        *was = Some(on);
        if let Some(m) = mats.get_mut(&mat.0) {
            m.emissive = if on {
                LinearRgba::rgb(0.79, 0.6, 0.12) * DISC_GLOW
            } else {
                LinearRgba::BLACK
            };
        }
    }
}

/// What USE would do for `f` standing here, as `World::pickup` would take
/// it: a vehicle in reach with a seat free first, then a pad holding its
/// gun. None for nothing (or not on foot and alive in play).
pub fn offer(core: &NetCore, now_ms: f64) -> Option<String> {
    let f: &Fighter = core.own().0;
    if !f.alive || f.riding().is_some() || core.phase() != Phase::Playing {
        return None;
    }
    let at = f.body.pos;
    let car = core
        .vehicles(now_ms)
        .into_iter()
        .filter(|v| v.seats.contains(&0) && (v.pos - at).flat().len() < ENTER_R && (v.pos.y - at.y).abs() < 2.0)
        .min_by(|a, b| (a.pos - at).flat().len().total_cmp(&(b.pos - at).flat().len()));
    if let Some(v) = car {
        return Some(if v.seats[DRIVER] == 0 { "drive" } else { "ride" }.into());
    }
    if core.rules.gun_game {
        return None;
    }
    let (_, p) = core.map.pads.iter().enumerate().find(|&(i, p)| {
        let d = p.pos - at;
        d.x.abs() < PAD_REACH && d.z.abs() < PAD_REACH && d.y.abs() < 1.5 && loaded(core, i)
    })?;
    Some(take(f, p.arm))
}

/// How taking `id` would land, in pickup's words.
fn take(f: &Fighter, id: u8) -> String {
    let a = arm(id);
    if a.dual && arm(f.gun().arm).dual && f.gun().arm != NONE {
        format!("dual wield {}", a.name)
    } else if f.guns.iter().any(|g| g.arm == id) {
        format!("refill {}", a.name)
    } else {
        format!("pick up {}", a.name)
    }
}

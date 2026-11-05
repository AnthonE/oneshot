//! What lies about the map for the taking: each pad's gun, turning over its
//! disc and gone while the pad waits (the snapshot's `empty_s`), the disc
//! lit while it holds one; the guns and grenades on the floor that the
//! snapshot names; and the line that says what USE would take here. All
//! read from the session; what USE takes is the sim's (`World::pickup`).

use bevy::prelude::*;
use client_net::core::NetCore;
use sim::arms::{arm, NONE};
use sim::consts::{DROP_REACH, ENTER_R, MAX_DROPS, PAD_REACH};
use sim::fighter::Fighter;
use sim::geom::V3;
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
/// A gun on the floor lies on its side this high (its half thickness).
const LIE_Y: f32 = 0.05;
/// A drop thrown by a blast: how fast the drawn one follows where the
/// snapshot says it is (1/s), how fast it tumbles for its speed (radians a
/// metre), and how fast it settles flat on landing (1/s). Further than
/// [`DROP_SNAP_M`] in one frame is another drop in the same slot: put down
/// there, not flown to.
const DROP_FOLLOW: f32 = 30.0;
const TUMBLE_PER_M: f32 = 2.2;
const SETTLE: f32 = 12.0;
const DROP_SNAP_M: f32 = 3.0;

/// A drop drawn in its slot: its entity and gun, where the snapshot puts
/// it and where it is drawn, and its tumble.
#[derive(Clone, Copy)]
struct Lying {
    e: Entity,
    arm: u8,
    to: Vec3,
    at: Vec3,
    /// Its turn on the floor (each slot its own) and the tumble on top.
    turn: Quat,
    spin: Quat,
}

/// Pad `i`'s gun.
#[derive(Component)]
pub struct PadGun(pub usize);

/// Pad `i`'s disc (`level::build`), its material its own.
#[derive(Component)]
pub struct PadDisc(pub usize);

/// The welcome the pads' guns were made for, which discs are lit, and the
/// drop drawn in each drop slot: its entity, gun and where it lies.
#[derive(Resource, Default)]
pub struct Items {
    built: u32,
    lit: Vec<Option<bool>>,
    drops: [Option<Lying>; MAX_DROPS],
}

/// Whether pad `i` holds its gun, as the last snapshot says.
fn loaded(core: &NetCore, i: usize) -> bool {
    core.latest().pads().get(i).is_none_or(|p| p.empty_s == 0)
}

/// `arm`'s gun under `root`, turned about its middle rather than its grip.
fn posed(commands: &mut Commands, models: &Models, arm: u8, basis: Quat, root: Entity) {
    if let (Some(g), Some((pivot, _))) = (guns::gun(arm), guns::spawn_posed(commands, models, arm, basis, root)) {
        let half = g.height * g.scale * (0.5 - g.grip);
        commands.entity(pivot).insert(Transform {
            translation: basis * Vec3::new(0.0, -half, 0.0),
            rotation: basis,
            ..default()
        });
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw(
    link: NonSend<crate::LinkRes>,
    mut items: ResMut<Items>,
    mut commands: Commands,
    models: Res<Models>,
    time: Res<Time>,
    mut held: Query<(&PadGun, &mut Transform, &mut Visibility)>,
    mut placed: Query<&mut Transform, Without<PadGun>>,
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
        items.drops = [None; MAX_DROPS];
        for (i, p) in core.map.pads.iter().enumerate() {
            let root = commands
                .spawn((
                    crate::level::LevelPart,
                    PadGun(i),
                    Transform::from_xyz(p.pos.x, p.pos.y + FLOAT_Y, p.pos.z),
                    Visibility::Hidden,
                ))
                .id();
            posed(&mut commands, &models, p.arm, guns::body_basis(), root);
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
    // The floor as the last snapshot names it: a slot gone or holding
    // another gun is taken down, a new one put down, one that moved (a blast
    // threw it) followed, tumbling while it flies and settling flat.
    let dt = time.delta_secs();
    let mut want = [None; MAX_DROPS];
    for d in core.latest().drops() {
        if let Some(w) = want.get_mut(d.slot as usize) {
            *w = Some((d.arm, Vec3::new(d.pos.x, d.pos.y + LIE_Y, d.pos.z)));
        }
    }
    for (slot, (have, want)) in items.drops.iter_mut().zip(want).enumerate() {
        if let (Some(l), Some((a, p))) = (have.as_mut(), want) {
            if l.arm == a && l.to.distance(p) < DROP_SNAP_M {
                l.to = p;
                fly(l, dt);
                if let Ok(mut tr) = placed.get_mut(l.e) {
                    tr.translation = l.at;
                    tr.rotation = l.spin * l.turn;
                }
                continue;
            }
        }
        if let Some(l) = have.take() {
            commands.entity(l.e).despawn();
        }
        let Some((a, p)) = want else {
            continue;
        };
        // On its side, each slot turned its own way.
        let turn = Quat::from_rotation_y(slot as f32 * 2.4);
        let root = commands
            .spawn((
                crate::level::LevelPart,
                Transform::from_translation(p).with_rotation(turn),
                Visibility::default(),
            ))
            .id();
        if a == NONE {
            commands.spawn((SceneRoot(models.frag.clone()), Transform::default(), ChildOf(root)));
        } else {
            let side = Quat::from_rotation_x(core::f32::consts::FRAC_PI_2);
            posed(&mut commands, &models, a, side * guns::body_basis(), root);
        }
        *have = Some(Lying {
            e: root,
            arm: a,
            to: p,
            at: p,
            turn,
            spin: Quat::IDENTITY,
        });
    }
}

/// One frame of a drop drawn toward where it is: tumbling end over end
/// about the level line across its travel while it moves, and, still,
/// turned the short way back onto whichever side is nearer the floor.
fn fly(l: &mut Lying, dt: f32) {
    let was = l.at;
    l.at += (l.to - l.at) * (1.0 - (-DROP_FOLLOW * dt).exp());
    let step = l.at - was;
    let across = Vec3::new(step.z, 0.0, -step.x);
    let m = across.length();
    if m > 1e-4 {
        l.spin = (Quat::from_axis_angle(across / m, -m * TUMBLE_PER_M) * l.spin).normalize();
    }
    if m < dt * 0.5 {
        let up = l.spin * Vec3::Y;
        let flat = if up.y >= 0.0 { Vec3::Y } else { Vec3::NEG_Y };
        let to = (Quat::from_rotation_arc(up, flat) * l.spin).normalize();
        l.spin = l.spin.slerp(to, 1.0 - (-SETTLE * dt).exp());
    }
}

/// What USE would do for `f` standing here, as `World::pickup` would take
/// it: a vehicle in reach with a seat free first, then the nearest pad
/// holding its gun or gun on the floor. None for nothing (or not on foot
/// and alive in play).
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
    let near = |p: V3, r: f32| {
        let d = p - at;
        (d.x.abs() < r && d.z.abs() < r && d.y.abs() < 1.5).then(|| d.flat().len())
    };
    let pads = core
        .map
        .pads
        .iter()
        .enumerate()
        .filter(|&(i, _)| loaded(core, i))
        .filter_map(|(_, p)| near(p.pos, PAD_REACH).map(|d| (d, p.arm, true)));
    let drops = core
        .latest()
        .drops()
        .iter()
        .filter(|d| d.arm != NONE)
        .filter_map(|d| near(d.pos, DROP_REACH).map(|r| (r, d.arm, false)));
    let (_, id, pad) = pads.chain(drops).min_by(|a, b| a.0.total_cmp(&b.0))?;
    take(f, id, pad)
}

/// How taking `id` would land, in pickup's words. A gun of a kind carried,
/// from the floor, is no offer: walking over took its rounds.
fn take(f: &Fighter, id: u8, pad: bool) -> Option<String> {
    let a = arm(id);
    Some(if a.dual && arm(f.gun().arm).dual {
        format!("dual wield {}", a.name)
    } else if f.guns.iter().any(|g| g.arm == id) {
        if !pad {
            return None;
        }
        format!("refill {}", a.name)
    } else {
        format!("pick up {}", a.name)
    })
}

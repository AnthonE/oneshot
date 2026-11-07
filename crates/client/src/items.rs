//! What lies about the map for the taking: each pad's gun lying on the
//! ground where it spawns (or its power-up: a glowing orb resting there,
//! green the overshield, pale the camouflage), gone while the pad waits
//! (the snapshot's `empty_s`); the guns and grenades on the floor that the
//! snapshot names, each glowing faintly to be seen; and the line that says what USE would take here. All
//! read from the session; what USE takes is the sim's (`World::pickup`).

use std::collections::HashMap;

use bevy::gltf::GltfMaterialName;
use bevy::prelude::*;
use bevy::scene::SceneInstanceReady;
use client_net::core::NetCore;
use sim::arms::{arm, NONE};
use sim::consts::{DROP_REACH, ENTER_R, MAX_DROPS, PAD_REACH};
use sim::fighter::Fighter;
use sim::geom::V3;
use sim::vehicle::DRIVER;
use sim::world::Phase;

use crate::look::guns::{self, Models};

/// A power-up's orb, resting on the ground: its radius, and its glow in
/// its colour; and how far it swells and shrinks, how fast.
const ORB_R: f32 = 0.26;
const ORB_GLOW: f32 = 6.0;
const ORB_PULSE: f32 = 0.08;
const ORB_HZ: f32 = 1.3;

/// A power-up's colour, linear: the overshield's green, camouflage's pale
/// blue (as their shells are drawn: `look/bodies.rs`).
fn orb_color(id: u8) -> LinearRgba {
    match id {
        sim::arms::OVERSHIELD => LinearRgba::rgb(0.25, 1.0, 0.45),
        sim::arms::FRAGS => LinearRgba::rgb(0.95, 0.5, 0.12),
        sim::arms::STICKIES => LinearRgba::rgb(0.2, 0.5, 1.0),
        _ => LinearRgba::rgb(0.6, 0.78, 1.0),
    }
}
/// A gun or grenades lying for the taking glow faintly this, the pads' old
/// gold, so they read off a grey floor; linear.
const LYING_GLOW: LinearRgba = LinearRgba::rgb(0.79 * 0.6, 0.64 * 0.6, 0.15 * 0.6);

/// Frags on a pad: how far apart they lie.
const PAD_FRAG_APART_M: f32 = 0.14;
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

/// The link (`front::LinkGen`) and welcome the pads' guns were made for,
/// and the drop drawn in each drop slot: its entity, gun and where it lies.
#[derive(Resource, Default)]
pub struct Items {
    built: Option<(u32, u32)>,
    drops: [Option<Lying>; MAX_DROPS],
    /// Drops picked up, hidden, by arm: a gun of that arm put down again
    /// shows one rather than load its scene anew (the oldest go past
    /// [`SPARE`]).
    spare: Vec<(u8, Entity)>,
}

/// Drops kept hidden for the next of their arm.
const SPARE: usize = 8;

/// Whether pad `i` holds its gun, as the last snapshot says.
fn loaded(core: &NetCore, i: usize) -> bool {
    core.latest().pads().get(i).is_none_or(|p| p.empty_s == 0)
}

/// Each gun's material with the glow of one lying for the taking, by the
/// gun's own: one copy however many lie about.
#[derive(Resource, Default)]
pub struct Glows(HashMap<AssetId<StandardMaterial>, Handle<StandardMaterial>>);

/// A lying gun's (or grenade's) scene has spawned: its materials take the
/// glow. A sword's blade is light already.
fn glow(
    ev: On<SceneInstanceReady>,
    children: Query<&Children>,
    plain: Query<(&MeshMaterial3d<StandardMaterial>, Option<&GltfMaterialName>)>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut glows: ResMut<Glows>,
    mut commands: Commands,
) {
    for e in children.iter_descendants(ev.entity) {
        let Ok((m, name)) = plain.get(e) else {
            continue;
        };
        if crate::look::blade::is_blade(name) {
            continue;
        }
        let handle = match glows.0.get(&m.0.id()) {
            Some(h) => h.clone(),
            None => {
                let Some(mut lit) = mats.get(&m.0).cloned() else {
                    continue;
                };
                lit.emissive = LYING_GLOW;
                let h = mats.add(lit);
                glows.0.insert(m.0.id(), h.clone());
                h
            }
        };
        commands.entity(e).insert(MeshMaterial3d(handle));
    }
}

/// Grenades lying under `root`, at `at`, glowing.
fn frag(commands: &mut Commands, models: &Models, at: Transform, root: Entity) {
    commands
        .spawn((SceneRoot(models.frag.clone()), at, ChildOf(root)))
        .observe(glow);
}

/// `arm`'s gun under `root`, turned about its middle rather than its grip,
/// glowing.
fn posed(commands: &mut Commands, models: &Models, arm: u8, basis: Quat, root: Entity) {
    if let (Some(g), Some((pivot, scene))) = (guns::gun(arm), guns::spawn_posed(commands, models, arm, basis, root)) {
        crate::look::blade::light(commands, scene, arm);
        commands.entity(scene).observe(glow);
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
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    plasma: Option<Res<crate::look::fx::plasma::Kit>>,
    gen: Option<Res<crate::front::LinkGen>>,
) {
    let s = link.0.session();
    if !s.ready {
        return;
    }
    let core = &s.core;
    // Keyed as the level is (`Built`): the title's stand-in and a match's
    // first welcome are both welcome 1.
    let key = Some((gen.map_or(0, |g| g.0), s.welcomes));
    if key != items.built {
        // The level's parts go with the level (`level::build`).
        items.built = key;
        items.drops = [None; MAX_DROPS];
        items.spare.clear();
        for (i, p) in core.map.pads.iter().enumerate() {
            // On the floor under where it spawns, each pad turned its own
            // way; an orb resting on it.
            let orb = sim::arms::power(p.arm);
            let lift = match p.arm {
                _ if orb => ORB_R,
                sim::arms::STICKIES => crate::look::fx::plasma::RADIUS_M,
                _ => LIE_Y,
            };
            let y = core.map.floor(p.pos.x, p.pos.z, 0.0, p.pos.y + 0.5);
            let root = commands
                .spawn((
                    crate::level::LevelPart,
                    PadGun(i),
                    Transform::from_xyz(p.pos.x, y + lift, p.pos.z)
                        .with_rotation(Quat::from_rotation_y(i as f32 * 2.4 + 0.7)),
                    Visibility::Hidden,
                ))
                .id();
            if p.arm == sim::arms::FRAGS {
                for side in [-1.0, 1.0] {
                    let at = Transform::from_xyz(side * PAD_FRAG_APART_M * 0.5, 0.0, 0.0);
                    frag(&mut commands, &models, at, root);
                }
            } else if p.arm == sim::arms::STICKIES {
                if let Some(kit) = &plasma {
                    commands
                        .entity(root)
                        .with_children(|c| crate::look::fx::plasma::spawn_lying(c, kit));
                }
            } else if orb {
                let c = orb_color(p.arm);
                commands.spawn((
                    Mesh3d(meshes.add(Sphere::new(ORB_R))),
                    MeshMaterial3d(mats.add(StandardMaterial {
                        base_color: Color::LinearRgba(c * 0.2),
                        emissive: c * ORB_GLOW,
                        perceptual_roughness: 0.2,
                        ..default()
                    })),
                    Transform::default(),
                    bevy::light::NotShadowCaster,
                    ChildOf(root),
                ));
            } else {
                let side = Quat::from_rotation_x(core::f32::consts::FRAC_PI_2);
                posed(&mut commands, &models, p.arm, side * guns::body_basis(), root);
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
        // A gun lies still: no write to turn its scene over. An orb
        // breathes, each a little out of step with the next.
        if on && sim::arms::power(def.arm) {
            let phase = pad.0 as f32 * 1.7;
            tr.scale = Vec3::splat(1.0 + ORB_PULSE * (t * ORB_HZ * core::f32::consts::TAU + phase).sin());
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
    let Items { drops, spare, .. } = &mut *items;
    for (slot, (have, want)) in drops.iter_mut().zip(want).enumerate() {
        if let (Some(l), Some((a, p))) = (have.as_mut(), want) {
            if l.arm == a && l.to.distance(p) < DROP_SNAP_M {
                l.to = p;
                fly(l, dt);
                // Settled, it stays put: no write to turn its scene over.
                let rot = l.spin * l.turn;
                if let Ok(mut tr) = placed.get_mut(l.e) {
                    if tr.translation != l.at || tr.rotation != rot {
                        tr.translation = l.at;
                        tr.rotation = rot;
                    }
                }
                continue;
            }
        }
        if let Some(l) = have.take() {
            commands.entity(l.e).insert(Visibility::Hidden);
            if spare.len() >= SPARE {
                commands.entity(spare.remove(0).1).despawn();
            }
            spare.push((l.arm, l.e));
        }
        let Some((a, p)) = want else {
            continue;
        };
        // On its side, each slot turned its own way.
        let turn = Quat::from_rotation_y(slot as f32 * 2.4);
        let at = Transform::from_translation(p).with_rotation(turn);
        let root = if let Some(i) = spare.iter().position(|s| s.0 == a) {
            let (_, e) = spare.remove(i);
            commands.entity(e).insert((at, Visibility::Inherited));
            e
        } else {
            let root = commands
                .spawn((crate::level::LevelPart, at, Visibility::default()))
                .id();
            if a == NONE {
                frag(&mut commands, &models, Transform::default(), root);
            } else {
                let side = Quat::from_rotation_x(core::f32::consts::FRAC_PI_2);
                posed(&mut commands, &models, a, side * guns::body_basis(), root);
            }
            root
        };
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
pub fn offer(core: &NetCore, vehicles: &[client_net::core::DrawnVehicle]) -> Option<Offer> {
    let f: &Fighter = core.own().0;
    if !f.alive || f.riding().is_some() || core.phase() != Phase::Playing {
        return None;
    }
    let at = f.body.pos;
    let car = vehicles
        .iter()
        .filter(|v| v.seats.contains(&0) && (v.pos - at).flat().len() < ENTER_R && (v.pos.y - at.y).abs() < 2.0)
        .min_by(|a, b| (a.pos - at).flat().len().total_cmp(&(b.pos - at).flat().len()));
    if let Some(v) = car {
        // The seat the sim would give us from here (`vehicle::boards`).
        let turret = core.map.vehicles.get(v.slot as usize).map_or(0, |d| d.turret);
        let ahead = (at.x - v.pos.x) * v.yaw.cos() + (at.z - v.pos.z) * v.yaw.sin();
        if let Some(seat) = sim::vehicle::boards(turret, v.seats, ahead, f.flag) {
            let verb = match seat {
                DRIVER => "drive",
                _ if turret != sim::vehicle::NO_TURRET => "gun",
                _ => "ride",
            };
            return Some(Offer { verb, gun: "" });
        }
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
        .filter(|&(i, p)| loaded(core, i) && !sim::arms::power(p.arm))
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
fn take(f: &Fighter, id: u8, pad: bool) -> Option<Offer> {
    let a = arm(id);
    let verb = if a.dual && arm(f.gun().arm).dual {
        "dual wield"
    } else if f.guns.iter().any(|g| g.arm == id) {
        if !pad {
            return None;
        }
        "refill"
    } else {
        "pick up"
    };
    Some(Offer { verb, gun: a.name })
}

/// What USE would do ([`offer`]): "drive", "gun", "ride", or "pick up" and the gun's
/// name. Nothing made to hold it: the prompt writes it when it changes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Offer {
    pub verb: &'static str,
    pub gun: &'static str,
}

impl std::fmt::Display for Offer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.verb)?;
        if !self.gun.is_empty() {
            write!(f, " {}", self.gun)?;
        }
        Ok(())
    }
}

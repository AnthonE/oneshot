//! A truck's turret (`sim::vehicle::turret`): a minigun on a ring its
//! gunner stands in, on the bed, where the sim fires it from
//! (`Vehicle::pivot`: `TURRET_RING` out from the gunner the way it aims,
//! `TURRET_Y` up; its barrels' ends `TURRET_REACH` out along the aim), so
//! its grips are in the gunner's hands whichever way it turns. The gun is
//! Meshy's, cut in two by `tools/models/turret.py`: the cradle
//! (`models/turret.glb`), which pitches, and the barrels
//! (`models/turret-barrels.glb`), which spin; the ring, its struts, the
//! carriage on it and the yoke are plain shapes. A child of the truck's
//! body, so it leans and turns over with it. It turns and its barrels rise to
//! where its gunner aims (ours as we look, anyone else's as they are drawn;
//! with nobody on it, level where it was left), and its barrels spin up as
//! the trigger is held and wind down after.
//!
//! **It runs hot where you can see it.** The barrels glow from the muzzle
//! back (a map baked by `turret.py`) as the gun heats, dull red to orange;
//! hot and let go, they smoke; run to its vent, it blows smoke off the
//! receiver and its barrels smoke till it is cool. Ours as the sim heats
//! it; anyone else's estimated from the rounds seen leaving it, by the
//! gun's own rates. Looks only: what fires is the sim's.

use bevy::prelude::*;
use bevy::scene::SceneInstanceReady;
use sim::arms::{Arm, HEAT_FULL, MINIGUN as GUN};
use sim::consts::{ticks, TURRET_GUNNER_X, TURRET_REACH, TURRET_RING, TURRET_Y};
use sim::events::{Cause, Kind};
use sim::vehicle::{MINIGUN, PASSENGER};

use super::fx::table::Layer;
use super::fx::Fx;
use super::guns::Models;
use crate::LinkRes;

/// The bed's floor, metres off the ground (the sim stands its gunner
/// there: `Vehicle::mount`).
const BED_Y: f32 = 0.9;
/// The ring about the gunner: its radius, its height (the gunner's hips),
/// its tube; on `STRUTS` struts down to the bed.
const RING_R: f32 = 0.52;
const RING_Y: f32 = 1.6;
const RING_TUBE: f32 = 0.035;
const STRUTS: usize = 4;
const STRUT_R: f32 = 0.025;
/// The gun's own numbers, what `turret.py` prints (`traces/guns/turret.json`):
/// where its barrels' rear sits ahead of the pivot, and how far under and
/// either side of its axis the cradle reaches, metres.
const BARRELS_AT: f32 = 0.2268;
const CRADLE_UNDER: f32 = 0.28;
const CRADLE_HALF_W: f32 = 0.22;
/// The yoke: arms either side of the cradle up to its trunnions, on a bar
/// under it, on a carriage that rides the ring.
const YOKE_ARM: f32 = 0.05;
const YOKE_GAP: f32 = 0.04;
/// Turns a second at full spin, and how fast the spin comes and goes
/// (per second) as drawn.
const SPIN_RPS: f32 = 9.0;
const SPIN_EASE: f32 = 8.0;
/// A gunner seen firing keeps it spinning this long past its last round.
const FIRING_S: f32 = 0.2;
/// How fast its head comes to its gunner's aim, and, left alone, its
/// barrels come level, per second.
const AIM_EASE: f32 = 30.0;
const REST_EASE: f32 = 2.0;
/// The barrels' glow: none below `GLOW_FROM` of full heat, then up to
/// `GLOW_MAX` (times the colour) at full, dull red turning orange.
const GLOW_FROM: f32 = 0.2;
const GLOW_MAX: f32 = 2.2;
const GLOW_DULL: [f32; 3] = [1.0, 0.06, 0.01];
const GLOW_HOT: [f32; 3] = [1.0, 0.3, 0.05];
/// Over `SMOKE_FROM` of full heat and not firing, its muzzle smokes, a
/// puff every `SMOKE_EVERY_S` at full, rarer as it cools; venting, every
/// `VENT_EVERY_S`, and the vent's first breath off the receiver is
/// `VENT_PUFFS` big ones, either side.
const SMOKE_FROM: f32 = 0.35;
const SMOKE_EVERY_S: f32 = 0.09;
const VENT_EVERY_S: f32 = 0.05;
const VENT_PUFFS: usize = 3;
/// A breath of heat shimmer rises this fast, m/s.
const SMOKE_RISE: f32 = 1.2;

/// The meshes and paint every turret's mount shares.
#[derive(Resource)]
pub struct Kit {
    ring: Handle<Mesh>,
    strut: Handle<Mesh>,
    carriage: Handle<Mesh>,
    bar: Handle<Mesh>,
    arm: Handle<Mesh>,
    boss: Handle<Mesh>,
    steel: Handle<StandardMaterial>,
    dark: Handle<StandardMaterial>,
}

/// One truck's turret: the vehicle it is on, its moving parts, and how
/// they are drawn now (radians, in the hull's frame; turns a second).
#[derive(Component)]
pub struct Turret {
    slot: u8,
    head: Entity,
    cradle: Entity,
    barrels: Entity,
    yaw: f32,
    pitch: f32,
    spin: f32,
    roll: f32,
    /// Seconds since its gunner was seen firing.
    since: f32,
    /// Its heat as drawn, 0 to 1, and whether it is venting.
    heat: f32,
    vent: bool,
    /// Seconds till its next breath of smoke.
    puff: f32,
    /// Its barrels' paint, its own, lit by its heat (empty till the scene
    /// is in), and the glow it was last given.
    glow: Vec<Handle<StandardMaterial>>,
    lit: f32,
}

pub fn plugin(app: &mut App) {
    app.add_systems(Startup, kit);
}

fn kit(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    let reach = CRADLE_UNDER + YOKE_GAP;
    let (from, to) = carriage();
    commands.insert_resource(Kit {
        ring: meshes.add(Torus::new(RING_R - RING_TUBE, RING_R + RING_TUBE)),
        strut: meshes.add(Cylinder::new(STRUT_R, RING_Y - BED_Y)),
        carriage: meshes.add(Cuboid::new(to.x - from.x, to.y - from.y, 0.14)),
        bar: meshes.add(Cuboid::new(0.16, YOKE_ARM, 2.0 * (CRADLE_HALF_W + YOKE_GAP) + YOKE_ARM)),
        arm: meshes.add(Cuboid::new(0.12, reach + YOKE_ARM, YOKE_ARM)),
        boss: meshes.add(Cylinder::new(0.055, YOKE_ARM + 0.02)),
        steel: mats.add(StandardMaterial {
            base_color: Color::srgb(0.11, 0.115, 0.12),
            metallic: 0.85,
            perceptual_roughness: 0.42,
            ..default()
        }),
        dark: mats.add(StandardMaterial {
            base_color: Color::srgb(0.2, 0.22, 0.14),
            metallic: 0.2,
            perceptual_roughness: 0.7,
            ..default()
        }),
    });
}

/// The carriage's corners in the turret's turning frame: from the ring's
/// rim to under the yoke's bar.
fn carriage() -> (Vec3, Vec3) {
    let bar = TURRET_Y - CRADLE_UNDER - YOKE_GAP - YOKE_ARM * 0.5;
    (
        Vec3::new(RING_R - 0.06, RING_Y - 0.04, 0.0),
        Vec3::new(TURRET_RING + 0.08, bar, 0.0),
    )
}

/// A turret of `kind` on `truck` (a truck's body: `truck::spawn`), vehicle
/// `slot`'s. Only the minigun so far; nothing for no turret.
pub fn spawn(commands: &mut Commands, kit: &Kit, models: &Models, truck: Entity, slot: u8, kind: u8) {
    if kind != MINIGUN {
        return;
    }
    let part =
        |commands: &mut Commands, mesh: &Handle<Mesh>, mat: &Handle<StandardMaterial>, t: Transform, of: Entity| {
            commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), t, ChildOf(of)));
        };
    let root = commands
        .spawn((
            Transform::from_xyz(TURRET_GUNNER_X, 0.0, 0.0),
            Visibility::default(),
            ChildOf(truck),
        ))
        .id();
    // The ring, standing on its struts about the gunner.
    part(
        commands,
        &kit.ring,
        &kit.steel,
        Transform::from_xyz(0.0, RING_Y, 0.0),
        root,
    );
    for k in 0..STRUTS {
        let a = (k as f32 + 0.5) * core::f32::consts::TAU / STRUTS as f32;
        part(
            commands,
            &kit.strut,
            &kit.steel,
            Transform::from_xyz(a.cos() * RING_R, (BED_Y + RING_Y) * 0.5, a.sin() * RING_R),
            root,
        );
    }
    // What turns: the carriage riding the ring, out to the yoke.
    let head = commands
        .spawn((Transform::default(), Visibility::default(), ChildOf(root)))
        .id();
    let (from, to) = carriage();
    part(
        commands,
        &kit.carriage,
        &kit.dark,
        Transform::from_translation((from + to) * 0.5),
        head,
    );
    let trunnion = commands
        .spawn((
            Transform::from_xyz(TURRET_RING, TURRET_Y, 0.0),
            Visibility::default(),
            ChildOf(head),
        ))
        .id();
    // The yoke: a bar under the cradle and an arm up either side to the
    // trunnions, a boss on each.
    let below = -(CRADLE_UNDER + YOKE_GAP);
    part(
        commands,
        &kit.bar,
        &kit.dark,
        Transform::from_xyz(0.0, below, 0.0),
        trunnion,
    );
    let across = Quat::from_rotation_x(core::f32::consts::FRAC_PI_2);
    for side in [-1.0, 1.0] {
        let z = side * (CRADLE_HALF_W + YOKE_GAP);
        part(
            commands,
            &kit.arm,
            &kit.dark,
            Transform::from_xyz(0.0, below * 0.5, z),
            trunnion,
        );
        part(
            commands,
            &kit.boss,
            &kit.steel,
            Transform::from_xyz(0.0, 0.0, z).with_rotation(across),
            trunnion,
        );
    }
    let cradle = commands
        .spawn((
            SceneRoot(models.turret.clone()),
            Transform::default(),
            Visibility::default(),
            ChildOf(trunnion),
        ))
        .id();
    let barrels = commands
        .spawn((
            SceneRoot(models.turret_barrels.clone()),
            Transform::from_xyz(BARRELS_AT, 0.0, 0.0),
            Visibility::default(),
            ChildOf(cradle),
        ))
        .observe(bind)
        .id();
    commands.entity(root).insert(Turret {
        slot,
        head,
        cradle,
        barrels,
        yaw: 0.0,
        pitch: 0.0,
        spin: 0.0,
        roll: 0.0,
        since: f32::MAX,
        heat: 0.0,
        vent: false,
        puff: 0.0,
        glow: Vec::new(),
        lit: -1.0,
    });
}

/// The barrels are in: their paint, this turret's own, dark till it heats.
fn bind(
    ev: On<SceneInstanceReady>,
    children: Query<&Children>,
    skins: Query<&MeshMaterial3d<StandardMaterial>>,
    mut turrets: Query<&mut Turret>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    let Some(mut t) = turrets.iter_mut().find(|t| t.barrels == ev.entity) else {
        return;
    };
    for e in children.iter_descendants(ev.entity) {
        let Ok(skin) = skins.get(e) else { continue };
        let Some(base) = mats.get(&skin.0).cloned() else {
            continue;
        };
        let own = mats.add(StandardMaterial {
            emissive: LinearRgba::BLACK,
            ..base
        });
        commands.entity(e).insert(MeshMaterial3d(own.clone()));
        t.glow.push(own);
    }
    // Lit afresh by `aim`, whatever it last lit.
    t.lit = -1.0;
}

/// A turret gun's heat as another's is drawn: up by its rounds while it is
/// seen firing, down by its cooling, and to its vent from full.
fn estimate(a: &Arm, heat: &mut f32, vent: &mut bool, firing: bool, dt: f32) {
    let per_s = |pct: u16| pct as f32 / 100.0 * dt;
    if *vent {
        *heat -= per_s(a.heat_vent);
        *vent = *heat > 0.0;
    } else {
        if firing {
            *heat += per_s(a.heat_shot) * 1000.0 / a.refire_ms.max(1) as f32;
        }
        *heat -= per_s(a.heat_cool);
        *vent = *heat >= 1.0;
    }
    *heat = heat.clamp(0.0, 1.0);
}

/// Every turret to its gunner's aim and its trigger, its heat lit and
/// smoking. After the trucks' bodies are posed (`truck::drive`).
#[allow(clippy::too_many_arguments)]
pub fn aim(
    link: NonSend<LinkRes>,
    look: Res<crate::Look>,
    seen: Res<crate::Seen>,
    heard: Res<crate::sound::Heard>,
    time: Res<Time>,
    posed: Option<Res<crate::pose::Pose>>,
    mut fx: ResMut<Fx>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut turrets: Query<&mut Turret>,
    mut parts: Query<(&mut Transform, &GlobalTransform), Without<Turret>>,
) {
    let dt = time.delta_secs().min(0.1);
    if dt <= 0.0 {
        return;
    }
    let core = &link.0.session().core;
    let (me, _) = core.own();
    let spun = ticks(GUN.spin_ms).max(1) as f32;
    for mut t in &mut turrets {
        let slot = t.slot;
        let ours = me.riding() == Some((slot as usize, PASSENGER));
        // The hull's heading, and who guns it.
        let (hull_yaw, gunner) = if ours {
            (core.own_vehicle().map_or(0.0, |v| v.2), Some(core.slot))
        } else {
            match seen.vehicles.iter().find(|v| v.slot == slot) {
                Some(v) => (v.yaw, v.seats[PASSENGER].checked_sub(1)),
                // The pose's truck: its own heading, the posed fighter on
                // its gun if it was put in the bed (`seat=1`).
                None if posed.is_some() => (
                    posed.as_ref().and_then(|p| p.2.truck).map_or(0.0, |t| t.1),
                    posed.as_ref().filter(|p| p.2.seat == Some(PASSENGER)).map(|p| p.0.slot),
                ),
                None => continue,
            }
        };
        let aim = if ours {
            Some((look.yaw, look.pitch))
        } else if let Some(p) = posed.as_ref().filter(|p| gunner == Some(p.0.slot)) {
            Some((p.0.yaw, p.0.pitch))
        } else {
            gunner.and_then(|g| {
                seen.others
                    .iter()
                    .find(|d| d.slot == g && d.alive)
                    .map(|d| (d.yaw, d.pitch))
            })
        };
        if let Some(g) = gunner {
            let fired = heard
                .0
                .iter()
                .any(|e| e.kind == Kind::Shot as u8 && e.a == g && e.c == Cause::Minigun as u16);
            t.since = if fired { 0.0 } else { t.since + dt };
        } else {
            t.since = f32::MAX;
        }
        // A posed one, drawn as hot as it is told, firing.
        let posed_heat = posed.as_ref().map_or(0.0, |p| p.2.turret_heat);
        let firing = t.since < FIRING_S || posed_heat > 0.0;
        // Spinning: ours as the sim spins it, anyone else's while seen firing.
        let want = if ours {
            me.mount.spin as f32 / spun
        } else if firing {
            1.0
        } else {
            0.0
        };
        t.spin += (want * SPIN_RPS - t.spin) * (SPIN_EASE * dt).min(1.0);
        t.roll = (t.roll + t.spin * core::f32::consts::TAU * dt) % core::f32::consts::TAU;
        match aim {
            Some((yaw, pitch)) => {
                let mut d = (yaw - hull_yaw) - t.yaw;
                d = (d + core::f32::consts::PI).rem_euclid(core::f32::consts::TAU) - core::f32::consts::PI;
                let k = (AIM_EASE * dt).min(1.0);
                t.yaw += d * k;
                t.pitch += (pitch - t.pitch) * k;
            }
            None => t.pitch -= t.pitch * (REST_EASE * dt).min(1.0),
        }
        if let Ok((mut tr, _)) = parts.get_mut(t.head) {
            tr.rotation = Quat::from_rotation_y(-t.yaw);
        }
        if let Ok((mut tr, _)) = parts.get_mut(t.cradle) {
            tr.rotation = Quat::from_rotation_z(t.pitch);
        }
        let Ok((mut tr, at)) = parts.get_mut(t.barrels) else {
            continue;
        };
        tr.rotation = Quat::from_rotation_x(t.roll);
        let (axis, from) = (at.right().as_vec3(), at.translation());

        // Its heat: ours the sim's, anyone else's as it is seen firing.
        let was_venting = t.vent;
        if ours {
            t.heat = me.mount.heat as f32 / HEAT_FULL as f32;
            t.vent = me.mount.vent;
        } else if posed_heat > 0.0 {
            t.heat = posed_heat;
        } else {
            let (mut h, mut v) = (t.heat, t.vent);
            estimate(&GUN, &mut h, &mut v, firing, dt);
            (t.heat, t.vent) = (h, v);
        }
        let lit = ((t.heat - GLOW_FROM) / (1.0 - GLOW_FROM)).clamp(0.0, 1.0);
        let lit = lit * lit;
        if (lit - t.lit).abs() > 0.004 {
            t.lit = lit;
            let c = |i: usize| GLOW_DULL[i] + (GLOW_HOT[i] - GLOW_DULL[i]) * lit;
            let glow = LinearRgba::rgb(c(0), c(1), c(2)) * (GLOW_MAX * lit);
            for h in &t.glow {
                if let Some(m) = mats.get_mut(h) {
                    m.emissive = glow;
                }
            }
        }

        // Smoke: off the muzzle while it is hot and quiet, thick while it
        // vents; the vent's first breath out of the receiver's sides.
        let muzzle = from + axis * (TURRET_REACH - BARRELS_AT);
        if t.vent && !was_venting {
            let side = axis.cross(Vec3::Y).normalize_or(Vec3::Z);
            for s in [-1.0, 1.0] {
                fx.emit_tint(
                    Layer::GunSmoke,
                    VENT_PUFFS,
                    from - axis * 0.15 + side * s * 0.2,
                    side * s + Vec3::Y * 0.5,
                    Color::WHITE,
                    3.0,
                );
            }
            fx.emit_tint(Layer::Wisp, 2, muzzle, Vec3::Y, Color::WHITE, 1.6);
        }
        let hot = (t.heat - SMOKE_FROM) / (1.0 - SMOKE_FROM);
        let smokes = t.vent || (hot > 0.0 && !firing);
        t.puff -= dt;
        if smokes && t.puff <= 0.0 {
            let every = if t.vent {
                VENT_EVERY_S
            } else {
                SMOKE_EVERY_S / hot.max(0.15)
            };
            t.puff = every * fx.range(0.7, 1.3);
            let along = fx.range(0.25, 1.0);
            let at = from + axis * (TURRET_REACH - BARRELS_AT) * along;
            let size = if t.vent { 1.6 } else { 0.8 + 0.6 * hot };
            fx.emit_tint(Layer::GunSmoke, 1, at, Vec3::Y * SMOKE_RISE, Color::WHITE, size);
        } else if !smokes {
            t.puff = 0.0;
        }
    }
}

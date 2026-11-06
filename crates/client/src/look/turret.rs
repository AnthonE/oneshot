//! A truck's turret (`sim::vehicle::turret`): a minigun on a post on its
//! bed, where the sim fires it from (`TURRET_X`, `TURRET_Y`, its barrels'
//! ends `TURRET_REACH` out along the aim). Built here of plain shapes, ours,
//! till it has a model; a child of the truck's body, so it leans and turns
//! over with it. Its head turns and its barrels rise to where its gunner
//! aims (ours as we look, anyone else's as they are drawn; with nobody on
//! it, level where it was left), and its barrels spin up as the trigger
//! is held and wind down after. Looks only: what fires is the sim's.

use bevy::prelude::*;
use sim::consts::{ticks, TURRET_REACH, TURRET_X, TURRET_Y};
use sim::events::{Cause, Kind};
use sim::vehicle::{MINIGUN, PASSENGER};

use crate::LinkRes;

/// The bed's floor under the post, metres off the ground (the sim stands
/// its gunner there: `Vehicle::mount`).
const BED_Y: f32 = 0.9;
const POST_R: f32 = 0.06;
/// Its barrels: six about the spindle, this far off it, this thick; from
/// this far ahead of the pivot to the muzzle.
const BARRELS: usize = 6;
const BARREL_RING: f32 = 0.05;
const BARREL_R: f32 = 0.021;
const BARREL_FROM: f32 = 0.27;
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

/// The meshes and paint every turret shares.
#[derive(Resource)]
pub struct Kit {
    post: Handle<Mesh>,
    plate: Handle<Mesh>,
    yoke: Handle<Mesh>,
    receiver: Handle<Mesh>,
    shield: Handle<Mesh>,
    can: Handle<Mesh>,
    grip: Handle<Mesh>,
    barrel: Handle<Mesh>,
    housing: Handle<Mesh>,
    clamp: Handle<Mesh>,
    steel: Handle<StandardMaterial>,
    dark: Handle<StandardMaterial>,
    drab: Handle<StandardMaterial>,
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
}

pub fn plugin(app: &mut App) {
    app.add_systems(Startup, kit);
}

fn kit(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    let post_h = TURRET_Y - BED_Y;
    let barrel_l = TURRET_REACH - BARREL_FROM;
    commands.insert_resource(Kit {
        post: meshes.add(Cylinder::new(POST_R, post_h)),
        plate: meshes.add(Cuboid::new(0.42, 0.04, 0.42)),
        yoke: meshes.add(Cuboid::new(0.14, 0.2, 0.34)),
        receiver: meshes.add(Cuboid::new(0.56, 0.19, 0.17)),
        shield: meshes.add(Cuboid::new(0.03, 0.44, 0.64)),
        can: meshes.add(Cuboid::new(0.26, 0.17, 0.13)),
        grip: meshes.add(Cuboid::new(0.13, 0.045, 0.045)),
        barrel: meshes.add(Cylinder::new(BARREL_R, barrel_l)),
        housing: meshes.add(Cylinder::new(0.085, 0.16)),
        clamp: meshes.add(Cylinder::new(0.078, 0.035)),
        steel: mats.add(StandardMaterial {
            base_color: Color::srgb(0.085, 0.09, 0.095),
            metallic: 0.85,
            perceptual_roughness: 0.42,
            ..default()
        }),
        dark: mats.add(StandardMaterial {
            base_color: Color::srgb(0.025, 0.025, 0.025),
            perceptual_roughness: 0.9,
            ..default()
        }),
        drab: mats.add(StandardMaterial {
            base_color: Color::srgb(0.2, 0.22, 0.14),
            metallic: 0.2,
            perceptual_roughness: 0.7,
            ..default()
        }),
    });
}

/// A turret of `kind` on `truck` (a truck's body: `truck::spawn`), vehicle
/// `slot`'s. Only the minigun so far; nothing for no turret.
pub fn spawn(commands: &mut Commands, kit: &Kit, truck: Entity, slot: u8, kind: u8) {
    if kind != MINIGUN {
        return;
    }
    let part =
        |commands: &mut Commands, mesh: &Handle<Mesh>, mat: &Handle<StandardMaterial>, t: Transform, of: Entity| {
            commands
                .spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), t, ChildOf(of)))
                .id()
        };
    let root = commands
        .spawn((
            Transform::from_xyz(TURRET_X, 0.0, 0.0),
            Visibility::default(),
            ChildOf(truck),
        ))
        .id();
    let post_h = TURRET_Y - BED_Y;
    part(
        commands,
        &kit.plate,
        &kit.steel,
        Transform::from_xyz(0.0, BED_Y + 0.02, 0.0),
        root,
    );
    part(
        commands,
        &kit.post,
        &kit.steel,
        Transform::from_xyz(0.0, BED_Y + post_h * 0.5, 0.0),
        root,
    );
    let head = commands
        .spawn((
            Transform::from_xyz(0.0, TURRET_Y, 0.0),
            Visibility::default(),
            ChildOf(root),
        ))
        .id();
    part(
        commands,
        &kit.yoke,
        &kit.steel,
        Transform::from_xyz(0.0, -0.06, 0.0),
        head,
    );
    let cradle = commands
        .spawn((Transform::default(), Visibility::default(), ChildOf(head)))
        .id();
    part(
        commands,
        &kit.receiver,
        &kit.steel,
        Transform::from_xyz(0.02, 0.0, 0.0),
        cradle,
    );
    part(
        commands,
        &kit.shield,
        &kit.drab,
        Transform::from_xyz(0.24, 0.04, 0.0),
        cradle,
    );
    part(
        commands,
        &kit.can,
        &kit.drab,
        Transform::from_xyz(-0.02, -0.02, 0.16),
        cradle,
    );
    for z in [-0.08, 0.08] {
        part(
            commands,
            &kit.grip,
            &kit.dark,
            Transform::from_xyz(-0.31, -0.03, z),
            cradle,
        );
    }
    // Along +X: a cylinder stands on Y, laid down about Z.
    let along = Quat::from_rotation_z(-core::f32::consts::FRAC_PI_2);
    part(
        commands,
        &kit.housing,
        &kit.steel,
        Transform::from_xyz(0.3, 0.0, 0.0).with_rotation(along),
        cradle,
    );
    let barrels = commands
        .spawn((
            Transform::from_xyz(BARREL_FROM, 0.0, 0.0),
            Visibility::default(),
            ChildOf(cradle),
        ))
        .id();
    let barrel_l = TURRET_REACH - BARREL_FROM;
    for k in 0..BARRELS {
        let a = k as f32 * core::f32::consts::TAU / BARRELS as f32;
        let at = Vec3::new(barrel_l * 0.5, a.cos() * BARREL_RING, a.sin() * BARREL_RING);
        part(
            commands,
            &kit.barrel,
            &kit.steel,
            Transform::from_translation(at).with_rotation(along),
            barrels,
        );
    }
    for x in [barrel_l * 0.45, barrel_l - 0.04] {
        part(
            commands,
            &kit.clamp,
            &kit.dark,
            Transform::from_xyz(x, 0.0, 0.0).with_rotation(along),
            barrels,
        );
    }
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
    });
}

/// Every turret to its gunner's aim and its trigger. After the trucks'
/// bodies are posed (`truck::drive`).
pub fn aim(
    link: NonSend<LinkRes>,
    look: Res<crate::Look>,
    seen: Res<crate::Seen>,
    heard: Res<crate::sound::Heard>,
    time: Res<Time>,
    mut turrets: Query<&mut Turret>,
    mut parts: Query<&mut Transform, Without<Turret>>,
) {
    let dt = time.delta_secs().min(0.1);
    if dt <= 0.0 {
        return;
    }
    let core = &link.0.session().core;
    let (me, _) = core.own();
    let spun = ticks(sim::arms::MINIGUN.spin_ms).max(1) as f32;
    for mut t in &mut turrets {
        let slot = t.slot;
        let ours = me.riding() == Some((slot as usize, PASSENGER));
        // The hull's heading, and who guns it.
        let (hull_yaw, gunner) = if ours {
            (core.own_vehicle().map_or(0.0, |v| v.2), Some(core.slot))
        } else {
            match seen.vehicles.iter().find(|v| v.slot == slot) {
                Some(v) => (v.yaw, v.seats[PASSENGER].checked_sub(1)),
                None => continue,
            }
        };
        let aim = if ours {
            Some((look.yaw, look.pitch))
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
        // Spinning: ours as the sim spins it, anyone else's while seen firing.
        let want = if ours {
            me.mount.spin as f32 / spun
        } else if t.since < FIRING_S {
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
        if let Ok(mut tr) = parts.get_mut(t.head) {
            tr.rotation = Quat::from_rotation_y(-t.yaw);
        }
        if let Ok(mut tr) = parts.get_mut(t.cradle) {
            tr.rotation = Quat::from_rotation_z(t.pitch);
        }
        if let Ok(mut tr) = parts.get_mut(t.barrels) {
            tr.rotation = Quat::from_rotation_x(t.roll);
        }
    }
}

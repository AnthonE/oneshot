//! The truck: what a vehicle's hull is drawn as (`models/truck.glb`, made
//! from a download by `tools/models/truck.py`; CC-BY, NOTICE). The sim's
//! hull is a box that yaws over the ground (`sim/vehicle.rs`); the rest is
//! here and is only looks: the model in the hull's frame, its wheels
//! rolling with the ground it covers, the front pair steering with its
//! turn, and its body leaning with the ground under its wheels on a
//! terrain. Nothing that decides reads any of it.
//!
//! It is drawn at the size it came, a real truck's (5.7 m long, 2.1 wide,
//! 1.8 tall), because that is the size the sim's riders fit: the driver's
//! mount is in its cab and the passenger's in its bed. The sim's hull is
//! still the buggy's box (4.4 by 2.2 by 1.5, `VEH_HALF_L` and `VEH_H`), so
//! the nose, the tail and the top of the roof take no hits and stop at no
//! wall until those grow to it.

use bevy::prelude::*;
use bevy::scene::SceneInstanceReady;

use super::guns::Models;
use crate::LinkRes;

/// Where its wheels are in the hull's frame (+X forward, +Z its right),
/// metres, front pair first: what `truck.py` prints, slid by [`AT`].
pub const WHEELS: [(f32, f32); 4] = [
    (1.97 + AT.x, 0.888),
    (1.97 + AT.x, -0.888),
    (-1.771 + AT.x, 0.888),
    (-1.771 + AT.x, -0.888),
];
const WHEEL_R: f32 = 0.455;
const WHEELBASE: f32 = 3.741;
const TRACK: f32 = 1.776;
/// The model's place in the hull's frame: slid so the sim's driver
/// (`Vehicle::mount`: 0.3 m behind the middle, 0.45 to the left) is on its
/// driver's seat.
const AT: Vec3 = Vec3::new(-0.35, 0.0, 0.0);
/// The most the front wheels turn, radians; slower than `STEER_FROM` m/s
/// they straighten (a yaw with no speed is no steer to read).
const STEER_MAX: f32 = 0.55;
const STEER_FROM: f32 = 0.5;
/// How fast the steer and the lean settle, per second.
const EASE: f32 = 10.0;
/// The most the body leans, radians, and the most it rides off the hull's
/// own height, metres.
const LEAN_MAX: f32 = 0.6;
const LIFT_MAX: f32 = 0.5;
/// A hull that moved further than this in a frame was put there (a
/// respawn), not driven: its wheels don't spin for it.
const JUMP_M: f32 = 5.0;

/// A hull's model: how the hull stood last frame, and what its wheels and
/// body have come to.
#[derive(Component, Default)]
pub struct Truck {
    last: Option<(Vec3, Vec3)>,
    roll: f32,
    steer: f32,
    pitch: f32,
    bank: f32,
    lift: f32,
}

/// One of its wheels; a front one steers.
#[derive(Component)]
pub struct Wheel {
    front: bool,
}

/// The truck as a child of `hull`, an entity at the sim's hull turned by
/// its yaw.
pub fn spawn(commands: &mut Commands, models: &Models, hull: Entity) {
    commands
        .spawn((
            Truck::default(),
            SceneRoot(models.truck.clone()),
            Transform::from_translation(AT),
            ChildOf(hull),
        ))
        .observe(bind);
}

/// The scene is in: its wheels, by name.
fn bind(ev: On<SceneInstanceReady>, children: Query<&Children>, names: Query<&Name>, mut commands: Commands) {
    for e in children.iter_descendants(ev.entity) {
        let Ok(name) = names.get(e) else { continue };
        match name.as_str() {
            "wheel_fl" | "wheel_fr" => commands.entity(e).insert(Wheel { front: true }),
            "wheel_rl" | "wheel_rr" => commands.entity(e).insert(Wheel { front: false }),
            _ => continue,
        };
    }
}

/// Every truck by how its hull moved this frame: the wheels roll and
/// steer, the body leans to the ground under them. After the hulls are
/// placed (`draw_vehicles`).
#[allow(clippy::type_complexity)]
pub fn drive(
    link: NonSend<LinkRes>,
    time: Res<Time>,
    hulls: Query<&Transform, (Without<Truck>, Without<Wheel>)>,
    mut trucks: Query<(Entity, &mut Truck, &mut Transform, &ChildOf), Without<Wheel>>,
    children: Query<&Children>,
    mut wheels: Query<(&Wheel, &mut Transform), Without<Truck>>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    let map = &link.0.session().core.map;
    let ease = (EASE * dt).min(1.0);
    for (e, mut truck, mut body, &ChildOf(hull)) in &mut trucks {
        let Ok(at) = hulls.get(hull) else { continue };
        let (p, fwd, right) = (at.translation, at.rotation * Vec3::X, at.rotation * Vec3::Z);
        let (was_p, was_fwd) = truck.last.unwrap_or((p, fwd));
        truck.last = Some((p, fwd));
        let moved = p - was_p;
        let along = if moved.length() < JUMP_M { moved.dot(fwd) } else { 0.0 };
        truck.roll = (truck.roll + along / WHEEL_R) % core::f32::consts::TAU;
        // The steer its turn takes at its speed (about +Y: left is positive).
        let turned = was_fwd.cross(fwd).y.atan2(was_fwd.dot(fwd));
        let steer = if (along / dt).abs() > STEER_FROM {
            (WHEELBASE * turned / along).atan().clamp(-STEER_MAX, STEER_MAX)
        } else {
            0.0
        };
        truck.steer += (steer - truck.steer) * ease;
        // The ground under each wheel, while the hull is on it; in the air
        // the body keeps the lean it left with.
        if map.terrain.is_some() && p.y - map.ground(p.x, p.z) < crate::WHEELS_DOWN_M {
            let h = WHEELS.map(|(x, z)| {
                let w = p + fwd * x + right * z;
                map.ground(w.x, w.z)
            });
            let pitch = ((h[0] + h[1] - h[2] - h[3]) / 2.0).atan2(WHEELBASE);
            let bank = ((h[1] + h[3] - h[0] - h[2]) / 2.0).atan2(TRACK);
            let lift = h.iter().sum::<f32>() / 4.0 - p.y;
            truck.pitch += (pitch.clamp(-LEAN_MAX, LEAN_MAX) - truck.pitch) * ease;
            truck.bank += (bank.clamp(-LEAN_MAX, LEAN_MAX) - truck.bank) * ease;
            truck.lift += (lift.clamp(-LIFT_MAX, LIFT_MAX) - truck.lift) * ease;
        }
        *body = Transform::from_translation(AT + Vec3::Y * truck.lift)
            .with_rotation(Quat::from_rotation_z(truck.pitch) * Quat::from_rotation_x(truck.bank));
        for w in children.iter_descendants(e) {
            let Ok((wheel, mut tr)) = wheels.get_mut(w) else {
                continue;
            };
            let steer = if wheel.front { truck.steer } else { 0.0 };
            // Forward is +X and the axle +Z: the top goes forward about -Z.
            tr.rotation = Quat::from_rotation_y(steer) * Quat::from_rotation_z(-truck.roll);
        }
    }
}

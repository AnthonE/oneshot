//! The truck: what a vehicle's hull is drawn as (`models/truck.glb`, made
//! from a download by `tools/models/truck.py`; CC-BY, NOTICE). The sim's
//! hull is a box that yaws over the ground (`sim/vehicle.rs`); the rest is
//! here and is only looks: the model in the hull's frame, its wheels
//! rolling with the ground it covers, the front pair steering with its
//! turn, and its body leaning with the ground under its wheels on a
//! terrain. Nothing that decides reads any of it.
//!
//! It is drawn at the size it came, a real truck's (5.7 m long, 2.1 wide,
//! 1.8 tall), and the sim's hull is that box (`VEH_HALF_L`, `VEH_HALF_W`,
//! `VEH_H`), its middle the model's: what is drawn is what meets a wall,
//! and its steel is what takes a shot, to the windows' sill and the bed's
//! rail (`VEH_SILL`, `VEH_BED_H`); its glass stops nothing. The sim's seats
//! are its seats (`Vehicle::mount`): the driver in the cab's left one, head
//! and shoulders behind the glass, the passenger standing in the bed.
//!
//! **It wears.** Its steel and its trim are `StandardMaterial` plus a
//! fragment stage (`shaders/truck_wear.wgsl`, as the level's weathering
//! is): scratches, then scuffs and dirt, then soot, then char with embers
//! in it, by how much of the hull's health is gone. The health is sent
//! with the hull, so every client draws the same truck, and a new hull
//! (its respawn) is clean again. A round that meets its steel leaves a
//! hole where it did, a blast a scorch: the newest [`MARKS`] of them, kept
//! in the truck's own frame.

use bevy::gltf::GltfMaterialName;
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::scene::SceneInstanceReady;
use bevy::shader::ShaderRef;
use sim::consts::{VEH_H, VEH_HALF_L, VEH_HALF_W};
use sim::events::Kind;
use sim::geom::V3;

use super::fx::HEARD;
use super::guns::Models;
use crate::LinkRes;

/// Where its wheels are in the hull's frame (+X forward, +Z its right),
/// metres, front pair first: what `truck.py` prints.
pub const WHEELS: [(f32, f32); 4] = [(1.97, 0.888), (1.97, -0.888), (-1.771, 0.888), (-1.771, -0.888)];
const WHEEL_R: f32 = 0.455;
const WHEELBASE: f32 = 3.741;
const TRACK: f32 = 1.776;
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

/// Its steel and its trim, worn.
pub type TruckMaterial = ExtendedMaterial<StandardMaterial, Wear>;
pub const SHADER: &str = "shaders/truck_wear.wgsl";
/// What of it wears: its materials' names in `truck.glb`.
const WORN: [&str; 2] = ["steel", "gray"];
/// Marks a truck keeps (the shader's array); the newest takes the oldest's
/// place.
pub const MARKS: usize = 16;
/// How far a round's mark reaches, m; a blast's at the hull, less the
/// further off it went, and none past `SCORCH_FROM_M`.
const HOLE_M: f32 = 0.11;
const SCORCH_M: f32 = 0.6;
const SCORCH_FROM_M: f32 = 2.0;
/// A shot that ended within this of the hull's steel met it.
const MET_M: f32 = 0.3;
/// How fast its wear comes to what its health says, per second; and how
/// much better than it looks a hull must be to be a new one, clean and
/// unmarked.
const WEAR_EASE: f32 = 3.0;
const MENDED: f32 = 0.3;
/// The share of its health gone it burns from (`fx/hulls.rs`'s fire).
const BURN_FROM: f32 = 1.0 - super::fx::hulls::FIRE_BELOW as f32 / 255.0;

/// The wear's uniform; each lane is named in `truck_wear.wgsl`.
#[derive(Clone, Copy, Debug, Default, PartialEq, ShaderType)]
pub struct WearParams {
    pub to_local: Mat4,
    pub state: Vec4,
    pub marks: [Vec4; MARKS],
}

/// The extension. Not `#[bindless]`, as the level's isn't: one uniform a
/// truck, the base's own bindings beside it.
#[derive(Asset, AsBindGroup, TypePath, Clone, Debug)]
pub struct Wear {
    #[uniform(100)]
    pub params: WearParams,
}

impl MaterialExtension for Wear {
    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }
    /// Forward only: this client never runs deferred.
    fn deferred_fragment_shader() -> ShaderRef {
        ShaderRef::Default
    }
}

pub fn plugin(app: &mut App) {
    app.add_plugins(MaterialPlugin::<TruckMaterial>::default());
}

/// The sim's hull a truck is drawn for: which, and its health of 255 as
/// last sent (`draw_vehicles` keeps both).
#[derive(Component, Clone, Copy)]
pub struct Hull {
    pub slot: u8,
    pub health: u8,
}

/// A hull's model: how the hull stood last frame, what its wheels and body
/// have come to, and how worn it is drawn.
#[derive(Component, Default)]
pub struct Truck {
    last: Option<(Vec3, Vec3)>,
    roll: f32,
    steer: f32,
    pitch: f32,
    bank: f32,
    lift: f32,
    skins: Vec<Handle<TruckMaterial>>,
    hurt: f32,
    marks: [Vec4; MARKS],
    next: usize,
}

impl Truck {
    /// A mark at `at` in the truck's own frame, `reach` metres across (a
    /// blast's negative).
    fn mark(&mut self, at: Vec3, reach: f32) {
        self.marks[self.next] = at.extend(reach);
        self.next = (self.next + 1) % MARKS;
    }
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
            Transform::default(),
            ChildOf(hull),
        ))
        .observe(bind);
}

/// The scene is in: its wheels, by name, and its steel and trim in
/// materials of this truck's own, that wear.
#[allow(clippy::too_many_arguments)]
fn bind(
    ev: On<SceneInstanceReady>,
    children: Query<&Children>,
    names: Query<&Name>,
    skins: Query<(&MeshMaterial3d<StandardMaterial>, &GltfMaterialName)>,
    (parents, warming): (Query<&ChildOf>, Query<(), With<super::prewarm::Warming>>),
    mut trucks: Query<&mut Truck>,
    mats: Res<Assets<StandardMaterial>>,
    mut worn: ResMut<Assets<TruckMaterial>>,
    mut commands: Commands,
) {
    let Ok(mut truck) = trucks.get_mut(ev.entity) else {
        return;
    };
    for e in children.iter_descendants(ev.entity) {
        match names.get(e).map(Name::as_str) {
            Ok("wheel_fl" | "wheel_fr") => {
                commands.entity(e).insert(Wheel { front: true });
            }
            Ok("wheel_rl" | "wheel_rr") => {
                commands.entity(e).insert(Wheel { front: false });
            }
            _ => {}
        }
        let Ok((mat, name)) = skins.get(e) else { continue };
        let (true, Some(base)) = (WORN.contains(&name.0.as_str()), mats.get(&mat.0)) else {
            continue;
        };
        let skin = worn.add(TruckMaterial {
            base: base.clone(),
            extension: Wear {
                params: WearParams::default(),
            },
        });
        commands
            .entity(e)
            .remove::<MeshMaterial3d<StandardMaterial>>()
            .insert(MeshMaterial3d(skin.clone()));
        truck.skins.push(skin);
    }
    // A warm truck's scene is in: its warm draw may count down (`prewarm.rs`).
    if let Ok(&ChildOf(parent)) = parents.get(ev.entity) {
        if warming.contains(parent) {
            commands.entity(parent).insert(super::prewarm::SceneDone);
        }
    }
}

fn v(p: V3) -> Vec3 {
    Vec3::new(p.x, p.y, p.z)
}

/// Where a round met `steel` (the hull as the sim shapes what stops a
/// shot), in the world, if `ev` is a shot that did. Anyone else's shot
/// ends where the server's did, so it met the hull if it ends on it. Our
/// own (`ours`) is drawn as predicted, its end the map's: a hull on its
/// way took it.
fn holed(steel: &sim::vehicle::Vehicle, ev: &proto::EventState, ours: bool) -> Option<Vec3> {
    let arm = ev.c as usize;
    if ev.kind != Kind::Shot as u8 || ev.part == HEARD || arm == 0 || arm >= sim::arms::COUNT {
        return None;
    }
    // A rocket or a bolt leaves its mark as a blast, where it goes off.
    if sim::arms::arm(arm as u8).speed != 0.0 {
        return None;
    }
    let (from, to) = (v(ev.pos), v(ev.end));
    let len = from.distance(to);
    if len < 1e-3 {
        return None;
    }
    let d = (to - from) / len;
    let t = steel.ray(ev.pos, V3::new(d.x, d.y, d.z), len + MET_M)?;
    (ours || t > len - MET_M).then(|| from + d * t)
}

/// The scorch a blast at `at` (the truck's own frame) leaves: where on the
/// hull, and its reach (negative, as the shader tells a blast's), if it
/// went off near enough.
fn scorched(at: Vec3) -> Option<(Vec3, f32)> {
    let on = at.clamp(
        Vec3::new(-VEH_HALF_L, 0.3, -VEH_HALF_W),
        Vec3::new(VEH_HALF_L, VEH_H, VEH_HALF_W),
    );
    let off = at.distance(on);
    (off < SCORCH_FROM_M).then(|| (on, -SCORCH_M * (1.0 - 0.5 * off / SCORCH_FROM_M)))
}

/// Every truck by how its hull moved this frame: the wheels roll and
/// steer, the body leans to the ground under them; and by what happened to
/// it: its wear to its health, a mark where it was hit. After the hulls are
/// placed (`draw_vehicles`).
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn drive(
    link: NonSend<LinkRes>,
    time: Res<Time>,
    heard: Res<crate::sound::Heard>,
    posed: Option<Res<crate::pose::Pose>>,
    hulls: Query<(&Transform, Option<&Hull>), (Without<Truck>, Without<Wheel>)>,
    mut trucks: Query<(Entity, &mut Truck, &mut Transform, &ChildOf), Without<Wheel>>,
    children: Query<&Children>,
    mut wheels: Query<(&Wheel, &mut Transform), Without<Truck>>,
    mut worn: ResMut<Assets<TruckMaterial>>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    let core = &link.0.session().core;
    let map = &core.map;
    let ease = (EASE * dt).min(1.0);
    for (e, mut truck, mut body, &ChildOf(hull)) in &mut trucks {
        let Ok((at, hull)) = hulls.get(hull) else { continue };
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
        *body = Transform::from_translation(Vec3::Y * truck.lift)
            .with_rotation(Quat::from_rotation_z(truck.pitch) * Quat::from_rotation_x(truck.bank));
        for w in children.iter_descendants(e) {
            let Ok((wheel, mut tr)) = wheels.get_mut(w) else {
                continue;
            };
            let steer = if wheel.front { truck.steer } else { 0.0 };
            // Forward is +X and the axle +Z: the top goes forward about -Z.
            tr.rotation = Quat::from_rotation_y(steer) * Quat::from_rotation_z(-truck.roll);
        }

        // The world into its own frame, for what is drawn on it.
        let to_local = (at.to_matrix() * body.to_matrix()).inverse();
        // A new hull is clean: its health is back, more than a wear's ease.
        let want = 1.0 - hull.map_or(255, |h| h.health) as f32 / 255.0;
        if want < truck.hurt - MENDED {
            truck.hurt = want;
            truck.marks = [Vec4::ZERO; MARKS];
        }
        truck.hurt += (want - truck.hurt) * (WEAR_EASE * dt).min(1.0);
        // Where it was hit this frame: a round whose way ends on its steel
        // (the sim's own shape of it), a blast beside it.
        let steel = sim::vehicle::Vehicle::new(V3::new(p.x, p.y, p.z), sim::geom::rad_to_yaw(fwd.z.atan2(fwd.x)), 0);
        for ev in &heard.0 {
            if let Some(at) = holed(&steel, ev, ev.a == core.slot) {
                truck.mark(to_local.transform_point3(at), HOLE_M);
            } else if ev.kind == Kind::Blast as u8 {
                if let Some((at, reach)) = scorched(to_local.transform_point3(v(ev.pos))) {
                    truck.mark(at, reach);
                }
            }
        }
        // The pose mode's: as hurt as it was asked, and marked for looking.
        if let Some(view) = posed.as_ref().map(|p| p.2) {
            truck.hurt = view.truck_hurt;
            if view.truck_marks && truck.marks[0].w == 0.0 {
                for (at, reach) in POSED_MARKS {
                    truck.mark(at, reach);
                }
            }
        }
        let burnt = ((truck.hurt - BURN_FROM) / (1.0 - BURN_FROM)).clamp(0.0, 1.0);
        let params = WearParams {
            to_local,
            state: Vec4::new(truck.hurt, burnt, hull.map_or(0.0, |h| h.slot as f32), 0.0),
            marks: truck.marks,
        };
        for skin in &truck.skins {
            if worn.get(skin).is_some_and(|m| m.extension.params != params) {
                if let Some(m) = worn.get_mut(skin) {
                    m.extension.params = params;
                }
            }
        }
    }
}

/// The marks a posed truck is given (`truckmarks=1`): rounds along a door
/// and its nose, a blast on its flank and its tail.
const POSED_MARKS: [(Vec3, f32); 9] = [
    (Vec3::new(0.9, 0.95, 1.05), HOLE_M),
    (Vec3::new(0.55, 1.05, 1.05), HOLE_M),
    (Vec3::new(0.2, 0.85, 1.05), HOLE_M),
    (Vec3::new(-0.3, 1.0, 1.05), HOLE_M),
    (Vec3::new(1.3, 0.8, -1.05), HOLE_M),
    (Vec3::new(0.7, 1.0, -1.05), HOLE_M),
    (Vec3::new(2.8, 0.75, 0.3), HOLE_M),
    (Vec3::new(-1.6, 0.9, 1.05), -SCORCH_M),
    (Vec3::new(-2.86, 0.9, -0.2), -SCORCH_M),
];

#[cfg(test)]
mod tests {
    use sim::consts::VEH_SILL;

    use super::*;

    /// A shot with a rifle from `from` to `to`.
    fn shot(from: Vec3, to: Vec3) -> proto::EventState {
        proto::EventState {
            kind: Kind::Shot as u8,
            c: sim::arms::BR as u16,
            pos: V3::new(from.x, from.y, from.z),
            end: V3::new(to.x, to.y, to.z),
            ..proto::EventState::default()
        }
    }

    #[test]
    fn a_round_marks_the_steel_it_met_and_nothing_else() {
        // A hull at the origin facing +X: its right flank is the plane z = 1.1.
        let steel = sim::vehicle::Vehicle::new(V3::ZERO, 0, 0);
        let off = Vec3::new(0.5, 0.9, 12.0);
        let door = Vec3::new(0.5, 0.9, VEH_HALF_W);
        // Someone's shot that ended on the door: a hole there.
        let at = holed(&steel, &shot(off, door), false).expect("it met the door");
        assert!(at.distance(door) < 0.01, "{at}");
        // One that ended short of it (on whoever stood in the way): none.
        assert_eq!(holed(&steel, &shot(off, Vec3::new(0.5, 0.9, 4.0)), false), None);
        // One through the glass, over the sill, to the far side: none.
        let over = VEH_SILL + 0.2;
        let through = shot(Vec3::new(0.5, over, 12.0), Vec3::new(0.5, over, -12.0));
        assert_eq!(holed(&steel, &through, false), None);
        // Our own is drawn to the wall behind the hull: the door took it.
        let ours = shot(off, Vec3::new(0.5, 0.9, -30.0));
        let at = holed(&steel, &ours, true).expect("the hull was in its way");
        assert!(at.distance(door) < 0.01, "{at}");
        // Anyone else's that passed through where the hull is drawn did
        // not end on it: the server put the hull elsewhere.
        assert_eq!(holed(&steel, &ours, false), None);
        // A shot only heard has no place to mark.
        let heard = proto::EventState {
            part: HEARD,
            ..shot(off, door)
        };
        assert_eq!(holed(&steel, &heard, false), None);
    }

    #[test]
    fn a_blast_scorches_the_hull_beside_it() {
        // Half a metre off the right flank: a scorch on the flank, a blast's.
        let (at, reach) = scorched(Vec3::new(0.0, 1.0, VEH_HALF_W + 0.5)).expect("near enough");
        assert_eq!(at, Vec3::new(0.0, 1.0, VEH_HALF_W));
        assert!(reach < 0.0 && -reach < SCORCH_M);
        // Across the yard: none.
        assert_eq!(scorched(Vec3::new(0.0, 1.0, VEH_HALF_W + SCORCH_FROM_M + 1.0)), None);
    }
}

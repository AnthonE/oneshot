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
/// Below this hurt, unmarked and unburnt, no layer of its wear shows
/// (`truck_wear.wgsl`'s first smoothstep).
const PRISTINE: f32 = 0.03;
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
    app.add_plugins(MaterialPlugin::<TruckMaterial>::default())
        .init_resource::<HullSound>();
}

/// What the trucks sound like this frame, for the ear (`sound/mod.rs`):
/// the one-shots since it last took them, and the loops going now.
#[derive(Resource, Default)]
pub struct HullSound {
    /// What, where, how hard (0 to 1).
    pub hits: Vec<(Clunk, Vec3, f32)>,
    /// Tyres skidding: where, how hard.
    pub skids: Vec<(Vec3, f32)>,
    /// Fire on a hull or a husk: where, how big.
    pub fires: Vec<(Vec3, f32)>,
}

/// A truck's one-shots.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Clunk {
    /// Down on its wheels from the air.
    Land,
    /// Its nose or its tail into something, at speed.
    Crash,
    /// A husk or a wheel off it striking the ground.
    Bounce,
}

impl HullSound {
    fn hit(&mut self, what: Clunk, at: Vec3, k: f32) {
        // The ear takes them every frame; with none listening, the oldest go.
        if self.hits.len() >= 16 {
            self.hits.remove(0);
        }
        self.hits.push((what, at, k.clamp(0.0, 1.0)));
    }
}

/// The sim's hull a truck is drawn for: which, its health of 255 as last
/// sent, and whether anyone is at its wheel (`draw_vehicles` keeps them).
#[derive(Component, Clone, Copy)]
pub struct Hull {
    pub slot: u8,
    pub health: u8,
    pub driven: bool,
}

/// A hull's model: how the hull stood last frame, what its wheels and body
/// have come to, how it is turning over in the air, and how worn it is
/// drawn.
#[derive(Component)]
pub struct Truck {
    last: Option<(Vec3, Vec3)>,
    roll: f32,
    steer: f32,
    pitch: f32,
    bank: f32,
    lift: f32,
    /// Its velocity, smoothed, and the fastest it has gone of late.
    vel: Vec3,
    peak: f32,
    /// Seconds it has been off the ground, and the fastest it fell.
    air: f32,
    fall: f32,
    /// Turning over: its turn off upright (about its middle, in the hull's
    /// frame) and how fast it turns, rad/s.
    tumble: Quat,
    spin: Vec3,
    /// A blast's spin, waiting to see the hull leave the ground, and for
    /// how much longer.
    kick: Vec3,
    kick_left: f32,
    /// The body on its springs: its sway (bank, pitch) from turning and
    /// braking, and its squat on landing; each with its speed.
    sway: Vec2,
    sway_v: Vec2,
    squat: f32,
    squat_v: f32,
    /// Metres since its last track, its exhaust's carry, a scrape's rest.
    track: f32,
    puff: f32,
    scraped: f32,
    skins: Vec<Handle<TruckMaterial>>,
    /// Its four wheels, found once its scene is in (`bind`).
    wheels: Vec<Entity>,
    hurt: f32,
    marks: [Vec4; MARKS],
    next: usize,
    rng: u32,
}

impl Default for Truck {
    fn default() -> Truck {
        Truck {
            last: None,
            roll: 0.0,
            steer: 0.0,
            pitch: 0.0,
            bank: 0.0,
            lift: 0.0,
            vel: Vec3::ZERO,
            peak: 0.0,
            air: 0.0,
            fall: 0.0,
            tumble: Quat::IDENTITY,
            spin: Vec3::ZERO,
            kick: Vec3::ZERO,
            kick_left: 0.0,
            sway: Vec2::ZERO,
            sway_v: Vec2::ZERO,
            squat: 0.0,
            squat_v: 0.0,
            track: 0.0,
            puff: 0.0,
            scraped: 0.0,
            skins: Vec::new(),
            wheels: Vec::new(),
            hurt: 0.0,
            marks: [Vec4::ZERO; MARKS],
            next: 0,
            rng: 0x2545_f491,
        }
    }
}

impl Truck {
    /// A mark at `at` in the truck's own frame, `reach` metres across (a
    /// blast's negative).
    fn mark(&mut self, at: Vec3, reach: f32) {
        self.marks[self.next] = at.extend(reach);
        self.next = (self.next + 1) % MARKS;
    }

    /// In `[-1, 1)`: looks only.
    fn jitter(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng >> 8) as f32 / (1u32 << 23) as f32 - 1.0
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
                truck.wheels.push(e);
            }
            Ok("wheel_rl" | "wheel_rr") => {
                commands.entity(e).insert(Wheel { front: false });
                truck.wheels.push(e);
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

/// What a blast of cause `c` is as the sim sets it off: its damage, its
/// reach and how far it is whole.
fn charge(c: u16) -> Option<(f32, f32, f32)> {
    use sim::consts::*;
    use sim::events::Cause;
    Some(match c {
        c if c == Cause::Grenade as u16 => (GREN_DMG as f32, GREN_RADIUS, GREN_FULL),
        c if c == Cause::Sticky as u16 => (STICKY_DMG as f32, STICKY_RADIUS, STICKY_FULL),
        c if c == Cause::Wreck as u16 => (WRECK_DMG as f32, WRECK_RADIUS, WRECK_FULL),
        c if (c as usize) < sim::arms::COUNT => {
            let a = sim::arms::arm(c as u8);
            if a.radius <= 0.0 {
                return None;
            }
            (a.dmg as f32, a.radius, a.full)
        }
        _ => return None,
    })
}

/// The spin a blast at `at` (the hull's frame) gives a hull it throws: the
/// push up and away from it, about its middle from the side it struck. A
/// blast under its door rolls it over away from the blast; under its nose,
/// flips it end over end. Before the share of its whole (`heft`).
fn kicked(at: Vec3) -> Vec3 {
    let mid = Vec3::Y * MID_Y;
    let struck = at.clamp(
        Vec3::new(-VEH_HALF_L, 0.0, -VEH_HALF_W),
        Vec3::new(VEH_HALF_L, VEH_H, VEH_HALF_W),
    );
    let away = (mid - at).with_y(0.0).normalize_or_zero() * 0.7 + Vec3::Y;
    (struck - mid).cross(away) * SPIN_K
}

/// Every truck by how its hull moved this frame: the wheels roll and
/// steer, the body leans to the ground under them and sways on its
/// springs, turns over in the air after a blast and comes down on its
/// wheels; its tracks, dust, scrapes and exhaust; and by what happened to
/// it: its wear to its health, a mark where it was hit, a husk where it
/// blew up. After the hulls are placed (`draw_vehicles`).
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn drive(
    link: NonSend<LinkRes>,
    time: Res<Time>,
    heard: Res<crate::sound::Heard>,
    posed: Option<Res<crate::pose::Pose>>,
    hulls: Query<(&Transform, Option<&Hull>, Option<&Visibility>), (Without<Truck>, Without<Wheel>)>,
    mut trucks: Query<(&mut Truck, &mut Transform, &ChildOf), Without<Wheel>>,
    mut wheels: Query<(&Wheel, &mut Transform), Without<Truck>>,
    mut worn: ResMut<Assets<TruckMaterial>>,
    (mut fx, mut noise, models, mut commands): (ResMut<super::fx::Fx>, ResMut<HullSound>, Res<Models>, Commands),
) {
    noise.skids.clear();
    noise.fires.clear();
    let dt = time.delta_secs().min(0.1);
    if dt <= 0.0 {
        return;
    }
    let core = &link.0.session().core;
    let map = &core.map;
    let ease = (EASE * dt).min(1.0);
    for (mut truck, mut body, &ChildOf(hull)) in &mut trucks {
        let Ok((at, hull, vis)) = hulls.get(hull) else { continue };
        let truck = &mut *truck;
        let shown = vis.is_none_or(|v| *v != Visibility::Hidden);
        let (p, fwd, right) = (at.translation, at.rotation * Vec3::X, at.rotation * Vec3::Z);
        let (was_p, was_fwd) = truck.last.unwrap_or((p, fwd));
        truck.last = Some((p, fwd));
        let moved = p - was_p;
        let jumped = moved.length() >= JUMP_M;
        let along = if jumped { 0.0 } else { moved.dot(fwd) };
        truck.roll = (truck.roll + along / WHEEL_R) % core::f32::consts::TAU;
        // The steer its turn takes at its speed (about +Y: left is positive).
        let turned = was_fwd.cross(fwd).y.atan2(was_fwd.dot(fwd));
        let steer = if (along / dt).abs() > STEER_FROM {
            (WHEELBASE * turned / along).atan().clamp(-STEER_MAX, STEER_MAX)
        } else {
            0.0
        };
        truck.steer += (steer - truck.steer) * ease;

        // How it moves: smoothed (remote hulls step with the snapshots),
        // and what that took (m/s², along it and across it).
        let raw = if jumped { Vec3::ZERO } else { moved / dt };
        let was_vel = truck.vel;
        if jumped {
            truck.vel = Vec3::ZERO;
            truck.peak = 0.0;
        } else {
            truck.vel += (raw - truck.vel) * (dt / VEL_SMOOTH_S).min(1.0);
        }
        let accel = (truck.vel - was_vel) / dt;
        let flat = truck.vel.with_y(0.0);
        let speed = flat.length();
        let floor = map.floor(p.x, p.z, VEH_HALF_W, p.y + sim::consts::VEH_STEP);
        let off = p.y - floor;
        let down = off < AIR_M;

        // A blast near it: the spin it will turn over with once it is seen
        // to leave the ground (a teammate's spares it, and the server says
        // so by not throwing it).
        let to_hull = at.to_matrix().inverse();
        for ev in heard.0.iter().filter(|ev| ev.kind == Kind::Blast as u8) {
            let Some((dmg, radius, full)) = charge(ev.c) else {
                continue;
            };
            let b = v(ev.pos);
            let dist = b.distance(p + Vec3::Y * MID_Y);
            if dist >= radius {
                continue;
            }
            let k = if dist <= full {
                1.0
            } else {
                1.0 - (dist - full) / (radius - full)
            };
            let heft = (dmg / sim::consts::VEH_LIFT_DMG as f32).min(1.0) * k;
            let wobble = Vec3::new(truck.jitter(), truck.jitter(), truck.jitter()) * WOBBLE;
            truck.kick = kicked(to_hull.transform_point3(b)) * heft + wobble * heft;
            truck.kick_left = KICK_WAIT_S;
        }
        truck.kick_left -= dt;

        if down {
            // On its wheels: the ground under each wheel leans the body;
            // whatever turn it came down with is set right.
            if truck.air > LANDED_AIR_S {
                let k = ((-truck.fall - 1.0) / LAND_HARD_MPS).clamp(0.15, 1.0);
                truck.squat_v -= SQUAT_KICK * k;
                let kick = Vec2::new(truck.jitter(), truck.jitter() - 0.5) * SWAY_KICK * k;
                truck.sway_v += kick;
                if shown {
                    let at_wheels = WHEELS.map(|(x, z)| p + fwd * x + right * z);
                    super::fx::hulls::touchdown(&mut fx, map, &at_wheels, fwd, k);
                    noise.hit(Clunk::Land, p, k);
                }
            }
            truck.air = 0.0;
            truck.fall = 0.0;
            truck.spin = Vec3::ZERO;
            truck.tumble = truck.tumble.slerp(Quat::IDENTITY, (RIGHT_RATE * dt).min(1.0));
            if p.y - map.ground(p.x, p.z) < crate::WHEELS_DOWN_M && map.terrain.is_some() {
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
        } else {
            // In the air: a blast's spin, once it is seen to go up; then it
            // turns, steered to come down on its wheels when it lands.
            // (Thrown again up there: the new spin on top of the old.)
            let rose = truck.air == 0.0 && truck.vel.y > 0.5;
            let again = truck.air > 0.0 && raw.y > truck.vel.y + REKICK_MPS;
            if truck.kick_left > 0.0 && (rose || again) {
                truck.spin = if rose { truck.kick } else { truck.spin + truck.kick };
                truck.kick_left = 0.0;
            }
            truck.air += dt;
            truck.fall = truck.fall.min(truck.vel.y);
            truck.spin = right_way_up(truck.tumble, truck.spin, truck.vel.y, off, dt);
            truck.tumble = (Quat::from_scaled_axis(truck.spin * dt) * truck.tumble).normalize();
        }

        // The springs: it rolls out of a turn and dips its nose braking,
        // only on its wheels; and squats after a landing.
        let want = if down && !jumped {
            Vec2::new(
                (-accel.dot(right) * SWAY_BANK).clamp(-SWAY_MAX, SWAY_MAX),
                (accel.dot(fwd) * SWAY_PITCH).clamp(-SWAY_MAX, SWAY_MAX),
            )
        } else {
            Vec2::ZERO
        };
        let pull = (want - truck.sway) * SWAY_SPRING - truck.sway_v * SWAY_DAMP;
        truck.sway_v += pull * dt;
        truck.sway += truck.sway_v * dt;
        let pull = -truck.squat * SQUAT_SPRING - truck.squat_v * SQUAT_DAMP;
        truck.squat_v += pull * dt;
        truck.squat = (truck.squat + truck.squat_v * dt).clamp(-SQUAT_MAX, SQUAT_MAX * 0.4);
        if truck.sway.length() + truck.sway_v.length() < 1e-4 {
            (truck.sway, truck.sway_v) = (Vec2::ZERO, Vec2::ZERO);
        }
        if truck.squat.abs() + truck.squat_v.abs() < 1e-4 {
            (truck.squat, truck.squat_v) = (0.0, 0.0);
        }

        // The body: leaned about the ground under it, turned over about its
        // middle. Parked, nothing moves: written only on a change, or every
        // part under the hull goes round the transforms again.
        let lean = Quat::from_rotation_z(truck.pitch + truck.sway.y) * Quat::from_rotation_x(truck.bank + truck.sway.x);
        let mid = Vec3::Y * MID_Y;
        let placed = Transform::from_translation(Vec3::Y * (truck.lift + truck.squat) + mid - truck.tumble * mid)
            .with_rotation(truck.tumble * lean);
        body.set_if_neq(placed);
        // Its wheels by what `bind` found: not a walk of the whole scene
        // a truck a frame.
        for &w in &truck.wheels {
            let Ok((wheel, mut tr)) = wheels.get_mut(w) else {
                continue;
            };
            let steer = if wheel.front { truck.steer } else { 0.0 };
            // Forward is +X and the axle +Z: the top goes forward about -Z.
            let turn = Quat::from_rotation_y(steer) * Quat::from_rotation_z(-truck.roll);
            if tr.rotation != turn {
                tr.rotation = turn;
            }
        }

        let health = hull.map_or(255, |h| h.health);
        let driven = hull.is_some_and(|h| h.driven) && health > 0;
        if shown && !jumped {
            // Into something at speed: its nose or tail scrapes it.
            truck.peak = (truck.peak - PEAK_FADE * dt).max(speed);
            truck.scraped -= dt;
            let lost = truck.peak - speed;
            if down && lost > CRASH_MPS && truck.scraped <= 0.0 {
                let k = ((lost - CRASH_MPS) / CRASH_FULL_MPS).clamp(0.1, 1.0);
                let ahead = if was_vel.dot(fwd) >= 0.0 { 1.0 } else { -1.0 };
                let hit = p + fwd * (VEH_HALF_L * ahead) + Vec3::Y * 0.7;
                super::fx::hulls::scrape(&mut fx, hit, -fwd * ahead, k);
                noise.hit(Clunk::Crash, hit, k);
                truck.peak = speed;
                truck.scraped = SCRAPE_REST_S;
            }
            // Its tyres sliding: across the way it points, or locked
            // braking. A track behind its back wheels as it goes.
            let slide = truck.vel.dot(right).abs();
            let braking = (-accel.dot(fwd) * flat.dot(fwd).signum()).max(0.0);
            let skid = if down && speed > SKID_FROM_MPS {
                ((slide - SKID_SLIDE_MPS) / SKID_FULL_MPS)
                    .max((braking - SKID_BRAKE) / SKID_BRAKE)
                    .clamp(0.0, 1.0)
            } else {
                0.0
            };
            if skid > 0.0 {
                noise.skids.push((p, skid));
                truck.track += speed * dt;
                if truck.track >= TRACK_EVERY_M {
                    truck.track = 0.0;
                    let way = flat.normalize_or(fwd);
                    for &(x, z) in &WHEELS[2..] {
                        super::fx::hulls::tread(&mut fx, map, p + fwd * x + right * z, way, skid);
                    }
                }
            } else {
                truck.track = TRACK_EVERY_M;
            }
            // Its exhaust, at idle and more on the throttle.
            if driven {
                let throttle = (accel.dot(fwd) / sim::consts::VEH_ACCEL).clamp(0.0, 1.0);
                truck.puff += dt * (EXHAUST_HZ.0 + EXHAUST_HZ.1 * throttle);
                while truck.puff >= 1.0 {
                    truck.puff -= 1.0;
                    let (x, y, z) = TAILPIPE;
                    let pipe = at.transform_point(body.transform_point(Vec3::new(x, y, z)));
                    super::fx::hulls::exhaust(&mut fx, pipe, -fwd, throttle);
                }
            }
            if health > 0 && health < super::fx::hulls::FIRE_BELOW {
                noise.fires.push((p + Vec3::Y * 1.2, 0.5));
            }
        }

        // The world into its own frame, for what is drawn on it.
        let to_local = (at.to_matrix() * body.to_matrix()).inverse();
        // A new hull is clean: its health is back, more than a wear's ease.
        let want = 1.0 - health as f32 / 255.0;
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
            } else if ev.kind == Kind::Wreck as u8 && hull.is_some_and(|h| h.slot == ev.part) {
                // Blown up: the hull is gone from the sim; its husk flies.
                let world = at.mul_transform(*body);
                husk(&mut commands, &models, truck, world, v(ev.pos));
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
        // Whole and unmarked, nothing in its wear reads its frame
        // (truck_wear.wgsl: every layer is off below a hurt of 0.03), so a
        // clean truck going about keeps the frame last written and its
        // material isn't prepared again every frame it moves.
        let pristine = truck.hurt < PRISTINE && burnt == 0.0 && truck.marks.iter().all(|m| m.w == 0.0);
        let to_local = match truck.skins.first().and_then(|s| worn.get(s)) {
            Some(m) if pristine => m.extension.params.to_local,
            _ => to_local,
        };
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

/// A hull in the air turning over at `spin`, `tumble` off upright, going
/// up at `vy` with `off` metres under it: the spin it has now, steered so it
/// comes down on its wheels. Going up it turns as it was thrown; coming
/// down it is brought round the way it is already turning (or the short
/// way, when that would be too fast) to be upright as it lands.
fn right_way_up(tumble: Quat, spin: Vec3, vy: f32, off: f32, dt: f32) -> Vec3 {
    let g = sim::consts::GRAVITY;
    let t = ((vy + (vy * vy + 2.0 * g * off.max(0.0)).sqrt()) / g).max(0.05);
    let (axis, angle) = tumble.inverse().to_axis_angle();
    let angle = if angle > core::f32::consts::PI {
        angle - core::f32::consts::TAU
    } else {
        angle
    };
    let mut to_go = axis * angle;
    // The long way round, in the way it turns, if that isn't too fast.
    let long = axis * (angle - angle.signum() * core::f32::consts::TAU);
    if spin.dot(to_go) < 0.0 && long.length() / t < SPIN_MAX && spin.length() > 2.0 {
        to_go = long;
    }
    let want = (to_go / t).clamp_length_max(SPIN_MAX);
    let k = if vy > 0.0 {
        (dt * STEER_UP).min(1.0)
    } else {
        (dt * STEER_DOWN / t.max(0.1)).min(1.0)
    };
    (spin + (want - spin) * k) * (1.0 - SPIN_DRAG * dt).max(0.0)
}

/// A truck's middle, metres over its ground: what it turns over about.
const MID_Y: f32 = 0.85;
/// Off the ground by more than this, it is in the air.
const AIR_M: f32 = 0.15;
/// Its velocity is smoothed over this, seconds.
const VEL_SMOOTH_S: f32 = 0.08;
/// The spin a blast gives it at full strength, per metre-of-lever.
const SPIN_K: f32 = 7.0;
/// And a little the other ways, rad/s, so no two throws are alike.
const WOBBLE: f32 = 0.9;
/// How long a blast's spin waits for the hull to be seen in the air: our
/// own hull's throw comes back from the server a round trip later.
const KICK_WAIT_S: f32 = 0.6;
/// In the air, going up this much faster than it was is another throw.
const REKICK_MPS: f32 = 3.0;
/// The fastest it turns over, rad/s; how quickly it is steered to its
/// wheels going up (a little) and coming down (all of it by landing).
const SPIN_MAX: f32 = 14.0;
const STEER_UP: f32 = 0.4;
const STEER_DOWN: f32 = 2.5;
const SPIN_DRAG: f32 = 0.15;
/// How fast a turn it came down with is set right on the ground, per s.
const RIGHT_RATE: f32 = 9.0;
/// In the air longer than this, it lands: hard at `LAND_HARD_MPS` down.
const LANDED_AIR_S: f32 = 0.2;
const LAND_HARD_MPS: f32 = 9.0;
/// The springs: the sway's stiffness and damping, how far it leans per
/// m/s² across and along, and at most; a landing's kick to it.
const SWAY_SPRING: f32 = 55.0;
const SWAY_DAMP: f32 = 7.0;
const SWAY_BANK: f32 = 0.0035;
const SWAY_PITCH: f32 = 0.004;
const SWAY_MAX: f32 = 0.09;
const SWAY_KICK: f32 = 0.5;
/// The squat's stiffness and damping, at most (m), a landing's kick (m/s).
const SQUAT_SPRING: f32 = 140.0;
const SQUAT_DAMP: f32 = 9.0;
const SQUAT_MAX: f32 = 0.28;
const SQUAT_KICK: f32 = 3.2;
/// Into a wall: speed lost this fast (m/s, from its fastest of late, which
/// fades at `PEAK_FADE` m/s²) is a crash, the hardest at `CRASH_FULL_MPS`
/// more; one at most every `SCRAPE_REST_S`.
const CRASH_MPS: f32 = 6.0;
const CRASH_FULL_MPS: f32 = 12.0;
const PEAK_FADE: f32 = 30.0;
const SCRAPE_REST_S: f32 = 0.4;
/// Skidding: from this speed, sliding across faster than `SKID_SLIDE_MPS`
/// (all of it `SKID_FULL_MPS` more), or braking harder than `SKID_BRAKE`
/// m/s²; a track every `TRACK_EVERY_M`.
const SKID_FROM_MPS: f32 = 4.0;
const SKID_SLIDE_MPS: f32 = 2.2;
const SKID_FULL_MPS: f32 = 5.0;
const SKID_BRAKE: f32 = 15.0;
const TRACK_EVERY_M: f32 = super::fx::hulls::TREAD_M * 0.75;
/// Its tailpipe (in its frame), and its puffs a second idling and more at
/// full throttle.
const TAILPIPE: (f32, f32, f32) = (-2.7, 0.38, -0.62);
const EXHAUST_HZ: (f32, f32) = (2.5, 12.0);

/// A blown-up truck's husk: charred, thrown up turning over, down on the
/// ground (bouncing, on whatever side it lands), burning, then cold, then
/// gone into the ground before its hull is back.
#[derive(Component)]
pub struct Husk {
    /// Its middle, its turn and their speeds, in the world.
    mid: Vec3,
    turn: Quat,
    vel: Vec3,
    spin: Vec3,
    age: f32,
    /// Where its fire is (`fx::blast::carry`), and whether it has struck the
    /// ground hard yet (the first scorches it).
    fire: Vec3,
    struck: bool,
    /// At rest: the turn it settles to and its middle's height over the
    /// ground there.
    rest: Option<(Quat, f32)>,
    /// Wheels thrown off it yet; its smoke's carry.
    shed: bool,
    puff: f32,
}

/// A wheel off a husk.
#[derive(Component)]
pub struct Flung {
    vel: Vec3,
    spin: Vec3,
    age: f32,
}

/// A husk for `truck`, at `world` as it was last drawn, blown up at `blast`.
fn husk(commands: &mut Commands, models: &Models, truck: &mut Truck, world: Transform, blast: Vec3) {
    let mid = world.transform_point(Vec3::Y * MID_Y);
    let fwd = world.rotation * Vec3::X;
    let right = world.rotation * Vec3::Z;
    let side = truck.jitter();
    let vel = truck.vel
        + Vec3::Y * (HUSK_UP + truck.jitter().abs() * 2.0)
        + (mid - blast).with_y(0.0).normalize_or_zero() * 2.0
        + right * side * 1.5;
    // End over end or barrel roll, by a coin, and a little of the other.
    let spin = if truck.jitter() > 0.0 {
        fwd * (HUSK_SPIN * side.signum()) + right * truck.jitter()
    } else {
        right * (HUSK_SPIN * 0.8 * side.signum()) + fwd * truck.jitter()
    } + Vec3::Y * truck.jitter() * 1.5;
    let mut charred = Truck {
        hurt: 1.0,
        marks: truck.marks,
        next: truck.next,
        rng: truck.rng ^ 0x9e37_79b9,
        ..Truck::default()
    };
    charred.roll = truck.roll;
    commands
        .spawn((
            Husk {
                mid,
                turn: world.rotation,
                vel,
                spin,
                age: 0.0,
                fire: blast + Vec3::Y * 0.8,
                struck: false,
                rest: None,
                shed: false,
                puff: 0.0,
            },
            charred,
            SceneRoot(models.truck.clone()),
            world,
            Visibility::default(),
            crate::level::LevelPart,
        ))
        .observe(bind);
}

/// Every husk and wheel off one this frame: thrown, turning over, striking
/// the ground and coming to rest on it; burning, then smouldering; then
/// gone. After the trucks (`drive`).
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn husks(
    link: NonSend<LinkRes>,
    time: Res<Time>,
    mut husks: Query<(Entity, &mut Husk, &mut Truck, &mut Transform), Without<Flung>>,
    mut flung: Query<(Entity, &mut Flung, &mut Transform), Without<Husk>>,
    children: Query<&Children>,
    wheels: Query<&GlobalTransform, With<Wheel>>,
    mut worn: ResMut<Assets<TruckMaterial>>,
    mut fx: ResMut<super::fx::Fx>,
    mut noise: ResMut<HullSound>,
    mut commands: Commands,
) {
    let dt = time.delta_secs().min(1.0 / 30.0);
    if dt <= 0.0 {
        return;
    }
    let map = &link.0.session().core.map;
    for (e, mut h, mut truck, mut tr) in &mut husks {
        let (h, truck) = (&mut *h, &mut *truck);
        h.age += dt;
        if h.age > HUSK_S {
            commands.entity(e).despawn();
            continue;
        }
        // Two of its wheels come off as it goes up.
        if !h.shed && h.age > SHED_S {
            let found: Vec<(Entity, Transform)> = children
                .iter_descendants(e)
                .filter_map(|w| wheels.get(w).ok().map(|g| (w, g.compute_transform())))
                .collect();
            // Its scene may take a frame or two to be in.
            h.shed = found.len() == 4 || h.age > 1.0;
            if found.len() == 4 {
                let first = (truck.jitter().abs() * 4.0) as usize % 4;
                for k in [first, (first + 1 + (truck.jitter().abs() * 3.0) as usize % 3) % 4] {
                    let (w, at) = found[k];
                    let out = (at.translation - h.mid).with_y(0.0).normalize_or(Vec3::X);
                    let fling = Flung {
                        vel: h.vel * 0.6 + out * (4.0 + 3.0 * truck.jitter().abs()) + Vec3::Y * 4.0,
                        spin: Vec3::new(truck.jitter(), truck.jitter(), truck.jitter()) * 12.0,
                        age: 0.0,
                    };
                    commands
                        .entity(w)
                        .remove::<(ChildOf, Wheel)>()
                        .insert((at, fling, crate::level::LevelPart));
                }
            }
        }
        match h.rest {
            None => {
                if let Some((ground, k)) = fly(h, map, dt) {
                    strike(h, truck, map, &mut fx, &mut noise, ground, k);
                }
            }
            Some((turn, high)) => {
                // Settled at last, not eased by ever less: at rest it is
                // written no more (below) until it sinks.
                h.turn = if h.turn.angle_between(turn) < SETTLED {
                    turn
                } else {
                    h.turn.slerp(turn, (SETTLE_RATE * dt).min(1.0))
                };
                let floor = map.floor(h.mid.x, h.mid.z, 0.5, h.mid.y + 1.5);
                let sink = ((h.age - (HUSK_S - SINK_S)) / SINK_S).clamp(0.0, 1.0) * SINK_M;
                let want = floor + high - sink;
                if (want - h.mid.y).abs() < SETTLED {
                    h.mid.y = want;
                } else {
                    h.mid.y += (want - h.mid.y) * (SETTLE_RATE * dt).min(1.0);
                }
            }
        }
        // Its fire rides it; once that is out, it smoulders.
        let left = super::fx::blast::carry(&mut fx, h.fire, h.mid);
        h.fire = h.mid;
        let cold = (h.age / (HUSK_S - SINK_S)).clamp(0.0, 1.0);
        if left > 0.0 {
            noise.fires.push((h.mid, 0.6 + 0.4 * left));
        } else if cold < 1.0 {
            noise.fires.push((h.mid, 0.35 * (1.0 - cold)));
            h.puff += dt * SMOLDER_HZ;
            while h.puff >= 1.0 {
                h.puff -= 1.0;
                super::fx::hulls::smolder(&mut fx, h.mid, cold);
            }
        }
        let mid = Vec3::Y * MID_Y;
        let placed = Transform::from_translation(h.mid - h.turn * mid).with_rotation(h.turn);
        // At rest, nothing: its scene isn't gone round again nor its
        // steel prepared again every frame for the same.
        tr.set_if_neq(placed);
        let params = WearParams {
            to_local: placed.to_matrix().inverse(),
            state: Vec4::new(1.0, 1.0, 0.0, 0.0),
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
    for (e, mut f, mut tr) in &mut flung {
        f.age += dt;
        if f.age > HUSK_S {
            commands.entity(e).despawn();
            continue;
        }
        f.vel.y -= sim::consts::GRAVITY * dt;
        tr.translation += f.vel * dt;
        tr.rotation = (Quat::from_scaled_axis(f.spin * dt) * tr.rotation).normalize();
        let p = tr.translation;
        let floor = map.floor(p.x, p.z, 0.1, p.y + 1.0);
        let sink = ((f.age - (HUSK_S - SINK_S)) / SINK_S).clamp(0.0, 1.0) * SINK_M;
        if p.y - WHEEL_R < floor - sink {
            tr.translation.y = floor + WHEEL_R - sink;
            if f.vel.y < -2.5 {
                noise.hit(Clunk::Bounce, p, (-f.vel.y / 12.0) * 0.5);
                fx.emit(
                    super::fx::table::Layer::Grit,
                    4,
                    Vec3::new(p.x, floor, p.z),
                    Vec3::Y,
                    super::fx::table::Matter::Dirt,
                );
            }
            f.vel.y = (-f.vel.y * 0.35).max(0.0);
            f.vel.x *= 1.0 - (2.5 * dt).min(1.0);
            f.vel.z *= 1.0 - (2.5 * dt).min(1.0);
            f.spin *= 1.0 - (3.0 * dt).min(1.0);
        }
    }
}

/// A husk's corners, about its middle: what of it meets the ground.
fn corners() -> [Vec3; 8] {
    let (l, w) = (VEH_HALF_L * 0.95, VEH_HALF_W);
    let (lo, hi) = (0.05 - MID_Y, VEH_H * 0.95 - MID_Y);
    [
        Vec3::new(l, lo, w),
        Vec3::new(l, lo, -w),
        Vec3::new(-l, lo, w),
        Vec3::new(-l, lo, -w),
        Vec3::new(l, hi, w),
        Vec3::new(l, hi, -w),
        Vec3::new(-l, hi, w),
        Vec3::new(-l, hi, -w),
    ]
}

/// One frame of a husk in the air or tumbling on the ground: it falls and
/// turns; a corner that goes into the ground is pushed out and kicks it
/// back up and round; slow enough on the ground, it settles on the side
/// nearest down. Where it struck the ground hard, and how hard (0 to 1).
fn fly(h: &mut Husk, map: &sim::map::Map, dt: f32) -> Option<(Vec3, f32)> {
    h.vel.y -= sim::consts::GRAVITY * dt;
    h.mid += h.vel * dt;
    h.turn = (Quat::from_scaled_axis(h.spin * dt) * h.turn).normalize();
    h.spin *= 1.0 - (0.2 * dt).min(1.0);
    // The deepest corner into the ground.
    let mut deepest: Option<(Vec3, f32, f32)> = None;
    for c in corners() {
        let r = h.turn * c;
        let at = h.mid + r;
        let floor = map.floor(at.x, at.z, 0.1, at.y + 1.5);
        let depth = floor - at.y;
        if depth > 0.0 && deepest.is_none_or(|d| depth > d.1) {
            deepest = Some((r, depth, floor));
        }
    }
    let (r, depth, floor) = deepest?;
    h.mid.y += depth;
    let at = h.mid + r;
    // The ground's push at that corner (an impulse, as a rigid body's:
    // what it takes off the corner's speed into the ground, with a little
    // back, and its scrape along it), shared between its going and its
    // turning.
    let moving = h.vel + h.spin.cross(r);
    let into = moving.y;
    let mut struck = None;
    if into < 0.0 {
        let rn = r.cross(Vec3::Y);
        let j = -(1.0 + HUSK_BOUNCE) * into / (1.0 + rn.length_squared() / HUSK_INERTIA);
        h.vel.y += j;
        h.spin += rn * (j / HUSK_INERTIA);
        let along = moving.with_y(0.0);
        if let Some(t) = along.try_normalize() {
            let rt = r.cross(t);
            let jt = (along.length() / (1.0 + rt.length_squared() / HUSK_INERTIA)).min(HUSK_GRIP * j);
            h.vel -= t * jt;
            h.spin -= rt * (jt / HUSK_INERTIA);
        }
        h.spin *= 1.0 - (HUSK_SCRUB * dt).min(1.0);
        if -into > 2.5 {
            struck = Some((Vec3::new(at.x, floor, at.z), (-into / 12.0).clamp(0.0, 1.0)));
        }
    }
    // Slow on the ground: it settles on whichever side is nearest down,
    // its middle that side's height over the ground.
    let flat = h.vel.with_y(0.0).length();
    if h.vel.y.abs() < 1.2 && flat < 1.2 && h.spin.length() < 2.5 {
        h.vel = Vec3::ZERO;
        h.spin = Vec3::ZERO;
        let sides = [
            (Vec3::Y, MID_Y),
            (Vec3::NEG_Y, VEH_H - MID_Y),
            (Vec3::Z, VEH_HALF_W),
            (Vec3::NEG_Z, VEH_HALF_W),
        ];
        let (up, high) = sides
            .iter()
            .copied()
            .max_by(|a, b| (h.turn * a.0).y.total_cmp(&(h.turn * b.0).y))
            .unwrap_or((Vec3::Y, MID_Y));
        let turn = Quat::from_rotation_arc((h.turn * up).normalize(), Vec3::Y) * h.turn;
        h.rest = Some((turn, high));
    }
    struck
}

/// A husk striking the ground hard at `ground`, `k` of the hardest: dust,
/// sparks and bits of it; the first strike scorches the ground and it.
fn strike(
    h: &mut Husk,
    truck: &mut Truck,
    map: &sim::map::Map,
    fx: &mut super::fx::Fx,
    noise: &mut HullSound,
    ground: Vec3,
    k: f32,
) {
    use super::fx::table::{Layer, Matter};
    noise.hit(Clunk::Bounce, ground, k);
    fx.emit(
        Layer::DustCloud,
        (1.0 + 3.0 * k) as usize,
        ground + Vec3::Y * 0.2,
        Vec3::Y,
        Matter::Dirt,
    );
    fx.emit(Layer::Sparks, (4.0 + 16.0 * k) as usize, ground, Vec3::Y, Matter::Hull);
    fx.chips
        .throw(ground, Vec3::Y, Matter::Hull, (2.0 + 4.0 * k) as usize, ground.y);
    if !h.struck {
        h.struck = true;
        let matter = if map.terrain.is_some() {
            Matter::Dirt
        } else {
            Matter::Concrete
        };
        fx.marks
            .lay(map, super::fx::marks::Kind::Scorch, 3.4, ground, Vec3::Y, matter);
        truck.mark(Vec3::new(0.0, VEH_H, 0.0), -SCORCH_M);
    }
}

/// A husk's throw up (m/s), its turn (rad/s); what of its speed a bounce
/// keeps up and along, and of its spin; how hard a corner's strike turns it.
const HUSK_UP: f32 = 9.0;
const HUSK_SPIN: f32 = 4.5;
const HUSK_BOUNCE: f32 = 0.3;
/// Its turning's inertia over its mass (m²: a box its size, about any
/// axis); the ground's grip on it (friction), and how fast its turn is
/// scrubbed off while it touches the ground, per second.
const HUSK_INERTIA: f32 = 2.4;
const HUSK_GRIP: f32 = 0.6;
const HUSK_SCRUB: f32 = 1.5;
/// How fast it settles on its side, per second.
const SETTLE_RATE: f32 = 5.0;
/// Close enough to where it settles (radians, metres) to be there.
const SETTLED: f32 = 1e-4;
/// How long a husk lasts, seconds; the last `SINK_S` of it sinking `SINK_M`
/// into the ground. Its wheels come off `SHED_S` in.
const HUSK_S: f32 = 18.0;
const SINK_S: f32 = 3.0;
const SINK_M: f32 = 2.2;
const SHED_S: f32 = 0.12;
/// Its smoke puffs a second once its fire is out.
const SMOLDER_HZ: f32 = 6.0;

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

    /// A hull thrown by a blast, as the sim throws it, turned as `kicked`
    /// and `right_way_up` turn it: how far over it went, and how far off
    /// upright it came down.
    fn throw(blast: Vec3, lift: f32) -> (f32, f32) {
        let dt = 1.0 / 60.0;
        let g = sim::consts::GRAVITY;
        let (mut y, mut vy) = (0.0f32, lift);
        let (mut tumble, mut spin) = (Quat::IDENTITY, kicked(blast));
        let mut most: f32 = 0.0;
        while y >= 0.0 {
            spin = right_way_up(tumble, spin, vy, y, dt);
            tumble = (Quat::from_scaled_axis(spin * dt) * tumble).normalize();
            most = most.max(tumble.angle_between(Quat::IDENTITY));
            vy -= g * dt;
            y += vy * dt;
        }
        (most, tumble.angle_between(Quat::IDENTITY))
    }

    #[test]
    fn a_thrown_truck_turns_over_and_comes_down_on_its_wheels() {
        // A rocket under its right door: well over on its side in the air.
        let (most, landed) = throw(Vec3::new(0.0, 0.2, VEH_HALF_W + 0.4), sim::consts::VEH_BLAST_LIFT);
        assert!(most > 1.2, "turned {most} rad");
        assert!(landed < 0.25, "came down {landed} rad off its wheels");
        // Under its nose: end over end, and down on its wheels all the same.
        let (most, landed) = throw(Vec3::new(VEH_HALF_L + 0.5, 0.2, 0.0), sim::consts::VEH_BLAST_LIFT);
        assert!(most > 1.0, "turned {most} rad");
        assert!(landed < 0.25, "came down {landed} rad off its wheels");
        // A small hop is set right as quickly.
        let (_, landed) = throw(Vec3::new(0.0, 0.2, VEH_HALF_W + 0.4), 3.0);
        assert!(landed < 0.25, "came down {landed} rad off its wheels");
    }

    #[test]
    fn a_husk_is_thrown_turning_over_and_comes_to_rest_on_a_side() {
        let map = sim::map::Map::open();
        let floor = map.floor(0.0, 0.0, 0.1, 5.0);
        // End over end, a barrel roll, either way, with a little of the rest.
        let throws = [
            Vec3::X * HUSK_SPIN + Vec3::Z * 0.5,
            Vec3::NEG_X * HUSK_SPIN + Vec3::Y * 1.2,
            Vec3::Z * HUSK_SPIN * 0.8 + Vec3::X * -0.7,
            Vec3::NEG_Z * HUSK_SPIN * 0.8 + Vec3::new(0.4, -1.0, 0.0),
        ];
        for spin in throws {
            let mut h = Husk {
                mid: Vec3::new(0.0, floor + MID_Y, 0.0),
                turn: Quat::IDENTITY,
                vel: Vec3::new(1.0, HUSK_UP, 2.0),
                spin,
                age: 0.0,
                fire: Vec3::ZERO,
                struck: false,
                rest: None,
                shed: false,
                puff: 0.0,
            };
            let (mut top, mut turned, mut strikes) = (0.0f32, 0.0f32, 0);
            let mut t = 0.0;
            while h.rest.is_none() && t < 8.0 {
                strikes += usize::from(fly(&mut h, &map, 1.0 / 60.0).is_some());
                top = top.max(h.mid.y - floor - MID_Y);
                turned = turned.max(h.turn.angle_between(Quat::IDENTITY));
                t += 1.0 / 60.0;
            }
            assert!(top > 3.0, "{spin}: thrown {top} m");
            assert!(turned > 1.5, "{spin}: turned over {turned} rad");
            assert!(strikes >= 1, "{spin}: struck the ground");
            let (rest, high) = h.rest.unwrap_or_else(|| panic!("{spin}: at rest within 8 s"));
            // On a side: one of its axes straight up.
            let ups = [Vec3::Y, Vec3::NEG_Y, Vec3::Z, Vec3::NEG_Z].map(|a| (rest * a).y);
            assert!(ups.iter().any(|y| *y > 0.999), "{spin}: {ups:?}");
            assert!(high > 0.8 && high < 1.2);
            assert!(h.mid.y > floor + 0.5, "{spin}: above the ground: {}", h.mid.y - floor);
        }
    }

    #[test]
    fn a_blast_rolls_a_truck_over_away_from_it() {
        // Under its right flank (+Z): its right side goes up, about -X.
        let spin = kicked(Vec3::new(0.0, 0.2, VEH_HALF_W + 0.4));
        assert!(spin.x < -1.0 && spin.y.abs() < 0.1, "{spin}");
        let right = Quat::from_scaled_axis(spin * 0.1) * Vec3::Z;
        assert!(right.y > 0.0, "{right}");
        // Under its nose (+X): the nose goes up, about +Z.
        let spin = kicked(Vec3::new(VEH_HALF_L + 0.4, 0.2, 0.0));
        let nose = Quat::from_scaled_axis(spin * 0.1) * Vec3::X;
        assert!(nose.y > 0.0, "{nose}");
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

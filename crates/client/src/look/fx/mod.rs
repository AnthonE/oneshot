//! Effects: shots, hits, blasts, what's in flight, shields, the world's
//! small motions. All drawn from what the session tells us: events (a shot
//! fog lets us only hear draws nothing), the bodies' shields and footing,
//! projectiles. Drawing only: nothing here decides anything.
//!
//! - `pool`: the materials, made once, faded by swapping handles.
//! - `particles`: the CPU particles and their tier's cap.
//! - `arms`: each gun's tracer, flash and impact.
//! - `shots`: gunfire and its impacts and marks.
//! - `blast`: explosions, a wreck's fire, the camera's shake.
//! - `flying`: rockets and frags drawn with their models, and their trails.
//! - `bodies`: shields flaring and popping, spawns, landings, pickups.
//! - `hulls`: vehicles' dust, and smoke and fire when hurt.
//! - `demo`: `?fxdemo=1`, each effect staged in front of the camera.

pub mod arms;
pub mod blast;
pub mod bodies;
pub mod demo;
pub mod flying;
pub mod hulls;
pub mod particles;
pub mod pool;
pub mod shots;

use std::collections::VecDeque;

use bevy::prelude::*;
use client_net::core::{Drawn, DrawnVehicle};
use proto::EventState;
use sim::events::Kind;
use sim::map::Map;

pub use particles::{Particle, Shape};
pub use pool::Tone;

use crate::Opts;

/// `EventState::part` on a shot fog lets us only hear (room's `HEARD`):
/// no tracer, no spark, its place rounded.
pub const HEARD: u8 = 1;

/// What a tier may have alive at once.
#[derive(Clone, Copy)]
pub struct Caps {
    pub particles: usize,
    /// Bullet marks and scorches, reused oldest first.
    pub marks: usize,
    /// Flashes' and blasts' point lights.
    pub lights: usize,
}

/// The desktop's.
const HIGH: Caps = Caps {
    particles: 2048,
    marks: 96,
    lights: 8,
};
/// A browser's (WebGL2), and `lite` (a software GPU).
const LOW: Caps = Caps {
    particles: 320,
    marks: 24,
    lights: 2,
};

#[derive(Resource)]
pub struct Fx {
    pub pool: pool::Pool,
    pub caps: Caps,
    /// Shared meshes: a unit quad facing +Z, a unit cube.
    quad: Handle<Mesh>,
    rod: Handle<Mesh>,
    pub ball: Handle<Mesh>,
    /// Particles alive (counted each frame, bumped by each emit).
    live: usize,
    lights: usize,
    marks: VecDeque<Entity>,
    /// The camera's turn, for billboards, and where it is.
    pub cam: Quat,
    pub eye: Vec3,
    /// The camera's shake this frame, metres and radians (`blast.rs`).
    pub shake: f32,
    /// Rounds each slot has fired that we saw, for every-nth tracers.
    rounds: [u16; 64],
    /// Slots whose muzzle already flashed this frame (a shotgun's pellets
    /// are a dozen shots).
    flashed: u64,
    seed: std::sync::atomic::AtomicU32,
    /// Each fighter's shield and footing last frame (`bodies.rs`).
    pub was: bodies::Was,
    /// Each vehicle's place last frame (`hulls.rs`).
    hulls: [hulls::Hull; 16],
}

impl Fx {
    /// A number in 0..1: looks only, so any spread will do.
    pub fn rand(&self) -> f32 {
        // xorshift32
        let mut x = self.seed.load(std::sync::atomic::Ordering::Relaxed);
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.seed.store(x, std::sync::atomic::Ordering::Relaxed);
        (x >> 8) as f32 / (1u32 << 24) as f32
    }
    /// Starts particle `p` at `at` (`particles::emit`).
    pub fn emit(&mut self, commands: &mut Commands, at: Vec3, p: Particle) -> Option<Entity> {
        particles::emit(commands, self, at, p)
    }
    pub fn range(&self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.rand()
    }
    /// A unit vector, anywhere on the sphere.
    pub fn unit(&self) -> Vec3 {
        let z = self.range(-1.0, 1.0);
        let a = self.range(0.0, core::f32::consts::TAU);
        let r = (1.0 - z * z).max(0.0).sqrt();
        Vec3::new(r * a.cos(), z, r * a.sin())
    }
    /// A unit vector within `cone` (0: `d` itself, 1: the half sphere) of `d`.
    pub fn around(&self, d: Vec3, cone: f32) -> Vec3 {
        let v = d + self.unit() * cone;
        let l = v.length();
        if l < 1e-4 {
            d
        } else {
            v / l
        }
    }
}

pub fn setup(
    mut commands: Commands,
    opts: Res<Opts>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let low = opts.lite || cfg!(target_arch = "wasm32");
    commands.insert_resource(Fx {
        pool: pool::Pool::new(&mut mats, &mut images),
        caps: if low { LOW } else { HIGH },
        quad: meshes.add(Rectangle::new(1.0, 1.0)),
        rod: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        ball: meshes.add(Sphere::new(1.0).mesh().uv(16, 12)),
        live: 0,
        lights: 0,
        marks: VecDeque::new(),
        cam: Quat::IDENTITY,
        eye: Vec3::ZERO,
        shake: 0.0,
        rounds: [0; 64],
        flashed: 0,
        seed: std::sync::atomic::AtomicU32::new(0x9e37_79b9),
        was: bodies::Was::default(),
        hulls: [hulls::Hull::default(); 16],
    });
}

/// A point light that dims to nothing: a flash's, a blast's.
#[derive(Component)]
pub struct Glow {
    age: f32,
    life: f32,
    peak: f32,
}

/// Lights a flash for `life` seconds, if the tier has a light to spare.
pub fn glow(commands: &mut Commands, fx: &mut Fx, at: Vec3, lumens: f32, range: f32, color: Color, life: f32) {
    if fx.lights >= fx.caps.lights {
        return;
    }
    fx.lights += 1;
    commands.spawn((
        Transform::from_translation(at),
        PointLight {
            intensity: lumens,
            range,
            color,
            shadows_enabled: false,
            ..default()
        },
        Glow {
            age: 0.0,
            life,
            peak: lumens,
        },
    ));
}

/// What the world looks like to the effects this frame: the map, who's in
/// view, and who we are.
pub struct Scene<'a> {
    pub map: &'a Map,
    pub others: &'a [Drawn],
    pub vehicles: &'a [DrawnVehicle],
    pub me: u8,
}

/// One frame's events: what they look like.
pub fn on_events(commands: &mut Commands, fx: &mut Fx, scene: &Scene, events: &[EventState]) {
    fx.flashed = 0;
    for e in events {
        let from = Vec3::new(e.pos.x, e.pos.y, e.pos.z);
        let to = Vec3::new(e.end.x, e.end.y, e.end.z);
        match e.kind {
            k if k == Kind::Shot as u8 && e.part != HEARD => shots::shot(commands, fx, scene, e, from, to),
            k if k == Kind::Blast as u8 => blast::blast(commands, fx, scene, from, e.c, e.part == HEARD),
            k if k == Kind::Wreck as u8 => blast::wreck(commands, fx, scene, from),
            k if k == Kind::Spawn as u8 => bodies::spawned(commands, fx, from),
            k if k == Kind::Pickup as u8 => bodies::picked_up(commands, fx, from),
            k if k == Kind::Melee as u8 => bodies::swung(commands, fx, scene, e.a, from),
            _ => {}
        }
    }
}

/// Every frame, after the camera is placed: particles move and fade,
/// lights dim, the shake settles.
#[allow(clippy::type_complexity)]
pub fn tick(
    mut commands: Commands,
    time: Res<Time>,
    mut fx: ResMut<Fx>,
    cam: Query<&Transform, (With<Camera3d>, Without<Particle>)>,
    mut q: Query<(
        Entity,
        &mut Particle,
        &mut Transform,
        &mut MeshMaterial3d<StandardMaterial>,
        &mut Visibility,
    )>,
    mut lights: Query<(Entity, &mut Glow, &mut PointLight)>,
    mut burning: Query<(Entity, &mut blast::Burning, &Transform), Without<Particle>>,
) {
    let dt = time.delta_secs().min(0.1);
    let fx = &mut *fx;
    if let Ok(c) = cam.single() {
        fx.cam = c.rotation;
        fx.eye = c.translation;
    }
    blast::burn(&mut commands, fx, dt, &mut burning);
    particles::step_all(&mut commands, fx, dt, &mut q);
    let mut n = 0;
    for (e, mut g, mut l) in &mut lights {
        g.age += dt;
        let u = g.age / g.life;
        if u >= 1.0 {
            commands.entity(e).despawn();
            continue;
        }
        n += 1;
        l.intensity = g.peak * (1.0 - u) * (1.0 - u);
    }
    fx.lights = n;
    fx.shake = blast::settle(fx.shake, dt);
}

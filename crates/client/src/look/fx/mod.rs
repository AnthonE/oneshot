//! Effects: shots, hits, blasts, what's in flight, shields, the world's
//! small motions. All drawn from what the session tells us: events (a shot
//! fog lets us only hear draws nothing), the bodies' shields and footing,
//! projectiles, and our own predicted gun. Drawing only: nothing here
//! decides anything.
//!
//! The engine is Gates' (`crates/client/src/render/fx/`, MIT, © 2026
//! MoreRight DAO): two pools drawn as two meshes, so the whole world's
//! particles are two draws on WebGL2 and on the desktop alike.
//!
//! - **glow**: unlit, additive, HDR vertex colour. Sparks, flashes, fire,
//!   tracers, arcs. A sprite carries its own halo and a streak is never
//!   drawn under ~1.5 px wide, so it reads as light with bloom off (the
//!   browser).
//! - **soft**: lit like the world, blended, sorted back to front. Dust,
//!   grit, smoke, mist.
//!
//! - `atlas`: the generated sprite sheet. `pool`: the pools and their mesh.
//! - `table`: what a blow throws by weapon × matter, and each gun's look.
//! - `marks`: bullet holes and scorches, one mesh.
//! - `shots`: gunfire, ours drawn the frame it breaks; impacts by matter.
//! - `blast`: explosions and a wreck's fire. `flying`: rockets and frags.
//! - `bodies`: shields flaring and breaking, landings, spawns, pickups.
//! - `hulls`: vehicles' dust, and smoke and fire when hurt.
//! - `demo`: `?fxdemo=1`, each effect staged in front of the camera.

pub mod atlas;
pub mod blast;
pub mod bodies;
pub mod demo;
pub mod flying;
pub mod hulls;
pub mod marks;
pub mod pool;
pub mod shots;
pub mod stage;
pub mod table;

use bevy::camera::visibility::NoFrustumCulling;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use client_net::core::{Drawn, DrawnVehicle};
use proto::EventState;
use sim::events::Kind;
use sim::map::Map;

use crate::look::shake::Shake;
use crate::Opts;
use pool::{pool_mesh, MeshWrite, Pool};
use table::{Layer, Matter, Weapon};

/// `EventState::part` on a shot fog lets us only hear (room's `HEARD`):
/// no tracer, no spark, its place rounded.
pub const HEARD: u8 = 1;

/// What a tier may have alive at once: the two pools, the marks, the
/// flash lights.
#[derive(Clone, Copy)]
pub struct Caps {
    pub glow: usize,
    pub soft: usize,
    pub marks: usize,
    pub lights: usize,
}

/// The desktop's.
const HIGH: Caps = Caps {
    glow: 1024,
    soft: 512,
    marks: 192,
    lights: 6,
};
/// A browser's (WebGL2), and `lite` (a software GPU).
const LOW: Caps = Caps {
    glow: 512,
    soft: 256,
    marks: 64,
    lights: 2,
};
/// Flash lights spawned (the tier uses the first `Caps::lights`).
const MAX_LIGHTS: usize = 6;

/// A light flash: peak intensity (lumens), range (metres), life (seconds),
/// linear colour.
#[derive(Clone, Copy, Debug)]
pub struct FlashDef {
    pub lumens: f32,
    pub range: f32,
    pub life: f32,
    pub color: [f32; 3],
}

/// A muzzle: bright, short, warm.
pub const MUZZLE_FLASH: FlashDef = FlashDef {
    lumens: 150_000.0,
    range: 6.0,
    life: 0.06,
    color: [1.0, 0.72, 0.4],
};
/// A blast: a hard orange light over everything near.
pub const BLAST_FLASH: FlashDef = FlashDef {
    lumens: 3_000_000.0,
    range: 18.0,
    life: 0.45,
    color: [1.0, 0.62, 0.3],
};
/// A shield cracking: a cold blue blink.
pub const ZAP_FLASH: FlashDef = FlashDef {
    lumens: 60_000.0,
    range: 4.0,
    life: 0.07,
    color: [0.5, 0.7, 1.0],
};

#[derive(Clone, Copy, Debug, Default)]
pub struct Flash {
    pub at: Vec3,
    pub left: f32,
    pub life: f32,
    pub lumens: f32,
    pub range: f32,
    pub color: [f32; 3],
}

#[derive(Resource)]
pub struct Fx {
    pub glow: Pool,
    pub soft: Pool,
    pub marks: marks::Marks,
    pub shake: Shake,
    pub caps: Caps,
    flashes: [Flash; MAX_LIGHTS],
    glow_mesh: Handle<Mesh>,
    soft_mesh: Handle<Mesh>,
    /// The camera last frame: where it is and its turn.
    pub eye: Vec3,
    pub cam: Quat,
    /// Rounds each slot has fired that we saw, for every-nth tracers.
    rounds: [u16; 64],
    /// Slots whose muzzle already flashed this frame (a shotgun's pellets
    /// are a dozen shots).
    flashed: u64,
    rng: u32,
    /// Each fighter's shield and footing last frame (`bodies.rs`).
    pub was: bodies::Was,
    /// Each vehicle's place last frame (`hulls.rs`).
    hulls: [hulls::Hull; 16],
    /// Wrecks burning (`blast.rs`).
    burning: [blast::Burn; 4],
    /// Our own rounds drawn so far (`shots::own`), and which hand drew the
    /// last while two guns are out.
    own_shots: u32,
    own_left: bool,
    /// Rounds of light flying last frame (`flying.rs`), to tell where one
    /// ended.
    last_light: Vec<proto::ProjState>,
}

impl Fx {
    /// In `[0, 1)`: looks only, so any spread will do.
    pub fn rand(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }
    pub fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.rand()
    }
    /// A unit vector, anywhere on the sphere.
    pub fn unit(&mut self) -> Vec3 {
        let z = self.range(-1.0, 1.0);
        let a = self.range(0.0, core::f32::consts::TAU);
        let r = (1.0 - z * z).max(0.0).sqrt();
        Vec3::new(r * a.cos(), z, r * a.sin())
    }

    /// Throws `n` of `layer` at `at` around `dir`, into the pool it draws in.
    pub fn emit(&mut self, layer: Layer, n: usize, at: Vec3, dir: Vec3, matter: Matter) {
        let pool = if layer.glow() { &mut self.glow } else { &mut self.soft };
        table::emit(pool, layer, n, at, dir, matter);
    }

    /// [`Fx::emit`] in a colour of ours, at `scale` times the layer's size.
    pub fn emit_tint(&mut self, layer: Layer, n: usize, at: Vec3, dir: Vec3, tint: Color, scale: f32) {
        let pool = if layer.glow() { &mut self.glow } else { &mut self.soft };
        table::emit_tint(pool, layer, n, at, dir, tint, scale);
    }

    /// What `weapon` striking `matter` at `at` throws, around `away` (the
    /// surface's normal, or back toward the shooter), thinned with distance;
    /// and the mark it leaves.
    pub fn impact(&mut self, weapon: Weapon, matter: Matter, at: Vec3, away: Vec3) {
        let def = table::effect(weapon, matter);
        let dist = at.distance(self.eye);
        let lod = if weapon == Weapon::Blast {
            table::lod_far(dist)
        } else {
            table::lod(dist)
        };
        for (layer, n) in def.layers {
            let n = table::scaled(n, lod);
            if n > 0 {
                self.emit(layer, n, at, away, matter);
            }
        }
        if matter == Matter::Shield && lod > 0.0 {
            self.flash(at, ZAP_FLASH);
        }
        if let Some((k, size)) = marks::kind(weapon, matter) {
            if lod > 0.0 {
                self.marks.place(k, size, at, away, matter);
            }
        }
    }

    /// Lights a flash at `at`: a free light, or the one nearest out.
    pub fn flash(&mut self, at: Vec3, def: FlashDef) {
        let n = self.caps.lights.min(MAX_LIGHTS);
        if n == 0 {
            return;
        }
        let i = (0..n)
            .min_by(|&a, &b| self.flashes[a].left.total_cmp(&self.flashes[b].left))
            .unwrap_or(0);
        // A brighter one burning keeps its light.
        if self.flashes[i].left > 0.0 && self.flashes[i].lumens > def.lumens * 2.0 {
            return;
        }
        self.flashes[i] = Flash {
            at,
            left: def.life,
            life: def.life,
            lumens: def.lumens,
            range: def.range,
            color: def.color,
        };
    }
}

/// A flash light's entity: its index into `Fx::flashes`.
#[derive(Component)]
pub struct FlashLight(usize);

/// One of the two particle meshes.
#[derive(Component)]
pub struct FxMesh {
    glow: bool,
}

/// Both pools, their meshes and materials over the generated sheet, the
/// flash lights, the marks. Always drawn (collapsed quads until something
/// flies), so their pipelines compile at load and not on the first spark.
pub fn setup(
    mut commands: Commands,
    opts: Res<Opts>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let caps = if opts.lite || cfg!(target_arch = "wasm32") {
        LOW
    } else {
        HIGH
    };
    let sheet = images.add(atlas::sprite_image());
    let glow_mesh = meshes.add(pool_mesh(caps.glow));
    let soft_mesh = meshes.add(pool_mesh(caps.soft));
    let glow = mats.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(sheet.clone()),
        unlit: true,
        alpha_mode: AlphaMode::Add,
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    let soft = mats.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(sheet),
        perceptual_roughness: 1.0,
        metallic: 0.0,
        reflectance: 0.0,
        alpha_mode: AlphaMode::Blend,
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    for i in 0..MAX_LIGHTS {
        commands.spawn((
            FlashLight(i),
            PointLight {
                intensity: 0.0,
                range: 1.0,
                shadows_enabled: false,
                ..default()
            },
            Transform::default(),
            Visibility::Hidden,
        ));
    }
    for (is_glow, mesh, mat) in [(false, soft_mesh.clone(), soft), (true, glow_mesh.clone(), glow)] {
        commands.spawn((
            FxMesh { glow: is_glow },
            Mesh3d(mesh),
            MeshMaterial3d(mat),
            Transform::IDENTITY,
            Visibility::Visible,
            NoFrustumCulling,
            NotShadowCaster,
        ));
    }
    let mut marks = marks::Marks::new(caps.marks);
    marks::setup(&mut commands, &mut marks, &mut meshes, &mut mats, &mut images);
    commands.insert_resource(Fx {
        glow: Pool::new(caps.glow, false),
        soft: Pool::new(caps.soft, true),
        marks,
        shake: Shake::default(),
        caps,
        flashes: [Flash::default(); MAX_LIGHTS],
        glow_mesh,
        soft_mesh,
        eye: Vec3::ZERO,
        cam: Quat::IDENTITY,
        rounds: [0; 64],
        flashed: 0,
        rng: 0x9e37_79b9,
        was: bodies::Was::default(),
        hulls: [hulls::Hull::default(); 16],
        burning: [blast::Burn::default(); 4],
        own_shots: 0,
        own_left: false,
        last_light: Vec::new(),
    });
}

/// What the world looks like to the effects this frame: the map, who's in
/// view, and who we are.
pub struct Scene<'a> {
    pub map: &'a Map,
    pub others: &'a [Drawn],
    pub vehicles: &'a [DrawnVehicle],
    pub me: u8,
    /// The time now, seconds.
    pub t: f32,
}

/// One frame's events: what they look like.
pub fn on_events(fx: &mut Fx, scene: &Scene, events: &[EventState]) {
    fx.flashed = 0;
    for e in events {
        let from = Vec3::new(e.pos.x, e.pos.y, e.pos.z);
        let to = Vec3::new(e.end.x, e.end.y, e.end.z);
        match e.kind {
            k if k == Kind::Shot as u8 && e.part != HEARD => shots::shot(fx, scene, e, from, to),
            k if k == Kind::Blast as u8 => blast::blast(fx, scene, from, e.c, e.part == HEARD),
            k if k == Kind::Wreck as u8 => blast::wreck(fx, scene, from),
            k if k == Kind::Spawn as u8 => bodies::spawned(fx, from),
            k if k == Kind::Pickup as u8 => bodies::picked_up(fx, from),
            k if k == Kind::Melee as u8 => bodies::swung(fx, scene, e.a, e.c, from),
            // A hit on us: the frame flinches, by how much it took.
            k if k == Kind::Hit as u8 && e.b == scene.me && e.a != scene.me => {
                let pts = e.c as f32;
                let t = (crate::look::shake::HURT_TRAUMA_BASE + pts * crate::look::shake::HURT_TRAUMA_PER_PT)
                    .min(crate::look::shake::HURT_TRAUMA_MAX);
                fx.shake.add(t);
                let (p, y) = (fx.shake.rand(), fx.shake.rand());
                fx.shake.kick(Vec3::new(p, y, 0.0) * crate::look::shake::FLINCH_RAD);
            }
            _ => {}
        }
    }
}

/// Every frame, after the camera is placed: the pools step and are copied
/// into their meshes, the wrecks burn, the lights dim, the marks fade.
#[allow(clippy::type_complexity)]
pub fn draw(
    time: Res<Time>,
    mut fx: ResMut<Fx>,
    cams: Query<&GlobalTransform, (With<Camera3d>, Without<FxMesh>)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut q: Query<(&FxMesh, &mut Transform, &mut GlobalTransform), Without<FlashLight>>,
    mut lights: Query<(&FlashLight, &mut PointLight, &mut Transform, &mut Visibility), Without<FxMesh>>,
) {
    // A hitch must not fling everything a hundred metres.
    let dt = time.delta_secs().min(0.1);
    let fx = &mut *fx;
    blast::burn(fx, dt);
    if let Some(mesh) = fx.marks.fade(dt).then(|| meshes.get_mut(fx.marks.mesh())).flatten() {
        fx.marks.write(mesh);
    }
    for (FlashLight(i), mut light, mut tf, mut vis) in &mut lights {
        let f = &mut fx.flashes[*i];
        if f.left <= 0.0 {
            if *vis != Visibility::Hidden {
                *vis = Visibility::Hidden;
            }
            continue;
        }
        f.left = (f.left - dt).max(0.0);
        let k = (f.left / f.life.max(1e-4)).powi(2);
        light.intensity = f.lumens * k;
        light.range = f.range;
        light.color = Color::linear_rgb(f.color[0], f.color[1], f.color[2]);
        tf.translation = f.at;
        *vis = Visibility::Visible;
    }
    // The camera this frame (ours is the only 3D one).
    let Ok(cam_gt) = cams.single() else {
        fx.glow.step(dt);
        fx.soft.step(dt);
        return;
    };
    let cam = pool::cam(cam_gt);
    let eye = cam_gt.translation();
    fx.eye = eye;
    fx.cam = cam_gt.compute_transform().rotation;
    let ahead: Vec3 = cam_gt.forward().into();
    for (m, mut tf, mut gt) in &mut q {
        // The additive mesh sorts at the eye, the soft one a metre ahead of
        // it: transparent draws go far to near, so sparks land over dust.
        let (pool, handle, anchor) = if m.glow {
            (&mut fx.glow, &fx.glow_mesh, eye)
        } else {
            (&mut fx.soft, &fx.soft_mesh, eye + ahead)
        };
        if tf.translation != anchor {
            tf.translation = anchor;
            *gt = GlobalTransform::from_translation(anchor);
        }
        if !pool.needs_write() {
            continue;
        }
        if let Some(mesh) = meshes.get_mut(handle) {
            pool.write(mesh, &cam, anchor);
        }
    }
    // Stepped after they are written, so everything thrown this frame is
    // drawn once where it was born: a flash shorter than a slow frame
    // (a browser on a weak GPU) would otherwise never be seen.
    fx.glow.step(dt);
    fx.soft.step(dt);
}

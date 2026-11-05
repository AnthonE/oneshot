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
//!   drawn under ~1.5 px wide, so it reads as light with bloom off
//!   (WebGL2, a device without room for it, or a rung without it:
//!   `look/quality.rs`). How many may fly is the EFFECTS row (`Caps`).
//! - **soft**: lit like the world, blended, sorted back to front. Dust,
//!   grit, smoke, mist. The same pool on every rung: smoke can hide a
//!   player.
//!
//! - **cloud** and **boom**: the baked sheets (`sheets`: smoke, dust,
//!   fire, plasma, flashes; the explosions), premultiplied, lit six ways
//!   in their own shader, faded against the wall behind. The same pool on
//!   every rung, as the soft one.
//!
//! - `atlas`: the generated sprite sheet. `pool`: the pools and their mesh.
//!   `sheets`: the baked flipbooks and their material.
//! - `table`: what a blow throws by weapon × matter, and each gun's look.
//! - `surface`: what a blow met in the map as drawn: the face, its normal,
//!   its matter.
//! - `marks`: holes, chips, dents, gashes, scuffs, blood and scorches laid
//!   on the drawn surface, one mesh. `chips`: lit debris, one mesh.
//! - `shots`: gunfire, ours drawn the frame it breaks; impacts by matter.
//! - `blast`: explosions and a wreck's fire. `flying`: rockets and frags.
//! - `bodies`: shields flaring and breaking, landings, spawns, pickups.
//! - `hulls`: vehicles' dust, and smoke and fire when hurt.
//! - `demo`: `?fxdemo=1`, each effect staged in front of the camera.

pub mod atlas;
pub mod blast;
pub mod bodies;
pub mod chips;
pub mod demo;
pub mod flying;
pub mod hulls;
pub mod marks;
pub mod pool;
pub mod sheets;
pub mod shell;
pub mod shots;
pub mod stage;
pub mod surface;
pub mod table;

use bevy::camera::visibility::NoFrustumCulling;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use client_net::core::{Drawn, DrawnVehicle};
use proto::EventState;
use sim::events::Kind;
use sim::geom::V3;
use sim::map::Map;

use crate::look::quality::{Effects, Tier};
use crate::look::shake::Shake;
use pool::{pool_mesh, MeshWrite, Pool};
use sheets::SheetMaterial;
use table::{Layer, Matter, Weapon, Which};

/// `EventState::part` on a shot fog lets us only hear (room's `HEARD`):
/// no tracer, no spark, its place rounded.
pub const HEARD: u8 = 1;

/// What may be alive at once: the two pools, the marks, the flash lights.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Caps {
    pub glow: usize,
    pub soft: usize,
    pub marks: usize,
    pub lights: usize,
}

/// Smoke, dust and marks: the same on every rung. They can stand between
/// a player and whoever is behind them, so a lower graphics rung that ran
/// out of them sooner would see more (`quality.rs`' fairness).
pub const SOFT: usize = 512;
pub const MARKS: usize = 192;
/// The baked pools, the same on every rung for the same reason: smoke.
pub const CLOUD: usize = 384;
pub const BOOM: usize = 48;

impl Caps {
    /// The EFFECTS row (`quality::Effects`): how many sparks and tracers
    /// may fly and how many flash lights burn. Those only show, never hide.
    pub fn for_effects(e: Effects) -> Caps {
        let (glow, lights) = match e {
            Effects::High => (2048, 8),
            Effects::Low => (512, 2),
        };
        Caps {
            glow,
            soft: SOFT,
            marks: MARKS,
            lights,
        }
    }
}

/// Flash lights spawned (the tier uses the first `Caps::lights`).
const MAX_LIGHTS: usize = 8;

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
    pub cloud: Pool,
    pub boom: Pool,
    pub marks: marks::Marks,
    pub chips: chips::Chips,
    /// The faces near a blow, reused for the casts that find them.
    pub near: surface::Near,
    /// Slots that bled this frame (one splat a body a frame, however many
    /// pellets: `shots.rs`, then the hits in `on_events`).
    bled: u64,
    pub shake: Shake,
    pub caps: Caps,
    flashes: [Flash; MAX_LIGHTS],
    glow_mesh: Handle<Mesh>,
    soft_mesh: Handle<Mesh>,
    cloud_mesh: Handle<Mesh>,
    boom_mesh: Handle<Mesh>,
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
    /// Debris flung off a blast, trailing smoke (`blast.rs`).
    trailers: [blast::Trailer; 16],
    /// Blasts' shock shells (`shell.rs`).
    pub shocks: [shell::Shock; shell::SHELLS],
    /// The last blast seen (not heard) this frame, for the grass's shock
    /// (`foliage::update` takes it).
    pub shock: Option<Vec3>,
    /// Our own rounds drawn so far, by hand (`shots::own`).
    own_rounds: crate::look::viewmodel::Rounds,
    /// Our own flash, by hand, until the frame's camera is placed
    /// (`shots::muzzle`).
    own_flash: [Option<shots::OwnFlash>; 2],
    /// Rounds of light flying last frame (`flying.rs`), to tell where one
    /// ended.
    last_light: Vec<client_net::core::DrawnProj>,
    /// Our own melee: its cooldown last frame, and where we looked as the
    /// swing began, which the server's event comes back to a round trip
    /// later (`bodies::swung`).
    swing: (u16, Option<Vec3>),
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

    /// Our own fighter's melee cooldown this frame and where we look: a
    /// swing starting (the cooldown rising) keeps that aim for its mark.
    pub fn own_swing(&mut self, melee_cool: u16, aim: Vec3) {
        if melee_cool > self.swing.0 {
            self.swing.1 = Some(aim);
        }
        self.swing.0 = melee_cool;
    }

    /// The aim our own last swing began with, once (`bodies::swung`).
    pub fn take_swing(&mut self) -> Option<Vec3> {
        self.swing.1.take()
    }

    /// The pool `which` names.
    pub fn pool(&mut self, which: Which) -> &mut Pool {
        match which {
            Which::Glow => &mut self.glow,
            Which::Soft => &mut self.soft,
            Which::Cloud => &mut self.cloud,
            Which::Boom => &mut self.boom,
        }
    }

    /// Throws `n` of `layer` at `at` around `dir`, into the pool it draws in.
    pub fn emit(&mut self, layer: Layer, n: usize, at: Vec3, dir: Vec3, matter: Matter) {
        let pool = self.pool(layer.pool());
        table::emit(pool, layer, n, at, dir, matter);
    }

    /// [`Fx::emit`] in a colour of ours, at `scale` times the layer's size.
    pub fn emit_tint(&mut self, layer: Layer, n: usize, at: Vec3, dir: Vec3, tint: Color, scale: f32) {
        let pool = self.pool(layer.pool());
        table::emit_tint(pool, layer, n, at, dir, tint, scale);
    }

    /// What `weapon` striking `matter` at `at` throws, around `away` (the
    /// surface's normal, or back toward the shooter), thinned with distance:
    /// its particles, its chips (landing on a floor at height `floor`,
    /// `surface::floor_under`), a shield's blink. How much was thrown
    /// (`table::lod`), 0 past the range anything is.
    pub fn impact(&mut self, weapon: Weapon, matter: Matter, at: Vec3, away: Vec3, floor: f32) -> f32 {
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
        let n = table::scaled(def.chips, lod);
        if n > 0 {
            self.chips.throw(at, away, matter, n, floor);
        }
        if matter == Matter::Shield && lod > 0.0 {
            self.flash(at, ZAP_FLASH);
        }
        if let Some(tone) = weapon.tone() {
            if lod > 0.0 && matter != Matter::Shield {
                let big = if weapon == Weapon::Beam { 0.5 } else { 0.3 };
                self.energy(at + away * 0.05, tone, big, 1);
            }
        }
        lod
    }

    /// `n` baked bursts of energy at `at` in `tone`, `scale` times the
    /// layer's size, each its own roll of the sheet.
    pub fn energy(&mut self, at: Vec3, tone: f32, scale: f32, n: usize) {
        let r = ::fx::recipe::Recipe {
            tone,
            ..table::recipe(Layer::Plasma)
        };
        let pool = self.pool(Layer::Plasma.pool());
        table::emit_recipe(pool, &r, n, at, Vec3::Y, Color::WHITE, scale);
    }

    /// [`Fx::impact`] on the map's face `hit`, and the mark the blow leaves
    /// there (`marks.rs`).
    pub fn strike(&mut self, map: &Map, weapon: Weapon, matter: Matter, hit: &surface::Hit) {
        let lod = self.impact(
            weapon,
            matter,
            hit.at,
            hit.normal,
            surface::floor_under(map, hit.at, hit.normal),
        );
        if lod <= 0.0 {
            return;
        }
        if let Some((k, size)) = marks::kind(weapon, matter) {
            self.marks.lay(map, k, size, hit.at, hit.normal, matter);
        }
    }

    /// Blood for the body in `slot`, struck at `at` along `dir`: its spray
    /// and drops every blow, a splat behind it once a frame (a shotgun's
    /// pellets are a dozen blows).
    pub fn bleed(&mut self, map: &Map, slot: u8, weapon: Weapon, at: Vec3, dir: Vec3) {
        let lod = self.impact(weapon, Matter::Flesh, at, -dir, surface::floor_under(map, at, -dir));
        let bit = 1u64 << (slot % 64);
        if lod > 0.0 && self.bled & bit == 0 {
            self.bled |= bit;
            self.marks.bleed(map, at, dir);
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

/// One of the particle meshes.
#[derive(Component)]
pub struct FxMesh(Which);

/// Both pools, their meshes and materials over the generated sheet, the
/// flash lights, the marks. Always drawn (collapsed quads until something
/// flies), so their pipelines compile at load and not on the first spark.
#[allow(clippy::too_many_arguments)]
pub fn setup(
    mut commands: Commands,
    tier: Res<Tier>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut sheet_mats: ResMut<Assets<SheetMaterial>>,
    mut shock_mats: ResMut<Assets<shell::ShockMaterial>>,
    mut images: ResMut<Assets<Image>>,
    assets: Res<AssetServer>,
) {
    let caps = Caps::for_effects(tier.gfx.effects);
    let sheet = images.add(atlas::sprite_image());
    let glow_mesh = meshes.add(pool_mesh(caps.glow));
    let soft_mesh = meshes.add(pool_mesh(caps.soft));
    let cloud_mesh = meshes.add(sheets::sheet_mesh(CLOUD));
    let boom_mesh = meshes.add(sheets::sheet_mesh(BOOM));
    // The sun's shadow on the smoke where the frame has room for the read:
    // not under WebGL2, whose fragment binds are counted (`gpu.rs`).
    let shadowed = !crate::look::gpu::WEBGL2;
    let (ca, cb) = sheets::load(&assets, "cloud");
    let cloud_mat = sheet_mats.add(sheets::material(ca, cb, shadowed));
    let (ba, bb) = sheets::load(&assets, "boom");
    let boom_mat = sheet_mats.add(sheets::material(ba, bb, shadowed));
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
        // A puff is not a surface (`fresnel::NO_SURFACE`).
        reflectance: super::fresnel::NO_SURFACE,
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
    for (which, mesh, mat) in [
        (Which::Soft, soft_mesh.clone(), soft),
        (Which::Glow, glow_mesh.clone(), glow),
    ] {
        commands.spawn((
            FxMesh(which),
            Mesh3d(mesh),
            MeshMaterial3d(mat),
            Transform::IDENTITY,
            Visibility::Visible,
            NoFrustumCulling,
            NotShadowCaster,
        ));
    }
    for (which, mesh, mat) in [
        (Which::Cloud, cloud_mesh.clone(), cloud_mat),
        (Which::Boom, boom_mesh.clone(), boom_mat),
    ] {
        commands.spawn((
            FxMesh(which),
            Mesh3d(mesh),
            MeshMaterial3d(mat),
            Transform::IDENTITY,
            Visibility::Visible,
            NoFrustumCulling,
            NotShadowCaster,
        ));
    }
    shell::setup(&mut commands, &mut meshes, &mut shock_mats);
    let mut marks = marks::Marks::new(caps.marks);
    marks::setup(&mut commands, &mut marks, &mut meshes, &mut mats, &mut images);
    let mut chips = chips::Chips::default();
    chips::setup(&mut commands, &mut chips, &mut meshes, &mut mats);
    commands.insert_resource(Fx {
        glow: Pool::new(caps.glow, false),
        soft: Pool::new(caps.soft, true),
        cloud: Pool::with_sheet(CLOUD, true, sheets::CLOUD),
        boom: Pool::with_sheet(BOOM, true, sheets::BOOM),
        marks,
        chips,
        near: surface::Near::default(),
        bled: 0,
        shake: Shake::default(),
        caps,
        flashes: [Flash::default(); MAX_LIGHTS],
        glow_mesh,
        soft_mesh,
        cloud_mesh,
        boom_mesh,
        eye: Vec3::ZERO,
        cam: Quat::IDENTITY,
        rounds: [0; 64],
        flashed: 0,
        rng: 0x9e37_79b9,
        was: bodies::Was::default(),
        hulls: [hulls::Hull::default(); 16],
        burning: [blast::Burn::default(); 4],
        trailers: [blast::Trailer::default(); 16],
        shocks: [shell::Shock::default(); shell::SHELLS],
        shock: None,
        own_rounds: Default::default(),
        own_flash: [None; 2],
        last_light: Vec::new(),
        swing: (0, None),
    });
}

/// The EFFECTS row moved: the spark pool at its new size (what was flying
/// in it goes) and the flash lights it may use. Smoke and marks stay.
pub fn retier(tier: Res<Tier>, fx: Option<ResMut<Fx>>, mut meshes: ResMut<Assets<Mesh>>) {
    let Some(mut fx) = fx else {
        return;
    };
    let caps = Caps::for_effects(tier.gfx.effects);
    if fx.caps == caps {
        return;
    }
    if caps.glow != fx.caps.glow {
        fx.glow = Pool::new(caps.glow, false);
        if let Some(m) = meshes.get_mut(&fx.glow_mesh) {
            *m = pool_mesh(caps.glow);
        }
    }
    fx.caps = caps;
}

/// What the world looks like to the effects this frame: the map, who's in
/// view, and who we are.
pub struct Scene<'a> {
    pub map: &'a Map,
    pub others: &'a [Drawn],
    pub vehicles: &'a [DrawnVehicle],
    pub me: u8,
    /// Where we aim (a unit vector): our own swing's way.
    pub aim: Vec3,
    /// The time now, seconds.
    pub t: f32,
}

/// One frame's events: what they look like.
pub fn on_events(fx: &mut Fx, scene: &Scene, events: &[EventState]) {
    fx.flashed = 0;
    fx.bled = 0;
    for e in events {
        let from = Vec3::new(e.pos.x, e.pos.y, e.pos.z);
        let to = Vec3::new(e.end.x, e.end.y, e.end.z);
        match e.kind {
            k if k == Kind::Shot as u8 && e.part != HEARD => shots::shot(fx, scene, e, from, to),
            k if k == Kind::Blast as u8 => blast::blast(fx, scene, from, e.c, e.part == HEARD),
            k if k == Kind::Wreck as u8 => blast::wreck(fx, scene, from),
            k if k == Kind::Spawn as u8 => bodies::spawned(fx, from),
            k if k == Kind::Pickup as u8 => bodies::picked_up(fx, from),
            k if k == Kind::Melee as u8 => bodies::swung(fx, scene, e, from),
            k if k == Kind::Hit as u8 => {
                // A hit on us: the frame flinches, by how much it took.
                if e.b == scene.me && e.a != scene.me {
                    let pts = e.c as f32;
                    let t = (crate::look::shake::HURT_TRAUMA_BASE + pts * crate::look::shake::HURT_TRAUMA_PER_PT)
                        .min(crate::look::shake::HURT_TRAUMA_MAX);
                    fx.shake.add(t);
                    let (p, y) = (fx.shake.rand(), fx.shake.rand());
                    fx.shake.kick(Vec3::new(p, y, 0.0) * crate::look::shake::FLINCH_RAD);
                }
                hurt(fx, scene, e, from);
            }
            _ => {}
        }
    }
}

/// Health taken (a hit's part 1 or 2), where the server told us the body
/// stood (it sends the place only of a body we may know of): blood behind
/// it, along the way from whoever struck it, unless a round's line already
/// bled it this frame (`shots.rs`). Our own body bleeds on the world, not
/// in front of our eyes.
fn hurt(fx: &mut Fx, scene: &Scene, e: &EventState, feet: Vec3) {
    if !(e.part == 1 || e.part == 2) || e.pos == V3::ZERO {
        return;
    }
    let chest = feet + Vec3::Y * 1.0;
    let from = if e.a == scene.me {
        Some(fx.eye)
    } else {
        scene
            .others
            .iter()
            .find(|d| d.slot == e.a && d.slot != e.b)
            .map(|d| Vec3::new(d.pos.x, d.pos.y + 1.4, d.pos.z))
    };
    let dir = match from {
        Some(f) if f.distance(chest) > 0.3 && e.a != e.b => (chest - f).normalize(),
        _ => {
            let a = fx.range(0.0, core::f32::consts::TAU);
            Vec3::new(a.cos(), -0.3, a.sin()).normalize()
        }
    };
    let bit = 1u64 << (e.b % 64);
    if fx.bled & bit != 0 {
        return;
    }
    if e.b == scene.me {
        fx.bled |= bit;
        fx.marks.bleed(scene.map, chest, dir);
        return;
    }
    fx.bleed(scene.map, e.b, Weapon::Melee, chest, dir);
}

/// Every frame, after the camera is placed: the pools step and are copied
/// into their meshes, the wrecks burn, the lights dim, the marks fade, the
/// chips fly, the shocks spread.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn draw(
    time: Res<Time>,
    mut fx: ResMut<Fx>,
    cams: Query<
        (&GlobalTransform, &Transform),
        (
            With<crate::look::rig::EyeCam>,
            Without<FxMesh>,
            Without<FlashLight>,
            Without<shell::ShockShell>,
        ),
    >,
    mut meshes: ResMut<Assets<Mesh>>,
    mut q: Query<(&FxMesh, &mut Transform, &mut GlobalTransform), Without<FlashLight>>,
    mut lights: Query<(&FlashLight, &mut PointLight, &mut Transform, &mut Visibility), Without<FxMesh>>,
    muzzles: Res<crate::look::viewmodel::ViewMuzzles>,
    mut shells: Query<(&shell::ShockShell, &mut Transform, &mut Visibility), (Without<FxMesh>, Without<FlashLight>)>,
    mut shock_mats: ResMut<Assets<shell::ShockMaterial>>,
) {
    // A hitch must not fling everything a hundred metres.
    let dt = time.delta_secs().min(0.1);
    let fx = &mut *fx;
    // Our own flash, off the camera as this frame places it (a root: its
    // transform is this frame's, its global one last frame's) and the gun
    // as this frame poses it (`draw_view` runs first).
    match cams.single() {
        Ok((_, placed)) => shots::muzzle(fx, placed, muzzles.0),
        Err(_) => fx.own_flash = [None; 2],
    }
    blast::burn(fx, dt);
    blast::trail(fx, dt);
    shell::draw(&mut fx.shocks, dt, &mut shells, &mut shock_mats);
    if let Some(mesh) = fx.marks.fade(dt).then(|| meshes.get_mut(fx.marks.mesh())).flatten() {
        fx.marks.write(mesh);
    }
    // Written, then stepped: a chip is drawn once where it was born.
    if fx.chips.needs_write() {
        if let Some(mesh) = meshes.get_mut(fx.chips.mesh()) {
            fx.chips.write(mesh);
        }
    }
    fx.chips.step(dt);
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
    let Ok((cam_gt, _)) = cams.single() else {
        fx.glow.step(dt);
        fx.soft.step(dt);
        fx.cloud.step(dt);
        fx.boom.step(dt);
        return;
    };
    let cam = pool::cam(cam_gt);
    let eye = cam_gt.translation();
    fx.eye = eye;
    fx.cam = cam_gt.compute_transform().rotation;
    let ahead: Vec3 = cam_gt.forward().into();
    for (m, mut tf, mut gt) in &mut q {
        // Transparent draws go far to near, and each mesh sorts where its
        // anchor stands: the baked clouds two metres ahead of the eye, the
        // soft dust one, the explosions half, the additive sparks at the
        // eye, so fire lands over smoke and sparks over all of it.
        let (pool, handle, anchor) = match m.0 {
            Which::Glow => (&mut fx.glow, &fx.glow_mesh, eye),
            Which::Soft => (&mut fx.soft, &fx.soft_mesh, eye + ahead),
            Which::Cloud => (&mut fx.cloud, &fx.cloud_mesh, eye + ahead * 2.0),
            Which::Boom => (&mut fx.boom, &fx.boom_mesh, eye + ahead * 0.5),
        };
        if tf.translation != anchor {
            tf.translation = anchor;
            *gt = GlobalTransform::from_translation(anchor);
        }
        if !pool.needs_write() {
            continue;
        }
        if let Some(mesh) = meshes.get_mut(handle) {
            match m.0 {
                Which::Cloud | Which::Boom => {
                    sheets::write(pool, mesh, &cam, anchor);
                }
                _ => {
                    pool.write(mesh, &cam, anchor);
                }
            }
        }
    }
    // Stepped after they are written, so everything thrown this frame is
    // drawn once where it was born: a flash shorter than a slow frame
    // (a browser on a weak GPU) would otherwise never be seen.
    fx.glow.step(dt);
    fx.soft.step(dt);
    fx.cloud.step(dt);
    fx.boom.step(dt);
}

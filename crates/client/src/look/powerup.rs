//! A power-up on its pad, drawn: no model, a sphere under its own shader
//! (`assets/shaders/powerup.wgsl`: the ball solved along each pixel's ray, a
//! heart in it, light curling through it, a shell of hard-light plates lit
//! along their seams), two rings turning round it, and a light on the pad
//! under it. The overshield a green star in its cage; camouflage a pale
//! glass bubble that wipes itself out and back. A halo of glow round it
//! gives it its size from across a map. The pad's root (`items.rs`)
//! bobs, turns and swells it; the rings turn by the clock in the shader, so
//! nothing here is written again once it stands. Drawing only.

use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::MeshTag;
use bevy::pbr::Material;
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;

pub const SHADER: &str = "shaders/powerup.wgsl";

/// The orb's radius, metres.
pub const RADIUS_M: f32 = 0.26;
/// How bright the orb, its rings and its halo are.
const GAIN: f32 = 1.0;
const RING_GAIN: f32 = 1.0;
const HALO_GAIN: f32 = 0.3;
/// The halo's radius, of the orb's.
const HALO_R: f32 = 2.3;
/// A ring: how far out (of the orb's radius) and how thick.
const RING_R: f32 = 1.42;
const RING_TUBE: f32 = 0.032;
/// The light it puts on the pad: lumens, how far it reaches (metres), and
/// how far over the orb's middle it hangs, below it (metres).
const LUMENS: f32 = 160_000.0;
const LIGHT_M: f32 = 7.0;
const LIGHT_DOWN_M: f32 = 0.1;

/// The plates on its shell: an icosahedron's faces each cut in nine, their
/// corners pushed out onto the sphere (12 pentagons and 80 hexagons, nearest
/// first). The shader's `CELLS` too.
pub const CELLS: usize = 92;

/// Which power-up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Over,
    Camo,
}

impl Kind {
    pub const ALL: [Kind; 2] = [Kind::Over, Kind::Camo];

    pub fn of(arm: u8) -> Option<Kind> {
        match arm {
            sim::arms::OVERSHIELD => Some(Kind::Over),
            sim::arms::CAMO => Some(Kind::Camo),
            _ => None,
        }
    }

    /// Linear HDR: in its depths, in its body (kept under 1, or the tone
    /// curve bleaches it white), in its heart and seams.
    const fn palette(self) -> [Vec4; 3] {
        match self {
            Kind::Over => [
                Vec4::new(0.0, 0.05, 0.012, 1.0),
                Vec4::new(0.02, 0.62, 0.12, 1.0),
                Vec4::new(0.25, 2.1, 0.45, 1.0),
            ],
            Kind::Camo => [
                Vec4::new(0.015, 0.03, 0.12, 1.0),
                Vec4::new(0.2, 0.36, 1.0, 1.0),
                Vec4::new(0.75, 1.3, 3.0, 1.0),
            ],
        }
    }

    /// Its light, linear.
    pub fn light(self) -> Color {
        match self {
            Kind::Over => Color::linear_rgb(0.25, 1.0, 0.4),
            Kind::Camo => Color::linear_rgb(0.5, 0.68, 1.0),
        }
    }
}

/// Field for field `Powerup` in the shader.
#[derive(Clone, Copy, Debug, ShaderType)]
pub struct PowerupParams {
    /// x how bright, y 0 the orb, 1 a ring or 2 its halo, z 0 the
    /// overshield or 1 camouflage, w a ring's tube's radius (metres).
    pub knobs: Vec4,
    pub deep: Vec4,
    pub mid: Vec4,
    pub hot: Vec4,
    pub cells: [Vec4; CELLS],
}

#[derive(Asset, AsBindGroup, TypePath, Clone, Debug)]
pub struct PowerupMaterial {
    #[uniform(0)]
    pub params: PowerupParams,
}

impl Material for PowerupMaterial {
    fn vertex_shader() -> ShaderRef {
        SHADER.into()
    }

    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Add
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }
}

/// The two meshes, and each kind's orb, ring and halo.
#[derive(Resource, Clone)]
pub struct Kit {
    pub ball: Handle<Mesh>,
    pub ring: Handle<Mesh>,
    orbs: [Handle<PowerupMaterial>; 2],
    rings: [Handle<PowerupMaterial>; 2],
    halos: [Handle<PowerupMaterial>; 2],
}

impl Kit {
    /// Every mesh with every material it wears, to warm their pipelines
    /// (`prewarm.rs`).
    pub fn all(&self) -> impl Iterator<Item = (&Handle<Mesh>, &Handle<PowerupMaterial>)> {
        let balls = self.orbs.iter().chain(&self.halos).map(|m| (&self.ball, m));
        balls.chain(self.rings.iter().map(|m| (&self.ring, m)))
    }
}

/// The plates' middles, on the unit sphere: each of an icosahedron's 20
/// faces cut in nine, every corner of the cut once (its own 12, two on
/// each of its 30 edges, one in each face).
fn cells() -> [Vec4; CELLS] {
    let g = (1.0 + 5f32.sqrt()) / 2.0;
    let mut corners = Vec::with_capacity(12);
    for a in [-1.0, 1.0] {
        for b in [-g, g] {
            corners.extend([Vec3::new(0.0, a, b), Vec3::new(a, b, 0.0), Vec3::new(b, 0.0, a)]);
        }
    }
    // Its edges are 2 long; the next nearest corners are further.
    let edge = |a: Vec3, b: Vec3| a.distance(b) < 2.1;
    let mut out: Vec<Vec4> = Vec::with_capacity(CELLS);
    for (i, &a) in corners.iter().enumerate() {
        for (j, &b) in corners.iter().enumerate().skip(i + 1) {
            for &c in corners.iter().skip(j + 1) {
                if !(edge(a, b) && edge(b, c) && edge(a, c)) {
                    continue;
                }
                for u in 0..=3 {
                    for v in 0..=3 - u {
                        let p = (a * u as f32 + b * v as f32 + c * (3 - u - v) as f32).normalize();
                        if out.iter().all(|q| q.truncate().distance(p) > 1e-3) {
                            out.push(p.extend(0.0));
                        }
                    }
                }
            }
        }
    }
    out.try_into().expect("a sphere of 92 plates")
}

pub fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<PowerupMaterial>>) {
    let cells = cells();
    let mut each = |gain: f32, part: f32, tube: f32| {
        Kind::ALL.map(|kind| {
            let [deep, mid, hot] = kind.palette();
            materials.add(PowerupMaterial {
                params: PowerupParams {
                    knobs: Vec4::new(gain, part, kind as u32 as f32, tube),
                    deep,
                    mid,
                    hot,
                    cells,
                },
            })
        })
    };
    let kit = Kit {
        ball: meshes.add(Sphere::new(1.0).mesh().ico(5).expect("a small icosphere")),
        ring: meshes.add(
            Torus::new(RING_R - RING_TUBE, RING_R + RING_TUBE)
                .mesh()
                .minor_resolution(8)
                .major_resolution(72),
        ),
        orbs: each(GAIN, 0.0, 0.0),
        rings: each(RING_GAIN, 1.0, RING_TUBE * RADIUS_M),
        halos: each(HALO_GAIN, 2.0, 0.0),
    };
    commands.insert_resource(kit);
}

/// `kind`'s orb, its rings and its light, for a pad's root to carry.
pub fn spawn(c: &mut ChildSpawnerCommands, kit: &Kit, kind: Kind) {
    let i = kind as usize;
    c.spawn((
        Mesh3d(kit.ball.clone()),
        MeshMaterial3d(kit.orbs[i].clone()),
        Transform::from_scale(Vec3::splat(RADIUS_M)),
        NotShadowCaster,
        NotShadowReceiver,
    ))
    .with_children(|o| {
        // Its tag says which ring, so which tilt and turn (`ring_turn`).
        for k in 0..2 {
            o.spawn((
                Mesh3d(kit.ring.clone()),
                MeshMaterial3d(kit.rings[i].clone()),
                MeshTag(k),
                Transform::default(),
                NotShadowCaster,
                NotShadowReceiver,
            ));
        }
        o.spawn((
            Mesh3d(kit.ball.clone()),
            MeshMaterial3d(kit.halos[i].clone()),
            Transform::from_scale(Vec3::splat(HALO_R)),
            NotShadowCaster,
            NotShadowReceiver,
        ));
    });
    c.spawn((
        Transform::from_xyz(0.0, -LIGHT_DOWN_M, 0.0),
        PointLight {
            color: kind.light(),
            intensity: LUMENS,
            range: LIGHT_M,
            radius: RADIUS_M,
            shadows_enabled: false,
            ..default()
        },
    ));
}

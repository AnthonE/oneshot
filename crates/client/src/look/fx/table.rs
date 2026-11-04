//! What a blow throws, by weapon × matter: the one table that picks sparks
//! or dust, arcs off a shield or a red mist off health. And each gun's own
//! look: its tracer, its flash. Numbers live here, in code.
//!
//! After Gates' `crates/client/src/render/fx/table.rs` (MIT, © 2026
//! MoreRight DAO): its layers and recipes, with our weapons and matters, and
//! the shield's arcs, zaps and gold sparks added.

use bevy::prelude::*;
use sim::arms::{
    AR, BEAM_RIFLE, BR, CANNON, CARBINE, ION_PISTOL, ION_RIFLE, LAUNCHER, MAGNUM, SHARDS, SHOTGUN, SMG, SNIPER,
};

use super::atlas::{ARC, FLAME, GLOW, PUFF, RING, SMOKE, SPECK, STAR, STREAK};
use super::pool::{Orient, Particle, Pool};

/// What struck.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Weapon {
    /// A rifle's, a pistol's, an SMG's round.
    Bullet,
    /// One of a shotgun's pellets.
    Pellet,
    /// A sniper round.
    Sniper,
    Melee,
    Blast,
}

impl Weapon {
    pub fn of_arm(arm: u8) -> Weapon {
        match arm {
            SHOTGUN => Weapon::Pellet,
            SNIPER | BEAM_RIFLE => Weapon::Sniper,
            _ => Weapon::Bullet,
        }
    }
}

/// What it struck.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Matter {
    /// A box: the arenas' walls, crates, floors.
    Concrete,
    Metal,
    /// The ground: soil, sand.
    Dirt,
    /// A fighter whose shield still holds.
    Shield,
    /// A fighter without one.
    Flesh,
    /// A vehicle.
    Hull,
}

/// One kind of thing an effect throws.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Layer {
    /// Hot streaks that fall and skitter once off the surface.
    Sparks,
    /// A star flash where it struck.
    Flash,
    /// A puff in the struck matter's colour.
    Dust,
    /// Specks of the struck matter thrown out and falling.
    Grit,
    Smoke,
    Fireball,
    /// The little grey breath a muzzle puts out.
    GunSmoke,
    /// A big slow cloud in the struck matter's colour.
    Plume,
    /// Lightning crawling off a shield.
    Arcs,
    /// A shield's blue-white crack of light.
    Zap,
    /// Gold specks thrown off a shield.
    ShieldSparks,
    /// A dark red mist off health.
    Hurt,
    /// A ring spreading along the ground.
    Shock,
    /// A muzzle's tongue of fire.
    Tongue,
}

impl Layer {
    /// Whether this layer is light (the additive pool) or matter (the alpha
    /// pool, lit like the world).
    pub fn glow(self) -> bool {
        matches!(
            self,
            Layer::Sparks
                | Layer::Flash
                | Layer::Fireball
                | Layer::Arcs
                | Layer::Zap
                | Layer::ShieldSparks
                | Layer::Shock
                | Layer::Tongue
        )
    }
}

/// What one blow throws: up to five layers with a count each.
#[derive(Clone, Copy, Debug)]
pub struct EffectDef {
    pub layers: [(Layer, u8); 5],
}

impl EffectDef {
    fn of(layers: &[(Layer, u8)]) -> Self {
        let mut out = [(Layer::Dust, 0u8); 5];
        out[..layers.len()].copy_from_slice(layers);
        Self { layers: out }
    }

    /// How many of `layer` this effect throws.
    pub fn count(&self, layer: Layer) -> u8 {
        self.layers.iter().filter(|(l, _)| *l == layer).map(|(_, n)| *n).sum()
    }
}

/// The effect of a blow. Metal showers sparks; concrete flashes a few in a
/// grey cloud; the ground is dust and grit; a shield crackles; health mists.
pub fn effect(weapon: Weapon, matter: Matter) -> EffectDef {
    use Layer::*;
    use Matter::*;
    match (weapon, matter) {
        (Weapon::Blast, _) => EffectDef::of(&[(Flash, 2), (Fireball, 14), (Smoke, 9), (Dust, 8), (Sparks, 28)]),
        (Weapon::Sniper, Shield) => EffectDef::of(&[(Zap, 1), (Arcs, 6), (ShieldSparks, 16)]),
        (_, Shield) => EffectDef::of(&[(Zap, 1), (Arcs, 3), (ShieldSparks, 7)]),
        (Weapon::Pellet, Flesh) => EffectDef::of(&[(Hurt, 1)]),
        (_, Flesh) => EffectDef::of(&[(Hurt, 3)]),
        (Weapon::Melee, Metal | Hull) => EffectDef::of(&[(Sparks, 14), (Flash, 1)]),
        (Weapon::Melee, Concrete) => EffectDef::of(&[(Sparks, 4), (Dust, 2), (Grit, 4)]),
        (Weapon::Melee, Dirt) => EffectDef::of(&[(Dust, 3), (Grit, 5)]),
        (Weapon::Bullet, Metal | Hull) => EffectDef::of(&[(Sparks, 12), (Flash, 1), (Dust, 1)]),
        (Weapon::Bullet, Concrete) => EffectDef::of(&[(Sparks, 4), (Flash, 1), (Dust, 2), (Grit, 6)]),
        (Weapon::Bullet, Dirt) => EffectDef::of(&[(Dust, 3), (Grit, 8)]),
        (Weapon::Pellet, Metal | Hull) => EffectDef::of(&[(Sparks, 4), (Flash, 1)]),
        (Weapon::Pellet, Concrete) => EffectDef::of(&[(Sparks, 1), (Dust, 1), (Grit, 2)]),
        (Weapon::Pellet, Dirt) => EffectDef::of(&[(Dust, 1), (Grit, 3)]),
        (Weapon::Sniper, Metal | Hull) => EffectDef::of(&[(Sparks, 24), (Flash, 2), (Dust, 2)]),
        (Weapon::Sniper, Concrete) => EffectDef::of(&[(Sparks, 9), (Flash, 1), (Dust, 4), (Grit, 12), (Plume, 2)]),
        (Weapon::Sniper, Dirt) => EffectDef::of(&[(Dust, 6), (Grit, 14), (Plume, 2)]),
    }
}

/// How much of an effect is thrown at `dist` metres from the eye: all of it
/// up close, less with distance, nothing past 100 m.
pub fn lod(dist: f32) -> f32 {
    if dist <= 25.0 {
        1.0
    } else if dist <= 50.0 {
        0.5
    } else if dist <= 100.0 {
        0.25
    } else {
        0.0
    }
}

/// [`lod`] for a blast, which is seen much further away.
pub fn lod_far(dist: f32) -> f32 {
    if dist <= 120.0 {
        1.0
    } else if dist <= 250.0 {
        0.5
    } else {
        0.25
    }
}

/// `count` scaled by `lod`, never rounding a layer in range to nothing.
pub fn scaled(count: u8, lod: f32) -> usize {
    if count == 0 || lod <= 0.0 {
        0
    } else {
        ((count as f32 * lod).ceil() as usize).max(1)
    }
}

/// A dust cloud's colour per matter, sRGB.
pub fn dust_tint(m: Matter) -> Color {
    match m {
        Matter::Concrete => Color::srgb(0.66, 0.64, 0.60),
        Matter::Metal | Matter::Hull => Color::srgb(0.52, 0.49, 0.44),
        Matter::Dirt => Color::srgb(0.66, 0.52, 0.38),
        Matter::Shield => Color::srgb(1.0, 0.85, 0.4),
        Matter::Flesh => Color::srgb(0.40, 0.05, 0.05),
    }
}

/// One layer's recipe.
struct Spec {
    orient: Orient,
    cell: u8,
    cells: u8,
    speed: (f32, f32),
    /// Share of the launch along the thrown direction rather than scattered.
    spray: f32,
    /// Extra upward speed, m/s.
    up: f32,
    gravity: f32,
    drag: f32,
    life: (f32, f32),
    size: (f32, f32),
    /// End size over start size.
    grow: f32,
    c0: [f32; 4],
    c1: [f32; 4],
    /// Multiply the colours by the matter's dust tint.
    tinted: bool,
    spin: f32,
    stretch: f32,
    len_max: f32,
    bounce: bool,
}

fn spec(layer: Layer) -> Spec {
    let base = Spec {
        orient: Orient::Billboard,
        cell: PUFF,
        cells: 1,
        speed: (0.0, 0.0),
        spray: 0.5,
        up: 0.0,
        gravity: 0.0,
        drag: 0.0,
        life: (0.5, 0.5),
        size: (0.05, 0.05),
        grow: 1.0,
        c0: [1.0; 4],
        c1: [1.0, 1.0, 1.0, 0.0],
        tinted: false,
        spin: 0.0,
        stretch: 0.0,
        len_max: 0.0,
        bounce: false,
    };
    match layer {
        Layer::Sparks => Spec {
            orient: Orient::Stretch,
            cell: STREAK,
            speed: (3.0, 9.0),
            spray: 0.65,
            up: 0.8,
            gravity: 9.81,
            drag: 2.4,
            life: (0.16, 0.42),
            size: (0.006, 0.008),
            grow: 0.5,
            c0: [22.0, 13.0, 5.0, 1.0],
            c1: [2.2, 0.35, 0.05, 0.0],
            stretch: 0.016,
            bounce: true,
            ..base
        },
        Layer::Flash => Spec {
            cell: STAR,
            cells: 3,
            life: (0.05, 0.08),
            size: (0.06, 0.1),
            grow: 1.4,
            c0: [7.0, 5.5, 3.0, 1.0],
            c1: [1.5, 0.8, 0.3, 0.0],
            ..base
        },
        Layer::Dust => Spec {
            cells: 3,
            speed: (0.4, 1.6),
            spray: 0.55,
            up: 0.2,
            gravity: -0.25,
            drag: 2.8,
            life: (0.8, 1.4),
            size: (0.05, 0.08),
            grow: 6.0,
            c0: [0.95, 0.95, 0.95, 0.4],
            c1: [1.05, 1.05, 1.05, 0.0],
            tinted: true,
            spin: 0.8,
            ..base
        },
        Layer::Grit => Spec {
            cell: SPECK,
            speed: (2.0, 4.5),
            spray: 0.6,
            up: 1.0,
            gravity: 9.81,
            drag: 0.6,
            life: (0.35, 0.7),
            size: (0.010, 0.014),
            grow: 0.8,
            c0: [0.55, 0.55, 0.55, 1.0],
            c1: [0.55, 0.55, 0.55, 0.7],
            tinted: true,
            bounce: true,
            ..base
        },
        Layer::Smoke => Spec {
            cell: SMOKE,
            cells: 2,
            speed: (0.3, 1.0),
            spray: 0.8,
            gravity: -0.6,
            drag: 1.2,
            life: (2.5, 4.5),
            size: (0.3, 0.45),
            grow: 4.0,
            c0: [0.25, 0.24, 0.23, 0.55],
            c1: [0.35, 0.35, 0.35, 0.0],
            spin: 0.3,
            ..base
        },
        Layer::GunSmoke => Spec {
            cell: SMOKE,
            cells: 2,
            speed: (0.6, 1.6),
            spray: 0.85,
            gravity: -0.3,
            drag: 3.0,
            life: (0.6, 1.0),
            size: (0.04, 0.06),
            grow: 5.0,
            c0: [0.55, 0.55, 0.55, 0.28],
            c1: [0.6, 0.6, 0.6, 0.0],
            spin: 0.6,
            ..base
        },
        Layer::Plume => Spec {
            cells: 3,
            speed: (0.5, 2.0),
            spray: 0.5,
            up: 0.5,
            gravity: -0.12,
            drag: 2.2,
            life: (1.4, 2.4),
            size: (0.14, 0.24),
            grow: 4.0,
            c0: [0.95, 0.95, 0.95, 0.42],
            c1: [1.05, 1.05, 1.05, 0.0],
            tinted: true,
            spin: 0.4,
            ..base
        },
        Layer::Fireball => Spec {
            cell: GLOW,
            speed: (1.0, 5.0),
            spray: 0.3,
            gravity: -1.0,
            drag: 3.0,
            life: (0.25, 0.55),
            size: (0.35, 0.55),
            grow: 3.0,
            c0: [14.0, 6.0, 1.5, 1.0],
            c1: [1.2, 0.2, 0.02, 0.0],
            spin: 0.5,
            ..base
        },
        // Lightning: a jagged bolt flung out and gone in a blink, rolled
        // at random so no two strikes match.
        Layer::Arcs => Spec {
            cell: ARC,
            speed: (0.0, 0.6),
            spray: 0.3,
            life: (0.05, 0.11),
            size: (0.18, 0.32),
            grow: 1.3,
            c0: [3.0, 6.0, 16.0, 1.0],
            c1: [1.0, 2.0, 6.0, 0.0],
            spin: 6.0,
            ..base
        },
        Layer::Zap => Spec {
            cell: GLOW,
            life: (0.06, 0.09),
            size: (0.18, 0.24),
            grow: 1.8,
            c0: [6.0, 9.0, 16.0, 1.0],
            c1: [0.8, 1.4, 3.0, 0.0],
            ..base
        },
        Layer::ShieldSparks => Spec {
            orient: Orient::Stretch,
            cell: STREAK,
            speed: (2.0, 6.5),
            spray: 0.6,
            gravity: 3.0,
            drag: 3.0,
            life: (0.12, 0.3),
            size: (0.006, 0.008),
            grow: 0.5,
            c0: [9.0, 6.0, 1.0, 1.0],
            c1: [2.5, 1.0, 0.1, 0.0],
            stretch: 0.02,
            ..base
        },
        Layer::Hurt => Spec {
            cells: 3,
            speed: (0.5, 1.8),
            spray: 0.6,
            gravity: 0.6,
            drag: 4.0,
            life: (0.3, 0.5),
            size: (0.04, 0.06),
            grow: 5.0,
            c0: [0.20, 0.004, 0.004, 0.85],
            c1: [0.24, 0.01, 0.01, 0.0],
            spin: 1.0,
            ..base
        },
        Layer::Shock => Spec {
            orient: Orient::Flat,
            cell: RING,
            life: (0.28, 0.32),
            size: (0.4, 0.5),
            grow: 13.0,
            c0: [3.0, 2.4, 1.6, 0.6],
            c1: [0.6, 0.4, 0.2, 0.0],
            ..base
        },
        Layer::Tongue => Spec {
            orient: Orient::Stretch,
            cell: FLAME,
            speed: (0.5, 0.5),
            spray: 1.0,
            life: (0.035, 0.05),
            size: (0.05, 0.07),
            grow: 0.7,
            c0: [12.0, 6.5, 2.0, 1.0],
            c1: [3.0, 1.0, 0.2, 0.0],
            stretch: 0.8,
            len_max: 0.6,
            ..base
        },
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Throw `n` particles of `layer` from `at`, around `dir`, coloured for
/// `matter`. Sparks and grit bounce once off the plane through `at` facing
/// `dir`: the surface they came off.
pub fn emit(pool: &mut Pool, layer: Layer, n: usize, at: Vec3, dir: Vec3, matter: Matter) {
    emit_tint(pool, layer, n, at, dir, dust_tint(matter), 1.0);
}

/// [`emit`] in a colour of the caller's, at `scale` times the layer's size.
/// A layer that is not tinted ignores the colour.
pub fn emit_tint(pool: &mut Pool, layer: Layer, n: usize, at: Vec3, dir: Vec3, tint: Color, scale: f32) {
    let s = spec(layer);
    let tint = if s.tinted {
        tint.to_linear().to_f32_array_no_alpha()
    } else {
        [1.0; 3]
    };
    let paint = |c: [f32; 4]| [c[0] * tint[0], c[1] * tint[1], c[2] * tint[2], c[3]];
    let dir = dir.normalize_or(Vec3::Y);
    for _ in 0..n {
        let scatter = Vec3::new(pool.signed(), pool.signed(), pool.signed()).normalize_or(Vec3::Y);
        let v = (dir * s.spray + scatter * (1.0 - s.spray)).normalize_or(dir);
        let speed = lerp(s.speed.0, s.speed.1, pool.roll());
        let life = lerp(s.life.0, s.life.1, pool.roll());
        let size = lerp(s.size.0, s.size.1, pool.roll()) * scale;
        let cell = s.cell + ((pool.roll() * s.cells as f32) as u8).min(s.cells - 1);
        let roll = pool.roll() * core::f32::consts::TAU;
        let spin = pool.signed() * s.spin;
        pool.spawn(Particle {
            left: life,
            life,
            // A couple of centimetres off the surface, so nothing is born
            // inside it.
            pos: at + v * 0.02,
            vel: v * speed * scale.sqrt() + Vec3::Y * s.up,
            size0: size,
            size1: size * s.grow,
            c0: paint(s.c0),
            c1: paint(s.c1),
            gravity: s.gravity,
            drag: s.drag,
            roll,
            spin,
            cell,
            orient: s.orient,
            stretch: s.stretch,
            len_max: s.len_max,
            plane_p: at,
            plane_n: if s.bounce { dir } else { Vec3::ZERO },
        });
    }
}

/// A gun's look when it fires.
#[derive(Clone, Copy, Debug)]
pub struct GunLook {
    /// The tracer's colour, linear HDR, at its head and as it goes.
    pub c0: [f32; 4],
    pub c1: [f32; 4],
    /// Every how many rounds a tracer is drawn (1: each).
    pub every: u16,
    /// The tracer's drawn speed (m/s), longest streak (m), width (m).
    pub mps: f32,
    pub len_m: f32,
    pub width: f32,
    /// A smoke wake along the line that hangs this long (s); 0: none.
    pub wake_s: f32,
    /// The muzzle: flash stars, their colour and size, the light (lumens).
    pub stars: u8,
    pub flash_c: [f32; 4],
    pub flash_scale: f32,
    pub light_lm: f32,
}

const RIFLE: GunLook = GunLook {
    c0: [6.0, 3.6, 1.1, 0.9],
    c1: [3.5, 1.8, 0.5, 0.5],
    every: 1,
    mps: 300.0,
    len_m: 5.0,
    width: 0.014,
    wake_s: 0.0,
    stars: 2,
    flash_c: [7.0, 5.5, 3.0, 1.0],
    flash_scale: 1.0,
    light_lm: 120_000.0,
};

pub fn gun(arm: u8) -> GunLook {
    // Blue-white, the precision rifles' and the SMG's; the sniper's a hot red.
    let blue = ([4.0, 7.0, 12.0, 0.9], [1.5, 3.0, 6.0, 0.5]);
    match arm {
        AR => GunLook { every: 2, ..RIFLE },
        SMG => GunLook {
            c0: blue.0,
            c1: blue.1,
            every: 2,
            width: 0.012,
            flash_scale: 0.8,
            ..RIFLE
        },
        BR => GunLook {
            c0: blue.0,
            c1: blue.1,
            mps: 340.0,
            ..RIFLE
        },
        MAGNUM => GunLook {
            width: 0.016,
            flash_scale: 1.2,
            stars: 3,
            ..RIFLE
        },
        SNIPER => GunLook {
            c0: [16.0, 3.0, 1.6, 1.0],
            c1: [6.0, 0.8, 0.3, 0.6],
            mps: 600.0,
            len_m: 14.0,
            width: 0.022,
            wake_s: 1.6,
            stars: 3,
            flash_scale: 2.0,
            light_lm: 260_000.0,
            ..RIFLE
        },
        SHOTGUN => GunLook {
            every: 3,
            mps: 220.0,
            len_m: 2.5,
            width: 0.010,
            stars: 3,
            flash_scale: 1.9,
            light_lm: 220_000.0,
            ..RIFLE
        },
        // Green rounds, a green flash.
        CARBINE => GunLook {
            c0: [2.0, 9.0, 3.0, 0.9],
            c1: [0.6, 3.5, 1.0, 0.5],
            flash_c: [3.0, 9.0, 3.5, 1.0],
            mps: 340.0,
            ..RIFLE
        },
        // A beam: pink-violet, all of its line in a frame.
        BEAM_RIFLE => GunLook {
            c0: [9.0, 3.0, 14.0, 1.0],
            c1: [3.0, 0.8, 5.0, 0.6],
            flash_c: [8.0, 3.0, 12.0, 1.0],
            mps: 3000.0,
            len_m: 60.0,
            width: 0.03,
            stars: 3,
            flash_scale: 1.5,
            light_lm: 200_000.0,
            ..RIFLE
        },
        // What flies is drawn in flight (`flying.rs`): only the muzzle here.
        ION_RIFLE | ION_PISTOL => GunLook {
            flash_c: [2.0, 5.0, 14.0, 1.0],
            flash_scale: 0.8,
            light_lm: 90_000.0,
            ..RIFLE
        },
        SHARDS => GunLook {
            flash_c: PINK,
            flash_scale: 0.8,
            light_lm: 90_000.0,
            ..RIFLE
        },
        CANNON => GunLook {
            flash_c: [3.0, 12.0, 4.0, 1.0],
            stars: 3,
            flash_scale: 2.2,
            light_lm: 300_000.0,
            ..RIFLE
        },
        LAUNCHER => GunLook {
            stars: 3,
            flash_scale: 1.8,
            light_lm: 200_000.0,
            ..RIFLE
        },
        _ => RIFLE,
    }
}

/// The SHARD GUN's crystals: a hot pink, linear HDR.
pub const PINK: [f32; 4] = [9.0, 2.2, 6.0, 1.0];

/// A muzzle's flash: `n` stars in colour `c`, `scale` times an impact's.
pub fn stars(pool: &mut Pool, at: Vec3, n: usize, scale: f32, c: [f32; 4]) {
    for _ in 0..n {
        let life = 0.05 + 0.03 * pool.roll();
        let size = (0.06 + 0.04 * pool.roll()) * scale;
        let cell = STAR + ((pool.roll() * 3.0) as u8).min(2);
        let roll = pool.roll() * core::f32::consts::TAU;
        pool.spawn(Particle {
            left: life,
            life,
            pos: at,
            size0: size,
            size1: size * 1.4,
            c0: c,
            c1: [c[0] * 0.2, c[1] * 0.15, c[2] * 0.1, 0.0],
            roll,
            cell,
            ..Particle::default()
        });
    }
}

/// A crystal shattering (on a wall, a hull, or in the burst): pink shards
/// flung out, a pink blink, `n` of them.
pub fn shatter(glow: &mut Pool, at: Vec3, n: usize, speed: f32) {
    for _ in 0..n {
        let d = Vec3::new(glow.signed(), glow.signed(), glow.signed()).normalize_or(Vec3::Y);
        let life = 0.18 + 0.25 * glow.roll();
        let v = d * speed * (0.5 + glow.roll());
        glow.spawn(Particle {
            left: life,
            life,
            pos: at,
            vel: v,
            gravity: 6.0,
            drag: 2.0,
            size0: 0.025,
            size1: 0.012,
            c0: PINK,
            c1: [2.0, 0.3, 1.2, 0.0],
            cell: super::atlas::SHARD,
            orient: Orient::Stretch,
            stretch: 0.03,
            len_max: 0.12,
            ..Particle::default()
        });
    }
    glow.spawn(Particle {
        left: 0.08,
        life: 0.08,
        pos: at,
        size0: 0.12,
        size1: 0.3,
        c0: PINK,
        c1: [1.5, 0.3, 1.0, 0.0],
        cell: GLOW,
        ..Particle::default()
    });
}

/// A tracer from `start` along `dir`, `dist` metres, in `look`'s colours.
pub fn tracer(pool: &mut Pool, look: &GunLook, start: Vec3, dir: Vec3, dist: f32) {
    if dist <= 1.0 {
        return;
    }
    let life = (dist / look.mps).clamp(0.02, 1.0);
    pool.spawn(Particle {
        left: life,
        life,
        pos: start,
        vel: dir * look.mps,
        size0: look.width,
        size1: look.width * 0.7,
        c0: look.c0,
        c1: look.c1,
        cell: STREAK,
        orient: Orient::Stretch,
        stretch: 0.02,
        len_max: look.len_m,
        ..Particle::default()
    });
}

/// A smoke wake hanging along a line (a sniper round's), as puffs.
pub fn wake(pool: &mut Pool, from: Vec3, to: Vec3, life: f32) {
    let len = from.distance(to);
    let n = ((len / 1.2) as usize).clamp(1, 40);
    for k in 0..n {
        let t = (k as f32 + pool.roll()) / n as f32;
        let at = from.lerp(to, t);
        let l = life * (0.8 + 0.4 * pool.roll());
        let roll = pool.roll() * core::f32::consts::TAU;
        let vel = Vec3::new(pool.signed(), pool.signed() + 0.6, pool.signed()) * 0.15;
        let spin = pool.signed() * 0.3;
        pool.spawn(Particle {
            left: l,
            life: l,
            pos: at,
            vel,
            size0: 0.05,
            size1: 0.32,
            c0: [0.8, 0.8, 0.82, 0.3],
            c1: [0.85, 0.85, 0.85, 0.0],
            drag: 0.8,
            roll,
            spin,
            cell: SMOKE + (k % 2) as u8,
            ..Particle::default()
        });
    }
}

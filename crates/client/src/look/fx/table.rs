//! What a blow throws, by weapon × matter: the one table that picks sparks
//! or dust, arcs off a shield or a red mist off health. And each gun's own
//! look: its tracer, its flash. Numbers live here, in code.
//!
//! After Gates' `crates/client/src/render/fx/table.rs` (MIT, © 2026
//! MoreRight DAO): its layers and recipes, with our weapons and matters, and
//! the shield's arcs, zaps and gold sparks added; its blood drops off a body
//! (drawn as specks: our sheet has no droplet cell, `atlas.rs`) and the lit
//! chips each blow throws (`chips.rs`, after its `impact.rs`).

use bevy::prelude::*;
use sim::arms::{
    AR, BEAM_RIFLE, BR, CANNON, CARBINE, ION_PISTOL, ION_RIFLE, LAUNCHER, MAGNUM, ROCKET, SHARDS, SHOTGUN, SMG, SNIPER,
};

use super::atlas::{ARC, FLAME, GLOW, PUFF, RING, SMOKE, SPECK, STAR, STREAK};
use super::pool::{Orient, Particle, Pool};
use super::sheets::{self, Strip, TONE_FIRE};
use ::fx::recipe::Recipe;
use std::sync::OnceLock;

/// What struck.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Weapon {
    /// A rifle's, a pistol's, an SMG's round.
    Bullet,
    /// One of a shotgun's pellets.
    Pellet,
    /// A sniper round.
    Sniper,
    /// A rifle butt, a blow.
    Melee,
    /// The sword's cut.
    Blade,
    Blast,
    /// The carbine's green round: energy, a scorch.
    Carbine,
    /// The beam rifle's beam: energy, a bigger scorch.
    Beam,
}

impl Weapon {
    pub fn of_arm(arm: u8) -> Weapon {
        match arm {
            SHOTGUN => Weapon::Pellet,
            SNIPER => Weapon::Sniper,
            CARBINE => Weapon::Carbine,
            BEAM_RIFLE => Weapon::Beam,
            _ => Weapon::Bullet,
        }
    }

    /// An energy weapon's tone (`sheets`), for the burst its round leaves.
    pub fn tone(self) -> Option<f32> {
        match self {
            Weapon::Carbine => Some(sheets::TONE_GREEN),
            Weapon::Beam => Some(sheets::TONE_PINK),
            _ => None,
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
    /// Blood thrown off a body and falling.
    BloodDrops,
    /// A ring spreading along the ground.
    Shock,
    /// A muzzle's tongue of fire.
    Tongue,
    /// A baked puff of smoke (`sheets::SMOKE`): lit six ways, rolling.
    Cloud,
    /// A baked cloud of dust off the ground (`sheets::DUST`), low and wide.
    DustCloud,
    /// Baked tongues of fire (`sheets::FIRE`), for a thing that burns.
    Flame,
    /// A baked burst of energy (`sheets::PLASMA`): heat only, in a tone.
    Plasma,
    /// A gun's baked flash (`sheets::FLASH`): a core and petals, four frames.
    MuzzleFlash,
    /// An energy gun's baked flash (`sheets::BLOOM`): a disc and a ring.
    Bloom,
    /// The baked breath of smoke off a muzzle (`sheets::WISP`).
    Wisp,
    /// A baked explosion (`sheets::EXPLOSION`): fire to soot, in place.
    Boom,
    /// Smaller explosions flung off the main one, each its own roll.
    Fireballs,
    /// Glowing specks that drift and fall long after the sparks are gone.
    Embers,
    /// White water thrown up where the swell meets rock, falling back
    /// (`sheets::SMOKE`'s rolling puff, in the sea's white: `ambient.rs`).
    Spray,
    /// A mote in the sun: pollen, dust, a seed on the wind (`ambient.rs`).
    Mote,
}

/// Which pool a layer draws in.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Which {
    /// Additive, unlit, HDR: light.
    Glow,
    /// Blended, lit like the world, sorted: matter, on the generated sheet.
    Soft,
    /// The baked cloud sheet (`sheets::CLOUD`).
    Cloud,
    /// The baked explosions (`sheets::BOOM`).
    Boom,
}

impl Layer {
    /// Every layer, in the order the knobs name them.
    pub const ALL: [Layer; 27] = [
        Layer::Sparks,
        Layer::Flash,
        Layer::Dust,
        Layer::Grit,
        Layer::Smoke,
        Layer::Fireball,
        Layer::GunSmoke,
        Layer::Plume,
        Layer::Arcs,
        Layer::Zap,
        Layer::ShieldSparks,
        Layer::Hurt,
        Layer::BloodDrops,
        Layer::Shock,
        Layer::Tongue,
        Layer::Cloud,
        Layer::DustCloud,
        Layer::Flame,
        Layer::Plasma,
        Layer::MuzzleFlash,
        Layer::Bloom,
        Layer::Wisp,
        Layer::Boom,
        Layer::Fireballs,
        Layer::Embers,
        Layer::Spray,
        Layer::Mote,
    ];

    /// Its name, as a knob names it (`Sparks.speed=4,12`).
    pub fn name(self) -> &'static str {
        match self {
            Layer::Sparks => "Sparks",
            Layer::Flash => "Flash",
            Layer::Dust => "Dust",
            Layer::Grit => "Grit",
            Layer::Smoke => "Smoke",
            Layer::Fireball => "Fireball",
            Layer::GunSmoke => "GunSmoke",
            Layer::Plume => "Plume",
            Layer::Arcs => "Arcs",
            Layer::Zap => "Zap",
            Layer::ShieldSparks => "ShieldSparks",
            Layer::Hurt => "Hurt",
            Layer::BloodDrops => "BloodDrops",
            Layer::Shock => "Shock",
            Layer::Tongue => "Tongue",
            Layer::Cloud => "Cloud",
            Layer::DustCloud => "DustCloud",
            Layer::Flame => "Flame",
            Layer::Plasma => "Plasma",
            Layer::MuzzleFlash => "MuzzleFlash",
            Layer::Bloom => "Bloom",
            Layer::Wisp => "Wisp",
            Layer::Boom => "Boom",
            Layer::Fireballs => "Fireballs",
            Layer::Embers => "Embers",
            Layer::Spray => "Spray",
            Layer::Mote => "Mote",
        }
    }

    /// The layer a knob names, any case.
    pub fn named(name: &str) -> Option<Layer> {
        Layer::ALL.into_iter().find(|l| l.name().eq_ignore_ascii_case(name))
    }

    /// Whether this layer is light (the additive pool) or matter (the alpha
    /// pool, lit like the world).
    pub fn glow(self) -> bool {
        self.pool() == Which::Glow
    }

    /// The pool it draws in.
    pub fn pool(self) -> Which {
        match self {
            Layer::Sparks
            | Layer::Flash
            | Layer::Fireball
            | Layer::Arcs
            | Layer::Zap
            | Layer::ShieldSparks
            | Layer::Shock
            | Layer::Tongue
            | Layer::Embers
            | Layer::Mote => Which::Glow,
            Layer::Dust
            | Layer::Grit
            | Layer::Smoke
            | Layer::GunSmoke
            | Layer::Plume
            | Layer::Hurt
            | Layer::BloodDrops => Which::Soft,
            Layer::Cloud
            | Layer::DustCloud
            | Layer::Flame
            | Layer::Plasma
            | Layer::MuzzleFlash
            | Layer::Bloom
            | Layer::Wisp
            | Layer::Spray => Which::Cloud,
            Layer::Boom | Layer::Fireballs => Which::Boom,
        }
    }

    /// A baked layer's strip on its sheet.
    pub fn strip(self) -> Option<Strip> {
        Some(match self {
            Layer::Cloud => sheets::SMOKE,
            Layer::DustCloud => sheets::DUST,
            Layer::Flame => sheets::FIRE,
            Layer::Plasma => sheets::PLASMA,
            Layer::MuzzleFlash => sheets::FLASH,
            Layer::Bloom => sheets::BLOOM,
            Layer::Wisp => sheets::WISP,
            Layer::Spray => sheets::SMOKE,
            Layer::Boom | Layer::Fireballs => sheets::EXPLOSION,
            _ => return None,
        })
    }
}

/// What one blow throws: up to five layers with a count each, and how
/// many lit chips of the struck matter fly with them (`chips.rs`).
#[derive(Clone, Copy, Debug)]
pub struct EffectDef {
    pub layers: [(Layer, u8); 5],
    pub chips: u8,
}

impl EffectDef {
    fn of(layers: &[(Layer, u8)], chips: u8) -> Self {
        let mut out = [(Layer::Dust, 0u8); 5];
        out[..layers.len()].copy_from_slice(layers);
        Self { layers: out, chips }
    }

    /// How many of `layer` this effect throws.
    pub fn count(&self, layer: Layer) -> u8 {
        self.layers.iter().filter(|(l, _)| *l == layer).map(|(_, n)| *n).sum()
    }
}

/// The effect of a blow. Metal showers sparks; concrete flashes a few in a
/// grey cloud and throws chips; the ground is dust and grit; a shield
/// crackles; health mists and drips; a sword's cut on metal is a shower.
pub fn effect(weapon: Weapon, matter: Matter) -> EffectDef {
    use Layer::*;
    use Matter::*;
    let of = EffectDef::of;
    match (weapon, matter) {
        (Weapon::Blast, _) => of(&[(Flash, 2), (Fireball, 14), (Smoke, 9), (Dust, 8), (Sparks, 28)], 16),
        (Weapon::Sniper, Shield) => of(&[(Zap, 1), (Arcs, 6), (ShieldSparks, 16)], 0),
        (_, Shield) => of(&[(Zap, 1), (Arcs, 3), (ShieldSparks, 7)], 0),
        (Weapon::Pellet, Flesh) => of(&[(Hurt, 1), (BloodDrops, 3)], 0),
        (Weapon::Sniper, Flesh) => of(&[(Hurt, 4), (BloodDrops, 12)], 0),
        (_, Flesh) => of(&[(Hurt, 3), (BloodDrops, 8)], 0),
        (Weapon::Melee, Metal | Hull) => of(&[(Sparks, 14), (Flash, 1)], 3),
        (Weapon::Melee, Concrete) => of(&[(Sparks, 4), (Dust, 2), (Grit, 4)], 8),
        (Weapon::Melee, Dirt) => of(&[(Dust, 3), (Grit, 5)], 3),
        (Weapon::Blade, Metal | Hull) => of(&[(Sparks, 18), (Flash, 1), (Dust, 1)], 2),
        (Weapon::Blade, Concrete) => of(&[(Sparks, 6), (Flash, 1), (Dust, 3), (Grit, 4)], 6),
        (Weapon::Blade, Dirt) => of(&[(Dust, 4), (Grit, 6)], 3),
        (Weapon::Bullet, Metal | Hull) => of(&[(Sparks, 12), (Flash, 1), (Dust, 1)], 2),
        (Weapon::Bullet, Concrete) => of(&[(Sparks, 4), (Flash, 1), (Dust, 2), (Grit, 6)], 4),
        (Weapon::Bullet, Dirt) => of(&[(Dust, 1), (Grit, 8), (DustCloud, 2)], 0),
        (Weapon::Pellet, Metal | Hull) => of(&[(Sparks, 4), (Flash, 1)], 1),
        (Weapon::Pellet, Concrete) => of(&[(Sparks, 1), (Dust, 1), (Grit, 2)], 2),
        (Weapon::Pellet, Dirt) => of(&[(Dust, 1), (Grit, 3)], 0),
        (Weapon::Sniper, Metal | Hull) => of(&[(Sparks, 24), (Flash, 2), (Dust, 2)], 4),
        (Weapon::Sniper, Concrete) => of(&[(Sparks, 9), (Flash, 1), (Dust, 4), (Grit, 12), (DustCloud, 2)], 8),
        (Weapon::Sniper, Dirt) => of(&[(Dust, 3), (Grit, 14), (DustCloud, 3)], 2),
        // Energy: a crack of light, a few sparks, a breath of dust, and
        // the burst in the weapon's own tone (`Fx::impact`).
        (Weapon::Carbine, Metal | Hull | Concrete) => of(&[(Zap, 1), (Sparks, 5), (Dust, 1)], 1),
        (Weapon::Carbine, Dirt) => of(&[(Zap, 1), (Dust, 2), (Grit, 4)], 0),
        (Weapon::Beam, Metal | Hull | Concrete) => of(&[(Zap, 2), (Sparks, 10), (Dust, 2)], 2),
        (Weapon::Beam, Dirt) => of(&[(Zap, 2), (Dust, 4), (Grit, 8), (DustCloud, 1)], 0),
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

/// The layer's recipe as built, before any knob (`fx::recipe::Recipe`).
pub fn built(layer: Layer) -> Recipe {
    let base = Recipe {
        orient: Orient::Billboard,
        cell: PUFF,
        cells: 1,
        frames: 0,
        tone: TONE_FIRE,
        glow: 1.0,
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
        Layer::Sparks => Recipe {
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
        Layer::Flash => Recipe {
            cell: STAR,
            cells: 3,
            life: (0.05, 0.08),
            size: (0.06, 0.1),
            grow: 1.4,
            c0: [7.0, 5.5, 3.0, 1.0],
            c1: [1.5, 0.8, 0.3, 0.0],
            ..base
        },
        Layer::Dust => Recipe {
            cells: 3,
            speed: (0.4, 1.6),
            spray: 0.55,
            up: 0.2,
            gravity: -0.25,
            drag: 2.8,
            life: (0.8, 1.4),
            size: (0.05, 0.08),
            grow: 3.2,
            c0: [0.95, 0.95, 0.95, 0.3],
            c1: [1.05, 1.05, 1.05, 0.0],
            tinted: true,
            spin: 0.8,
            ..base
        },
        Layer::Grit => Recipe {
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
        Layer::Smoke => Recipe {
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
        Layer::GunSmoke => Recipe {
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
        Layer::Plume => Recipe {
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
        Layer::Fireball => Recipe {
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
        Layer::Arcs => Recipe {
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
        Layer::Zap => Recipe {
            cell: GLOW,
            life: (0.06, 0.09),
            size: (0.18, 0.24),
            grow: 1.8,
            c0: [6.0, 9.0, 16.0, 1.0],
            c1: [0.8, 1.4, 3.0, 0.0],
            ..base
        },
        Layer::ShieldSparks => Recipe {
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
        Layer::Hurt => Recipe {
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
        // Gates' blood drops, as specks.
        Layer::BloodDrops => Recipe {
            cell: SPECK,
            speed: (1.5, 3.5),
            spray: 0.5,
            up: 0.8,
            gravity: 9.81,
            drag: 0.5,
            life: (0.35, 0.7),
            size: (0.010, 0.014),
            grow: 0.8,
            c0: [0.16, 0.002, 0.002, 1.0],
            c1: [0.16, 0.002, 0.002, 0.6],
            bounce: true,
            ..base
        },
        Layer::Shock => Recipe {
            orient: Orient::Flat,
            cell: RING,
            life: (0.28, 0.32),
            size: (0.4, 0.5),
            grow: 13.0,
            c0: [3.0, 3.0, 3.0, 0.6],
            c1: [0.6, 0.6, 0.6, 0.0],
            tinted: true,
            ..base
        },
        Layer::Tongue => Recipe {
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
        // The baked sheets: each plays its strip once over its life. The
        // sheet does the rolling and the thinning, so a puff hardly grows.
        Layer::Cloud => Recipe {
            speed: (0.3, 1.2),
            spray: 0.6,
            up: 0.3,
            gravity: -0.5,
            drag: 1.6,
            life: (1.8, 3.0),
            size: (0.45, 0.7),
            grow: 1.6,
            c0: [0.55, 0.55, 0.55, 1.0],
            c1: [0.65, 0.65, 0.65, 0.0],
            tinted: true,
            spin: 0.25,
            ..strip(base, sheets::SMOKE)
        },
        Layer::DustCloud => Recipe {
            speed: (0.3, 1.0),
            spray: 0.4,
            up: 0.15,
            gravity: 0.2,
            drag: 2.4,
            life: (1.0, 1.8),
            size: (0.12, 0.18),
            grow: 1.8,
            c0: [0.95, 0.95, 0.95, 0.85],
            c1: [1.0, 1.0, 1.0, 0.0],
            tinted: true,
            spin: 0.1,
            ..strip(base, sheets::DUST)
        },
        Layer::Flame => Recipe {
            glow: 1.1,
            speed: (0.0, 0.4),
            spray: 0.5,
            gravity: -1.5,
            drag: 2.0,
            life: (0.45, 0.7),
            size: (0.3, 0.45),
            grow: 1.3,
            c0: [0.2, 0.2, 0.2, 0.8],
            c1: [0.3, 0.3, 0.3, 0.0],
            ..strip(base, sheets::FIRE)
        },
        Layer::Plasma => Recipe {
            tone: sheets::TONE_PLASMA,
            glow: 2.2,
            life: (0.3, 0.45),
            size: (0.5, 0.7),
            grow: 1.8,
            c0: [1.0, 1.0, 1.0, 1.0],
            c1: [1.0, 1.0, 1.0, 0.2],
            spin: 1.0,
            ..strip(base, sheets::PLASMA)
        },
        Layer::MuzzleFlash => Recipe {
            glow: 3.0,
            life: (0.05, 0.07),
            size: (0.16, 0.22),
            grow: 1.3,
            c0: [1.0, 1.0, 1.0, 1.0],
            c1: [1.0, 1.0, 1.0, 1.0],
            ..strip(base, sheets::FLASH)
        },
        Layer::Bloom => Recipe {
            tone: sheets::TONE_PLASMA,
            glow: 0.7,
            life: (0.06, 0.09),
            size: (0.09, 0.13),
            grow: 1.5,
            c0: [1.0, 1.0, 1.0, 1.0],
            c1: [1.0, 1.0, 1.0, 1.0],
            ..strip(base, sheets::BLOOM)
        },
        Layer::Wisp => Recipe {
            speed: (0.5, 1.4),
            spray: 0.8,
            gravity: -0.4,
            drag: 2.5,
            life: (0.6, 0.9),
            size: (0.07, 0.11),
            grow: 2.0,
            c0: [0.6, 0.6, 0.6, 0.32],
            c1: [0.65, 0.65, 0.65, 0.0],
            spin: 0.5,
            ..strip(base, sheets::WISP)
        },
        // The soot's albedo is a dark grey (0.1 linear): lit by the sun it
        // reads as smoke, not cloud; the fire's glow is kept under what
        // clips without bloom, so its body stays orange.
        Layer::Boom => Recipe {
            glow: 1.4,
            speed: (0.0, 0.5),
            spray: 0.4,
            gravity: -0.6,
            drag: 1.5,
            life: (1.3, 1.7),
            size: (1.8, 2.4),
            grow: 1.5,
            c0: [0.1, 0.09, 0.085, 1.0],
            c1: [0.18, 0.18, 0.18, 0.0],
            spin: 0.15,
            ..strip(base, sheets::EXPLOSION)
        },
        // Flung off the main one sideways, faster, not up into it.
        Layer::Fireballs => Recipe {
            glow: 1.2,
            speed: (3.5, 7.0),
            spray: 0.15,
            up: 0.3,
            gravity: -0.4,
            drag: 2.6,
            life: (0.9, 1.3),
            size: (0.8, 1.2),
            grow: 1.6,
            c0: [0.1, 0.09, 0.085, 1.0],
            c1: [0.18, 0.18, 0.18, 0.0],
            spin: 0.3,
            ..strip(base, sheets::EXPLOSION)
        },
        // Motes big enough to see drift and fall, in the blast's colour.
        Layer::Embers => Recipe {
            cell: GLOW,
            speed: (2.0, 7.0),
            spray: 0.3,
            up: 2.0,
            gravity: 3.0,
            drag: 0.6,
            life: (0.8, 1.8),
            size: (0.03, 0.045),
            grow: 0.6,
            c0: [8.0, 8.0, 8.0, 1.0],
            c1: [1.6, 1.6, 1.6, 0.0],
            tinted: true,
            ..base
        },
        // Thrown up a couple of metres and let fall, swelling as it thins:
        // a wave breaking on a rock, seen from the beach.
        Layer::Spray => Recipe {
            speed: (2.0, 4.5),
            spray: 0.75,
            up: 1.0,
            gravity: 5.0,
            drag: 1.4,
            life: (0.9, 1.5),
            size: (0.7, 1.2),
            grow: 2.2,
            c0: [1.0, 1.0, 1.0, 0.8],
            c1: [1.0, 1.0, 1.0, 0.0],
            tinted: true,
            spin: 0.2,
            ..strip(base, sheets::SMOKE)
        },
        // A few millimetres of light riding the wind and sinking a little:
        // it is there, then fades. Faint: a field of them is air, not snow.
        Layer::Mote => Recipe {
            cell: GLOW,
            speed: (0.15, 0.6),
            spray: 0.8,
            gravity: 0.03,
            drag: 0.1,
            life: (3.5, 6.5),
            size: (0.012, 0.022),
            c0: [1.5, 1.35, 0.95, 0.9],
            c1: [1.5, 1.35, 0.95, 0.0],
            tinted: true,
            ..base
        },
    }
}

/// `base` on a baked strip: its first cell, its variants, its frames.
fn strip(base: Recipe, s: Strip) -> Recipe {
    Recipe {
        cell: s.cell,
        cells: s.variants,
        frames: s.frames,
        ..base
    }
}

/// Throw `n` particles of `layer` from `at`, around `dir`, coloured for
/// `matter`. Sparks and grit bounce once off the plane through `at` facing
/// `dir`: the surface they came off.
pub fn emit(pool: &mut Pool, layer: Layer, n: usize, at: Vec3, dir: Vec3, matter: Matter) {
    emit_tint(pool, layer, n, at, dir, dust_tint(matter), 1.0);
}

/// Throw `n` of a recipe of the caller's own (a layer's, retuned: another
/// tone, another size) from `at` around `dir`.
pub fn emit_recipe(pool: &mut Pool, r: &Recipe, n: usize, at: Vec3, dir: Vec3, tint: Color, scale: f32) {
    let t = tint.to_linear();
    ::fx::recipe::emit(pool, r, n, at.into(), dir.into(), [t.red, t.green, t.blue], scale);
}

/// [`emit`] in a colour of the caller's, at `scale` times the layer's size.
/// A layer that is not tinted ignores the colour.
pub fn emit_tint(pool: &mut Pool, layer: Layer, n: usize, at: Vec3, dir: Vec3, tint: Color, scale: f32) {
    ::fx::recipe::emit(
        pool,
        &recipe(layer),
        n,
        at.into(),
        dir.into(),
        tint.to_linear().to_f32_array_no_alpha(),
        scale,
    );
}

/// Recipes the fx stage retuned (`?knobs=`), set once at its start.
static KNOBS: OnceLock<Vec<(Layer, Recipe)>> = OnceLock::new();

/// The recipe a layer throws now: as built, or as the stage's knobs retuned it.
pub fn recipe(layer: Layer) -> Recipe {
    KNOBS
        .get()
        .and_then(|k| k.iter().find(|(l, _)| *l == layer).map(|(_, r)| *r))
        .unwrap_or_else(|| built(layer))
}

/// Retune layers from knobs (`Sparks.speed=4,12;Flash.life=0.04,0.07`) for
/// the rest of the run. Once per run: the fx stage calls it at start-up.
pub fn set_knobs(text: &str) -> Result<(), String> {
    let mut out: Vec<(Layer, Recipe)> = Vec::new();
    for k in ::fx::recipe::knobs(text) {
        let k = k?;
        let layer = Layer::named(k.layer).ok_or_else(|| {
            let names: Vec<_> = Layer::ALL.iter().map(|l| l.name()).collect();
            format!("no layer `{}` (layers: {})", k.layer, names.join(", "))
        })?;
        let i = match out.iter().position(|(l, _)| *l == layer) {
            Some(i) => i,
            None => {
                out.push((layer, built(layer)));
                out.len() - 1
            }
        };
        out[i].1.set(k.key, k.val)?;
    }
    KNOBS.set(out).map_err(|_| "knobs are set once a run".to_string())
}

/// What a gun's muzzle puts out: burning powder (a baked flash of petals,
/// a tongue of fire, a breath of smoke) or energy (a baked bloom in a
/// tone, no smoke).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Flash {
    Powder,
    Energy(f32),
}

/// What a gun throws out of its port each round.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Casing {
    None,
    Brass,
    Shell,
}

/// A gun's look when it fires.
#[derive(Clone, Copy, Debug)]
pub struct GunLook {
    pub flash: Flash,
    pub casing: Casing,
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
    flash: Flash::Powder,
    casing: Casing::Brass,
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
            casing: Casing::Shell,
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
            flash: Flash::Energy(sheets::TONE_GREEN),
            casing: Casing::None,
            c0: [2.0, 9.0, 3.0, 0.9],
            c1: [0.6, 3.5, 1.0, 0.5],
            flash_c: [3.0, 9.0, 3.5, 1.0],
            mps: 340.0,
            ..RIFLE
        },
        // A beam: pink-violet, all of its line in a frame.
        BEAM_RIFLE => GunLook {
            flash: Flash::Energy(sheets::TONE_PINK),
            casing: Casing::None,
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
            flash: Flash::Energy(sheets::TONE_PLASMA),
            casing: Casing::None,
            flash_c: [2.0, 5.0, 14.0, 1.0],
            flash_scale: 0.8,
            light_lm: 90_000.0,
            ..RIFLE
        },
        SHARDS => GunLook {
            flash: Flash::Energy(sheets::TONE_PINK),
            casing: Casing::None,
            flash_c: PINK,
            flash_scale: 0.8,
            light_lm: 90_000.0,
            ..RIFLE
        },
        CANNON => GunLook {
            flash: Flash::Energy(sheets::TONE_GREEN),
            casing: Casing::None,
            flash_c: [3.0, 12.0, 4.0, 1.0],
            stars: 3,
            flash_scale: 2.2,
            light_lm: 300_000.0,
            ..RIFLE
        },
        LAUNCHER => GunLook {
            casing: Casing::None,
            stars: 3,
            flash_scale: 1.8,
            light_lm: 200_000.0,
            ..RIFLE
        },
        ROCKET => GunLook {
            casing: Casing::None,
            ..RIFLE
        },
        _ => RIFLE,
    }
}

/// The SHARD GUN's crystals: a hot pink, linear HDR.
pub const PINK: [f32; 4] = [9.0, 2.2, 6.0, 1.0];

/// A muzzle's flash: `n` stars in colour `c`, `scale` times an impact's.
pub fn stars(pool: &mut Pool, at: Vec3, n: usize, scale: f32, c: [f32; 4]) {
    stars_for(pool, at, n, scale, c, STAR_LIFE_S);
}

/// How long a muzzle's star burns, seconds: shortest and longest.
const STAR_LIFE_S: (f32, f32) = (0.05, 0.08);
/// Our own gun's: a frame or two. A star is a mark in the world, and the
/// gun in view turns with the camera, so a longer one is left behind as
/// the view swings or kicks.
pub const OWN_STAR_LIFE_S: (f32, f32) = (0.025, 0.04);

/// Our own gun's flash, at the muzzle in view: [`stars`] that burn for
/// [`OWN_STAR_LIFE_S`].
pub fn own_stars(pool: &mut Pool, at: Vec3, n: usize, scale: f32, c: [f32; 4]) {
    stars_for(pool, at, n, scale, c, OWN_STAR_LIFE_S);
}

fn stars_for(pool: &mut Pool, at: Vec3, n: usize, scale: f32, c: [f32; 4], (short, long): (f32, f32)) {
    for _ in 0..n {
        let life = short + (long - short) * pool.roll();
        let size = (0.06 + 0.04 * pool.roll()) * scale;
        let cell = STAR + ((pool.roll() * 3.0) as u8).min(2);
        let roll = pool.roll() * core::f32::consts::TAU;
        pool.spawn(Particle {
            left: life,
            life,
            pos: at.into(),
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
            pos: at.into(),
            vel: v.into(),
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
        pos: at.into(),
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
        pos: start.into(),
        vel: (dir * look.mps).into(),
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
            pos: at.into(),
            vel: vel.into(),
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

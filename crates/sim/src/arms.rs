//! The guns. Ported from Fireplace's `ARMS` (`src/layout.mjs`, measured at
//! 125 Hz there), restated in ms and radians. Halo's set: an equal start of
//! a rifle and a sidearm, power weapons on pads.
//!
//! Tuned to Halo 2's feel: its counts (shots to kill, rounds to kill) and
//! times carried onto our 100 shield and 100 health (Halo 2's are 70 and
//! 45). `tests/range.rs` measures every gun in a match and pins it; its
//! ignored `the_range_table` prints the lot.

use crate::geom::V3;

/// No gun in this hand.
pub const NONE: u8 = 0;
pub const AR: u8 = 1;
pub const BR: u8 = 2;
pub const MAGNUM: u8 = 3;
pub const SNIPER: u8 = 4;
pub const SHOTGUN: u8 = 5;
pub const ROCKET: u8 = 6;
pub const SMG: u8 = 7;
/// The sword: a kill in one, in reach or at the end of a lunge.
pub const SWORD: u8 = 8;
/// A semi-automatic rifle between the BR and the sniper: a 7-shot.
pub const CARBINE: u8 = 9;
/// A sniper that runs hot instead of empty: two quick shots, then a vent.
pub const BEAM_RIFLE: u8 = 10;
/// An automatic of ion bolts: strips shields fast, slow on health; heats.
pub const ION_RIFLE: u8 = 11;
/// Ion bolts on a tap; held, a charged bolt that homes and takes every
/// shield it touches (then the pistol vents).
pub const ION_PISTOL: u8 = 12;
/// Homing shards that stick; enough of them stuck go off together, a kill.
pub const SHARDS: u8 = 13;
/// Grenades that skip off the floor and go off on a body or just after.
pub const LAUNCHER: u8 = 14;
/// A heavy ion ball: slow, a long splash, a kill on a direct hit.
pub const CANNON: u8 = 15;
/// Arm ids go on the wire in 4 bits.
pub const COUNT: usize = 16;

/// What a pad may hold past the guns: power-ups, taken by walking onto the
/// pad (no USE), never carried as a gun or dropped (`World::powers`).
/// Overshield: the shield climbs to three times full, then drains back.
pub const OVERSHIELD: u8 = 16;
/// Active camouflage: a while nearly unseen, shown by moving fast, firing,
/// a throw, a blow or a hit (`Fighter::cloak`).
pub const CAMO: u8 = 17;
/// Grenades lying on a pad: walking on takes `GREN_PAD_GIVES`, if any fit.
pub const FRAGS: u8 = 18;
pub const STICKIES: u8 = 19;

/// Whether a pad's `id` is taken by walking on rather than a gun.
pub const fn power(id: u8) -> bool {
    id == OVERSHIELD || id == CAMO || id == FRAGS || id == STICKIES
}

/// Gun Game's ladder: each kill moves the killer up one, the power weapons
/// first, and a kill with the last one wins.
pub const LADDER: [u8; 7] = [ROCKET, SHOTGUN, SMG, AR, SNIPER, BR, MAGNUM];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Hitscan,
    /// A projectile that blows up where it lands: a blast of `dmg` to
    /// `radius`, whole to `full`.
    Rocket,
    /// The same, drawn as an ion ball.
    Cannon,
    /// A projectile that hurts what it hits, `dmg` (`shield_k` on shields);
    /// a charge weapon's charged bolt homes and strips shields instead.
    Bolt,
    /// A homing projectile that sticks: `stick` of them on one fighter go
    /// off together and kill.
    Needle,
    /// A grenade under `gravity` that goes off on a body, or `fuse_ms`
    /// after it first touches anything.
    Lob,
    /// No round: a swing, or a lunge at whoever the aim is on (`World`).
    Sword,
}

impl Kind {
    /// Whether a round is a ray (drawn as a tracer), not something flying.
    pub fn ray(self) -> bool {
        self == Kind::Hitscan
    }
}

/// Heat is kept in 1/128 % so a cooling rate in % a second is whole a tick.
pub const HEAT_FULL: u16 = 100 * 128;

/// What a head hit does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Head {
    /// Just damage.
    None,
    /// Kills once the shields are down.
    Unshielded,
    /// Always kills.
    Always,
}

#[derive(Clone, Copy, Debug)]
pub struct Arm {
    pub name: &'static str,
    pub kind: Kind,
    /// Fires while the trigger is held.
    pub auto: bool,
    /// Rounds a trigger pull fires (1 for most).
    pub burst: u8,
    /// Between rounds of a burst.
    pub burst_gap_ms: u32,
    /// Between trigger pulls (or rounds, for an automatic).
    pub refire_ms: u32,
    pub mag: u16,
    pub reserve: u16,
    /// Points a round (a pellet, for the shotgun).
    pub dmg: i32,
    pub head: Head,
    /// Cone half-angle in radians.
    pub spread: f32,
    pub pellets: u8,
    /// Hitscan range in m.
    pub range: f32,
    /// Full damage to here, then linear down to `falloff_min` of it at range.
    pub falloff: f32,
    pub falloff_min: f32,
    pub reload_ms: u32,
    /// Scope magnification, for the client (1: no scope), and a second
    /// level a further click steps to (0: none). Halo 2's: 2x on the
    /// rifles and launchers, 5x then 10x on the snipers.
    pub zoom: u8,
    pub zoom2: u8,
    /// Rocket speed off the tube in m/s, rising (evenly over distance) to
    /// `speed_max` once it has flown `ramp` m.
    pub speed: f32,
    pub speed_max: f32,
    pub ramp: f32,
    /// Blast radius, and the radius of full damage in it.
    pub radius: f32,
    pub full: f32,
    /// A projectile's share of gravity.
    pub gravity: f32,
    /// How fast a homing round turns at its target, radians a second.
    pub homing: f32,
    /// Damage to shields is this times `dmg` (ion runs over 1, so it
    /// strips shields and is weak on health).
    pub shield_k: f32,
    /// Heat, for a gun that runs hot (0: it doesn't): % a shot, % a second
    /// it cools, % a second it vents once past 100 (when it may not fire).
    /// Such a gun's `mag` is a battery: no reserve, no reload.
    pub heat_shot: u16,
    pub heat_cool: u16,
    pub heat_vent: u16,
    /// A charge weapon: held this long, the release fires a charged round,
    /// at `charge_speed`, spending `charge_cost` of the battery and
    /// `charge_heat` %.
    pub charge_ms: u32,
    pub charge_speed: f32,
    pub charge_cost: u16,
    pub charge_heat: u16,
    /// Needles stuck that go off.
    pub stick: u8,
    /// A lob's fuse, from when it first touches something.
    pub fuse_ms: u32,
    /// One-handed: it may be dual wielded, beside another that is, with
    /// its cone grown to `dual_spread` (Halo 2's: the same shots to kill,
    /// less accurate).
    pub dual: bool,
    pub dual_spread: f32,
    /// How much a bot wants it over another it holds (`Bot::think`): power
    /// weapons high.
    pub rank: u8,
    /// Bullet magnetism (a hitscan gun's): a round that misses everyone
    /// but passes through an enemy's body widened by this angle (radians)
    /// at its distance ([`Arm::magnet_pad`]), within `aim_range`, is bent
    /// onto the body (`World::magnet`). Never onto the head. Nothing while
    /// [`crate::consts::MAGNETISM`] is off.
    pub magnet: f32,
    /// The red reticle's reach (the client's, over an enemy), and
    /// magnetism's.
    pub aim_range: f32,
    /// A turret's barrels spin up this long, the trigger held, before the
    /// first round (0: none).
    pub spin_ms: u32,
    /// A hot gun's cone at full heat (0: `spread`, hot or cold): it opens
    /// from `spread` with the heat, so a burst is tighter than a hose.
    pub bloom: f32,
    /// Damage to a vehicle's hull is this times `dmg`.
    pub hull_k: f32,
}

impl Arm {
    /// How much magnetism widens a body `dist` away: its angle at that
    /// distance, nothing with [`crate::consts::MAGNETISM`] off.
    pub fn magnet_pad(&self, dist: f32) -> f32 {
        if crate::consts::MAGNETISM {
            dist * self.magnet
        } else {
            0.0
        }
    }

    /// Where a ray from `eye` along `dir` meets a body standing on `base`,
    /// `top` tall, widened by [`Arm::magnet_pad`], within `reach`: the zone
    /// a near miss is bent from (`World::magnet`) and the red reticle's.
    pub fn magnet_zone(&self, eye: V3, dir: V3, base: V3, top: f32, reach: f32) -> Option<f32> {
        let pad = self.magnet_pad((base - eye).len());
        crate::geom::ray_cylinder(eye, dir, base, crate::consts::HIT_R + pad, top, reach)
    }

    /// The magnification at scope level `level` (0 unscoped, 1, 2), 1 for
    /// none.
    pub fn zoom_at(&self, level: u8) -> u8 {
        match level {
            0 => 1,
            1 => self.zoom.max(1),
            _ if self.zoom2 > 1 => self.zoom2,
            _ => self.zoom.max(1),
        }
    }

    /// Scope levels: 0, 1 or 2.
    pub fn zoom_levels(&self) -> u8 {
        (self.zoom > 1) as u8 + (self.zoom > 1 && self.zoom2 > 1) as u8
    }

    /// Its cone at `heat` (1/128 %): `spread`, opening to `bloom` at full.
    pub fn cone(&self, heat: u16) -> f32 {
        if self.bloom <= 0.0 {
            return self.spread;
        }
        let k = heat.min(crate::arms::HEAT_FULL) as f32 / crate::arms::HEAT_FULL as f32;
        self.spread + (self.bloom - self.spread) * k
    }

    pub fn hot(&self) -> bool {
        self.heat_shot > 0
    }

    /// Ticks a vent takes from full.
    pub fn vent_ticks(&self) -> u32 {
        crate::consts::ticks(100_000 / self.heat_vent.max(1) as u32)
    }

    /// Ticks the fighter's `reload` timer starts at: a reload, or a hot
    /// gun's vent.
    pub fn reload_ticks(&self) -> u32 {
        if self.hot() {
            self.vent_ticks()
        } else {
            crate::consts::ticks(self.reload_ms)
        }
    }
}

/// 1/16384 turn (Fireplace's spread unit) in radians.
const fn q14(n: f32) -> f32 {
    n * (core::f32::consts::TAU / 16384.0)
}

/// Degrees in radians.
const fn deg(n: f32) -> f32 {
    n * (core::f32::consts::PI / 180.0)
}

const BASE: Arm = Arm {
    name: "",
    kind: Kind::Hitscan,
    auto: false,
    burst: 1,
    burst_gap_ms: 0,
    refire_ms: 100,
    mag: 0,
    reserve: 0,
    dmg: 0,
    head: Head::None,
    spread: 0.0,
    pellets: 1,
    range: 100.0,
    falloff: 1000.0,
    falloff_min: 1.0,
    reload_ms: 2000,
    zoom: 1,
    zoom2: 0,
    speed: 0.0,
    speed_max: 0.0,
    ramp: 1.0,
    radius: 0.0,
    full: 0.0,
    gravity: 0.0,
    homing: 0.0,
    shield_k: 1.0,
    heat_shot: 0,
    heat_cool: 0,
    heat_vent: 0,
    charge_ms: 0,
    charge_speed: 0.0,
    charge_cost: 0,
    charge_heat: 0,
    stick: 0,
    fuse_ms: 0,
    dual: false,
    dual_spread: 0.0,
    rank: 0,
    magnet: 0.0,
    aim_range: 0.0,
    spin_ms: 0,
    bloom: 0.0,
    hull_k: 1.0,
};

pub static ARMS: [Arm; COUNT] = [
    BASE,
    Arm {
        name: "ASSAULT RIFLE",
        magnet: deg(0.3),
        aim_range: 25.0,
        rank: 2,
        auto: true,
        refire_ms: 88,
        mag: 32,
        reserve: 192,
        dmg: 8,
        spread: q14(40.0),
        range: 90.0,
        reload_ms: 1760,
        ..BASE
    },
    Arm {
        name: "BATTLE RIFLE",
        magnet: deg(0.4),
        aim_range: 45.0,
        rank: 3,
        burst: 3,
        // Halo 2's: 15 rounds a second in a burst, 0.266 s between bursts.
        burst_gap_ms: 67,
        refire_ms: 266,
        mag: 36,
        reserve: 108,
        // Ten rounds strip the shields, so the eleventh in the head kills:
        // four bursts (Halo 2's 4-shot), seven to the body alone.
        dmg: 10,
        head: Head::Unshielded,
        spread: q14(6.0),
        range: 160.0,
        reload_ms: 1760,
        zoom: 2,
        ..BASE
    },
    Arm {
        name: "MAGNUM",
        magnet: deg(0.35),
        aim_range: 30.0,
        rank: 1,
        refire_ms: 176,
        // Halo 2's: 12 a magazine, 48 more, and no scope.
        mag: 12,
        reserve: 48,
        dmg: 28,
        head: Head::Unshielded,
        spread: q14(4.0),
        range: 110.0,
        reload_ms: 1280,
        dual: true,
        dual_spread: q14(14.0),
        ..BASE
    },
    Arm {
        name: "SNIPER RIFLE",
        aim_range: 100.0,
        rank: 6,
        refire_ms: 500,
        mag: 4,
        reserve: 20,
        dmg: 120,
        head: Head::Always,
        range: 420.0,
        reload_ms: 1920,
        zoom: 5,
        zoom2: 10,
        ..BASE
    },
    Arm {
        name: "SHOTGUN",
        aim_range: 10.0,
        rank: 5,
        // Halo 2's: a second a shell, 12 in the tube and 36 more.
        refire_ms: 1000,
        mag: 12,
        reserve: 36,
        // Halo 2's: ten pellets, seven of them a kill.
        dmg: 29,
        spread: q14(260.0),
        pellets: 10,
        range: 28.0,
        falloff: 8.0,
        falloff_min: 0.2,
        reload_ms: 2000,
        ..BASE
    },
    Arm {
        name: "ROCKET LAUNCHER",
        aim_range: 40.0,
        rank: 7,
        kind: Kind::Rocket,
        refire_ms: 880,
        mag: 2,
        reserve: 6,
        dmg: 230,
        reload_ms: 2000,
        zoom: 2,
        // Halo 2's: 8 wu/s off the tube to 20 wu/s by 20 wu (a wu is 3.048
        // m), so it is slow up close and quick across a map.
        speed: 24.4,
        speed_max: 61.0,
        ramp: 61.0,
        // A kill to 3.4 m from where it lands, as Halo 2's.
        radius: 6.0,
        full: 3.0,
        ..BASE
    },
    Arm {
        name: "SMG",
        magnet: deg(0.3),
        aim_range: 18.0,
        rank: 2,
        auto: true,
        // Halo 2's: 15 rounds a second, 25 to kill.
        refire_ms: 67,
        // Halo 2's: 60 a magazine, 180 more.
        mag: 60,
        reserve: 180,
        dmg: 8,
        spread: q14(80.0),
        range: 50.0,
        falloff: 20.0,
        falloff_min: 0.6,
        reload_ms: 1600,
        // Halo 2's: 1–2.75° alone, 1.5–3° dual.
        dual: true,
        dual_spread: q14(100.0),
        ..BASE
    },
    Arm {
        name: "SWORD",
        aim_range: 6.0,
        rank: 6,
        kind: Kind::Sword,
        refire_ms: 560,
        // A battery that never runs down, as Halo 2's in multiplayer: the
        // mag is shown and never spent.
        mag: 100,
        // Everything a full fighter has.
        dmg: 200,
        // How far it lunges from (the reach a bot fights at).
        range: crate::consts::LUNGE_RANGE,
        ..BASE
    },
    // The rest are Halo 2's archetypes (the Covenant's guns), its numbers
    // carried onto ours the way the rest are: counts and times kept.
    Arm {
        name: "CARBINE",
        magnet: deg(0.4),
        aim_range: 40.0,
        rank: 3,
        // Halo 2's: 0.14 s a shot, rounded to 6 a second by its tick.
        refire_ms: 167,
        mag: 18,
        reserve: 72,
        // Six strip the shields, the seventh in the head kills: Halo 2's
        // 7-shot. Twelve to the body.
        dmg: 17,
        head: Head::Unshielded,
        spread: deg(0.3),
        range: 122.0,
        reload_ms: 1800,
        zoom: 2,
        ..BASE
    },
    Arm {
        name: "BEAM RIFLE",
        aim_range: 100.0,
        rank: 6,
        // Halo 2's: 4 a second, but 65% heat a shot and 57.5% a second off:
        // two quick shots, then a vent of three seconds.
        refire_ms: 250,
        mag: 18,
        dmg: 120,
        head: Head::Always,
        range: 366.0,
        zoom: 5,
        zoom2: 10,
        heat_shot: 65,
        heat_cool: 58,
        heat_vent: 30,
        ..BASE
    },
    Arm {
        name: "ION RIFLE",
        aim_range: 25.0,
        rank: 2,
        kind: Kind::Bolt,
        auto: true,
        // Halo 2's plasma rifle: 7.5 bolts a second, 1.5× on shields and
        // 0.35× on health: 5 bolts strip the shields, 13 more kill.
        refire_ms: 133,
        mag: 400,
        dmg: 8,
        shield_k: 2.5,
        spread: deg(0.5),
        range: 100.0,
        speed: 64.0,
        heat_shot: 15,
        heat_cool: 85,
        heat_vent: 35,
        // Halo 2's: 0.25–0.75° alone, 1.5–3° dual.
        dual: true,
        dual_spread: deg(2.0),
        ..BASE
    },
    Arm {
        name: "ION PISTOL",
        aim_range: 25.0,
        rank: 2,
        kind: Kind::Bolt,
        // Halo 2's plasma pistol: as fast as a finger, 7 bolts strip the
        // shields, 20 more kill; 0.6 s held, a charged bolt that turns 80°
        // a second at whoever the aim was on and takes all their shields.
        refire_ms: 67,
        mag: 250,
        dmg: 5,
        shield_k: 3.0,
        range: 122.0,
        speed: 50.0,
        homing: deg(80.0),
        heat_shot: 16,
        heat_cool: 60,
        heat_vent: 45,
        charge_ms: 600,
        charge_speed: 23.0,
        charge_cost: 37,
        charge_heat: 100,
        dual: true,
        dual_spread: deg(0.5),
        ..BASE
    },
    Arm {
        name: "SHARD GUN",
        aim_range: 25.0,
        rank: 3,
        kind: Kind::Needle,
        auto: true,
        // Halo 2's needler: 7.5 a second, slow (6 wu/s), turning 15–30° a
        // second at the top of that, eight stuck go off, a kill.
        refire_ms: 133,
        mag: 30,
        reserve: 90,
        dmg: 4,
        shield_k: 1.5,
        spread: deg(1.0),
        range: 73.0,
        speed: 18.3,
        homing: deg(30.0),
        stick: 8,
        reload_ms: 2000,
        // Halo 2's: 0.5–1.5° alone, 1–3° dual.
        dual: true,
        dual_spread: deg(2.0),
        ..BASE
    },
    Arm {
        name: "GRENADE LAUNCHER",
        aim_range: 30.0,
        rank: 4,
        kind: Kind::Lob,
        // Halo 2's brute shot: 0.3 s a round, 4 to a drum, 61 m/s slowing
        // to 30 by 46 m, 30% gravity, off 0.2 s after it first lands; three
        // near hits kill.
        refire_ms: 300,
        mag: 4,
        reserve: 12,
        dmg: 75,
        spread: deg(0.6),
        reload_ms: 2200,
        speed: 61.0,
        speed_max: 30.0,
        ramp: 46.0,
        gravity: 0.3,
        radius: 3.5,
        full: 1.0,
        fuse_ms: 200,
        ..BASE
    },
    Arm {
        name: "ION CANNON",
        aim_range: 40.0,
        rank: 6,
        kind: Kind::Cannon,
        // Halo 2's fuel rod: 0.4 s a round, 5 a load, 46 m/s slowing to 20
        // by 15 m under a little gravity; a direct hit kills.
        refire_ms: 400,
        mag: 5,
        reserve: 25,
        dmg: 210,
        reload_ms: 2400,
        zoom: 2,
        speed: 46.0,
        speed_max: 20.0,
        ramp: 15.0,
        gravity: 0.025,
        radius: 4.5,
        full: 1.0,
        ..BASE
    },
];

/// The guns a vehicle's turret mounts (`vehicle::turret`): never held, on
/// a pad or dropped, so no arm id. Their rounds and kills carry a
/// [`crate::events::Cause`] instead.
///
/// The minigun on the truck's bed: Halo's chaingun. Its barrels spin up,
/// then it pours rounds out as long as the trigger is held and it is cool
/// enough; past full it vents. About two seconds on target kill.
pub static MINIGUN: Arm = Arm {
    name: "MINIGUN",
    magnet: deg(0.3),
    aim_range: 45.0,
    auto: true,
    refire_ms: 62,
    mag: 1,
    dmg: 7,
    // Tight cold, a hose hot: a second's burst still lands at 40 m, five
    // seconds held paints a wall.
    spread: q14(25.0),
    bloom: q14(120.0),
    range: 130.0,
    falloff: 45.0,
    falloff_min: 0.4,
    // 3% a round at 16 a second, 20% a second off: about three and a half
    // seconds held, then a vent of two and a half; let go, it is cold again
    // as fast as it heated.
    heat_shot: 3,
    heat_cool: 20,
    heat_vent: 40,
    spin_ms: 350,
    // Small rounds on armour: a truck takes a minigun's two vents and more,
    // not one.
    hull_k: 0.5,
    ..BASE
};

/// The skimmer's twin bolt guns, its pilot's (`vehicle::BOLTS`): Halo 2's
/// light craft's plasma, fired left and right by turns as long as the
/// trigger is held, cold while it boosts. About two seconds on target kill
/// a fighter; they barely mark armour.
pub static BOLTS: Arm = Arm {
    name: "BOLT GUNS",
    magnet: deg(0.4),
    aim_range: 40.0,
    auto: true,
    refire_ms: 95,
    mag: 1,
    dmg: 10,
    spread: q14(30.0),
    range: 110.0,
    falloff: 40.0,
    falloff_min: 0.5,
    hull_k: 0.6,
    ..BASE
};

/// The flier's wing guns, its pilot's (`vehicle::WING_BOLTS`): the
/// skimmer's, a little harder and a little slower.
pub static WING_BOLTS: Arm = Arm {
    name: "WING GUNS",
    magnet: deg(0.4),
    aim_range: 45.0,
    auto: true,
    refire_ms: 105,
    mag: 1,
    dmg: 12,
    spread: q14(35.0),
    range: 120.0,
    falloff: 45.0,
    falloff_min: 0.5,
    hull_k: 0.6,
    ..BASE
};

/// What fired a round, by the id its shot carries (`Event::c`): a gun's arm
/// id, or a turret's cause.
pub fn fired(c: u16) -> Option<&'static Arm> {
    match c {
        c if (1..COUNT as u16).contains(&c) => Some(&ARMS[c as usize]),
        c if c == crate::events::Cause::Minigun as u16 => Some(&MINIGUN),
        c if c == crate::events::Cause::Bolts as u16 => Some(&BOLTS),
        c if c == crate::events::Cause::WingBolts as u16 => Some(&WING_BOLTS),
        _ => None,
    }
}

pub fn arm(id: u8) -> &'static Arm {
    &ARMS[(id as usize).min(COUNT - 1)]
}

/// Swapping guns takes this long before either can fire.
pub const SWAP_MS: u32 = 320;

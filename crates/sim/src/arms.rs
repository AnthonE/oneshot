//! The guns. Ported from Fireplace's `ARMS` (`src/layout.mjs`, measured at
//! 125 Hz there), restated in ms and radians. Halo's set: an equal start of
//! a rifle and a sidearm, power weapons on pads.
//!
//! Tuned to Halo 2's feel: its counts (shots to kill, rounds to kill) and
//! times carried onto our 100 shield and 100 health (Halo 2's are 70 and
//! 45). `tests/range.rs` measures every gun in a match and pins it; its
//! ignored `the_range_table` prints the lot.

/// No gun in this hand.
pub const NONE: u8 = 0;
pub const AR: u8 = 1;
pub const BR: u8 = 2;
pub const MAGNUM: u8 = 3;
pub const SNIPER: u8 = 4;
pub const SHOTGUN: u8 = 5;
pub const ROCKET: u8 = 6;
pub const SMG: u8 = 7;
/// The energy sword: a kill in one, in reach or at the end of a lunge.
pub const SWORD: u8 = 8;
pub const COUNT: usize = 9;

/// Gun Game's ladder: each kill moves the killer up one, the power weapons
/// first, and a kill with the last one wins.
pub const LADDER: [u8; 7] = [ROCKET, SHOTGUN, SMG, AR, SNIPER, BR, MAGNUM];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Hitscan,
    Rocket,
    /// No round: a swing, or a lunge at whoever the aim is on (`World`).
    Sword,
}

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
    /// Scope magnification, for the client.
    pub zoom: u8,
    /// Rocket speed off the tube in m/s, rising (evenly over distance) to
    /// `speed_max` once it has flown `ramp` m.
    pub speed: f32,
    pub speed_max: f32,
    pub ramp: f32,
    /// Blast radius, and the radius of full damage in it.
    pub radius: f32,
    pub full: f32,
}

/// 1/16384 turn (Fireplace's spread unit) in radians.
const fn q14(n: f32) -> f32 {
    n * (core::f32::consts::TAU / 16384.0)
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
    speed: 0.0,
    speed_max: 0.0,
    ramp: 1.0,
    radius: 0.0,
    full: 0.0,
};

pub static ARMS: [Arm; COUNT] = [
    BASE,
    Arm {
        name: "ASSAULT RIFLE",
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
        refire_ms: 176,
        mag: 8,
        reserve: 40,
        dmg: 28,
        head: Head::Unshielded,
        spread: q14(4.0),
        range: 110.0,
        reload_ms: 1280,
        zoom: 2,
        ..BASE
    },
    Arm {
        name: "SNIPER RIFLE",
        refire_ms: 500,
        mag: 4,
        reserve: 20,
        dmg: 120,
        head: Head::Always,
        range: 420.0,
        reload_ms: 1920,
        zoom: 5,
        ..BASE
    },
    Arm {
        name: "SHOTGUN",
        refire_ms: 840,
        mag: 6,
        reserve: 18,
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
        auto: true,
        // Halo 2's: 15 rounds a second, 25 to kill.
        refire_ms: 67,
        mag: 48,
        reserve: 192,
        dmg: 8,
        spread: q14(80.0),
        range: 50.0,
        falloff: 20.0,
        falloff_min: 0.6,
        reload_ms: 1600,
        ..BASE
    },
    Arm {
        name: "ENERGY SWORD",
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
];

pub fn arm(id: u8) -> &'static Arm {
    &ARMS[(id as usize).min(COUNT - 1)]
}

/// Swapping guns takes this long before either can fire.
pub const SWAP_MS: u32 = 320;

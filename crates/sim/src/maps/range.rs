//! The Range: a practice map, out of the rotation (oneshot-server --map range): every gun on a pad in a row along the south wall, back in 5 s; a 60 m lane east to west along the pads for the long guns, a wall across the middle with a gap, low walls and a step block for grenades and the launcher to skip off, a ramp to lunge down from.
//!
//! Generated from `maps/range/map.json` by `tools/maps/compile.mjs`: edit the JSON,
//! then run it. CI fails if this file is not what the JSON compiles to.

use super::Def;
#[allow(unused_imports)]
use crate::arms::*;
#[allow(unused_imports)]
use crate::terrain::TerrainDef;

#[rustfmt::skip]
pub const DEF: Def = Def {
    id: 4,
    name: "The Range",
    practice: true,
    modes: 0,
    lo: (-30.0, -20.0),
    hi: (30.0, 20.0),
    void: f32::NEG_INFINITY,
    boxes: &[
        [-31.0, 0.0, -21.0, 31.0, 8.0, -20.0],
        [-31.0, 0.0, 20.0, 31.0, 8.0, 21.0],
        [-31.0, 0.0, -20.0, -30.0, 8.0, 20.0],
        [30.0, 0.0, -20.0, 31.0, 8.0, 20.0],
        [-24.0, 0.0, -20.0, 24.0, 0.2, -16.0],
        [-30.0, 0.0, 2.0, -4.0, 4.0, 3.0],
        [4.0, 0.0, 2.0, 30.0, 4.0, 3.0],
        [-14.0, 0.0, -2.0, -12.0, 1.2, 2.0],
        [12.0, 0.0, -2.0, 14.0, 1.2, 2.0],
        [-1.0, 0.0, 6.0, 1.0, 1.2, 10.0],
        [-4.0, 0.0, 13.0, 4.0, 1.0, 15.0],
        [20.0, 0.0, 6.0, 28.0, 1.0, 14.0],
        [22.0, 1.0, 8.0, 28.0, 2.5, 14.0],
        [-28.0, 0.0, 6.0, -20.0, 2.5, 14.0],
        [-20.0, 0.0, 6.0, -18.0, 1.2, 14.0],
    ],
    looks: &[0, 0, 0, 0, 2, 0, 0, 1, 1, 1, 1, 5, 5, 3, 3],
    spawns: &[
        ([-8.0, 0.0, -12.0], 16384, 255),
        ([8.0, 0.0, -12.0], 16384, 255),
        ([-20.0, 0.0, -10.0], 16384, 255),
        ([20.0, 0.0, -10.0], 16384, 255),
        ([-6.0, 0.0, 17.0], 49152, 255),
        ([6.0, 0.0, 17.0], 49152, 255),
        ([-24.0, 0.0, 0.0], 0, 255),
        ([24.0, 0.0, 0.0], 32768, 255),
    ],
    pads: &[
        ([-27.0, 0.0, -18.0], FRAGS, 5000),
        ([27.0, 0.0, -18.0], STICKIES, 5000),
        ([-21.0, 0.2, -18.0], AR, 5000),
        ([-18.0, 0.2, -18.0], BR, 5000),
        ([-15.0, 0.2, -18.0], MAGNUM, 5000),
        ([-12.0, 0.2, -18.0], SMG, 5000),
        ([-9.0, 0.2, -18.0], SHOTGUN, 5000),
        ([-6.0, 0.2, -18.0], SNIPER, 5000),
        ([-3.0, 0.2, -18.0], ROCKET, 5000),
        ([0.0, 0.2, -18.0], SWORD, 5000),
        ([3.0, 0.2, -18.0], CARBINE, 5000),
        ([6.0, 0.2, -18.0], BEAM_RIFLE, 5000),
        ([9.0, 0.2, -18.0], ION_RIFLE, 5000),
        ([12.0, 0.2, -18.0], ION_PISTOL, 5000),
        ([15.0, 0.2, -18.0], SHARDS, 5000),
        ([18.0, 0.2, -18.0], LAUNCHER, 5000),
        ([21.0, 0.2, -18.0], CANNON, 5000),
        ([-24.0, 0.2, -18.0], OVERSHIELD, 5000),
        ([24.0, 0.2, -18.0], CAMO, 5000),
    ],
    roam: &[[0.0, 0.0], [-10.0, -8.0], [10.0, -8.0], [-10.0, 8.0], [10.0, 8.0], [0.0, 17.0], [-24.0, 0.0], [24.0, 0.0]],
    terrain: None,
    launchers: &[
    ],
    vehicles: &[
    ],
    solids: &[
    ],
    solid_looks: &[],
    dress: &[
    ],
    dress_solids: &[
    ],
    profiles: &[
    ],
    dress_profiles: &[
    ],
    nav: false,
    flags: &[],
    light: crate::map::LightDef::DAY,
};

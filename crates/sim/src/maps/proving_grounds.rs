//! Proving Grounds: the test arena, 48 m square, mirrored between red (-x) and blue (+x): a raised centre with the rocket, a platform on each side reached by steps, pillars and waist-high walls for cover, the sniper and the shotgun at the ends of the middle lane.
//!
//! Generated from `maps/proving_grounds/map.json` by `tools/maps/compile.mjs`: edit the JSON,
//! then run it. CI fails if this file is not what the JSON compiles to.

use super::Def;
#[allow(unused_imports)]
use crate::arms::*;
#[allow(unused_imports)]
use crate::terrain::TerrainDef;

#[rustfmt::skip]
pub const DEF: Def = Def {
    id: 0,
    name: "Proving Grounds",
    practice: false,
    lo: (-24.0, -24.0),
    hi: (24.0, 24.0),
    boxes: &[
        [-25.0, 0.0, -25.0, 25.0, 8.0, -24.0],
        [-25.0, 0.0, 24.0, 25.0, 8.0, 25.0],
        [-25.0, 0.0, -24.0, -24.0, 8.0, 24.0],
        [24.0, 0.0, -24.0, 25.0, 8.0, 24.0],
        [-3.0, 0.0, -3.0, 3.0, 0.5, 3.0],
        [-2.0, 0.5, -2.0, 2.0, 1.0, 2.0],
        [-20.0, 0.0, -4.0, -12.0, 2.5, 4.0],
        [-12.0, 0.0, -1.5, -11.0, 2.0, 1.5],
        [-11.0, 0.0, -1.5, -10.0, 1.5, 1.5],
        [-10.0, 0.0, -1.5, -9.0, 1.0, 1.5],
        [-9.0, 0.0, -1.5, -8.0, 0.5, 1.5],
        [-8.75, 0.0, -12.75, -7.25, 3.0, -11.25],
        [-8.75, 0.0, 11.25, -7.25, 3.0, 12.75],
        [-6.0, 0.0, -16.25, 6.0, 1.2, -15.75],
        [12.0, 0.0, -4.0, 20.0, 2.5, 4.0],
        [11.0, 0.0, -1.5, 12.0, 2.0, 1.5],
        [10.0, 0.0, -1.5, 11.0, 1.5, 1.5],
        [9.0, 0.0, -1.5, 10.0, 1.0, 1.5],
        [8.0, 0.0, -1.5, 9.0, 0.5, 1.5],
        [7.25, 0.0, -12.75, 8.75, 3.0, -11.25],
        [7.25, 0.0, 11.25, 8.75, 3.0, 12.75],
        [-6.0, 0.0, 15.75, 6.0, 1.2, 16.25],
    ],
    looks: &[0, 0, 0, 0, 1, 1, 3, 1, 1, 1, 1, 1, 1, 1, 4, 1, 1, 1, 1, 1, 1, 1],
    spawns: &[
        ([-22.0, 0.0, 20.0], 0, 0),
        ([-22.0, 0.0, -20.0], 0, 0),
        ([-22.0, 0.0, 8.0], 0, 0),
        ([-22.0, 0.0, -8.0], 0, 0),
        ([22.0, 0.0, 20.0], 32768, 1),
        ([22.0, 0.0, -20.0], 32768, 1),
        ([22.0, 0.0, 8.0], 32768, 1),
        ([22.0, 0.0, -8.0], 32768, 1),
    ],
    pads: &[
        ([0.0, 1.0, 0.0], ROCKET, 120000),
        ([0.0, 0.0, 20.0], SNIPER, 90000),
        ([0.0, 0.0, -20.0], SHOTGUN, 60000),
        ([-16.0, 2.5, 0.0], SMG, 30000),
        ([16.0, 2.5, 0.0], SMG, 30000),
        ([0.0, 0.0, 10.0], OVERSHIELD, 120000),
        ([0.0, 0.0, -10.0], CAMO, 120000),
    ],
    roam: &[],
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
    nav: false,
    flags: &[],
    light: crate::map::LightDef::DAY,
};

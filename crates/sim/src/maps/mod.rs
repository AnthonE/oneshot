//! The maps, from `maps/*/map.json` by `tools/maps/compile.mjs`. Generated:
//! run the compiler, never edit by hand.

pub mod canyon;
pub mod cistern;
pub mod overpass;
pub mod proving_grounds;

pub use crate::map::Def;

/// Every map, in id order.
pub const ALL: [&Def; 4] = [&proving_grounds::DEF, &overpass::DEF, &cistern::DEF, &canyon::DEF];

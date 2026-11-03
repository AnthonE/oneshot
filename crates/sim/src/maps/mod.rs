//! The maps, from `maps/*/map.json` by `tools/maps/compile.mjs`. Generated:
//! run the compiler, never edit by hand.

pub mod bulkhead;
pub mod canyon;
pub mod chrysalis;
pub mod cistern;
pub mod overpass;
pub mod proving_grounds;

pub use crate::map::Def;

/// Every map, in id order.
pub const ALL: [&Def; 6] = [
    &proving_grounds::DEF,
    &overpass::DEF,
    &cistern::DEF,
    &canyon::DEF,
    &bulkhead::DEF,
    &chrysalis::DEF,
];

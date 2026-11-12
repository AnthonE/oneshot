//! The maps, from `maps/*/map.json` by `tools/maps/compile.mjs`. Generated:
//! run the compiler, never edit by hand.

pub mod bulkhead;
pub mod canyon;
pub mod chrysalis;
pub mod cistern;
pub mod cloudbreak;
pub mod icefall;
pub mod overpass;
pub mod pithead;
pub mod proving_grounds;
pub mod range;

pub use crate::map::Def;

/// Every map, in id order.
pub const ALL: [&Def; 10] = [
    &proving_grounds::DEF,
    &overpass::DEF,
    &cistern::DEF,
    &canyon::DEF,
    &range::DEF,
    &bulkhead::DEF,
    &chrysalis::DEF,
    &cloudbreak::DEF,
    &pithead::DEF,
    &icefall::DEF,
];

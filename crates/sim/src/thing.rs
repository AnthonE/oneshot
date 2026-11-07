//! What every kind of thing a world holds shares: whether it is in play and
//! how it feeds the world's hash. Each kind lives in its own fixed array (a
//! [`crate::pool::Pool`] where slots come and go, a plain array where the
//! index means something, like a seat or a map's pad), so the tick never
//! allocates.
//!
//! [`Kind`] names every kind. The places that must handle each one match on
//! it with no wildcard, so a new kind (a vehicle) doesn't compile until it
//! is hashed (`World::hash`), sent (`room::build`) and checked by the leak
//! gate (`room/tests/leaks.rs`). What the compiler can't see stays a short
//! list: `Pack` for its wire form (`proto`), its fog rule (`room/src/fog.rs`)
//! and how the client draws it (`client-net`).

use crate::hash::Fnv;

pub trait Thing: Copy + Default {
    /// In play: a seat taken, a projectile flying.
    fn live(&self) -> bool;
    /// Its whole state into the world's hash, field by field in a fixed
    /// order. Changing the order moves the golden.
    fn hash(&self, h: &mut Fnv);
}

/// Every kind of thing in a world, in the order the world hashes them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Fighter,
    Projectile,
    Pad,
    Vehicle,
    /// A gun (or grenades) on the floor where someone died or let it go.
    Drop,
    /// Capture the Flag's two flags (red's, blue's); always at rest at
    /// home outside that mode.
    Flag,
    /// Siege's generators and cores; untouched outside that mode.
    Node,
    /// Siege's minions; none outside that mode.
    Minion,
}

impl Kind {
    pub const ALL: [Kind; 8] = [
        Kind::Fighter,
        Kind::Projectile,
        Kind::Pad,
        Kind::Vehicle,
        Kind::Drop,
        Kind::Flag,
        Kind::Node,
        Kind::Minion,
    ];
}

/// Hashes every slot of `items`, in play or not, so a slot's history counts.
pub fn hash_all<T: Thing>(items: &[T], h: &mut Fnv) {
    for t in items {
        t.hash(h);
    }
}

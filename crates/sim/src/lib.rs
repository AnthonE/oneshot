//! The game's rules. Deterministic: the same inputs give the same bits
//! native (the server) and wasm (the browser's prediction). No I/O, clocks,
//! threads, `HashMap` iteration or platform libm (`clippy.toml`), and no
//! allocation in [`World::step`] after setup (`tests/alloc.rs`).

#![forbid(unsafe_code)]

pub mod arms;
pub mod bot;
pub mod consts;
pub mod contact;
pub mod events;
pub mod fighter;
pub mod fmath;
pub mod geom;
pub mod grid;
pub mod hash;
pub mod input;
pub mod map;
pub mod maps;
pub mod movement;
pub mod nav;
pub mod pool;
pub mod profile;
pub mod rewind;
pub mod rng;
pub mod solid;
pub mod terrain;
pub mod thing;
pub mod vehicle;
pub mod world;

/// This sim's stamp: a hash of its sources, its rules and every compiled
/// map (`build.rs`). Sent in the handshake (`proto::Msg::Hello`); the
/// server refuses a client whose stamp is not its own, so a page left over
/// from before a map or a rule changed never predicts against walls or
/// numbers the server doesn't have.
pub const STAMP: u32 = include!(concat!(env!("OUT_DIR"), "/stamp.rs"));

pub use input::Input;
pub use map::Map;
pub use world::{Phase, Rules, World};

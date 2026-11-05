//! The client's netcode. [`core`] is the logic (with [`proj`], rounds in
//! flight as drawn) and [`session`] the connection's game side, neither
//! with a transport in it; `web` (wasm) puts a session on the browser's
//! WebTransport and hands it to the page, `native` (the `native` feature)
//! on wtransport for a native client.
//! [`tape`] is what a session was sent, played back to one; [`clip`] keeps
//! a live session's last minute as one, for the clip key.

pub mod clip;
pub mod core;
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
pub mod native;
pub mod proj;
pub mod session;
pub mod tape;
#[cfg(target_arch = "wasm32")]
pub mod web;
#[cfg(target_arch = "wasm32")]
pub mod web_lobby;

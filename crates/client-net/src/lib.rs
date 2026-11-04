//! The client's netcode. [`core`] is the logic (with [`proj`], rounds in
//! flight as drawn) and [`session`] the connection's game side, neither
//! with a transport in it; `web` (wasm) puts a session on the browser's
//! WebTransport and hands it to the page, `native` (the `native` feature)
//! on wtransport for a native client.

pub mod core;
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
pub mod native;
pub mod proj;
pub mod session;
#[cfg(target_arch = "wasm32")]
pub mod web;

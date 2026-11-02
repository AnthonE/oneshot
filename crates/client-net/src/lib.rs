//! The browser's netcode. [`core`] is the logic, with no transport;
//! `web` (wasm only) puts it on WebTransport and hands it to the page.

pub mod core;
#[cfg(target_arch = "wasm32")]
pub mod web;

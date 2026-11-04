//! `cfg(webgl2)`: the browser module that draws through WebGL2 (after
//! Gates' `crates/client/build.rs`, MIT).
//!
//! The page ships two modules (ci/client.sh): one built with the `webgpu`
//! feature, which a browser with a WebGPU adapter loads, and one without,
//! for every other browser. Both are the same platform (one thread, the
//! page's socket), so being in a tab stays `target_arch = "wasm32"`. What
//! differs is the GPU API, which Bevy fixes at compile time: under `webgpu`
//! it drops every WebGL workaround it has. This names that set once, so a
//! downgrade that exists because WebGL2 can't do something is spelled
//! `cfg(webgl2)` and the WebGPU module takes the desktop's path.
fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rustc-check-cfg=cfg(webgl2)");
    let wasm = std::env::var("CARGO_CFG_TARGET_ARCH").is_ok_and(|a| a == "wasm32");
    let webgpu = std::env::var_os("CARGO_FEATURE_WEBGPU").is_some();
    if wasm && !webgpu {
        println!("cargo::rustc-cfg=webgl2");
    }
}

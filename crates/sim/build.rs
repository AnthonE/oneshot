//! The sim's stamp (`sim::STAMP`): FNV-1a over every source file under
//! `src/` (its rules and every compiled map), by path in order, carriage
//! returns dropped so a checkout with Windows line ends stamps the same.
//! Two builds whose sims could play differently never share it; the
//! handshake refuses a client whose stamp is not the server's, so nobody
//! predicts against walls the server doesn't have (a map's layout changes
//! without the wire's `PROTO_VER` moving).
//!
//! A build script reads files and formats text; the sim's walls
//! (`clippy.toml`) are for the sim it builds, not for this.
#![allow(clippy::disallowed_methods, clippy::disallowed_types, clippy::disallowed_macros)]

use std::fs;
use std::path::{Path, PathBuf};

fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let src = root.join("src");
    println!("cargo:rerun-if-changed=src");
    let mut all = Vec::new();
    files(&src, &mut all);
    let rel = |p: &Path| {
        p.strip_prefix(&src)
            .unwrap_or(p)
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/")
    };
    all.sort_by_key(|p| rel(p));
    let mut h: u32 = 0x811c_9dc5;
    let mut eat = |b: u8| {
        h ^= u32::from(b);
        h = h.wrapping_mul(0x0100_0193);
    };
    for p in &all {
        for b in rel(p).bytes() {
            eat(b);
        }
        eat(0);
        for b in fs::read(p).unwrap_or_default() {
            if b != b'\r' {
                eat(b);
            }
        }
        eat(0);
    }
    let out = Path::new(&std::env::var("OUT_DIR").expect("OUT_DIR")).join("stamp.rs");
    fs::write(out, format!("0x{h:08x}\n")).expect("the stamp");
}

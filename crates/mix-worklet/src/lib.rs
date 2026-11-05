//! `mix` on the browser's audio thread: the exports
//! `crates/client/web/mix-worklet.js` calls from its `AudioWorkletProcessor`.
//! Plain ones, no wasm-bindgen: the processor writes bytes or floats into
//! [`mix_buf`], calls [`mix_cmds`] or [`mix_install`], and reads each block
//! from [`mix_render`]'s pointer. One processor, one thread: the engine is a
//! thread-local.

use std::cell::RefCell;

use mix::{Clip, Cmd, Engine, BLOCK};

struct Worklet {
    engine: Engine,
    /// Where the processor writes what it hands over.
    buf: Vec<u8>,
    /// The last block, interleaved, and the same as two planes (left, then
    /// right) for the processor's two channels.
    mixed: [f32; BLOCK * 2],
    planes: [f32; BLOCK * 2],
    stats: [u32; 7],
}

/// What [`mix_buf`] holds from the start: a frame's commands fit many times
/// over, so it grows only for a clip (at load), never mid-game.
const BUF_START: usize = 64 << 10;

thread_local! {
    static W: RefCell<Worklet> = RefCell::new(Worklet {
        engine: Engine::default(),
        buf: Vec::with_capacity(BUF_START),
        mixed: [0.0; BLOCK * 2],
        planes: [0.0; BLOCK * 2],
        stats: [0; 7],
    });
}

/// Room for `len` bytes to be written; its address. Valid until the next
/// call here (memory may grow under it). It only grows: what is past `len`
/// is stale, and the calls below read only the `len` they are told.
#[no_mangle]
pub extern "C" fn mix_buf(len: usize) -> *mut u8 {
    W.with(|w| {
        let w = &mut *w.borrow_mut();
        if w.buf.len() < len {
            w.buf.resize(len, 0);
        }
        w.buf.as_mut_ptr()
    })
}

/// The first `len` bytes of [`mix_buf`] are a frame's commands
/// (`mix::encode`).
#[no_mangle]
pub extern "C" fn mix_cmds(len: usize) {
    W.with(|w| {
        let w = &mut *w.borrow_mut();
        let n = len.min(w.buf.len());
        let Worklet { engine, buf, .. } = w;
        engine.apply_bytes(&buf[..n]);
    })
}

/// The first `n` floats of [`mix_buf`] are clip `id`'s mono samples at
/// `rate`.
#[no_mangle]
pub extern "C" fn mix_install(id: u32, rate: u32, n: usize) {
    W.with(|w| {
        let w = &mut *w.borrow_mut();
        let n = n.min(w.buf.len() / 4);
        let pcm: Vec<f32> = w.buf[..n * 4]
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect();
        w.engine.apply(Cmd::Install {
            id: id as u16,
            clip: Clip::from_f32(&pcm, rate),
        });
    })
}

/// Renders the next block; the address of its two planes, [`BLOCK`] left
/// samples then [`BLOCK`] right.
#[no_mangle]
pub extern "C" fn mix_render() -> *const f32 {
    W.with(|w| {
        let w = &mut *w.borrow_mut();
        let Worklet {
            engine, mixed, planes, ..
        } = w;
        engine.render(mixed);
        for i in 0..BLOCK {
            planes[i] = mixed[2 * i];
            planes[BLOCK + i] = mixed[2 * i + 1];
        }
        planes.as_ptr()
    })
}

/// Frames a block holds.
#[no_mangle]
pub extern "C" fn mix_block() -> usize {
    BLOCK
}

/// The engine's gauges (`mix::Stats`) as seven u32s: blocks, started,
/// refused, missing, installed, playing, held.
#[no_mangle]
pub extern "C" fn mix_stats() -> *const u32 {
    W.with(|w| {
        let w = &mut *w.borrow_mut();
        let s = w.engine.stats();
        w.stats = [
            s.blocks,
            s.started,
            s.refused,
            s.missing,
            s.installed,
            s.playing,
            s.held,
        ];
        w.stats.as_ptr()
    })
}

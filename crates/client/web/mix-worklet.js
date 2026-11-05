// The game's sound on the browser's audio thread: an AudioWorkletProcessor
// around `mix_worklet.wasm` (crates/mix-worklet, the game's mixer, crates/mix),
// after Gates' audio-processor.js. mix.js makes it and posts it the wasm's
// bytes (a worklet's scope can't fetch), each decoded clip and each frame's
// commands. Nothing here decides anything: it moves bytes in and samples out.

// Quanta between reports to the page (~256 ms at 48 kHz): gauges, not
// control, and a message is an allocation on this thread.
const REPORT_EVERY = 96;

class OneshotMix extends AudioWorkletProcessor {
  constructor() {
    super();
    this.x = null;
    this.mem = null;
    this.block = 0;
    // Views over the wasm's memory: growing it (installing a clip does)
    // detaches every one, and a detached view reads zeros, so the buffer
    // is compared each quantum and the views made again when it moved.
    this.buffer = null;
    this.out = null;
    // The block's two planes, and the whole memory as bytes: made with the
    // views above, so a quantum and a message make no garbage here.
    this.outL = null;
    this.outR = null;
    this.heap = null;
    this.since = 0;
    this.port.onmessage = (e) => this.on(e.data);
    this.port.onmessageerror = () => this.port.postMessage({ kind: 'error', why: 'a message did not decode' });
  }

  // `bytes` (a Uint8Array) into the wasm's buffer.
  put(bytes) {
    const p = this.x.mix_buf(bytes.length);
    // mix_buf may have grown the memory, which detaches the old view.
    if (this.heap === null || this.heap.buffer !== this.mem.buffer) this.heap = new Uint8Array(this.mem.buffer);
    this.heap.set(bytes, p);
  }

  on(m) {
    try {
      if (m.kind === 'init') {
        // Compiled here, from bytes: a compiled module doesn't always cross
        // into a worklet's scope (Gates measured Chromium refusing one).
        const inst = new WebAssembly.Instance(new WebAssembly.Module(m.bytes), {});
        this.x = inst.exports;
        this.mem = this.x.memory;
        this.block = this.x.mix_block();
        return;
      }
      if (!this.x) return;
      if (m.kind === 'cmds') {
        this.put(m.bytes);
        this.x.mix_cmds(m.bytes.byteLength);
      } else if (m.kind === 'install') {
        this.put(new Uint8Array(m.pcm.buffer, m.pcm.byteOffset, m.pcm.byteLength));
        this.x.mix_install(m.id, m.rate, m.pcm.length);
      }
    } catch (e) {
      // Thrown in here, it would read on the page as plain silence.
      this.port.postMessage({ kind: 'error', why: String((e && e.message) || e) });
    }
  }

  process(_inputs, outputs) {
    const out = outputs[0];
    if (!this.x || !out || out.length === 0) return true;
    const left = out[0];
    const right = out.length > 1 ? out[1] : null;
    const n = this.block;
    let filled = 0;
    while (filled < left.length) {
      const p = this.x.mix_render();
      if (this.buffer !== this.mem.buffer || this.out === null || this.out.byteOffset !== p) {
        this.buffer = this.mem.buffer;
        this.out = new Float32Array(this.buffer, p, 2 * n);
        this.outL = this.out.subarray(0, n);
        this.outR = this.out.subarray(n, 2 * n);
      }
      const k = Math.min(n, left.length - filled);
      if (k === n) {
        left.set(this.outL, filled);
        if (right) right.set(this.outR, filled);
      } else {
        left.set(this.out.subarray(0, k), filled);
        if (right) right.set(this.out.subarray(n, n + k), filled);
      }
      filled += k;
    }
    for (let c = 2; c < out.length; c++) out[c].set(left);
    if (++this.since >= REPORT_EVERY) {
      this.since = 0;
      const s = new Uint32Array(this.mem.buffer, this.x.mix_stats(), 7);
      this.port.postMessage({ kind: 'stats', s: Array.from(s) });
    }
    // Never false: that retires the processor for good.
    return true;
  }
}

registerProcessor('oneshot-mix', OneshotMix);

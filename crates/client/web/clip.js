/* clip.js — the clip page: a player's last half minute drawn by the game
   itself and encoded on their own machine. After Gates' web/clip.js (MIT).

   The game page (`boot.js`, `window.__oneshotClip`) frames this page and
   posts it the tape the clip key took (crates/client/src/clip.rs). This page
   loads the same module the game runs, hands it the tape (`film`,
   crates/client/src/film/), and takes each frame and each slice of the mix
   as the film draws it. The frames go through WebCodecs to H.264, the mix
   to AAC (Opus where the browser has no AAC encoder), and `mp4.js` writes
   the file. The video goes back to the game page as a Blob. Nothing is
   uploaded: there is no server on this path at all.

   A page holds one Bevy app for its life, so one film per frame: the game
   page makes a new frame for every clip. */
import { mux } from "./mp4.js";

const q = new URLSearchParams(location.search);
const parent = window.parent !== window ? window.parent : window.opener;
const tell = (msg, transfer) => { if (parent) parent.postMessage(msg, location.origin, transfer || []); };

const MODULES = {
  webgl2: { js: "./oneshot_client.js", wasm: "oneshot_client_bg.wasm" },
  webgpu: { js: "./oneshot_client_gpu.js", wasm: "oneshot_client_gpu_bg.wasm" },
};

/* The film's hooks into its page (look/gpu.rs): a panic, or a WebGPU device
   lost, ends the clip; a refused frame under WebGPU is logged. */
window.__oneshotPanic = (msg) => fail(String(msg || "the game stopped"));
window.__oneshotGpuError = (msg, kind) => { if (kind === "lost") fail("the graphics device was lost"); };

let failed = false;
function fail(why) {
  if (failed) return;
  failed = true;
  console.error("clip:", why);
  tell({ type: "clip-failed", why });
}

async function load(gfx) {
  const m = MODULES[gfx] || MODULES.webgl2;
  let v = "";
  try {
    const b = await (await fetch("build.json", { cache: "no-cache" })).json();
    const sha = b && b.modules && b.modules[gfx] && b.modules[gfx].sha256;
    if (sha) v = "?v=" + String(sha).slice(0, 12);
  } catch (err) { /* a checkout with no build.json: unversioned names */ }
  const res = fetch(m.wasm + v);
  const mod = await import(m.js + v);
  await mod.default({ module_or_path: res });
  return mod;
}

/* ── what goes over the picture ─────────────────────────────────────────── */

const WORD = "Unitblock";
const FACE = "Aileron";
const fonts = Promise.all([
  new FontFace(WORD, "url(assets/fonts/Unitblock.ttf)").load(),
  new FontFace(FACE, "url(assets/fonts/Aileron-Bold.otf)", { weight: "700" }).load(),
]).then((faces) => { for (const f of faces) document.fonts.add(f); }).catch(() => {});

function text(g, s, x, y, px, face, align, alpha) {
  g.save();
  g.globalAlpha = alpha;
  g.font = face === WORD ? `400 ${px}px "${WORD}", sans-serif` : `700 ${px}px "${FACE}", system-ui, sans-serif`;
  g.textAlign = align;
  g.textBaseline = "middle";
  g.lineWidth = Math.max(2, px * 0.12);
  g.strokeStyle = "rgba(0,0,0,.55)";
  g.lineJoin = "round";
  g.strokeText(s, x, y);
  g.fillStyle = "#edebe6";
  g.fillText(s, x, y);
  g.restore();
}

/* The player's own line, across the top for the first seconds: what a post
   says over its first frame. */
function hook(g, w, h, line, t) {
  if (!line || t > 3.2) return;
  const a = Math.min(1, t / 0.25, (3.2 - t) / 0.3);
  const px = Math.round(Math.min(w, h) * 0.07);
  text(g, line, w / 2, h * (h > w ? 0.17 : 0.14), px, FACE, "center", a);
}

/* The tag in a corner while the clip plays: where it was made. */
function tag(g, w, h) {
  const px = Math.round(Math.min(w, h) * 0.034);
  text(g, "ONESHOT", px * 1.1, h - px * 2.2, px, WORD, "left", 0.72);
}

/* The end card: the last frame dimmed, the wordmark, where to play. */
function card(g, w, h, last, link, u) {
  if (last) g.drawImage(last, 0, 0, w, h);
  g.fillStyle = `rgba(27,25,22,${0.35 + 0.45 * Math.min(1, u * 3)})`;
  g.fillRect(0, 0, w, h);
  const a = Math.min(1, u * 4);
  const big = Math.round(Math.min(w, h) * 0.16);
  text(g, "ONESHOT", w / 2, h * 0.44, big, WORD, "center", a);
  g.save();
  g.globalAlpha = a;
  g.fillStyle = "#e8b931";
  g.fillRect(w / 2 - big * 0.6, h * 0.44 + big * 0.62, big * 1.2, Math.max(3, big * 0.05));
  g.restore();
  text(g, "Arena slayer, free in your browser", w / 2, h * 0.44 + big * 1.0, Math.round(big * 0.2), FACE, "center", a);
  text(g, link, w / 2, h * 0.44 + big * 1.4, Math.round(big * 0.22), FACE, "center", a);
}

/* ── the encoders ───────────────────────────────────────────────────────── */

const AVC = ["avc1.640028", "avc1.4d0028", "avc1.640032", "avc1.42e032"];

async function videoConfig(w, h, fps) {
  if (!("VideoEncoder" in window)) throw new Error("this browser can't encode video here (no WebCodecs): try Chrome, Edge, Firefox or Safari");
  for (const codec of AVC) {
    const c = { codec, width: w, height: h, framerate: fps, bitrate: w * h > 1.5e6 ? 10e6 : 6e6, avc: { format: "avc" } };
    try {
      const r = await VideoEncoder.isConfigSupported(c);
      if (r.supported) return r.config;
    } catch (err) { /* the next one */ }
  }
  throw new Error("this browser can't encode H.264 video: try Chrome, Edge or Safari");
}

async function audioConfig() {
  if (!("AudioEncoder" in window)) return null;
  for (const c of [
    { codec: "mp4a.40.2", sampleRate: 48000, numberOfChannels: 2, bitrate: 160000 },
    { codec: "opus", sampleRate: 48000, numberOfChannels: 2, bitrate: 128000 },
  ]) {
    try {
      const r = await AudioEncoder.isConfigSupported(c);
      if (r.supported) return r.config;
    } catch (err) { /* the next one */ }
  }
  return null;
}

const bytes = (d) => {
  if (!d) return null;
  if (d instanceof ArrayBuffer) return new Uint8Array(d);
  return new Uint8Array(d.buffer.slice(d.byteOffset, d.byteOffset + d.byteLength));
};

/* ── the film's sink ────────────────────────────────────────────────────── */

function sink(opts) {
  let w = 0, h = 0, fps = 30, frames = 0;
  let canvas = null, g = null, scratch = null, sg = null, last = null;
  let encoder = null, avcC = null;
  const samples = [];
  const pcm = [];
  let pcmLen = 0;
  let encErr = null;
  let ready = null;

  function open(ow, oh, rate) {
    w = ow; h = oh; fps = rate;
    canvas = new OffscreenCanvas(w, h);
    g = canvas.getContext("2d", { alpha: false });
    ready = videoConfig(w, h, fps).then((config) => {
      encoder = new VideoEncoder({
        output: (chunk, meta) => {
          if (meta && meta.decoderConfig && meta.decoderConfig.description) avcC = bytes(meta.decoderConfig.description);
          const data = new Uint8Array(chunk.byteLength);
          chunk.copyTo(data);
          samples.push({ data, pts: chunk.timestamp, key: chunk.type === "key" });
        },
        error: (e) => { encErr = e; },
      });
      encoder.configure(config);
    });
    ready.catch((e) => fail(e.message || String(e)));
  }

  function emit(draw) {
    const t = frames / fps;
    draw(g, t);
    const vf = new VideoFrame(canvas, { timestamp: Math.round((frames * 1e6) / fps), duration: Math.round(1e6 / fps) });
    encoder.encode(vf, { keyFrame: frames % (fps * 2) === 0 });
    vf.close();
    frames++;
  }

  /* Frames arrive faster than an encoder may take them only on a fast box;
     they are held here and fed in order as it drains. */
  const queue = [];
  let pumping = false;
  let broke = null;
  async function pump() {
    if (pumping) return;
    pumping = true;
    try {
      await ready;
      while (queue.length) {
        if (encErr) throw encErr;
        if (encoder.encodeQueueSize > 8) { await new Promise((r) => setTimeout(r, 4)); continue; }
        emit(queue.shift());
      }
    } catch (e) {
      // What is left would never be fed: done() stops waiting on it.
      broke = e;
      queue.length = 0;
      fail(e.message || String(e));
    } finally {
      pumping = false;
    }
  }

  return {
    shot(i, name, dw, dh, ow, oh, rate) {
      if (!canvas) open(ow, oh, rate);
      if (dw !== w || dh !== h) {
        scratch = new OffscreenCanvas(dw, dh);
        sg = scratch.getContext("2d");
      } else {
        scratch = null;
      }
    },
    frame(i, n, rgba, dw, dh) {
      const img = new ImageData(new Uint8ClampedArray(rgba.buffer, rgba.byteOffset, rgba.byteLength), dw, dh);
      // Scaled as its own shot is (a later shot may come before it is fed).
      const sc = scratch, scg = sg;
      queue.push((g, t) => {
        if (sc) {
          scg.putImageData(img, 0, 0);
          g.drawImage(sc, 0, 0, w, h);
        } else {
          g.putImageData(img, 0, 0);
        }
        if (!last) last = new OffscreenCanvas(w, h);
        last.getContext("2d").drawImage(canvas, 0, 0);
        if (opts.tag) tag(g, w, h);
        hook(g, w, h, opts.line, t);
      });
      pump();
    },
    audio(i, slice) {
      pcm.push(slice);
      pcmLen += slice.length;
    },
    shotEnd() {},
    progress(f) { tell({ type: "clip-progress", f }); },
    async done() {
      try {
        await fonts;
        if (opts.tag) {
          const n = Math.round(fps * 2.4);
          for (let k = 0; k < n; k++) {
            const u = k / n;
            queue.push((g) => card(g, w, h, last, opts.link, u));
          }
        }
        await pump();
        while (queue.length || pumping) await new Promise((r) => setTimeout(r, 10));
        if (broke) return;
        if (!encoder) throw new Error("no frames reached the encoder");
        await encoder.flush();
        if (encErr) throw encErr;
        tell({ type: "clip-progress", f: 1, encoding: true });
        const audio = await encodeAudio(pcm, pcmLen, frames / fps);
        const blob = mux({ video: { samples, width: w, height: h, fps, avcC }, audio });
        tell({ type: "clip-done", blob, seconds: frames / fps, width: w, height: h });
      } catch (e) {
        fail(e.message || String(e));
      }
    },
    fail(why) { fail(String(why)); },
  };
}

/* The game's own mix, laid end to end, run out to the video's length with a
   fade over the end card, and encoded. */
async function encodeAudio(chunks, len, seconds) {
  const config = await audioConfig();
  if (!config) return null;
  const rate = 48000, ch = 2;
  const total = Math.round(seconds * rate) * ch;
  const all = new Float32Array(total);
  let at = 0;
  for (const c of chunks) {
    if (at >= total) break;
    all.set(c.subarray(0, Math.min(c.length, total - at)), at);
    at += c.length;
  }
  const fadeFrom = Math.max(0, Math.min(at, total) - rate * ch * 0.8);
  for (let i = fadeFrom; i < total; i++) all[i] *= Math.max(0, 1 - (i - fadeFrom) / (rate * ch * 0.8));

  const out = [];
  let desc = null;
  let err = null;
  const encoder = new AudioEncoder({
    output: (chunk, meta) => {
      if (meta && meta.decoderConfig && meta.decoderConfig.description) desc = bytes(meta.decoderConfig.description);
      const data = new Uint8Array(chunk.byteLength);
      chunk.copyTo(data);
      out.push({ data, pts: chunk.timestamp, key: true });
    },
    error: (e) => { err = e; },
  });
  encoder.configure(config);
  const block = 4800;
  for (let f = 0; f * ch < total; f += block) {
    const n = Math.min(block, total / ch - f);
    encoder.encode(new AudioData({
      format: "f32", sampleRate: rate, numberOfChannels: ch, numberOfFrames: n,
      timestamp: Math.round((f * 1e6) / rate), data: all.subarray(f * ch, (f + n) * ch),
    }));
  }
  await encoder.flush();
  if (err) throw err;
  const opus = config.codec === "opus";
  return {
    samples: out,
    codec: opus ? "opus" : "aac",
    /* AAC-LC, 48 kHz, stereo, when the encoder named no config of its own. */
    config: desc || (opus ? null : new Uint8Array([0x11, 0x90])),
    rate, channels: ch,
    frame: opus ? 960 : 1024,
    bitrate: config.bitrate || 128000,
  };
}

/* ── the handoff ────────────────────────────────────────────────────────── */

let started = false;
window.addEventListener("message", async (ev) => {
  if (ev.origin !== location.origin || !ev.data || ev.data.type !== "clip-tape" || started) return;
  started = true;
  const { tape, style, line, link, tag: withTag, quality } = ev.data;
  try {
    const mod = await load(q.get("gfx") || "webgl2");
    await fonts;
    const s = sink({ line: String(line || "").slice(0, 60), link: link || "elopros.com/games/oneshot/play", tag: withTag !== false });
    /* Bevy's loop owns the page from here (`film`, crates/client/src/lib.rs). */
    mod.film(new Uint8Array(tape), style || "wide", "#film", s, String(quality || ""));
  } catch (e) {
    const why = (e && e.message) || String(e);
    /* winit hands Bevy's loop to the browser with a throw it says is not an
       error; the film is running. */
    if (!/exceptions for control flow/.test(why)) fail(why);
  }
});
tell({ type: "clip-ready" });

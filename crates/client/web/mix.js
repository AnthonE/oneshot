// The game's sound in a browser (crates/client/src/sound): the mixer runs
// on the browser's audio thread (mix-worklet.js), not on a timer on this
// page's (Bevy's own audio filled buffers on the page's thread every ~46 ms;
// Gates measured it and moved off it). The game hands each clip's Ogg here
// once, which the browser decodes off this thread, and each frame's
// commands as bytes (crates/mix's `encode`). `window.__oneshotMix` is what
// the game calls (sound::on, wasm).

// The mixer's rate (crates/mix's RATE): the context is made at it.
const RATE = 48000;
// Messages kept while the worklet starts; past this the oldest go.
const WAITING = 512;

export function startMix(v) {
  let ctx = null;
  try {
    ctx = new AudioContext({ sampleRate: RATE, latencyHint: 'interactive' });
  } catch {
    try { ctx = new AudioContext(); } catch { /* no Web Audio: the game plays silent */ }
  }
  let node = null;
  const waiting = [];
  const st = { installed: 0, failed: 0, sent: 0, errors: 0, mixer: null };
  const post = (m, transfer) => {
    if (node) node.port.postMessage(m, transfer);
    else {
      waiting.push([m, transfer]);
      if (waiting.length > WAITING) waiting.shift();
    }
  };
  // The theme (assets/sound/music/intro.ogg): too long for the worklet's
  // bank, so the browser streams it itself, looping, at the level the game
  // last asked for. Not fetched until it is asked for.
  let tune = null;
  let tuneAt = 0;
  const tunePlay = () => { if (tune && tuneAt > 0 && tune.paused) tune.play().catch(() => {}); };
  if (ctx) {
    (async () => {
      await ctx.audioWorklet.addModule(`mix-worklet.js${v}`);
      const bytes = await (await fetch(`mix_worklet.wasm${v}`)).arrayBuffer();
      node = new AudioWorkletNode(ctx, 'oneshot-mix', { numberOfInputs: 0, outputChannelCount: [2] });
      node.port.onmessage = (e) => {
        const m = e.data;
        if (m.kind === 'stats') st.mixer = m.s;
        else if (m.kind === 'error') {
          st.errors++;
          console.warn('oneshot: sound:', m.why);
        }
      };
      node.port.postMessage({ kind: 'init', bytes }, [bytes]);
      node.connect(ctx.destination);
      for (const [m, t] of waiting.splice(0)) node.port.postMessage(m, t);
    })().catch((e) => console.warn('oneshot: the sound did not start', e));
    // A browser starts a context only after a gesture: on the first one.
    const wake = () => { if (ctx.state === 'suspended') ctx.resume().catch(() => {}); };
    addEventListener('pointerdown', wake, { capture: true });
    addEventListener('keydown', wake, { capture: true });
  }
  // A browser plays a tune, too, only after a gesture.
  addEventListener('pointerdown', tunePlay, { capture: true });
  addEventListener('keydown', tunePlay, { capture: true });
  window.__oneshotMix = {
    /** Clip `id`'s Ogg, decoded by the browser into the worklet's bank. */
    install(id, ogg) {
      if (!ctx) return;
      ctx.decodeAudioData(ogg.buffer.slice(ogg.byteOffset, ogg.byteOffset + ogg.byteLength)).then((buf) => {
        const pcm = new Float32Array(buf.length);
        for (let c = 0; c < buf.numberOfChannels; c++) {
          const ch = buf.getChannelData(c);
          for (let i = 0; i < pcm.length; i++) pcm[i] += ch[i] / buf.numberOfChannels;
        }
        post({ kind: 'install', id, rate: buf.sampleRate, pcm }, [pcm.buffer]);
        st.installed++;
      }, () => { st.failed++; });
    },
    /** A frame's commands (taken: the game made these bytes for us). */
    send(bytes) {
      st.sent++;
      post({ kind: 'cmds', bytes }, [bytes.buffer]);
    },
    /** The theme at `level` (0: stopped, and from its start the next time). */
    theme(level) {
      tuneAt = level;
      if (!tune) {
        if (level <= 0) return;
        tune = new Audio(`assets/sound/music/intro.ogg${v}`);
        tune.loop = true;
      }
      tune.volume = Math.min(1, Math.max(0, level));
      if (level > 0) tunePlay();
      else {
        tune.pause();
        tune.currentTime = 0;
      }
    },
    /** For the browser test: decoded, failed, frames sent, the context's
     * state, worklet errors, and the worklet's gauges (crates/mix Stats:
     * blocks, started, refused, missing, installed, playing, held). */
    stats: () => ({ ...st, state: ctx ? ctx.state : 'none' }),
  };
}

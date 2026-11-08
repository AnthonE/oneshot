// The game's sound in a browser (crates/client/src/sound): the mixer runs
// on the browser's audio thread (mix-worklet.js), not on a timer on this
// page's (Bevy's own audio filled buffers on the page's thread every ~46 ms;
// Gates measured it and moved off it). The game hands each clip's Ogg here
// once, which the browser decodes off this thread, and each frame's
// commands as bytes (crates/mix's `encode`). `window.__oneshotMix` is what
// the game calls (sound::on, wasm).
//
// Silence is never quiet about itself: the console says when the sound is
// on and why it isn't, and a bar over the canvas (#soundbar) says so when
// it failed or the browser held it after a click.

// The mixer's rate (crates/mix's RATE): the context is made at it.
const RATE = 48000;
// Frames' commands kept while the worklet starts: what lasts (a volume, a
// loop held or stopped; the desktop's inbox keeps those too: crates/mix
// Shared::send). One-shots are not among them (`sieve`, below). A worklet
// not up after this many frames (minutes) never will be: no sound, said,
// rather than a queue that only grows. Clips are kept apart.
const WAITING = 7200;
// After a gesture, how long the context has to start before the bar says
// the browser is holding it (a muted site, an autoplay block).
const HELD_MS = 1500;
// A one-shot sent after a gesture waits this long for the context to run
// (it starts on that gesture, within a tenth of a second as a rule; a slow
// machine's frame may itself be late to send the click): the click that
// wakes the sound is heard, a touch late. Older than this, it is stale, and
// dropped.
const HOLD_MS = 1000;
// What a browser counts as a person's gesture: any one may start a context.
const GESTURES = ['pointerdown', 'pointerup', 'mousedown', 'touchend', 'keydown', 'click'];

// crates/mix's `encode`: a command is its tag, then its bytes (a place is
// four f32s: each ear's gain, the rate, the filter). Kept in step with it;
// the mixer's own test pins the Rust side (`bytes_play_what_the_commands_play`).
const T_PLAY = 1;
const PLACE = 16;
const AFTER_TAG = { 1: 2 + PLACE + 4, 2: 1 + 2 + PLACE, 3: 1 + PLACE, 4: 1, 5: 4 };

// `bytes` sieved: what lasts (a loop held, moved or stopped, the volume),
// then the one-shots (T_PLAY), and how many of those. A torn tail is
// dropped, as the mixer drops it.
function sieve(bytes) {
  const lasting = new Uint8Array(bytes.byteLength);
  const shots = new Uint8Array(bytes.byteLength);
  let l = 0;
  let s = 0;
  let n = 0;
  for (let i = 0; i < bytes.byteLength;) {
    const len = AFTER_TAG[bytes[i]];
    if (len === undefined || i + 1 + len > bytes.byteLength) break;
    const end = i + 1 + len;
    if (bytes[i] === T_PLAY) {
      shots.set(bytes.subarray(i, end), s);
      s += end - i;
      n++;
    } else {
      lasting.set(bytes.subarray(i, end), l);
      l += end - i;
    }
    i = end;
  }
  return [lasting.subarray(0, l), shots.subarray(0, s), n];
}

// `v` versions what comes with the game (the theme), `mv` the mixer's own
// pair, worklet and wasm (build.json's `mix`: it changes on its own).
export function startMix(v, mv = v) {
  let ctx = null;
  let why = 'starting';
  let failed = false;
  try {
    ctx = new AudioContext({ sampleRate: RATE, latencyHint: 'interactive' });
  } catch {
    try { ctx = new AudioContext(); } catch { /* below */ }
  }
  let node = null;
  const waiting = [];
  const clips = [];
  const st = { installed: 0, failed: 0, sent: 0, unheard: 0, errors: 0, mixer: null };
  const post = (m, transfer) => {
    if (failed) return;
    if (node) node.port.postMessage(m, transfer);
    else {
      waiting.push([m, transfer]);
      if (waiting.length > WAITING) fail('the mixer did not start');
    }
  };
  const bar = (text) => {
    let b = document.getElementById('soundbar');
    if (!text) { if (b) b.hidden = true; return; }
    if (!b) {
      b = document.createElement('div');
      b.id = 'soundbar';
      document.body.appendChild(b);
    }
    b.textContent = text;
    b.hidden = false;
  };
  const fail = (what, e) => {
    failed = true;
    why = `${what}${e ? `: ${String(e?.message ?? e)}` : ''}`;
    console.warn(`oneshot: no sound: ${why}`);
    bar(`NO SOUND: ${why.toUpperCase()}`);
    // Nothing more goes to a mixer that isn't there.
    clips.length = 0;
    waiting.length = 0;
  };
  // The theme (assets/sound/music/intro.ogg): too long for the worklet's
  // bank, so the browser streams it itself, looping, at the level the game
  // last asked for. Not fetched until it is asked for.
  let tune = null;
  let tuneAt = 0;
  const tunePlay = () => { if (tune && tuneAt > 0 && tune.paused) tune.play().catch(() => {}); };
  let gestured = 0;
  const running = () => ctx && ctx.state === 'running';
  // One-shots since the last gesture, each frame's with when it came, till
  // the mixer is up and the context runs (`heldFlush`: on both).
  const held = [];
  const heldFlush = () => {
    if (!node || !running() || failed) return;
    const now = performance.now();
    for (const [at, shots, n] of held.splice(0)) {
      if (now - at <= HOLD_MS) node.port.postMessage({ kind: 'cmds', bytes: shots }, [shots.buffer]);
      else st.unheard += n;
    }
  };
  // Said once each time it changes, so a console shows what the ear did.
  let said = '';
  const settle = () => {
    if (!ctx || failed) return;
    heldFlush();
    const now = running() ? (node ? 'on' : 'starting') : gestured ? 'held by the browser' : 'waiting for a click';
    if (now === why) return;
    why = now;
    if (why === 'on') {
      bar(null);
      if (said !== 'on') console.info(`oneshot: sound on (${ctx.sampleRate} Hz, ${st.installed} clips)`);
    } else if (why === 'held by the browser') {
      bar('NO SOUND: THE BROWSER IS HOLDING IT. CHECK THIS TAB OR SITE ISN’T MUTED, THEN CLICK THE GAME');
    }
    said = why;
  };
  if (!ctx) fail('no Web Audio in this browser');
  else if (!ctx.audioWorklet) fail('no AudioWorklet here (the page must be on https or localhost)');
  else {
    (async () => {
      await ctx.audioWorklet.addModule(`mix-worklet.js${mv}`);
      const res = await fetch(`mix_worklet.wasm${mv}`);
      if (!res.ok) throw new Error(`mix_worklet.wasm: HTTP ${res.status}`);
      const bytes = await res.arrayBuffer();
      node = new AudioWorkletNode(ctx, 'oneshot-mix', { numberOfInputs: 0, outputChannelCount: [2] });
      node.port.onmessage = (e) => {
        const m = e.data;
        if (m.kind === 'stats') st.mixer = m.s;
        else if (m.kind === 'error') {
          st.errors++;
          if (m.fatal) fail('the mixer did not start', m.why);
          else console.warn('oneshot: sound:', m.why);
        }
      };
      node.onprocessorerror = (e) => fail('the mixer stopped', e?.message ?? 'its processor threw');
      node.port.postMessage({ kind: 'init', bytes }, [bytes]);
      node.connect(ctx.destination);
      for (const [m, t] of clips.splice(0)) node.port.postMessage(m, t);
      for (const [m, t] of waiting.splice(0)) node.port.postMessage(m, t);
      settle();
    })().catch((e) => fail('the mixer did not start', e));
    // A browser starts a context only after a gesture: on any one, and
    // again whenever it was stopped since (a tab hidden, a device gone).
    const wake = () => {
      gestured = performance.now();
      if (ctx.state !== 'running' && ctx.state !== 'closed') ctx.resume().catch(() => {});
      setTimeout(settle, HELD_MS);
    };
    for (const g of GESTURES) addEventListener(g, wake, { capture: true });
    addEventListener('visibilitychange', () => {
      if (document.visibilityState === 'visible' && gestured && ctx.state !== 'running') ctx.resume().catch(() => {});
    });
    ctx.addEventListener('statechange', settle);
  }
  // A browser plays a tune, too, only after a gesture.
  for (const g of GESTURES) addEventListener(g, tunePlay, { capture: true });
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
        const m = { kind: 'install', id, rate: buf.sampleRate, pcm };
        if (failed) return;
        if (node) node.port.postMessage(m, [pcm.buffer]);
        else clips.push([m, [pcm.buffer]]);
        st.installed++;
      }, () => { st.failed++; });
    },
    /** A frame's commands (taken: the game made these bytes for us). A
     * one-shot nobody can hear now is never heard: with no mixer up yet, or
     * the context not running (a browser holds it till a click), it is
     * dropped here, not kept to go off with every other when the sound
     * comes (the first click on the page let out every tick before it, all
     * at once). Only what came after a gesture waits, briefly, for the
     * context that gesture starts. What lasts goes on. */
    send(bytes) {
      st.sent++;
      if (!node || !running()) {
        const [lasting, shots, n] = sieve(bytes);
        if (n && gestured && performance.now() - gestured < HOLD_MS) held.push([performance.now(), shots, n]);
        else st.unheard += n;
        if (lasting.byteLength === 0) return;
        bytes = lasting;
      } else {
        heldFlush();
      }
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
    /** For the browser test: decoded, failed, frames sent, one-shots
     * dropped unheard, the context's state, why it isn't on, worklet
     * errors, and the worklet's gauges
     * (crates/mix Stats: blocks, started, refused, missing, installed,
     * playing, held). */
    stats: () => ({ ...st, state: ctx ? ctx.state : 'none', why }),
  };
}

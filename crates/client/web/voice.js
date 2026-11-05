// Party voice in the page: the microphone through the browser's own Opus
// (WebCodecs) to the lobby session, and each member's frames decoded and
// played a little behind, so a late one still fits (Gates' talk.rs: 60 ms
// of preroll, never more than 300 ms behind). The server relays a frame to
// the party and nobody else; it never decodes one.
//
// Talking is held V (or open mic, past a level gate). A member can be muted
// here, for this browser only. Nothing is recorded.

const RATE = 48000;
const FRAME = 960; // 20 ms
const PREROLL_S = 0.06;
const MAX_LAG_S = 0.3;
const TALKING_MS = 350;
const GATE_DB = -45;
const GATE_HOLD_MS = 400;
const MUTES = 'oneshot.mutes';

const store = (k, v) => {
  try {
    if (v === undefined) return localStorage.getItem(k);
    localStorage.setItem(k, v);
  } catch { /* private window */ }
  return v;
};

export const voiceWorks = () => typeof AudioEncoder === 'function' && typeof AudioDecoder === 'function'
  && !!(navigator.mediaDevices && navigator.mediaDevices.getUserMedia);

export class Voice {
  /** `send(seq, end, bytes)` puts a frame on the lobby; `changed()` is
   * called when who is talking changes. */
  constructor(send, changed) {
    this.send = send;
    this.changed = changed || (() => {});
    this.ctx = null;
    this.out = null;
    this.listeners = new Map(); // tag -> {dec, next, heard, gain}
    this.mutes = new Set(JSON.parse(store(MUTES) || '[]'));
    this.mode = store('oneshot.talk') || 'ptt';
    this.volume = Number(store('oneshot.voiceVol') ?? 80) / 100;
    this.mic = null; // {stream, node, enc}
    this.held = false;
    this.seq = 0;
    this.sentAt = 0;
    this.gateUntil = 0;
    this.ends = [];
    this.heard = 0; // frames played, for the gate test
    this.sent = 0;
    const down = (e) => { if (e.code === 'KeyV' && !e.repeat && !typing(e)) { this.held = true; } };
    const up = (e) => { if (e.code === 'KeyV') this.held = false; };
    // Capture: the game's canvas may stop a key from bubbling.
    addEventListener('keydown', down, true);
    addEventListener('keyup', up, true);
    addEventListener('blur', () => { this.held = false; });
  }

  /** The audio context, made or woken on a gesture (a browser starts one
   * suspended otherwise). */
  async wake() {
    if (!this.ctx) {
      try {
        this.ctx = new AudioContext({ sampleRate: RATE, latencyHint: 'interactive' });
      } catch {
        this.ctx = new AudioContext();
      }
      this.out = this.ctx.createGain();
      this.out.gain.value = this.volume;
      this.out.connect(this.ctx.destination);
    }
    if (this.ctx.state === 'suspended') {
      try { await this.ctx.resume(); } catch { /* still waiting for a gesture */ }
    }
    return this.ctx.state === 'running';
  }

  get asleep() {
    return !this.ctx || this.ctx.state !== 'running';
  }

  setVolume(v) {
    this.volume = v;
    store('oneshot.voiceVol', String(Math.round(v * 100)));
    if (this.out) this.out.gain.value = v;
  }

  setMode(m) {
    this.mode = m === 'open' ? 'open' : 'ptt';
    store('oneshot.talk', this.mode);
  }

  muted(player) {
    return this.mutes.has(player);
  }

  /** Mutes or unmutes a member, by their id (or their name for a guest). */
  toggleMute(player) {
    if (this.mutes.has(player)) this.mutes.delete(player);
    else this.mutes.add(player);
    store(MUTES, JSON.stringify([...this.mutes].slice(-200)));
    this.changed();
  }

  /** Who is talking now: tags. */
  talking() {
    const now = performance.now();
    const t = [];
    for (const [tag, l] of this.listeners) if (now - l.heard < TALKING_MS) t.push(tag);
    return t;
  }

  /** Whether we are on the air now. */
  get onAir() {
    return performance.now() - this.sentAt < TALKING_MS;
  }

  /** The microphone on: asks the browser the first time. */
  async micOn() {
    if (this.mic) return true;
    if (!this.opening) this.opening = this.open().finally(() => { this.opening = null; });
    return this.opening;
  }

  async open() {
    if (!voiceWorks()) throw new Error("this browser can't do voice");
    await this.wake();
    const stream = await navigator.mediaDevices.getUserMedia({
      audio: { echoCancellation: true, noiseSuppression: true, autoGainControl: true, channelCount: 1 },
    });
    await this.ctx.audioWorklet.addModule('voice-worklet.js');
    const src = this.ctx.createMediaStreamSource(stream);
    const node = new AudioWorkletNode(this.ctx, 'oneshot-mic', { processorOptions: { frame: Math.round(this.ctx.sampleRate / 50) } });
    const sink = this.ctx.createGain();
    sink.gain.value = 0;
    src.connect(node).connect(sink).connect(this.ctx.destination);
    const enc = new AudioEncoder({
      output: (chunk) => {
        const bytes = new Uint8Array(chunk.byteLength);
        chunk.copyTo(bytes);
        const end = this.ends.shift() || false;
        if (this.send(this.seq, end, bytes)) this.sent++;
        this.seq = (this.seq + 1) & 0xffff;
      },
      error: () => { /* a frame lost */ },
    });
    const config = { codec: 'opus', sampleRate: this.ctx.sampleRate, numberOfChannels: 1, bitrate: 24000 };
    try {
      enc.configure({ ...config, opus: { frameDuration: 20000, application: 'voip', signal: 'voice' } });
    } catch {
      enc.configure({ ...config, opus: { frameDuration: 20000 } });
    }
    let t = 0;
    let was = false;
    node.port.onmessage = (e) => {
      const pcm = e.data;
      const now = performance.now();
      let want = this.mode === 'open' ? false : this.held;
      if (this.mode === 'open') {
        let s = 0;
        for (let i = 0; i < pcm.length; i++) s += pcm[i] * pcm[i];
        const db = 10 * Math.log10(s / pcm.length + 1e-12);
        if (db > GATE_DB) this.gateUntil = now + GATE_HOLD_MS;
        want = now < this.gateUntil;
      }
      if (!want && !was) return;
      // The last frame of a go at talking: silence, flagged so the light goes out.
      const end = was && !want;
      const data = end ? new Float32Array(pcm.length) : pcm;
      if (enc.state === 'configured') {
        this.ends.push(end);
        enc.encode(new AudioData({ format: 'f32-planar', sampleRate: this.ctx.sampleRate, numberOfFrames: data.length,
          numberOfChannels: 1, timestamp: t, data }));
        t += Math.round((data.length / this.ctx.sampleRate) * 1e6);
      }
      if (want) this.sentAt = now;
      if (want !== was) this.changed();
      was = want;
    };
    this.mic = { stream, node, enc, src };
    store('oneshot.mic', 'on');
    return true;
  }

  micOff() {
    if (!this.mic) return;
    const { stream, node, enc, src } = this.mic;
    for (const tr of stream.getTracks()) tr.stop();
    try { src.disconnect(); node.disconnect(); enc.close(); } catch { /* gone already */ }
    this.mic = null;
    this.ends = [];
    store('oneshot.mic', 'off');
    this.changed();
  }

  /** A member's frame, as the lobby hands it: decoded and played behind
   * their last one. `player` is who they are (for mutes). */
  hear(tag, seq, flags, bytes, player) {
    // Asleep (no gesture yet): dropped, not piled up to play at once later.
    if (this.asleep || this.muted(player)) return;
    let l = this.listeners.get(tag);
    if (!l) {
      const gain = this.ctx.createGain();
      gain.connect(this.out);
      l = { next: 0, heard: 0, gain, dec: null, t: 0 };
      l.dec = new AudioDecoder({
        output: (ad) => this.play(l, ad),
        error: () => { this.listeners.delete(tag); },
      });
      l.dec.configure({ codec: 'opus', sampleRate: RATE, numberOfChannels: 1 });
      this.listeners.set(tag, l);
    }
    const was = performance.now() - l.heard < TALKING_MS;
    l.heard = flags & 1 ? 0 : performance.now();
    if (l.dec.state === 'configured') {
      l.dec.decode(new EncodedAudioChunk({ type: 'key', timestamp: l.t, data: bytes }));
      l.t += 20000;
    }
    if (was !== (l.heard > 0)) this.changed();
  }

  play(l, ad) {
    const n = ad.numberOfFrames;
    const buf = this.ctx.createBuffer(1, n, ad.sampleRate);
    const pcm = new Float32Array(n);
    try {
      ad.copyTo(pcm, { planeIndex: 0, format: 'f32-planar' });
    } catch {
      // Mono f32 is one plane however it is labelled.
      try { ad.copyTo(pcm, { planeIndex: 0 }); } catch { ad.close(); return; }
    }
    ad.close();
    buf.copyToChannel(pcm, 0);
    const now = this.ctx.currentTime;
    // Behind by more than the cap, or caught up: start again at the preroll.
    if (l.next < now || l.next - now > MAX_LAG_S) l.next = now + PREROLL_S;
    const s = this.ctx.createBufferSource();
    s.buffer = buf;
    s.connect(l.gain);
    s.start(l.next);
    l.next += n / ad.sampleRate;
    this.heard++;
  }

  /** A member left the party: their decoder goes. */
  forget(tags) {
    for (const [tag, l] of this.listeners) {
      if (tags.includes(tag)) continue;
      try { l.dec.close(); l.gain.disconnect(); } catch { /* gone already */ }
      this.listeners.delete(tag);
    }
  }
}

function typing(e) {
  const t = e.target;
  return t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA') && t.type !== 'checkbox' && t.type !== 'range';
}

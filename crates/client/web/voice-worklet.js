// The microphone's tap (voice.js): mono, in 20 ms frames at the context's
// rate (48 kHz: 960 samples), posted to the page by transfer. After Gates'
// GatesMic. Its output stays silent; it is connected so engines pull it.
class OneshotMic extends AudioWorkletProcessor {
  constructor(options) {
    super();
    this.n = (options && options.processorOptions && options.processorOptions.frame) || 960;
    this.buf = new Float32Array(this.n);
    this.at = 0;
  }

  process(inputs) {
    const ch = inputs[0];
    if (ch && ch.length) {
      const a = ch[0];
      const b = ch[1];
      for (let i = 0; i < a.length; i++) {
        this.buf[this.at++] = b ? (a[i] + b[i]) * 0.5 : a[i];
        if (this.at === this.n) {
          this.port.postMessage(this.buf, [this.buf.buffer]);
          this.buf = new Float32Array(this.n);
          this.at = 0;
        }
      }
    }
    return true;
  }
}
registerProcessor('oneshot-mic', OneshotMic);

//! The mixer: one source Bevy plays, every voice summed inside it (after
//! Gates' `sound::engine`, trimmed). A sound per Bevy player was a decoder
//! and a sink per shot, mixed in cpal's callback, which in a tab is the
//! main thread; and it left us rodio's spatial law (inverse square, the far
//! ear never under half). Here the game decides each voice's two ear gains,
//! its pitch (Doppler, an engine's revs) and its low-pass (distance, a wall
//! in the way); this only turns that into samples.
//!
//! One-shots play a decoded clip once, after an optional delay (sound's
//! travel). Held voices loop in numbered slots the game moves every frame;
//! their gains, pitch and filter ramp across a block, so a moving source
//! never clicks. Past [`VOICES`] the quietest one-shot gives way to a louder
//! one, or the new one is dropped. A peak limiter sits last.
//!
//! Pure: no clock, no I/O; `render` is what the audio thread runs, and the
//! tests below drive it with no device.

use std::sync::{Arc, Mutex};

/// The mixer's rate; rodio converts to the device's.
pub const RATE: u32 = 48_000;
/// Frames rendered at a time.
pub const BLOCK: usize = 256;
/// One-shots at once.
pub const VOICES: usize = 40;
/// Held (looping) slots.
pub const LOOPS: usize = 32;
/// Commands waiting past this are dropped (the audio thread stalled).
const INBOX: usize = 512;
/// The limiter's ceiling, and how much of the way back to unity its gain
/// comes each block once the peak has passed (~0.25 s).
const CEILING: f32 = 0.9;
const RELEASE: f32 = 0.02;
/// A 16-bit sample to ±1.
const PCM: f32 = 1.0 / 32768.0;

/// A decoded sound: mono 16-bit samples at their own rate.
#[derive(Clone)]
pub struct Clip {
    pub pcm: Arc<[i16]>,
    pub rate: u32,
}

impl Clip {
    pub fn secs(&self) -> f32 {
        self.pcm.len() as f32 / self.rate.max(1) as f32
    }
}

/// Where a voice sits: its gain in each ear, its playback rate (1 as
/// recorded), and its one-pole low-pass coefficient (1 open, toward 0
/// darker; [`lp_coeff`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Place {
    pub l: f32,
    pub r: f32,
    pub rate: f32,
    pub lp: f32,
}

impl Place {
    /// Centred, unfiltered, at `gain` (√½ an ear for 1, as a centred
    /// positional sound has).
    pub fn centre(gain: f32) -> Place {
        let g = gain * core::f32::consts::FRAC_1_SQRT_2;
        Place {
            l: g,
            r: g,
            rate: 1.0,
            lp: 1.0,
        }
    }
}

/// The one-pole coefficient for a corner at `hz`.
pub fn lp_coeff(hz: f32) -> f32 {
    let a = 1.0 - (-core::f32::consts::TAU * hz / RATE as f32).exp();
    a.clamp(0.01, 1.0)
}

pub enum Cmd {
    /// A one-shot, `delay` frames from now.
    Play { clip: Clip, at: Place, delay: u32 },
    /// Start `clip` looping in `slot` (in from silence over a block).
    Hold { slot: u8, clip: Clip, at: Place },
    /// A held slot's new place.
    Move { slot: u8, at: Place },
    /// Fade a held slot out over a block and free it.
    Stop { slot: u8 },
    /// Everything's gain (the player's volume).
    Master(f32),
}

/// What the game thread and the audio thread share: the inbox.
#[derive(Default)]
pub struct Shared {
    inbox: Mutex<Vec<Cmd>>,
}

impl Shared {
    /// Hand over a frame's commands (emptying `cmds`).
    pub fn send(&self, cmds: &mut Vec<Cmd>) {
        if cmds.is_empty() {
            return;
        }
        if let Ok(mut q) = self.inbox.lock() {
            let room = INBOX.saturating_sub(q.len());
            if cmds.len() > room {
                cmds.truncate(room);
            }
            q.append(cmds);
        }
        cmds.clear();
    }
}

#[derive(Clone, Default)]
struct Voice {
    pcm: Option<Arc<[i16]>>,
    pos: usize,
    frac: f32,
    step: f32,
    l: f32,
    r: f32,
    lp: f32,
    y: f32,
    wait: u32,
}

#[derive(Clone, Default)]
struct Held {
    pcm: Option<Arc<[i16]>>,
    src_rate: f32,
    pos: usize,
    frac: f32,
    now: [f32; 4],
    to: [f32; 4],
    y: f32,
    stopping: bool,
}

/// The renderer: what the audio thread owns.
pub struct Engine {
    voices: Vec<Voice>,
    held: Vec<Held>,
    master: f32,
    limit: f32,
    taken: Vec<Cmd>,
}

impl Default for Engine {
    fn default() -> Self {
        Engine {
            voices: vec![Voice::default(); VOICES],
            held: vec![Held::default(); LOOPS],
            master: 1.0,
            limit: 1.0,
            taken: Vec::with_capacity(INBOX),
        }
    }
}

fn step_of(src_rate: f32, rate: f32) -> f32 {
    (src_rate / RATE as f32 * rate).clamp(0.05, 8.0)
}

impl Engine {
    /// Take what the game sent, if the inbox is free this block.
    pub fn take(&mut self, shared: &Shared) {
        if let Ok(mut q) = shared.inbox.try_lock() {
            core::mem::swap(&mut *q, &mut self.taken);
        }
        let mut cmds = core::mem::take(&mut self.taken);
        for c in cmds.drain(..) {
            self.apply(c);
        }
        self.taken = cmds;
    }

    pub fn apply(&mut self, c: Cmd) {
        match c {
            Cmd::Play { clip, at, delay } => {
                let loud = at.l + at.r;
                let slot = match self.voices.iter().position(|v| v.pcm.is_none()) {
                    Some(i) => i,
                    None => {
                        let (i, q) = self
                            .voices
                            .iter()
                            .enumerate()
                            .map(|(i, v)| (i, v.l + v.r))
                            .fold((0, f32::MAX), |a, b| if b.1 < a.1 { b } else { a });
                        if q >= loud {
                            return;
                        }
                        i
                    }
                };
                self.voices[slot] = Voice {
                    step: step_of(clip.rate as f32, at.rate),
                    pcm: Some(clip.pcm),
                    pos: 0,
                    frac: 0.0,
                    l: at.l,
                    r: at.r,
                    lp: at.lp,
                    y: 0.0,
                    wait: delay,
                };
            }
            Cmd::Hold { slot, clip, at } => {
                if let Some(h) = self.held.get_mut(slot as usize) {
                    let to = [at.l, at.r, step_of(clip.rate as f32, at.rate), at.lp];
                    *h = Held {
                        src_rate: clip.rate as f32,
                        pcm: Some(clip.pcm),
                        pos: 0,
                        frac: 0.0,
                        now: [0.0, 0.0, to[2], to[3]],
                        to,
                        y: 0.0,
                        stopping: false,
                    };
                }
            }
            Cmd::Move { slot, at } => {
                if let Some(h) = self.held.get_mut(slot as usize) {
                    if h.pcm.is_some() && !h.stopping {
                        h.to = [at.l, at.r, step_of(h.src_rate, at.rate), at.lp];
                    }
                }
            }
            Cmd::Stop { slot } => {
                if let Some(h) = self.held.get_mut(slot as usize) {
                    h.to[0] = 0.0;
                    h.to[1] = 0.0;
                    h.stopping = true;
                }
            }
            Cmd::Master(g) => self.master = g.clamp(0.0, 2.0),
        }
    }

    /// One block of interleaved stereo.
    pub fn render(&mut self, out: &mut [f32; BLOCK * 2]) {
        out.fill(0.0);
        for v in &mut self.voices {
            let Some(pcm) = v.pcm.as_ref() else { continue };
            let n = pcm.len();
            let mut done = n < 2;
            for i in 0..BLOCK {
                if done {
                    break;
                }
                if v.wait > 0 {
                    v.wait -= 1;
                    continue;
                }
                let a = pcm[v.pos] as f32 * PCM;
                let b = pcm[v.pos + 1] as f32 * PCM;
                let x = a + (b - a) * v.frac;
                v.y += v.lp * (x - v.y);
                out[2 * i] += v.y * v.l;
                out[2 * i + 1] += v.y * v.r;
                v.frac += v.step;
                let whole = v.frac as usize;
                v.pos += whole;
                v.frac -= whole as f32;
                if v.pos + 1 >= n {
                    done = true;
                }
            }
            if done {
                v.pcm = None;
            }
        }
        let inv = 1.0 / BLOCK as f32;
        for h in &mut self.held {
            let Some(pcm) = h.pcm.as_ref() else { continue };
            let n = pcm.len();
            if n < 2 {
                h.pcm = None;
                continue;
            }
            let d = [
                (h.to[0] - h.now[0]) * inv,
                (h.to[1] - h.now[1]) * inv,
                (h.to[2] - h.now[2]) * inv,
                (h.to[3] - h.now[3]) * inv,
            ];
            for i in 0..BLOCK {
                for (now, d) in h.now.iter_mut().zip(d) {
                    *now += d;
                }
                let a = pcm[h.pos] as f32 * PCM;
                let b = pcm[(h.pos + 1) % n] as f32 * PCM;
                let x = a + (b - a) * h.frac;
                h.y += h.now[3] * (x - h.y);
                out[2 * i] += h.y * h.now[0];
                out[2 * i + 1] += h.y * h.now[1];
                h.frac += h.now[2];
                let whole = h.frac as usize;
                h.pos = (h.pos + whole) % n;
                h.frac -= whole as f32;
            }
            h.now = h.to;
            if h.stopping {
                h.pcm = None;
            }
        }
        // The limiter: a peak over the ceiling brings the gain down; it
        // climbs back slowly.
        let peak = out.iter().fold(0.0f32, |m, s| m.max(s.abs())) * self.master;
        let want = if peak > CEILING { CEILING / peak } else { 1.0 };
        // Down at once (the onset that needs it masks the step); back up
        // eased across the block.
        let to = if want < self.limit {
            want
        } else {
            self.limit + (1.0 - self.limit) * RELEASE
        };
        let from = self.limit.min(to);
        for i in 0..BLOCK {
            let g = self.master * (from + (to - from) * (i + 1) as f32 * inv);
            out[2 * i] = (out[2 * i] * g).clamp(-1.0, 1.0);
            out[2 * i + 1] = (out[2 * i + 1] * g).clamp(-1.0, 1.0);
        }
        self.limit = to;
    }

    /// One-shots playing (for the tests).
    pub fn playing(&self) -> usize {
        self.voices.iter().filter(|v| v.pcm.is_some()).count()
    }
}

/// The stream Bevy plays: the engine, a block at a time.
pub struct Stream {
    shared: Arc<Shared>,
    engine: Engine,
    buf: [f32; BLOCK * 2],
    at: usize,
}

impl Stream {
    pub fn new(shared: Arc<Shared>) -> Stream {
        Stream {
            shared,
            engine: Engine::default(),
            buf: [0.0; BLOCK * 2],
            at: BLOCK * 2,
        }
    }
}

impl Iterator for Stream {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if self.at >= BLOCK * 2 {
            self.engine.take(&self.shared);
            self.engine.render(&mut self.buf);
            self.at = 0;
        }
        let s = self.buf[self.at];
        self.at += 1;
        Some(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(n: usize) -> Clip {
        let pcm: Vec<i16> = (0..n).map(|i| ((i as f32 * 0.05).sin() * 32767.0) as i16).collect();
        Clip {
            pcm: pcm.into(),
            rate: RATE,
        }
    }

    fn energy(b: &[f32; BLOCK * 2], ch: usize) -> f32 {
        b.iter().skip(ch).step_by(2).map(|s| s * s).sum()
    }

    #[test]
    fn a_one_shot_plays_once_then_frees_its_voice() {
        let mut e = Engine::default();
        e.apply(Cmd::Play {
            clip: tone(BLOCK + 10),
            at: Place::centre(1.0),
            delay: 0,
        });
        let mut b = [0.0; BLOCK * 2];
        e.render(&mut b);
        assert!(energy(&b, 0) > 1.0);
        assert_eq!(e.playing(), 1);
        e.render(&mut b);
        assert_eq!(e.playing(), 0);
        e.render(&mut b);
        assert_eq!(energy(&b, 0), 0.0);
    }

    #[test]
    fn pan_puts_a_voice_in_one_ear() {
        let mut e = Engine::default();
        e.apply(Cmd::Play {
            clip: tone(4 * BLOCK),
            at: Place {
                l: 0.0,
                r: 0.8,
                rate: 1.0,
                lp: 1.0,
            },
            delay: 0,
        });
        let mut b = [0.0; BLOCK * 2];
        e.render(&mut b);
        assert_eq!(energy(&b, 0), 0.0);
        assert!(energy(&b, 1) > 1.0);
    }

    #[test]
    fn a_delayed_voice_waits() {
        let mut e = Engine::default();
        e.apply(Cmd::Play {
            clip: tone(4 * BLOCK),
            at: Place::centre(1.0),
            delay: BLOCK as u32,
        });
        let mut b = [0.0; BLOCK * 2];
        e.render(&mut b);
        assert_eq!(energy(&b, 0), 0.0);
        e.render(&mut b);
        assert!(energy(&b, 0) > 1.0);
    }

    #[test]
    fn a_full_pool_gives_way_only_to_a_louder_voice() {
        let mut e = Engine::default();
        for _ in 0..VOICES {
            e.apply(Cmd::Play {
                clip: tone(8 * BLOCK),
                at: Place::centre(0.2),
                delay: 0,
            });
        }
        assert_eq!(e.playing(), VOICES);
        e.apply(Cmd::Play {
            clip: tone(8 * BLOCK),
            at: Place::centre(0.1),
            delay: 0,
        });
        assert!(e
            .voices
            .iter()
            .all(|v| (v.l - 0.2 * core::f32::consts::FRAC_1_SQRT_2).abs() < 1e-6));
        e.apply(Cmd::Play {
            clip: tone(8 * BLOCK),
            at: Place::centre(0.9),
            delay: 0,
        });
        assert_eq!(e.voices.iter().filter(|v| v.l > 0.5).count(), 1);
    }

    #[test]
    fn a_held_loop_fades_in_wraps_and_stops() {
        let mut e = Engine::default();
        e.apply(Cmd::Hold {
            slot: 3,
            clip: tone(100),
            at: Place::centre(1.0),
        });
        let mut b = [0.0; BLOCK * 2];
        e.render(&mut b);
        // From silence: the first sample is all but nothing.
        assert!(b[0].abs() < 0.01);
        e.render(&mut b);
        assert!(energy(&b, 0) > 1.0);
        e.apply(Cmd::Stop { slot: 3 });
        e.render(&mut b);
        e.render(&mut b);
        assert_eq!(energy(&b, 0), 0.0);
        assert!(e.held[3].pcm.is_none());
    }

    #[test]
    fn the_limiter_keeps_a_pile_up_inside_full_scale() {
        let mut e = Engine::default();
        for _ in 0..30 {
            e.apply(Cmd::Play {
                clip: tone(8 * BLOCK),
                at: Place::centre(1.4),
                delay: 0,
            });
        }
        let mut b = [0.0; BLOCK * 2];
        for _ in 0..3 {
            e.render(&mut b);
        }
        assert!(b.iter().all(|s| s.abs() <= CEILING + 1e-4));
    }

    #[test]
    fn the_stream_takes_the_inbox() {
        let shared = Arc::new(Shared::default());
        let mut s = Stream::new(shared.clone());
        let mut cmds = vec![Cmd::Play {
            clip: tone(4 * BLOCK),
            at: Place::centre(1.0),
            delay: 0,
        }];
        shared.send(&mut cmds);
        assert!(cmds.is_empty());
        let e: f32 = (&mut s).take(BLOCK * 2).map(|x| x * x).sum();
        assert!(e > 1.0);
    }
}

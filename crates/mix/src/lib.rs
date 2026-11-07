//! The game's mixer (after Gates' `sound::engine`, trimmed): every voice
//! summed in one renderer, the game deciding each voice's two ear gains, its
//! pitch (Doppler, an engine's revs) and its low-pass (distance, a wall in
//! the way). This only turns those decisions into samples.
//!
//! It runs where the platform's audio thread is: inside a Bevy audio source
//! on the desktop (the client's `sound::on`, through [`Shared`] and
//! [`Stream`]), and in the browser inside an `AudioWorkletProcessor`
//! (`mix-worklet`), where commands arrive as bytes ([`encode`],
//! [`Engine::apply_bytes`]) and decoded clips by [`Cmd::Install`]. A sound
//! per Bevy player was a decoder and a sink per shot; in a tab its output
//! ran on a timer on the page's own thread.
//!
//! One-shots play an installed clip once, after an optional delay (sound's
//! travel). Held voices loop in numbered slots the game moves every frame;
//! their gains, pitch and filter ramp across a block, so a moving source
//! never clicks. Past [`VOICES`] the quietest one-shot gives way to a louder
//! one, or the new one is dropped. A peak limiter sits last.
//!
//! Pure: no clock, no I/O, no allocation in [`Engine::render`].

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};

/// The mixer's rate: the browser's context is made at it, and the desktop's
/// stream says it (rodio converts to the device's).
pub const RATE: u32 = 48_000;
/// Frames rendered at a time: an AudioWorklet's render quantum.
pub const BLOCK: usize = 128;
/// One-shots at once.
pub const VOICES: usize = 40;
/// Held (looping) slots.
pub const LOOPS: usize = 32;
/// Clips the bank holds, by id.
pub const CLIPS: usize = 512;
/// Commands waiting past this are dropped or held back (the audio thread
/// stalled: [`Shared::send`]).
const INBOX: usize = 512;
/// The limiter's ceiling, and how much of the way back to unity its gain
/// comes each block once the peak has passed (~0.25 s).
const CEILING: f32 = 0.9;
const RELEASE: f32 = 0.01;
/// A 16-bit sample to ±1.
const PCM: f32 = 1.0 / 32768.0;

/// A decoded sound: mono 16-bit samples at their own rate.
#[derive(Clone)]
pub struct Clip {
    pub pcm: Arc<[i16]>,
    pub rate: u32,
}

impl Clip {
    /// From floats in ±1 (the browser's decoder gives these).
    pub fn from_f32(pcm: &[f32], rate: u32) -> Clip {
        Clip::from_samples(pcm.iter().copied(), rate)
    }

    /// From floats in ±1 as they come, straight into the clip's one
    /// allocation (the worklet reads them off its byte buffer).
    pub fn from_samples(pcm: impl ExactSizeIterator<Item = f32>, rate: u32) -> Clip {
        let pcm: Arc<[i16]> = pcm.map(|s| (s * 32767.0).clamp(-32768.0, 32767.0) as i16).collect();
        Clip { pcm, rate }
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
    /// Put a decoded clip in the bank as `id`.
    Install { id: u16, clip: Clip },
    /// Clip `clip` once, `delay` frames from now.
    Play { clip: u16, at: Place, delay: u32 },
    /// Start clip `clip` looping in `slot` (in from silence over a block).
    Hold { slot: u8, clip: u16, at: Place },
    /// A held slot's new place.
    Move { slot: u8, at: Place },
    /// Fade a held slot out over a block and free it.
    Stop { slot: u8 },
    /// Everything's gain (the player's volume).
    Master(f32),
}

const T_PLAY: u8 = 1;
const T_HOLD: u8 = 2;
const T_MOVE: u8 = 3;
const T_STOP: u8 = 4;
const T_MASTER: u8 = 5;

fn put_place(out: &mut Vec<u8>, at: &Place) {
    for v in [at.l, at.r, at.rate, at.lp] {
        out.extend_from_slice(&v.to_le_bytes());
    }
}

/// A frame's commands as bytes, for an audio thread across a message port.
/// [`Cmd::Install`] is not among them: a clip goes on its own.
pub fn encode(cmds: &[Cmd], out: &mut Vec<u8>) {
    for c in cmds {
        match c {
            Cmd::Install { .. } => {}
            Cmd::Play { clip, at, delay } => {
                out.push(T_PLAY);
                out.extend_from_slice(&clip.to_le_bytes());
                put_place(out, at);
                out.extend_from_slice(&delay.to_le_bytes());
            }
            Cmd::Hold { slot, clip, at } => {
                out.push(T_HOLD);
                out.push(*slot);
                out.extend_from_slice(&clip.to_le_bytes());
                put_place(out, at);
            }
            Cmd::Move { slot, at } => {
                out.push(T_MOVE);
                out.push(*slot);
                put_place(out, at);
            }
            Cmd::Stop { slot } => {
                out.push(T_STOP);
                out.push(*slot);
            }
            Cmd::Master(g) => {
                out.push(T_MASTER);
                out.extend_from_slice(&g.to_le_bytes());
            }
        }
    }
}

/// Reads [`encode`]'s bytes back, one at a time.
struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn take<const N: usize>(&mut self) -> Option<[u8; N]> {
        let s = self.b.get(self.at..self.at + N)?;
        self.at += N;
        s.try_into().ok()
    }
    fn u8(&mut self) -> Option<u8> {
        self.take::<1>().map(|b| b[0])
    }
    fn u16(&mut self) -> Option<u16> {
        self.take().map(u16::from_le_bytes)
    }
    fn u32(&mut self) -> Option<u32> {
        self.take().map(u32::from_le_bytes)
    }
    fn f32(&mut self) -> Option<f32> {
        self.take().map(f32::from_le_bytes)
    }
    fn place(&mut self) -> Option<Place> {
        Some(Place {
            l: self.f32()?,
            r: self.f32()?,
            rate: self.f32()?,
            lp: self.f32()?,
        })
    }
    fn cmd(&mut self) -> Option<Cmd> {
        Some(match self.u8()? {
            T_PLAY => Cmd::Play {
                clip: self.u16()?,
                at: self.place()?,
                delay: self.u32()?,
            },
            T_HOLD => Cmd::Hold {
                slot: self.u8()?,
                clip: self.u16()?,
                at: self.place()?,
            },
            T_MOVE => Cmd::Move {
                slot: self.u8()?,
                at: self.place()?,
            },
            T_STOP => Cmd::Stop { slot: self.u8()? },
            T_MASTER => Cmd::Master(self.f32()?),
            _ => return None,
        })
    }
}

/// What the game thread and the desktop's audio thread share: the inbox, a
/// bounded channel (std's array channel: no lock and no allocation a
/// command, either side, while neither waits on it). Full (the audio thread
/// stalled), a frame's momentary commands (a one-shot, a move) are dropped
/// and counted; what lasts (a clip installed, a loop started or stopped, the
/// volume) is held back, the latest for each, and goes ahead of the next
/// frame's.
pub struct Shared {
    tx: SyncSender<Cmd>,
    /// The audio thread's end, till the one stream takes it.
    rx: Mutex<Option<Receiver<Cmd>>>,
    /// Held back from a full inbox, in order: the game thread's alone.
    later: Mutex<Vec<Cmd>>,
    /// Commands dropped at a full inbox.
    dropped: AtomicU32,
}

impl Default for Shared {
    fn default() -> Self {
        let (tx, rx) = sync_channel(INBOX);
        Shared {
            tx,
            rx: Mutex::new(Some(rx)),
            later: Mutex::new(Vec::new()),
            dropped: AtomicU32::new(0),
        }
    }
}

/// Whether `a` says what `b` says about the same thing, so only the later
/// need be held: one clip id, one slot, the volume.
fn same_target(a: &Cmd, b: &Cmd) -> bool {
    let slot = |c: &Cmd| match *c {
        Cmd::Hold { slot, .. } | Cmd::Stop { slot } => Some(slot),
        _ => None,
    };
    match (a, b) {
        (Cmd::Install { id: x, .. }, Cmd::Install { id: y, .. }) => x == y,
        (Cmd::Master(_), Cmd::Master(_)) => true,
        _ => slot(a).is_some() && slot(a) == slot(b),
    }
}

impl Shared {
    /// Hand over a frame's commands (emptying `cmds`), after whatever a
    /// full inbox held back before.
    pub fn send(&self, cmds: &mut Vec<Cmd>) {
        let mut later = self.later.lock().unwrap_or_else(|e| e.into_inner());
        let mut full = false;
        for c in later.drain(..).collect::<Vec<_>>().into_iter().chain(cmds.drain(..)) {
            let c = if full {
                c
            } else {
                match self.tx.try_send(c) {
                    Ok(()) => continue,
                    Err(TrySendError::Full(c)) => {
                        full = true;
                        c
                    }
                    // No audio thread any more: nothing will play.
                    Err(TrySendError::Disconnected(_)) => {
                        self.dropped.fetch_add(1, Ordering::Relaxed);
                        continue;
                    }
                }
            };
            match c {
                Cmd::Play { .. } | Cmd::Move { .. } => {
                    self.dropped.fetch_add(1, Ordering::Relaxed);
                }
                c => {
                    later.retain(|o| !same_target(o, &c));
                    later.push(c);
                }
            }
        }
    }

    /// Commands dropped at a full inbox so far (one-shots and moves; what
    /// lasts is held back, never dropped).
    pub fn dropped(&self) -> u32 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// The audio thread's end of the inbox: the first stream's (one plays
    /// the engine), taken once when it is made.
    fn inbox(&self) -> Option<Inbox> {
        self.rx.lock().ok()?.take().map(Inbox)
    }
}

/// The audio thread's end of [`Shared`]'s inbox.
pub struct Inbox(Receiver<Cmd>);

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
    /// Asked for before its clip was installed (a browser decodes them as
    /// they come): the clip and where it is wanted, until it lands.
    waiting: Option<(u16, Place)>,
}

/// What the renderer has done, for a page's gauges: blocks rendered,
/// one-shots started, one-shots refused (the pool full of louder ones),
/// commands that named a clip not yet installed, clips installed, and the
/// one-shots and held voices playing now.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub blocks: u32,
    pub started: u32,
    pub refused: u32,
    pub missing: u32,
    pub installed: u32,
    pub playing: u32,
    pub held: u32,
}

/// The renderer: what the audio thread owns.
pub struct Engine {
    bank: Vec<Option<Clip>>,
    voices: Vec<Voice>,
    held: Vec<Held>,
    master: f32,
    limit: f32,
    stats: Stats,
}

impl Default for Engine {
    fn default() -> Self {
        Engine {
            bank: vec![None; CLIPS],
            voices: vec![Voice::default(); VOICES],
            held: vec![Held::default(); LOOPS],
            master: 1.0,
            limit: 1.0,
            stats: Stats::default(),
        }
    }
}

fn step_of(src_rate: f32, rate: f32) -> f32 {
    (src_rate / RATE as f32 * rate).clamp(0.05, 8.0)
}

impl Engine {
    /// Take what the game has sent, never waiting for it.
    pub fn take(&mut self, inbox: &Inbox) {
        for _ in 0..INBOX {
            let Ok(c) = inbox.0.try_recv() else { break };
            self.apply(c);
        }
    }

    /// Apply [`encode`]'s bytes; a malformed tail is dropped.
    pub fn apply_bytes(&mut self, bytes: &[u8]) {
        let mut r = Reader { b: bytes, at: 0 };
        while let Some(c) = r.cmd() {
            self.apply(c);
        }
    }

    fn clip(&mut self, id: u16) -> Option<Clip> {
        let c = self.bank.get(id as usize).and_then(|c| c.clone());
        if c.is_none() {
            self.stats.missing += 1;
        }
        c
    }

    pub fn apply(&mut self, c: Cmd) {
        match c {
            Cmd::Install { id, clip } => {
                if let Some(b) = self.bank.get_mut(id as usize) {
                    if b.is_none() {
                        self.stats.installed += 1;
                    }
                    *b = Some(clip);
                }
                // A loop asked for before its clip came starts now.
                for slot in 0..self.held.len() {
                    if let Some((want, at)) = self.held[slot].waiting {
                        if want == id {
                            self.apply(Cmd::Hold {
                                slot: slot as u8,
                                clip: id,
                                at,
                            });
                        }
                    }
                }
            }
            Cmd::Play { clip, at, delay } => {
                let Some(clip) = self.clip(clip) else { return };
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
                            self.stats.refused += 1;
                            return;
                        }
                        i
                    }
                };
                self.stats.started += 1;
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
            Cmd::Hold { slot, clip: id, at } => {
                let Some(clip) = self.clip(id) else {
                    if let Some(h) = self.held.get_mut(slot as usize) {
                        *h = Held {
                            waiting: Some((id, at)),
                            ..Held::default()
                        };
                    }
                    return;
                };
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
                        waiting: None,
                    };
                }
            }
            Cmd::Move { slot, at } => {
                if let Some(h) = self.held.get_mut(slot as usize) {
                    if let Some((_, want)) = &mut h.waiting {
                        *want = at;
                    } else if h.pcm.is_some() && !h.stopping {
                        h.to = [at.l, at.r, step_of(h.src_rate, at.rate), at.lp];
                    }
                }
            }
            Cmd::Stop { slot } => {
                if let Some(h) = self.held.get_mut(slot as usize) {
                    h.to[0] = 0.0;
                    h.to[1] = 0.0;
                    h.stopping = true;
                    h.waiting = None;
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
                // The loop's wrap without a division a sample.
                let next = if h.pos + 1 == n { 0 } else { h.pos + 1 };
                let b = pcm[next] as f32 * PCM;
                let x = a + (b - a) * h.frac;
                h.y += h.now[3] * (x - h.y);
                out[2 * i] += h.y * h.now[0];
                out[2 * i + 1] += h.y * h.now[1];
                h.frac += h.now[2];
                let whole = h.frac as usize;
                h.pos += whole;
                if h.pos >= n {
                    h.pos %= n;
                }
                h.frac -= whole as f32;
            }
            h.now = h.to;
            if h.stopping {
                h.pcm = None;
            }
        }
        // The limiter: a peak over the ceiling brings the gain down at once
        // (the onset that needs it masks the step); it climbs back eased.
        let peak = out.iter().fold(0.0f32, |m, s| m.max(s.abs())) * self.master;
        let want = if peak > CEILING { CEILING / peak } else { 1.0 };
        let to = want.min(self.limit + (1.0 - self.limit) * RELEASE);
        let from = self.limit.min(to);
        for i in 0..BLOCK {
            let g = self.master * (from + (to - from) * (i + 1) as f32 * inv);
            out[2 * i] = (out[2 * i] * g).clamp(-1.0, 1.0);
            out[2 * i + 1] = (out[2 * i + 1] * g).clamp(-1.0, 1.0);
        }
        self.limit = to;
        self.stats.blocks = self.stats.blocks.wrapping_add(1);
    }

    pub fn stats(&self) -> Stats {
        Stats {
            playing: self.voices.iter().filter(|v| v.pcm.is_some()).count() as u32,
            held: self.held.iter().filter(|h| h.pcm.is_some()).count() as u32,
            ..self.stats
        }
    }
}

/// The desktop's stream: the engine, a block at a time, as samples.
pub struct Stream {
    inbox: Option<Inbox>,
    engine: Engine,
    buf: [f32; BLOCK * 2],
    at: usize,
}

impl Stream {
    pub fn new(shared: Arc<Shared>) -> Stream {
        Stream {
            inbox: shared.inbox(),
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
            if let Some(inbox) = &self.inbox {
                self.engine.take(inbox);
            }
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

    const TONE: u16 = 7;

    fn tone(n: usize) -> Clip {
        let pcm: Vec<i16> = (0..n).map(|i| ((i as f32 * 0.05).sin() * 32767.0) as i16).collect();
        Clip {
            pcm: pcm.into(),
            rate: RATE,
        }
    }

    fn with_tone(n: usize) -> Engine {
        let mut e = Engine::default();
        e.apply(Cmd::Install {
            id: TONE,
            clip: tone(n),
        });
        e
    }

    fn play(at: Place, delay: u32) -> Cmd {
        Cmd::Play { clip: TONE, at, delay }
    }

    fn energy(b: &[f32; BLOCK * 2], ch: usize) -> f32 {
        b.iter().skip(ch).step_by(2).map(|s| s * s).sum()
    }

    #[test]
    fn a_one_shot_plays_once_then_frees_its_voice() {
        let mut e = with_tone(BLOCK + 10);
        e.apply(play(Place::centre(1.0), 0));
        let mut b = [0.0; BLOCK * 2];
        e.render(&mut b);
        assert!(energy(&b, 0) > 1.0);
        assert_eq!(e.stats().playing, 1);
        e.render(&mut b);
        assert_eq!(e.stats().playing, 0);
        e.render(&mut b);
        assert_eq!(energy(&b, 0), 0.0);
    }

    #[test]
    fn a_clip_not_installed_is_counted_not_played() {
        let mut e = Engine::default();
        e.apply(play(Place::centre(1.0), 0));
        assert_eq!(e.stats().playing, 0);
        assert_eq!(e.stats().missing, 1);
    }

    #[test]
    fn pan_puts_a_voice_in_one_ear() {
        let mut e = with_tone(4 * BLOCK);
        e.apply(play(
            Place {
                l: 0.0,
                r: 0.8,
                rate: 1.0,
                lp: 1.0,
            },
            0,
        ));
        let mut b = [0.0; BLOCK * 2];
        e.render(&mut b);
        assert_eq!(energy(&b, 0), 0.0);
        assert!(energy(&b, 1) > 1.0);
    }

    #[test]
    fn a_delayed_voice_waits() {
        let mut e = with_tone(4 * BLOCK);
        e.apply(play(Place::centre(1.0), BLOCK as u32));
        let mut b = [0.0; BLOCK * 2];
        e.render(&mut b);
        assert_eq!(energy(&b, 0), 0.0);
        e.render(&mut b);
        assert!(energy(&b, 0) > 1.0);
    }

    #[test]
    fn a_full_pool_gives_way_only_to_a_louder_voice() {
        let mut e = with_tone(8 * BLOCK);
        for _ in 0..VOICES {
            e.apply(play(Place::centre(0.2), 0));
        }
        assert_eq!(e.stats().playing as usize, VOICES);
        e.apply(play(Place::centre(0.1), 0));
        assert_eq!(e.stats().refused, 1);
        e.apply(play(Place::centre(0.9), 0));
        assert_eq!(e.voices.iter().filter(|v| v.l > 0.5).count(), 1);
    }

    #[test]
    fn a_held_loop_fades_in_wraps_and_stops() {
        let mut e = with_tone(100);
        e.apply(Cmd::Hold {
            slot: 3,
            clip: TONE,
            at: Place::centre(1.0),
        });
        let mut b = [0.0; BLOCK * 2];
        e.render(&mut b);
        // From silence: the first sample is all but nothing.
        assert!(b[0].abs() < 0.02);
        e.render(&mut b);
        assert!(energy(&b, 0) > 1.0);
        e.apply(Cmd::Stop { slot: 3 });
        e.render(&mut b);
        e.render(&mut b);
        assert_eq!(energy(&b, 0), 0.0);
        assert_eq!(e.stats().held, 0);
    }

    /// In a browser a clip lands when the browser has decoded it: a loop
    /// asked for (and moved) before then starts the moment it does, where
    /// it was last wanted; one stopped meanwhile never starts.
    #[test]
    fn a_loop_asked_for_before_its_clip_starts_when_it_lands() {
        let mut e = Engine::default();
        e.apply(Cmd::Hold {
            slot: 1,
            clip: TONE,
            at: Place::centre(0.2),
        });
        e.apply(Cmd::Move {
            slot: 1,
            at: Place::centre(1.0),
        });
        e.apply(Cmd::Hold {
            slot: 2,
            clip: TONE,
            at: Place::centre(1.0),
        });
        e.apply(Cmd::Stop { slot: 2 });
        let mut b = [0.0; BLOCK * 2];
        e.render(&mut b);
        assert_eq!(e.stats().held, 0);
        e.apply(Cmd::Install {
            id: TONE,
            clip: tone(1000),
        });
        assert_eq!(e.stats().held, 1);
        assert_eq!(e.held[1].to[0], Place::centre(1.0).l);
        e.render(&mut b);
        e.render(&mut b);
        assert!(energy(&b, 0) > 1.0);
    }

    #[test]
    fn the_limiter_keeps_a_pile_up_inside_full_scale() {
        let mut e = with_tone(8 * BLOCK);
        for _ in 0..30 {
            e.apply(play(Place::centre(1.4), 0));
        }
        let mut b = [0.0; BLOCK * 2];
        for _ in 0..3 {
            e.render(&mut b);
            assert!(b.iter().all(|s| s.abs() <= CEILING + 1e-4));
        }
    }

    #[test]
    fn the_stream_takes_the_inbox() {
        let shared = Arc::new(Shared::default());
        let mut s = Stream::new(shared.clone());
        let mut cmds = vec![
            Cmd::Install {
                id: TONE,
                clip: tone(4 * BLOCK),
            },
            play(Place::centre(1.0), 0),
        ];
        shared.send(&mut cmds);
        assert!(cmds.is_empty());
        let e: f32 = (&mut s).take(BLOCK * 2).map(|x| x * x).sum();
        assert!(e > 1.0);
    }

    /// A stalled audio thread loses the frame's one-shots, counted, but
    /// never a clip: an install is sent once.
    #[test]
    fn a_full_inbox_holds_installs_back() {
        let shared = Shared::default();
        let inbox = shared.inbox().unwrap();
        let mut cmds: Vec<Cmd> = (0..INBOX + 10).map(|_| play(Place::centre(1.0), 0)).collect();
        cmds.push(Cmd::Install {
            id: TONE,
            clip: tone(BLOCK),
        });
        shared.send(&mut cmds);
        assert!(cmds.is_empty());
        assert_eq!(shared.dropped(), 10);
        let mut e = Engine::default();
        e.take(&inbox);
        assert_eq!(e.stats().installed, 0);
        shared.send(&mut cmds);
        e.take(&inbox);
        assert_eq!(e.stats().installed, 1);
    }

    /// The bytes a page posts are the commands the desktop sends, played
    /// the same: two engines, one fed each way, render the same samples.
    #[test]
    fn bytes_play_what_the_commands_play() {
        let at = Place {
            l: 0.3,
            r: 0.6,
            rate: 1.1,
            lp: 0.5,
        };
        let cmds = || {
            vec![
                Cmd::Master(0.8),
                play(at, 5),
                Cmd::Hold {
                    slot: 2,
                    clip: TONE,
                    at,
                },
                Cmd::Move {
                    slot: 2,
                    at: Place::centre(0.4),
                },
            ]
        };
        let (mut a, mut b) = (with_tone(3 * BLOCK), with_tone(3 * BLOCK));
        for c in cmds() {
            a.apply(c);
        }
        let mut bytes = Vec::new();
        encode(&cmds(), &mut bytes);
        // A torn tail is dropped, not misread.
        bytes.push(T_PLAY);
        bytes.push(0);
        b.apply_bytes(&bytes);
        let (mut x, mut y) = ([0.0; BLOCK * 2], [0.0; BLOCK * 2]);
        for _ in 0..3 {
            a.render(&mut x);
            b.render(&mut y);
            assert_eq!(x, y);
        }
        b.apply_bytes(&[T_STOP, 2]);
        b.render(&mut y);
        b.render(&mut y);
        assert_eq!(b.stats().held, 0);
    }
}

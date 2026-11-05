//! The film's own mix: the cues the game's ear decides (`sound::decide`)
//! rendered a frame at a time instead of by a sound card, so the sound is
//! in step with a picture that takes however long it takes to draw. After
//! Gates' film `Mix` (MIT); the decoding is ours (`lewton`, the Vorbis
//! decoder Bevy's audio already uses in a browser).
//!
//! A cue placed in the world is heard from the camera the frame it starts,
//! the way Bevy's spatial audio hears it live: an inverse square of the
//! scaled distance (`sound::SCALE`), capped at one, and a pan off the
//! camera's right. Ours (unplaced) is in the middle. No player's volume: a
//! clip is at the game's own level.

use std::collections::BTreeMap;
use std::sync::Arc;

use bevy::prelude::*;

use super::script::FILM_RATE;
use crate::sound::{Cue, MASTER, SCALE};

/// Voices at once: past this the oldest is cut.
pub const VOICES: usize = 64;
/// How far a placed cue leans to the side away from it, at most.
const PAN: f32 = 0.45;
/// Where the bus starts to bend toward its ceiling, so a burst of blasts
/// rounds over rather than clips.
const KNEE: f32 = 0.7;

/// A cut decoded to [`FILM_RATE`], stereo, interleaved.
type Pcm = Arc<Vec<f32>>;

struct Voice {
    pcm: Pcm,
    /// Where in it, in frames (fractional: the cue's speed).
    at: f64,
    step: f64,
    l: f32,
    r: f32,
}

#[derive(Default)]
pub struct Mix {
    cuts: BTreeMap<String, Option<Pcm>>,
    voices: Vec<Voice>,
    buf: Vec<f32>,
}

impl Mix {
    /// A cut, decoded the first time it is asked for. `bytes` fetches its
    /// file; a cut that isn't there yet (a page's sound still loading) is
    /// asked for again next time, one that won't decode never is.
    fn cut(&mut self, name: &str, bytes: &mut dyn FnMut(&str) -> Option<Vec<u8>>) -> Option<Pcm> {
        if let Some(c) = self.cuts.get(name) {
            return c.clone();
        }
        let ogg = bytes(name)?;
        let pcm = match decode(&ogg) {
            Ok(p) => Some(Arc::new(p)),
            Err(e) => {
                warn!("film: sound/{name}.ogg: {e}");
                None
            }
        };
        self.cuts.insert(name.to_string(), pcm.clone());
        pcm
    }

    /// Start `cue`, heard from `ear` (the camera).
    pub fn start(&mut self, cue: &Cue, ear: &GlobalTransform, bytes: &mut dyn FnMut(&str) -> Option<Vec<u8>>) {
        let Some(pcm) = self.cut(&cue.name, bytes) else {
            return;
        };
        let (l, r) = place(cue.at, ear);
        let g = cue.gain * MASTER;
        if self.voices.len() >= VOICES {
            self.voices.remove(0);
        }
        self.voices.push(Voice {
            pcm,
            at: 0.0,
            step: cue.speed.max(0.1) as f64,
            l: l * g,
            r: r * g,
        });
    }

    /// Silence everything playing: the film has jumped.
    pub fn hush(&mut self) {
        self.voices.clear();
    }

    /// The next `frames` stereo frames, interleaved.
    pub fn render(&mut self, frames: usize) -> &[f32] {
        self.buf.clear();
        self.buf.resize(frames * 2, 0.0);
        for v in &mut self.voices {
            let n = v.pcm.len() / 2;
            for k in 0..frames {
                let i = v.at as usize;
                if i + 1 >= n {
                    v.at = n as f64;
                    break;
                }
                let u = (v.at - i as f64) as f32;
                let s = |c: usize| v.pcm[2 * i + c] * (1.0 - u) + v.pcm[2 * (i + 1) + c] * u;
                self.buf[2 * k] += s(0) * v.l;
                self.buf[2 * k + 1] += s(1) * v.r;
                v.at += v.step;
            }
        }
        self.voices.retain(|v| (v.at as usize) + 1 < v.pcm.len() / 2);
        for s in &mut self.buf {
            *s = knee(*s);
        }
        &self.buf
    }
}

/// Left and right gains for a cue at `at` heard from `ear`.
fn place(at: Option<Vec3>, ear: &GlobalTransform) -> (f32, f32) {
    let Some(p) = at else {
        return (1.0, 1.0);
    };
    let to = p - ear.translation();
    let d = to.length() * SCALE;
    let fall = if d > 1.0 { 1.0 / (d * d) } else { 1.0 };
    let side = if to.length() > 1e-3 {
        to.normalize().dot(ear.right().as_vec3()).clamp(-1.0, 1.0)
    } else {
        0.0
    };
    (
        fall * (1.0 - PAN * side.max(0.0)),
        fall * (1.0 - PAN * (-side).max(0.0)),
    )
}

/// The bus: straight to [`KNEE`], then bent toward 1.
fn knee(x: f32) -> f32 {
    let a = x.abs();
    if a <= KNEE {
        x
    } else {
        let over = (a - KNEE) / (1.0 - KNEE);
        x.signum() * (KNEE + (1.0 - KNEE) * over.tanh())
    }
}

/// An Ogg Vorbis file to [`FILM_RATE`] stereo, interleaved floats.
pub fn decode(ogg: &[u8]) -> Result<Vec<f32>, String> {
    let mut r = lewton::inside_ogg::OggStreamReader::new(std::io::Cursor::new(ogg)).map_err(|e| e.to_string())?;
    let ch = r.ident_hdr.audio_channels.max(1) as usize;
    let rate = r.ident_hdr.audio_sample_rate.max(1);
    let mut frames: Vec<[f32; 2]> = Vec::new();
    while let Some(p) = r.read_dec_packet_itl().map_err(|e| e.to_string())? {
        for f in p.chunks_exact(ch) {
            let l = f[0] as f32 / 32768.0;
            let rr = if ch > 1 { f[1] as f32 / 32768.0 } else { l };
            frames.push([l, rr]);
        }
    }
    Ok(resample(&frames, rate))
}

/// Linear resampling to [`FILM_RATE`]: the cuts are short and bright, and a
/// film's mix is heard once.
fn resample(frames: &[[f32; 2]], rate: u32) -> Vec<f32> {
    if frames.is_empty() {
        return Vec::new();
    }
    let ratio = rate as f64 / FILM_RATE as f64;
    let n = ((frames.len() as f64) / ratio).floor().max(1.0) as usize;
    let mut out = Vec::with_capacity(n * 2);
    for k in 0..n {
        let x = k as f64 * ratio;
        let i = (x as usize).min(frames.len() - 1);
        let j = (i + 1).min(frames.len() - 1);
        let u = (x - i as f64) as f32;
        for (a, b) in frames[i].iter().zip(frames[j].iter()) {
            out.push(a * (1.0 - u) + b * u);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_voice_plays_through_once_and_is_gone() {
        let mut m = Mix::default();
        let pcm: Pcm = Arc::new(vec![0.5; 2 * 100]);
        m.cuts.insert("x".into(), Some(pcm));
        let cue = Cue {
            name: "x".into(),
            at: None,
            gain: 1.0,
            speed: 1.0,
        };
        m.start(&cue, &GlobalTransform::IDENTITY, &mut |_| None);
        let out = m.render(60).to_vec();
        assert!((out[0] - 0.5 * MASTER).abs() < 1e-6);
        let out = m.render(60).to_vec();
        assert!(out[2 * 50..].iter().all(|s| *s == 0.0), "done by frame 100");
        assert!(m.voices.is_empty());
    }

    #[test]
    fn far_is_quiet_and_the_side_leans() {
        let ear = GlobalTransform::IDENTITY;
        let (l0, r0) = place(Some(Vec3::new(0.0, 0.0, -2.0)), &ear);
        assert_eq!((l0, r0), (1.0, 1.0));
        let (lf, _) = place(Some(Vec3::new(0.0, 0.0, -40.0)), &ear);
        assert!(lf < 0.05);
        let (l, r) = place(Some(Vec3::new(3.0, 0.0, 0.0)), &ear);
        assert!(r > l, "a cue to the right is louder on the right");
        assert!(knee(3.0) < 1.0 && knee(0.5) == 0.5);
    }

    #[test]
    fn resampling_keeps_the_length_in_time() {
        let frames = vec![[0.25f32, -0.25]; 22_050];
        let out = resample(&frames, 22_050);
        assert_eq!(out.len() / 2, FILM_RATE as usize);
        assert!(out.iter().step_by(2).all(|s| (*s - 0.25).abs() < 1e-6));
    }
}

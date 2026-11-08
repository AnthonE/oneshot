//! Bred recipes: a genome is a base seed and one 16-bit seed per accepted
//! mutation, and replaying it rebuilds what it describes bit for bit (Such
//! et al. 2017, "Deep Neuroevolution"), so a whole bred thing fits in a
//! tweet. Copied from Gates (`crates/sim-core/src/genome.rs`, after Quake
//! Golf's `client/brain.mjs`) and trimmed to the recipe and its text form:
//! the same splitmix32 draws, the same three-uniform noise, the same seed
//! mix and step table. Here it breeds the zoo's aim assists
//! (`crate::assist`, docs/ANTICHEAT.md §5), not nets.
//!
//! **What a genome says.** Nothing on its own: it is a recipe for one list
//! of values. The four-byte prefix of the text form names the list (`oa1:`
//! is an assist's knobs), and a change to it is a new prefix, never a
//! silently different cheat.

use sim::fmath::fabs;

/// Accepted mutations a genome holds: as many as still fit a tweet in text
/// form (4 prefix bytes + base64url of 4 + 2·101 bytes = 279 characters).
pub const MAX_MUTATIONS: usize = 101;
/// The longest text form, bytes.
pub const TEXT_MAX: usize = 280;

/// A mutation's step, picked by its seed's top two bits.
const SIGMA: [f32; 4] = [0.05, 0.1, 0.2, 0.4];

/// A recipe: the base seed, then one seed per accepted mutation. Fixed
/// size, so it is `Copy` and a pool of them is a flat array.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Genome {
    pub base: u32,
    n: u8,
    muts: [u16; MAX_MUTATIONS],
}

/// Why a text form was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GenomeError {
    /// It does not start with the prefix expected.
    Prefix,
    /// A character outside base64url.
    Alphabet,
    /// Not 4 + 2n bytes, or a length base64 cannot produce.
    Length,
    /// More mutations than [`MAX_MUTATIONS`].
    TooLong,
}

impl Genome {
    /// The unmutated recipe from `base`.
    pub const fn new(base: u32) -> Self {
        Self {
            base,
            n: 0,
            muts: [0; MAX_MUTATIONS],
        }
    }

    pub fn muts(&self) -> &[u16] {
        &self.muts[..self.n as usize]
    }

    pub fn len(&self) -> usize {
        self.n as usize
    }

    pub fn is_empty(&self) -> bool {
        self.n == 0
    }

    /// Room for another mutation.
    pub fn full(&self) -> bool {
        self.n as usize >= MAX_MUTATIONS
    }

    /// This genome with one more mutation; `None` once it is full.
    pub fn child(&self, m: u16) -> Option<Self> {
        if self.full() {
            return None;
        }
        let mut c = *self;
        c.muts[c.n as usize] = m;
        c.n += 1;
        Some(c)
    }

    /// The values it describes, each `scale` times unit noise from the
    /// base, then every mutation's step in turn.
    pub fn build(&self, w: &mut [f32], scale: f32) {
        let mut u = Draw(self.base);
        for wi in w.iter_mut() {
            *wi = noise(&mut u) * scale;
        }
        for (g, &m) in self.muts().iter().enumerate() {
            perturb(w, |_| scale, self.base, g, m);
        }
    }

    /// The text form: `prefix` then base64url, little-endian base and
    /// seeds, no padding. Returns the bytes written to `out`.
    pub fn encode(&self, prefix: &[u8; 4], out: &mut [u8; TEXT_MAX]) -> usize {
        let mut raw = [0u8; 4 + 2 * MAX_MUTATIONS];
        raw[..4].copy_from_slice(&self.base.to_le_bytes());
        for (i, m) in self.muts().iter().enumerate() {
            raw[4 + 2 * i..6 + 2 * i].copy_from_slice(&m.to_le_bytes());
        }
        out[..4].copy_from_slice(prefix);
        4 + b64_encode(&raw[..4 + 2 * self.len()], &mut out[4..])
    }

    /// The text form as a string.
    pub fn text(&self, prefix: &[u8; 4]) -> String {
        let mut buf = [0u8; TEXT_MAX];
        let n = self.encode(prefix, &mut buf);
        String::from_utf8_lossy(&buf[..n]).into_owned()
    }

    /// Read a text form back. Surrounding whitespace is ignored.
    pub fn decode(prefix: &[u8; 4], text: &[u8]) -> Result<Self, GenomeError> {
        let text = text.trim_ascii();
        let body = text.strip_prefix(prefix.as_slice()).ok_or(GenomeError::Prefix)?;
        let mut raw = [0u8; 4 + 2 * MAX_MUTATIONS + 2];
        let n = b64_decode(body, &mut raw)?;
        if n < 4 || !n.is_multiple_of(2) {
            return Err(GenomeError::Length);
        }
        let muts = (n - 4) / 2;
        if muts > MAX_MUTATIONS {
            return Err(GenomeError::TooLong);
        }
        let mut g = Self::new(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]));
        for i in 0..muts {
            g.muts[i] = u16::from_le_bytes([raw[4 + 2 * i], raw[5 + 2 * i]]);
        }
        g.n = muts as u8;
        Ok(g)
    }
}

/// One mutation step over any values: mutation number `g` (from zero)
/// with seed `m`, on a genome whose base is `base`, each value moved by
/// the seed's step size times `scale(i)` times a fresh draw.
pub fn perturb(w: &mut [f32], scale: impl Fn(usize) -> f32, base: u32, g: usize, m: u16) {
    let mut u = Draw(mix(base, g as u32, (m & 0x3fff) as u32));
    let sig = SIGMA[(m >> 14) as usize];
    for (i, wi) in w.iter_mut().enumerate() {
        *wi += sig * scale(i) * noise(&mut u);
    }
}

/// Softsign: tanh's shape from one division.
pub fn soft(x: f32) -> f32 {
    x / (1.0 + fabs(x))
}

/// splitmix32 for the bits, then 24 of them as an exact f32 in [0, 1).
#[derive(Clone, Copy, Debug)]
pub struct Draw(pub u32);

impl Draw {
    pub fn draw(&mut self) -> f32 {
        self.0 = self.0.wrapping_add(0x9e37_79b9);
        let mut z = self.0;
        z = (z ^ (z >> 16)).wrapping_mul(0x85eb_ca6b);
        z = (z ^ (z >> 13)).wrapping_mul(0xc2b2_ae35);
        ((z ^ (z >> 16)) >> 8) as f32 * (1.0 / 16_777_216.0)
    }
}

/// Three uniforms, centred and scaled to variance 1.
fn noise(u: &mut Draw) -> f32 {
    let (a, b, c) = (u.draw(), u.draw(), u.draw());
    (a + b + c - 1.5) * 2.0
}

/// Mutation `g`'s draw seed: the base, the mutation's place and its own
/// seed, so the same seed at another place is another step.
fn mix(base: u32, g: u32, s: u32) -> u32 {
    (base ^ 0x5bd1_e995).wrapping_mul(0x27d4_eb2d)
        ^ g.wrapping_add(1).wrapping_mul(0x9e37_79b9)
        ^ s.wrapping_add(1).wrapping_mul(0x85eb_ca6b)
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn b64_encode(raw: &[u8], out: &mut [u8]) -> usize {
    let mut n = 0;
    for chunk in raw.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let v = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        for k in 0..chunk.len() + 1 {
            out[n] = B64[((v >> (18 - 6 * k)) & 63) as usize];
            n += 1;
        }
    }
    n
}

fn b64_decode(text: &[u8], out: &mut [u8]) -> Result<usize, GenomeError> {
    if text.len() % 4 == 1 {
        return Err(GenomeError::Length);
    }
    let bytes = text.len() * 3 / 4;
    if bytes > out.len() {
        return Err(GenomeError::TooLong);
    }
    let mut n = 0;
    for chunk in text.chunks(4) {
        let mut v = 0u32;
        for (k, &c) in chunk.iter().enumerate() {
            let d = B64.iter().position(|&a| a == c).ok_or(GenomeError::Alphabet)?;
            v |= (d as u32) << (18 - 6 * k);
        }
        for k in 0..chunk.len() - 1 {
            out[n] = (v >> (16 - 8 * k)) as u8;
            n += 1;
        }
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One mutation at each step size, and the largest seed.
    fn sample() -> Genome {
        let mut g = Genome::new(0xdead_beef);
        for m in [1, 0x4002, 0x8003, 0xc004, 0xffff] {
            g = g.child(m).unwrap();
        }
        g
    }

    /// Printed by Quake Golf's own `brain.mjs` (node 22), as Gates holds
    /// its copy: the recipe is the original's, not this file's.
    #[test]
    fn it_draws_the_golfers_numbers() {
        let mut u = Draw(7);
        assert_eq!(u.draw() as f64, 0.1370641589164734);
        assert_eq!(u.draw() as f64, 0.4510822296142578);
        assert_eq!(noise(&mut Draw(7)) as f64, 0.17249536514282227);
        assert_eq!(mix(0xdead_beef, 3, 0x1234), 2_497_828_529);
    }

    #[test]
    fn the_text_form_is_the_golfers_and_round_trips() {
        let g = sample();
        assert_eq!(g.text(b"qg2:"), "qg2:776t3gEAAkADgATA__8");
        assert_eq!(Genome::decode(b"qg2:", g.text(b"qg2:").as_bytes()), Ok(g));
        assert_eq!(
            Genome::decode(b"oa1:", b"qg2:776t3gEAAkADgATA__8"),
            Err(GenomeError::Prefix)
        );
        assert_eq!(Genome::decode(b"qg2:", b"qg2:77*t"), Err(GenomeError::Alphabet));
        assert_eq!(Genome::decode(b"qg2:", b"qg2:776t3"), Err(GenomeError::Length));
        let mut full = Genome::new(u32::MAX);
        while let Some(c) = full.child(0xffff) {
            full = c;
        }
        assert!(full.text(b"oa1:").len() <= TEXT_MAX);
    }

    #[test]
    fn a_child_is_its_parent_plus_one_step() {
        let parent = sample();
        let child = parent.child(0x9abc).unwrap();
        let mut stepped = [0.0; 9];
        parent.build(&mut stepped, 0.5);
        perturb(&mut stepped, |_| 0.5, parent.base, parent.len(), 0x9abc);
        let mut built = [0.0; 9];
        child.build(&mut built, 0.5);
        assert_eq!(stepped, built);
    }
}

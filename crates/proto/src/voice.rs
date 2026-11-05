//! Party voice: one Opus frame per datagram, both ways, on the lobby
//! session (Gates' `protocol/src/voice.rs`, its layout and caps).
//!
//! Up, the talker's own frame: flags, a sequence number, the payload. Down,
//! the server's copy of it to one listener, with the talker's party tag in
//! front. The server relays it to the talker's party and nobody else; it
//! never decodes it. The payload is opaque here: an Opus packet, one 20 ms
//! frame ([`is_one_frame`]), at most [`VOICE_MAX_BYTES`].

use crate::bits::{BitReader, BitWriter, WireError};
use crate::party::TAG_BITS;

type R<T> = Result<T, WireError>;

/// Longest Opus payload one frame carries: 20 ms at 64 kbps. Speech runs
/// near 24 kbps (~60 B), so this is headroom, not a target.
pub const VOICE_MAX_BYTES: usize = 160;
/// The last frame of one go at talking: the listener's light goes out.
pub const VOICE_END: u8 = 1;
/// A talker's frames a second (20 ms each), and the burst over it the
/// server lets through.
pub const VOICE_FPS: u32 = 50;
pub const VOICE_BURST: u32 = 10;

pub(crate) const KIND_VOICE: u32 = 2;

/// One voice frame. `from` is the talker's tag down, 0 up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VoiceFrame {
    pub from: u8,
    pub seq: u16,
    pub flags: u8,
    len: u8,
    bytes: [u8; VOICE_MAX_BYTES],
}

impl VoiceFrame {
    /// A frame around `payload`; none when it is empty or over the cap.
    pub fn new(seq: u16, flags: u8, payload: &[u8]) -> Option<VoiceFrame> {
        if payload.is_empty() || payload.len() > VOICE_MAX_BYTES || flags > VOICE_END {
            return None;
        }
        let mut f = VoiceFrame {
            from: 0,
            seq,
            flags,
            len: payload.len() as u8,
            bytes: [0; VOICE_MAX_BYTES],
        };
        f.bytes[..payload.len()].copy_from_slice(payload);
        Some(f)
    }

    pub fn payload(&self) -> &[u8] {
        &self.bytes[..self.len as usize]
    }
}

fn body(w: &mut BitWriter, f: &VoiceFrame) -> R<()> {
    w.write(f.seq as u32, 16)?;
    w.write(f.len as u32, 8)?;
    for &b in f.payload() {
        w.write(b as u32, 8)?;
    }
    Ok(())
}

fn r_body(r: &mut BitReader, from: u8, flags: u8) -> R<VoiceFrame> {
    let seq = r.read(16)? as u16;
    let len = r.read(8)? as usize;
    if len == 0 || len > VOICE_MAX_BYTES {
        return Err(WireError::Cap);
    }
    let mut payload = [0u8; VOICE_MAX_BYTES];
    for b in payload[..len].iter_mut() {
        *b = r.read(8)? as u8;
    }
    let mut f = VoiceFrame::new(seq, flags, &payload[..len]).ok_or(WireError::Malformed)?;
    f.from = from;
    Ok(f)
}

/// Client → server: `[kind:4][flags:4][seq:16][len:8][payload]`.
pub fn encode_voice_up(f: &VoiceFrame, out: &mut [u8]) -> R<usize> {
    let mut w = BitWriter::new(out);
    w.write(KIND_VOICE, 4)?;
    w.write(f.flags as u32, 4)?;
    body(&mut w, f)?;
    Ok(w.finish())
}

pub fn decode_voice_up(buf: &[u8]) -> R<VoiceFrame> {
    let mut r = BitReader::new(buf);
    if r.read(4)? != KIND_VOICE {
        return Err(WireError::Malformed);
    }
    let flags = r.read(4)? as u8;
    let f = r_body(&mut r, 0, flags)?;
    crate::done(&r)?;
    Ok(f)
}

/// Server → client: `[kind:4][flags:4][from:4][seq:16][len:8][payload]`.
pub fn encode_voice(f: &VoiceFrame, out: &mut [u8]) -> R<usize> {
    let mut w = BitWriter::new(out);
    w.write(KIND_VOICE, 4)?;
    w.write(f.flags as u32, 4)?;
    w.write(f.from as u32, TAG_BITS)?;
    body(&mut w, f)?;
    Ok(w.finish())
}

pub fn decode_voice(buf: &[u8]) -> R<VoiceFrame> {
    let mut r = BitReader::new(buf);
    if r.read(4)? != KIND_VOICE {
        return Err(WireError::Malformed);
    }
    let flags = r.read(4)? as u8;
    let from = r.read(TAG_BITS)? as u8;
    let f = r_body(&mut r, from, flags)?;
    crate::done(&r)?;
    Ok(f)
}

/// Whether an Opus packet is one 20 ms frame, the only shape a talker
/// sends (Gates' `voice::is_one_frame`): the TOC byte (RFC 6716 §3.1) says
/// code 0, one frame, in a configuration whose frame is 20 ms. Anything
/// else is dropped, so a forged packet can't make a listener decode 120 ms.
pub fn is_one_frame(packet: &[u8]) -> bool {
    let Some(&toc) = packet.first() else {
        return false;
    };
    if toc & 3 != 0 {
        return false;
    }
    let config = toc >> 3;
    match config {
        0..=11 => config % 4 == 1,
        12..=15 => config % 2 == 1,
        _ => config % 4 == 3,
    }
}

/// A talker's allowance: [`VOICE_FPS`] frames a second, [`VOICE_BURST`]
/// over it at once (Gates' `VoiceIn`). Over it, a frame is dropped.
#[derive(Clone, Copy, Debug)]
pub struct Bucket {
    tokens: f64,
    at_ms: f64,
}

impl Default for Bucket {
    fn default() -> Self {
        Bucket {
            tokens: VOICE_BURST as f64,
            at_ms: 0.0,
        }
    }
}

impl Bucket {
    pub fn allow(&mut self, now_ms: f64) -> bool {
        let dt = (now_ms - self.at_ms).max(0.0) / 1000.0;
        self.at_ms = now_ms;
        self.tokens = (self.tokens + dt * VOICE_FPS as f64).min(VOICE_BURST as f64);
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_round_trip_and_the_cap_holds() {
        let mut f = VoiceFrame::new(65_535, VOICE_END, &[0x0b, 1, 2, 3]).unwrap();
        let mut buf = [0u8; 256];
        let n = encode_voice_up(&f, &mut buf).unwrap();
        assert_eq!(n, 4 + 4);
        assert_eq!(decode_voice_up(&buf[..n]).unwrap(), f);
        f.from = 7;
        let n = encode_voice(&f, &mut buf).unwrap();
        assert_eq!(decode_voice(&buf[..n]).unwrap(), f);
        assert!(VoiceFrame::new(0, 0, &[]).is_none());
        assert!(VoiceFrame::new(0, 0, &[0; VOICE_MAX_BYTES + 1]).is_none());
        assert!(decode_voice_up(&buf[..n - 1]).is_err());
    }

    #[test]
    fn only_one_twenty_ms_frame_is_taken() {
        // SILK wideband 20 ms (config 9), code 0: yes. Code 3: no.
        assert!(is_one_frame(&[9 << 3, 0]));
        assert!(!is_one_frame(&[(9 << 3) | 3, 0]));
        // CELT fullband 20 ms is config 31; 2.5 ms (28) is not.
        assert!(is_one_frame(&[31 << 3]));
        assert!(!is_one_frame(&[28 << 3]));
        // SILK 60 ms (config 3) is not.
        assert!(!is_one_frame(&[3 << 3]));
    }

    #[test]
    fn the_bucket_lets_fifty_a_second_through() {
        let mut b = Bucket::default();
        let took = (0..100).filter(|i| b.allow(*i as f64 * 10.0)).count();
        // A second at 100 a second: the burst, then fifty a second.
        assert!((55..=62).contains(&took), "{took}");
    }
}

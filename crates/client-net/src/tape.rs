//! A tape: everything a session was sent, and when, so the same client can
//! draw it again (the clip key, `crate::clip`; film mode, the client's
//! `film.rs`). After Gates' `client/src/film.rs` (MIT).
//!
//! An entry is a stream message or a snapshot datagram, as bytes off the
//! wire, with the session clock it was taken in at (ms). A [`Replay`] hands
//! them back to an ordinary [`Session`] at the same clock, through the same
//! `on_msg` and `on_datagram` the transports call, so everything downstream
//! (the netcode's clock, its interpolation, the kill cam) runs exactly as it
//! did live. Nothing on a tape is more than this client was sent: fog
//! shaped every snapshot on the server (`room/src/fog.rs`), and a replay
//! can only draw what it holds.
//!
//! The session that plays a tape **watches** (`Session::replay`): there is
//! no input to predict its own fighter from, so that fighter is drawn off
//! the snapshots between two of them like everyone else
//! (`NetCore::watch`), and nothing it makes is sent anywhere.
//!
//! A tape is wire bytes, so it is only good for the build that speaks its
//! wire: the header carries `PROTO_VER` and [`read`] refuses any other.

use proto::{decode_msg, encode_msg, Msg, MAX_STREAM_MSG, PROTO_VER};

use crate::session::{Session, Wire};

pub const MAGIC: &[u8; 8] = b"ONESHOTT";
/// Magic and the wire's version.
pub const HEADER_BYTES: usize = 12;
/// An entry ahead of its bytes: the clock (f64), the lane, the length.
pub const ENTRY_BYTES: usize = 8 + 1 + 4;
/// The most an entry may hold: a datagram's or a stream message's.
pub const MAX_ENTRY: usize = if proto::MAX_DATAGRAM > MAX_STREAM_MSG {
    proto::MAX_DATAGRAM
} else {
    MAX_STREAM_MSG
};

/// Which way an entry came in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lane {
    /// A snapshot datagram.
    Datagram = 0,
    /// A stream message (`proto::Msg`, encoded).
    Msg = 1,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    /// The session clock it was taken in at, ms from the tape's start.
    pub t_ms: f64,
    pub lane: Lane,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tape {
    pub entries: Vec<Entry>,
}

impl Tape {
    /// The clock of the last entry: how much match the tape holds.
    pub fn end_ms(&self) -> f64 {
        self.entries.last().map_or(0.0, |e| e.t_ms)
    }

    /// The first welcome on the tape, which names its seat and its map.
    pub fn welcome(&self) -> Option<Msg> {
        self.messages().find(|m| matches!(m, Msg::Welcome { .. }))
    }

    /// The map the tape's welcome names.
    pub fn map(&self) -> Option<u8> {
        match self.welcome()? {
            Msg::Welcome { map, .. } => Some(map),
            _ => None,
        }
    }

    /// Every stream message on the tape, decoded, in order.
    pub fn messages(&self) -> impl Iterator<Item = Msg> + '_ {
        self.entries
            .iter()
            .filter(|e| e.lane == Lane::Msg)
            .filter_map(|e| decode_msg(&e.bytes).ok())
    }
}

pub fn write_header(out: &mut Vec<u8>) {
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&u32::from(PROTO_VER).to_le_bytes());
}

pub fn write_entry(out: &mut Vec<u8>, t_ms: f64, lane: Lane, bytes: &[u8]) {
    out.extend_from_slice(&t_ms.to_le_bytes());
    out.push(lane as u8);
    out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(bytes);
}

/// A stream message as an entry's bytes.
pub fn msg_bytes(m: &Msg) -> Option<Vec<u8>> {
    let mut buf = [0u8; MAX_STREAM_MSG];
    let n = encode_msg(m, &mut buf).ok()?;
    Some(buf[..n].to_vec())
}

/// A whole tape from its bytes. A torn tail is an error with its offset,
/// never a shorter tape; a tape from another wire says so.
pub fn read(all: &[u8]) -> Result<Tape, String> {
    let mut c = Cursor { all, at: 0 };
    if c.take(8)? != MAGIC {
        return Err("not a oneshot tape".into());
    }
    let ver = c.u32()?;
    if ver != u32::from(PROTO_VER) {
        return Err(format!(
            "taped on wire v{ver}; this build speaks v{PROTO_VER}: tape it again with this build"
        ));
    }
    let mut entries = Vec::new();
    let mut last = 0.0f64;
    while c.at < all.len() {
        let at = c.at;
        let mut t = [0u8; 8];
        t.copy_from_slice(c.take(8)?);
        let t_ms = f64::from_le_bytes(t);
        let lane = match c.take(1)?[0] {
            0 => Lane::Datagram,
            1 => Lane::Msg,
            other => return Err(format!("unknown lane {other} at byte {at}")),
        };
        let len = c.u32()? as usize;
        if len > MAX_ENTRY {
            return Err(format!("an entry of {len} bytes at byte {at}"));
        }
        if !t_ms.is_finite() || t_ms < last {
            return Err(format!("the clock runs back at byte {at}"));
        }
        last = t_ms;
        let bytes = c.take(len)?.to_vec();
        entries.push(Entry { t_ms, lane, bytes });
    }
    Ok(Tape { entries })
}

/// A cursor over the whole tape, so a short read names its offset.
struct Cursor<'a> {
    all: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let s = self
            .all
            .get(self.at..self.at + n)
            .ok_or_else(|| format!("torn at byte {} of {}", self.at, self.all.len()))?;
        self.at += n;
        Ok(s)
    }

    fn u32(&mut self) -> Result<u32, String> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
}

/// Where a replayed session's datagrams go: nowhere.
pub struct Nowhere;

impl Wire for Nowhere {
    fn send_datagram(&mut self, _: &[u8]) -> bool {
        true
    }
}

/// A tape's end of a replayed session: each frame the clock moves on and
/// whatever the tape holds up to it goes in.
#[derive(Clone, Debug)]
pub struct Replay {
    tape: Tape,
    next: usize,
    /// The session clock the replay has reached, ms.
    pub clock_ms: f64,
}

impl Replay {
    pub fn new(tape: Tape) -> Replay {
        Replay {
            tape,
            next: 0,
            clock_ms: 0.0,
        }
    }

    pub fn tape(&self) -> &Tape {
        &self.tape
    }

    pub fn end_ms(&self) -> f64 {
        self.tape.end_ms()
    }

    /// Every entry has gone in.
    pub fn done(&self) -> bool {
        self.next >= self.tape.entries.len()
    }

    /// Hands `s` every entry at or before the clock, as a transport does:
    /// messages to `on_msg`, datagrams to `on_datagram` at the clock they
    /// came in at. Returns how many went in.
    pub fn feed(&mut self, s: &mut Session) -> usize {
        let mut fed = 0;
        while let Some(e) = self.tape.entries.get(self.next) {
            if e.t_ms > self.clock_ms {
                break;
            }
            match e.lane {
                Lane::Msg => {
                    if let Ok(m) = decode_msg(&e.bytes) {
                        s.on_msg(m);
                    }
                }
                Lane::Datagram => s.on_datagram(e.t_ms, &e.bytes),
            }
            self.next += 1;
            fed += 1;
        }
        fed
    }

    /// One frame of a replayed session: what the tape holds to the clock,
    /// then the session's frame at it (its interpolation; nothing is sent).
    pub fn frame(&mut self, s: &mut Session) {
        self.feed(s);
        s.update(self.clock_ms, sim::input::Input::default(), &mut Nowhere);
    }

    /// Runs the tape on to `to_ms` without drawing it, in steps of `step_ms`
    /// (a snapshot's interval is plenty: the netcode's clock is estimated
    /// from arrivals, so each one must go in at its own time). What the
    /// steps made for effects is dropped: it happened off screen. The
    /// stream's messages are kept for whoever reads them (who sits where).
    pub fn seek(&mut self, s: &mut Session, to_ms: f64, step_ms: f64) {
        let step = step_ms.max(1.0);
        while self.clock_ms + step <= to_ms {
            self.clock_ms += step;
            self.frame(s);
            s.core.drain_events(self.clock_ms);
        }
        if self.clock_ms < to_ms {
            self.clock_ms = to_ms;
            self.frame(s);
            s.core.drain_events(self.clock_ms);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tape_reads_back_what_was_written_and_a_torn_one_is_refused() {
        let mut buf = Vec::new();
        write_header(&mut buf);
        let bye = msg_bytes(&Msg::Bye).unwrap();
        write_entry(&mut buf, 0.0, Lane::Msg, &bye);
        write_entry(&mut buf, 33.5, Lane::Datagram, &[]);
        write_entry(&mut buf, 66.25, Lane::Datagram, &[9; 300]);
        let t = read(&buf).unwrap();
        assert_eq!(t.entries.len(), 3);
        assert_eq!(t.entries[0].lane, Lane::Msg);
        assert_eq!(t.messages().next(), Some(Msg::Bye));
        assert_eq!(t.entries[2].t_ms, 66.25);
        assert_eq!(t.entries[2].bytes.len(), 300);
        assert_eq!(t.end_ms(), 66.25);
        buf.truncate(buf.len() - 1);
        assert!(read(&buf).is_err());
        let mut other = Vec::new();
        other.extend_from_slice(MAGIC);
        other.extend_from_slice(&(u32::from(PROTO_VER) + 1).to_le_bytes());
        assert!(read(&other).unwrap_err().contains("wire"));
        assert!(read(b"GATESREC\0\0\0\0").is_err());
    }
}

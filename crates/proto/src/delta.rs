//! Delta snapshots (docs/DESIGN.md §10): a snapshot as what changed since
//! one the client has acked, record by record.
//!
//! A snapshot is cut into keyed records ([`Keyed`]): our own fighter, the
//! vehicle we ride, the pads, the flags, the score and clock, the guns on
//! the floor, the rounds in flight, the events, and each other fighter and
//! vehicle by its slot. Each record is the full snapshot's own bits for that
//! thing, byte-padded. A delta lists the records this snapshot has, each
//! either as bytes XOR the base's record of the same key (most of a record
//! is the same 16 ms later, so most of its bytes are zero) or whole. A
//! record the base had and this snapshot hasn't is simply not listed: a
//! fighter fog no longer shows is gone, as in a full snapshot.
//!
//! The base is always one the client acked, never older than
//! [`DELTA_WINDOW`] ticks, and both ends keep every snapshot of that window
//! ([`Ring`]); a base either end lacks means a full snapshot instead. A
//! delta says nothing a full snapshot of the same tick wouldn't: it is a
//! function of that snapshot and one the client already has.

use super::*;

pub(crate) const KIND_DELTA: u32 = 3;
/// How far behind a delta its base may be, in ticks: what both ends keep.
pub const DELTA_WINDOW: u32 = 64;
/// Snapshots kept, by `(tick / 2) % RING`: every snapshot of the window at
/// any rate that sends on even ticks.
const RING: usize = (DELTA_WINDOW / 2) as usize;

const K_OWN: u8 = 0;
const K_OWN_VEHICLE: u8 = 1;
const K_PADS: u8 = 2;
const K_FLAGS: u8 = 3;
const K_TAIL: u8 = 4;
const K_DROPS: u8 = 5;
const K_PROJ: u8 = 6;
const K_EVENTS: u8 = 7;
/// Another fighter, by slot.
const K_REMOTE: u8 = 8;
/// A vehicle, by slot.
const K_VEHICLE: u8 = K_REMOTE + MAX_FIGHTERS as u8;
const MAX_KEYS: usize = K_VEHICLE as usize + MAX_SNAP_VEHICLES;
const KEY_BITS: u32 = 6;
const K_END: u32 = (1 << KEY_BITS) - 1;
const _: () = assert!(MAX_KEYS < K_END as usize);
/// A record's length on the wire, when sent whole.
const LEN_BITS: u32 = 11;
/// Room for a datagram's snapshot cut into records, each padded to a byte.
pub const KEYED_BYTES: usize = MAX_DATAGRAM + MAX_KEYS;
const _: () = assert!(KEYED_BYTES < 1 << LEN_BITS);
/// XOR'd records go in chunks of this many bytes: a flag for the chunk,
/// then one for each byte in a chunk that changed.
const CHUNK: usize = 8;

/// A snapshot cut into records: what a delta is taken against and made of.
#[derive(Clone)]
pub struct Keyed {
    pub tick: u32,
    pub last_seq: u32,
    pub depth: i8,
    n: usize,
    keys: [u8; MAX_KEYS],
    /// Where each record ends in `bytes` (the first starts at 0).
    ends: [u16; MAX_KEYS],
    bytes: [u8; KEYED_BYTES],
}

impl Default for Keyed {
    fn default() -> Self {
        Keyed {
            tick: u32::MAX,
            last_seq: 0,
            depth: 0,
            n: 0,
            keys: [0; MAX_KEYS],
            ends: [0; MAX_KEYS],
            bytes: [0; KEYED_BYTES],
        }
    }
}

fn align(w: &mut BitWriter) -> R<()> {
    let pad = (8 - w.bit_pos() % 8) % 8;
    if pad > 0 {
        w.write(0, pad as u32)?;
    }
    Ok(())
}

impl Keyed {
    /// `s` cut into records. Fails (and a full snapshot goes instead) only
    /// if `s` is over its caps or its fighters or vehicles are out of slot
    /// order, which `room::build` never makes.
    pub fn fill(&mut self, s: &Snapshot) -> R<()> {
        // Nobody's base until it is whole.
        self.tick = u32::MAX;
        self.last_seq = s.last_seq;
        self.depth = s.depth;
        self.n = 0;
        let (keys, ends) = (&mut self.keys, &mut self.ends);
        let mut n = 0usize;
        let mut w = BitWriter::new(&mut self.bytes);
        let mut rec = |w: &mut BitWriter, key: u8, f: &mut dyn FnMut(&mut BitWriter) -> R<()>| -> R<()> {
            if n >= MAX_KEYS || (n > 0 && keys[n - 1] >= key) {
                return Err(WireError::Malformed);
            }
            f(w)?;
            align(w)?;
            keys[n] = key;
            ends[n] = (w.bit_pos() / 8) as u16;
            n += 1;
            Ok(())
        };
        rec(&mut w, K_OWN, &mut |w| w_own(w, &s.own))?;
        if let Some(v) = &s.own_vehicle {
            rec(&mut w, K_OWN_VEHICLE, &mut |w| w_vehicle(w, v))?;
        }
        rec(&mut w, K_PADS, &mut |w| w_list(w, s.pads(), 5))?;
        rec(&mut w, K_FLAGS, &mut |w| w_list(w, s.flags(), 2))?;
        rec(&mut w, K_TAIL, &mut |w| {
            if s.n_score as usize > MAX_FIGHTERS {
                return Err(WireError::Cap);
            }
            w.write(s.n_score as u32, SLOT_BITS + 1)?;
            for &sc in &s.score[..s.n_score as usize] {
                w_i32(w, score_on_wire(sc), 16)?;
            }
            w.write(s.phase as u32, 2)?;
            w.write(s.clock, 32)?;
            w.write(s.winner as u32, 8)?;
            w_held(w, s.held)
        })?;
        rec(&mut w, K_DROPS, &mut |w| w_list(w, s.drops(), 4))?;
        rec(&mut w, K_PROJ, &mut |w| w_list(w, s.proj(), 5))?;
        rec(&mut w, K_EVENTS, &mut |w| w_list(w, s.events(), 6))?;
        for r in s.remotes() {
            if r.slot as usize >= MAX_FIGHTERS {
                return Err(WireError::Malformed);
            }
            rec(&mut w, K_REMOTE + r.slot, &mut |w| r.pack(w))?;
        }
        for v in s.vehicles() {
            if v.slot as usize >= MAX_SNAP_VEHICLES {
                return Err(WireError::Malformed);
            }
            rec(&mut w, K_VEHICLE + v.slot, &mut |w| v.pack(w))?;
        }
        self.n = n;
        self.tick = s.tick;
        Ok(())
    }

    fn record(&self, i: usize) -> &[u8] {
        let start = if i == 0 { 0 } else { self.ends[i - 1] as usize };
        &self.bytes[start..self.ends[i] as usize]
    }

    fn find(&self, key: u8) -> Option<&[u8]> {
        self.keys[..self.n].binary_search(&key).ok().map(|i| self.record(i))
    }

    /// The snapshot these records make.
    pub fn snapshot(&self) -> R<Snapshot> {
        let mut s = Snapshot {
            tick: self.tick,
            last_seq: self.last_seq,
            depth: self.depth,
            ..Snapshot::default()
        };
        let (mut own, mut tail) = (false, false);
        for i in 0..self.n {
            let key = self.keys[i];
            let mut r = BitReader::new(self.record(i));
            match key {
                K_OWN => {
                    s.own = r_own(&mut r)?;
                    own = true;
                }
                K_OWN_VEHICLE => s.own_vehicle = Some(r_vehicle(&mut r)?),
                K_PADS => s.n_pads = r_list(&mut r, &mut s.pads, 5)?,
                K_FLAGS => s.n_flags = r_list(&mut r, &mut s.flags, 2)?,
                K_TAIL => {
                    s.n_score = r.read(SLOT_BITS + 1)? as u8;
                    if s.n_score as usize > MAX_FIGHTERS {
                        return Err(WireError::Cap);
                    }
                    for sc in s.score[..s.n_score as usize].iter_mut() {
                        *sc = r_i32(&mut r, 16)?;
                    }
                    s.phase = r.read(2)? as u8;
                    s.clock = r.read(32)?;
                    s.winner = r.read(8)? as u8;
                    s.held = r_held(&mut r)?;
                    tail = true;
                }
                K_DROPS => s.n_drops = r_list(&mut r, &mut s.drops, 4)?,
                K_PROJ => s.n_proj = r_list(&mut r, &mut s.proj, 5)?,
                K_EVENTS => s.n_events = r_list(&mut r, &mut s.events, 6)?,
                k if (K_REMOTE..K_VEHICLE).contains(&k) => {
                    let x = Remote::unpack(&mut r)?;
                    if x.slot != k - K_REMOTE || s.n_remote as usize >= MAX_REMOTES {
                        return Err(WireError::Malformed);
                    }
                    s.remotes[s.n_remote as usize] = x;
                    s.n_remote += 1;
                }
                k if (K_VEHICLE as usize..MAX_KEYS).contains(&(k as usize)) => {
                    let v = VehState::unpack(&mut r)?;
                    if v.slot != k - K_VEHICLE {
                        return Err(WireError::Malformed);
                    }
                    s.vehicles[s.n_vehicles as usize] = v;
                    s.n_vehicles += 1;
                }
                _ => return Err(WireError::Malformed),
            }
            done(&r)?;
        }
        if !own || !tail {
            return Err(WireError::Malformed);
        }
        Ok(s)
    }
}

/// Bits `cur` takes XOR'd against `base` (the same length).
fn xor_bits(cur: &[u8], base: &[u8]) -> usize {
    cur.chunks(CHUNK)
        .zip(base.chunks(CHUNK))
        .map(|(c, b)| {
            let changed = c.iter().zip(b).filter(|(x, y)| x != y).count();
            if changed == 0 {
                1
            } else {
                1 + c.len() + 8 * changed
            }
        })
        .sum()
}

/// `cur` as a delta against `base`, into `out`: its bytes on the wire.
pub fn encode_delta(cur: &Keyed, base: &Keyed, out: &mut [u8]) -> R<usize> {
    let back = cur.tick.wrapping_sub(base.tick);
    if back == 0 || back >= DELTA_WINDOW {
        return Err(WireError::Range);
    }
    let mut w = BitWriter::new(out);
    w.write(KIND_DELTA, 4)?;
    w.write(cur.tick, 32)?;
    w.write(cur.last_seq, 32)?;
    w_i32(&mut w, cur.depth as i32, 8)?;
    w.write(back, 6)?;
    for i in 0..cur.n {
        let key = cur.keys[i];
        let rec = cur.record(i);
        w.write(key as u32, KEY_BITS)?;
        match base.find(key).filter(|b| b.len() == rec.len()) {
            Some(b) if xor_bits(rec, b) < LEN_BITS as usize + 8 * rec.len() => {
                w.write_bit(true)?;
                for (c, bc) in rec.chunks(CHUNK).zip(b.chunks(CHUNK)) {
                    let any = c != bc;
                    w.write_bit(any)?;
                    if !any {
                        continue;
                    }
                    for (x, y) in c.iter().zip(bc) {
                        w.write_bit(x != y)?;
                        if x != y {
                            w.write((x ^ y) as u32, 8)?;
                        }
                    }
                }
            }
            _ => {
                w.write_bit(false)?;
                w.write(rec.len() as u32, LEN_BITS)?;
                for &x in rec {
                    w.write(x as u32, 8)?;
                }
            }
        }
    }
    w.write(K_END, KEY_BITS)?;
    Ok(w.finish())
}

/// The tick a delta's base is, without decoding the rest.
pub fn delta_base(buf: &[u8]) -> R<u32> {
    let mut r = BitReader::new(buf);
    if r.read(4)? != KIND_DELTA {
        return Err(WireError::Malformed);
    }
    let tick = r.read(32)?;
    r.read(32)?;
    r.read(8)?;
    Ok(tick.wrapping_sub(r.read(6)?))
}

/// A delta made whole against `base` (the one [`delta_base`] names), into
/// `out`.
pub fn decode_delta(buf: &[u8], base: &Keyed, out: &mut Keyed) -> R<()> {
    let mut r = BitReader::new(buf);
    if r.read(4)? != KIND_DELTA {
        return Err(WireError::Malformed);
    }
    out.tick = r.read(32)?;
    out.last_seq = r.read(32)?;
    out.depth = r_i32(&mut r, 8)? as i8;
    let back = r.read(6)?;
    if back == 0 || base.tick != out.tick.wrapping_sub(back) {
        return Err(WireError::Malformed);
    }
    out.n = 0;
    let mut at = 0usize;
    loop {
        let key = r.read(KEY_BITS)?;
        if key == K_END {
            break;
        }
        let key = key as u8;
        if key as usize >= MAX_KEYS || (out.n > 0 && out.keys[out.n - 1] >= key) {
            return Err(WireError::Malformed);
        }
        if r.read_bit()? {
            let b = base.find(key).ok_or(WireError::Malformed)?;
            let rec = out.bytes.get_mut(at..at + b.len()).ok_or(WireError::Cap)?;
            rec.copy_from_slice(b);
            for c in rec.chunks_mut(CHUNK) {
                if !r.read_bit()? {
                    continue;
                }
                for x in c.iter_mut() {
                    if r.read_bit()? {
                        *x ^= r.read(8)? as u8;
                    }
                }
            }
            at += b.len();
        } else {
            let len = r.read(LEN_BITS)? as usize;
            let rec = out.bytes.get_mut(at..at + len).ok_or(WireError::Cap)?;
            for x in rec.iter_mut() {
                *x = r.read(8)? as u8;
            }
            at += len;
        }
        out.keys[out.n] = key;
        out.ends[out.n] = at as u16;
        out.n += 1;
    }
    done(&r)
}

/// The snapshots of the last [`DELTA_WINDOW`] ticks, cut into records: the
/// server's of what it sent a client, a client's of what it took in.
#[derive(Clone)]
pub struct Ring {
    slots: [Keyed; RING],
}

impl Default for Ring {
    fn default() -> Self {
        Ring {
            slots: core::array::from_fn(|_| Keyed::default()),
        }
    }
}

impl Ring {
    /// The slot `tick`'s records go in.
    pub fn slot(&mut self, tick: u32) -> &mut Keyed {
        &mut self.slots[(tick / 2) as usize % RING]
    }

    pub fn get(&self, tick: u32) -> Option<&Keyed> {
        Some(&self.slots[(tick / 2) as usize % RING]).filter(|k| k.tick == tick)
    }

    /// Forgets everything (a new room).
    pub fn clear(&mut self) {
        for k in self.slots.iter_mut() {
            k.tick = u32::MAX;
        }
    }
}

/// A client's end: a snapshot datagram, full or a delta, made whole, and
/// kept as a base for the deltas after it.
#[derive(Default)]
pub struct Receiver {
    ring: Box<Ring>,
    scratch: Box<Keyed>,
}

impl Receiver {
    pub fn clear(&mut self) {
        self.ring.clear();
    }

    /// The snapshot `d` carries: full, or a delta against one this kept.
    pub fn take(&mut self, d: &[u8]) -> R<Snapshot> {
        if datagram_kind(d) == Some(DG_DELTA) {
            let tick = delta_base(d)?;
            let base = self.ring.get(tick).ok_or(WireError::Malformed)?;
            decode_delta(d, base, &mut self.scratch)?;
            let s = self.scratch.snapshot()?;
            core::mem::swap(self.ring.slot(s.tick), &mut self.scratch);
            Ok(s)
        } else {
            let s = decode_snapshot(d)?;
            // A snapshot that can't be cut can't be a base: it is still drawn.
            let k = self.ring.slot(s.tick);
            if k.fill(&s).is_err() {
                k.tick = u32::MAX;
            }
            Ok(s)
        }
    }
}

//! Replays: a match as the calls that made it (docs/DESIGN.md §12). The sim
//! is deterministic, so the seed, the salt, the rules, the map, every seat
//! change and every tick's inputs and rewind depths are the whole match;
//! [`verify`] runs them through a fresh `World` and checks the final hash
//! the server wrote. Bots' inputs are recorded like anyone's, so a replay
//! never depends on the bot's code.
//!
//! Format, little-endian: `OSR5`, u16 PROTO_VER, u64 golden tag, u64 seed,
//! u64 salt × 2, the rules (teams, score limit, time limit, loadout, seats,
//! gun game, friendly fire, capture the flag, siege), u8 map; then records: `S` slot team (a seat),
//! `U` slot (an unseat), `T` u16 changed-input mask, inputs, u16
//! changed-rewind mask, rewinds (a tick); `E` u32 ticks, u64 hash, i32 × 16
//! scores (the end).

use proto::PROTO_VER;
use sim::consts::MAX_FIGHTERS;
use sim::events::Kind as Ev;
use sim::input::Input;
use sim::map::Map;
use sim::world::{Rules, World};

pub const MAGIC: &[u8; 4] = b"OSR5";
/// Which build of the rules a replay needs: the sim's determinism golden
/// (`crates/sim/tests/determinism.rs`). A replay from other rules is refused
/// rather than run wrong.
pub const RULES_TAG: u64 = 0xd9f4_2121_99eb_e3d0;

/// The map a replay names.
pub fn map_by_id(id: u8) -> Option<Map> {
    sim::map::by_id(id)
}

pub struct Recorder {
    buf: Vec<u8>,
    inputs: [Input; MAX_FIGHTERS],
    rewind: [u32; MAX_FIGHTERS],
    ticks: u32,
}

/// A tick's record with a few people moving: its tag, masks and inputs.
const TYPICAL_TICK_BYTES: usize = 48;

impl Recorder {
    pub fn new(seed: u64, salt: (u64, u64), rules: &Rules, map: u8) -> Recorder {
        // Room for a typical match up to its time limit, so the buffer
        // doesn't grow by copying megabytes on the sim thread mid-match
        // (pages not written are never touched).
        let ticks = rules.time_limit_ms as usize * sim::consts::TICK_HZ as usize / 1000;
        let mut buf = Vec::with_capacity((ticks * TYPICAL_TICK_BYTES).clamp(1 << 16, 8 << 20));
        buf.extend_from_slice(MAGIC);
        buf.extend_from_slice(&PROTO_VER.to_le_bytes());
        buf.extend_from_slice(&RULES_TAG.to_le_bytes());
        for w in [seed, salt.0, salt.1] {
            buf.extend_from_slice(&w.to_le_bytes());
        }
        buf.push(rules.teams as u8);
        buf.extend_from_slice(&rules.score_limit.to_le_bytes());
        buf.extend_from_slice(&rules.time_limit_ms.to_le_bytes());
        buf.extend_from_slice(&rules.loadout);
        buf.push(rules.seats);
        buf.push(rules.gun_game as u8);
        buf.push(rules.friendly_fire as u8);
        buf.push(rules.ctf as u8);
        buf.push(rules.siege as u8);
        buf.push(map);
        Recorder {
            buf,
            inputs: [Input::default(); MAX_FIGHTERS],
            rewind: [0; MAX_FIGHTERS],
            ticks: 0,
        }
    }

    pub fn seat(&mut self, slot: usize, team: u8) {
        self.buf.extend_from_slice(&[b'S', slot as u8, team]);
    }

    pub fn unseat(&mut self, slot: usize) {
        self.buf.extend_from_slice(&[b'U', slot as u8]);
    }

    /// One `World::step`'s arguments.
    pub fn step(&mut self, inputs: &[Input; MAX_FIGHTERS], rewind: &[u32; MAX_FIGHTERS]) {
        self.buf.push(b'T');
        let mut mask = 0u16;
        for (s, i) in inputs.iter().enumerate() {
            if *i != self.inputs[s] {
                mask |= 1 << s;
            }
        }
        self.buf.extend_from_slice(&mask.to_le_bytes());
        for (s, i) in inputs.iter().enumerate() {
            if mask & (1 << s) != 0 {
                self.buf.extend_from_slice(&i.buttons.to_le_bytes());
                self.buf.extend_from_slice(&i.yaw.to_le_bytes());
                self.buf.extend_from_slice(&i.pitch.to_le_bytes());
                self.buf.push(i.mx as u8);
                self.buf.push(i.mz as u8);
            }
        }
        let mut rmask = 0u16;
        for (s, &r) in rewind.iter().enumerate() {
            if r != self.rewind[s] {
                rmask |= 1 << s;
            }
        }
        self.buf.extend_from_slice(&rmask.to_le_bytes());
        for (s, &r) in rewind.iter().enumerate() {
            if rmask & (1 << s) != 0 {
                self.buf.extend_from_slice(&r.to_le_bytes());
            }
        }
        self.inputs = *inputs;
        self.rewind = *rewind;
        self.ticks += 1;
    }

    /// The file: everything so far, and the end as `world` now stands.
    pub fn finish(&self, world: &World) -> Vec<u8> {
        let mut out = self.buf.clone();
        out.push(b'E');
        out.extend_from_slice(&self.ticks.to_le_bytes());
        out.extend_from_slice(&world.hash().to_le_bytes());
        for s in world.score {
            out.extend_from_slice(&s.to_le_bytes());
        }
        out
    }
}

/// What a replay played out to.
#[derive(Debug)]
pub struct Summary {
    pub ticks: u32,
    pub hash: u64,
    pub score: [i32; MAX_FIGHTERS],
    pub seed: u64,
    pub salt: (u64, u64),
    /// (tick, killer, victim, cause) for every kill.
    pub kills: Vec<(u32, u8, u8, u16)>,
}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], String> {
        let s = self.b.get(self.at..self.at + n).ok_or("the file ends early")?;
        self.at += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
}

/// Plays a replay through a fresh `World` and checks it ends where the
/// server said it did.
pub fn verify(bytes: &[u8]) -> Result<Summary, String> {
    let mut r = Reader { b: bytes, at: 0 };
    if r.take(4)? != MAGIC {
        return Err("not a replay".into());
    }
    let ver = r.u16()?;
    let tag = r.u64()?;
    if tag != RULES_TAG {
        return Err(format!(
            "recorded under other rules ({tag:#x}, wire {ver}); this build is {RULES_TAG:#x}"
        ));
    }
    let seed = r.u64()?;
    let salt = (r.u64()?, r.u64()?);
    let rules = Rules {
        teams: r.u8()? != 0,
        score_limit: r.u32()? as i32,
        time_limit_ms: r.u32()?,
        loadout: [r.u8()?, r.u8()?],
        seats: r.u8()?,
        gun_game: r.u8()? != 0,
        friendly_fire: r.u8()? != 0,
        ctf: r.u8()? != 0,
        siege: r.u8()? != 0,
    };
    if rules.seats == 0 || rules.seats as usize > MAX_FIGHTERS {
        return Err("a seat count past the last".into());
    }
    let map = map_by_id(r.u8()?).ok_or("an unknown map")?;
    let mut w = World::new(map, rules, seed, salt);
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    let mut rewind = [0u32; MAX_FIGHTERS];
    let mut kills = Vec::new();
    let mut ticks = 0u32;
    loop {
        match r.u8()? {
            b'S' => {
                let (s, t) = (r.u8()? as usize, r.u8()?);
                if s >= MAX_FIGHTERS {
                    return Err("a seat past the last".into());
                }
                w.seat(s, t);
            }
            b'U' => {
                let s = r.u8()? as usize;
                if s >= MAX_FIGHTERS {
                    return Err("a seat past the last".into());
                }
                w.unseat(s);
            }
            b'T' => {
                let mask = r.u16()?;
                for (s, i) in inputs.iter_mut().enumerate() {
                    if mask & (1 << s) != 0 {
                        *i = Input {
                            buttons: r.u16()?,
                            yaw: r.u16()?,
                            pitch: r.u16()? as i16,
                            mx: r.u8()? as i8,
                            mz: r.u8()? as i8,
                        };
                    }
                }
                let rmask = r.u16()?;
                for (s, d) in rewind.iter_mut().enumerate() {
                    if rmask & (1 << s) != 0 {
                        *d = r.u32()?;
                    }
                }
                w.step(&inputs, &rewind);
                ticks += 1;
                for e in w.events.as_slice() {
                    if e.kind == Ev::Kill {
                        kills.push((w.tick, e.a, e.b, e.c));
                    }
                }
            }
            b'E' => {
                let want_ticks = r.u32()?;
                let want = r.u64()?;
                let mut score = [0i32; MAX_FIGHTERS];
                for s in score.iter_mut() {
                    *s = r.u32()? as i32;
                }
                if r.at != bytes.len() {
                    return Err("bytes after the end".into());
                }
                if want_ticks != ticks || want != w.hash() || score != w.score {
                    return Err(format!(
                        "it doesn't replay: {ticks} ticks to {:#018x}, the server said {want_ticks} to {want:#018x}",
                        w.hash()
                    ));
                }
                return Ok(Summary {
                    ticks,
                    hash: want,
                    score,
                    seed,
                    salt,
                    kills,
                });
            }
            k => return Err(format!("an unknown record {k:#x} at byte {}", r.at - 1)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Room;

    fn recorded() -> Vec<u8> {
        let rules = Rules {
            score_limit: 1000,
            time_limit_ms: 0,
            ..Rules::doubles()
        };
        let mut room = Room::new(Map::proving_grounds(), rules, 9, (4, 5));
        for t in 0..6000 {
            room.step(t as f64);
        }
        room.replay()
    }

    #[test]
    fn a_bot_match_replays_to_the_same_hash() {
        let bytes = recorded();
        let s = verify(&bytes).unwrap();
        assert_eq!(s.ticks, 6000);
        assert!(!s.kills.is_empty(), "the bots fought");
        // Small: a changed input costs 8 bytes, an unchanged one nothing.
        assert!(bytes.len() < 6000 * 40, "{} bytes", bytes.len());
    }

    #[test]
    fn edited_inputs_are_caught() {
        let mut bytes = recorded();
        // Walk the records; past the middle of the match, turn every changed
        // input's forward stick around.
        let mut at = 4 + 2 + 8 + 24 + 1 + 4 + 4 + 2 + 1 + 1 + 1 + 1 + 1 + 1;
        let mut ticks = 0;
        let mut edited = 0;
        while bytes[at] != b'E' {
            match bytes[at] {
                b'S' => at += 3,
                b'U' => at += 2,
                b'T' => {
                    let mask = u16::from_le_bytes([bytes[at + 1], bytes[at + 2]]);
                    at += 3;
                    for _ in 0..mask.count_ones() {
                        if ticks > 3000 {
                            bytes[at + 7] ^= 0x80;
                            edited += 1;
                        }
                        at += 8;
                    }
                    let rmask = u16::from_le_bytes([bytes[at], bytes[at + 1]]);
                    at += 2 + 4 * rmask.count_ones() as usize;
                    ticks += 1;
                }
                k => panic!("record {k:#x}"),
            }
        }
        assert!(edited > 100);
        assert!(verify(&bytes).is_err());
        // And a byte of the end record.
        let mut bytes = recorded();
        let n = bytes.len();
        bytes[n - 40] ^= 1;
        assert!(verify(&bytes).is_err());
    }
}

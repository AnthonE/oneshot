//! The wire: what the browser and the server say to each other
//! (docs/DESIGN.md §10). Datagrams carry inputs up and snapshots down; one
//! bidirectional stream carries the handshake, the kill feed and the
//! match's phases, each message in a [`frame`].
//!
//! Every message is bit-packed by [`bits`] into a caller's buffer. Decoders
//! are strict: a bad kind, a count over its cap, a value out of range or
//! trailing bytes is an error, never a guess. A layout change bumps
//! [`PROTO_VER`] and moves the goldens in `tests/goldens.rs`.

pub mod bits;
pub mod delta;
pub mod frame;
pub mod medal;
pub mod party;
pub mod voice;

use bits::{BitReader, BitWriter, WireError};
use party::{Party, PlayerId};
use sim::consts::MAX_FIGHTERS;
use sim::fighter::{Fighter, Gun, Mount};
use sim::geom::V3;
use sim::input::{Input, BUTTONS, BUTTON_BITS};
use sim::movement::Body;
use sim::vehicle::Vehicle;
use sim::world::Rules;

pub const PROTO_VER: u16 = 34;
/// What every lobby ticket starts with (the server makes them, `auth.rs`;
/// the client reads one's own fields back to know whose it is).
pub const TICKET_PREFIX: &str = "os1.";
/// Under QUIC's 1,200-byte floor with room for its headers.
pub const MAX_DATAGRAM: usize = 1100;
/// Input frames one datagram resends: everything the server hasn't run.
pub const MAX_INPUT_FRAMES: usize = 16;
/// Everyone but yourself.
pub const MAX_REMOTES: usize = MAX_FIGHTERS - 1;
pub const MAX_SNAP_PROJ: usize = 16;
pub const MAX_SNAP_EVENTS: usize = 32;
pub const MAX_SNAP_PADS: usize = sim::consts::MAX_PADS;
pub const MAX_SNAP_VEHICLES: usize = sim::consts::MAX_VEHICLES;
/// Guns on the floor a snapshot names, nearest first (`room::build`).
pub const MAX_SNAP_DROPS: usize = 8;
/// Bits for a seat number on the wire: a remote's slot, a projectile's
/// owner, an event's two fighters.
pub const SLOT_BITS: u32 = 4;
/// An event's `a` or `b` that is no fighter (`sim::world::NO_WINNER`): a
/// flag gone home on its own, a vehicle lost to nobody. On the wire, one
/// bit says whether a seat follows; any value past the seats goes as
/// this.
pub const NOBODY: u8 = 255;
/// A stream message's ceiling: a hello with an Elo Pros pass is the
/// largest.
pub const MAX_STREAM_MSG: usize = 1024;
/// An Elo Pros pass's ceiling (Gates' `PASS_MAX` is 600).
pub const MAX_PASS: usize = 640;
pub const MAX_NAME: usize = 16;

const KIND_INPUT: u32 = 0;
const KIND_SNAPSHOT: u32 = 1;

/// Remote positions on a 1/64 m grid, velocities in 1/64 m/s.
const COARSE: f32 = 64.0;

type R<T> = Result<T, WireError>;

fn w_i32(w: &mut BitWriter, v: i32, bits: u32) -> R<()> {
    let lim = 1i64 << (bits - 1);
    if (v as i64) < -lim || (v as i64) >= lim {
        return Err(WireError::Range);
    }
    let mask = if bits == 32 { u32::MAX } else { (1u32 << bits) - 1 };
    w.write(v as u32 & mask, bits)
}

/// A score as its 16 bits carry it: pinned at their ends (a match with no
/// limit left running all night), never a snapshot that fails to encode.
pub fn score_on_wire(sc: i32) -> i32 {
    sc.clamp(i16::MIN as i32, i16::MAX as i32)
}

fn r_i32(r: &mut BitReader, bits: u32) -> R<i32> {
    let v = r.read(bits)?;
    let shift = 32 - bits;
    Ok(((v << shift) as i32) >> shift)
}

pub(crate) fn done(r: &BitReader) -> R<()> {
    if r.remaining_bits() >= 8 {
        Err(WireError::Malformed)
    } else {
        Ok(())
    }
}

/// A float on the sim's 1/1024 grid, sent exactly.
fn w_grid(w: &mut BitWriter, v: f32) -> R<()> {
    w_i32(w, sim::fmath::round(v * sim::consts::QUANT) as i32, 32)
}

fn r_grid(r: &mut BitReader) -> R<f32> {
    Ok(r_i32(r, 32)? as f32 * (1.0 / sim::consts::QUANT))
}

/// A bubble member's place and speed, exactly (the sim's grid) in fewer
/// bits than `w_grid`: ±8192 m, ±512 m/s.
const EXACT_POS_BITS: u32 = 24;
const EXACT_VEL_BITS: u32 = 20;

/// Whether a body or hull at `pos`, moving at `vel`, can go out exactly
/// (`VehState::near`, `Remote::near`).
pub fn exact_fits(pos: V3, vel: V3) -> bool {
    let fits = |v: f32, bits: u32| {
        let q = sim::fmath::round(v * sim::consts::QUANT);
        q.is_finite() && q.abs() < (1u32 << (bits - 1)) as f32
    };
    [pos.x, pos.y, pos.z].iter().all(|&v| fits(v, EXACT_POS_BITS))
        && [vel.x, vel.y, vel.z].iter().all(|&v| fits(v, EXACT_VEL_BITS))
}

fn w_exact(w: &mut BitWriter, pos: V3, vel: V3) -> R<()> {
    for c in [pos.x, pos.y, pos.z] {
        w_i32(w, sim::fmath::round(c * sim::consts::QUANT) as i32, EXACT_POS_BITS)?;
    }
    for c in [vel.x, vel.y, vel.z] {
        w_i32(w, sim::fmath::round(c * sim::consts::QUANT) as i32, EXACT_VEL_BITS)?;
    }
    Ok(())
}

fn r_exact(r: &mut BitReader) -> R<(V3, V3)> {
    let mut c = [0.0f32; 6];
    for (k, v) in c.iter_mut().enumerate() {
        let bits = if k < 3 { EXACT_POS_BITS } else { EXACT_VEL_BITS };
        *v = r_i32(r, bits)? as f32 * (1.0 / sim::consts::QUANT);
    }
    Ok((V3::new(c[0], c[1], c[2]), V3::new(c[3], c[4], c[5])))
}

/// A share of a shooter's rewind (`sim::world::Seen`), 0..=15.
fn w_share(w: &mut BitWriter, share: u8) -> R<()> {
    w.write(share.min(sim::consts::SHARE_FULL) as u32, 4)
}

fn r_share(r: &mut BitReader) -> R<u8> {
    let s = r.read(4)? as u8;
    if s > sim::consts::SHARE_FULL {
        return Err(WireError::Malformed);
    }
    Ok(s)
}

fn w_coarse(w: &mut BitWriter, v: f32) -> R<()> {
    let q = sim::fmath::round(v * COARSE).clamp(-32768.0, 32767.0);
    w_i32(w, q as i32, 16)
}

fn r_coarse(r: &mut BitReader) -> R<f32> {
    Ok(r_i32(r, 16)? as f32 * (1.0 / COARSE))
}

fn w_v3c(w: &mut BitWriter, v: V3) -> R<()> {
    w_coarse(w, v.x)?;
    w_coarse(w, v.y)?;
    w_coarse(w, v.z)
}

fn r_v3c(r: &mut BitReader) -> R<V3> {
    Ok(V3::new(r_coarse(r)?, r_coarse(r)?, r_coarse(r)?))
}

// ---------------------------------------------------------------- inputs

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InputMsg {
    /// The newest snapshot tick this client has.
    pub ack_tick: u32,
    /// How far behind its newest snapshot it draws others, in ticks. The
    /// server clamps it; it never decides a rewind on its own.
    pub playout: u8,
    /// The seq of `frames[0]`; the rest follow one a tick.
    pub first_seq: u32,
    pub n: u8,
    pub frames: [Input; MAX_INPUT_FRAMES],
}

impl Default for InputMsg {
    fn default() -> Self {
        InputMsg {
            ack_tick: 0,
            playout: 0,
            first_seq: 0,
            n: 0,
            frames: [Input::default(); MAX_INPUT_FRAMES],
        }
    }
}

impl InputMsg {
    pub fn frames(&self) -> &[Input] {
        &self.frames[..self.n as usize]
    }
}

pub fn encode_input(m: &InputMsg, out: &mut [u8]) -> R<usize> {
    if m.n as usize > MAX_INPUT_FRAMES {
        return Err(WireError::Cap);
    }
    let mut w = BitWriter::new(out);
    w.write(KIND_INPUT, 4)?;
    w.write(m.ack_tick, 32)?;
    w.write(m.playout as u32, 8)?;
    w.write(m.first_seq, 32)?;
    w.write(m.n as u32, 5)?;
    for f in m.frames() {
        w.write((f.buttons & BUTTONS) as u32, BUTTON_BITS)?;
        w.write(f.yaw as u32, 16)?;
        w.write(f.pitch as u16 as u32, 16)?;
        w.write(f.mx as u8 as u32, 8)?;
        w.write(f.mz as u8 as u32, 8)?;
    }
    Ok(w.finish())
}

pub fn decode_input(buf: &[u8]) -> R<InputMsg> {
    let mut r = BitReader::new(buf);
    if r.read(4)? != KIND_INPUT {
        return Err(WireError::Malformed);
    }
    let mut m = InputMsg {
        ack_tick: r.read(32)?,
        playout: r.read(8)? as u8,
        first_seq: r.read(32)?,
        n: r.read(5)? as u8,
        ..InputMsg::default()
    };
    if m.n as usize > MAX_INPUT_FRAMES {
        return Err(WireError::Cap);
    }
    for i in 0..m.n as usize {
        m.frames[i] = Input {
            buttons: r.read(BUTTON_BITS)? as u16,
            yaw: r.read(16)? as u16,
            pitch: r.read(16)? as u16 as i16,
            mx: r.read(8)? as u8 as i8,
            mz: r.read(8)? as u8 as i8,
        };
    }
    done(&r)?;
    Ok(m)
}

/// Which kind a datagram is, without decoding it.
pub fn datagram_kind(buf: &[u8]) -> Option<u8> {
    buf.first().map(|b| b & 0xf)
}
pub const DG_INPUT: u8 = KIND_INPUT as u8;
pub const DG_SNAPSHOT: u8 = KIND_SNAPSHOT as u8;
/// A snapshot as what changed since one the client acked ([`delta`]).
pub const DG_DELTA: u8 = delta::KIND_DELTA as u8;
/// Party voice, both ways, on a lobby session ([`voice`]).
pub const DG_VOICE: u8 = voice::KIND_VOICE as u8;

// ------------------------------------------------------------- snapshots

/// A thing's wire form: written and read field by field, the same bits
/// both ways. A new kind on the wire is an impl of this, a count in the
/// snapshot, and a line in the leak gate.
pub trait Pack: Sized {
    fn pack(&self, w: &mut BitWriter) -> R<()>;
    fn unpack(r: &mut BitReader) -> R<Self>;
}

/// `items`, counted in `bits` bits.
fn w_list<T: Pack>(w: &mut BitWriter, items: &[T], bits: u32) -> R<()> {
    w.write(items.len() as u32, bits)?;
    for t in items {
        t.pack(w)?;
    }
    Ok(())
}

/// A list counted in `bits` bits into `out`; more than `out` holds is
/// refused. Returns the count.
fn r_list<T: Pack>(r: &mut BitReader, out: &mut [T], bits: u32) -> R<u8> {
    let n = r.read(bits)? as usize;
    if n > out.len() {
        return Err(WireError::Cap);
    }
    for t in out[..n].iter_mut() {
        *t = T::unpack(r)?;
    }
    Ok(n as u8)
}

/// Another fighter as this client may draw it: coarse, no ammo, no timers.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Remote {
    pub slot: u8,
    pub team: u8,
    pub alive: bool,
    pub crouched: bool,
    pub grounded: bool,
    pub reloading: bool,
    pub pos: V3,
    pub vel: V3,
    pub yaw: u16,
    pub pitch: i16,
    pub arm: u8,
    /// Shields as a fraction of full, 0..255, for the flare.
    pub shield: u8,
    /// An overshield on (climbing, or over full).
    pub over: bool,
    /// How hidden its camouflage keeps it (`Fighter::cloak`), 0 to
    /// [`CLOAK_STEPS`] of all the way: what this client draws it at.
    pub cloak: u8,
    /// Carrying a flag (Capture the Flag).
    pub flag: bool,
    /// In this client's bubble (`room::bubble`): `pos` and `vel` exact,
    /// and what it predicts the body on.
    pub near: Option<NearBody>,
}

/// What a client predicts a body in its bubble on: its crouch as far as it
/// has gone, the input it last ran (the stick, a jump, a crouch held), and
/// its share of this client's rewind (`sim::world::Seen`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NearBody {
    pub duck: u8,
    pub mx: i8,
    pub mz: i8,
    pub jump: bool,
    pub crouch: bool,
    pub share: u8,
}

/// What a client predicts a hull in its bubble on: its footing, the stick
/// its driver drives it by (None: parked), and its share of this client's
/// rewind.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NearHull {
    pub grounded: bool,
    pub stick: Option<(i8, i8)>,
    pub share: u8,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ProjState {
    /// `sim::world::ProjKind` as a number: 0 rocket, 1 frag, 2 ion bolt,
    /// 3 charged bolt, 4 shard, 5 launcher grenade, 6 cannon ball, 7 sticky.
    pub kind: u8,
    pub owner: u8,
    pub pos: V3,
    /// Its velocity; for a sticky stuck to someone, its offset from them
    /// (from a fighter's feet, a vehicle's middle), as the sim keeps it.
    /// The client finds whom it rides by it (`client_net::proj`).
    pub vel: V3,
}

/// A cosmetic event (sim::events::Event, coarsened).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EventState {
    /// Counts up per client: an event comes again in every snapshot until
    /// one that carried it is acked, and the client keeps the first.
    pub id: u16,
    pub kind: u8,
    /// Its two fighters' seats (`sim::events::Kind`), or [`NOBODY`].
    pub a: u8,
    pub b: u8,
    pub part: u8,
    pub c: u16,
    pub pos: V3,
    pub end: V3,
}

/// Capture the Flag's flag, as every client may know it: at home or on
/// the floor, where it is; carried, by whom, never where (the carrier is
/// drawn with it when fog shows them).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FlagState {
    /// `sim::world::FLAG_HOME`, `FLAG_HELD` or `FLAG_DOWN`.
    pub state: u8,
    /// The carrier's seat, while held.
    pub carrier: u8,
    /// Zero while held.
    pub pos: V3,
    /// Whole seconds till a dropped flag goes home.
    pub down_s: u8,
}

impl Pack for FlagState {
    fn pack(&self, w: &mut BitWriter) -> R<()> {
        // Held: who, never where. At home or down: where (and down, how
        // long till it goes home).
        w.write(self.state as u32, 2)?;
        if self.state == 1 {
            return w.write(self.carrier as u32, SLOT_BITS);
        }
        w_v3c(w, self.pos)?;
        if self.state == 2 {
            w.write(self.down_s as u32, 6)?;
        }
        Ok(())
    }
    fn unpack(r: &mut BitReader) -> R<Self> {
        let mut f = FlagState {
            state: r.read(2)? as u8,
            ..FlagState::default()
        };
        match f.state {
            0 => f.pos = r_v3c(r)?,
            1 => f.carrier = r.read(SLOT_BITS)? as u8,
            2 => {
                f.pos = r_v3c(r)?;
                f.down_s = r.read(6)? as u8;
            }
            _ => return Err(WireError::Malformed),
        }
        Ok(f)
    }
}

/// A Siege node (a generator or a core), as every client may know it:
/// how far its overload has gone, whether it is broken, and what its ring
/// holds (the HUD's "under attack", as Halo's territories show it). A
/// core's shield is down once its side's generators are all broken, or in
/// sudden death (the clock at 0 while playing): the client works it out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NodeState {
    /// The overload as a fraction of full, 0..[`NODE_LOAD_MAX`].
    pub load: u8,
    pub broken: bool,
    /// `sim::world::NODE_QUIET` and the rest.
    pub held: u8,
}

/// A node's overload goes in 6 bits: full is this.
pub const NODE_LOAD_MAX: u8 = 63;

impl Pack for NodeState {
    fn pack(&self, w: &mut BitWriter) -> R<()> {
        w.write_bit(self.broken)?;
        if self.broken {
            return Ok(());
        }
        w.write(self.load.min(NODE_LOAD_MAX) as u32, 6)?;
        w.write(self.held as u32, 2)
    }
    fn unpack(r: &mut BitReader) -> R<Self> {
        if r.read_bit()? {
            return Ok(NodeState {
                broken: true,
                load: NODE_LOAD_MAX,
                held: 0,
            });
        }
        Ok(NodeState {
            load: r.read(6)? as u8,
            held: r.read(2)? as u8,
            broken: false,
        })
    }
}

/// A Siege minion, as this client may see it (its own side's, or one in
/// clear sight; the nearest [`MAX_SNAP_MINIONS`]): its slot (so a drawn one
/// is kept), its side, where it stands (coarse) and which way it faces.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MinionState {
    pub slot: u8,
    pub team: u8,
    pub pos: V3,
    /// Its yaw's top 6 bits.
    pub yaw: u8,
}

/// Minions a snapshot carries at most, and the bits of their count.
pub const MAX_SNAP_MINIONS: usize = 8;
pub(crate) const MINION_BITS: u32 = 4;
const _: () = assert!(MAX_SNAP_MINIONS < 1 << MINION_BITS);
const _: () = assert!(sim::consts::MAX_MINIONS <= 16, "a minion's slot is 4 bits");

impl Pack for MinionState {
    fn pack(&self, w: &mut BitWriter) -> R<()> {
        w.write(self.slot as u32, 4)?;
        w.write(self.team as u32 & 1, 1)?;
        w_v3c(w, self.pos)?;
        w.write(self.yaw as u32, 6)
    }
    fn unpack(r: &mut BitReader) -> R<Self> {
        Ok(MinionState {
            slot: r.read(4)? as u8,
            team: r.read(1)? as u8,
            pos: r_v3c(r)?,
            yaw: r.read(6)? as u8,
        })
    }
}

/// Siege's nodes in a list's count.
pub(crate) const NODE_BITS: u32 = 4;
const _: () = assert!(sim::consts::SIEGE_NODES < 1 << NODE_BITS);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PadState {
    /// Whole seconds until the gun (or power-up) is back, to
    /// [`PAD_S_MAX`]; 0 means it's there.
    pub empty_s: u16,
}

/// The longest wait a pad's timer says on the wire (8 bits, over 4
/// minutes; no pad waits more than 2).
pub const PAD_S_MAX: u16 = 255;
/// The most camouflage ticks the own fighter's field says (14 bits).
const CAMO_TICKS_MAX: u16 = (1 << 14) - 1;
const _: () = assert!(sim::consts::ticks(sim::consts::CAMO_MS) <= CAMO_TICKS_MAX as u32);
const _: () = assert!(sim::consts::ticks(sim::consts::CAMO_FLARE_MS) <= u8::MAX as u32);
/// How finely a remote's camouflage is told (3 bits).
pub const CLOAK_STEPS: f32 = 7.0;

impl Remote {
    /// `f` in seat `slot`, coarsened: what a client may draw of it.
    pub fn of(slot: u8, f: &Fighter) -> Remote {
        let full = sim::consts::SHIELD_MAX as i64;
        Remote {
            slot,
            team: f.team,
            alive: f.alive,
            crouched: f.body.crouched,
            grounded: f.body.grounded,
            reloading: f.reload > 0,
            pos: f.body.pos,
            vel: f.body.vel,
            yaw: f.yaw,
            pitch: f.pitch,
            arm: f.gun().arm,
            shield: (f.shield.max(0) as i64 * 255 / full).min(255) as u8,
            over: f.overshield(),
            cloak: (f.cloak() * CLOAK_STEPS + 0.5) as u8,
            flag: f.flag,
            near: None,
        }
    }
}

impl Pack for Remote {
    fn pack(&self, w: &mut BitWriter) -> R<()> {
        w.write(self.slot as u32, SLOT_BITS)?;
        w.write(self.team as u32 & 1, 1)?;
        w.write_bit(self.alive)?;
        w.write_bit(self.crouched)?;
        w.write_bit(self.grounded)?;
        w.write_bit(self.reloading)?;
        w.write_bit(self.near.is_some())?;
        if self.near.is_some() {
            w_exact(w, self.pos, self.vel)?;
        } else {
            w_v3c(w, self.pos)?;
            w_v3c(w, self.vel)?;
        }
        w.write(self.yaw as u32, 16)?;
        w.write(self.pitch as u16 as u32, 16)?;
        w.write(self.arm as u32, 4)?;
        w.write(self.shield as u32, 8)?;
        w.write_bit(self.over)?;
        w.write(self.cloak as u32, 3)?;
        w.write_bit(self.flag)?;
        if let Some(n) = self.near {
            w.write(n.duck as u32, 5)?;
            w.write(n.mx as u8 as u32, 8)?;
            w.write(n.mz as u8 as u32, 8)?;
            w.write_bit(n.jump)?;
            w.write_bit(n.crouch)?;
            w_share(w, n.share)?;
        }
        Ok(())
    }
    fn unpack(r: &mut BitReader) -> R<Self> {
        let (slot, team) = (r.read(SLOT_BITS)? as u8, r.read(1)? as u8);
        let (alive, crouched, grounded, reloading) = (r.read_bit()?, r.read_bit()?, r.read_bit()?, r.read_bit()?);
        let near = r.read_bit()?;
        let (pos, vel) = if near { r_exact(r)? } else { (r_v3c(r)?, r_v3c(r)?) };
        let mut x = Remote {
            slot,
            team,
            alive,
            crouched,
            grounded,
            reloading,
            pos,
            vel,
            yaw: r.read(16)? as u16,
            pitch: r.read(16)? as u16 as i16,
            arm: r.read(4)? as u8,
            shield: r.read(8)? as u8,
            over: r.read_bit()?,
            cloak: r.read(3)? as u8,
            flag: r.read_bit()?,
            near: None,
        };
        if near {
            let n = NearBody {
                duck: r.read(5)? as u8,
                mx: r.read(8)? as u8 as i8,
                mz: r.read(8)? as u8 as i8,
                jump: r.read_bit()?,
                crouch: r.read_bit()?,
                share: r_share(r)?,
            };
            // As the server runs it: a crouch never past its full, a stick
            // never -128 (`Input::sanitized`).
            if n.duck > sim::consts::CROUCH_TICKS || n.mx == i8::MIN || n.mz == i8::MIN {
                return Err(WireError::Malformed);
            }
            x.near = Some(n);
        }
        Ok(x)
    }
}

impl Pack for ProjState {
    fn pack(&self, w: &mut BitWriter) -> R<()> {
        w.write(self.kind as u32, 3)?;
        w.write(self.owner as u32, SLOT_BITS)?;
        w_v3c(w, self.pos)?;
        w_v3c(w, self.vel)
    }
    fn unpack(r: &mut BitReader) -> R<Self> {
        Ok(ProjState {
            kind: r.read(3)? as u8,
            owner: r.read(SLOT_BITS)? as u8,
            pos: r_v3c(r)?,
            vel: r_v3c(r)?,
        })
    }
}

/// An event's fighter: a seat, or [`NOBODY`].
fn w_who(w: &mut BitWriter, q: u8) -> R<()> {
    let seat = (q as usize) < MAX_FIGHTERS;
    w.write_bit(seat)?;
    if seat {
        w.write(q as u32, SLOT_BITS)?;
    }
    Ok(())
}

fn r_who(r: &mut BitReader) -> R<u8> {
    Ok(if r.read_bit()? {
        r.read(SLOT_BITS)? as u8
    } else {
        NOBODY
    })
}

impl Pack for EventState {
    fn pack(&self, w: &mut BitWriter) -> R<()> {
        w.write(self.id as u32, 16)?;
        w.write(self.kind as u32, 4)?;
        w_who(w, self.a)?;
        w_who(w, self.b)?;
        w.write(self.part as u32, 3)?;
        w.write(self.c as u32, 16)?;
        w_v3c(w, self.pos)?;
        w_v3c(w, self.end)
    }
    fn unpack(r: &mut BitReader) -> R<Self> {
        Ok(EventState {
            id: r.read(16)? as u16,
            kind: r.read(4)? as u8,
            a: r_who(r)?,
            b: r_who(r)?,
            part: r.read(3)? as u8,
            c: r.read(16)? as u16,
            pos: r_v3c(r)?,
            end: r_v3c(r)?,
        })
    }
}

/// A vehicle as this client may draw it: coarse, health as a fraction, and
/// who rides only when this client may know them.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VehState {
    pub slot: u8,
    pub pos: V3,
    pub vel: V3,
    pub yaw: u16,
    /// Health as a fraction of full, 0..255.
    pub health: u8,
    /// Each seat's rider's slot + 1, or 0 (nobody, or nobody it may know).
    pub seats: [u8; 2],
    /// In this client's bubble (`room::bubble`): `pos` and `vel` exact,
    /// and what it predicts the hull on.
    pub near: Option<NearHull>,
}

impl Pack for VehState {
    fn pack(&self, w: &mut BitWriter) -> R<()> {
        w.write(self.slot as u32, 3)?;
        w.write_bit(self.near.is_some())?;
        if self.near.is_some() {
            w_exact(w, self.pos, self.vel)?;
        } else {
            w_v3c(w, self.pos)?;
            w_v3c(w, self.vel)?;
        }
        w.write(self.yaw as u32, 16)?;
        w.write(self.health as u32, 8)?;
        for &s in &self.seats {
            w.write(s as u32, SLOT_BITS + 1)?;
        }
        if let Some(n) = self.near {
            w.write_bit(n.grounded)?;
            w_drive(w, n.stick)?;
            w_share(w, n.share)?;
        }
        Ok(())
    }
    fn unpack(r: &mut BitReader) -> R<Self> {
        let slot = r.read(3)? as u8;
        let near = r.read_bit()?;
        let (pos, vel) = if near { r_exact(r)? } else { (r_v3c(r)?, r_v3c(r)?) };
        let mut v = VehState {
            slot,
            pos,
            vel,
            yaw: r.read(16)? as u16,
            health: r.read(8)? as u8,
            seats: [0; 2],
            near: None,
        };
        for s in v.seats.iter_mut() {
            *s = r.read(SLOT_BITS + 1)? as u8;
            if *s as usize > MAX_FIGHTERS {
                return Err(WireError::Malformed);
            }
        }
        if near {
            v.near = Some(NearHull {
                grounded: r.read_bit()?,
                stick: r_drive(r)?,
                share: r_share(r)?,
            });
        }
        Ok(v)
    }
}

/// The vehicle this client rides in, exactly: what a driver's
/// reconciliation adopts.
fn w_vehicle(w: &mut BitWriter, v: &Vehicle) -> R<()> {
    w.write_bit(v.active)?;
    for c in [v.pos.x, v.pos.y, v.pos.z, v.vel.x, v.vel.y, v.vel.z] {
        w_grid(w, c)?;
    }
    w.write(v.yaw as u32, 16)?;
    w.write_bit(v.grounded)?;
    w_i32(w, v.health, 32)?;
    for &s in &v.seats {
        w.write(s as u32, SLOT_BITS + 1)?;
    }
    w.write(v.respawn, 32)?;
    w.write(v.home as u32, 8 - TURRET_BITS)?;
    w.write(v.turret as u32, TURRET_BITS)
}

/// A hull's turret kind (`sim::vehicle::TURRETS`), in the byte its home
/// had to itself.
const TURRET_BITS: u32 = 2;
const _: () = assert!((sim::vehicle::TURRETS as u32) <= 1 << TURRET_BITS);
const _: () = assert!(sim::consts::MAX_VEHICLES <= 1 << (8 - TURRET_BITS));

/// A passenger's driver's stick ([`Snapshot::drive`]), after its hull.
fn w_drive(w: &mut BitWriter, d: Option<(i8, i8)>) -> R<()> {
    w.write_bit(d.is_some())?;
    if let Some((mx, mz)) = d {
        w.write(mx as u8 as u32, 8)?;
        w.write(mz as u8 as u32, 8)?;
    }
    Ok(())
}

/// The server runs a stick sanitized (`Input::sanitized`): never -128.
fn r_drive(r: &mut BitReader) -> R<Option<(i8, i8)>> {
    if !r.read_bit()? {
        return Ok(None);
    }
    let (mx, mz) = (r.read(8)? as u8 as i8, r.read(8)? as u8 as i8);
    if mx == i8::MIN || mz == i8::MIN {
        return Err(WireError::Malformed);
    }
    Ok(Some((mx, mz)))
}

fn r_vehicle(r: &mut BitReader) -> R<Vehicle> {
    let active = r.read_bit()?;
    let pos = V3::new(r_grid(r)?, r_grid(r)?, r_grid(r)?);
    let vel = V3::new(r_grid(r)?, r_grid(r)?, r_grid(r)?);
    let mut v = Vehicle {
        active,
        pos,
        vel,
        yaw: r.read(16)? as u16,
        grounded: r.read_bit()?,
        health: r_i32(r, 32)?,
        ..Vehicle::default()
    };
    for s in v.seats.iter_mut() {
        *s = r.read(SLOT_BITS + 1)? as u8;
    }
    v.respawn = r.read(32)?;
    v.home = r.read(8 - TURRET_BITS)? as u8;
    v.turret = r.read(TURRET_BITS)? as u8;
    if v.turret >= sim::vehicle::TURRETS {
        return Err(WireError::Malformed);
    }
    Ok(v)
}

/// A gun on the floor, as this client may see it: its slot (so a drawn one
/// is kept), the gun (`sim::arms::NONE`: grenades), where it lies, coarse.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DropState {
    pub slot: u8,
    pub arm: u8,
    pub pos: V3,
}

impl Pack for DropState {
    fn pack(&self, w: &mut BitWriter) -> R<()> {
        w.write(self.slot as u32, 5)?;
        w.write(self.arm as u32, 4)?;
        w_v3c(w, self.pos)
    }
    fn unpack(r: &mut BitReader) -> R<Self> {
        let d = DropState {
            slot: r.read(5)? as u8,
            arm: r.read(4)? as u8,
            pos: r_v3c(r)?,
        };
        if d.slot as usize >= sim::consts::MAX_DROPS || d.arm as usize >= sim::arms::COUNT {
            return Err(WireError::Malformed);
        }
        Ok(d)
    }
}

impl Pack for PadState {
    fn pack(&self, w: &mut BitWriter) -> R<()> {
        w.write(self.empty_s.min(PAD_S_MAX) as u32, 8)
    }
    fn unpack(r: &mut BitReader) -> R<Self> {
        Ok(PadState {
            empty_s: r.read(8)? as u16,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Snapshot {
    pub tick: u32,
    /// This client's last input seq the server ran.
    pub last_seq: u32,
    /// The fewest frames of this client's waiting in the server's buffer
    /// over the ticks since the last snapshot (−1: a tick ran dry), what
    /// its clock steers on (`room::ClientNet::take_low`).
    pub depth: i8,
    /// This client's own fighter, exactly: what reconciliation adopts.
    pub own: Fighter,
    /// The vehicle it rides in, exactly, if it rides.
    pub own_vehicle: Option<Vehicle>,
    /// Riding behind someone (not at the wheel), the stick (`mx`, `mz`) its
    /// driver drove the hull by on this tick: the passenger predicts the
    /// hull on it, as kept on (`sim::world::World::drive_stick`). None at the
    /// wheel, on foot, or in a hull nobody drives (parked). Only ever
    /// with `own_vehicle`.
    pub drive: Option<(i8, i8)>,
    pub n_remote: u8,
    pub remotes: [Remote; MAX_REMOTES],
    pub n_proj: u8,
    pub proj: [ProjState; MAX_SNAP_PROJ],
    pub n_events: u8,
    pub events: [EventState; MAX_SNAP_EVENTS],
    pub n_pads: u8,
    pub pads: [PadState; MAX_SNAP_PADS],
    pub n_vehicles: u8,
    pub vehicles: [VehState; MAX_SNAP_VEHICLES],
    pub n_drops: u8,
    pub drops: [DropState; MAX_SNAP_DROPS],
    /// Capture the Flag's two flags, red's then blue's; none in other modes.
    pub n_flags: u8,
    pub flags: [FlagState; 2],
    /// Siege's nodes (`sim::world::World::nodes`); none in other modes.
    pub n_nodes: u8,
    pub nodes: [NodeState; sim::consts::SIEGE_NODES],
    /// Siege's minions this client may see; none in other modes.
    pub n_minions: u8,
    pub minions: [MinionState; MAX_SNAP_MINIONS],
    /// Scores in use: each side's (2) with teams, else each seat's. The
    /// rest are 0 and not sent.
    pub n_score: u8,
    pub score: [i32; MAX_FIGHTERS],
    pub phase: u8,
    pub clock: u32,
    pub winner: u8,
    /// Which of `remotes` (by slot) are held, not updated: a far one on a
    /// big map is updated every few snapshots (`room::build`) and between
    /// repeats its last update unchanged, so a delta of it costs almost
    /// nothing. The client draws it from its updates. Never one fog hides.
    pub held: u16,
}

const _: () = assert!(MAX_FIGHTERS <= u16::BITS as usize, "held is a u16 by slot");

impl Default for Snapshot {
    fn default() -> Self {
        Snapshot {
            tick: 0,
            last_seq: 0,
            depth: 0,
            own: Fighter::default(),
            own_vehicle: None,
            drive: None,
            n_remote: 0,
            remotes: [Remote::default(); MAX_REMOTES],
            n_proj: 0,
            proj: [ProjState::default(); MAX_SNAP_PROJ],
            n_events: 0,
            events: [EventState::default(); MAX_SNAP_EVENTS],
            n_pads: 0,
            pads: [PadState::default(); MAX_SNAP_PADS],
            n_vehicles: 0,
            vehicles: [VehState::default(); MAX_SNAP_VEHICLES],
            n_drops: 0,
            drops: [DropState::default(); MAX_SNAP_DROPS],
            n_flags: 0,
            flags: [FlagState::default(); 2],
            n_nodes: 0,
            nodes: [NodeState::default(); sim::consts::SIEGE_NODES],
            n_minions: 0,
            minions: [MinionState::default(); MAX_SNAP_MINIONS],
            n_score: 0,
            score: [0; MAX_FIGHTERS],
            phase: 0,
            clock: 0,
            winner: 255,
            held: 0,
        }
    }
}

impl Snapshot {
    pub fn remotes(&self) -> &[Remote] {
        &self.remotes[..self.n_remote as usize]
    }
    pub fn proj(&self) -> &[ProjState] {
        &self.proj[..self.n_proj as usize]
    }
    pub fn events(&self) -> &[EventState] {
        &self.events[..self.n_events as usize]
    }
    pub fn vehicles(&self) -> &[VehState] {
        &self.vehicles[..self.n_vehicles as usize]
    }
    pub fn pads(&self) -> &[PadState] {
        &self.pads[..self.n_pads as usize]
    }
    pub fn drops(&self) -> &[DropState] {
        &self.drops[..self.n_drops as usize]
    }
    pub fn flags(&self) -> &[FlagState] {
        &self.flags[..self.n_flags as usize]
    }
    pub fn nodes(&self) -> &[NodeState] {
        &self.nodes[..self.n_nodes as usize]
    }
    pub fn minions(&self) -> &[MinionState] {
        &self.minions[..self.n_minions as usize]
    }
}

/// How far down our own crouch has gone, 0 to `CROUCH_TICKS`: what our
/// prediction steps on from.
const DUCK_BITS: u32 = 5;
const _: () = assert!((sim::consts::CROUCH_TICKS as u32) < 1 << DUCK_BITS);

fn w_own(w: &mut BitWriter, f: &Fighter) -> R<()> {
    w.write_bit(f.seated)?;
    w.write(f.team as u32, 1)?;
    w.write_bit(f.alive)?;
    w.write_bit(f.body.grounded)?;
    w.write_bit(f.body.crouched)?;
    w.write(f.body.duck.min(sim::consts::CROUCH_TICKS) as u32, DUCK_BITS)?;
    for v in [
        f.body.pos.x,
        f.body.pos.y,
        f.body.pos.z,
        f.body.vel.x,
        f.body.vel.y,
        f.body.vel.z,
    ] {
        w_grid(w, v)?;
    }
    w.write(f.yaw as u32, 16)?;
    w.write(f.pitch as u16 as u32, 16)?;
    w_i32(w, f.shield, 32)?;
    w_i32(w, f.health, 32)?;
    for g in &f.guns {
        w_gun(w, g)?;
    }
    w.write(f.cur as u32 & 1, 1)?;
    w.write(f.refire as u32, 16)?;
    w.write(f.reload as u32, 16)?;
    w.write(f.burst_left as u32, 8)?;
    w.write(f.burst_gap as u32, 16)?;
    w.write(f.melee_cool as u32, COOL_BITS)?;
    w.write(f.charge as u32, 16)?;
    w.write(f.gren_cool as u32, COOL_BITS)?;
    w.write(f.grenades as u32, GREN_BITS)?;
    w.write(f.shots, 32)?;
    w.write(f.last_buttons as u32, 16)?;
    w.write(f.respawn as u32, 16)?;
    w.write(f.kills as u32, 16)?;
    w.write(f.deaths as u32, 16)?;
    w.write(f.ride as u32, RIDE_BITS)?;
    w_gun(w, &f.left)?;
    w.write(f.left_refire as u32, 16)?;
    w.write(f.left_reload as u32, 16)?;
    w.write(f.left_charge as u32, 16)?;
    w.write(f.stickies as u32, GREN_BITS)?;
    w.write(f.gren_kind as u32, 1)?;
    w.write_bit(f.charging)?;
    // Camouflage's 45 s and its 1.2 s flare, in ticks, with room.
    w.write(f.camo.min(CAMO_TICKS_MAX) as u32, 14)?;
    w.write(f.flare.min(u8::MAX as u16) as u32, 8)?;
    w.write_bit(f.flag)?;
    // The turret it guns: a bit, then its state only if any.
    let m = f.mount;
    w.write_bit(m != Mount::default())?;
    if m != Mount::default() {
        w.write(m.spin as u32, MOUNT_TICK_BITS)?;
        w.write(m.heat as u32, HEAT_BITS)?;
        w.write(m.refire as u32, MOUNT_TICK_BITS)?;
        w.write_bit(m.vent)?;
    }
    Ok(())
}

/// Grenades of a kind carried, to `GREN_MAX`; a seat in a vehicle (its
/// slot × 2 + the seat, plus 1).
const GREN_BITS: u32 = 3;
/// A melee's and a throw's cooldown, in ticks.
const COOL_BITS: u32 = 8;
const _: () = assert!(sim::consts::ticks(sim::consts::MELEE_COOL_MS) < 1 << COOL_BITS);
const _: () = assert!(sim::consts::ticks(sim::consts::GREN_COOL_MS) < 1 << COOL_BITS);
const RIDE_BITS: u32 = 5;
const _: () = assert!((sim::consts::GREN_MAX as u32) < 1 << GREN_BITS);
const _: () = assert!(2 * sim::consts::MAX_VEHICLES < 1 << RIDE_BITS);

/// A turret's spin and refire in ticks, and its heat (1/128 %, to full).
const MOUNT_TICK_BITS: u32 = 6;
const HEAT_BITS: u32 = 14;
const _: () = assert!(sim::consts::ticks(sim::arms::MINIGUN.spin_ms) < 1 << MOUNT_TICK_BITS);
const _: () = assert!(sim::consts::ticks(sim::arms::MINIGUN.refire_ms) < 1 << MOUNT_TICK_BITS);
const _: () = assert!((sim::arms::HEAT_FULL as u32) < 1 << HEAT_BITS);

fn w_gun(w: &mut BitWriter, g: &Gun) -> R<()> {
    w.write(g.arm as u32, 4)?;
    w.write(g.mag as u32, 16)?;
    w.write(g.reserve as u32, 16)?;
    w.write(g.heat as u32, 16)
}

fn r_gun(r: &mut BitReader) -> R<Gun> {
    let g = Gun {
        arm: r.read(4)? as u8,
        mag: r.read(16)? as u16,
        reserve: r.read(16)? as u16,
        heat: r.read(16)? as u16,
    };
    if g.arm as usize >= sim::arms::COUNT {
        return Err(WireError::Malformed);
    }
    Ok(g)
}

fn r_own(r: &mut BitReader) -> R<Fighter> {
    let seated = r.read_bit()?;
    let team = r.read(1)? as u8;
    let alive = r.read_bit()?;
    let grounded = r.read_bit()?;
    let crouched = r.read_bit()?;
    let duck = r.read(DUCK_BITS)? as u8;
    if duck > sim::consts::CROUCH_TICKS {
        return Err(WireError::Malformed);
    }
    let pos = V3::new(r_grid(r)?, r_grid(r)?, r_grid(r)?);
    let vel = V3::new(r_grid(r)?, r_grid(r)?, r_grid(r)?);
    let mut f = Fighter {
        seated,
        team,
        alive,
        body: Body {
            pos,
            vel,
            grounded,
            crouched,
            duck,
        },
        yaw: r.read(16)? as u16,
        pitch: r.read(16)? as u16 as i16,
        shield: r_i32(r, 32)?,
        health: r_i32(r, 32)?,
        ..Fighter::default()
    };
    for g in f.guns.iter_mut() {
        *g = r_gun(r)?;
    }
    f.cur = r.read(1)? as u8;
    f.refire = r.read(16)? as u16;
    f.reload = r.read(16)? as u16;
    f.burst_left = r.read(8)? as u8;
    f.burst_gap = r.read(16)? as u16;
    f.melee_cool = r.read(COOL_BITS)? as u16;
    f.charge = r.read(16)? as u16;
    f.gren_cool = r.read(COOL_BITS)? as u16;
    f.grenades = r.read(GREN_BITS)? as u8;
    f.shots = r.read(32)?;
    f.last_buttons = r.read(16)? as u16;
    f.respawn = r.read(16)? as u16;
    f.kills = r.read(16)? as u16;
    f.deaths = r.read(16)? as u16;
    f.ride = r.read(RIDE_BITS)? as u8;
    if f.ride as usize > 2 * sim::consts::MAX_VEHICLES {
        return Err(WireError::Malformed);
    }
    f.left = r_gun(r)?;
    f.left_refire = r.read(16)? as u16;
    f.left_reload = r.read(16)? as u16;
    f.left_charge = r.read(16)? as u16;
    f.stickies = r.read(GREN_BITS)? as u8;
    f.gren_kind = r.read(1)? as u8;
    f.charging = r.read_bit()?;
    f.camo = r.read(14)? as u16;
    f.flare = r.read(8)? as u16;
    f.flag = r.read_bit()?;
    if r.read_bit()? {
        f.mount = Mount {
            spin: r.read(MOUNT_TICK_BITS)? as u16,
            heat: r.read(HEAT_BITS)? as u16,
            refire: r.read(MOUNT_TICK_BITS)? as u16,
            vent: r.read_bit()?,
        };
    }
    Ok(f)
}

/// [`Snapshot::held`]: a bit, then the slots only if any (an arena never
/// holds anyone, so it pays one bit).
fn w_held(w: &mut BitWriter, held: u16) -> R<()> {
    w.write_bit(held != 0)?;
    if held != 0 {
        w.write(held as u32, MAX_FIGHTERS as u32)?;
    }
    Ok(())
}

fn r_held(r: &mut BitReader) -> R<u16> {
    Ok(if r.read_bit()? {
        r.read(MAX_FIGHTERS as u32)? as u16
    } else {
        0
    })
}

pub fn encode_snapshot(s: &Snapshot, out: &mut [u8]) -> R<usize> {
    let caps = [
        (s.n_remote as usize, MAX_REMOTES),
        (s.n_proj as usize, MAX_SNAP_PROJ),
        (s.n_events as usize, MAX_SNAP_EVENTS),
        (s.n_pads as usize, MAX_SNAP_PADS),
        (s.n_score as usize, MAX_FIGHTERS),
        (s.n_vehicles as usize, MAX_SNAP_VEHICLES),
        (s.n_drops as usize, MAX_SNAP_DROPS),
        (s.n_flags as usize, 2),
        (s.n_nodes as usize, sim::consts::SIEGE_NODES),
        (s.n_minions as usize, MAX_SNAP_MINIONS),
    ];
    if caps.iter().any(|&(n, cap)| n > cap) {
        return Err(WireError::Cap);
    }
    let mut w = BitWriter::new(out);
    w.write(KIND_SNAPSHOT, 4)?;
    w.write(s.tick, 32)?;
    w.write(s.last_seq, 32)?;
    w_i32(&mut w, s.depth as i32, 8)?;
    w_own(&mut w, &s.own)?;
    w.write_bit(s.own_vehicle.is_some())?;
    if let Some(v) = &s.own_vehicle {
        w_vehicle(&mut w, v)?;
        w_drive(&mut w, s.drive)?;
    }
    w_list(&mut w, s.remotes(), 4)?;
    w_list(&mut w, s.proj(), 5)?;
    w_list(&mut w, s.events(), 6)?;
    w_list(&mut w, s.pads(), 5)?;
    w_list(&mut w, s.vehicles(), 4)?;
    w_list(&mut w, s.drops(), 4)?;
    w_list(&mut w, s.flags(), 2)?;
    w_list(&mut w, s.nodes(), NODE_BITS)?;
    w_list(&mut w, s.minions(), MINION_BITS)?;
    w.write(s.n_score as u32, SLOT_BITS + 1)?;
    for &sc in &s.score[..s.n_score as usize] {
        w_i32(&mut w, score_on_wire(sc), 16)?;
    }
    w.write(s.phase as u32, 2)?;
    w.write(s.clock, 32)?;
    w.write(s.winner as u32, 8)?;
    w_held(&mut w, s.held)?;
    Ok(w.finish())
}

pub fn decode_snapshot(buf: &[u8]) -> R<Snapshot> {
    let mut r = BitReader::new(buf);
    if r.read(4)? != KIND_SNAPSHOT {
        return Err(WireError::Malformed);
    }
    let mut s = Snapshot {
        tick: r.read(32)?,
        last_seq: r.read(32)?,
        depth: r_i32(&mut r, 8)? as i8,
        own: r_own(&mut r)?,
        ..Snapshot::default()
    };
    if r.read_bit()? {
        s.own_vehicle = Some(r_vehicle(&mut r)?);
        s.drive = r_drive(&mut r)?;
    }
    s.n_remote = r_list(&mut r, &mut s.remotes, 4)?;
    s.n_proj = r_list(&mut r, &mut s.proj, 5)?;
    s.n_events = r_list(&mut r, &mut s.events, 6)?;
    s.n_pads = r_list(&mut r, &mut s.pads, 5)?;
    s.n_vehicles = r_list(&mut r, &mut s.vehicles, 4)?;
    s.n_drops = r_list(&mut r, &mut s.drops, 4)?;
    s.n_flags = r_list(&mut r, &mut s.flags, 2)?;
    s.n_nodes = r_list(&mut r, &mut s.nodes, NODE_BITS)?;
    s.n_minions = r_list(&mut r, &mut s.minions, MINION_BITS)?;
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
    done(&r)?;
    Ok(s)
}

// --------------------------------------------------------------- stream

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Name {
    pub len: u8,
    pub bytes: [u8; MAX_NAME],
}

impl Name {
    /// Printable ASCII only, cut to [`MAX_NAME`].
    pub fn new(s: &str) -> Name {
        let mut n = Name {
            len: 0,
            bytes: [0; MAX_NAME],
        };
        for b in s.bytes().filter(|b| (0x20..0x7f).contains(b)).take(MAX_NAME) {
            n.bytes[n.len as usize] = b;
            n.len += 1;
        }
        n
    }

    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len as usize]).unwrap_or("")
    }
}

/// A game server's `host:port`, as a server list names it: at most
/// [`MAX_SERVER_ADDR`] of letters, digits and `.-:[]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ServerAddr {
    pub len: u8,
    pub bytes: [u8; MAX_SERVER_ADDR],
}

pub const MAX_SERVER_ADDR: usize = 96;
const _: () = assert!(MAX_SERVER_ADDR < 1 << 7);

impl ServerAddr {
    pub const NONE: ServerAddr = ServerAddr {
        len: 0,
        bytes: [0; MAX_SERVER_ADDR],
    };

    fn ok(b: u8) -> bool {
        b.is_ascii_alphanumeric() || b".-:[]".contains(&b)
    }

    /// None unless it is 1 to [`MAX_SERVER_ADDR`] of the characters above.
    pub fn new(s: &str) -> Option<ServerAddr> {
        let b = s.trim().as_bytes();
        if b.is_empty() || b.len() > MAX_SERVER_ADDR || !b.iter().all(|&c| Self::ok(c)) {
            return None;
        }
        let mut a = ServerAddr::NONE;
        a.bytes[..b.len()].copy_from_slice(b);
        a.len = b.len() as u8;
        Some(a)
    }

    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len as usize]).unwrap_or("")
    }
}

/// A match id from Elo Pros' queue (`q-` and 16 url-safe characters
/// today), at most [`MAX_QUEUE_MATCH`]: the match a ranked hello comes for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QueueMatch {
    pub len: u8,
    pub bytes: [u8; MAX_QUEUE_MATCH],
}

pub const MAX_QUEUE_MATCH: usize = 32;
const QUEUE_MATCH_LEN_BITS: u32 = 6;
const _: () = assert!(MAX_QUEUE_MATCH < 1 << QUEUE_MATCH_LEN_BITS);

impl QueueMatch {
    pub const NONE: QueueMatch = QueueMatch {
        len: 0,
        bytes: [0; MAX_QUEUE_MATCH],
    };

    fn ok(b: u8) -> bool {
        b.is_ascii_alphanumeric() || b == b'-' || b == b'_'
    }

    /// None unless it is 1 to [`MAX_QUEUE_MATCH`] of A-Z a-z 0-9 - _.
    pub fn new(s: &str) -> Option<QueueMatch> {
        let b = s.trim().as_bytes();
        if b.is_empty() || b.len() > MAX_QUEUE_MATCH || !b.iter().all(|&c| Self::ok(c)) {
            return None;
        }
        let mut q = QueueMatch::NONE;
        q.bytes[..b.len()].copy_from_slice(b);
        q.len = b.len() as u8;
        Some(q)
    }

    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len as usize]).unwrap_or("")
    }

    pub fn is_none(&self) -> bool {
        self.len == 0
    }
}

/// An Elo Pros pass as the client was handed it: `b64url(text).b64url(sig)`,
/// printable ASCII. Empty for a guest. Opaque here; the server checks it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pass {
    pub len: u16,
    pub bytes: [u8; MAX_PASS],
}

impl Pass {
    pub const NONE: Pass = Pass {
        len: 0,
        bytes: [0; MAX_PASS],
    };

    /// None if it is too long or not printable ASCII.
    pub fn new(s: &str) -> Option<Pass> {
        let b = s.trim().as_bytes();
        if b.len() > MAX_PASS || !b.iter().all(|c| (0x21..0x7f).contains(c)) {
            return None;
        }
        let mut p = Pass::NONE;
        p.bytes[..b.len()].copy_from_slice(b);
        p.len = b.len() as u16;
        Some(p)
    }

    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len as usize]).unwrap_or("")
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

/// Skins, by the id the wire carries: cosmetic only (docs/DESIGN.md §15).
/// A skin never changes a hitbox, a sound or how well a fighter shows; the
/// page draws it as trim on the side's colour. Each is a Metaplex Core item
/// of the game's collection on Elo Pros, under its name here, except the
/// standard look everyone has.
pub const SKINS: [&str; 6] = ["standard", "pill", "ember", "glacier", "verdant", "gilded"];
pub const SKIN_STANDARD: u8 = 0;
/// The rank lock, by skin (`SKINS` order): the floor on Elo Pros (its
/// ratings' mu − 3 sigma on the Elo scale, idle time counted; a wallet's best
/// across the ladders) a wallet needs to wear the skin it owns. 0 is no lock.
/// Owning one is a market; wearing one is a record. A wallet below the line
/// wears the standard look, and can sell the skin to one above it.
pub const SKIN_FLOOR: [u16; 6] = [0, 0, 1300, 1500, 1650, 1800];

/// An emblem, Halo's way: a glyph on a field, each in its own colour, worn
/// on the back. Cosmetic only, as a skin: never lit, never on a hitbox, never
/// changes how well a fighter shows (docs/DESIGN.md §15). Sixteen bits: the
/// glyph (`EMBLEM_GLYPHS`, 5 bits; 0 is no emblem at all), the field
/// (`EMBLEM_FIELDS`, 3 bits; 0 is none), the glyph's colour and the field's
/// (4 bits each, the client's palette).
pub type Emblem = u16;
pub const EMBLEM_NONE: Emblem = 0;
pub const EMBLEM_GLYPHS: [&str; 17] = [
    "none",
    "star",
    "chevron",
    "bolt",
    "crosshair",
    "ring",
    "diamond",
    "triangle",
    "cross",
    "arrow",
    "moon",
    "heart",
    "eye",
    "shield",
    "sun",
    "wings",
    "crown",
];
pub const EMBLEM_FIELDS: [&str; 8] = [
    "none", "circle", "square", "diamond", "hexagon", "shield", "triangle", "octagon",
];
/// Colours in the palette.
pub const EMBLEM_COLOURS: u16 = 16;
/// The rank lock, by glyph (`EMBLEM_GLYPHS` order), as [`SKIN_FLOOR`]: a
/// wallet whose floor is under the line wears no emblem. 0 is no lock.
pub const GLYPH_FLOOR: [u16; 17] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1500, 1650, 1800];

/// Gear, by the id the wire carries, a slot each: worn with any skin, and
/// with each other. Cosmetic only, as a skin (docs/DESIGN.md §15). Every
/// slot is open to everyone, with no rank lock; what goes in it is worn
/// only if the wallet owns it on Elo Pros, under its name here. Nobody
/// owns any by default: none is the only gear everyone has.
pub const HEADWEAR: [&str; 14] = [
    "none",
    "shades",
    "aviators",
    "rounds",
    "visor",
    "goggles",
    "respirator",
    "demon",
    "welder",
    "skull",
    "hardhat",
    "helmet",
    "ten-gallon",
    "beanie",
];
pub const FOOTWEAR: [&str; 6] = ["none", "loggers", "hightops", "spurs", "magboots", "wellies"];
pub const GEAR_NONE: u8 = 0;

/// `g` if it names gear in `slot` ([`HEADWEAR`], [`FOOTWEAR`]), else none.
pub const fn gear_valid(slot: &[&str], g: u8) -> u8 {
    if (g as usize) < slot.len() {
        g
    } else {
        GEAR_NONE
    }
}

/// What a fighter wears, as the server checked it: its skin, its emblem and
/// its gear.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Look {
    pub skin: u8,
    pub emblem: Emblem,
    pub headwear: u8,
    pub footwear: u8,
}

/// An emblem's parts: (glyph, field, glyph colour, field colour).
pub const fn emblem_parts(e: Emblem) -> (u8, u8, u8, u8) {
    (
        (e & 31) as u8,
        ((e >> 5) & 7) as u8,
        ((e >> 8) & 15) as u8,
        ((e >> 12) & 15) as u8,
    )
}

/// An emblem from its parts, each taken modulo its range.
pub const fn emblem(glyph: u8, field: u8, glyph_colour: u8, field_colour: u8) -> Emblem {
    let g = glyph as u16 % EMBLEM_GLYPHS.len() as u16;
    if g == 0 {
        return EMBLEM_NONE;
    }
    g | (field as u16 & 7) << 5 | (glyph_colour as u16 & 15) << 8 | (field_colour as u16 & 15) << 12
}

/// `e` if it names a glyph that exists, else [`EMBLEM_NONE`]: what a room
/// keeps of what a client asked for.
pub const fn emblem_valid(e: Emblem) -> Emblem {
    let (g, ..) = emblem_parts(e);
    if g == 0 || g as usize >= EMBLEM_GLYPHS.len() {
        EMBLEM_NONE
    } else {
        e
    }
}

/// What a client asks to play (`Msg::Hello::mode`): casual against a bot,
/// ranked Duel or Doubles, casual free-for-all or Gun Game with bots in the
/// empty seats.
pub const MODE_CASUAL: u8 = 0;
pub const MODE_DUEL: u8 = 1;
pub const MODE_DOUBLES: u8 = 2;
pub const MODE_FFA: u8 = 3;
pub const MODE_GUN_GAME: u8 = 4;
/// A cup: free-for-all heats, Swiss rounds, a final (ranked: a wallet's).
pub const MODE_CUP: u8 = 5;
/// Big Team: 8v8 Team Slayer, casual, bots in the empty seats.
pub const MODE_BIG_TEAM: u8 = 6;
/// No match: the menu's own session, for a party, its friends and its
/// voice ([`party`]). Kept open beside a match.
pub const MODE_LOBBY: u8 = 7;
/// Capture the Flag: four a side, casual, bots in the empty seats.
pub const MODE_CTF: u8 = 8;
/// No seat: watch a match as one of its players saw it, late
/// (`room::spectate`). The hello's `party` names whose: a party's code
/// (a friend's), or 0 for any public match.
pub const MODE_WATCH: u8 = 9;
/// Siege: Big Team's sixteen seats, generators to overload, then the core
/// (docs/DESIGN.md §18); casual, bots in the empty seats.
pub const MODE_SIEGE: u8 = 10;
/// Modes fit this many bits.
const MODE_BITS: u32 = 4;

/// A mode as a player reads it.
pub fn mode_name(mode: u8) -> &'static str {
    match mode {
        MODE_CASUAL => "the bot",
        MODE_DUEL => "Ranked Duel",
        MODE_DOUBLES => "Ranked Doubles",
        MODE_FFA => "Free for all",
        MODE_GUN_GAME => "Gun Game",
        MODE_CUP => "a cup",
        MODE_BIG_TEAM => "Big Team",
        MODE_CTF => "Capture the Flag",
        MODE_WATCH => "watching",
        MODE_SIEGE => "Siege",
        _ => "oneshot",
    }
}
/// Bits a medal id takes (`medal::COUNT` fits).
const MEDAL_BITS: u32 = 6;
const _: () = assert!(medal::COUNT <= 1 << MEDAL_BITS);

// The hello carries a pass (640 bytes) and so sizes every message; stream
// messages are a handful a second, so `Copy` is worth more than the bytes.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Msg {
    /// Client → server, first: who (a pass from Elo Pros, or none for a
    /// guest) and what to play.
    Hello {
        proto: u16,
        name: Name,
        pass: Pass,
        mode: u8,
        /// The skin it would wear: worn only if the pass's wallet owns it.
        skin: u8,
        /// The emblem it would wear on its back ([`Emblem`]).
        emblem: Emblem,
        /// A match: the party it comes with (its code), to be seated with
        /// it; a lobby: the party to join. 0: none.
        party: u32,
        /// The client's sim's stamp (`sim::STAMP`): a server whose own
        /// differs refuses it ([`REFUSE_VERSION`]), as it does an old
        /// `proto`. 0 when a client from before it sent none, so that
        /// client still reads its refusal.
        stamp: u32,
        /// The gear it would wear ([`HEADWEAR`], [`FOOTWEAR`]): none when
        /// an older client sent none, so it reads its refusal.
        headwear: u8,
        footwear: u8,
        /// Ranked: the match Elo Pros' queue sent it to (`lobbies.py`),
        /// whose sides the server seats; none for the server's own queue.
        /// Last on the wire, and none when a client sent none.
        queued: QueueMatch,
    },
    /// Server → client, while queued: Elo Pros' queue put the match on
    /// another server. The client goes there for match `queued` (with the
    /// same pass or ticket), and this session ends.
    Moved {
        addr: ServerAddr,
        queued: QueueMatch,
    },
    /// Server → client: in the ranked queue, `waiting_s` so far.
    Queued {
        mode: u8,
        waiting_s: u16,
    },
    /// Server → client: you are in.
    Welcome {
        slot: u8,
        team: u8,
        tick: u32,
        tick_hz: u16,
        map: u8,
        rules: Rules,
        /// SHA-256 of the match's salt (see [`salt_commit`]): the salt
        /// itself comes in [`Msg::Reveal`] once the match is over.
        salt_commit: [u8; 32],
    },
    /// Server → client: you are not.
    Refuse {
        reason: u8,
    },
    /// Who sits where (sent for every seat on join, and on any change).
    Seat {
        slot: u8,
        team: u8,
        bot: bool,
        name: Name,
        /// The skin the server checked it owns; [`SKIN_STANDARD`] otherwise.
        skin: u8,
        /// The emblem the server checked; [`EMBLEM_NONE`] otherwise.
        emblem: Emblem,
        /// The gear the server checked; [`GEAR_NONE`] otherwise.
        headwear: u8,
        footwear: u8,
    },
    Kill {
        killer: u8,
        victim: u8,
        cause: u16,
        /// The blow was to the head.
        head: bool,
    },
    Phase {
        phase: u8,
        winner: u8,
    },
    /// Server → a cup's entrant, as it forms and after every heat: where they
    /// stand. `round` counts Swiss rounds from 1, `rounds + 1` is the
    /// final; `done` once it is over (`place` is then final).
    Cup {
        round: u8,
        rounds: u8,
        entrants: u8,
        place: u8,
        points: u16,
        done: bool,
    },
    /// Server → everyone, once the match is over: the salt every spread was
    /// drawn with, to check against the welcome's commitment and to replay.
    Reveal {
        salt: [u64; 2],
    },
    Bye,
    /// A watcher: client → server, follow the next player; server → client,
    /// the seat it follows now (`slot`, on side `team`). Its snapshots'
    /// own fighter is that seat's.
    Follow {
        slot: u8,
        team: u8,
    },
    /// Server → whoever earned it, or everyone for a public one
    /// ([`medal::Info::public`]): a medal (`medal`), for the kill on
    /// `victim`.
    Medal {
        slot: u8,
        medal: u8,
        victim: u8,
    },
    /// Server → everyone, once the match is over: one seat's carnage
    /// report. Rounds are hitscan rounds; damage is in points.
    Carnage {
        slot: u8,
        kills: u16,
        deaths: u16,
        assists: u16,
        betrayals: u16,
        shots: u16,
        hits: u16,
        heads: u16,
        damage: u16,
        taken: u16,
        best_spree: u16,
        best_multi: u8,
        /// Each medal's count, by id (saturated).
        medals: [u8; medal::COUNT],
    },
    /// Server → a session that signed in, and a guest's lobby: what the
    /// server takes in place of a pass for a while (`server::auth`), so a
    /// page that reloads after a match is still who it was.
    Ticket {
        ticket: Pass,
    },
    /// Client → server on a lobby: sign in now, with a fresh pass.
    SignIn {
        pass: Pass,
    },
    /// Client → server on a lobby: a party act (`party::ACT_*`).
    PartyAct {
        act: u8,
        arg: u32,
    },
    /// Client → server on a lobby: let this friend into my party, and tell
    /// them if they're here.
    PartyInvite {
        player: PlayerId,
    },
    /// Server → a lobby: its party as it is now.
    Party(Party),
    /// Server → each member present: the leader started `mode`. Each opens
    /// a match session with the party's code; `private`: the party alone.
    Go {
        mode: u8,
        private: bool,
    },
    /// Server → a lobby: `n` friend rows follow ([`Msg::Friend`]).
    /// `reachable`: Elo Pros answered (false: we couldn't ask).
    Friends {
        n: u8,
        reachable: bool,
    },
    /// Server → a lobby: one of my friends, and where they are.
    Friend {
        i: u8,
        name: Name,
        player: PlayerId,
        /// `party::AT_*`.
        at: u8,
        /// The party they're in, if I may join it; 0 otherwise.
        party: u32,
        /// How many are in their party.
        size: u8,
        /// What they're in: a mode, or `party::PLAYING_NONE`.
        playing: u8,
    },
    /// Server → a lobby: `from` asks me into party `code`.
    Invited {
        from: Name,
        code: u32,
    },
    /// Server → a lobby, on joining and on signing in: what its wallet
    /// owns and its rating floor, so the armory can say what it may wear.
    /// The server still checks every hello (`server::net`); this only
    /// shows it.
    Wardrobe(Wardrobe),
    /// Client → server on a match: its level is all there (`loading.rs`).
    /// The countdown waits for every person seated in it to say so, or for
    /// the room's guard (`room::LOAD_WAIT_MS`).
    Loaded,
}

/// What a lobby's wallet owns ([`Msg::Wardrobe`]): a bit an item, by the id
/// the wire carries ([`SKINS`], [`HEADWEAR`], [`FOOTWEAR`]); the standard
/// look and no gear are everyone's, so their bits are always set.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Wardrobe {
    /// A wallet is signed in (false: a guest, or a pass without one, who
    /// wears the standard look and no gear).
    pub wallet: bool,
    /// Elo Pros answered the items list (false: we couldn't ask).
    pub known: bool,
    pub skins: u32,
    pub headwear: u32,
    pub footwear: u32,
    /// The wallet's floor (`SKIN_FLOOR`'s scale), if it is rated.
    pub floor: Option<u16>,
}

impl Wardrobe {
    /// Owns nothing: the standard look and no gear.
    pub const BARE: Wardrobe = Wardrobe {
        wallet: false,
        known: true,
        skins: 1,
        headwear: 1,
        footwear: 1,
        floor: None,
    };

    /// Item `i` of a list (`skins`, `headwear` or `footwear`) is owned.
    pub const fn has(mask: u32, i: u8) -> bool {
        i == 0 || (i < 32 && mask & (1 << i) != 0)
    }

    /// The masks from a list of item names (a server's items answer).
    pub fn from_names<'a>(names: impl IntoIterator<Item = &'a str>) -> (u32, u32, u32) {
        let bit = |list: &[&str], n: &str| list.iter().position(|s| *s == n).map_or(0, |i| 1u32 << i);
        let (mut s, mut h, mut f) = (1, 1, 1);
        for n in names {
            s |= bit(&SKINS, n);
            h |= bit(&HEADWEAR, n);
            f |= bit(&FOOTWEAR, n);
        }
        (s, h, f)
    }

    /// May wear skin `i`: owned and at its line.
    pub fn may_wear_skin(&self, i: u8) -> bool {
        let line = SKIN_FLOOR.get(i as usize).copied().unwrap_or(u16::MAX);
        Self::has(self.skins, i) && (line == 0 || self.floor.is_some_and(|f| f >= line))
    }
}

// Every list's items fit a wardrobe's masks.
const _: () = assert!(SKINS.len() <= 32 && HEADWEAR.len() <= 32 && FOOTWEAR.len() <= 32);

/// The commitment to a match's salt: SHA-256 of its two words, little-endian.
pub fn salt_commit(salt: [u64; 2]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(salt[0].to_le_bytes());
    h.update(salt[1].to_le_bytes());
    h.finalize().into()
}

pub const REFUSE_VERSION: u8 = 1;
pub const REFUSE_FULL: u8 = 2;
/// The pass didn't check out (signature, game, host, nonce or time).
pub const REFUSE_PASS: u8 = 3;
/// Ranked needs a pass with a wallet: ratings are a wallet's.
pub const REFUSE_NEEDS_WALLET: u8 = 4;
/// This wallet is already queued or playing.
pub const REFUSE_BUSY: u8 = 5;
/// The game has banned this wallet from ranked (Elo Pros `/bans/oneshot`).
pub const REFUSE_BANNED: u8 = 6;
/// No party has that code (any more).
pub const REFUSE_NO_PARTY: u8 = 7;
pub const REFUSE_PARTY_FULL: u8 = 8;
/// The party is for the leader's friends, and you aren't one.
pub const REFUSE_FRIENDS_ONLY: u8 = 9;
/// The party is invite only, and you weren't asked.
pub const REFUSE_INVITE_ONLY: u8 = 10;
/// The leader removed you from that party.
pub const REFUSE_KICKED: u8 = 11;
/// Only the party's leader does that.
pub const REFUSE_NOT_LEADER: u8 = 12;
/// The party doesn't fit that mode (too many, or someone without a wallet
/// for ranked).
pub const REFUSE_PARTY_MODE: u8 = 13;
/// Not one of your friends on Elo Pros, or not signed in.
pub const REFUSE_NOT_FRIEND: u8 = 14;
/// Nothing to watch: no public match, that party isn't in one, or it is
/// your own.
pub const REFUSE_NOTHING_TO_WATCH: u8 = 15;
/// Ranked: the match Elo Pros' queue named isn't one this server can play
/// for this wallet (not on its sides, not this game or server, gone), or
/// not everyone came for it in time: queue again.
pub const REFUSE_MATCH: u8 = 16;

pub(crate) fn w_name(w: &mut BitWriter, n: &Name) -> R<()> {
    w.write(n.len as u32, 5)?;
    for &b in &n.bytes[..n.len as usize] {
        w.write(b as u32, 8)?;
    }
    Ok(())
}

pub(crate) fn r_name(r: &mut BitReader) -> R<Name> {
    let len = r.read(5)? as usize;
    if len > MAX_NAME {
        return Err(WireError::Cap);
    }
    let mut n = Name {
        len: len as u8,
        bytes: [0; MAX_NAME],
    };
    for i in 0..len {
        let b = r.read(8)? as u8;
        if !(0x20..0x7f).contains(&b) {
            return Err(WireError::Malformed);
        }
        n.bytes[i] = b;
    }
    Ok(n)
}

pub fn encode_msg(m: &Msg, out: &mut [u8]) -> R<usize> {
    let mut w = BitWriter::new(out);
    match m {
        Msg::Hello {
            proto,
            name,
            pass,
            mode,
            skin,
            emblem,
            party,
            stamp,
            headwear,
            footwear,
            queued,
        } => {
            w.write(1, 8)?;
            w.write(*proto as u32, 16)?;
            w_name(&mut w, name)?;
            w_pass(&mut w, pass)?;
            w.write(*mode as u32, MODE_BITS)?;
            w.write(*skin as u32, 8)?;
            w.write(*emblem as u32, 16)?;
            w.write(*party, party::CODE_BITS)?;
            w.write(*stamp, 32)?;
            w.write(*headwear as u32, 8)?;
            w.write(*footwear as u32, 8)?;
            w.write(queued.len as u32, QUEUE_MATCH_LEN_BITS)?;
            for &b in &queued.bytes[..queued.len as usize] {
                w.write(b as u32, 7)?;
            }
        }
        Msg::Queued { mode, waiting_s } => {
            w.write(9, 8)?;
            w.write(*mode as u32, MODE_BITS)?;
            w.write(*waiting_s as u32, 16)?;
        }
        Msg::Welcome {
            slot,
            team,
            tick,
            tick_hz,
            map,
            rules,
            salt_commit,
        } => {
            w.write(2, 8)?;
            w.write(*slot as u32, SLOT_BITS)?;
            w.write(*team as u32 & 1, 1)?;
            w.write(*tick, 32)?;
            w.write(*tick_hz as u32, 16)?;
            w.write(*map as u32, 8)?;
            w.write_bit(rules.teams)?;
            w_i32(&mut w, rules.score_limit, 32)?;
            w.write(rules.time_limit_ms, 32)?;
            w.write(rules.loadout[0] as u32, 4)?;
            w.write(rules.loadout[1] as u32, 4)?;
            w.write(rules.seats as u32, SLOT_BITS + 1)?;
            w.write_bit(rules.gun_game)?;
            w.write_bit(rules.friendly_fire)?;
            w.write_bit(rules.ctf)?;
            w.write_bit(rules.siege)?;
            for &b in salt_commit {
                w.write(b as u32, 8)?;
            }
        }
        Msg::Refuse { reason } => {
            w.write(3, 8)?;
            w.write(*reason as u32, 8)?;
        }
        Msg::Seat {
            slot,
            team,
            bot,
            name,
            skin,
            emblem,
            headwear,
            footwear,
        } => {
            w.write(4, 8)?;
            w.write(*slot as u32, SLOT_BITS)?;
            w.write(*team as u32 & 1, 1)?;
            w.write_bit(*bot)?;
            w_name(&mut w, name)?;
            w.write(*skin as u32, 8)?;
            w.write(*emblem as u32, 16)?;
            w.write(*headwear as u32, 8)?;
            w.write(*footwear as u32, 8)?;
        }
        Msg::Kill {
            killer,
            victim,
            cause,
            head,
        } => {
            w.write(5, 8)?;
            w.write(*killer as u32, SLOT_BITS)?;
            w.write(*victim as u32, SLOT_BITS)?;
            w.write(*cause as u32, 16)?;
            w.write_bit(*head)?;
        }
        Msg::Phase { phase, winner } => {
            w.write(6, 8)?;
            w.write(*phase as u32, 2)?;
            w.write(*winner as u32, 8)?;
        }
        Msg::Bye => w.write(7, 8)?,
        Msg::Loaded => w.write(25, 8)?,
        Msg::Moved { addr, queued } => {
            w.write(23, 8)?;
            w.write(addr.len as u32, 7)?;
            for &b in &addr.bytes[..addr.len as usize] {
                w.write(b as u32, 7)?;
            }
            w.write(queued.len as u32, QUEUE_MATCH_LEN_BITS)?;
            for &b in &queued.bytes[..queued.len as usize] {
                w.write(b as u32, 7)?;
            }
        }
        Msg::Follow { slot, team } => {
            w.write(22, 8)?;
            w.write(*slot as u32, SLOT_BITS)?;
            w.write(*team as u32, 1)?;
        }
        Msg::Cup {
            round,
            rounds,
            entrants,
            place,
            points,
            done,
        } => {
            w.write(10, 8)?;
            w.write(*round as u32, 8)?;
            w.write(*rounds as u32, 8)?;
            w.write(*entrants as u32, 8)?;
            w.write(*place as u32, 8)?;
            w.write(*points as u32, 16)?;
            w.write_bit(*done)?;
        }
        Msg::Reveal { salt } => {
            w.write(8, 8)?;
            for &x in salt {
                w.write(x as u32, 32)?;
                w.write((x >> 32) as u32, 32)?;
            }
        }
        Msg::Medal { slot, medal, victim } => {
            w.write(11, 8)?;
            w.write(*slot as u32, SLOT_BITS)?;
            w.write(*medal as u32, MEDAL_BITS)?;
            w.write(*victim as u32, SLOT_BITS)?;
        }
        Msg::Carnage {
            slot,
            kills,
            deaths,
            assists,
            betrayals,
            shots,
            hits,
            heads,
            damage,
            taken,
            best_spree,
            best_multi,
            medals,
        } => {
            w.write(12, 8)?;
            w.write(*slot as u32, SLOT_BITS)?;
            for &x in &[
                *kills,
                *deaths,
                *assists,
                *betrayals,
                *shots,
                *hits,
                *heads,
                *damage,
                *taken,
                *best_spree,
            ] {
                w.write(x as u32, 16)?;
            }
            w.write(*best_multi as u32, 8)?;
            // Only the medals earned: an id and its count each.
            let n = medals.iter().filter(|&&c| c > 0).count();
            w.write(n as u32, MEDAL_BITS)?;
            for (id, &c) in medals.iter().enumerate().filter(|(_, &c)| c > 0) {
                w.write(id as u32, MEDAL_BITS)?;
                w.write(c as u32, 8)?;
            }
        }
        Msg::Ticket { ticket: p } | Msg::SignIn { pass: p } => {
            w.write(if matches!(m, Msg::Ticket { .. }) { 13 } else { 14 }, 8)?;
            w_pass(&mut w, p)?;
        }
        Msg::PartyAct { act, arg } => {
            w.write(15, 8)?;
            w.write(*act as u32, 8)?;
            w.write(*arg, 32)?;
        }
        Msg::PartyInvite { player } => {
            w.write(16, 8)?;
            player.pack(&mut w)?;
        }
        Msg::Party(p) => {
            w.write(17, 8)?;
            p.pack(&mut w)?;
        }
        Msg::Go { mode, private } => {
            w.write(18, 8)?;
            w.write(*mode as u32, MODE_BITS)?;
            w.write_bit(*private)?;
        }
        Msg::Friends { n, reachable } => {
            w.write(19, 8)?;
            w.write(*n as u32, 8)?;
            w.write_bit(*reachable)?;
        }
        Msg::Friend {
            i,
            name,
            player,
            at,
            party: code,
            size,
            playing,
        } => {
            w.write(20, 8)?;
            w.write(*i as u32, 8)?;
            w_name(&mut w, name)?;
            player.pack(&mut w)?;
            party::w_at(&mut w, *at)?;
            w.write(*code, party::CODE_BITS)?;
            w.write(*size as u32, 4)?;
            party::w_play(&mut w, *playing)?;
        }
        Msg::Invited { from, code } => {
            w.write(21, 8)?;
            w_name(&mut w, from)?;
            w.write(*code, party::CODE_BITS)?;
        }
        Msg::Wardrobe(d) => {
            w.write(24, 8)?;
            w.write_bit(d.wallet)?;
            w.write_bit(d.known)?;
            w.write(d.skins, 32)?;
            w.write(d.headwear, 32)?;
            w.write(d.footwear, 32)?;
            w.write_bit(d.floor.is_some())?;
            if let Some(f) = d.floor {
                w.write(f as u32, 16)?;
            }
        }
    }
    Ok(w.finish())
}

/// A queue's match id: [`QueueMatch`]'s characters, counted.
fn r_queue_match(r: &mut BitReader) -> R<QueueMatch> {
    let len = r.read(QUEUE_MATCH_LEN_BITS)? as usize;
    if len > MAX_QUEUE_MATCH {
        return Err(WireError::Cap);
    }
    let mut q = QueueMatch::NONE;
    for b in q.bytes[..len].iter_mut() {
        *b = r.read(7)? as u8;
        if !QueueMatch::ok(*b) {
            return Err(WireError::Malformed);
        }
    }
    q.len = len as u8;
    Ok(q)
}

/// A pass or a ticket: printable ASCII, counted.
fn w_pass(w: &mut BitWriter, p: &Pass) -> R<()> {
    w.write(p.len as u32, 10)?;
    for &b in &p.bytes[..p.len as usize] {
        w.write(b as u32, 7)?;
    }
    Ok(())
}

fn r_pass(r: &mut BitReader) -> R<Pass> {
    let len = r.read(10)? as usize;
    if len > MAX_PASS {
        return Err(WireError::Cap);
    }
    let mut p = Pass::NONE;
    for b in p.bytes[..len].iter_mut() {
        *b = r.read(7)? as u8;
        if !(0x21..0x7f).contains(b) {
            return Err(WireError::Malformed);
        }
    }
    p.len = len as u16;
    Ok(p)
}

/// A medal id: [`medal::NONE`] or past the table is malformed.
fn r_medal(r: &mut BitReader) -> R<u8> {
    match r.read(MEDAL_BITS)? as usize {
        id if id > 0 && id < medal::COUNT => Ok(id as u8),
        _ => Err(WireError::Malformed),
    }
}

pub fn decode_msg(buf: &[u8]) -> R<Msg> {
    let mut r = BitReader::new(buf);
    let m = match r.read(8)? {
        1 => Msg::Hello {
            proto: r.read(16)? as u16,
            name: r_name(&mut r)?,
            pass: r_pass(&mut r)?,
            mode: r.read(MODE_BITS)? as u8,
            skin: r.read(8)? as u8,
            emblem: r.read(16)? as u16,
            party: r.read(party::CODE_BITS)?,
            stamp: if r.remaining_bits() >= 32 { r.read(32)? } else { 0 },
            headwear: if r.remaining_bits() >= 8 { r.read(8)? as u8 } else { 0 },
            footwear: if r.remaining_bits() >= 8 { r.read(8)? as u8 } else { 0 },
            queued: if r.remaining_bits() >= QUEUE_MATCH_LEN_BITS as usize {
                r_queue_match(&mut r)?
            } else {
                QueueMatch::NONE
            },
        },
        9 => Msg::Queued {
            mode: r.read(MODE_BITS)? as u8,
            waiting_s: r.read(16)? as u16,
        },
        2 => Msg::Welcome {
            slot: r.read(SLOT_BITS)? as u8,
            team: r.read(1)? as u8,
            tick: r.read(32)?,
            tick_hz: r.read(16)? as u16,
            map: r.read(8)? as u8,
            rules: Rules {
                teams: r.read_bit()?,
                score_limit: r_i32(&mut r, 32)?,
                time_limit_ms: r.read(32)?,
                loadout: [r.read(4)? as u8, r.read(4)? as u8],
                seats: match r.read(SLOT_BITS + 1)? as u8 {
                    n if (1..=MAX_FIGHTERS as u8).contains(&n) => n,
                    _ => return Err(WireError::Malformed),
                },
                gun_game: r.read_bit()?,
                friendly_fire: r.read_bit()?,
                ctf: r.read_bit()?,
                siege: r.read_bit()?,
            },
            salt_commit: {
                let mut c = [0u8; 32];
                for b in c.iter_mut() {
                    *b = r.read(8)? as u8;
                }
                c
            },
        },
        3 => Msg::Refuse {
            reason: r.read(8)? as u8,
        },
        4 => Msg::Seat {
            slot: r.read(SLOT_BITS)? as u8,
            team: r.read(1)? as u8,
            bot: r.read_bit()?,
            name: r_name(&mut r)?,
            skin: r.read(8)? as u8,
            emblem: r.read(16)? as u16,
            headwear: r.read(8)? as u8,
            footwear: r.read(8)? as u8,
        },
        5 => Msg::Kill {
            killer: r.read(SLOT_BITS)? as u8,
            victim: r.read(SLOT_BITS)? as u8,
            cause: r.read(16)? as u16,
            head: r.read_bit()?,
        },
        6 => Msg::Phase {
            phase: r.read(2)? as u8,
            winner: r.read(8)? as u8,
        },
        7 => Msg::Bye,
        25 => Msg::Loaded,
        23 => {
            let len = r.read(7)? as usize;
            if len > MAX_SERVER_ADDR {
                return Err(WireError::Cap);
            }
            let mut addr = ServerAddr::NONE;
            for b in addr.bytes[..len].iter_mut() {
                *b = r.read(7)? as u8;
                if !ServerAddr::ok(*b) {
                    return Err(WireError::Malformed);
                }
            }
            addr.len = len as u8;
            Msg::Moved {
                addr,
                queued: r_queue_match(&mut r)?,
            }
        }
        22 => Msg::Follow {
            slot: r.read(SLOT_BITS)? as u8,
            team: r.read(1)? as u8,
        },
        10 => Msg::Cup {
            round: r.read(8)? as u8,
            rounds: r.read(8)? as u8,
            entrants: r.read(8)? as u8,
            place: r.read(8)? as u8,
            points: r.read(16)? as u16,
            done: r.read_bit()?,
        },
        8 => {
            let mut salt = [0u64; 2];
            for x in salt.iter_mut() {
                *x = r.read(32)? as u64 | (r.read(32)? as u64) << 32;
            }
            Msg::Reveal { salt }
        }
        11 => Msg::Medal {
            slot: r.read(SLOT_BITS)? as u8,
            medal: r_medal(&mut r)?,
            victim: r.read(SLOT_BITS)? as u8,
        },
        12 => {
            let slot = r.read(SLOT_BITS)? as u8;
            let mut v = [0u16; 10];
            for x in v.iter_mut() {
                *x = r.read(16)? as u16;
            }
            let best_multi = r.read(8)? as u8;
            let n = r.read(MEDAL_BITS)? as usize;
            if n >= medal::COUNT {
                return Err(WireError::Cap);
            }
            // Ascending, each earned: one way to write any tally.
            let mut medals = [0u8; medal::COUNT];
            let mut last = 0;
            for _ in 0..n {
                let id = r_medal(&mut r)? as usize;
                let c = r.read(8)? as u8;
                if id <= last || c == 0 {
                    return Err(WireError::Malformed);
                }
                medals[id] = c;
                last = id;
            }
            Msg::Carnage {
                slot,
                kills: v[0],
                deaths: v[1],
                assists: v[2],
                betrayals: v[3],
                shots: v[4],
                hits: v[5],
                heads: v[6],
                damage: v[7],
                taken: v[8],
                best_spree: v[9],
                best_multi,
                medals,
            }
        }
        13 => Msg::Ticket {
            ticket: r_pass(&mut r)?,
        },
        14 => Msg::SignIn { pass: r_pass(&mut r)? },
        15 => Msg::PartyAct {
            act: r.read(8)? as u8,
            arg: r.read(32)?,
        },
        16 => Msg::PartyInvite {
            player: PlayerId::unpack(&mut r)?,
        },
        17 => Msg::Party(Party::unpack(&mut r)?),
        18 => Msg::Go {
            mode: r.read(MODE_BITS)? as u8,
            private: r.read_bit()?,
        },
        19 => Msg::Friends {
            n: r.read(8)? as u8,
            reachable: r.read_bit()?,
        },
        20 => Msg::Friend {
            i: r.read(8)? as u8,
            name: r_name(&mut r)?,
            player: PlayerId::unpack(&mut r)?,
            at: party::r_at(&mut r)?,
            party: r.read(party::CODE_BITS)?,
            size: r.read(4)? as u8,
            playing: party::r_play(&mut r)?,
        },
        21 => Msg::Invited {
            from: r_name(&mut r)?,
            code: r.read(party::CODE_BITS)?,
        },
        24 => Msg::Wardrobe(Wardrobe {
            wallet: r.read_bit()?,
            known: r.read_bit()?,
            skins: r.read(32)?,
            headwear: r.read(32)?,
            footwear: r.read(32)?,
            floor: if r.read_bit()? { Some(r.read(16)? as u16) } else { None },
        }),
        _ => return Err(WireError::Malformed),
    };
    done(&r)?;
    Ok(m)
}

/// A message framed for the stream: length prefix, then the message.
pub fn frame_msg(m: &Msg, out: &mut [u8; MAX_STREAM_MSG + frame::LEN_PREFIX_BYTES]) -> R<usize> {
    let mut body = [0u8; MAX_STREAM_MSG];
    let n = encode_msg(m, &mut body)?;
    frame::encode_into(out, &body[..n]).ok_or(WireError::Overflow)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A hello from a page from before the stamp (no stamp on the wire)
    /// still reads, its stamp 0, so the server can tell it to reload.
    #[test]
    fn a_hello_without_a_stamp_reads_as_stamp_zero() {
        let mut buf = [0u8; MAX_STREAM_MSG];
        let mut w = BitWriter::new(&mut buf);
        w.write(1, 8).unwrap();
        w.write(23, 16).unwrap();
        w_name(&mut w, &Name::new("old page")).unwrap();
        w_pass(&mut w, &Pass::NONE).unwrap();
        w.write(MODE_CASUAL as u32, MODE_BITS).unwrap();
        w.write(0, 8).unwrap();
        w.write(0, 16).unwrap();
        w.write(0, party::CODE_BITS).unwrap();
        let n = w.finish();
        match decode_msg(&buf[..n]).unwrap() {
            Msg::Hello { proto, stamp, .. } => assert_eq!((proto, stamp), (23, 0)),
            m => panic!("{m:?}"),
        }
    }
}

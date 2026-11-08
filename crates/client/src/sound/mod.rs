//! Sound. Every effect goes through our own mixer (`crates/mix`), the game
//! deciding each voice's ear gains, pitch and filter: on the desktop one
//! stream Bevy plays, in a browser the browser's audio thread (an
//! AudioWorklet: `web/mix.js`, `web/mix-worklet.js`, `crates/mix-worklet`),
//! never a timer on the page's own.
//!
//! - **Where a sound is:** its gain falls with distance (a near-field
//!   `near / (near + d)` times `(1 − d/reach)²`, silent at its reach), it
//!   pans equal-power by where it is from the ear, it darkens with distance
//!   and behind us, and a wall between us muffles it (`Map::clear`). Past
//!   [`FAR_M`] a shot is its far take, and a far sound arrives late, at the
//!   speed of sound.
//! - **What moves:** a vehicle's engine and a rocket in flight are held
//!   loops that follow them, pitched by speed and by Doppler.
//! - **Our own body:** the shield's hit, its break, Halo 2's alarm while
//!   it's down and the whine as it comes back; reloads (a hot gun's vent),
//!   the swap, the scope, a dry trigger, a charge's hum; steps by what we
//!   stand on (sand, grass, gravel, rock, metal, concrete, water), the jump, a
//!   launcher's throw, the landing; the spawn.
//! - **Everyone else's**, as far as we were sent them: shots, steps,
//!   landings, reloads, a shield breaking or coming back, a death, a spawn,
//!   rounds passing our head or striking near us, ricochets, pickups, a blow
//!   landing, a body run down, getting in and out of a vehicle, hits on a hull,
//!   grenades bouncing.
//! - **The match:** the countdown's ticks and its start; wind on a big map,
//!   the sea as we near it, a hum in an arena.
//! - **The menus:** a button come onto or pressed, a match found, an invite.
//!
//! All of it is ours (`tools/sfx.mjs`). The announcer is apart: our voiced
//! medals and its lines about the match (the mode, a flag, the lead, the clock's
//! last calls, a betrayal, the end), one at a time (`assets/sound/voice`,
//! `tools/voice.mjs`), a newer rung of a ladder taking the place of one still
//! waiting. So is the theme, under the title (`assets/sound/music`): too
//! long for the mixer's bank, it is streamed (Bevy's audio on the desktop,
//! the browser's own player on the page: `web/mix.js`).
//!
//! What to hear is decided every frame on every build ([`decide`]). It is
//! played live always in a browser and on the desktop with the `sound`
//! feature (it needs ALSA's headers on Linux) ([`on`]); a film mixes the
//! one-shots itself from [`Cues`], on its own frame clock (`film/mix.rs`).

use bevy::prelude::*;

/// This frame's events, for the ear (`happened` in lib.rs fills it).
#[derive(Resource, Default)]
pub struct Heard(pub Vec<proto::EventState>);

pub struct SoundPlugin;

impl Plugin for SoundPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Heard>()
            .insert_resource(Ear::new())
            .init_resource::<Cues>()
            .add_systems(
                Update,
                // After the trucks: their sounds this frame (`look/truck.rs`).
                (menus, decide.after(crate::happened).after(crate::look::truck::husks)).chain(),
            );
        #[cfg(any(target_arch = "wasm32", feature = "sound"))]
        on::build(app);
    }
}

/// One sound started this frame, for a film's mix (`film/mix.rs`): which
/// cut (`assets/sound/<name>.ogg`), where (none: ours, unplaced), how loud
/// before the player's volume, and how fast.
#[derive(Clone, Debug, PartialEq)]
pub struct Cue {
    pub name: String,
    pub at: Option<Vec3>,
    pub gain: f32,
    pub speed: f32,
}

/// This frame's cues.
#[derive(Resource, Default)]
pub struct Cues(pub Vec<Cue>);

use core::cmp::Ordering;
use std::collections::BTreeMap;
use std::sync::Arc;

use client_net::core::{Drawn, NetCore};
use sim::arms;
use sim::consts::{MAX_FIGHTERS, MAX_VEHICLES, SHIELD_MAX, TICK_HZ};
use sim::events::Kind;
use sim::geom::V3;

use crate::look::fx::surface::{self, Near, On};
use crate::look::fx::table::Matter;
use crate::look::fx::HEARD;
use crate::look::guns::GUNS;
use crate::LinkRes;
use mix::{Cmd, Place, Shared};

/// Past this, a shot is its far take.
pub const FAR_M: f32 = 35.0;
/// A film's mix (`film/mix.rs`): metres to its inverse square's units, and
/// its level; the announcer's level too.
pub const SCALE: f32 = 0.15;
pub const MASTER: f32 = 0.6;
/// The live mixer's level, before the player's volume.
const LIVE: f32 = 0.9;
/// Metres a second, and how much more than real a pitch bends by it.
const SOUND_SPEED: f32 = 343.0;
const DOPPLER: f32 = 1.6;
/// Sound arrives late only past this (closer, it would read as lag).
const DELAY_FROM_M: f32 = 25.0;
/// How far toward one ear a sound to the side goes (1: all of it).
const PAN: f32 = 0.85;
/// The low-pass corner near us, and through a wall.
const OPEN_HZ: f32 = 16_000.0;
const WALLED_HZ: f32 = 900.0;
const WALLED_GAIN: f32 = 0.5;
/// Halo 2's alarm while the shield is down: one bip this often.
const BEEP_S: f32 = 0.28;
/// A stride, in metres, and the slowest a body steps at.
const STRIDE_M: f32 = 2.6;
const STEP_SPEED: f32 = 2.5;

/// How a kind of sound carries: full to about `near` m, gone at `reach`.
#[derive(Clone, Copy)]
struct Carry {
    near: f32,
    reach: f32,
}
const GUN: Carry = Carry {
    near: 25.0,
    reach: 260.0,
};
const BLAST: Carry = Carry {
    near: 40.0,
    reach: 320.0,
};
const STEP: Carry = Carry { near: 4.0, reach: 28.0 };
const BODY: Carry = Carry { near: 4.0, reach: 22.0 };
const SHIELD: Carry = Carry { near: 6.0, reach: 40.0 };
/// The one-seaters' engines over a truck's (`whine`): the skimmer's, the
/// flier's, and either boosting.
const SKIM_WHINE: f32 = 1.7;
const FLY_WHINE: f32 = 1.35;
const BOOST_WHINE: f32 = 1.25;
const ENGINE: Carry = Carry {
    near: 10.0,
    reach: 90.0,
};
const FLIGHT: Carry = Carry {
    near: 10.0,
    reach: 120.0,
};
const SMALL: Carry = Carry { near: 5.0, reach: 35.0 };
/// The shotgun's pump comes this long after its shot (the viewmodel racks
/// it with it: `look::viewmodel::PUMP_AT_S`).
pub const PUMP_AT_S: f32 = 0.3;
/// The hits of one frame on one body are one sound (a shotgun's pellets
/// are ten), each hit past the first this much louder, to at most this.
const HITS_LOUDER: f32 = 0.12;
const HITS_LOUDEST: f32 = 1.5;
const PASS: Carry = Carry { near: 1.5, reach: 6.0 };

/// The held slots.
const SLOT_WIND: u8 = 0;
const SLOT_CHARGE: u8 = 1;
const SLOT_OWN_ENGINE: u8 = 2;
const SLOT_ENGINE: u8 = 3;
/// The nearest tyres skidding and the nearest fire (a hull's, a husk's):
/// one each.
const SLOT_SKID: u8 = SLOT_ENGINE + MAX_VEHICLES as u8;
const SLOT_FIRE: u8 = SLOT_SKID + 1;
/// The sea, and an arena's hum.
const SLOT_SURF: u8 = SLOT_FIRE + 1;
const SLOT_ROOM: u8 = SLOT_SURF + 1;
const SLOT_PROJ: u8 = SLOT_ROOM + 1;
const PROJ_LOOPS: usize = mix::LOOPS - SLOT_PROJ as usize;

/// A remote fighter, as last heard.
#[derive(Clone, Copy, Default)]
struct Body {
    seen: bool,
    alive: bool,
    grounded: bool,
    reloading: bool,
    shield: u8,
    charging: bool,
    fall: f32,
    step: f32,
}

/// A vehicle, as last heard: where, and its speed (smoothed).
#[derive(Clone, Copy, Default)]
struct Hull {
    on: bool,
    pos: Vec3,
    vel: Vec3,
}

/// A projectile we follow across snapshots.
#[derive(Clone, Copy, Default)]
struct Round {
    kind: u8,
    owner: u8,
    pos: Vec3,
    vel: Vec3,
    slot: Option<u8>,
    /// A sticky: how long it has been stuck, seconds.
    stuck: f32,
}

/// What the ear remembers between frames, and which clips the mixer has.
#[derive(Resource, Default)]
pub struct Ear {
    /// The desktop's mixer; in a browser `on::post` takes the commands.
    mixer: Option<Arc<Shared>>,
    taken_elsewhere: bool,
    cmds: Vec<Cmd>,
    /// Every clip's id in the mixer's bank, whether the mixer has it yet,
    /// and its length (the announcer waits it out); clips still loading.
    ids: BTreeMap<String, u16>,
    ready: Vec<bool>,
    secs: Vec<f32>,
    loading: u32,
    /// This frame's one-shots, for a film (`decide` hands them to [`Cues`]),
    /// kept only while one runs.
    cues: Vec<Cue>,
    filming: bool,
    rng: u32,
    volume: f32,
    /// GAME SOUNDS (`Settings::effects`): the match's own, out in the world
    /// and ours; the mixer's master is VOLUME alone, so the announcer and
    /// the menus ([`Ear::ping`]) are under it and not this.
    world: f32,
    in_match: bool,
    /// The link (`front::LinkGen`) and welcome the match's sound is of: a
    /// new one (the title's backdrop after a match, a cup's next heat)
    /// starts from silence, as a link closing does.
    heard_of: (u32, u32),
    // Our own body.
    last_reload: bool,
    /// The turret we gun was venting.
    last_vent: bool,
    last_arm: u8,
    last_grounded: bool,
    last_mag: u16,
    last_shield: i32,
    last_alive: bool,
    last_vy: f32,
    recharging: bool,
    beep: f32,
    step: f32,
    dry: f32,
    charging: bool,
    last_gren: u8,
    last_scope: u8,
    /// What a step or a round's end looks through (`surface::cast`), kept.
    near: Near,
    // Everyone else, the hulls, the rounds in flight.
    bodies: [Body; MAX_FIGHTERS],
    hulls: [Hull; MAX_VEHICLES],
    own_engine: bool,
    skid: bool,
    fire: bool,
    rounds: Vec<Round>,
    /// The next snapshot's rounds, built here and swapped in.
    rounds_next: Vec<Round>,
    snap_tick: u32,
    held_proj: [bool; PROJ_LOOPS],
    // The match.
    last_phase: Option<sim::world::Phase>,
    last_count: u32,
    last_clock: u32,
    /// How our score stood against the best of the rest.
    standing: Option<Ordering>,
    wind: bool,
    room: bool,
    surf: bool,
    /// How loud the sea is, how loud it should be, and when we next look.
    surf_gain: f32,
    surf_want: f32,
    surf_look: f32,
    /// The announcer's lines about the match waiting, said before a medal,
    /// and how many it has said, all told.
    lines: Vec<&'static str>,
    said: u32,
    /// Commands sent the mixer, all told.
    sent: u32,
    /// Where each held slot was last sent (`flush` drops a move that says
    /// it again), and frames since every move went regardless.
    placed: [Option<Place>; mix::LOOPS],
    since_all: u32,
}

/// A clip's name, built on the stack (`base_k`): a sound played allocates
/// nothing. Names are ASCII; one past the room matches no clip.
#[derive(Clone, Copy)]
struct ClipName {
    buf: [u8; CLIP_NAME_MAX],
    len: usize,
}

const CLIP_NAME_MAX: usize = 48;

impl ClipName {
    /// `base`, then `k` in decimal if there is one.
    fn of(base: &str, k: Option<u8>) -> ClipName {
        let mut n = ClipName {
            buf: [0; CLIP_NAME_MAX],
            len: 0,
        };
        n.push(base.as_bytes());
        if let Some(k) = k {
            n.digits(k);
        }
        n
    }

    fn with(mut self, b: &[u8]) -> ClipName {
        self.push(b);
        self
    }

    fn digits(&mut self, k: u8) {
        let (h, t, o) = (k / 100, k / 10 % 10, k % 10);
        if h > 0 {
            self.push(&[b'0' + h]);
        }
        if h > 0 || t > 0 {
            self.push(&[b'0' + t]);
        }
        self.push(&[b'0' + o]);
    }

    fn push(&mut self, b: &[u8]) {
        let end = self.len + b.len();
        if end > CLIP_NAME_MAX {
            self.len = CLIP_NAME_MAX + 1;
            return;
        }
        self.buf[self.len..end].copy_from_slice(b);
        self.len = end;
    }
}

impl core::ops::Deref for ClipName {
    type Target = str;
    fn deref(&self) -> &str {
        self.buf
            .get(..self.len)
            .and_then(|b| core::str::from_utf8(b).ok())
            .unwrap_or("")
    }
}

impl From<&str> for ClipName {
    fn from(s: &str) -> ClipName {
        ClipName::of(s, None)
    }
}

/// Where we hear from, this frame.
struct Ears<'a> {
    at: Vec3,
    right: Vec3,
    fwd: Vec3,
    vel: Vec3,
    map: &'a sim::map::Map,
}

fn v(p: V3) -> Vec3 {
    Vec3::new(p.x, p.y, p.z)
}

impl Ears<'_> {
    /// Where a sound at `p` sits for us: its place and the frames until
    /// it arrives; None out of reach.
    fn place(&self, p: Vec3, gain: f32, c: Carry) -> Option<(Place, u32)> {
        let d = p - self.at;
        let dist = d.length();
        if dist >= c.reach {
            return None;
        }
        let fall = 1.0 - dist / c.reach;
        let mut g = gain * c.near / (c.near + dist) * fall * fall;
        let mut hz = OPEN_HZ * (1.0 - 0.8 * dist / c.reach);
        let (mut l, mut r) = (core::f32::consts::FRAC_1_SQRT_2, core::f32::consts::FRAC_1_SQRT_2);
        if dist > 0.3 {
            let n = d / dist;
            let p = n.dot(self.right).clamp(-1.0, 1.0) * PAN;
            let th = (p + 1.0) * core::f32::consts::FRAC_PI_4;
            (l, r) = (th.cos(), th.sin());
            if n.dot(self.fwd) < 0.0 {
                hz *= 0.6;
            }
        }
        if dist > 1.5 && !self.map.clear(sim_v(self.at), sim_v(p + Vec3::Y * 0.2)) {
            g *= WALLED_GAIN;
            hz = hz.min(WALLED_HZ);
        }
        let delay = ((dist - DELAY_FROM_M).max(0.0) / SOUND_SPEED * mix::RATE as f32) as u32;
        Some((
            Place {
                l: l * g,
                r: r * g,
                rate: 1.0,
                lp: mix::lp_coeff(hz),
            },
            delay,
        ))
    }

    /// A moving source's pitch: Doppler, from both our velocities.
    fn doppler(&self, p: Vec3, vel: Vec3) -> f32 {
        let d = self.at - p;
        let dist = d.length();
        if dist < 0.5 {
            return 1.0;
        }
        let n = d / dist;
        let toward_us = vel.dot(n) * DOPPLER;
        let toward_it = -self.vel.dot(n) * DOPPLER;
        ((SOUND_SPEED + toward_it) / (SOUND_SPEED - toward_us).max(60.0)).clamp(0.6, 1.7)
    }

    fn walled(&self, p: Vec3) -> bool {
        !self.map.clear(sim_v(self.at), sim_v(p))
    }
}

fn sim_v(p: Vec3) -> V3 {
    V3::new(p.x, p.y, p.z)
}

impl Ear {
    fn new() -> Ear {
        let ids: BTreeMap<String, u16> = names()
            .into_iter()
            .chain(voices())
            .take(mix::CLIPS)
            .enumerate()
            .map(|(i, n)| (n, i as u16))
            .collect();
        Ear {
            ready: vec![false; ids.len()],
            secs: vec![0.0; ids.len()],
            ids,
            cmds: Vec::with_capacity(256),
            // Not a level: the first frame's goes to the mixer whatever it
            // is (it starts at full; a 0 would never have been sent).
            volume: -1.0,
            ..Ear::default()
        }
    }

    /// Clips handed to the mixer, clips still loading, commands sent it,
    /// lines about the match the announcer has said.
    pub fn stats(&self) -> (u32, u32, u32, u32) {
        let ready = self.ready.iter().filter(|r| **r).count() as u32;
        (ready, self.loading, self.sent, self.said)
    }

    /// Clip `name` is the mixer's now (or will be by the time it plays:
    /// a browser decodes it off the page's thread), `secs` long.
    #[cfg_attr(not(any(target_arch = "wasm32", feature = "sound")), allow(dead_code))]
    fn handed(&mut self, name: &str, secs: f32) -> Option<u16> {
        let id = *self.ids.get(name)?;
        self.ready[id as usize] = true;
        self.secs[id as usize] = secs;
        Some(id)
    }

    fn roll(&mut self, n: u8) -> u8 {
        // Cosmetic only: which take, and a little pitch.
        if self.rng == 0 {
            self.rng = 0x9e37_79b9;
        }
        self.rng = self.rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        ((self.rng >> 16) % n.max(1) as u32) as u8
    }
    fn pitch(&mut self) -> f32 {
        0.96 + self.roll(9) as f32 * 0.01
    }
    fn chance(&mut self, pct: u8) -> bool {
        self.roll(100) < pct
    }
    /// A line about the match for the announcer; news of the lead takes
    /// the place of older news of it still waiting.
    fn say(&mut self, line: &'static str) {
        let lead = |l: &str| l.ends_with("_lead");
        match self.lines.iter_mut().find(|q| lead(q) && lead(line)) {
            Some(q) => *q = line,
            None => self.lines.push(line),
        }
        if self.lines.len() > VOICE_WAIT {
            self.lines.remove(0);
        }
    }
    fn clip(&self, name: &str) -> Option<u16> {
        let id = *self.ids.get(name)?;
        self.ready[id as usize].then_some(id)
    }
    /// One take of `base` (`base_0` … `base_{n-1}`).
    fn take(&mut self, base: &str, n: u8) -> ClipName {
        let k = self.roll(n);
        let mut name = ClipName::of(base, None).with(b"_");
        name.digits(k);
        name
    }

    /// A sound from outside the match (`front.rs`), in our head: under
    /// VOLUME, not GAME SOUNDS.
    pub fn ping(&mut self, name: &str) {
        self.head(name, 0.6, 1.0);
    }

    /// A sound of our own, in our head.
    fn mine(&mut self, name: &str, gain: f32) {
        self.head(name, gain, self.world);
    }

    /// [`Ear::mine`], `secs` from now.
    fn mine_later(&mut self, name: &str, gain: f32, secs: f32) {
        let speed = self.pitch();
        self.cue(name, None, gain, speed);
        let Some(clip) = self.clip(name) else { return };
        let mut at = Place::centre(gain * self.world);
        at.rate = speed;
        let delay = (secs.max(0.0) * mix::RATE as f32) as u32;
        self.cmds.push(Cmd::Play { clip, at, delay });
    }

    fn head(&mut self, name: &str, gain: f32, level: f32) {
        let speed = self.pitch();
        self.cue(name, None, gain, speed);
        let Some(clip) = self.clip(name) else { return };
        let mut at = Place::centre(gain * level);
        at.rate = speed;
        self.cmds.push(Cmd::Play { clip, at, delay: 0 });
    }

    /// A place in the match at GAME SOUNDS' level.
    fn game(&self, mut at: Place) -> Place {
        at.l *= self.world;
        at.r *= self.world;
        at
    }

    fn cue(&mut self, name: &str, at: Option<Vec3>, gain: f32, speed: f32) {
        if !self.filming {
            return;
        }
        self.cues.push(Cue {
            name: name.to_string(),
            at,
            gain,
            speed,
        });
    }

    /// A sound out in the world.
    fn at(&mut self, ears: &Ears, name: &str, p: Vec3, gain: f32, c: Carry) {
        let Some((mut at, delay)) = ears.place(p, gain, c) else {
            return;
        };
        let speed = self.pitch();
        self.cue(name, Some(p), gain, speed);
        let Some(clip) = self.clip(name) else { return };
        at.rate = speed;
        let at = self.game(at);
        self.cmds.push(Cmd::Play { clip, at, delay });
    }

    /// [`Ear::at`] at `rate` and `secs` later than its distance says.
    #[allow(clippy::too_many_arguments)]
    fn at_rate(&mut self, ears: &Ears, name: &str, p: Vec3, gain: f32, c: Carry, secs: f32, rate: f32) {
        let Some((mut at, delay)) = ears.place(p, gain, c) else {
            return;
        };
        self.cue(name, Some(p), gain, rate);
        let Some(clip) = self.clip(name) else { return };
        at.rate = rate;
        let at = self.game(at);
        let delay = delay + (secs.max(0.0) * mix::RATE as f32) as u32;
        self.cmds.push(Cmd::Play { clip, at, delay });
    }

    /// Keep a held slot going at `p` (start it if it's quiet), or stop it.
    fn hold(
        &mut self,
        ears: Option<&Ears>,
        slot: u8,
        on: &mut bool,
        name: &str,
        src: Option<(Vec3, Vec3, f32, f32, Carry)>,
    ) {
        let place = match (ears, src) {
            (Some(e), Some((p, vel, gain, rate, c))) => e.place(p, gain, c).map(|(mut at, _)| {
                at.rate = rate * e.doppler(p, vel);
                self.game(at)
            }),
            _ => None,
        };
        match (place, *on) {
            (Some(at), true) => self.cmds.push(Cmd::Move { slot, at }),
            (Some(at), false) => {
                if let Some(clip) = self.clip(name) {
                    self.cmds.push(Cmd::Hold { slot, clip, at });
                    *on = true;
                }
            }
            (None, true) => {
                self.cmds.push(Cmd::Stop { slot });
                *on = false;
            }
            (None, false) => {}
        }
    }

    /// A sound we hold in our head (the charge, the wind).
    fn hold_mine(&mut self, slot: u8, on: &mut bool, name: &str, want: Option<(f32, f32)>) {
        let want = want.map(|(gain, rate)| (gain * self.world, rate));
        match (want, *on) {
            (Some((gain, rate)), true) => {
                let mut at = Place::centre(gain);
                at.rate = rate;
                self.cmds.push(Cmd::Move { slot, at });
            }
            (Some((gain, rate)), false) => {
                if let Some(clip) = self.clip(name) {
                    let mut at = Place::centre(gain);
                    at.rate = rate;
                    self.cmds.push(Cmd::Hold { slot, clip, at });
                    *on = true;
                }
            }
            (None, true) => {
                self.cmds.push(Cmd::Stop { slot });
                *on = false;
            }
            (None, false) => {}
        }
    }
}

/// Every clip a cue may name (the announcer's are `on::load`'s).
pub fn names() -> Vec<String> {
    let mut names: Vec<String> = [
        "dry_fire_0",
        "headshot_0",
        "headshot_1",
        "hit_0",
        "kill_0",
        "pump_shotgun_0",
        "gren_switch_0",
        "scope_in_0",
        "scope_out_0",
        "launch_0",
        "over_on_0",
        "camo_on_0",
        "ui_hover_0",
        "ui_click_0",
        "match_found_0",
        "invite_0",
        "shield_beep_0",
        "shield_charge_0",
        "shield_pop_0",
        "shot_ion_charged_0",
        "charge_hold_0",
        "sword_on_0",
        "vent_0",
        "engine_0",
        "rocket_fly_0",
        "truck_land_0",
        "truck_land_1",
        "crash_0",
        "skid_0",
        "fire_0",
        "sticky_sizzle_0",
        "sticky_stick_0",
        "sticky_whine_0",
        "wind_0",
        "surf_0",
        "room_hum_0",
        "spawn_0",
        "tick_0",
        "go_0",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    for (base, n) in [
        ("shot_minigun", 3),
        ("shot_minigun_far", 1),
        ("step_concrete", 8),
        ("step_metal", 8),
        ("step_grass", 8),
        ("step_rock", 8),
        ("step_sand", 8),
        ("step_gravel", 8),
        ("step_water", 8),
        ("flyby", 4),
        ("ricochet", 4),
        ("impact_metal", 5),
        ("bullet_concrete", 3),
        ("bullet_metal", 3),
        ("bullet_dirt", 3),
        ("bolt_hit", 2),
        ("blast", 3),
        ("flesh", 3),
        ("hurt", 2),
        ("death", 3),
        ("land", 2),
        ("jump", 2),
        ("splat", 2),
        ("swap", 3),
        ("pickup", 2),
        ("grenades", 2),
        ("ride", 2),
        ("swing", 3),
        ("melee_hit", 3),
        ("throw", 3),
        ("shield_hit", 3),
        ("tink", 3),
        ("sword_swing", 2),
    ] {
        for k in 0..n {
            names.push(format!("{base}_{k}"));
        }
    }
    for (_, g) in &GUNS {
        for k in 0..g.takes {
            names.push(format!("{}_{k}", g.sound));
        }
        names.push(format!("{}_0", g.far));
        names.push(g.reload.to_string());
    }
    names.sort();
    names.dedup();
    names
}

/// What a body stands on, for its steps: water over its feet, or the face
/// under them as a round's mark reads it (`look/fx/surface.rs`): a box or a
/// solid by its look, a terrain by what it is drawn as there (`level.rs`), or
/// the arena floor's plates.
fn ground(near: &mut Near, map: &sim::map::Map, p: V3) -> &'static str {
    if p.y < map.light.sea {
        return "step_water";
    }
    let look = match surface::cast(map, near, v(p) + Vec3::Y * 0.3, Vec3::NEG_Y, 0.8).map(|h| h.on) {
        Some(On::Box(i)) => map.looks.get(i as usize),
        Some(On::Solid(i)) => map.solid_looks.get(i as usize),
        _ if map.terrain.is_none() => return "step_metal",
        _ => {
            let w = crate::level::splat_at(map, p.x, p.z);
            let most = (0..w.len()).max_by(|a, b| w[*a].total_cmp(&w[*b])).unwrap_or(0);
            return ["step_sand", "step_grass", "step_gravel", "step_rock"][most];
        }
    };
    // tools/maps/compile.mjs LOOKS: wall, stone, stone_dark; rock.
    match look {
        Some(0 | 30 | 31) => "step_concrete",
        Some(6) => "step_rock",
        _ => "step_metal",
    }
}

/// How near a launcher's throw (`Map::launchers`) a body just off the ground
/// must be going to have been thrown by it, m/s.
const LAUNCHED_NEAR: f32 = 2.0;

/// Whether a body just off the ground, going at `vel`, was thrown.
fn launched(map: &sim::map::Map, vel: V3) -> bool {
    map.launchers.iter().any(|l| (vel - l.vel).len() < LAUNCHED_NEAR)
}

fn gun_takes(arm: u8) -> Option<&'static crate::look::guns::Gun> {
    crate::look::guns::gun(arm)
}

/// A gun's reload, or its vent if it runs hot.
fn reload_of(arm: u8) -> &'static str {
    if arm != arms::NONE && arms::arm(arm).hot() {
        "vent_0"
    } else {
        gun_takes(arm).map_or("reload_rifle_0", |g| g.reload)
    }
}

/// This frame's sound: the events heard, our own body, everyone else, the
/// hulls and rounds, the match. Into the live mixer (silent under a film)
/// and, as one-shot [`Cues`], to a film's own mix.
#[allow(clippy::too_many_arguments)]
pub fn decide(
    link: NonSend<LinkRes>,
    heard: Res<Heard>,
    time: Res<Time>,
    mut ear: ResMut<Ear>,
    mut cues: ResMut<Cues>,
    cam: Query<&GlobalTransform, With<crate::look::rig::EyeCam>>,
    settings: Res<crate::settings::Settings>,
    film: Option<Res<crate::film::Film>>,
    seen: Res<crate::Seen>,
    look: Res<crate::Look>,
    mut trucks: ResMut<crate::look::truck::HullSound>,
    gen: Option<Res<crate::front::LinkGen>>,
    cover: Res<crate::loading::Cover>,
) {
    let ear = &mut *ear;
    ear.cues.clear();
    cues.0.clear();
    ear.filming = film.is_some();
    ear.world = settings.effects;
    let volume = if film.is_some() { 0.0 } else { settings.master * LIVE };
    if volume != ear.volume {
        ear.volume = volume;
        ear.cmds.push(Cmd::Master(volume));
    }
    let s = link.0.session();
    let of = (gen.map_or(0, |g| g.0), s.welcomes);
    if !s.ready || of != ear.heard_of {
        if ear.in_match {
            for slot in 0..mix::LOOPS as u8 {
                ear.cmds.push(Cmd::Stop { slot });
            }
            ear.in_match = false;
            ear.own_engine = false;
            ear.skid = false;
            ear.fire = false;
            ear.charging = false;
            ear.wind = false;
            ear.room = false;
            ear.surf = false;
            ear.surf_gain = 0.0;
            ear.surf_want = 0.0;
            ear.surf_look = 0.0;
            ear.held_proj = [false; PROJ_LOOPS];
            ear.hulls = [Hull::default(); MAX_VEHICLES];
            ear.rounds.clear();
            ear.bodies = [Body::default(); MAX_FIGHTERS];
        }
        ear.heard_of = of;
        if !s.ready {
            flush(ear);
            return;
        }
    }
    ear.in_match = true;
    let me = s.core.slot;
    let dt = time.delta_secs().min(0.1);
    let (f, feet) = s.core.own();
    let (f, feet) = (*f, v(feet));
    let (eye, right, fwd) = cam
        .iter()
        .next()
        .map_or((feet + Vec3::Y * 1.6, Vec3::X, Vec3::NEG_Z), |g| {
            (g.translation(), g.right().as_vec3(), g.forward().as_vec3())
        });
    let own_vehicle = s.core.own_vehicle();
    let my_vel = own_vehicle.map_or(v(f.body.vel), |(veh, _, _)| v(veh.vel));
    let ears = Ears {
        at: eye,
        right,
        fwd,
        vel: my_vel,
        map: &s.core.map,
    };

    events(ear, &ears, &heard.0, me);
    own_body(ear, &ears, &f, feet, dt, look.level);
    everyone(ear, &ears, &seen.others, dt);

    // Engines: ours (predicted), and every other hull with a driver.
    let mut on = ear.own_engine;
    let own_src = own_vehicle
        .filter(|(veh, _, _)| veh.seats[sim::vehicle::DRIVER] != 0)
        .map(|(veh, p, _)| {
            let speed = if veh.kind == sim::vehicle::FLIER {
                v(veh.vel).length()
            } else {
                (veh.vel.x * veh.vel.x + veh.vel.z * veh.vel.z).sqrt()
            };
            (
                v(p) + Vec3::Y * 0.6,
                Vec3::ZERO,
                0.5 + speed / 40.0,
                (0.75 + speed / 22.0) * whine(veh.kind, veh.boosting),
                ENGINE,
            )
        });
    ear.hold(Some(&ears), SLOT_OWN_ENGINE, &mut on, "engine_0", own_src);
    ear.own_engine = on;
    let mut seen_hull = [false; MAX_VEHICLES];
    for hv in &seen.vehicles {
        let i = hv.slot as usize;
        if i >= MAX_VEHICLES {
            continue;
        }
        seen_hull[i] = true;
        let p = v(hv.pos);
        let h = &mut ear.hulls[i];
        let raw = if dt > 0.0 && h.pos != Vec3::ZERO {
            (p - h.pos) / dt
        } else {
            Vec3::ZERO
        };
        // Smoothed: drawn positions step with the snapshots.
        h.vel += (raw.clamp_length_max(60.0) - h.vel) * (dt / 0.15).min(1.0);
        h.pos = p;
        let vel = h.vel;
        let driven = hv.seats[sim::vehicle::DRIVER] != 0 && hv.health > 0;
        let speed = if hv.kind == sim::vehicle::FLIER {
            vel.length()
        } else {
            Vec3::new(vel.x, 0.0, vel.z).length()
        };
        let rate = (0.75 + speed / 22.0) * whine(hv.kind, hv.boosting);
        let src = driven.then_some((p + Vec3::Y * 0.6, vel, 0.5 + speed / 40.0, rate, ENGINE));
        let mut on = ear.hulls[i].on;
        ear.hold(Some(&ears), SLOT_ENGINE + i as u8, &mut on, "engine_0", src);
        ear.hulls[i].on = on;
    }
    for (i, was) in seen_hull.iter().enumerate() {
        if !was {
            let mut on = ear.hulls[i].on;
            ear.hold(None, SLOT_ENGINE + i as u8, &mut on, "engine_0", None);
            ear.hulls[i] = Hull::default();
        }
    }

    hull_sounds(ear, &ears, &mut trucks);
    rounds(ear, &ears, s.core.latest());
    // Under the loading overlay the countdown isn't heard yet (nor the
    // room): the announcer calls the match as it lifts.
    if !cover.up {
        the_match(ear, &s.core, s.core.map.terrain.is_some());
    }
    surf(ear, &ears, dt);
    cues.0.append(&mut ear.cues);
    flush(ear);
}

/// Pieces of a wreck heard coming down after it.
const DEBRIS: u8 = 5;
/// Shards going off together: the blast's take this loud, this much
/// higher, and the shards' pings after it.
const SUPER_GAIN: f32 = 1.0;
const SUPER_RATE: f32 = 1.3;
const SUPER_PINGS: u8 = 4;
/// How a truck's tyres and its fire carry.
const SKID: Carry = Carry { near: 6.0, reach: 70.0 };
const FIRE: Carry = Carry { near: 4.0, reach: 45.0 };

/// The trucks' own (`look/truck.rs`): a landing's slam, a crash, a husk
/// striking the ground; the nearest tyres skidding and fire burning, held.
/// How much higher a hull's engine runs than a truck's: the one-seaters'
/// turbines whine (the skimmer's highest), higher still boosting.
fn whine(kind: u8, boosting: bool) -> f32 {
    let k = match kind {
        sim::vehicle::SKIMMER => SKIM_WHINE,
        sim::vehicle::FLIER => FLY_WHINE,
        _ => 1.0,
    };
    if boosting {
        k * BOOST_WHINE
    } else {
        k
    }
}

fn hull_sounds(ear: &mut Ear, ears: &Ears, trucks: &mut crate::look::truck::HullSound) {
    use crate::look::truck::Clunk;
    for (what, at, k) in trucks.hits.drain(..) {
        match what {
            Clunk::Land => {
                let n = ear.take("truck_land", 2);
                ear.at_rate(ears, &n, at, 0.45 + 0.55 * k, SMALL, 0.0, 1.08 - 0.16 * k);
            }
            Clunk::Crash => {
                ear.at_rate(ears, "crash_0", at, 0.4 + 0.6 * k, SMALL, 0.0, 1.1 - 0.2 * k);
            }
            Clunk::Bounce => {
                let n = ear.take("impact_metal", 5);
                ear.at_rate(ears, &n, at, 0.3 + 0.5 * k, SMALL, 0.0, 0.6 + 0.2 * (1.0 - k));
                if k > 0.4 {
                    let n = ear.take("truck_land", 2);
                    ear.at_rate(ears, &n, at, 0.5 * k, SMALL, 0.0, 0.85);
                }
            }
        }
    }
    let nearest = |list: &[(Vec3, f32)]| {
        list.iter()
            .copied()
            .min_by(|a, b| a.0.distance_squared(ears.at).total_cmp(&b.0.distance_squared(ears.at)))
    };
    let skid = nearest(&trucks.skids).map(|(p, k)| (p, Vec3::ZERO, 0.25 + 0.6 * k, 0.9 + 0.2 * k, SKID));
    let mut on = ear.skid;
    ear.hold(Some(ears), SLOT_SKID, &mut on, "skid_0", skid);
    ear.skid = on;
    let fire = nearest(&trucks.fires).map(|(p, k)| (p, Vec3::ZERO, 0.3 + 0.5 * k, 0.9 + 0.15 * k, FIRE));
    let mut on = ear.fire;
    ear.hold(Some(ears), SLOT_FIRE, &mut on, "fire_0", fire);
    ear.fire = on;
}

fn flush(ear: &mut Ear) {
    drop_still_moves(ear);
    if let Some(m) = &ear.mixer {
        ear.sent = ear.sent.wrapping_add(ear.cmds.len() as u32);
        m.send(&mut ear.cmds);
    } else if !ear.taken_elsewhere {
        ear.cmds.clear();
    }
}

/// Frames between moves sent whether or not they moved: a command the
/// mixer's full inbox dropped is put right within this.
const MOVES_ALL_EVERY: u32 = 64;
/// A held slot's move nearer the last one sent than this (gain, rate, the
/// filter's coefficient) says nothing new.
const MOVE_EPS: f32 = 1e-4;

/// Drops this frame's moves that put a held slot where the last one sent
/// did: the wind, a parked engine and a held charge say the same thing
/// every frame, and each is a lock on the desktop and a message to the audio
/// thread in a browser.
fn drop_still_moves(ear: &mut Ear) {
    ear.since_all += 1;
    if ear.since_all >= MOVES_ALL_EVERY {
        ear.since_all = 0;
        ear.placed = [None; mix::LOOPS];
    }
    let same = |a: &Place, b: &Place| {
        (a.l - b.l).abs() < MOVE_EPS
            && (a.r - b.r).abs() < MOVE_EPS
            && (a.rate - b.rate).abs() < MOVE_EPS
            && (a.lp - b.lp).abs() < MOVE_EPS
    };
    let Ear { cmds, placed, .. } = ear;
    cmds.retain(|c| match c {
        Cmd::Hold { slot, at, .. } => {
            if let Some(p) = placed.get_mut(*slot as usize) {
                *p = Some(*at);
            }
            true
        }
        Cmd::Move { slot, at } => match placed.get_mut(*slot as usize) {
            Some(p) if p.as_ref().is_some_and(|was| same(was, at)) => false,
            Some(p) => {
                *p = Some(*at);
                true
            }
            None => true,
        },
        Cmd::Stop { slot } => {
            if let Some(p) = placed.get_mut(*slot as usize) {
                *p = None;
            }
            true
        }
        _ => true,
    });
}

/// Lines the announcer may have waiting; past it the oldest goes.
const VOICE_WAIT: usize = 4;
/// What the announcer says about the match, past the medals (recorded:
/// `tools/voice.sh cut`).
const LINES: [&str; 19] = [
    "slayer",
    "free_for_all",
    "team_slayer",
    "gun_game",
    "capture_the_flag",
    "flag_taken",
    "flag_dropped",
    "flag_returned",
    "flag_captured",
    "one_minute",
    "thirty_seconds",
    "taken_lead",
    "lost_lead",
    "tied_for_lead",
    "game_over",
    "game_over_you_win",
    "game_over_you_lose",
    "betrayal",
    "betrayed",
];
/// The clock's last calls: seconds left, and the line.
const CALLS: [(u32, &str); 2] = [(60, "one_minute"), (30, "thirty_seconds")];

/// The announcer's lines, by name: the voiced medals, then the match's.
fn voices() -> impl Iterator<Item = String> {
    (1..proto::medal::COUNT as u8)
        .filter(|id| proto::medal::info(*id).voiced)
        .map(proto::medal::slug)
        .chain(LINES.iter().map(|l| l.to_string()))
        .map(|n| format!("voice/{n}"))
}

/// What the announcer calls a match as its countdown starts.
fn mode_line(r: &sim::world::Rules) -> &'static str {
    if r.ctf {
        "capture_the_flag"
    } else if r.gun_game {
        "gun_game"
    } else if r.teams {
        "team_slayer"
    } else if r.seats > 2 {
        "free_for_all"
    } else {
        "slayer"
    }
}

/// What the announcer says of a flag event (`Kind::Flag`'s `part`).
fn flag_line(part: u8) -> &'static str {
    match part {
        0 => "flag_taken",
        1 => "flag_dropped",
        2 => "flag_returned",
        _ => "flag_captured",
    }
}

/// What the announcer says when our score's standing against the best of
/// the rest changes: the lead taken, tied for, or lost. Nothing the first
/// time it is looked at.
fn lead_line(was: Option<Ordering>, now: Ordering) -> Option<&'static str> {
    use Ordering::{Equal, Greater, Less};
    match (was?, now) {
        (Less | Equal, Greater) => Some("taken_lead"),
        (Less | Greater, Equal) => Some("tied_for_lead"),
        (Greater | Equal, Less) => Some("lost_lead"),
        _ => None,
    }
}

/// How long an Ogg Vorbis file plays: its last page's granule position
/// (samples) over the rate in its first header.
#[cfg_attr(not(any(target_arch = "wasm32", feature = "sound")), allow(dead_code))]
fn ogg_secs(b: &[u8]) -> Option<f32> {
    let id = b.windows(7).position(|w| w == b"\x01vorbis")?;
    let rate = u32::from_le_bytes(b.get(id + 12..id + 16)?.try_into().ok()?);
    let last = b.windows(4).rposition(|w| w == b"OggS")?;
    let gran = u64::from_le_bytes(b.get(last + 6..last + 14)?.try_into().ok()?);
    (rate > 0).then(|| gran as f32 / rate as f32)
}

/// How near us a round's end is heard striking, metres, and how many a frame.
const STRUCK_M: f32 = 30.0;
const STRUCK_MAX: u8 = 3;

/// A round ending on a face of the map near us, by what the face is made of. A
/// round that ended in a body met no face (the hit is the body's sound).
fn struck(ear: &mut Ear, ears: &Ears, from: Vec3, to: Vec3) {
    let dir = (to - from).normalize_or_zero();
    let Some(hit) = surface::struck(ears.map, &mut ear.near, to, dir) else {
        return;
    };
    let base = match surface::matter(ears.map, hit.on) {
        Matter::Metal | Matter::Hull => "bullet_metal",
        Matter::Dirt => "bullet_dirt",
        _ => "bullet_concrete",
    };
    let n = ear.take(base, 3);
    ear.at(ears, &n, to, 0.6, SMALL);
}

fn events(ear: &mut Ear, ears: &Ears, list: &[proto::EventState], me: u8) {
    let mut hull_hit = [false; MAX_VEHICLES];
    // Who swung a fist this frame: a hit of theirs is a blow landing.
    let mut swung = [false; MAX_FIGHTERS];
    let mut struck_left = STRUCK_MAX;
    // This frame's hits, by body: ours on each (its worst: 1 shield, 2
    // health, 3 the head), on us (1 shield, 2 health), and others' on a
    // shield, each with how many and where.
    let mut ours_on = [(0u8, 0u8); MAX_FIGHTERS];
    let mut on_us = (0u8, 0u8);
    let mut shield_on = [(0u8, Vec3::ZERO); MAX_FIGHTERS];
    for e in list {
        let at = Vec3::new(e.pos.x, e.pos.y, e.pos.z);
        match e.kind {
            k if k == Kind::Shot as u8 => {
                let Some(g) = crate::look::guns::heard_as(e.c) else {
                    continue;
                };
                let ours = e.a == me && e.part != HEARD;
                let (sound, takes, far) = if e.c == sim::events::Cause::Minigun as u16 {
                    ("shot_minigun", 3, "shot_minigun_far")
                } else {
                    (g.sound, g.takes, g.far)
                };
                let pump = e.c == arms::SHOTGUN as u16;
                if ours {
                    let n = ear.take(sound, takes);
                    ear.mine(&n, 0.75);
                    if pump {
                        ear.mine_later("pump_shotgun_0", 0.6, PUMP_AT_S);
                    }
                } else {
                    let far_off = at.distance(ears.at) > FAR_M || ears.walled(at);
                    let n = if far_off {
                        ClipName::of(far, None).with(b"_0")
                    } else {
                        ear.take(sound, takes)
                    };
                    ear.at(ears, &n, at, 1.0, GUN);
                    if pump && !far_off {
                        ear.at_rate(ears, "pump_shotgun_0", at, 0.5, BODY, PUMP_AT_S, 1.0);
                    }
                }
                if e.part == HEARD {
                    continue;
                }
                // A round past our head, one striking near us, one off a wall.
                let end = Vec3::new(e.end.x, e.end.y, e.end.z);
                let hitscan = arms::fired(e.c).is_some_and(|a| a.speed == 0.0);
                if hitscan {
                    let seg = end - at;
                    let len2 = seg.length_squared();
                    if !ours && len2 > 1.0 {
                        let u = ((ears.at - at).dot(seg) / len2).clamp(0.0, 1.0);
                        let near = at + seg * u;
                        let miss = near.distance(ears.at);
                        if u > 0.05 && u < 0.98 && miss < PASS.reach && near.distance(at) > 3.0 {
                            let n = ear.take("flyby", 4);
                            ear.at(ears, &n, near, 0.9, PASS);
                        }
                    }
                    if end.distance(ears.at) < STRUCK_M {
                        if struck_left > 0 {
                            struck_left -= 1;
                            struck(ear, ears, at, end);
                        }
                        if ear.chance(12) {
                            let n = ear.take("ricochet", 4);
                            ear.at(ears, &n, end, 0.5, SMALL);
                        }
                    }
                }
            }
            k if k == Kind::Hit as u8 => {
                // Only the two it is between are told of a hit.
                if swung.get(e.a as usize).copied().unwrap_or(false) {
                    let n = ear.take("melee_hit", 3);
                    ear.mine(&n, 0.7);
                }
                let shield = e.part == 0 || e.part == 3;
                let b = e.b as usize;
                if e.a == me && e.b != me && b < MAX_FIGHTERS {
                    let worst = match e.part {
                        2 => 3,
                        _ if shield => 1,
                        _ => 2,
                    };
                    let o = &mut ours_on[b];
                    *o = (o.0.max(worst), o.1.saturating_add(1));
                }
                if e.b == me {
                    on_us = (on_us.0.max(if shield { 1 } else { 2 }), on_us.1.saturating_add(1));
                } else if e.a != me && shield && b < MAX_FIGHTERS {
                    let o = &mut shield_on[b];
                    *o = (o.0.saturating_add(1), at);
                }
            }
            k if k == Kind::Kill as u8 => {
                // Ours: told apart from a hit.
                if e.a == me && e.b != me {
                    ear.mine("kill_0", 0.7);
                }
                // Run down, if we may know where.
                if e.c == sim::events::Cause::Splatter as u16 && at != Vec3::ZERO {
                    let n = ear.take("splat", 2);
                    ear.at(ears, &n, at, 1.0, SMALL);
                }
            }
            k if k == Kind::Spawn as u8 => {
                if e.a != me {
                    ear.at(ears, "spawn_0", at, 0.6, BODY);
                }
            }
            k if k == Kind::Flag as u8 => ear.say(flag_line(e.part)),
            k if k == Kind::Blast as u8 && e.c == sim::arms::SHARDS as u16 => {
                // Shards going off together: a sharp, high crack with a
                // shield's burst in it, and glass pinging away. Not fire.
                let n = ear.take("blast", 3);
                ear.at_rate(ears, &n, at, SUPER_GAIN, BLAST, 0.0, SUPER_RATE);
                ear.at(ears, "shield_pop_0", at, 1.0, SMALL);
                for k in 0..SUPER_PINGS {
                    let n = ear.take("shot_shards", 3);
                    let rate = 1.2 + ear.roll(50) as f32 * 0.01;
                    let secs = 0.04 + k as f32 * 0.07 + ear.roll(4) as f32 * 0.01;
                    ear.at_rate(ears, &n, at, 0.5, SMALL, secs, rate);
                }
            }
            k if k == Kind::Blast as u8 => {
                let n = ear.take("blast", 3);
                ear.at(ears, &n, at, 1.3, BLAST);
            }
            k if k == Kind::Wreck as u8 => {
                let n = ear.take("blast", 3);
                ear.at(ears, &n, at, 1.4, BLAST);
                let n = ear.take("impact_metal", 5);
                ear.at(ears, &n, at, 1.0, SMALL);
                ear.at_rate(ears, "crash_0", at, 0.9, BLAST, 0.0, 0.8);
                // What it threw coming down around it.
                for k in 0..DEBRIS {
                    let n = ear.take("impact_metal", 5);
                    let rate = 0.7 + ear.roll(40) as f32 * 0.01;
                    let secs = 0.6 + k as f32 * 0.22 + ear.roll(20) as f32 * 0.01;
                    let off = Vec3::new(ear.roll(9) as f32 - 4.0, 0.0, ear.roll(9) as f32 - 4.0);
                    ear.at_rate(ears, &n, at + off, 0.45 - k as f32 * 0.06, SMALL, secs, rate);
                }
            }
            k if k == Kind::Melee as u8 => {
                let sword = e.c == arms::SWORD as u16;
                if let Some(s) = swung.get_mut(e.a as usize) {
                    *s = !sword;
                }
                let n = match (sword, e.part) {
                    (true, 2) => ear.take("impact_metal", 5),
                    (true, _) => ear.take("sword_swing", 2),
                    _ => ear.take("swing", 3),
                };
                if e.a == me {
                    ear.mine(&n, 0.7);
                } else {
                    ear.at(ears, &n, at, 0.8, BODY);
                }
            }
            k if k == Kind::Throw as u8 => {
                let n = ear.take("throw", 3);
                if e.a == me && e.part != HEARD {
                    ear.mine(&n, 0.6);
                } else {
                    ear.at(ears, &n, at, 0.7, BODY);
                }
            }
            k if k == Kind::Pickup as u8 => {
                let n = match e.c as u8 {
                    arms::OVERSHIELD => "over_on_0".into(),
                    arms::CAMO => "camo_on_0".into(),
                    arms::FRAGS | arms::STICKIES => ear.take("grenades", 2),
                    _ => ear.take("pickup", 2),
                };
                if e.a == me {
                    ear.mine(&n, 0.6);
                } else {
                    ear.at(ears, &n, at, 0.6, BODY);
                }
            }
            k if k == Kind::Ride as u8 => {
                let n = ear.take("ride", 2);
                if e.a == me {
                    ear.mine(&n, 0.55);
                } else {
                    ear.at(ears, &n, at, 0.8, BODY);
                }
            }
            k if k == Kind::VehicleHit as u8 => {
                let i = e.part as usize;
                if i < MAX_VEHICLES && !hull_hit[i] {
                    hull_hit[i] = true;
                    let n = ear.take("impact_metal", 5);
                    ear.at(ears, &n, at, if e.a == me { 0.7 } else { 0.5 }, SMALL);
                }
            }
            _ => {}
        }
    }
    let louder = |n: u8| (1.0 + HITS_LOUDER * f32::from(n.saturating_sub(1))).min(HITS_LOUDEST);
    for &(worst, n) in &ours_on {
        let (clip, gain) = match worst {
            0 => continue,
            1 => (ear.take("shield_hit", 3), 0.35),
            2 => ("hit_0".into(), 0.5),
            _ => (ear.take("headshot", 2), 0.5),
        };
        ear.mine(&clip, gain * louder(n));
    }
    match on_us {
        (0, _) => {}
        (1, n) => {
            let clip = ear.take("shield_hit", 3);
            ear.mine(&clip, 0.55 * louder(n));
        }
        (_, n) => {
            let clip = ear.take("flesh", 3);
            ear.mine(&clip, 0.6 * louder(n));
        }
    }
    for &(n, at) in &shield_on {
        if n > 0 {
            let clip = ear.take("shield_hit", 3);
            ear.at(ears, &clip, at, 0.5 * louder(n), SHIELD);
        }
    }
}

fn own_body(ear: &mut Ear, ears: &Ears, f: &sim::fighter::Fighter, feet: Vec3, dt: f32, scope: u8) {
    // The shield: the hit is the event's; here, the break, the alarm
    // while it's down, and the whine as it comes back.
    if f.alive {
        if f.shield <= 0 && ear.last_shield > 0 {
            ear.mine("shield_pop_0", 0.7);
            let n = ear.take("hurt", 2);
            ear.mine(&n, 0.35);
            ear.beep = 0.15;
        }
        if f.shield > ear.last_shield && ear.last_alive {
            if !ear.recharging {
                ear.mine("shield_charge_0", 0.45);
            }
            ear.recharging = true;
        } else if f.shield < ear.last_shield || f.shield >= SHIELD_MAX {
            ear.recharging = false;
        }
        if f.shield <= 0 && !ear.recharging {
            ear.beep -= dt;
            if ear.beep <= 0.0 {
                ear.mine("shield_beep_0", 0.4);
                ear.beep += BEEP_S;
            }
        }
    }
    if !f.alive && ear.last_alive {
        let n = ear.take("death", 3);
        ear.mine(&n, 0.7);
    }
    if f.alive && !ear.last_alive {
        ear.mine("spawn_0", 0.45);
    }
    ear.last_shield = f.shield;
    ear.last_alive = f.alive;

    let g = *f.gun();
    let reloading = f.reload > 0;
    if reloading && !ear.last_reload {
        ear.mine(reload_of(g.arm), 0.7);
    }
    ear.last_reload = reloading;
    // The turret we gun running hot to its vent: the gun's own vent, louder.
    if f.mount.vent && !ear.last_vent {
        ear.mine("vent_0", 0.95);
    }
    ear.last_vent = f.mount.vent;
    if g.arm != ear.last_arm {
        if g.arm == arms::SWORD {
            ear.mine("sword_on_0", 0.5);
        } else {
            let n = ear.take("swap", 3);
            ear.mine(&n, 0.5);
        }
        ear.last_arm = g.arm;
    }
    if scope != ear.last_scope {
        ear.mine(
            if scope > ear.last_scope {
                "scope_in_0"
            } else {
                "scope_out_0"
            },
            0.5,
        );
        ear.last_scope = scope;
    }
    if f.gren_kind != ear.last_gren {
        if f.alive {
            ear.mine("gren_switch_0", 0.5);
        }
        ear.last_gren = f.gren_kind;
    }
    ear.dry -= dt;
    let firing = f.last_buttons & sim::input::FIRE != 0;
    if f.alive && g.mag == 0 && ear.last_mag == 0 && firing && ear.dry <= 0.0 && !reloading {
        ear.mine("dry_fire_0", 0.6);
        ear.dry = 0.3;
    }
    ear.last_mag = g.mag;
    // A charge's hum, rising as it fills.
    let charge = |arm: u8, t: u16| {
        let a = arms::arm(arm);
        (arm != arms::NONE && a.charge_ms > 0 && t > 0)
            .then(|| (t as f32 / (a.charge_ms as f32 * TICK_HZ as f32 / 1000.0)).min(1.0))
    };
    let fill = charge(g.arm, f.charge).or(charge(f.left.arm, f.left_charge));
    let mut on = ear.charging;
    ear.hold_mine(
        SLOT_CHARGE,
        &mut on,
        "charge_hold_0",
        fill.filter(|_| f.alive).map(|u| (0.25 + 0.2 * u, 0.7 + 0.6 * u)),
    );
    ear.charging = on;

    // Steps, the jump, the landing.
    let vel = f.body.vel;
    let speed = (vel.x * vel.x + vel.z * vel.z).sqrt();
    let grounded = f.body.grounded && f.riding().is_none();
    if f.alive && grounded && speed > STEP_SPEED {
        ear.step -= dt * speed;
        if ear.step <= 0.0 {
            let under = ground(&mut ear.near, ears.map, sim_v(feet));
            let n = ear.take(under, 8);
            ear.mine(&n, if f.body.crouched { 0.12 } else { 0.3 });
            ear.step = STRIDE_M;
        }
    }
    if f.alive && !grounded && ear.last_grounded && f.riding().is_none() {
        if launched(ears.map, vel) {
            ear.mine("launch_0", 0.7);
        } else if vel.y > 2.0 {
            let n = ear.take("jump", 2);
            ear.mine(&n, 0.25);
        }
    }
    if f.alive && grounded && !ear.last_grounded {
        let hard = (-ear.last_vy / 12.0).clamp(0.3, 1.0);
        let n = ear.take("land", 2);
        ear.mine(&n, 0.45 * hard);
        let under = ground(&mut ear.near, ears.map, sim_v(feet));
        let n = ear.take(under, 8);
        ear.mine(&n, 0.3 * hard);
        ear.step = STRIDE_M * 0.5;
    }
    ear.last_grounded = grounded;
    ear.last_vy = vel.y;
}

/// Everyone else we were sent: steps, landings, reloads, their shields, a
/// launcher's throw, a death.
fn everyone(ear: &mut Ear, ears: &Ears, others: &[Drawn], dt: f32) {
    let mut seen = [false; MAX_FIGHTERS];
    for o in others {
        let i = o.slot as usize;
        if i >= MAX_FIGHTERS {
            continue;
        }
        seen[i] = true;
        let p = v(o.pos);
        let b = ear.bodies[i];
        let mut nb = Body {
            seen: true,
            alive: o.alive,
            grounded: o.grounded,
            reloading: o.reloading,
            shield: o.shield,
            charging: b.charging,
            fall: o.vel.y,
            step: b.step,
        };
        if b.seen && o.alive && b.alive {
            if o.shield == 0 && b.shield > 0 {
                ear.at(ears, "shield_pop_0", p + Vec3::Y, 0.8, SHIELD);
            }
            if o.shield > b.shield && !b.charging {
                ear.at(ears, "shield_charge_0", p + Vec3::Y, 0.4, BODY);
                nb.charging = true;
            } else if o.shield < b.shield || o.shield == u8::MAX {
                nb.charging = false;
            }
            if o.reloading && !b.reloading {
                ear.at(ears, reload_of(o.arm), p + Vec3::Y, 0.7, BODY);
            }
            let speed = (o.vel.x * o.vel.x + o.vel.z * o.vel.z).sqrt();
            if o.grounded && speed > STEP_SPEED {
                nb.step -= dt * speed;
                if nb.step <= 0.0 {
                    let under = ground(&mut ear.near, ears.map, o.pos);
                    let n = ear.take(under, 8);
                    ear.at(ears, &n, p, if o.crouched { 0.15 } else { 0.6 }, STEP);
                    nb.step = STRIDE_M;
                }
            }
            if o.grounded && !b.grounded {
                let hard = (-b.fall / 12.0).clamp(0.3, 1.0);
                let n = ear.take("land", 2);
                ear.at(ears, &n, p, 0.7 * hard, STEP);
                nb.step = STRIDE_M * 0.5;
            }
            if !o.grounded && b.grounded && launched(ears.map, o.vel) {
                ear.at(ears, "launch_0", p, 0.9, SMALL);
            }
        } else if b.seen && b.alive && !o.alive {
            let n = ear.take("death", 3);
            ear.at(ears, &n, p + Vec3::Y, 0.9, SHIELD);
        }
        ear.bodies[i] = nb;
    }
    for (i, s) in seen.iter().enumerate() {
        if !s {
            ear.bodies[i] = Body::default();
        }
    }
}

/// A sticky's sizzle: how loud, and how much higher with its fuse run out.
const STICKY_GAIN: f32 = 1.8;
const STICKY_CLIMB: f32 = 0.5;
const STICKY_WHINE_GAIN: f32 = 0.7;
/// A charged bolt in flight: the same sizzle, lower.
const ORB_SIZZLE_GAIN: f32 = 1.4;
const ORB_SIZZLE_RATE: f32 = 0.72;
const STICKY_FUSE_S: f32 = sim::consts::STICKY_FUSE_MS as f32 / 1000.0;

/// The rounds in flight: a rocket's roar following it, a sticky's sizzle and
/// it landing, a grenade's bounce, a charged bolt's whoomp as it leaves.
fn rounds(ear: &mut Ear, ears: &Ears, snap: &proto::Snapshot) {
    const ROCKET: u8 = 0;
    const FRAG: u8 = 1;
    const BOLT: u8 = 2;
    const CHARGED: u8 = 3;
    const NEEDLE: u8 = 4;
    const LOB: u8 = 5;
    const CANNON: u8 = 6;
    const STICKY: u8 = 7;
    let fresh = snap.tick != ear.snap_tick;
    if fresh {
        let gap = (snap.tick.wrapping_sub(ear.snap_tick)).min(TICK_HZ) as f32 / TICK_HZ as f32;
        ear.snap_tick = snap.tick;
        let mut next = core::mem::take(&mut ear.rounds_next);
        next.clear();
        let mut taken = [false; 64];
        for p in &snap.proj[..snap.n_proj as usize] {
            let (pos, vel) = (v(p.pos), v(p.vel));
            // The one we followed: same kind and owner, nearest where it
            // should be now.
            let best = ear
                .rounds
                .iter()
                .enumerate()
                .filter(|(i, r)| *i < taken.len() && !taken[*i] && r.kind == p.kind && r.owner == p.owner)
                .map(|(i, r)| (i, (r.pos + r.vel * gap).distance(pos)))
                .filter(|(_, d)| *d < 4.0 + vel.length() * gap * 0.5)
                .min_by(|a, b| a.1.total_cmp(&b.1));
            let mut r = Round {
                kind: p.kind,
                owner: p.owner,
                pos,
                vel,
                slot: None,
                stuck: 0.0,
            };
            match best {
                Some((i, _)) => {
                    taken[i] = true;
                    let was = ear.rounds[i];
                    r.slot = was.slot;
                    // On the wire a stuck one's velocity is its offset.
                    if p.kind == STICKY && vel.length() < client_net::proj::STUCK_V {
                        r.stuck = was.stuck + gap;
                        if was.vel.length() >= client_net::proj::STUCK_V {
                            ear.at(ears, "sticky_stick_0", pos, 1.0, SMALL);
                            ear.at(ears, "sticky_whine_0", pos, STICKY_WHINE_GAIN, SMALL);
                        }
                    }
                    let bounced = (was.vel.y < -1.0 && vel.y > 0.3)
                        || Vec3::new(was.vel.x, 0.0, was.vel.z).dot(Vec3::new(vel.x, 0.0, vel.z)) < 0.0;
                    if (p.kind == FRAG || p.kind == LOB) && bounced {
                        let n = ear.take("tink", 3);
                        ear.at(ears, &n, pos, 0.8, SMALL);
                    }
                }
                None => {
                    if p.kind == CHARGED {
                        ear.at(ears, "shot_ion_charged_0", pos, 1.0, GUN);
                    }
                }
            }
            next.push(r);
        }
        // Gone: free its slot, and a bolt or a shard ends on what it met.
        for (i, kept) in taken.iter().enumerate().take(ear.rounds.len()) {
            if *kept {
                continue;
            }
            let r = ear.rounds[i];
            if let Some(s) = r.slot {
                ear.cmds.push(Cmd::Stop { slot: s });
                ear.held_proj[(s - SLOT_PROJ) as usize] = false;
            }
            if r.kind == BOLT || r.kind == NEEDLE {
                let n = ear.take("bolt_hit", 2);
                ear.at(ears, &n, r.pos, 0.7, SMALL);
            }
        }
        ear.rounds_next = core::mem::replace(&mut ear.rounds, next);
    }
    // The roar of what flies under its own power.
    for k in 0..ear.rounds.len() {
        let r = ear.rounds[k];
        if r.kind != ROCKET && r.kind != CANNON && r.kind != STICKY && r.kind != CHARGED {
            continue;
        }
        let slot = match r.slot {
            Some(s) => s,
            None => match ear.held_proj.iter().position(|b| !b) {
                Some(j) => SLOT_PROJ + j as u8,
                None => continue,
            },
        };
        let j = (slot - SLOT_PROJ) as usize;
        let mut on = ear.held_proj[j];
        let (clip, gain, rate) = match r.kind {
            CANNON => ("rocket_fly_0", 0.8, 0.7),
            STICKY => (
                "sticky_sizzle_0",
                STICKY_GAIN,
                1.0 + STICKY_CLIMB * (r.stuck / STICKY_FUSE_S).min(1.0),
            ),
            CHARGED => ("sticky_sizzle_0", ORB_SIZZLE_GAIN, ORB_SIZZLE_RATE),
            _ => ("rocket_fly_0", 0.8, 1.0),
        };
        ear.hold(
            Some(ears),
            slot,
            &mut on,
            clip,
            Some((r.pos, r.vel, gain, rate, FLIGHT)),
        );
        ear.held_proj[j] = on;
        ear.rounds[k].slot = on.then_some(slot);
    }
}

/// The countdown's ticks and its start, the wind on a big map and the hum in
/// an arena; and
/// what the announcer says of the match: its mode as the countdown starts,
/// who leads, the clock's last calls, the end.
fn the_match(ear: &mut Ear, net: &NetCore, big: bool) {
    use sim::world::Phase;
    let snap = net.latest();
    let phase = net.phase();
    if phase == Phase::Countdown {
        let secs = snap.clock.div_ceil(TICK_HZ);
        // A countdown begun (the title's backdrop has no clock): the mode.
        if secs > ear.last_count {
            ear.lines.clear();
            ear.say(mode_line(&net.rules));
        }
        if secs != ear.last_count && (1..=3).contains(&secs) {
            ear.mine("tick_0", 0.5);
        }
        ear.last_count = secs;
    } else if phase == Phase::Playing && ear.last_phase == Some(Phase::Countdown) {
        ear.mine("go_0", 0.55);
        ear.last_count = 0;
    } else if phase == Phase::Over && ear.last_phase == Some(Phase::Playing) {
        let won = if net.rules.teams {
            snap.winner & 1 == net.team & 1
        } else {
            snap.winner == net.slot
        };
        ear.lines.clear();
        ear.say(match (snap.winner == sim::world::NO_WINNER, won) {
            (true, _) => "game_over",
            (_, true) => "game_over_you_win",
            _ => "game_over_you_lose",
        });
    }
    ear.last_phase = Some(phase);
    if phase == Phase::Playing {
        // The lead: our score (our side's) against the best of the rest.
        let me = if net.rules.teams {
            net.team as usize & 1
        } else {
            net.slot as usize
        };
        let scores = &snap.score[..snap.n_score as usize];
        let mine = scores.get(me).copied().unwrap_or(0);
        let best = scores
            .iter()
            .enumerate()
            .filter(|&(i, _)| i != me)
            .map(|(_, &v)| v)
            .max();
        if let Some(best) = best {
            let now = mine.cmp(&best);
            if let Some(line) = lead_line(ear.standing, now) {
                ear.say(line);
            }
            ear.standing = Some(now);
        }
        for (at, line) in CALLS {
            let t = at * TICK_HZ;
            if snap.clock <= t && ear.last_clock > t {
                ear.say(line);
            }
        }
        ear.last_clock = snap.clock;
    } else {
        ear.standing = None;
        ear.last_clock = 0;
    }
    let mut on = ear.wind;
    ear.hold_mine(SLOT_WIND, &mut on, "wind_0", big.then_some((0.12, 1.0)));
    ear.wind = on;
    let mut on = ear.room;
    ear.hold_mine(SLOT_ROOM, &mut on, "room_hum_0", (!big).then_some((ROOM_GAIN, 1.0)));
    ear.room = on;
}

/// An arena's hum, under everything.
const ROOM_GAIN: f32 = 0.1;
/// The sea, heard from the land: how far out we look for water, ring by ring
/// round the ear, and how loud it is from each; how often we look, and how
/// long the level takes to follow.
const SURF_RINGS: [(f32, f32); 4] = [(12.0, 0.5), (25.0, 0.35), (50.0, 0.2), (90.0, 0.1)];
const SURF_EVERY_S: f32 = 0.5;
const SURF_FOLLOW_S: f32 = 2.0;
/// Quieter than this, it is off.
const SURF_MIN: f32 = 0.01;

/// The sea, louder the nearer the water. A void's sea is cloud far under it.
fn surf(ear: &mut Ear, ears: &Ears, dt: f32) {
    let map = ears.map;
    let sea = map.light.sea;
    if !sea.is_finite() || map.void > f32::NEG_INFINITY {
        return;
    }
    ear.surf_look -= dt;
    if ear.surf_look <= 0.0 {
        ear.surf_look = SURF_EVERY_S;
        let wet = |r: f32| {
            (0..8).any(|k| {
                let a = k as f32 * core::f32::consts::FRAC_PI_4;
                map.ground(ears.at.x + r * a.cos(), ears.at.z + r * a.sin()) < sea
            })
        };
        ear.surf_want = SURF_RINGS.iter().find(|(r, _)| wet(*r)).map_or(0.0, |(_, g)| *g);
    }
    ear.surf_gain += (ear.surf_want - ear.surf_gain) * (dt / SURF_FOLLOW_S).min(1.0);
    let want = (ear.surf_gain > SURF_MIN).then_some((ear.surf_gain, 1.0));
    let mut on = ear.surf;
    ear.hold_mine(SLOT_SURF, &mut on, "surf_0", want);
    ear.surf = on;
}

/// The buttons whose state changed this frame, and where each is.
type ButtonsChanged<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static Interaction, &'static bevy::ui::UiGlobalTransform),
    (Changed<Interaction>, With<Button>),
>;

/// A button this near where the last one stood, px, is that one: a menu rebuilt
/// under a still pointer hands it a new entity, and one relabelled (a sign-in
/// going through) a centre a little off. Two buttons sit no nearer.
const SAME_BUTTON_PX: f32 = 12.0;

/// The menus: a tick as a button is come onto (by the pointer or a pad's
/// focus), a click as one is pressed; a chime for an invite.
fn menus(
    mut ear: ResMut<Ear>,
    buttons: ButtonsChanged,
    party: Option<Res<crate::party::PartyView>>,
    mut over: Local<Option<(Entity, Vec2)>>,
    mut invite: Local<Option<u32>>,
) {
    for (e, i, at) in &buttons {
        let at = at.translation;
        match i {
            Interaction::Pressed => ear.head("ui_click_0", 0.5, 1.0),
            Interaction::Hovered => {
                if !over.is_some_and(|(_, was)| was.distance(at) < SAME_BUTTON_PX) {
                    ear.head("ui_hover_0", 0.4, 1.0);
                }
                *over = Some((e, at));
            }
            // Left: the one we were on, or its stand-in after a rebuild.
            Interaction::None if over.is_some_and(|(was, p)| was == e || p.distance(at) < SAME_BUTTON_PX) => {
                *over = None;
            }
            _ => {}
        }
    }
    let code = party.and_then(|p| p.invite.as_ref().map(|(_, code)| *code));
    if code != *invite {
        if code.is_some() {
            ear.ping("invite_0");
        }
        *invite = code;
    }
}

/// The live half: the clips handed to the mixer, the mixer's output, and
/// the announcer. On the desktop the mixer is a stream Bevy plays and clips
/// are decoded here; in a browser it runs on the audio thread (`web/mix.js`
/// makes it), the browser decodes the clips, and each frame's commands go
/// down as bytes. A film plays none of it (`film.rs`).
#[cfg(any(target_arch = "wasm32", feature = "sound"))]
pub mod on {
    use std::collections::BTreeMap;

    use bevy::prelude::*;
    use mix::{Cmd, Place};

    use super::{Ear, MASTER, VOICE_WAIT};

    #[cfg(not(target_arch = "wasm32"))]
    pub use desktop::Sound;
    #[cfg(target_arch = "wasm32")]
    pub use page::Sound;

    /// Milliseconds a frame spends handing clips over (decoding them, on the
    /// desktop), past the first.
    const HAND_MS: f32 = 3.0;
    const VOICE_GAIN: f32 = 1.1;
    /// A breath between two of the announcer's lines.
    const VOICE_GAP_S: f32 = 0.15;
    /// The theme (`assets/sound/music/intro.ogg`: the track as it came, its
    /// silent lead-in off, Vorbis at `-q:a 4`), against the master volume,
    /// and how long it takes to come and to go.
    const THEME_GAIN: f32 = 0.35;
    const THEME_FADE_S: f32 = 1.5;

    pub fn build(app: &mut App) {
        #[cfg(not(target_arch = "wasm32"))]
        desktop::build(app);
        #[cfg(target_arch = "wasm32")]
        page::build(app);
        app.init_resource::<Clips>()
            .init_resource::<Announcer>()
            .add_systems(Startup, load)
            .add_systems(Update, hand.before(super::decide))
            .add_systems(
                Update,
                announce
                    .after(super::decide)
                    .run_if(not(resource_exists::<crate::film::Film>)),
            );
    }

    /// The cuts by name (a film reads their bytes in a browser), and those
    /// not yet handed to the mixer.
    #[derive(Resource, Default)]
    pub struct Clips(pub BTreeMap<String, Handle<Sound>>, Vec<String>);

    /// Medals waiting for the announcer, oldest first, and when the line
    /// playing ends (seconds of the app's clock).
    #[derive(Resource, Default)]
    struct Announcer(Vec<u8>, f32);

    fn load(mut clips: ResMut<Clips>, assets: Res<AssetServer>) {
        for n in super::names().into_iter().chain(super::voices()) {
            let h = assets.load(format!("sound/{n}.ogg"));
            clips.0.insert(n.clone(), h);
            clips.1.push(n);
        }
    }

    /// Hands clips to the mixer as they load, a few milliseconds a frame.
    fn hand(mut ear: ResMut<Ear>, mut clips: ResMut<Clips>, sources: Res<Assets<Sound>>, assets: Res<AssetServer>) {
        let clips = &mut *clips;
        let t0 = bevy::platform::time::Instant::now();
        let mut i = 0;
        while i < clips.1.len() && t0.elapsed().as_secs_f32() * 1000.0 < HAND_MS {
            let Some(h) = clips.0.get(&clips.1[i]) else {
                clips.1.swap_remove(i);
                continue;
            };
            if let Some(src) = sources.get(h) {
                let name = clips.1.swap_remove(i);
                let secs = super::ogg_secs(&src.bytes).unwrap_or(1.0);
                if let Some(id) = ear.handed(&name, secs) {
                    #[cfg(not(target_arch = "wasm32"))]
                    ear.cmds.push(Cmd::Install {
                        id,
                        clip: desktop::decode(src),
                    });
                    #[cfg(target_arch = "wasm32")]
                    page::install(id, &src.bytes);
                }
            } else if assets.load_state(h).is_failed() {
                clips.1.swap_remove(i);
            } else {
                i += 1;
            }
        }
        ear.loading = clips.1.len() as u32;
    }

    /// The theme's level this frame, 0 to 1: it plays under the title (and
    /// its settings) and fades away when a match starts. No title (a
    /// picture, the pose mode), no theme.
    fn theme_level(level: &mut f32, in_match: Option<Res<crate::front::InMatch>>, dt: f32) -> bool {
        let want = in_match.is_some_and(|m| !m.0);
        let step = dt / THEME_FADE_S;
        *level = if want {
            (*level + step).min(1.0)
        } else {
            (*level - step).max(0.0)
        };
        want
    }

    /// Takes our new voiced medals and the feed's lines, and says the next
    /// when the last is done.
    fn announce(
        mut st: ResMut<crate::hud::HudState>,
        mut waiting: ResMut<Announcer>,
        mut ear: ResMut<Ear>,
        time: Res<Time>,
        cover: Res<crate::loading::Cover>,
    ) {
        use proto::medal as md;
        let Announcer(voice, until) = &mut *waiting;
        for id in core::mem::take(&mut st.announce) {
            let family = md::info(id).family;
            let ladder = matches!(family, md::Family::Multi | md::Family::Spree);
            // A higher rung says what the lower one waiting would have.
            match voice.iter_mut().find(|q| ladder && md::info(**q).family == family) {
                Some(q) => *q = id,
                None => voice.push(id),
            }
            if voice.len() > VOICE_WAIT {
                voice.remove(0);
            }
        }
        for line in core::mem::take(&mut st.say) {
            ear.say(line);
        }
        // Nothing said under the loading overlay: it waits for the match.
        let now = time.elapsed_secs();
        if now < *until || cover.up {
            return;
        }
        // A line about the match goes before a medal.
        let (name, of_match) = if !ear.lines.is_empty() {
            (ear.lines.remove(0).to_string(), true)
        } else if !voice.is_empty() {
            (md::slug(voice.remove(0)), false)
        } else {
            return;
        };
        let Some(clip) = ear.clip(&format!("voice/{name}")) else {
            return;
        };
        if of_match {
            ear.said = ear.said.wrapping_add(1);
        }
        *until = now + ear.secs[clip as usize] + VOICE_GAP_S;
        // The level it had through Bevy, in both ears: the mixer's master
        // carries the player's volume and LIVE already.
        let gain = VOICE_GAIN * MASTER / super::LIVE * core::f32::consts::SQRT_2;
        ear.cmds.push(Cmd::Play {
            clip,
            at: Place::centre(gain),
            delay: 0,
        });
    }

    /// The desktop: the mixer as a Bevy audio source, its clips decoded here.
    #[cfg(not(target_arch = "wasm32"))]
    mod desktop {
        use std::sync::Arc;

        use bevy::audio::{AddAudioSource, Decodable, Volume};
        use bevy::prelude::*;
        use mix::{Clip, Shared, Stream};

        use super::super::Ear;
        use super::{theme_level, THEME_GAIN};

        pub type Sound = bevy::audio::AudioSource;

        pub fn build(app: &mut App) {
            app.add_audio_source::<Mixer>()
                .add_systems(Startup, start)
                .add_systems(Update, theme.run_if(not(resource_exists::<crate::film::Film>)));
        }

        /// The theme playing.
        #[derive(Component)]
        struct Theme;

        /// The theme, looping, as Bevy streams it.
        #[allow(clippy::too_many_arguments)]
        fn theme(
            in_match: Option<Res<crate::front::InMatch>>,
            settings: Res<crate::settings::Settings>,
            time: Res<Time>,
            assets: Res<AssetServer>,
            mut track: Local<Option<Handle<Sound>>>,
            mut level: Local<f32>,
            mut commands: Commands,
            mut playing: Query<(Entity, Option<&mut AudioSink>), With<Theme>>,
        ) {
            let track = track.get_or_insert_with(|| assets.load("sound/music/intro.ogg"));
            let want = theme_level(&mut level, in_match, time.delta_secs());
            let volume = Volume::Linear(*level * THEME_GAIN * settings.master);
            match playing.single_mut() {
                Ok((e, _)) if *level <= 0.0 => commands.entity(e).despawn(),
                Ok((_, Some(mut sink))) => sink.set_volume(volume),
                Ok(_) => {}
                Err(_) if want => {
                    let s = PlaybackSettings::LOOP.with_volume(volume);
                    commands.spawn((Theme, AudioPlayer::new(track.clone()), s));
                }
                Err(_) => {}
            }
        }

        /// The mixer, as an asset Bevy can play: its stream is the engine.
        #[derive(Asset, TypePath)]
        pub struct Mixer(Arc<Shared>);

        impl Decodable for Mixer {
            type DecoderItem = f32;
            type Decoder = Player;

            fn decoder(&self) -> Player {
                Player(Stream::new(self.0.clone()))
            }
        }

        /// [`Stream`] as rodio's source.
        pub struct Player(Stream);

        impl Iterator for Player {
            type Item = f32;
            fn next(&mut self) -> Option<f32> {
                self.0.next()
            }
        }

        impl bevy::audio::Source for Player {
            fn current_frame_len(&self) -> Option<usize> {
                None
            }
            fn channels(&self) -> u16 {
                2
            }
            fn sample_rate(&self) -> u32 {
                mix::RATE
            }
            fn total_duration(&self) -> Option<std::time::Duration> {
                None
            }
        }

        /// The mixer's one stream, playing from the start.
        fn start(mut commands: Commands, mut ear: ResMut<Ear>, mut mixers: ResMut<Assets<Mixer>>) {
            let shared = Arc::new(Shared::default());
            let h = mixers.add(Mixer(shared.clone()));
            commands.spawn(AudioPlayer::<Mixer>(h));
            ear.mixer = Some(shared);
        }

        /// An Ogg's samples, mono.
        pub fn decode(src: &Sound) -> Clip {
            use bevy::audio::{Sample, Source};
            let d = src.decoder();
            let ch = d.channels().max(1) as usize;
            let rate = d.sample_rate();
            let mut pcm = Vec::new();
            let (mut acc, mut k) = (0.0f32, 0);
            for s in d {
                acc += s.to_f32();
                k += 1;
                if k == ch {
                    pcm.push((acc / ch as f32 * 32767.0).clamp(-32768.0, 32767.0) as i16);
                    acc = 0.0;
                    k = 0;
                }
            }
            Clip { pcm: pcm.into(), rate }
        }
    }

    /// The browser: the mixer on its audio thread (`web/mix.js`), handed
    /// each clip's Ogg and each frame's commands as bytes.
    #[cfg(target_arch = "wasm32")]
    mod page {
        use bevy::asset::io::Reader;
        use bevy::asset::{AssetLoader, LoadContext};
        use bevy::prelude::*;
        use wasm_bindgen::{JsCast, JsValue};

        use super::super::Ear;
        use super::{theme_level, THEME_GAIN};

        /// A sound file's bytes, as Bevy loads it (the browser decodes it).
        #[derive(Asset, TypePath)]
        pub struct Sound {
            pub bytes: Vec<u8>,
        }

        #[derive(Default, TypePath)]
        struct OggLoader;

        impl AssetLoader for OggLoader {
            type Asset = Sound;
            type Settings = ();
            type Error = std::io::Error;

            async fn load(
                &self,
                reader: &mut dyn Reader,
                _settings: &(),
                _load: &mut LoadContext<'_>,
            ) -> Result<Sound, Self::Error> {
                let mut bytes = Vec::new();
                reader.read_to_end(&mut bytes).await?;
                Ok(Sound { bytes })
            }

            fn extensions(&self) -> &[&str] {
                &["ogg"]
            }
        }

        pub fn build(app: &mut App) {
            app.init_asset::<Sound>()
                .init_asset_loader::<OggLoader>()
                .add_systems(Update, post.after(super::super::decide))
                .add_systems(Update, theme.run_if(not(resource_exists::<crate::film::Film>)));
            app.world_mut().resource_mut::<Ear>().taken_elsewhere = true;
        }

        /// The theme's level to the page, which streams it (`web/mix.js`).
        fn theme(
            in_match: Option<Res<crate::front::InMatch>>,
            settings: Res<crate::settings::Settings>,
            time: Res<Time>,
            mut level: Local<f32>,
            mut asked: Local<f32>,
        ) {
            theme_level(&mut level, in_match, time.delta_secs());
            let volume = *level * THEME_GAIN * settings.master;
            if volume != *asked {
                *asked = volume;
                call("theme", &[volume.into()]);
            }
        }

        /// `window.__oneshotMix.<name>(args…)`, if the page made it. The
        /// object and each function are found once (`send` goes every
        /// frame), and looked for again until the page has made them.
        fn call(name: &'static str, args: &[JsValue]) {
            type Found = (JsValue, Vec<(&'static str, js_sys::Function)>);
            thread_local! {
                static MIX: std::cell::RefCell<Option<Found>> = const { std::cell::RefCell::new(None) };
            }
            MIX.with(|m| {
                let mut m = m.borrow_mut();
                if m.is_none() {
                    let Some(w) = web_sys::window() else { return };
                    let Ok(mix) = js_sys::Reflect::get(&w, &JsValue::from_str("__oneshotMix")) else {
                        return;
                    };
                    if !mix.is_object() {
                        return;
                    }
                    *m = Some((mix, Vec::new()));
                }
                let Some((mix, fns)) = m.as_mut() else { return };
                let i = match fns.iter().position(|(n, _)| *n == name) {
                    Some(i) => i,
                    None => {
                        let Some(f) = js_sys::Reflect::get(mix, &JsValue::from_str(name))
                            .ok()
                            .and_then(|f| f.dyn_into::<js_sys::Function>().ok())
                        else {
                            return;
                        };
                        fns.push((name, f));
                        fns.len() - 1
                    }
                };
                let f = &fns[i].1;
                let _ = match args {
                    [] => f.call0(mix),
                    [a] => f.call1(mix, a),
                    [a, b] => f.call2(mix, a, b),
                    _ => {
                        let a = js_sys::Array::new();
                        for x in args {
                            a.push(x);
                        }
                        f.apply(mix, &a)
                    }
                };
            });
        }

        pub fn install(id: u16, ogg: &[u8]) {
            call("install", &[id.into(), js_sys::Uint8Array::from(ogg).into()]);
        }

        /// This frame's commands down to the audio thread, as one message.
        fn post(mut ear: ResMut<Ear>, mut bytes: Local<Vec<u8>>) {
            if ear.cmds.is_empty() {
                return;
            }
            bytes.clear();
            mix::encode(&ear.cmds, &mut bytes);
            ear.sent = ear.sent.wrapping_add(ear.cmds.len() as u32);
            ear.cmds.clear();
            if !bytes.is_empty() {
                call("send", &[js_sys::Uint8Array::from(&bytes[..]).into()]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use core::cmp::Ordering;

    use super::{flag_line, lead_line, mode_line};
    use sim::world::Rules;

    #[test]
    fn a_match_is_called_what_it_is() {
        let ctf = Rules::ctf();
        assert!(
            ctf.teams,
            "Capture the Flag has sides, and must not be called Team Slayer"
        );
        assert_eq!(mode_line(&ctf), "capture_the_flag");
        assert_eq!(flag_line(3), "flag_captured");
    }

    #[test]
    fn the_lead_is_taken_tied_for_and_lost() {
        // A duel's scores, ours then theirs, as they go.
        let scores = [(0, 0), (1, 0), (2, 0), (2, 2), (2, 3), (2, 4), (3, 4), (4, 4), (5, 4)];
        let mut was: Option<Ordering> = None;
        let mut said = Vec::new();
        for (us, them) in scores {
            let now = us.cmp(&them);
            said.push(lead_line(was, now));
            was = Some(now);
        }
        let (taken, tied, lost) = (Some("taken_lead"), Some("tied_for_lead"), Some("lost_lead"));
        assert_eq!(said, [None, taken, None, tied, lost, None, None, tied, taken]);
    }

    /// The announcer waits a line out by this: it must read the file's own
    /// length (ffprobe: 1.288 s and 0.100 s).
    #[test]
    fn an_oggs_length_is_its_last_granule_over_its_rate() {
        let read = |n: &str| {
            let b = std::fs::read(format!("assets/sound/{n}.ogg")).expect(n);
            super::ogg_secs(&b).expect(n)
        };
        assert!((read("voice/double_kill") - 1.2878).abs() < 0.002);
        assert!((read("tick_0") - 0.1).abs() < 0.002);
        assert_eq!(super::ogg_secs(b"not an ogg"), None);
    }
}

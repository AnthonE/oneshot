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
//!   the swap, a dry trigger, a charge's hum; steps by what we stand on
//!   (grass, rock, metal, concrete), the jump, the landing; the spawn.
//! - **Everyone else's**, as far as we were sent them: shots, steps,
//!   landings, reloads, a shield breaking or coming back, rounds passing
//!   our head, ricochets, pickups, getting in and out of a vehicle, hits on
//!   a hull, grenades bouncing.
//! - **The match:** the countdown's ticks and its start; wind on a big map.
//!
//! Recordings are Gates' CC0 cuts (`tools/assets.mjs`, NOTICE); the rest
//! is ours (`tools/sfx.mjs`). The announcer is apart: our voiced medals and
//! its lines about the match (the mode, the lead, the clock's last calls, a
//! betrayal, the end), one at a time (`assets/sound/voice`,
//! `tools/voice.sh`), a newer rung of a ladder taking the place of one still
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
            .add_systems(Update, decide.after(crate::happened));
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
const ENGINE: Carry = Carry {
    near: 10.0,
    reach: 90.0,
};
const FLIGHT: Carry = Carry {
    near: 10.0,
    reach: 120.0,
};
const SMALL: Carry = Carry { near: 5.0, reach: 35.0 };
const PASS: Carry = Carry { near: 1.5, reach: 6.0 };

/// The held slots.
const SLOT_WIND: u8 = 0;
const SLOT_CHARGE: u8 = 1;
const SLOT_OWN_ENGINE: u8 = 2;
const SLOT_ENGINE: u8 = 3;
const SLOT_PROJ: u8 = SLOT_ENGINE + MAX_VEHICLES as u8;
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
    in_match: bool,
    // Our own body.
    last_reload: bool,
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
    // Everyone else, the hulls, the rounds in flight.
    bodies: [Body; MAX_FIGHTERS],
    hulls: [Hull; MAX_VEHICLES],
    own_engine: bool,
    rounds: Vec<Round>,
    /// The next snapshot's rounds, built here and swapped in.
    rounds_next: Vec<Round>,
    snap_tick: u32,
    held_proj: [bool; PROJ_LOOPS],
    // The match.
    last_phase: u8,
    last_count: u32,
    last_clock: u32,
    /// How our score stood against the best of the rest.
    standing: Option<Ordering>,
    wind: bool,
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

    /// A sound of our own, in our head.
    fn mine(&mut self, name: &str, gain: f32) {
        let speed = self.pitch();
        self.cue(name, None, gain, speed);
        let Some(clip) = self.clip(name) else { return };
        let mut at = Place::centre(gain);
        at.rate = speed;
        self.cmds.push(Cmd::Play { clip, at, delay: 0 });
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
                at
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
        "equip_0",
        "equip_1",
        "equip_2",
        "blast_0",
        "blast_1",
        "flesh_0",
        "flesh_1",
        "land_0",
        "swing_0",
        "fuse_0",
        "hurt_0",
        "death_0",
        "shield_beep_0",
        "shield_charge_0",
        "shield_pop_0",
        "shot_ion_charged_0",
        "charge_hold_0",
        "sword_on_0",
        "vent_0",
        "engine_0",
        "rocket_fly_0",
        "sticky_sizzle_0",
        "sticky_stick_0",
        "sticky_whine_0",
        "wind_0",
        "spawn_0",
        "tick_0",
        "go_0",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    for (base, n) in [
        ("step_concrete", 8),
        ("step_metal", 8),
        ("step_grass", 8),
        ("step_rock", 8),
        ("flyby", 4),
        ("ricochet", 4),
        ("impact_metal", 5),
        ("shield_hit", 3),
        ("tink", 3),
        ("sword_swing", 2),
    ] {
        for k in 0..n {
            names.push(format!("{base}_{k}"));
        }
    }
    for k in 0..3 {
        names.push(format!("impactMetal_light_00{k}"));
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

/// What a body stands on, for its steps: the box under it by its look
/// (`level.rs`), a terrain's grass, or the arena floor's plates.
fn ground(map: &sim::map::Map, p: V3) -> &'static str {
    const PAD: f32 = 0.05;
    for (b, look) in map.boxes.iter().zip(map.looks.iter()) {
        if p.x >= b.min.x - PAD
            && p.x <= b.max.x + PAD
            && p.z >= b.min.z - PAD
            && p.z <= b.max.z + PAD
            && (p.y - b.max.y).abs() < 0.2
        {
            return match look {
                0 => "step_concrete",
                6 => "step_rock",
                _ => "step_metal",
            };
        }
    }
    if map.terrain.is_some() {
        if (p.y - map.ground(p.x, p.z)).abs() < 0.3 {
            return if map.ground_normal(p.x, p.z).y < 0.85 {
                "step_rock"
            } else {
                "step_grass"
            };
        }
        return "step_concrete";
    }
    if p.y.abs() < 0.2 {
        "step_metal"
    } else {
        "step_concrete"
    }
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
) {
    let ear = &mut *ear;
    ear.cues.clear();
    cues.0.clear();
    ear.filming = film.is_some();
    let volume = if film.is_some() { 0.0 } else { settings.volume() * LIVE };
    if volume != ear.volume {
        ear.volume = volume;
        ear.cmds.push(Cmd::Master(volume));
    }
    let s = link.0.session();
    if !s.ready {
        if ear.in_match {
            for slot in 0..mix::LOOPS as u8 {
                ear.cmds.push(Cmd::Stop { slot });
            }
            ear.in_match = false;
            ear.own_engine = false;
            ear.charging = false;
            ear.wind = false;
            ear.held_proj = [false; PROJ_LOOPS];
            ear.hulls = [Hull::default(); MAX_VEHICLES];
            ear.rounds.clear();
            ear.bodies = [Body::default(); MAX_FIGHTERS];
        }
        flush(ear);
        return;
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
    own_body(ear, &ears, &f, feet, dt);
    everyone(ear, &ears, &seen.others, dt);

    // Engines: ours (predicted), and every other hull with a driver.
    let mut on = ear.own_engine;
    let own_src = own_vehicle
        .filter(|(veh, _, _)| veh.seats[sim::vehicle::DRIVER] != 0)
        .map(|(veh, p, _)| {
            let speed = (veh.vel.x * veh.vel.x + veh.vel.z * veh.vel.z).sqrt();
            (
                v(p) + Vec3::Y * 0.6,
                Vec3::ZERO,
                0.5 + speed / 40.0,
                0.75 + speed / 22.0,
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
        let speed = Vec3::new(vel.x, 0.0, vel.z).length();
        let src = driven.then_some((p + Vec3::Y * 0.6, vel, 0.5 + speed / 40.0, 0.75 + speed / 22.0, ENGINE));
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

    rounds(ear, &ears, s.core.latest());
    the_match(ear, &s.core, s.core.map.terrain.is_some());
    cues.0.append(&mut ear.cues);
    flush(ear);
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
const LINES: [&str; 12] = [
    "free_for_all",
    "team_slayer",
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

fn events(ear: &mut Ear, ears: &Ears, list: &[proto::EventState], me: u8) {
    let mut hull_hit = [false; MAX_VEHICLES];
    for e in list {
        let at = Vec3::new(e.pos.x, e.pos.y, e.pos.z);
        match e.kind {
            k if k == Kind::Shot as u8 => {
                let Some(g) = gun_takes(e.c as u8) else { continue };
                let ours = e.a == me && e.part != HEARD;
                if ours {
                    let n = ear.take(g.sound, g.takes);
                    ear.mine(&n, 0.75);
                    continue;
                }
                let n = if at.distance(ears.at) > FAR_M || ears.walled(at) {
                    ClipName::of(g.far, None).with(b"_0")
                } else {
                    ear.take(g.sound, g.takes)
                };
                ear.at(ears, &n, at, 1.0, GUN);
                if e.part == HEARD {
                    continue;
                }
                // A round past our head, and one off a wall near us.
                let end = Vec3::new(e.end.x, e.end.y, e.end.z);
                let hitscan = arms::arm(e.c as u8).speed == 0.0;
                if hitscan {
                    let seg = end - at;
                    let len2 = seg.length_squared();
                    if len2 > 1.0 {
                        let u = ((ears.at - at).dot(seg) / len2).clamp(0.0, 1.0);
                        let near = at + seg * u;
                        let miss = near.distance(ears.at);
                        if u > 0.05 && u < 0.98 && miss < PASS.reach && near.distance(at) > 3.0 {
                            let n = ear.take("flyby", 4);
                            ear.at(ears, &n, near, 0.9, PASS);
                        }
                    }
                    if end.distance(ears.at) < 30.0 && ear.chance(12) {
                        let n = ear.take("ricochet", 4);
                        ear.at(ears, &n, end, 0.5, SMALL);
                    }
                }
            }
            k if k == Kind::Hit as u8 => {
                let shield = e.part == 0 || e.part == 3;
                if e.a == me && e.b != me {
                    let n = match e.part {
                        2 => ear.take("headshot", 2),
                        _ if shield => ear.take("shield_hit", 3),
                        _ => "hit_0".into(),
                    };
                    ear.mine(&n, if shield { 0.35 } else { 0.5 });
                }
                if e.b == me {
                    if shield {
                        let n = ear.take("shield_hit", 3);
                        ear.mine(&n, 0.55);
                    } else {
                        let n = ear.take("flesh", 2);
                        ear.mine(&n, 0.6);
                    }
                } else if e.a != me && shield {
                    let n = ear.take("shield_hit", 3);
                    ear.at(ears, &n, at, 0.5, SHIELD);
                }
            }
            k if k == Kind::Blast as u8 => {
                let n = ear.take("blast", 2);
                ear.at(ears, &n, at, 1.3, BLAST);
            }
            k if k == Kind::Wreck as u8 => {
                let n = ear.take("blast", 2);
                ear.at(ears, &n, at, 1.4, BLAST);
                let n = ear.take("impact_metal", 5);
                ear.at(ears, &n, at, 1.0, SMALL);
            }
            k if k == Kind::Melee as u8 => {
                let sword = e.c == arms::SWORD as u16;
                let n = match (sword, e.part) {
                    (true, 2) => ear.take("impact_metal", 5),
                    (true, _) => ear.take("sword_swing", 2),
                    _ => "swing_0".into(),
                };
                if e.a == me {
                    ear.mine(&n, 0.7);
                } else {
                    ear.at(ears, &n, at, 0.8, BODY);
                }
            }
            k if k == Kind::Throw as u8 => {
                if e.a == me && e.part != HEARD {
                    ear.mine("fuse_0", 0.6);
                } else {
                    ear.at(ears, "fuse_0", at, 0.7, BODY);
                }
            }
            k if k == Kind::Pickup as u8 => {
                let n = ClipName::of("impactMetal_light_00", Some(ear.roll(3)));
                if e.a == me {
                    ear.mine("equip_1", 0.6);
                    ear.mine(&n, 0.35);
                } else {
                    ear.at(ears, "equip_1", at, 0.6, BODY);
                }
            }
            k if k == Kind::Ride as u8 => {
                let n = ClipName::of("impactMetal_light_00", Some(ear.roll(3)));
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
}

fn own_body(ear: &mut Ear, ears: &Ears, f: &sim::fighter::Fighter, feet: Vec3, dt: f32) {
    // The shield: the hit is the event's; here, the break, the alarm
    // while it's down, and the whine as it comes back.
    if f.alive {
        if f.shield <= 0 && ear.last_shield > 0 {
            ear.mine("shield_pop_0", 0.7);
            ear.mine("hurt_0", 0.35);
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
        ear.mine("death_0", 0.7);
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
    if g.arm != ear.last_arm {
        if g.arm == arms::SWORD {
            ear.mine("sword_on_0", 0.5);
        } else {
            ear.mine("equip_0", 0.5);
        }
        ear.last_arm = g.arm;
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
            let n = ear.take(ground(ears.map, sim_v(feet)), 8);
            ear.mine(&n, if f.body.crouched { 0.12 } else { 0.3 });
            ear.step = STRIDE_M;
        }
    }
    if f.alive && !grounded && ear.last_grounded && vel.y > 2.0 && f.riding().is_none() {
        ear.mine("equip_2", 0.25);
    }
    if f.alive && grounded && !ear.last_grounded {
        let hard = (-ear.last_vy / 12.0).clamp(0.3, 1.0);
        ear.mine("land_0", 0.45 * hard);
        let n = ear.take(ground(ears.map, sim_v(feet)), 8);
        ear.mine(&n, 0.3 * hard);
        ear.step = STRIDE_M * 0.5;
    }
    ear.last_grounded = grounded;
    ear.last_vy = vel.y;
}

/// Everyone else we were sent: steps, landings, reloads, their shields.
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
                    let n = ear.take(ground(ears.map, o.pos), 8);
                    ear.at(ears, &n, p, if o.crouched { 0.15 } else { 0.6 }, STEP);
                    nb.step = STRIDE_M;
                }
            }
            if o.grounded && !b.grounded {
                let hard = (-b.fall / 12.0).clamp(0.3, 1.0);
                ear.at(ears, "land_0", p, 0.7 * hard, STEP);
                nb.step = STRIDE_M * 0.5;
            }
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
    const CHARGED: u8 = 3;
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
        // Gone: free its slot.
        for (i, r) in ear.rounds.iter().enumerate() {
            if i < taken.len() && !taken[i] {
                if let Some(s) = r.slot {
                    ear.cmds.push(Cmd::Stop { slot: s });
                    ear.held_proj[(s - SLOT_PROJ) as usize] = false;
                }
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

/// The countdown's ticks and its start, and the wind on a big map; and
/// what the announcer says of the match: its mode as the countdown starts,
/// who leads, the clock's last calls, the end.
fn the_match(ear: &mut Ear, net: &NetCore, big: bool) {
    let snap = net.latest();
    if snap.phase == 0 {
        let secs = snap.clock.div_ceil(TICK_HZ);
        // A countdown begun (the title's backdrop has no clock): the mode,
        // if it is one of the two the announcer knows.
        if secs > ear.last_count {
            ear.lines.clear();
            if net.rules.teams {
                ear.say("team_slayer");
            } else if net.rules.seats > 2 {
                ear.say("free_for_all");
            }
        }
        if secs != ear.last_count && (1..=3).contains(&secs) {
            ear.mine("tick_0", 0.5);
        }
        ear.last_count = secs;
    } else if snap.phase == 1 && ear.last_phase == 0 {
        ear.mine("go_0", 0.55);
        ear.last_count = 0;
    } else if snap.phase == 2 && ear.last_phase == 1 {
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
    ear.last_phase = snap.phase;
    if snap.phase == 1 {
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
        let now = time.elapsed_secs();
        if now < *until {
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

    use super::lead_line;

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

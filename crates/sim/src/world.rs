//! A match: the fighters, what they fire, the pads, the vehicles, the score
//! and the clock.
//! [`World::step`] is the server's tick. The browser never runs it for
//! anyone but itself (docs/DESIGN.md §4–5).

use crate::arms::{arm, Head, Kind, NONE};
use crate::consts::*;
use crate::events::{Cause, Event, Events, Kind as Ev};
use crate::fighter::{Fighter, Gun, Local};
use crate::geom::{aim_dir, ray_cylinder, ray_sphere, V3};
use crate::hash::Fnv;
use crate::input::Input;
use crate::map::{Map, ANY_TEAM};
use crate::pool::Pool;
use crate::rewind::{Pose, Rewind};
use crate::rng::{sip24, Pcg32};
use crate::thing::{self, hash_all, Thing};
use crate::vehicle::{VPose, Vehicle, DRIVER};

/// No winner (yet, or a draw).
pub const NO_WINNER: u8 = 255;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rules {
    /// Red against blue; otherwise everyone for themselves.
    pub teams: bool,
    pub score_limit: i32,
    /// 0 for no clock.
    pub time_limit_ms: u32,
    /// The guns everyone spawns with: in hand, then on the back.
    pub loadout: [u8; 2],
    /// Seats the mode fills: 2 for a duel, 4 for doubles, 8 for a free for
    /// all, 16 for Big Team; at most `MAX_FIGHTERS`.
    pub seats: u8,
    /// Gun Game: everyone carries the gun of their rung on
    /// [`crate::arms::LADDER`], a kill moves the killer up a rung, pads are
    /// off, and the score is the rung.
    pub gun_game: bool,
}

impl Rules {
    /// 1v1 Slayer: first to 15, or the most kills at 10 minutes.
    pub fn duel() -> Rules {
        Rules {
            teams: false,
            score_limit: 15,
            time_limit_ms: 600_000,
            loadout: [crate::arms::BR, crate::arms::MAGNUM],
            seats: 2,
            gun_game: false,
        }
    }

    /// 2v2 Team Slayer: first side to 30, or the most at 12 minutes.
    pub fn doubles() -> Rules {
        Rules {
            teams: true,
            score_limit: 30,
            time_limit_ms: 720_000,
            loadout: [crate::arms::BR, crate::arms::MAGNUM],
            seats: 4,
            gun_game: false,
        }
    }

    /// Free for all, eight seats: first to 25, or the most at 10 minutes.
    /// Casual with bots in the empty seats; a cup's heat.
    pub fn ffa() -> Rules {
        Rules {
            teams: false,
            score_limit: 25,
            time_limit_ms: 600_000,
            loadout: [crate::arms::BR, crate::arms::MAGNUM],
            seats: 8,
            gun_game: false,
        }
    }

    /// Big Team, 8v8 Team Slayer: first side to 50, or the most at 15
    /// minutes. Casual, bots in the empty seats.
    pub fn big_team() -> Rules {
        Rules {
            teams: true,
            score_limit: 50,
            time_limit_ms: 900_000,
            loadout: [crate::arms::BR, crate::arms::MAGNUM],
            seats: MAX_FIGHTERS as u8,
            gun_game: false,
        }
    }

    /// Gun Game, eight seats: up the ladder, a kill a rung; the first kill
    /// with the last gun wins.
    pub fn gun_game() -> Rules {
        Rules {
            score_limit: crate::arms::LADDER.len() as i32,
            loadout: [crate::arms::LADDER[0], NONE],
            gun_game: true,
            ..Rules::ffa()
        }
    }

    /// The gun on rung `score` of Gun Game's ladder.
    pub fn rung(score: i32) -> u8 {
        let l = &crate::arms::LADDER;
        l[score.clamp(0, l.len() as i32 - 1) as usize]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Everyone stands at their spawn; nothing moves.
    Countdown,
    Playing,
    Over,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProjKind {
    #[default]
    Rocket,
    Frag,
    /// A SEEKER crystal: it turns toward `target`.
    Seeker,
}

/// A crystal with nobody to seek.
pub const NO_TARGET: u8 = u8::MAX;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Projectile {
    pub active: bool,
    pub kind: ProjKind,
    pub owner: u8,
    pub pos: V3,
    pub vel: V3,
    /// Ticks left: a rocket's life, a frag's fuse.
    pub ttl: u16,
    /// Ticks alive, so a rocket clears its owner.
    pub age: u16,
    /// A crystal's: the fighter it seeks, or [`NO_TARGET`].
    pub target: u8,
}

impl Thing for Projectile {
    fn live(&self) -> bool {
        self.active
    }

    fn hash(&self, h: &mut Fnv) {
        h.bool(self.active);
        h.u8(self.kind as u8);
        h.u8(self.owner);
        h.v3(self.pos);
        h.v3(self.vel);
        h.u16(self.ttl);
        h.u16(self.age);
        // Only a crystal has a target: every match without one hashes as it
        // did before crystals.
        if self.kind == ProjKind::Seeker {
            h.u8(self.target);
        }
    }
}

/// SEEKER crystals stuck in one fighter: how many, ticks until they fall
/// out, and whose.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stuck {
    pub n: u8,
    pub ttl: u16,
    pub owner: u8,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pad {
    /// Ticks until the gun is back; 0 means it's there.
    pub empty_for: u32,
}

impl Thing for Pad {
    /// A pad is part of its map: always there, with a gun or without.
    fn live(&self) -> bool {
        true
    }

    fn hash(&self, h: &mut Fnv) {
        h.u32(self.empty_for);
    }
}

pub struct World {
    pub tick: u32,
    pub map: Map,
    pub rules: Rules,
    pub fighters: [Fighter; MAX_FIGHTERS],
    pub proj: Pool<Projectile, MAX_PROJ>,
    pub pads: [Pad; MAX_PADS],
    /// By slot: slot `i` is always the map's vehicle `i` (`Map::vehicles`).
    pub vehicles: Pool<Vehicle, MAX_VEHICLES>,
    pub phase: Phase,
    /// Ticks left of the countdown, or of the match's clock.
    pub clock: u32,
    /// Each fighter's score (everyone for themselves) or each side's.
    pub score: [i32; MAX_FIGHTERS],
    pub winner: u8,
    pub events: Events,
    /// Crystals stuck in each fighter.
    pub stuck: [Stuck; MAX_FIGHTERS],
    rewind: Box<Rewind<Pose, MAX_FIGHTERS>>,
    vrewind: Box<Rewind<VPose, MAX_VEHICLES>>,
    rng: Pcg32,
    /// The match's secret: keys every spread draw. Never leaves the server
    /// while the match runs (docs/DESIGN.md §9).
    salt: (u64, u64),
}

impl World {
    pub fn new(map: Map, rules: Rules, seed: u64, salt: (u64, u64)) -> World {
        let mut vehicles = Pool::default();
        for (i, v) in map.vehicles.iter().enumerate().take(MAX_VEHICLES) {
            vehicles[i] = Vehicle::new(v.pos, v.yaw, i as u8);
        }
        World {
            tick: 0,
            map,
            rules,
            fighters: [Fighter::default(); MAX_FIGHTERS],
            proj: Pool::default(),
            pads: [Pad::default(); MAX_PADS],
            vehicles,
            phase: Phase::Countdown,
            clock: ticks(COUNTDOWN_MS),
            score: [0; MAX_FIGHTERS],
            winner: NO_WINNER,
            events: Events::default(),
            stuck: [Stuck::default(); MAX_FIGHTERS],
            rewind: Box::default(),
            vrewind: Box::default(),
            rng: Pcg32::new(seed, 1),
            salt,
        }
    }

    /// Seats a fighter and puts it at a spawn point.
    pub fn seat(&mut self, slot: usize, team: u8) {
        let f = &mut self.fighters[slot];
        *f = Fighter {
            seated: true,
            team,
            ..Fighter::default()
        };
        // Everyone for themselves: the seat's score is the newcomer's own.
        if !self.rules.teams {
            self.score[slot] = 0;
        }
        self.respawn(slot);
    }

    pub fn unseat(&mut self, slot: usize) {
        self.leave_vehicle(slot);
        self.fighters[slot] = Fighter::default();
        self.rewind.forget(slot);
    }

    /// One tick. `inputs` is each seat's frame for this tick (repeated by
    /// the server when one never came); `rewind_q8` is how far back each
    /// seat's hitscan is checked, in 1/256 tick, chosen by the server from
    /// what it saw of that client's acks (docs/DESIGN.md §7).
    pub fn step(&mut self, inputs: &[Input; MAX_FIGHTERS], rewind_q8: &[u32; MAX_FIGHTERS]) {
        self.events.clear();
        self.tick = self.tick.wrapping_add(1);
        match self.phase {
            Phase::Countdown => {
                for (f, inp) in self.fighters.iter_mut().zip(inputs) {
                    f.step_in(Phase::Countdown, &inp.sanitized(), &self.map);
                }
                self.clock -= 1;
                if self.clock == 0 {
                    self.phase = Phase::Playing;
                    self.clock = if self.rules.time_limit_ms == 0 {
                        u32::MAX
                    } else {
                        ticks(self.rules.time_limit_ms)
                    };
                }
            }
            Phase::Playing => self.play(inputs, rewind_q8),
            Phase::Over => {}
        }
        let mut poses = [Pose::default(); MAX_FIGHTERS];
        for (p, f) in poses.iter_mut().zip(&self.fighters) {
            *p = pose(f);
        }
        self.rewind.write(self.tick, &poses);
        let mut vposes = [VPose::default(); MAX_VEHICLES];
        for (p, v) in vposes.iter_mut().zip(self.vehicles.iter()) {
            *p = VPose::of(v);
        }
        self.vrewind.write(self.tick, &vposes);
    }

    fn play(&mut self, inputs: &[Input; MAX_FIGHTERS], rewind_q8: &[u32; MAX_FIGHTERS]) {
        // Everyone moves first, then what they fired is resolved against
        // where everyone now is, so seat order never decides a trade.
        let mut locals = [Local::default(); MAX_FIGHTERS];
        for (s, l) in locals.iter_mut().enumerate() {
            *l = self.fighters[s].step_in(Phase::Playing, &inputs[s].sanitized(), &self.map);
        }
        // Vehicles drive under their drivers' inputs (what a driver
        // predicts: Vehicle::step_local), then everyone riding sits.
        for i in 0..MAX_VEHICLES {
            if !self.vehicles[i].active {
                continue;
            }
            match self.vehicles[i].driver() {
                Some(d) if self.fighters[d].active() => {
                    let inp = inputs[d].sanitized();
                    self.vehicles[i].step_local(&inp, &self.map);
                }
                _ => self.vehicles[i].step_parked(&self.map),
            }
        }
        self.seat_riders();
        self.splatter();
        for (s, l) in locals.iter().enumerate() {
            if l.fired {
                self.fire(s, rewind_q8[s]);
            }
            if l.melee {
                self.melee(s, rewind_q8[s]);
            }
            if l.throw {
                self.throw(s);
            }
            // A vehicle in reach (or the one ridden) beats a pad.
            if l.use_pad && self.fighters[s].active() && !self.use_vehicle(s) {
                self.pickup(s);
            }
        }
        self.projectiles();
        for s in 0..MAX_FIGHTERS {
            self.recover(s);
        }
        for i in 0..MAX_VEHICLES {
            self.vehicle_back(i);
        }
        for p in self.pads.iter_mut() {
            p.empty_for = p.empty_for.saturating_sub(1);
        }
        if self.phase == Phase::Playing {
            self.clock -= 1;
            if self.clock == 0 {
                self.end();
            }
        }
    }

    fn fire(&mut self, s: usize, depth: u32) {
        let f = self.fighters[s];
        let id = f.gun().arm;
        let a = arm(id);
        let eye = f.eye();
        let base = aim_dir(f.yaw, f.pitch);
        match a.kind {
            Kind::Hitscan => {
                for k in 0..a.pellets {
                    let dir = spread(base, a.spread, self.salt, s, f.shots, k);
                    self.hitscan(s, eye, dir, id, depth);
                }
            }
            Kind::Seeker => {
                let dir = spread(base, a.spread, self.salt, s, f.shots, 0);
                // Who it seeks: picked down the aim, not the spread.
                let target = self.seek_target(s, eye, base, a.range, depth);
                let _ = self.proj.alloc(Projectile {
                    active: true,
                    kind: ProjKind::Seeker,
                    owner: s as u8,
                    pos: eye + dir * 0.5,
                    vel: dir * a.speed,
                    ttl: ticks(SEEK_LIFE_MS) as u16,
                    age: 0,
                    target,
                });
                self.events.push(Event {
                    kind: Ev::Shot,
                    a: s as u8,
                    c: id as u16,
                    pos: eye,
                    end: eye + dir,
                    ..Event::default()
                });
            }
            Kind::Rocket => {
                let dir = spread(base, a.spread, self.salt, s, f.shots, 0);
                self.launch(ProjKind::Rocket, s, eye + dir * 0.4, dir * a.speed, ticks(5000));
                self.events.push(Event {
                    kind: Ev::Shot,
                    a: s as u8,
                    c: id as u16,
                    pos: eye,
                    end: eye + dir,
                    ..Event::default()
                });
            }
        }
    }

    /// The nearest enemy (or vehicle) a ray from `eye` meets, where the
    /// server says the shooter saw them, and how far.
    fn trace(&self, s: usize, eye: V3, dir: V3, reach: f32, depth: u32, pad: f32) -> Option<(Target, f32)> {
        let me = &self.fighters[s];
        let mut best: Option<(Target, f32)> = None;
        let mut near = reach;
        for (q, v) in self.fighters.iter().enumerate() {
            if q == s || !v.active() || (self.rules.teams && v.team == me.team) {
                continue;
            }
            let p = self.rewind.pose_at(self.tick, q, depth, &pose(v));
            if !p.alive {
                continue;
            }
            let (hy, top) = if p.crouched {
                (HEAD_Y_CROUCH, HIT_TOP_CROUCH)
            } else {
                (HEAD_Y, HIT_TOP)
            };
            if let Some(t) = ray_sphere(eye, dir, p.pos + V3::new(0.0, hy, 0.0), HEAD_R + pad, near) {
                if t < near || best.is_none() {
                    near = t;
                    best = Some((Target::Fighter(q, true), t));
                }
            }
            if let Some(t) = ray_cylinder(eye, dir, p.pos, HIT_R + pad, top, near) {
                if t < near {
                    near = t;
                    best = Some((Target::Fighter(q, false), t));
                }
            }
        }
        // Hulls, rewound like bodies. A rider hit before its hull is hit.
        let ride = me.riding().map(|(i, _)| i);
        for (i, v) in self.vehicles.iter().enumerate() {
            if !v.active || ride == Some(i) || self.spares(s, v) {
                continue;
            }
            let p = self.vrewind.pose_at(self.tick, i, depth, &VPose::of(v));
            if !p.alive {
                continue;
            }
            if let Some(t) = p.as_vehicle().ray(eye, dir, near) {
                if t < near {
                    near = t;
                    best = Some((Target::Vehicle(i), t));
                }
            }
        }
        best
    }

    /// Whether `att` may not hurt `v`: with sides, a teammate of theirs
    /// rides in it.
    fn spares(&self, att: usize, v: &Vehicle) -> bool {
        let team = self.fighters[att].team;
        self.rules.teams
            && v.seats
                .iter()
                .any(|&x| x != 0 && x as usize - 1 != att && self.fighters[x as usize - 1].team == team)
    }

    fn hitscan(&mut self, s: usize, eye: V3, dir: V3, id: u8, depth: u32) {
        let a = arm(id);
        let wall = self.map.ray(eye, dir, a.range);
        let hit = self.trace(s, eye, dir, wall, depth, 0.0);
        let t = hit.map_or(wall, |h| h.1);
        self.events.push(Event {
            kind: Ev::Shot,
            a: s as u8,
            c: id as u16,
            pos: eye,
            end: eye + dir * t,
            ..Event::default()
        });
        if let Some((target, t)) = hit {
            let k = if t <= a.falloff {
                1.0
            } else {
                let u = ((t - a.falloff) / (a.range - a.falloff)).min(1.0);
                1.0 + (a.falloff_min - 1.0) * u
            };
            let pts = (a.dmg * PT) as f32 * k;
            match target {
                Target::Fighter(q, head) => self.damage(s, q, pts as i32, head, a.head, id as u16),
                Target::Vehicle(i) => self.damage_vehicle(s, i, pts as i32, id as u16),
            }
        }
    }

    fn melee(&mut self, s: usize, depth: u32) {
        let f = self.fighters[s];
        let eye = f.eye();
        let dir = aim_dir(f.yaw, f.pitch);
        let wall = self.map.ray(eye, dir, MELEE_RANGE);
        self.events.push(Event {
            kind: Ev::Melee,
            a: s as u8,
            pos: eye,
            ..Event::default()
        });
        let hit = self.trace(s, eye, dir, wall, depth, 0.25);
        if let Some((Target::Vehicle(i), _)) = hit {
            self.damage_vehicle(s, i, MELEE_DMG * PT, Cause::Melee as u16);
        }
        if let Some((Target::Fighter(q, _), _)) = hit {
            let v = &self.fighters[q];
            let (sy, cy) = crate::geom::yaw_sincos(v.yaw);
            let toward = (v.body.pos - f.body.pos).flat().norm();
            if V3::new(cy, 0.0, sy).dot(toward) > BEHIND_COS {
                self.damage(s, q, HEALTH_MAX + SHIELD_MAX, false, Head::None, Cause::Behind as u16);
            } else {
                self.damage(s, q, MELEE_DMG * PT, false, Head::None, Cause::Melee as u16);
            }
        }
    }

    fn throw(&mut self, s: usize) {
        let f = self.fighters[s];
        let dir = aim_dir(f.yaw, f.pitch);
        let vel = dir * GREN_SPEED + V3::new(0.0, 2.0, 0.0);
        self.launch(ProjKind::Frag, s, f.eye() + dir * 0.4, vel, ticks(GREN_FUSE_MS));
        self.events.push(Event {
            kind: Ev::Throw,
            a: s as u8,
            pos: f.eye(),
            ..Event::default()
        });
    }

    fn launch(&mut self, kind: ProjKind, s: usize, pos: V3, vel: V3, ttl: u32) {
        // A full pool drops the throw, as it always has.
        let _ = self.proj.alloc(Projectile {
            active: true,
            kind,
            owner: s as u8,
            pos,
            vel,
            ttl: ttl as u16,
            age: 0,
            target: NO_TARGET,
        });
    }

    /// The enemy a crystal fired from `eye` along `dir` seeks: the nearest
    /// in the lock cone within `range`, in clear sight, where the server says
    /// the shooter saw them (rewound like a hitscan shot).
    fn seek_target(&self, s: usize, eye: V3, dir: V3, range: f32, depth: u32) -> u8 {
        let me = &self.fighters[s];
        let mut best = NO_TARGET;
        let mut near = range;
        for (q, v) in self.fighters.iter().enumerate() {
            if q == s || !v.active() || (self.rules.teams && v.team == me.team) {
                continue;
            }
            let p = self.rewind.pose_at(self.tick, q, depth, &pose(v));
            if !p.alive {
                continue;
            }
            let y = if p.crouched { SEEK_CHEST_Y_CROUCH } else { SEEK_CHEST_Y };
            let to = p.pos + V3::new(0.0, y, 0.0) - eye;
            let d = to.len();
            if d < 0.5 || d >= near || to.dot(dir) < d * SEEK_LOCK_COS {
                continue;
            }
            if !self.map.clear(eye, eye + to) {
                continue;
            }
            best = q as u8;
            near = d;
        }
        best
    }

    /// One crystal's tick: turn toward its target, at most `SEEK_TURN` a
    /// second (a chord, so square roots only), fly, and end on what it
    /// meets. Whether it ended.
    fn seek(&mut self, i: usize, p: &mut Projectile) -> bool {
        let speed = p.vel.len().max(1e-3);
        let mut d = p.vel * (1.0 / speed);
        let t = p.target as usize;
        if t < MAX_FIGHTERS && self.fighters[t].active() {
            let v = &self.fighters[t];
            let y = if v.body.crouched {
                SEEK_CHEST_Y_CROUCH
            } else {
                SEEK_CHEST_Y
            };
            let to = v.body.pos + V3::new(0.0, y, 0.0) - p.pos;
            let dist = to.len();
            if dist > 1e-3 {
                let delta = to * (1.0 / dist) - d;
                let dl = delta.len();
                let max = SEEK_TURN * DT;
                let turn = if dl > max { delta * (max / dl) } else { delta };
                let nd = d + turn;
                let l = nd.len();
                if l > 1e-4 {
                    d = nd * (1.0 / l);
                }
            }
        }
        p.vel = d * speed;
        let step = speed * DT;
        let mut t = self.map.ray(p.pos, d, step);
        let mut met = if t < step { Some(Met::World) } else { None };
        let owner = p.owner as usize;
        let team = self.fighters[owner].team;
        for (q, v) in self.fighters.iter().enumerate() {
            if q == owner || !v.active() || (self.rules.teams && v.team == team) {
                continue;
            }
            if let Some(h) = ray_cylinder(p.pos, d, v.body.pos, HIT_R + 0.05, v.body.height(), t) {
                if h < t || met.is_none() {
                    t = h;
                    met = Some(Met::Fighter(q));
                }
            }
        }
        let own = self.fighters[owner].riding().map(|(k, _)| k);
        for (k, v) in self.vehicles.iter().enumerate() {
            if !v.active || own == Some(k) || self.spares(owner, v) {
                continue;
            }
            if let Some(h) = v.ray(p.pos, d, t) {
                if h < t || met.is_none() {
                    t = h;
                    met = Some(Met::Hull(k));
                }
            }
        }
        if met.is_none() && p.ttl == 0 {
            met = Some(Met::World);
        }
        let Some(m) = met else {
            p.pos = p.pos + p.vel * DT;
            return false;
        };
        self.proj[i].active = false;
        self.stick(owner, m, p.pos + d * t);
        true
    }

    /// A crystal of `s`'s ended at `at` on `m`: in a fighter it hurts and
    /// stays, and enough in one body burst.
    fn stick(&mut self, s: usize, m: Met, at: V3) {
        let id = crate::arms::SEEKER;
        let pts = arm(id).dmg * PT;
        let mut ev = Event {
            kind: Ev::Stick,
            a: s as u8,
            b: s as u8,
            c: crate::events::STUCK_WORLD,
            pos: at,
            ..Event::default()
        };
        match m {
            Met::World => self.events.push(ev),
            Met::Hull(k) => {
                ev.c = crate::events::STUCK_HULL;
                self.events.push(ev);
                self.damage_vehicle(s, k, pts, id as u16);
            }
            Met::Fighter(q) => {
                let st = &mut self.stuck[q];
                st.n = st.n.saturating_add(1);
                st.ttl = ticks(STICK_MS) as u16;
                st.owner = s as u8;
                let n = st.n;
                ev.b = q as u8;
                ev.c = crate::events::STUCK_FIGHTER;
                ev.part = n.min(STICK_BURST);
                self.events.push(ev);
                self.damage(s, q, pts, false, Head::None, id as u16);
                if n >= STICK_BURST {
                    self.stuck[q] = Stuck::default();
                    let v = &self.fighters[q];
                    let chest = v.body.pos + V3::new(0.0, SEEK_CHEST_Y, 0.0);
                    self.blast(chest, s, BURST_DMG, BURST_RADIUS, BURST_FULL, 0.5, Cause::Burst as u16);
                }
            }
        }
    }

    fn projectiles(&mut self) {
        for i in 0..MAX_PROJ {
            let mut p = self.proj[i];
            if !p.active {
                continue;
            }
            p.age = p.age.saturating_add(1);
            p.ttl = p.ttl.saturating_sub(1);
            match p.kind {
                ProjKind::Seeker => {
                    if self.seek(i, &mut p) {
                        continue;
                    }
                }
                ProjKind::Rocket => {
                    let speed = p.vel.len();
                    let d = p.vel * (1.0 / speed);
                    let step = speed * DT;
                    let mut t = self.map.ray(p.pos, d, step);
                    let mut met = t < step;
                    for (q, v) in self.fighters.iter().enumerate() {
                        if !v.active() || (q == p.owner as usize && p.age < 16) {
                            continue;
                        }
                        if let Some(h) = ray_cylinder(p.pos, d, v.body.pos, HIT_R + 0.1, v.body.height(), t) {
                            if h < t || !met {
                                t = h;
                                met = true;
                            }
                        }
                    }
                    let own = self.fighters[p.owner as usize].riding().map(|(i, _)| i);
                    for (k, v) in self.vehicles.iter().enumerate() {
                        if !v.active || (own == Some(k) && p.age < 16) {
                            continue;
                        }
                        if let Some(h) = v.ray(p.pos, d, t) {
                            if h < t || !met {
                                t = h;
                                met = true;
                            }
                        }
                    }
                    if met || p.ttl == 0 {
                        let at = p.pos + d * (t - 0.05).max(0.0);
                        let a = arm(crate::arms::ROCKET);
                        self.proj[i].active = false;
                        self.blast(
                            at,
                            p.owner as usize,
                            a.dmg,
                            a.radius,
                            a.full,
                            0.5,
                            crate::arms::ROCKET as u16,
                        );
                        continue;
                    }
                    p.pos = p.pos + p.vel * DT;
                }
                ProjKind::Frag => {
                    p.vel.y -= GRAVITY * DT;
                    let mut n = p.pos + p.vel * DT;
                    let floor = self.map.floor(n.x, n.z, 0.0, p.pos.y + 0.001);
                    if n.y < floor {
                        n.y = floor;
                        if self.map.terrain.is_some() && floor == self.map.ground(n.x, n.z) {
                            // Off a slope: the part into the ground bounces
                            // back, the part along it slides on.
                            let g = self.map.ground_normal(n.x, n.z);
                            let into = p.vel.dot(g);
                            let along = p.vel - g * into;
                            p.vel = along * GREN_SLIDE - g * (into * GREN_BOUNCE);
                        } else {
                            p.vel = V3::new(p.vel.x * GREN_SLIDE, -p.vel.y * GREN_BOUNCE, p.vel.z * GREN_SLIDE);
                        }
                    }
                    for b in &self.map.boxes {
                        if !b.contains(n) {
                            continue;
                        }
                        if p.pos.x <= b.min.x || p.pos.x >= b.max.x {
                            n.x = if p.pos.x <= b.min.x {
                                b.min.x - 0.01
                            } else {
                                b.max.x + 0.01
                            };
                            p.vel.x = -p.vel.x * GREN_BOUNCE;
                        } else if p.pos.z <= b.min.z || p.pos.z >= b.max.z {
                            n.z = if p.pos.z <= b.min.z {
                                b.min.z - 0.01
                            } else {
                                b.max.z + 0.01
                            };
                            p.vel.z = -p.vel.z * GREN_BOUNCE;
                        } else if p.pos.y <= b.min.y {
                            n.y = b.min.y - 0.01;
                            p.vel.y = -p.vel.y * GREN_BOUNCE;
                        }
                    }
                    p.pos = n;
                    if p.ttl == 0 {
                        self.proj[i].active = false;
                        self.blast(
                            p.pos + V3::new(0.0, 0.05, 0.0),
                            p.owner as usize,
                            GREN_DMG,
                            GREN_RADIUS,
                            GREN_FULL,
                            1.0,
                            Cause::Grenade as u16,
                        );
                        continue;
                    }
                }
            }
            p.pos = V3::new(
                crate::movement::quant(p.pos.x),
                crate::movement::quant(p.pos.y),
                crate::movement::quant(p.pos.z),
            );
            p.vel = V3::new(
                crate::movement::quant(p.vel.x),
                crate::movement::quant(p.vel.y),
                crate::movement::quant(p.vel.z),
            );
            self.proj[i] = p;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn blast(&mut self, at: V3, owner: usize, dmg: i32, radius: f32, full: f32, self_k: f32, cause: u16) {
        self.events.push(Event {
            kind: Ev::Blast,
            a: owner as u8,
            c: cause,
            pos: at,
            ..Event::default()
        });
        let team = self.fighters[owner].team;
        for q in 0..MAX_FIGHTERS {
            let v = self.fighters[q];
            if !v.active() || (self.rules.teams && q != owner && v.team == team) {
                continue;
            }
            let chest = v.body.pos + V3::new(0.0, 0.9, 0.0);
            let d = chest - at;
            let dist = d.len();
            if dist >= radius || !self.map.clear(at, chest) {
                continue;
            }
            let k = if dist <= full {
                1.0
            } else {
                (radius - dist) / (radius - full)
            };
            let mut pts = (dmg * PT) as f32 * k;
            if q == owner {
                pts *= self_k;
            }
            let away = if dist > 0.0 {
                d * (1.0 / dist)
            } else {
                V3::new(0.0, 1.0, 0.0)
            };
            let b = &mut self.fighters[q].body;
            b.vel = b.vel + away * (BLAST_PUSH * k);
            if away.y > 0.0 {
                b.grounded = false;
            }
            self.damage(owner, q, pts as i32, false, Head::None, cause);
        }
        for i in 0..MAX_VEHICLES {
            let v = self.vehicles[i];
            if !v.active || self.spares(owner, &v) {
                continue;
            }
            let mid = v.pos + V3::new(0.0, VEH_H * 0.5, 0.0);
            let d = mid - at;
            let dist = d.len();
            if dist >= radius || !self.map.clear(at, mid) {
                continue;
            }
            let k = if dist <= full {
                1.0
            } else {
                (radius - dist) / (radius - full)
            };
            let away = if dist > 0.0 {
                d * (1.0 / dist)
            } else {
                V3::new(0.0, 1.0, 0.0)
            };
            let veh = &mut self.vehicles[i];
            veh.vel = veh.vel + away * (BLAST_PUSH * VEH_PUSH_K * k);
            if away.y > 0.0 {
                veh.grounded = false;
            }
            self.damage_vehicle(owner, i, ((dmg * PT) as f32 * k) as i32, cause);
        }
    }

    /// `att` hurts vehicle `i` for `pts`; at nothing left it blows up.
    fn damage_vehicle(&mut self, att: usize, i: usize, pts: i32, cause: u16) {
        let v = &mut self.vehicles[i];
        if !v.active || pts <= 0 {
            return;
        }
        v.health -= pts;
        let pos = v.pos;
        let dead = v.health <= 0;
        self.events.push(Event {
            kind: Ev::VehicleHit,
            a: att as u8,
            part: i as u8,
            c: (pts / PT).min(u16::MAX as i32) as u16,
            pos,
            ..Event::default()
        });
        if dead {
            self.wreck(i, att, cause);
        }
    }

    /// Vehicle `i` blows up, `att`'s doing: whoever rides in it dies, a
    /// blast around it, back at its spawn after the map says.
    fn wreck(&mut self, i: usize, att: usize, _cause: u16) {
        let v = self.vehicles[i];
        let back = self.map.vehicles.get(v.home as usize).map_or(30_000, |d| d.respawn_ms);
        self.vehicles[i] = Vehicle {
            active: false,
            respawn: ticks(back).max(1),
            home: v.home,
            pos: v.pos,
            yaw: v.yaw,
            ..Vehicle::default()
        };
        self.events.push(Event {
            kind: Ev::Wreck,
            a: att as u8,
            part: i as u8,
            pos: v.pos,
            ..Event::default()
        });
        for &x in &v.seats {
            if x != 0 {
                let q = x as usize - 1;
                self.fighters[q].ride = 0;
                if self.fighters[q].alive {
                    self.kill(att, q, Cause::Wreck as u16);
                }
            }
        }
        self.blast(
            v.pos + V3::new(0.0, 0.8, 0.0),
            att,
            GREN_DMG,
            GREN_RADIUS,
            GREN_FULL,
            1.0,
            Cause::Wreck as u16,
        );
    }

    /// A destroyed vehicle counts down to coming back at its spawn.
    fn vehicle_back(&mut self, i: usize) {
        let v = &mut self.vehicles[i];
        if v.active || v.respawn == 0 {
            return;
        }
        v.respawn -= 1;
        if v.respawn == 0 {
            if let Some(d) = self.map.vehicles.get(v.home as usize) {
                *v = Vehicle::new(d.pos, d.yaw, v.home);
            }
        }
    }

    /// Everyone riding is put at their seat.
    fn seat_riders(&mut self) {
        for s in 0..MAX_FIGHTERS {
            if let Some((i, seat)) = self.fighters[s].riding() {
                self.fighters[s].body = self.vehicles[i].seat_body(seat);
            }
        }
    }

    /// USE: out of the vehicle `s` rides, or into the nearest in reach
    /// with a seat free (the driver's first). Whether it did either.
    fn use_vehicle(&mut self, s: usize) -> bool {
        if let Some((i, seat)) = self.fighters[s].riding() {
            self.leave_vehicle(s);
            let v = self.vehicles[i];
            let (f, r) = v.axes();
            let at = if seat == DRIVER {
                v.pos - r * (VEH_HALF_W + 0.9)
            } else {
                v.pos - f * (VEH_HALF_L + 0.9)
            };
            let x = at.x.clamp(self.map.lo.0, self.map.hi.0);
            let z = at.z.clamp(self.map.lo.1, self.map.hi.1);
            let y = self.map.floor(x, z, BODY_R, v.pos.y + 1.5);
            let b = &mut self.fighters[s].body;
            b.pos = V3::new(
                crate::movement::quant(x),
                crate::movement::quant(y),
                crate::movement::quant(z),
            );
            b.vel = V3::ZERO;
            b.grounded = true;
            return true;
        }
        let at = self.fighters[s].body.pos;
        let mut best: Option<usize> = None;
        let mut near = ENTER_R;
        for (i, v) in self.vehicles.iter().enumerate() {
            let d = (v.pos - at).flat().len();
            if v.active && v.seats.contains(&0) && d < near && crate::fmath::fabs(v.pos.y - at.y) < 2.0 {
                near = d;
                best = Some(i);
            }
        }
        let Some(i) = best else {
            return false;
        };
        let seat = if self.vehicles[i].seats[DRIVER] == 0 {
            DRIVER
        } else {
            crate::vehicle::PASSENGER
        };
        self.vehicles[i].seats[seat] = s as u8 + 1;
        let f = &mut self.fighters[s];
        f.ride = Fighter::ride_of(i, seat);
        f.reload = 0;
        f.burst_left = 0;
        f.body = self.vehicles[i].seat_body(seat);
        self.events.push(Event {
            kind: Ev::Ride,
            a: s as u8,
            b: seat as u8,
            part: i as u8,
            pos: self.vehicles[i].pos,
            ..Event::default()
        });
        true
    }

    /// `s` is no longer in its vehicle (if any); where it stands is the
    /// caller's.
    fn leave_vehicle(&mut self, s: usize) {
        if let Some((i, seat)) = self.fighters[s].riding() {
            self.vehicles[i].seats[seat] = 0;
            self.fighters[s].ride = 0;
            self.events.push(Event {
                kind: Ev::Ride,
                a: s as u8,
                b: seat as u8,
                c: 1,
                part: i as u8,
                pos: self.vehicles[i].pos,
                ..Event::default()
            });
        }
    }

    /// A vehicle moving fast runs down whoever is on foot in its way: at
    /// [`SPLAT_KILL`] dead, slower knocked aside. Its driver did it (or
    /// nobody drove: then the fighter's own death).
    fn splatter(&mut self) {
        for i in 0..MAX_VEHICLES {
            let v = self.vehicles[i];
            let speed = v.vel.flat().len();
            if !v.active || speed < SPLAT_SPEED {
                continue;
            }
            let driver = v.driver();
            for q in 0..MAX_FIGHTERS {
                let f = self.fighters[q];
                if !f.active() || f.ride != 0 || !v.touches(f.body.pos, BODY_R, f.body.height()) {
                    continue;
                }
                if driver.is_some_and(|d| d != q && self.rules.teams && self.fighters[d].team == f.team) {
                    continue;
                }
                let away = (f.body.pos - v.pos).flat().norm();
                let b = &mut self.fighters[q].body;
                b.vel = b.vel + away * 6.0 + V3::new(0.0, 3.0, 0.0);
                b.grounded = false;
                if speed >= SPLAT_KILL {
                    self.damage(
                        driver.unwrap_or(q),
                        q,
                        HEALTH_MAX + SHIELD_MAX,
                        false,
                        Head::None,
                        Cause::Splatter as u16,
                    );
                }
            }
        }
    }

    fn damage(&mut self, att: usize, vic: usize, pts: i32, head: bool, rule: Head, cause: u16) {
        let v = &mut self.fighters[vic];
        if !v.alive || pts <= 0 {
            return;
        }
        v.since_hit = 0;
        let lethal = head && (rule == Head::Always || (rule == Head::Unshielded && v.shield == 0));
        let part = if lethal {
            v.shield = 0;
            v.health = 0;
            2
        } else {
            let soak = pts.min(v.shield);
            v.shield -= soak;
            v.health -= pts - soak;
            match (head, pts > soak) {
                (true, true) => 2,
                (false, true) => 1,
                // A head hit the shields took whole: told apart from a body
                // hit for the server's aim review (room/src/watch.rs).
                (true, false) => 3,
                (false, false) => 0,
            }
        };
        let dead = v.health <= 0;
        self.events.push(Event {
            kind: Ev::Hit,
            a: att as u8,
            b: vic as u8,
            part,
            c: (pts / PT).min(u16::MAX as i32) as u16,
            pos: v.body.pos,
            ..Event::default()
        });
        if dead {
            self.kill(att, vic, cause);
        }
    }

    fn kill(&mut self, att: usize, vic: usize, cause: u16) {
        self.leave_vehicle(vic);
        let v = &mut self.fighters[vic];
        v.alive = false;
        v.health = 0;
        v.shield = 0;
        v.body.vel = V3::ZERO;
        v.respawn = ticks(RESPAWN_MS) as u16;
        v.deaths += 1;
        let vteam = v.team;
        let pos = v.body.pos;
        let k = &mut self.fighters[att];
        let own = att == vic || (self.rules.teams && k.team == vteam);
        let side = if self.rules.teams { k.team as usize & 1 } else { att };
        let gun_game = self.rules.gun_game;
        if own {
            // A rung is never lost below the first.
            self.score[side] -= 1;
            if gun_game {
                self.score[side] = self.score[side].max(0);
            }
        } else {
            k.kills += 1;
            k.grenades = (k.grenades + 1).min(GREN_MAX);
            self.score[side] += 1;
            if gun_game && k.alive && self.score[side] < self.rules.score_limit {
                // Up a rung, at once: the new gun full and in hand.
                k.guns = [Gun::full(Rules::rung(self.score[side])), Gun::default()];
                k.cur = 0;
                k.reload = 0;
                k.burst_left = 0;
                k.refire = k.refire.max(ticks(crate::arms::SWAP_MS) as u16);
            }
        }
        self.events.push(Event {
            kind: Ev::Kill,
            a: att as u8,
            b: vic as u8,
            c: cause,
            pos,
            ..Event::default()
        });
        if self.score[side] >= self.rules.score_limit {
            self.end();
        }
    }

    fn end(&mut self) {
        self.phase = Phase::Over;
        let mut best = i32::MIN;
        let mut who = NO_WINNER;
        let n = if self.rules.teams { 2 } else { MAX_FIGHTERS };
        for (i, &sc) in self.score.iter().enumerate().take(n) {
            if !self.rules.teams && !self.fighters[i].seated {
                continue;
            }
            if sc > best {
                best = sc;
                who = i as u8;
            } else if sc == best {
                who = NO_WINNER;
            }
        }
        self.winner = who;
    }

    fn pickup(&mut self, s: usize) {
        if self.rules.gun_game {
            return;
        }
        let at = self.fighters[s].body.pos;
        for (i, def) in self.map.pads.iter().enumerate().take(MAX_PADS) {
            let d = def.pos - at;
            let near = crate::fmath::fabs(d.x) < PAD_REACH
                && crate::fmath::fabs(d.z) < PAD_REACH
                && crate::fmath::fabs(d.y) < 1.5;
            if !near || self.pads[i].empty_for > 0 {
                continue;
            }
            let f = &mut self.fighters[s];
            let full = Gun::full(def.arm);
            if let Some(g) = f.guns.iter_mut().find(|g| g.arm == def.arm) {
                *g = full;
            } else if f.guns[1].arm == NONE {
                f.guns[1] = full;
                f.cur = 1;
            } else {
                f.guns[f.cur as usize & 1] = full;
            }
            f.reload = 0;
            f.burst_left = 0;
            f.refire = f.refire.max(ticks(crate::arms::SWAP_MS) as u16);
            self.pads[i].empty_for = ticks(def.respawn_ms);
            self.events.push(Event {
                kind: Ev::Pickup,
                a: s as u8,
                c: def.arm as u16,
                pos: def.pos,
                ..Event::default()
            });
            return;
        }
    }

    /// Shields and health come back; the dead come back.
    fn recover(&mut self, s: usize) {
        let st = &mut self.stuck[s];
        if st.n > 0 {
            st.ttl = st.ttl.saturating_sub(1);
            if st.ttl == 0 {
                *st = Stuck::default();
            }
        }
        let f = &mut self.fighters[s];
        if !f.seated {
            return;
        }
        if !f.alive {
            f.respawn = f.respawn.saturating_sub(1);
            if f.respawn == 0 {
                self.respawn(s);
            }
            return;
        }
        f.since_hit = f.since_hit.saturating_add(1);
        if f.since_hit >= ticks(SHIELD_DELAY_MS) {
            if f.shield < SHIELD_MAX {
                f.shield = (f.shield + SHIELD_RATE * PT / TICK_HZ as i32).min(SHIELD_MAX);
            } else if f.health < HEALTH_MAX {
                f.health = (f.health + HEALTH_RATE * PT / TICK_HZ as i32).min(HEALTH_MAX);
            }
        }
    }

    /// Back at the spawn point farthest from the nearest living enemy, of a
    /// few drawn.
    fn respawn(&mut self, s: usize) {
        self.stuck[s] = Stuck::default();
        let team = self.fighters[s].team;
        let teams = self.rules.teams;
        let n = self.map.spawns.len() as u32;
        let mut pick = 0usize;
        let mut far = -1.0f32;
        for _ in 0..SPAWN_DRAWS {
            let mut i = self.rng.next_bounded(n) as usize;
            // Walk to the next spawn this side may use.
            for _ in 0..n {
                let sp = &self.map.spawns[i];
                if !teams || sp.team == team || sp.team == ANY_TEAM {
                    break;
                }
                i = (i + 1) % n as usize;
            }
            let at = self.map.spawns[i].pos;
            let mut near = f32::INFINITY;
            for (q, v) in self.fighters.iter().enumerate() {
                if q != s && v.active() && !(teams && v.team == team) {
                    near = near.min((v.body.pos - at).len());
                }
            }
            if near > far {
                far = near;
                pick = i;
            }
        }
        let sp = self.map.spawns[pick];
        let loadout = if self.rules.gun_game {
            [Rules::rung(self.score[s]), NONE]
        } else {
            self.rules.loadout
        };
        self.fighters[s].spawn(sp.pos, sp.yaw, loadout);
        self.rewind.forget(s);
        self.events.push(Event {
            kind: Ev::Spawn,
            a: s as u8,
            pos: sp.pos,
            ..Event::default()
        });
    }

    /// The state's hash, the same native and wasm.
    pub fn hash(&self) -> u64 {
        let mut h = Fnv::default();
        h.u32(self.tick);
        h.u8(self.phase as u8);
        h.u32(self.clock);
        h.u8(self.winner);
        for &s in &self.score {
            h.i32(s);
        }
        // Every kind, every slot; no wildcard, so a new kind must be added.
        for k in thing::Kind::ALL {
            match k {
                thing::Kind::Fighter => hash_all(&self.fighters, &mut h),
                thing::Kind::Projectile => hash_all(&*self.proj, &mut h),
                thing::Kind::Pad => hash_all(&self.pads, &mut h),
                thing::Kind::Vehicle => hash_all(&*self.vehicles, &mut h),
            }
        }
        // Stuck crystals, only where there are some: a match without them
        // hashes as it did before them.
        for (q, st) in self.stuck.iter().enumerate() {
            if st.n > 0 {
                h.u8(q as u8);
                h.u8(st.n);
                h.u16(st.ttl);
                h.u8(st.owner);
            }
        }
        let (a, b) = self.rng.state();
        h.u64(a);
        h.u64(b);
        for e in self.events.as_slice() {
            h.u8(e.kind as u8);
            h.u8(e.a);
            h.u8(e.b);
            h.u8(e.part);
            h.u16(e.c);
            h.v3(e.pos);
            h.v3(e.end);
        }
        h.finish()
    }
}

/// What a ray (a shot, a swing) meets: a fighter (and whether in the head),
/// or a vehicle's hull.
/// What a crystal met.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Met {
    World,
    Fighter(usize),
    Hull(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Target {
    Fighter(usize, bool),
    Vehicle(usize),
}

fn pose(f: &Fighter) -> Pose {
    Pose {
        pos: f.body.pos,
        crouched: f.body.crouched,
        alive: f.active(),
    }
}

/// A round's direction: `base` turned off-axis by up to `cone` radians,
/// uniform over the cone's disc, from a draw keyed by the match's salt. A
/// client that knows its own shot counter still can't name the draw.
pub fn spread(base: V3, cone: f32, salt: (u64, u64), slot: usize, shot: u32, pellet: u8) -> V3 {
    if cone == 0.0 {
        return base;
    }
    let h = sip24(salt.0, salt.1, ((slot as u64) << 32) | shot as u64, pellet as u64);
    let u = (h & 0xff_ffff) as f32 * (1.0 / 16_777_216.0);
    let v = ((h >> 24) & 0xff_ffff) as f32 * (1.0 / 16_777_216.0);
    let r = cone * u.sqrt();
    let th = core::f32::consts::TAU * v;
    let up = if crate::fmath::fabs(base.y) > 0.99 {
        V3::new(1.0, 0.0, 0.0)
    } else {
        V3::new(0.0, 1.0, 0.0)
    };
    let right = base.cross(up).norm();
    let up2 = right.cross(base);
    let (sr, cr) = crate::fmath::sin_cos(r);
    let (st, ct) = crate::fmath::sin_cos(th);
    (base * cr + (right * ct + up2 * st) * sr).norm()
}

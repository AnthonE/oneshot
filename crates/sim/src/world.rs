//! A match: the fighters, what they fire, the pads, the score and the clock.
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
}

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
    }
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
    pub phase: Phase,
    /// Ticks left of the countdown, or of the match's clock.
    pub clock: u32,
    /// Each fighter's score (everyone for themselves) or each side's.
    pub score: [i32; MAX_FIGHTERS],
    pub winner: u8,
    pub events: Events,
    rewind: Box<Rewind<Pose, MAX_FIGHTERS>>,
    rng: Pcg32,
    /// The match's secret: keys every spread draw. Never leaves the server
    /// while the match runs (docs/DESIGN.md §9).
    salt: (u64, u64),
}

impl World {
    pub fn new(map: Map, rules: Rules, seed: u64, salt: (u64, u64)) -> World {
        World {
            tick: 0,
            map,
            rules,
            fighters: [Fighter::default(); MAX_FIGHTERS],
            proj: Pool::default(),
            pads: [Pad::default(); MAX_PADS],
            phase: Phase::Countdown,
            clock: ticks(COUNTDOWN_MS),
            score: [0; MAX_FIGHTERS],
            winner: NO_WINNER,
            events: Events::default(),
            rewind: Box::default(),
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
    }

    fn play(&mut self, inputs: &[Input; MAX_FIGHTERS], rewind_q8: &[u32; MAX_FIGHTERS]) {
        // Everyone moves first, then what they fired is resolved against
        // where everyone now is, so seat order never decides a trade.
        let mut locals = [Local::default(); MAX_FIGHTERS];
        for (s, l) in locals.iter_mut().enumerate() {
            *l = self.fighters[s].step_in(Phase::Playing, &inputs[s].sanitized(), &self.map);
        }
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
            if l.use_pad && self.fighters[s].active() {
                self.pickup(s);
            }
        }
        self.projectiles();
        for s in 0..MAX_FIGHTERS {
            self.recover(s);
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

    /// The nearest enemy a ray from `eye` meets, where the server says the
    /// shooter saw them: `(slot, t, head)`.
    fn trace(&self, s: usize, eye: V3, dir: V3, reach: f32, depth: u32, pad: f32) -> Option<(usize, f32, bool)> {
        let me = &self.fighters[s];
        let mut best: Option<(usize, f32, bool)> = None;
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
                    best = Some((q, t, true));
                }
            }
            if let Some(t) = ray_cylinder(eye, dir, p.pos, HIT_R + pad, top, near) {
                if t < near {
                    near = t;
                    best = Some((q, t, false));
                }
            }
        }
        best
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
        if let Some((q, t, head)) = hit {
            let k = if t <= a.falloff {
                1.0
            } else {
                let u = ((t - a.falloff) / (a.range - a.falloff)).min(1.0);
                1.0 + (a.falloff_min - 1.0) * u
            };
            let pts = (a.dmg * PT) as f32 * k;
            self.damage(s, q, pts as i32, head, a.head, id as u16);
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
        if let Some((q, _, _)) = self.trace(s, eye, dir, wall, depth, 0.25) {
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
        });
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
                        p.vel = V3::new(p.vel.x * GREN_SLIDE, -p.vel.y * GREN_BOUNCE, p.vel.z * GREN_SLIDE);
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

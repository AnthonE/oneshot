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

/// What a blow that kills whatever does (from behind, a sticky worn, a
/// shard burst, a splatter): more than the most a body can hold.
const KILL_PTS: i32 = HEALTH_MAX + OVERSHIELD_MAX;

/// Whether a body standing at `at` is on the pad at `pad`.
fn on_pad(pad: V3, at: V3) -> bool {
    let d = pad - at;
    crate::fmath::fabs(d.x) < PAD_REACH && crate::fmath::fabs(d.z) < PAD_REACH && crate::fmath::fabs(d.y) < 1.5
}

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
    /// With sides: rounds, blasts and blows hurt teammates too, and a
    /// teammate killed costs the side a point (ranked, as MLG plays). Nobody
    /// homes on, lunges at or is magnetised toward a teammate either way.
    pub friendly_fire: bool,
    /// Capture the Flag (with sides, on a map with flag bases): the score
    /// is captures, not kills.
    pub ctf: bool,
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
            friendly_fire: false,
            ctf: false,
        }
    }

    /// 2v2 Team Slayer: first side to 30, or the most at 12 minutes.
    /// Ranked, so friendly fire is on.
    pub fn doubles() -> Rules {
        Rules {
            teams: true,
            score_limit: 30,
            time_limit_ms: 720_000,
            loadout: [crate::arms::BR, crate::arms::MAGNUM],
            seats: 4,
            gun_game: false,
            friendly_fire: true,
            ctf: false,
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
            friendly_fire: false,
            ctf: false,
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
            friendly_fire: false,
            ctf: false,
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

    /// Capture the Flag, four a side: first to 3 captures, or the most at
    /// 15 minutes. Casual, bots in the empty seats.
    pub fn ctf() -> Rules {
        Rules {
            score_limit: 3,
            time_limit_ms: 900_000,
            seats: 8,
            ctf: true,
            ..Rules::big_team()
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
    /// An ion bolt; a charged one.
    Bolt,
    Charge,
    /// A shard (`arms::SHARDS`).
    Needle,
    /// A launcher's grenade.
    Lob,
    /// An ion cannon's ball.
    Cannon,
    /// A sticky grenade.
    Sticky,
}

/// A projectile homing on nobody.
pub const NO_TARGET: u8 = 255;

/// A grenade's fuse lit at `age` ticks from the throw: `after_ms` from now,
/// but never sooner than `arm_ms` from the throw (Halo 2's).
fn fuse(age: u16, after_ms: u32, arm_ms: u32) -> u16 {
    ticks(after_ms).max(ticks(arm_ms).saturating_sub(age as u32)) as u16
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
    /// The gun it came from (a frag's is `arms::NONE`).
    pub arm: u8,
    /// The seat it homes on, or [`NO_TARGET`]. A sticky stuck to a
    /// fighter names its seat; to a vehicle, `MAX_FIGHTERS` + its slot (and
    /// then `vel` is where on it, from its feet or its middle).
    pub target: u8,
    /// A lob has touched something: its fuse is lit.
    pub landed: bool,
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
        h.u8(self.arm);
        h.u8(self.target);
        h.bool(self.landed);
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

/// A gun on the floor, with what was left in it, or the grenades someone
/// carried (`gun.arm` [`NONE`]): left by the dead, and by a hand that let
/// go for another gun. USE takes the gun as a pad's; walking over one takes
/// its rounds for a gun of the same kind carried, and its grenades.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Dropped {
    pub active: bool,
    pub gun: Gun,
    pub frags: u8,
    pub stickies: u8,
    /// On the floor, on the grid; or in the air, thrown by a blast or out
    /// of a falling hand.
    pub pos: V3,
    /// Zero lying still; otherwise it flies and bounces as a frag does
    /// ([`World::fly_drops`]) until it settles.
    pub vel: V3,
    /// Ticks till it goes.
    pub ttl: u32,
}

impl Dropped {
    /// Nothing left to take: no rounds in the gun, no grenades.
    fn spent(&self) -> bool {
        (self.gun.arm == NONE || self.gun.spent()) && self.frags == 0 && self.stickies == 0
    }

    /// Within reach of a body standing at `at`.
    pub fn reach(&self, at: V3) -> bool {
        let d = self.pos - at;
        crate::fmath::fabs(d.x) < DROP_REACH && crate::fmath::fabs(d.z) < DROP_REACH && crate::fmath::fabs(d.y) < 1.5
    }
}

impl Thing for Dropped {
    fn live(&self) -> bool {
        self.active
    }

    fn hash(&self, h: &mut Fnv) {
        h.bool(self.active);
        h.u8(self.gun.arm);
        h.u16(self.gun.mag);
        h.u16(self.gun.reserve);
        h.u16(self.gun.heat);
        h.u8(self.frags);
        h.u8(self.stickies);
        h.v3(self.pos);
        h.v3(self.vel);
        h.u32(self.ttl);
    }
}

/// Where a flag is.
pub const FLAG_HOME: u8 = 0;
/// In an enemy's hands (`Flag::carrier`).
pub const FLAG_HELD: u8 = 1;
/// On the floor where its carrier fell or let go.
pub const FLAG_DOWN: u8 = 2;

/// Capture the Flag's flag: index 0 is red's, 1 blue's. Taken by an enemy
/// who touches it, scored when that carrier reaches its own base while its
/// own flag is home, returned by a defender's touch or on its own.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Flag {
    pub state: u8,
    /// The seat carrying it, while held.
    pub carrier: u8,
    /// Where it is: its base, its carrier's feet, or where it fell.
    pub pos: V3,
    /// Ticks till a dropped flag goes home.
    pub down_for: u32,
}

impl Thing for Flag {
    fn live(&self) -> bool {
        true
    }

    fn hash(&self, h: &mut Fnv) {
        h.u8(self.state);
        h.u8(self.carrier);
        h.v3(self.pos);
        h.u32(self.down_for);
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
    pub drops: Pool<Dropped, MAX_DROPS>,
    /// Capture the Flag's, red's then blue's; at home and still otherwise.
    pub flags: [Flag; 2],
    pub phase: Phase,
    /// Ticks left of the countdown, or of the match's clock.
    pub clock: u32,
    /// Each fighter's score (everyone for themselves) or each side's.
    pub score: [i32; MAX_FIGHTERS],
    pub winner: u8,
    pub events: Events,
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
        let mut flags = [Flag::default(); 2];
        if let Some(bases) = map.flags {
            for (f, b) in flags.iter_mut().zip(bases) {
                f.pos = b;
            }
        }
        World {
            flags,
            tick: 0,
            map,
            rules,
            fighters: [Fighter::default(); MAX_FIGHTERS],
            proj: Pool::default(),
            pads: [Pad::default(); MAX_PADS],
            vehicles,
            drops: Pool::default(),
            phase: Phase::Countdown,
            clock: ticks(COUNTDOWN_MS),
            score: [0; MAX_FIGHTERS],
            winner: NO_WINNER,
            events: Events::default(),
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
        self.drop_flag(slot, false);
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
            let left = self.fighters[s].left;
            *l = self.fighters[s].step_in(Phase::Playing, &inputs[s].sanitized(), &self.map);
            // A melee or a swap let go of the left gun (the fighter's own
            // tick, which the client predicts): it falls here.
            let f = &self.fighters[s];
            if left.arm != NONE && f.left.arm == NONE && f.active() {
                self.spill(left, 0, 0, f.body.pos + V3::new(DROP_APART, 0.0, 0.0), V3::ZERO);
            }
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
        self.falls();
        self.splatter();
        for (s, l) in locals.iter().enumerate() {
            if l.fired || l.fired_left || l.melee || l.throw {
                self.unveil(s);
            }
            if l.fired {
                self.fire(s, rewind_q8[s], l.charged, false);
            }
            if l.fired_left {
                self.fire(s, rewind_q8[s], l.charged_left, true);
            }
            if l.melee {
                self.melee(s, rewind_q8[s]);
            }
            if l.throw {
                self.throw(s, l.sticky);
            }
            // A carrier's USE lets go of the flag; otherwise a vehicle in
            // reach (or the one ridden) beats a pad.
            if l.use_pad && self.fighters[s].flag {
                self.drop_flag(s, false);
            } else if l.use_pad && self.fighters[s].active() && !self.use_vehicle(s) {
                self.pickup(s);
            }
        }
        self.lunges();
        self.projectiles();
        for s in 0..MAX_FIGHTERS {
            self.loot(s);
            self.powers(s);
        }
        for s in 0..MAX_FIGHTERS {
            self.recover(s);
        }
        for i in 0..MAX_VEHICLES {
            self.vehicle_back(i);
        }
        for p in self.pads.iter_mut() {
            p.empty_for = p.empty_for.saturating_sub(1);
        }
        self.fly_drops();
        self.flags_tick();
        for d in self.drops.iter_mut().filter(|d| d.active) {
            d.ttl = d.ttl.saturating_sub(1);
            if d.ttl == 0 {
                d.active = false;
            }
        }
        if self.phase == Phase::Playing {
            self.clock -= 1;
            if self.clock == 0 {
                self.end();
            }
        }
    }

    /// `s` fires the gun in its right hand, or its `left`.
    fn fire(&mut self, s: usize, depth: u32, charged: bool, left: bool) {
        let f = self.fighters[s];
        let id = if left { f.left.arm } else { f.gun().arm };
        let a = arm(id);
        // Dual, each gun's cone is its dual one; the left's draws apart
        // from the right's (both count in `shots` the same tick).
        let cone = if f.dual() { a.dual_spread } else { a.spread };
        let pk = if left { 64 } else { 0 };
        let eye = f.eye();
        let base = aim_dir(f.yaw, f.pitch);
        match a.kind {
            Kind::Hitscan => {
                for k in 0..a.pellets {
                    let dir = spread(base, cone, self.salt, s, f.shots, pk + k);
                    self.hitscan(s, eye, dir, id, depth);
                }
            }
            Kind::Sword => self.swing(s, depth, true),
            Kind::Rocket | Kind::Cannon | Kind::Bolt | Kind::Needle | Kind::Lob => {
                let dir = spread(base, cone, self.salt, s, f.shots, pk);
                let (kind, speed) = match a.kind {
                    Kind::Rocket => (ProjKind::Rocket, a.speed),
                    Kind::Cannon => (ProjKind::Cannon, a.speed),
                    Kind::Needle => (ProjKind::Needle, a.speed),
                    Kind::Lob => (ProjKind::Lob, a.speed),
                    _ if charged => (ProjKind::Charge, a.charge_speed),
                    _ => (ProjKind::Bolt, a.speed),
                };
                // A homing round locks on whoever the aim was on, as seen.
                let mut target = NO_TARGET;
                if matches!(kind, ProjKind::Needle | ProjKind::Charge) {
                    if let Some((Target::Fighter(q, _), _)) = self.trace(s, eye, base, a.range, depth, HOMING_PAD, true)
                    {
                        target = q as u8;
                    }
                }
                // A round that flies straight lives as long as its range
                // takes; one that blows up, till it lands.
                let ttl = match kind {
                    ProjKind::Rocket | ProjKind::Lob | ProjKind::Cannon => ticks(5000),
                    _ => ticks((a.range / speed * 1000.0) as u32).max(1),
                };
                self.launch(
                    Projectile {
                        kind,
                        arm: id,
                        target,
                        pos: eye + dir * 0.4,
                        vel: dir * speed,
                        ttl: ttl as u16,
                        ..Projectile::default()
                    },
                    s,
                );
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

    /// The nearest fighter (or vehicle) a ray from `eye` meets that `s` may
    /// hurt, where the server says the shooter saw them, and how far. With
    /// `lock` (homing, a lunge) a teammate is never the one, friendly fire
    /// or not.
    #[allow(clippy::too_many_arguments)]
    fn trace(&self, s: usize, eye: V3, dir: V3, reach: f32, depth: u32, pad: f32, lock: bool) -> Option<(Target, f32)> {
        let mut best: Option<(Target, f32)> = None;
        let mut near = reach;
        for (q, v) in self.fighters.iter().enumerate() {
            if q == s || !v.active() || (if lock { self.mate(s, q) } else { self.spared(s, q) }) {
                continue;
            }
            let p = self.rewind.pose_at(self.tick, q, depth, &pose(v));
            if !p.alive {
                continue;
            }
            let hy = crate::movement::ducked(p.duck, HEAD_Y, HEAD_Y_CROUCH);
            let top = crate::movement::ducked(p.duck, HIT_TOP, HIT_TOP_CROUCH);
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
        let ride = self.fighters[s].riding().map(|(i, _)| i);
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

    /// Whether `q` is on `att`'s side (never `att` itself).
    fn mate(&self, att: usize, q: usize) -> bool {
        self.rules.teams && q != att && self.fighters[q].team == self.fighters[att].team
    }

    /// Whether `att` may not hurt `q`: a teammate, with friendly fire off.
    fn spared(&self, att: usize, q: usize) -> bool {
        !self.rules.friendly_fire && self.mate(att, q)
    }

    /// Whether `att` may not hurt `v`: with sides and friendly fire off, a
    /// teammate of theirs rides in it.
    fn spares(&self, att: usize, v: &Vehicle) -> bool {
        let team = self.fighters[att].team;
        self.rules.teams
            && !self.rules.friendly_fire
            && v.seats
                .iter()
                .any(|&x| x != 0 && x as usize - 1 != att && self.fighters[x as usize - 1].team == team)
    }

    /// Bullet magnetism: of the enemies a ray from `eye` along `dir`
    /// misses, the one whose rewound chest it passes nearest in angle,
    /// within `id`'s [`crate::arms::Arm::magnet`] (plus a body's width)
    /// and its `aim_range`, nearer than `limit` and in clear line: the way
    /// to that chest. Only + − × ÷ and sqrt; never the head.
    fn magnet(&self, s: usize, eye: V3, dir: V3, id: u8, depth: u32, limit: f32) -> Option<V3> {
        let a = arm(id);
        let me = &self.fighters[s];
        let reach = a.aim_range.min(limit);
        let mut best: Option<(f32, V3)> = None;
        for (q, v) in self.fighters.iter().enumerate() {
            if q == s || !v.active() || (self.rules.teams && v.team == me.team) {
                continue;
            }
            let p = self.rewind.pose_at(self.tick, q, depth, &pose(v));
            if !p.alive {
                continue;
            }
            let c = p.pos + V3::new(0.0, crate::movement::ducked(p.duck, CHEST_Y, CHEST_Y_CROUCH), 0.0);
            let to = c - eye;
            let t = to.dot(dir);
            if t <= 0.0 || t > reach {
                continue;
            }
            let off = (to - dir * t).len();
            if off > HIT_R + t * a.magnet || best.is_some_and(|b| b.0 * t <= off) {
                continue;
            }
            if self.map.clear(eye, c) {
                best = Some((off / t, c));
            }
        }
        best.map(|(_, c)| (c - eye).norm())
    }

    fn hitscan(&mut self, s: usize, eye: V3, dir: V3, id: u8, depth: u32) {
        let a = arm(id);
        let mut wall = self.map.ray(eye, dir, a.range);
        let mut hit = self.trace(s, eye, dir, wall, depth, 0.0, false);
        let mut dir = dir;
        if a.magnet > 0.0 && !matches!(hit, Some((Target::Fighter(..), _))) {
            if let Some(bent) = self.magnet(s, eye, dir, id, depth, hit.map_or(wall, |h| h.1)) {
                dir = bent;
                wall = self.map.ray(eye, dir, a.range);
                hit = self.trace(s, eye, dir, wall, depth, 0.0, false);
            }
        }
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
        // With a sword in hand the melee is the sword's swing, no lunge.
        if arm(f.gun().arm).kind == Kind::Sword {
            self.swing(s, depth, false);
            return;
        }
        let eye = f.eye();
        let dir = aim_dir(f.yaw, f.pitch);
        let wall = self.map.ray(eye, dir, MELEE_RANGE);
        self.events.push(Event {
            kind: Ev::Melee,
            a: s as u8,
            pos: eye,
            ..Event::default()
        });
        let hit = self.trace(s, eye, dir, wall, depth, 0.25, false);
        if let Some((Target::Vehicle(i), _)) = hit {
            self.damage_vehicle(s, i, MELEE_DMG * PT, Cause::Melee as u16);
        }
        if let Some((Target::Fighter(q, _), _)) = hit {
            self.blow(s, q);
            return;
        }
        if hit.is_some() || f.ride != 0 {
            return;
        }
        // Past reach but near, with the aim on them: the lunge (Halo 2's),
        // and the blow when it gets there (`lunges`).
        let wall = self.map.ray(eye, dir, MELEE_LUNGE_RANGE);
        if let Some((Target::Fighter(q, _), _)) = self.trace(s, eye, dir, wall, depth, MELEE_LUNGE_PAD, true) {
            let me = &mut self.fighters[s];
            me.lunge = q as u8 + 1;
            me.lunge_left = ticks(MELEE_LUNGE_MS) as u16;
            me.lunge_melee = true;
            self.steer_lunge(s, q);
        }
    }

    /// A melee lands on `q`: from behind it kills, else it's a blow.
    fn blow(&mut self, s: usize, q: usize) {
        let v = &self.fighters[q];
        let (sy, cy) = crate::geom::yaw_sincos(v.yaw);
        let toward = (v.body.pos - self.fighters[s].body.pos).flat().norm();
        if V3::new(cy, 0.0, sy).dot(toward) > BEHIND_COS {
            self.damage(s, q, KILL_PTS, false, Head::None, Cause::Behind as u16);
        } else {
            self.damage(s, q, MELEE_DMG * PT, false, Head::None, Cause::Melee as u16);
        }
    }

    /// A sword pulled (or swung, without `lunge`): whoever the aim is on,
    /// their body grown by [`LUNGE_PAD`] and where `s` saw them, dies if in
    /// [`SWORD_REACH`]; farther, to [`LUNGE_RANGE`], `s` lunges at them.
    /// A hull in reach is cut; a hull is never lunged at.
    fn swing(&mut self, s: usize, depth: u32, lunge: bool) {
        let f = self.fighters[s];
        let eye = f.eye();
        let dir = aim_dir(f.yaw, f.pitch);
        self.events.push(Event {
            kind: Ev::Melee,
            a: s as u8,
            c: crate::arms::SWORD as u16,
            pos: eye,
            ..Event::default()
        });
        let reach = if lunge { LUNGE_RANGE } else { SWORD_REACH };
        let wall = self.map.ray(eye, dir, reach);
        match self.trace(s, eye, dir, wall, depth, LUNGE_PAD, false) {
            Some((Target::Vehicle(i), t)) if t <= SWORD_REACH => {
                let pts = arm(crate::arms::SWORD).dmg * PT;
                self.damage_vehicle(s, i, pts, crate::arms::SWORD as u16);
            }
            Some((Target::Fighter(q, _), t)) if t <= SWORD_REACH => self.strike(s, q),
            Some((Target::Fighter(q, _), _)) if lunge && !self.mate(s, q) => {
                let me = &mut self.fighters[s];
                me.lunge = q as u8 + 1;
                me.lunge_left = ticks(LUNGE_MS) as u16;
                self.steer_lunge(s, q);
            }
            _ => {}
        }
    }

    /// Every lunge: on, at the target as it is now, till it is reached
    /// (the kill), the time runs out, or either is gone.
    fn lunges(&mut self) {
        for s in 0..MAX_FIGHTERS {
            let f = self.fighters[s];
            if f.lunge == 0 {
                continue;
            }
            let q = f.lunge as usize - 1;
            let v = self.fighters[q];
            let armed = f.lunge_melee || f.gun().arm == crate::arms::SWORD;
            let on = f.active() && f.ride == 0 && v.active() && v.ride == 0 && armed;
            if !on {
                self.end_lunge(s);
                continue;
            }
            let d = v.body.pos - f.body.pos;
            let chest = v.body.pos + V3::new(0.0, 0.9, 0.0);
            if d.flat().len() <= STRIKE_R && crate::fmath::fabs(d.y) < BODY_H && self.map.clear(f.eye(), chest) {
                self.end_lunge(s);
                if f.lunge_melee {
                    self.blow(s, q);
                } else {
                    self.strike(s, q);
                }
                continue;
            }
            self.fighters[s].lunge_left -= 1;
            if self.fighters[s].lunge_left == 0 {
                self.end_lunge(s);
            } else {
                self.steer_lunge(s, q);
            }
        }
    }

    /// `s`'s body driven straight at `q`'s at [`LUNGE_SPEED`] (a melee's
    /// at [`MELEE_LUNGE_SPEED`]).
    fn steer_lunge(&mut self, s: usize, q: usize) {
        let to = self.fighters[q].body.pos - self.fighters[s].body.pos;
        let len = to.len();
        if len <= 0.0 {
            return;
        }
        let speed = if self.fighters[s].lunge_melee {
            MELEE_LUNGE_SPEED
        } else {
            LUNGE_SPEED
        };
        let v = to * (speed / len);
        let b = &mut self.fighters[s].body;
        b.vel = V3::new(
            crate::movement::quant(v.x),
            crate::movement::quant(v.y),
            crate::movement::quant(v.z),
        );
        if v.y > 0.0 {
            b.grounded = false;
        }
    }

    /// A lunge over: the body stops where it got to (falling, if it was).
    fn end_lunge(&mut self, s: usize) {
        let f = &mut self.fighters[s];
        if f.lunge != 0 && f.active() {
            let v = f.body.vel;
            f.body.vel = V3::new(0.0, v.y.min(0.0), 0.0);
        }
        f.lunge = 0;
        f.lunge_left = 0;
        f.lunge_melee = false;
    }

    /// The sword lands on `q`: a kill, unless `q` was lunging back at `s`
    /// (a clash: both thrown apart, nobody hurt).
    fn strike(&mut self, s: usize, q: usize) {
        if self.fighters[q].lunge == s as u8 + 1 && !self.fighters[q].lunge_melee {
            self.end_lunge(q);
            let away = (self.fighters[q].body.pos - self.fighters[s].body.pos).flat().norm();
            for (k, dir) in [(s, away * -1.0), (q, away)] {
                let b = &mut self.fighters[k].body;
                b.vel = dir * CLASH_PUSH + V3::new(0.0, 3.0, 0.0);
                b.grounded = false;
            }
            // Thrown off an edge by it: the other's doing.
            self.mark_hurt(s, q);
            self.mark_hurt(q, s);
            self.events.push(Event {
                kind: Ev::Melee,
                a: s as u8,
                part: 2,
                c: crate::arms::SWORD as u16,
                pos: self.fighters[s].eye(),
                ..Event::default()
            });
            return;
        }
        let pts = arm(crate::arms::SWORD).dmg * PT;
        self.damage(s, q, pts, false, Head::None, crate::arms::SWORD as u16);
    }

    fn throw(&mut self, s: usize, sticky: bool) {
        let f = self.fighters[s];
        let dir = aim_dir(f.yaw, f.pitch);
        let vel = dir * GREN_SPEED + V3::new(0.0, 2.0, 0.0);
        // A grenade's fuse is lit when it sticks or comes down; till then,
        // long.
        let (kind, ttl) = if sticky {
            (ProjKind::Sticky, ticks(10_000))
        } else {
            (ProjKind::Frag, ticks(GREN_LIFE_MS))
        };
        // Never from the far side of a wall it was thrown up against.
        let reach = self.map.ray(f.eye(), dir, 0.4);
        let out = if reach < 0.4 { (reach - 0.05).max(0.0) } else { 0.4 };
        self.launch(
            Projectile {
                kind,
                pos: f.eye() + dir * out,
                vel,
                ttl: ttl as u16,
                target: NO_TARGET,
                ..Projectile::default()
            },
            s,
        );
        self.events.push(Event {
            kind: Ev::Throw,
            a: s as u8,
            c: sticky as u16,
            pos: f.eye(),
            ..Event::default()
        });
    }

    /// A sticky's tick: riding what it is stuck to, or still where it stuck
    /// to the map; else flying, sticking to the first thing it meets. Whether
    /// it is still about (it goes off at the end of its fuse).
    fn sticky(&mut self, p: &mut Projectile) -> bool {
        if p.landed {
            if p.target != NO_TARGET {
                let t = p.target as usize;
                let on = if t < MAX_FIGHTERS {
                    let v = &self.fighters[t];
                    v.active().then_some(v.body.pos)
                } else {
                    let v = &self.vehicles[t - MAX_FIGHTERS];
                    v.active.then_some(v.pos)
                };
                match on {
                    Some(at) => p.pos = at + p.vel,
                    // What it was on is gone: it falls, its fuse running, and
                    // sticks to what it meets.
                    None => {
                        p.target = NO_TARGET;
                        p.vel = V3::ZERO;
                        p.landed = false;
                    }
                }
            }
        } else {
            p.vel.y -= GRAVITY * DT;
            let speed = p.vel.len();
            let d = if speed > 0.0 { p.vel * (1.0 / speed) } else { V3::ZERO };
            match self.flight_hit(p, d, speed * DT, false) {
                (t, true, Some(who)) => {
                    let (target, base) = match who {
                        Target::Fighter(q, _) => (q as u8, self.fighters[q].body.pos),
                        Target::Vehicle(i) => ((MAX_FIGHTERS + i) as u8, self.vehicles[i].pos),
                    };
                    p.target = target;
                    p.pos = p.pos + d * t;
                    p.vel = p.pos - base;
                    p.landed = true;
                    p.ttl = p.ttl.min(fuse(p.age, STICKY_FUSE_MS, STICKY_ARM_MS));
                }
                (t, true, None) => {
                    p.pos = p.pos + d * (t - STICKY_OFF_M).max(0.0);
                    p.vel = V3::ZERO;
                    p.landed = true;
                    p.ttl = p.ttl.min(fuse(p.age, STICKY_FUSE_MS, STICKY_ARM_MS));
                }
                _ => p.pos = p.pos + p.vel * DT,
            }
        }
        if p.ttl > 0 {
            return true;
        }
        // Off: the one it is stuck to dies (unless a teammate of the
        // thrower's, as with every blast), then the blast.
        let owner = p.owner as usize;
        if p.landed && (p.target as usize) < MAX_FIGHTERS {
            let q = p.target as usize;
            if !self.spared(owner, q) {
                self.damage(owner, q, KILL_PTS, false, Head::None, Cause::Sticky as u16);
            }
        }
        self.blast_k(
            p.pos + V3::new(0.0, 0.05, 0.0),
            owner,
            STICKY_DMG,
            STICKY_RADIUS,
            STICKY_FULL,
            STICKY_EDGE,
            1.0,
            Cause::Sticky as u16,
            GREN_SHIELD_K,
        );
        false
    }

    fn launch(&mut self, p: Projectile, s: usize) {
        // A full pool drops the throw, as it always has.
        let _ = self.proj.alloc(Projectile {
            active: true,
            owner: s as u8,
            age: 0,
            landed: false,
            ..p
        });
    }

    /// What a ray from `pos` along `d` (unit) meets first within `reach`:
    /// a fighter (its body grown by 0.1 m), a hull, or the map (None), and
    /// how far. A round clears its owner (and the hull it rides) for its
    /// first 16 ticks; one that `spares` sides flies through teammates.
    fn flight_hit(&self, p: &Projectile, d: V3, reach: f32, spares: bool) -> (f32, bool, Option<Target>) {
        let mut t = self.map.ray(p.pos, d, reach);
        let mut met = t < reach;
        let mut who = None;
        let owner = p.owner as usize;
        for (q, v) in self.fighters.iter().enumerate() {
            if !v.active() || (q == owner && p.age < 16) {
                continue;
            }
            if spares && self.spared(owner, q) {
                continue;
            }
            if let Some(h) = ray_cylinder(p.pos, d, v.body.pos, HIT_R + 0.1, v.body.height(), t) {
                if h < t || !met {
                    t = h;
                    met = true;
                    who = Some(Target::Fighter(q, false));
                }
            }
        }
        let own = self.fighters[owner].riding().map(|(i, _)| i);
        for (k, v) in self.vehicles.iter().enumerate() {
            if !v.active || (own == Some(k) && p.age < 16) || (spares && self.spares(owner, v)) {
                continue;
            }
            if let Some(h) = v.ray(p.pos, d, t) {
                if h < t || !met {
                    t = h;
                    met = true;
                    who = Some(Target::Vehicle(k));
                }
            }
        }
        (t, met, who)
    }

    /// A round that flies straight (bent by its gravity, turned at its
    /// target, its speed eased toward its top): one tick of it. Whether it
    /// is still flying.
    fn fly(&mut self, p: &mut Projectile) -> bool {
        let a = arm(p.arm);
        if a.homing > 0.0 && (p.target as usize) < MAX_FIGHTERS {
            let v = self.fighters[p.target as usize];
            let speed = p.vel.len();
            if v.active() && speed > 0.0 {
                let to = (v.body.pos + V3::new(0.0, 1.0, 0.0) - p.pos).norm();
                let d = p.vel * (1.0 / speed);
                let chord = (to - d).len();
                let turn = a.homing * DT;
                let nd = if chord <= turn {
                    to
                } else {
                    (d + (to - d) * (turn / chord)).norm()
                };
                p.vel = nd * speed;
            }
        }
        if a.gravity > 0.0 {
            p.vel.y -= GRAVITY * a.gravity * DT;
        }
        let speed = p.vel.len();
        if speed <= 0.0 {
            return false;
        }
        let d = p.vel * (1.0 / speed);
        let step = speed * DT;
        let blasts = matches!(p.kind, ProjKind::Rocket | ProjKind::Cannon);
        let (t, met, who) = self.flight_hit(p, d, step, !blasts);
        if met || p.ttl == 0 {
            let at = p.pos + d * (t - 0.05).max(0.0);
            self.impact(p, at, if met { who } else { None });
            return false;
        }
        p.pos = p.pos + p.vel * DT;
        // Eased by the metre toward its top speed (faster for a rocket,
        // slower for a launcher's or a cannon's round).
        if a.speed_max > 0.0 {
            let k = (a.speed_max - a.speed) / a.ramp;
            if k * (a.speed_max - speed) > 0.0 {
                let to = speed + k * step;
                let to = if k > 0.0 {
                    to.min(a.speed_max)
                } else {
                    to.max(a.speed_max)
                };
                p.vel = d * to;
            }
        }
        true
    }

    /// A round lands at `at`, on `who` if anyone.
    fn impact(&mut self, p: &Projectile, at: V3, who: Option<Target>) {
        let a = arm(p.arm);
        let owner = p.owner as usize;
        let cause = p.arm as u16;
        match (p.kind, who) {
            (ProjKind::Rocket | ProjKind::Cannon | ProjKind::Lob, _) => {
                self.blast(at, owner, a.dmg, a.radius, a.full, 0, 0.5, cause);
            }
            (ProjKind::Bolt, Some(Target::Fighter(q, _))) => {
                self.damage_k(owner, q, a.dmg * PT, false, Head::None, cause, a.shield_k);
            }
            (ProjKind::Charge, Some(Target::Fighter(q, _))) => self.strip(owner, q),
            (ProjKind::Needle, Some(Target::Fighter(q, _))) => {
                self.damage_k(owner, q, a.dmg * PT, false, Head::None, cause, a.shield_k);
                self.stick(owner, q, p.arm);
            }
            (ProjKind::Bolt | ProjKind::Needle, Some(Target::Vehicle(i))) => {
                self.damage_vehicle(owner, i, a.dmg * PT, cause);
            }
            (ProjKind::Charge, Some(Target::Vehicle(i))) => {
                self.damage_vehicle(owner, i, CHARGE_VEH_DMG * PT, cause);
            }
            _ => {}
        }
    }

    /// A charged bolt takes every shield `vic` has and nothing else.
    fn strip(&mut self, att: usize, vic: usize) {
        let v = &mut self.fighters[vic];
        if !v.alive {
            return;
        }
        let took = v.shield;
        v.shield = 0;
        v.charging = false;
        v.since_hit = 0;
        if v.camo > 0 {
            v.flare = ticks(CAMO_FLARE_MS) as u16;
        }
        self.events.push(Event {
            kind: Ev::Hit,
            a: att as u8,
            b: vic as u8,
            part: 0,
            c: (took / PT) as u16,
            pos: v.body.pos,
            ..Event::default()
        });
    }

    /// A shard sticks in `vic`; at the gun's `stick` they go off together.
    fn stick(&mut self, att: usize, vic: usize, id: u8) {
        let v = &mut self.fighters[vic];
        if !v.alive {
            return;
        }
        v.needles += 1;
        v.needle_t = 0;
        if v.needles < arm(id).stick {
            return;
        }
        v.needles = 0;
        let at = v.body.pos + V3::new(0.0, 1.0, 0.0);
        self.events.push(Event {
            kind: Ev::Blast,
            a: att as u8,
            c: id as u16,
            pos: at,
            ..Event::default()
        });
        self.damage(att, vic, KILL_PTS, false, Head::None, id as u16);
    }

    /// A frag's or a lob's tick under gravity off the map's floors and
    /// walls: where it goes, and whether it touched anything.
    /// One tick of a thrown thing under `g`, off what it meets: whether it
    /// touched anything, and whether that was a floor (the map's, a box's
    /// top, a solid's), not a wall.
    fn bounce(&self, p: &mut Projectile, g: f32) -> (bool, bool) {
        self.bounce_at(&mut p.pos, &mut p.vel, g, GREN_BOUNCE, GREN_SLIDE)
    }

    /// [`Self::bounce`] for anything at `pos` moving at `vel`: `bounce` of
    /// the speed into a surface comes back, `slide` of the speed along a
    /// floor is kept.
    fn bounce_at(&self, pos: &mut V3, vel: &mut V3, g: f32, bounce: f32, slide: f32) -> (bool, bool) {
        let mut touched = false;
        let mut ground = false;
        vel.y -= g * DT;
        let mut n = *pos + *vel * DT;
        // Profile solids are met below, along their own normals.
        let floor = self.map.floor_flat(n.x, n.z, 0.0, pos.y + 0.001);
        if n.y < floor {
            touched = true;
            ground = true;
            n.y = floor;
            if self.map.terrain.is_some() && floor == self.map.ground(n.x, n.z) {
                // Off a slope: the part into the ground bounces back, the
                // part along it slides on.
                let gn = self.map.ground_normal(n.x, n.z);
                let into = vel.dot(gn);
                let along = *vel - gn * into;
                *vel = along * slide - gn * (into * bounce);
            } else {
                *vel = V3::new(vel.x * slide, -vel.y * bounce, vel.z * slide);
            }
        }
        for b in &self.map.boxes {
            if !b.contains(n) {
                continue;
            }
            touched = true;
            if pos.x <= b.min.x || pos.x >= b.max.x {
                n.x = if pos.x <= b.min.x {
                    b.min.x - 0.01
                } else {
                    b.max.x + 0.01
                };
                vel.x = -vel.x * bounce;
            } else if pos.z <= b.min.z || pos.z >= b.max.z {
                n.z = if pos.z <= b.min.z {
                    b.min.z - 0.01
                } else {
                    b.max.z + 0.01
                };
                vel.z = -vel.z * bounce;
            } else if pos.y <= b.min.y {
                n.y = b.min.y - 0.01;
                vel.y = -vel.y * bounce;
            }
        }
        for s in &self.map.solids {
            if s.is_profile() {
                if !s.contains(n) {
                    continue;
                }
                touched = true;
                let step = n - *pos;
                let l = step.len();
                let hit = if l > 0.0 {
                    s.ray_hit(*pos, step * (1.0 / l), l)
                } else {
                    None
                };
                match hit {
                    Some((t, piece)) if piece != crate::profile::Piece::Inside => {
                        // Off the surface as it is there: along the part
                        // into it, back out; on ground it can stand on, the
                        // rest slides on as on a slope.
                        let h = *pos + step * (t / l);
                        let nn = s.normal(piece, h);
                        n = h + nn * 0.01;
                        let into = vel.dot(nn);
                        if nn.y >= crate::consts::WALK_NY {
                            ground = true;
                            let along = *vel - nn * into;
                            *vel = along * slide - nn * (into * bounce);
                        } else if into < 0.0 {
                            *vel = *vel - nn * (into * (1.0 + bounce));
                        }
                    }
                    _ => {
                        // It started inside: up out of the top.
                        ground = true;
                        n.y = s.top_at(n.x, n.z) + 0.01;
                        vel.y = -vel.y * bounce;
                    }
                }
                continue;
            }
            let top = s.top_at(n.x, n.z);
            if !(n.y > s.y0 && n.y < top && s.over(n.x, n.z, 0.0)) {
                continue;
            }
            touched = true;
            if pos.y >= s.top_at(pos.x, pos.z) - 0.05 {
                // Onto its top (a slope moves up under a projectile).
                ground = true;
                n.y = top + 0.01;
                vel.y = -vel.y * bounce;
            } else if pos.y <= s.y0 {
                n.y = s.y0 - 0.01;
                vel.y = -vel.y * bounce;
            } else if let Some((x, z, ux, uz)) = s.push(n.x, n.z, 0.01) {
                // Off its side: the part into the face back out.
                n.x = x;
                n.z = z;
                let into = vel.x * ux + vel.z * uz;
                if into < 0.0 {
                    vel.x -= ux * into * (1.0 + bounce);
                    vel.z -= uz * into * (1.0 + bounce);
                }
            }
        }
        *pos = n;
        (touched, ground)
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
                ProjKind::Frag => {
                    // Its fuse lit on first coming down on a floor.
                    if self.bounce(&mut p, GRAVITY).1 && !p.landed {
                        p.landed = true;
                        p.ttl = p.ttl.min(fuse(p.age, GREN_FUSE_MS, GREN_ARM_MS));
                    }
                    if p.ttl == 0 {
                        self.proj[i].active = false;
                        self.blast_k(
                            p.pos + V3::new(0.0, 0.05, 0.0),
                            p.owner as usize,
                            GREN_DMG,
                            GREN_RADIUS,
                            GREN_FULL,
                            GREN_EDGE,
                            1.0,
                            Cause::Grenade as u16,
                            GREN_SHIELD_K,
                        );
                        continue;
                    }
                }
                ProjKind::Lob => {
                    // Off on a body it meets; else its fuse, lit on first
                    // touching anything.
                    let a = arm(p.arm);
                    let speed = p.vel.len();
                    if speed > 0.0 {
                        let d = p.vel * (1.0 / speed);
                        if let (t, true, Some(Target::Fighter(..) | Target::Vehicle(_))) =
                            self.flight_hit(&p, d, speed * DT, true)
                        {
                            self.proj[i].active = false;
                            let at = p.pos + d * (t - 0.05).max(0.0);
                            self.impact(&p, at, None);
                            continue;
                        }
                    }
                    if self.bounce(&mut p, GRAVITY * a.gravity).0 && !p.landed {
                        p.landed = true;
                        p.ttl = p.ttl.min(ticks(a.fuse_ms) as u16);
                    }
                    if p.ttl == 0 {
                        self.proj[i].active = false;
                        self.impact(&p, p.pos + V3::new(0.0, 0.05, 0.0), None);
                        continue;
                    }
                }
                ProjKind::Sticky => {
                    if !self.sticky(&mut p) {
                        self.proj[i].active = false;
                        continue;
                    }
                }
                _ => {
                    if !self.fly(&mut p) {
                        self.proj[i].active = false;
                        continue;
                    }
                }
            }
            // Into the void: gone, never going off where nobody stands.
            if p.pos.y < self.map.void {
                self.proj[i].active = false;
                continue;
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

    /// A blast of `dmg` at `at`: whole to `full`, then down to `edge` at
    /// `radius` (0 for most; Halo 2's grenades still do 50 there), nothing
    /// past it or through the map.
    #[allow(clippy::too_many_arguments)]
    fn blast(&mut self, at: V3, owner: usize, dmg: i32, radius: f32, full: f32, edge: i32, self_k: f32, cause: u16) {
        self.blast_k(at, owner, dmg, radius, full, edge, self_k, cause, 1.0);
    }

    /// [`Self::blast`] with shields taking `shield_k` of it: a grenade's.
    #[allow(clippy::too_many_arguments)]
    fn blast_k(
        &mut self,
        at: V3,
        owner: usize,
        dmg: i32,
        radius: f32,
        full: f32,
        edge: i32,
        self_k: f32,
        cause: u16,
        shield_k: f32,
    ) {
        let rim = edge as f32 / dmg.max(1) as f32;
        let falloff = |dist: f32| {
            if dist <= full {
                1.0
            } else {
                1.0 - (dist - full) / (radius - full) * (1.0 - rim)
            }
        };
        self.events.push(Event {
            kind: Ev::Blast,
            a: owner as u8,
            c: cause,
            pos: at,
            ..Event::default()
        });
        for q in 0..MAX_FIGHTERS {
            let v = self.fighters[q];
            if !v.active() || self.spared(owner, q) {
                continue;
            }
            let chest = v.body.pos + V3::new(0.0, 0.9, 0.0);
            let d = chest - at;
            let dist = d.len();
            if dist >= radius || !self.map.clear(at, chest) {
                continue;
            }
            let k = falloff(dist);
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
            self.damage_k(owner, q, pts as i32, false, Head::None, cause, shield_k);
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
            let k = falloff(dist);
            let away = if dist > 0.0 {
                d * (1.0 / dist)
            } else {
                V3::new(0.0, 1.0, 0.0)
            };
            // Thrown, by how big a charge it was (a rocket's whole): along
            // the ground away from it, and up off its wheels. How it turns
            // over in the air is the renderer's (`look/truck.rs`).
            let heft = (dmg as f32 / VEH_LIFT_DMG as f32).min(1.0) * k;
            let veh = &mut self.vehicles[i];
            let flat = V3::new(away.x, 0.0, away.z);
            let up = (veh.vel.y.max(0.0) + VEH_BLAST_LIFT * heft).min(VEH_LIFT_MAX);
            veh.vel = veh.vel + flat * (BLAST_PUSH * VEH_PUSH_K * heft);
            veh.vel.y = up;
            veh.grounded = false;
            self.damage_vehicle(owner, i, ((dmg * PT) as f32 * k) as i32, cause);
        }
        // Guns and grenades on the floor (the blast's own dead's among
        // them) go flying.
        for i in 0..MAX_DROPS {
            let d = self.drops[i];
            if !d.active {
                continue;
            }
            let mid = d.pos + V3::new(0.0, DROP_MID, 0.0);
            let off = mid - at;
            let dist = off.len();
            if dist >= radius || !self.map.clear(at, mid) {
                continue;
            }
            let k = falloff(dist);
            let away = if dist > 0.0 {
                off * (1.0 / dist)
            } else {
                V3::new(0.0, 1.0, 0.0)
            };
            let d = &mut self.drops[i];
            d.vel = d.vel + away * (BLAST_PUSH * DROP_PUSH_K * k) + V3::new(0.0, DROP_LIFT * k, 0.0);
            // Grenades lying in it cook off, a pile of a kind as one.
            let (frags, stickies, pos) = (d.frags > 0, d.stickies > 0, d.pos);
            d.frags = 0;
            d.stickies = 0;
            if d.spent() {
                d.active = false;
            }
            if frags {
                self.cook_off(ProjKind::Frag, pos, owner);
            }
            if stickies {
                self.cook_off(ProjKind::Sticky, pos, owner);
            }
        }
        // Grenades in its reach go off a moment after it, each its own
        // thrower's; one not stuck to anything is thrown by it too.
        for j in 0..MAX_PROJ {
            let g = self.proj[j];
            if !g.active || !matches!(g.kind, ProjKind::Frag | ProjKind::Sticky | ProjKind::Lob) {
                continue;
            }
            let off = g.pos - at;
            let dist = off.len();
            if dist >= radius || !self.map.clear(at, g.pos) {
                continue;
            }
            let away = if dist > 0.0 {
                off * (1.0 / dist)
            } else {
                V3::new(0.0, 1.0, 0.0)
            };
            let g = &mut self.proj[j];
            g.ttl = g.ttl.min(ticks(GREN_CHAIN_MS) as u16);
            if !(g.kind == ProjKind::Sticky && g.landed) {
                g.vel = g.vel + away * (BLAST_PUSH * falloff(dist));
            }
        }
    }

    /// A grenade going off where it lies, `owner`'s doing.
    fn cook_off(&mut self, kind: ProjKind, pos: V3, owner: usize) {
        // A full pool leaves it unlit, as a full pool drops a throw.
        let _ = self.proj.alloc(Projectile {
            active: true,
            kind,
            owner: owner as u8,
            pos: pos + V3::new(0.0, DROP_MID, 0.0),
            ttl: ticks(GREN_CHAIN_MS) as u16,
            target: NO_TARGET,
            landed: true,
            ..Projectile::default()
        });
    }

    /// Drops in the air: a tick of each under gravity, off the floors and
    /// walls as a frag goes, until one lies still on a floor. Kept on the
    /// map's floor plan; one that falls out of the world is gone.
    fn fly_drops(&mut self) {
        for i in 0..MAX_DROPS {
            let mut d = self.drops[i];
            if !d.active || d.vel == V3::ZERO {
                continue;
            }
            let (_, ground) = self.bounce_at(&mut d.pos, &mut d.vel, GRAVITY, DROP_BOUNCE, DROP_SLIDE);
            let (x, z) = (
                d.pos.x.clamp(self.map.lo.0, self.map.hi.0),
                d.pos.z.clamp(self.map.lo.1, self.map.hi.1),
            );
            if x != d.pos.x {
                d.vel.x = 0.0;
            }
            if z != d.pos.z {
                d.vel.z = 0.0;
            }
            d.pos.x = x;
            d.pos.z = z;
            if ground && d.vel.len() < DROP_SETTLE {
                d.vel = V3::ZERO;
                d.pos = V3::new(
                    crate::movement::quant(d.pos.x),
                    crate::movement::quant(d.pos.y),
                    crate::movement::quant(d.pos.z),
                );
            }
            if d.pos.y < DROP_LOST_Y.max(self.map.void) {
                d.active = false;
            }
            self.drops[i] = d;
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
                    self.kill(att, q, Cause::Wreck as u16, false);
                }
            }
        }
        self.blast(
            v.pos + V3::new(0.0, 0.8, 0.0),
            att,
            WRECK_DMG,
            WRECK_RADIUS,
            WRECK_FULL,
            0,
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

    /// On a map with a void: `att` hurt (or threw) `vic`, so a fall soon
    /// after is `att`'s kill ([`FALL_CREDIT_MS`]).
    fn mark_hurt(&mut self, att: usize, vic: usize) {
        if self.map.void > f32::NEG_INFINITY && att != vic && att < MAX_FIGHTERS {
            let v = &mut self.fighters[vic];
            v.hurt_by = att as u8 + 1;
            v.hurt_t = ticks(FALL_CREDIT_MS) as u16;
        }
    }

    /// The void: a fighter whose feet are under it has fallen to its death,
    /// the kill of whoever last hurt it if that was within
    /// [`FALL_CREDIT_MS`] on its feet (and they're still here), else its
    /// own; a vehicle under it is lost (its riders fell first), back at its
    /// spawn after the map says. The credit's clock runs here too, on the
    /// ground only.
    fn falls(&mut self) {
        let void = self.map.void;
        if void == f32::NEG_INFINITY {
            return;
        }
        for s in 0..MAX_FIGHTERS {
            let f = &mut self.fighters[s];
            if !f.active() {
                continue;
            }
            if f.body.pos.y >= void {
                if f.body.grounded && f.hurt_by != 0 {
                    f.hurt_t = f.hurt_t.saturating_sub(1);
                    if f.hurt_t == 0 {
                        f.hurt_by = 0;
                    }
                }
                continue;
            }
            let by = f.hurt_by as usize;
            let att = if by != 0 && self.fighters[by - 1].seated {
                by - 1
            } else {
                s
            };
            self.kill(att, s, Cause::Fall as u16, false);
        }
        for i in 0..MAX_VEHICLES {
            let v = self.vehicles[i];
            if !v.active || v.pos.y >= void {
                continue;
            }
            for &x in &v.seats {
                if x != 0 {
                    self.fighters[x as usize - 1].ride = 0;
                }
            }
            let back = self.map.vehicles.get(v.home as usize).map_or(30_000, |d| d.respawn_ms);
            self.vehicles[i] = Vehicle {
                active: false,
                respawn: ticks(back).max(1),
                home: v.home,
                pos: v.pos,
                yaw: v.yaw,
                ..Vehicle::default()
            };
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
                if driver.is_some_and(|d| self.spared(d, q)) {
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
                        KILL_PTS,
                        false,
                        Head::None,
                        Cause::Splatter as u16,
                    );
                }
            }
        }
    }

    fn damage(&mut self, att: usize, vic: usize, pts: i32, head: bool, rule: Head, cause: u16) {
        self.damage_k(att, vic, pts, head, rule, cause, 1.0);
    }

    /// [`Self::damage`] with the shields taking `pts × shield_k`; what gets
    /// past them reaches health as itself.
    #[allow(clippy::too_many_arguments)]
    fn damage_k(&mut self, att: usize, vic: usize, pts: i32, head: bool, rule: Head, cause: u16, shield_k: f32) {
        if !self.fighters[vic].alive || pts <= 0 {
            return;
        }
        self.mark_hurt(att, vic);
        let v = &mut self.fighters[vic];
        v.since_hit = 0;
        if v.camo > 0 {
            v.flare = ticks(CAMO_FLARE_MS) as u16;
        }
        let lethal = head && (rule == Head::Always || (rule == Head::Unshielded && v.shield == 0));
        let part = if lethal {
            v.shield = 0;
            v.health = 0;
            2
        } else {
            let on_shield = if shield_k == 1.0 {
                pts
            } else {
                (pts as f32 * shield_k) as i32
            };
            let soak = on_shield.min(v.shield);
            // The share of `pts` the shields used up.
            let used = if on_shield > 0 {
                (soak as i64 * pts as i64 / on_shield as i64) as i32
            } else {
                0
            };
            v.shield -= soak;
            v.health -= pts - used;
            match (head, pts > used) {
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
            self.kill(att, vic, cause, part == 2);
        }
    }

    /// `att` killed `vic` with `cause`; `head`: the blow was to the head.
    fn kill(&mut self, att: usize, vic: usize, cause: u16, head: bool) {
        self.leave_vehicle(vic);
        // Off the map, the flag goes home; otherwise it falls with them.
        self.drop_flag(vic, cause == crate::events::Cause::Fall as u16);
        let v = &mut self.fighters[vic];
        v.alive = false;
        v.health = 0;
        v.shield = 0;
        v.charging = false;
        v.camo = 0;
        v.flare = 0;
        // What they held leaves their hands with some of how they moved.
        let toss = v.body.vel * DROP_CARRY + V3::new(0.0, DROP_TOSS, 0.0);
        v.body.vel = V3::ZERO;
        v.needles = 0;
        v.respawn = ticks(RESPAWN_MS) as u16;
        v.deaths += 1;
        let vteam = v.team;
        let pos = v.body.pos;
        // What they held falls where they fell (Gun Game's ladder guns
        // aren't for the taking).
        let (hand, left, frags, stickies) = (*v.gun(), v.left, v.grenades, v.stickies);
        // Fallen into the void, what they held went with them.
        if !self.rules.gun_game && pos.y >= self.map.void {
            self.spill(hand, 0, 0, pos, toss);
            self.spill(left, 0, 0, pos + V3::new(DROP_APART, 0.0, 0.0), toss);
            self.spill(
                Gun::default(),
                frags,
                stickies,
                pos + V3::new(0.0, 0.0, DROP_APART),
                toss,
            );
        }
        let k = &mut self.fighters[att];
        let own = att == vic || (self.rules.teams && k.team == vteam);
        let side = if self.rules.teams { k.team as usize & 1 } else { att };
        let gun_game = self.rules.gun_game;
        if self.rules.ctf {
            // Captures score; a kill only counts for the fighter.
            if !own {
                k.kills += 1;
                k.grenades = (k.grenades + 1).min(GREN_MAX);
            }
        } else if own {
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
                k.drop_left();
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
            part: if head { 2 } else { 0 },
            c: cause,
            pos,
            ..Event::default()
        });
        if !self.rules.ctf && self.score[side] >= self.rules.score_limit {
            self.end();
        }
    }

    /// Capture the Flag's tick: carried flags follow their carriers and
    /// score at home; a flag at home or on the floor is taken by the first
    /// enemy on foot to touch it (seat order), a flag on the floor returned
    /// by the first defender, or by its clock.
    fn flags_tick(&mut self) {
        let Some(bases) = self.map.flags.filter(|_| self.rules.ctf && self.rules.teams) else {
            return;
        };
        for t in 0..2 {
            let fl = self.flags[t];
            match fl.state {
                FLAG_HELD => {
                    let c = fl.carrier as usize;
                    let at = self.fighters[c].body.pos;
                    self.flags[t].pos = at;
                    let own = t ^ 1;
                    if self.flags[own].state == FLAG_HOME && touches(at, bases[own]) {
                        self.fighters[c].flag = false;
                        self.flags[t] = Flag {
                            pos: bases[t],
                            ..Flag::default()
                        };
                        self.flag_event(3, c as u8, t, bases[own]);
                        self.score[own] += 1;
                        if self.score[own] >= self.rules.score_limit && self.phase == Phase::Playing {
                            self.end();
                        }
                    }
                }
                _ => {
                    let down = fl.state == FLAG_DOWN;
                    if down {
                        self.flags[t].down_for = fl.down_for.saturating_sub(1);
                        if self.flags[t].down_for == 0 {
                            self.flag_home(t, NO_WINNER);
                            continue;
                        }
                    }
                    for s in 0..MAX_FIGHTERS {
                        let f = self.fighters[s];
                        if !f.active() || f.riding().is_some() || !touches(f.body.pos, fl.pos) {
                            continue;
                        }
                        if f.team as usize & 1 == t {
                            if down {
                                self.flag_home(t, s as u8);
                                break;
                            }
                        } else if !f.flag && !(down && self.flags[t].down_for > ticks(FLAG_RETURN_MS - FLAG_REGRAB_MS))
                        {
                            self.fighters[s].flag = true;
                            self.flags[t] = Flag {
                                state: FLAG_HELD,
                                carrier: s as u8,
                                pos: f.body.pos,
                                down_for: 0,
                            };
                            self.flag_event(0, s as u8, t, fl.pos);
                            break;
                        }
                    }
                }
            }
        }
    }

    /// Seat `s` lets go of the flag it carries, if any: where it stands,
    /// or home if `home`.
    fn drop_flag(&mut self, s: usize, home: bool) {
        if !self.fighters[s].flag {
            return;
        }
        self.fighters[s].flag = false;
        let Some(t) = (0..2).find(|&t| self.flags[t].state == FLAG_HELD && self.flags[t].carrier as usize == s) else {
            return;
        };
        if home {
            self.flag_home(t, NO_WINNER);
            return;
        }
        // It falls to the floor under them.
        let mut at = self.fighters[s].body.pos;
        at.y = self.map.floor(at.x, at.z, 0.2, at.y + 0.1);
        self.flags[t] = Flag {
            state: FLAG_DOWN,
            carrier: 0,
            pos: at,
            down_for: ticks(FLAG_RETURN_MS),
        };
        self.flag_event(1, s as u8, t, at);
    }

    /// Flag `t` goes home: returned by seat `by`, or by its clock
    /// ([`NO_WINNER`]).
    fn flag_home(&mut self, t: usize, by: u8) {
        let Some(bases) = self.map.flags else {
            return;
        };
        let at = self.flags[t].pos;
        self.flags[t] = Flag {
            pos: bases[t],
            ..Flag::default()
        };
        self.flag_event(2, by, t, at);
    }

    /// `part`: 0 taken, 1 dropped, 2 returned, 3 captured; `a` who, `b` the
    /// flag's side, at `pos`.
    fn flag_event(&mut self, part: u8, a: u8, t: usize, pos: V3) {
        self.events.push(Event {
            kind: Ev::Flag,
            a,
            b: t as u8,
            part,
            pos,
            ..Event::default()
        });
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

    /// USE on foot: the nearest pad holding its gun, or gun on the floor,
    /// in reach. What a hand lets go of for it falls at `s`'s feet.
    fn pickup(&mut self, s: usize) {
        if self.rules.gun_game {
            return;
        }
        let at = self.fighters[s].body.pos;
        // How far, a pad (or a drop), its index.
        let mut best: Option<(f32, bool, usize)> = None;
        for (i, def) in self.map.pads.iter().enumerate().take(MAX_PADS) {
            let far = (def.pos - at).flat().len();
            let near = on_pad(def.pos, at) && !crate::arms::power(def.arm);
            if near && self.pads[i].empty_for == 0 && best.is_none_or(|b| far < b.0) {
                best = Some((far, true, i));
            }
        }
        for (i, d) in self.drops.live() {
            let far = (d.pos - at).flat().len();
            if d.gun.arm != NONE && d.reach(at) && best.is_none_or(|b| far < b.0) {
                best = Some((far, false, i));
            }
        }
        let Some((_, pad, i)) = best else {
            return;
        };
        let (gun, pos) = if pad {
            let def = &self.map.pads[i];
            self.pads[i].empty_for = ticks(def.respawn_ms);
            (Gun::full(def.arm), def.pos)
        } else {
            let d = &mut self.drops[i];
            let gun = d.gun;
            d.gun = Gun::default();
            d.active = !d.spent();
            (gun, d.pos)
        };
        let gone = self.equip(s, gun, pad);
        let feet = self.fighters[s].body.pos;
        self.spill(gone[0], 0, 0, feet + V3::new(DROP_APART, 0.0, 0.0), V3::ZERO);
        self.spill(gone[1], 0, 0, feet, V3::ZERO);
        self.events.push(Event {
            kind: Ev::Pickup,
            a: s as u8,
            c: gun.arm as u16,
            pos,
            ..Event::default()
        });
    }

    /// `s` takes `g` by a pickup's rules, and gives back what its hands let
    /// go of for it: the left hand's gun, then the one in hand. A
    /// one-handed gun beside another goes in the left hand; a gun for two
    /// hands empties the left. A gun of a kind carried is `refill`ed (a
    /// pad's, full), or from the floor the fuller of the two is kept.
    fn equip(&mut self, s: usize, g: Gun, refill: bool) -> [Gun; 2] {
        let f = &mut self.fighters[s];
        let a = arm(g.arm);
        let swap = ticks(crate::arms::SWAP_MS) as u16;
        let mut gone = [Gun::default(); 2];
        if a.dual && arm(f.gun().arm).dual {
            gone[0] = f.left;
            f.left = g;
            f.left_reload = 0;
            f.left_charge = 0;
            f.left_refire = swap;
        } else {
            if f.dual() {
                gone[0] = f.left;
                f.drop_left();
            }
            let cur = f.cur as usize & 1;
            let rounds = |g: &Gun| g.mag as u32 + g.reserve as u32;
            if let Some(h) = f.guns.iter_mut().find(|h| h.arm == g.arm) {
                if refill || rounds(&g) > rounds(h) {
                    if !refill {
                        gone[1] = *h;
                    }
                    *h = g;
                } else {
                    gone[1] = g;
                }
            } else if f.guns[1].arm == NONE {
                f.guns[1] = g;
                f.cur = 1;
            } else {
                gone[1] = f.guns[cur];
                f.guns[cur] = g;
            }
        }
        f.reload = 0;
        f.burst_left = 0;
        f.refire = f.refire.max(swap);
        gone
    }

    /// A gun (with what's left in it) and grenades onto the floor under
    /// `at`. Nothing for nothing (an empty hand, a spent gun); with the
    /// floor full, the oldest drop gives its slot.
    fn spill(&mut self, gun: Gun, frags: u8, stickies: u8, at: V3, vel: V3) {
        let gun = if gun.arm == NONE || gun.spent() {
            Gun::default()
        } else {
            gun
        };
        let x = at.x.clamp(self.map.lo.0, self.map.hi.0);
        let z = at.z.clamp(self.map.lo.1, self.map.hi.1);
        let y = self.map.floor(x, z, 0.1, at.y + STEP);
        let d = Dropped {
            active: true,
            gun,
            frags,
            stickies,
            pos: V3::new(
                crate::movement::quant(x),
                crate::movement::quant(y),
                crate::movement::quant(z),
            ),
            vel,
            ttl: ticks(DROP_MS),
        };
        if d.spent() || self.drops.alloc(d).is_some() {
            return;
        }
        let mut old = 0;
        for i in 1..MAX_DROPS {
            if self.drops[i].ttl < self.drops[old].ttl {
                old = i;
            }
        }
        self.drops[old] = d;
    }

    /// Walking over the floor: a dropped gun's rounds into the reserve of
    /// the same kind of gun carried (a battery isn't poured: that's a
    /// swap), and grenades up to what may be carried.
    fn loot(&mut self, s: usize) {
        let f = &self.fighters[s];
        if !f.active() || f.riding().is_some() || self.rules.gun_game {
            return;
        }
        let at = f.body.pos;
        for i in 0..MAX_DROPS {
            let mut d = self.drops[i];
            if !d.active || !d.reach(at) {
                continue;
            }
            let f = &mut self.fighters[s];
            let mut took = false;
            let frags = d.frags.min(GREN_MAX.saturating_sub(f.grenades));
            let stickies = d.stickies.min(GREN_MAX.saturating_sub(f.stickies));
            if frags + stickies > 0 {
                f.grenades += frags;
                f.stickies += stickies;
                d.frags -= frags;
                d.stickies -= stickies;
                took = true;
            }
            let a = arm(d.gun.arm);
            if d.gun.arm != NONE && !a.hot() {
                for h in f.guns.iter_mut().chain(core::iter::once(&mut f.left)) {
                    if h.arm != d.gun.arm || h.reserve >= a.reserve {
                        continue;
                    }
                    let want = a.reserve - h.reserve;
                    let from_reserve = want.min(d.gun.reserve);
                    let from_mag = (want - from_reserve).min(d.gun.mag);
                    h.reserve += from_reserve + from_mag;
                    d.gun.reserve -= from_reserve;
                    d.gun.mag -= from_mag;
                    took |= from_reserve + from_mag > 0;
                }
            }
            if !took {
                continue;
            }
            if d.gun.arm != NONE && d.gun.spent() {
                d.gun = Gun::default();
            }
            d.active = !d.spent();
            self.drops[i] = d;
            self.events.push(Event {
                kind: Ev::Pickup,
                a: s as u8,
                c: d.gun.arm as u16,
                pos: d.pos,
                ..Event::default()
            });
        }
    }

    /// Walking onto a pad holding a power-up takes it: the overshield
    /// starts to climb (health whole), or the camouflage is on, full; or
    /// grenades, as many as fit.
    fn powers(&mut self, s: usize) {
        let f = &self.fighters[s];
        if !f.active() || f.riding().is_some() || self.rules.gun_game {
            return;
        }
        let at = f.body.pos;
        for i in 0..self.map.pads.len().min(MAX_PADS) {
            let def = self.map.pads[i];
            if !crate::arms::power(def.arm) || self.pads[i].empty_for != 0 || !on_pad(def.pos, at) {
                continue;
            }
            let f = &mut self.fighters[s];
            match def.arm {
                crate::arms::OVERSHIELD => {
                    f.charging = true;
                    f.health = HEALTH_MAX;
                }
                crate::arms::CAMO => {
                    f.camo = ticks(CAMO_MS) as u16;
                    f.flare = 0;
                }
                kind => {
                    // Left lying for whoever has room.
                    let have = if kind == crate::arms::FRAGS {
                        &mut f.grenades
                    } else {
                        &mut f.stickies
                    };
                    if *have >= GREN_MAX {
                        continue;
                    }
                    *have = (*have + GREN_PAD_GIVES).min(GREN_MAX);
                }
            }
            self.pads[i].empty_for = ticks(def.respawn_ms);
            self.events.push(Event {
                kind: Ev::Pickup,
                a: s as u8,
                c: def.arm as u16,
                pos: def.pos,
                ..Event::default()
            });
        }
    }

    /// `s` fired, threw or swung: its camouflage shows it a while.
    fn unveil(&mut self, s: usize) {
        let f = &mut self.fighters[s];
        if f.camo > 0 {
            f.flare = ticks(CAMO_FLARE_MS) as u16;
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
        // Shards not added to for a while fall out.
        if f.needles > 0 {
            f.needle_t = f.needle_t.saturating_add(1);
            if f.needle_t as u32 >= ticks(NEEDLE_KEEP_MS) {
                f.needles = 0;
                f.needle_t = 0;
            }
        }
        f.camo = f.camo.saturating_sub(1);
        f.flare = if f.camo == 0 { 0 } else { f.flare.saturating_sub(1) };
        // The overshield climbs whatever hits it, then drains to full.
        if f.charging {
            f.shield = (f.shield + OVERSHIELD_RATE * PT / TICK_HZ as i32).min(OVERSHIELD_MAX);
            f.charging = f.shield < OVERSHIELD_MAX;
        } else if f.shield > SHIELD_MAX {
            f.shield = (f.shield - OVERSHIELD_DRAIN * PT / TICK_HZ as i32).max(SHIELD_MAX);
        }
        if f.since_hit >= ticks(SHIELD_DELAY_MS) {
            if f.shield < SHIELD_MAX {
                f.shield = (f.shield + SHIELD_RATE * PT / TICK_HZ as i32).min(SHIELD_MAX);
            } else if f.health < HEALTH_MAX {
                f.health = (f.health + HEALTH_RATE * PT / TICK_HZ as i32).min(HEALTH_MAX);
            }
        }
    }

    /// Back at the spawn its side may use that is safest: farthest from the
    /// nearest living enemy (to [`SPAWN_FAR_M`]), out of every living
    /// enemy's sight, near a teammate, with a little chance
    /// (`SPAWN_JITTER_M`) between close calls.
    fn respawn(&mut self, s: usize) {
        let team = self.fighters[s].team;
        let teams = self.rules.teams;
        let mut pick = 0usize;
        let mut best = f32::NEG_INFINITY;
        for i in 0..self.map.spawns.len() {
            let sp = self.map.spawns[i];
            if teams && sp.team != team && sp.team != ANY_TEAM {
                continue;
            }
            let eye = sp.pos + V3::new(0.0, EYE, 0.0);
            let mut near = SPAWN_FAR_M;
            let mut seen = false;
            let mut mate = false;
            for (q, v) in self.fighters.iter().enumerate() {
                if q == s || !v.active() {
                    continue;
                }
                let far = (v.body.pos - sp.pos).len();
                if teams && v.team == team {
                    mate = mate || far < SPAWN_MATE_R;
                    continue;
                }
                near = near.min(far);
                seen = seen || self.map.clear(v.eye(), eye);
            }
            let luck = self.rng.next_bounded(1024) as f32 * (SPAWN_JITTER_M / 1024.0);
            let score = near - if seen { SPAWN_SEEN_M } else { 0.0 } + if mate { SPAWN_MATE_M } else { 0.0 } + luck;
            if score > best {
                best = score;
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
        // A practice map is for feeling them: all that can be carried.
        if self.map.practice {
            self.fighters[s].grenades = GREN_MAX;
            self.fighters[s].stickies = GREN_MAX;
        }
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
                thing::Kind::Drop => hash_all(&*self.drops, &mut h),
                // Only in its mode, so every other mode's golden stands.
                thing::Kind::Flag => {
                    if self.rules.ctf {
                        hash_all(&self.flags, &mut h)
                    }
                }
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Target {
    Fighter(usize, bool),
    Vehicle(usize),
}

/// A body at `at` touches a flag at `flag`.
fn touches(at: V3, flag: V3) -> bool {
    let d = at - flag;
    d.flat().len() < FLAG_REACH && crate::fmath::fabs(d.y) < FLAG_RISE
}

fn pose(f: &Fighter) -> Pose {
    Pose {
        pos: f.body.pos,
        duck: f.body.duck,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A hemisphere 2 m round at the origin, a flat floor round it.
    fn dome() -> Map {
        crate::map::Def {
            id: 250,
            practice: true,
            modes: 0,
            name: "dome",
            lo: (-10.0, -10.0),
            hi: (10.0, 10.0),
            void: f32::NEG_INFINITY,
            boxes: &[],
            looks: &[],
            spawns: &[([-8.0, 0.0, 0.0], 0, 0), ([8.0, 0.0, 0.0], 0, 1)],
            pads: &[],
            roam: &[],
            terrain: None,
            launchers: &[],
            vehicles: &[],
            solids: &[],
            solid_looks: &[],
            dress: &[],
            dress_solids: &[],
            profiles: &[(
                3,
                [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
                &[
                    [0.0, 0.0, 2.0],
                    [0.7, 0.0, 1.873],
                    [1.2, 0.0, 1.6],
                    [1.6, 0.0, 1.2],
                    [1.873, 0.0, 0.7],
                    [2.0, 0.0, 0.0],
                ],
                0,
            )],
            dress_profiles: &[],
            nav: false,
            light: crate::map::LightDef::DAY,
            flags: &[],
        }
        .build()
    }

    #[test]
    fn a_grenade_bounces_off_a_dome_along_its_normal() {
        let w = World::new(dome(), Rules::ffa(), 1, (0, 0));
        // Straight down onto its flank where it is 60° steep: off it,
        // outward.
        let until = |mut pos: V3, mut vel: V3| {
            for _ in 0..TICK_HZ {
                let (touched, ground) = w.bounce_at(&mut pos, &mut vel, 0.0, 0.5, 0.8);
                if touched {
                    return (pos, vel, ground);
                }
            }
            panic!("never met it: {pos:?}");
        };
        let (pos, vel, ground) = until(V3::new(1.75, 2.5, 0.0), V3::new(0.0, -10.0, 0.0));
        assert!(!ground && vel.x > 3.0 && vel.y > -8.0, "{pos:?} {vel:?}");
        assert!(!w.map.solids[0].contains(pos), "out of it: {pos:?}");
        // Down onto its crown: it lands, as on a floor, rolling outward
        // off the crown's slope.
        let (pos, vel, ground) = until(V3::new(0.1, 2.5, 0.0), V3::new(0.0, -6.0, 0.0));
        assert!(ground && vel.y > 0.0 && vel.x > 0.0 && vel.x < 2.0, "{pos:?} {vel:?}");
    }
}

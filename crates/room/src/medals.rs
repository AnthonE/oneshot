//! Medals (`proto::medal`): what each kill was worth past the score, read
//! from the world's events after every tick, as the aim review is
//! (`watch.rs`). Nothing here changes the match, and a replay re-run gives
//! the same medals: they are a function of the world's ticks.
//!
//! - **Multi-kills:** each kill within [`MULTI_MS`] of the one before
//!   lengthens the chain, from the grave too; a betrayal or a suicide
//!   neither counts nor breaks it.
//! - **Sprees:** enemy kills in a life, a rung every five.
//! - **A fight** is what a fighter took since its shields last began to
//!   recharge: who started it, who hurt it and how much, whose grenade.
//!   Assists ([`ASSIST_PTS`] or more in the victim's fight), Reversal,
//!   Protector and Distraction read it.
//! - **Perfect:** a precision gun's fewest trigger pulls, none missed,
//!   every round of the fight the killer's with that gun, from full.
//! - **How and when:** the cause on the kill (melee from the front or
//!   behind, a splatter, a grenade stuck or not, a sniper scoped or not),
//!   the round that made it (a headshot, the magazine's last), and what
//!   came before (the victim's spree, who last killed whom).

use proto::medal as md;
use sim::arms::{arm, Head, Kind as ArmKind, BEAM_RIFLE, NONE, SNIPER};
use sim::consts::{ticks, HEALTH_MAX, MAX_FIGHTERS, PT, SHIELD_DELAY_MS, SHIELD_MAX};
use sim::events::{Cause, Kind as Ev};
use sim::geom::V3;
use sim::input::ZOOM;
use sim::vehicle::PASSENGER;
use sim::world::{Phase, ProjKind, World};

/// The most between two kills of a chain.
pub const MULTI_MS: u32 = 4000;
/// Damage in the victim's fight that earns an assist, in points (of 200:
/// shields and health).
pub const ASSIST_PTS: u32 = 40;
/// An enemy who killed a teammate this recently: Avenger.
pub const AVENGE_MS: u32 = 4000;
/// An enemy who hurt a teammate this recently: Protector (and the
/// teammate's Distraction).
pub const PROTECT_MS: u32 = 2000;
/// A grenade kill from at least this far from the throw: Hail Mary.
pub const HAIL_MARY_M: f32 = 25.0;
/// Extermination needs an enemy side this big.
pub const EXTERMINATION_MIN: usize = 4;

/// A medal earned: by whom, which, and the kill it was for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Award {
    pub slot: u8,
    pub medal: u8,
    pub victim: u8,
}

/// One seat's carnage report, kept as the match runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tally {
    /// Enemies killed.
    pub kills: u16,
    pub deaths: u16,
    pub assists: u16,
    /// Teammates killed.
    pub betrayals: u16,
    pub suicides: u16,
    /// Points of damage done to enemies, and taken from them.
    pub damage: u32,
    pub taken: u32,
    pub best_spree: u16,
    pub best_multi: u8,
    /// Hitscan rounds fired (a pellet is a round), those that hit someone,
    /// and those on the head.
    pub shots: u32,
    pub hits: u32,
    pub heads: u32,
    /// Each medal's count, by id.
    pub medals: [u16; md::COUNT],
}

impl Default for Tally {
    fn default() -> Self {
        Tally {
            kills: 0,
            deaths: 0,
            assists: 0,
            betrayals: 0,
            suicides: 0,
            damage: 0,
            taken: 0,
            best_spree: 0,
            best_multi: 0,
            shots: 0,
            hits: 0,
            heads: 0,
            medals: [0; md::COUNT],
        }
    }
}

/// A fight from its victim's side: since its shields last began to
/// recharge.
#[derive(Clone, Copy, Debug)]
struct Fight {
    on: bool,
    /// Who hurt it first.
    first: u8,
    /// Each seat's damage in it, in points, and when it last hurt.
    pts: [u32; MAX_FIGHTERS],
    at: [u32; MAX_FIGHTERS],
    /// Seats whose grenade hurt it (a bit each).
    nades: u16,
    /// For a Perfect: the precision gun every round came from, the pulls
    /// that landed, when the current one's first round did, and the first
    /// one's pulls fired before the fight's first.
    arm: u8,
    pulls: u8,
    pull_at: u32,
    fired_from: u32,
    /// From full, and every round the first one's with `arm`.
    clean: bool,
}

impl Default for Fight {
    fn default() -> Self {
        Fight {
            on: false,
            first: 0,
            pts: [0; MAX_FIGHTERS],
            at: [0; MAX_FIGHTERS],
            nades: 0,
            arm: NONE,
            pulls: 0,
            pull_at: 0,
            fired_from: 0,
            clean: false,
        }
    }
}

impl Fight {
    /// `q` hurt it within the last `ms` before `now`.
    fn hurt_by(&self, q: usize, now: u32, ms: u32) -> bool {
        self.on && self.pts[q] > 0 && now.wrapping_sub(self.at[q]) <= ticks(ms)
    }
}

#[derive(Clone, Copy, Debug)]
struct Seat {
    dead: bool,
    /// When it last died: a kill the same tick is a trade, not from the
    /// grave.
    died_at: u32,
    spree: u16,
    chain: u8,
    last_kill: Option<u32>,
    /// Whom this seat last killed, and when.
    last_victim: Option<(u8, u32)>,
    /// Who last killed this seat, until it pays them back.
    nemesis: Option<u8>,
    fight: Fight,
    /// Trigger pulls fired with a hitscan gun, and when the last began.
    fired: u32,
    fired_at: u32,
    /// Where it last threw a grenade from.
    threw: V3,
    /// At full shields and health at the end of the last tick.
    full: bool,
    /// The seats whose sticky is stuck to this one (a bit each).
    stuck: u16,
    tally: Tally,
}

impl Default for Seat {
    fn default() -> Self {
        Seat {
            dead: true,
            died_at: 0,
            spree: 0,
            chain: 0,
            last_kill: None,
            last_victim: None,
            nemesis: None,
            fight: Fight::default(),
            fired: 0,
            fired_at: 0,
            threw: V3::ZERO,
            full: false,
            stuck: 0,
            tally: Tally::default(),
        }
    }
}

/// Medals for every seat, kept as the match runs.
#[derive(Clone, Debug)]
pub struct Medals {
    seats: [Seat; MAX_FIGHTERS],
    first_blood: bool,
    /// This tick's medals, in the order earned.
    awards: Vec<Award>,
}

impl Default for Medals {
    fn default() -> Self {
        Medals {
            seats: [Seat::default(); MAX_FIGHTERS],
            first_blood: false,
            awards: Vec::new(),
        }
    }
}

/// The fewest trigger pulls `id` kills in from full, every round landing
/// and the last pull's first in the head: a precision gun's (the battle
/// rifle's 4, the magnum's 5), None for others.
pub fn fewest_pulls(id: u8) -> Option<u8> {
    let a = arm(id);
    if a.kind != ArmKind::Hitscan || a.head != Head::Unshielded || a.pellets != 1 || a.dmg <= 0 {
        return None;
    }
    let on_shield = (a.dmg * PT) as f32 * a.shield_k;
    // Rounds that take the shields down, then one to the head.
    let strip = (SHIELD_MAX as f32 / on_shield).ceil() as u32;
    let rounds = strip + 1;
    let burst = a.burst.max(1) as u32;
    Some(rounds.div_ceil(burst) as u8)
}

/// Ticks one trigger pull of `id` spans: its burst's rounds land within it.
fn pull_span(id: u8) -> u32 {
    let a = arm(id);
    ticks(a.burst_gap_ms) * (a.burst.max(1) as u32 - 1)
}

impl Medals {
    /// A new occupant in seat `s`: a clean slate.
    pub fn reset(&mut self, s: usize) {
        self.seats[s] = Seat::default();
        for q in self.seats.iter_mut() {
            q.fight.pts[s] = 0;
            q.fight.nades &= !(1 << s);
            q.stuck &= !(1 << s);
            if q.nemesis == Some(s as u8) {
                q.nemesis = None;
            }
        }
    }

    pub fn tally(&self, s: usize) -> &Tally {
        &self.seats[s].tally
    }

    /// The medals of the tick `w` just stepped, in the order earned.
    pub fn observe(&mut self, w: &World) -> &[Award] {
        self.awards.clear();
        let now = w.tick;
        let enemies = |a: usize, b: usize| a != b && !(w.rules.teams && w.fighters[a].team == w.fighters[b].team);
        // Each seat's hitscan gun this tick (a hit right after its shot is
        // that gun's), and whose grenade's blast is being dealt out.
        let mut round: [Option<u8>; MAX_FIGHTERS] = [None; MAX_FIGHTERS];
        let mut blast: Option<u8> = None;
        // The winners' last kill this tick: the one that won it (a trade the
        // same tick doesn't take it away).
        let side = |k: usize| {
            if w.rules.teams {
                w.fighters[k].team as usize & 1
            } else {
                k
            }
        };
        let won = |k: usize| w.phase == Phase::Over && w.score[side(k)] >= w.rules.score_limit;
        let mut winning: Option<(usize, usize)> = None;
        let events = w.events.as_slice();
        for (i, e) in events.iter().enumerate() {
            let (a, b) = (e.a as usize, e.b as usize);
            if a >= MAX_FIGHTERS || b >= MAX_FIGHTERS {
                continue;
            }
            match e.kind {
                Ev::Shot => {
                    blast = None;
                    let id = e.c as u8;
                    let hitscan = arm(id).kind == ArmKind::Hitscan;
                    round[a] = hitscan.then_some(id);
                    let s = &mut self.seats[a];
                    if hitscan {
                        s.tally.shots += 1;
                        if s.fired == 0 || now.wrapping_sub(s.fired_at) > pull_span(id) {
                            s.fired += 1;
                            s.fired_at = now;
                        }
                    }
                }
                Ev::Blast => {
                    let nade = e.c == Cause::Grenade as u16 || e.c == Cause::Sticky as u16;
                    blast = nade.then_some(e.a);
                }
                Ev::Throw => self.seats[a].threw = e.pos,
                Ev::Spawn => {
                    let s = &mut self.seats[a];
                    s.dead = false;
                    s.fight = Fight::default();
                    s.stuck = 0;
                }
                Ev::Hit => {
                    let after_shot = i > 0 && events[i - 1].kind == Ev::Shot && events[i - 1].a == e.a;
                    let gun = if after_shot { round[a] } else { None };
                    let nade = blast == Some(e.a);
                    self.hit(a, b, gun, nade, e.part, e.c as u32, now, enemies(a, b));
                }
                Ev::Kill => {
                    // The round that made it: the last hit before (a rider
                    // gets out between), if it was this one's.
                    let head = events[..i]
                        .iter()
                        .rev()
                        .find(|h| h.kind == Ev::Hit)
                        .is_some_and(|h| h.a == e.a && h.b == e.b && h.part == 2);
                    if enemies(a, b) && won(a) {
                        winning = Some((a, b));
                    }
                    self.kill(w, a, b, e.c, head, now, &enemies);
                }
                _ => {}
            }
        }
        // The kill that won the match, and a flawless winner's.
        if let Some((k, v)) = winning {
            self.award(k, md::GAME_WINNER, v);
            let t = &self.seats[k].tally;
            if t.deaths == 0 && t.kills >= md::PERFECTION_KILLS {
                self.award(k, md::PERFECTION, v);
            }
        }
        // What the next tick needs of this one. A fight is over once the
        // victim's shields begin to recharge: nothing in it counts after.
        for (q, s) in self.seats.iter_mut().enumerate() {
            let f = &w.fighters[q];
            s.full = f.active() && f.shield >= SHIELD_MAX && f.health >= HEALTH_MAX;
            if !f.active() || f.since_hit >= ticks(SHIELD_DELAY_MS) {
                s.fight = Fight::default();
            }
            s.stuck = 0;
            // The world is the truth between ticks (a seat filled before
            // the first one spawned unseen).
            s.dead = !f.active();
        }
        for (_, p) in w.proj.live() {
            if p.kind == ProjKind::Sticky && p.landed && (p.target as usize) < MAX_FIGHTERS {
                self.seats[p.target as usize].stuck |= 1 << p.owner;
            }
        }
        &self.awards
    }

    fn award(&mut self, s: usize, medal: u8, victim: usize) {
        let t = &mut self.seats[s].tally;
        t.medals[medal as usize] = t.medals[medal as usize].saturating_add(1);
        self.awards.push(Award {
            slot: s as u8,
            medal,
            victim: victim as u8,
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn hit(&mut self, a: usize, b: usize, gun: Option<u8>, nade: bool, part: u8, pts: u32, now: u32, enemy: bool) {
        if !enemy {
            return;
        }
        let fired = self.seats[a].fired;
        {
            let t = &mut self.seats[a].tally;
            t.damage += pts;
            if gun.is_some() {
                t.hits += 1;
                t.heads += (part == 2 || part == 3) as u32;
            }
        }
        let v = &mut self.seats[b];
        v.tally.taken += pts;
        let precise = gun.filter(|&g| fewest_pulls(g).is_some());
        let f = &mut v.fight;
        if !f.on {
            *f = Fight {
                on: true,
                first: a as u8,
                arm: precise.unwrap_or(NONE),
                fired_from: fired.saturating_sub(1),
                clean: v.full && precise.is_some(),
                ..Fight::default()
            };
        }
        f.pts[a] += pts;
        f.at[a] = now;
        if nade {
            f.nades |= 1 << a;
        }
        if f.first != a as u8 || precise != Some(f.arm) {
            f.clean = false;
        }
        // Rounds of one burst land within its span: one pull.
        if f.pulls == 0 || now.wrapping_sub(f.pull_at) > pull_span(f.arm) {
            f.pulls = f.pulls.saturating_add(1);
            f.pull_at = now;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn kill(
        &mut self,
        w: &World,
        k: usize,
        v: usize,
        cause: u16,
        head: bool,
        now: u32,
        enemies: &impl Fn(usize, usize) -> bool,
    ) {
        // The victim's side of it, read before it is cleared.
        let fight = self.seats[v].fight;
        let victim_spree = self.seats[v].spree;
        let stuck = self.seats[v].stuck;
        {
            let s = &mut self.seats[v];
            s.dead = true;
            s.died_at = now;
            s.spree = 0;
            s.fight = Fight::default();
            s.tally.deaths += 1;
            if k != v {
                s.nemesis = Some(k as u8);
            }
        }
        if k == v {
            self.seats[k].tally.suicides += 1;
            return;
        }
        if !enemies(k, v) {
            self.seats[k].tally.betrayals += 1;
            return;
        }

        let ks = self.seats[k];
        // From the grave: died an earlier tick (the same tick is a trade).
        let grave = ks.dead && ks.died_at != now;
        let chain = match ks.last_kill {
            Some(t) if now.wrapping_sub(t) <= ticks(MULTI_MS) => ks.chain.saturating_add(1),
            _ => 1,
        };
        // A spree is a life's: a dead man's kills don't add to the next.
        let spree = if ks.dead { 0 } else { ks.spree + 1 };
        {
            let s = &mut self.seats[k];
            s.tally.kills += 1;
            s.tally.best_multi = s.tally.best_multi.max(chain);
            s.tally.best_spree = s.tally.best_spree.max(spree);
            s.chain = chain;
            s.last_kill = Some(now);
            s.last_victim = Some((v as u8, now));
            s.spree = spree;
        }

        // The ladders first: what the announcer says first.
        if let Some(m) = md::multi(chain) {
            self.award(k, m, v);
        }
        if let Some(m) = md::spree(spree) {
            self.award(k, m, v);
        }

        // How it was made.
        let id = cause as u8;
        let by_gun = cause < Cause::Grenade as u16;
        let killer = &w.fighters[k];
        let headshot = head && by_gun && arm(id).head != Head::None;
        if headshot {
            self.award(k, md::HEADSHOT, v);
        }
        let sniper = by_gun && (id == SNIPER || id == BEAM_RIFLE);
        if sniper && headshot {
            self.award(k, md::SNIPER_KILL, v);
        }
        if sniper && !grave && killer.last_buttons & ZOOM == 0 {
            self.award(k, md::NO_SCOPE, v);
        }
        let nade = cause == Cause::Grenade as u16 || cause == Cause::Sticky as u16;
        if cause == Cause::Melee as u16 {
            self.award(k, md::BEAT_DOWN, v);
        } else if cause == Cause::Behind as u16 {
            self.award(k, md::ASSASSINATION, v);
        } else if cause == Cause::Splatter as u16 {
            self.award(k, md::SPLATTER, v);
        } else if cause == Cause::Sticky as u16 && stuck & (1 << k) != 0 {
            self.award(k, md::STICK, v);
        } else if nade {
            self.award(k, md::GRENADE_KILL, v);
        }
        if nade && (ks.threw - w.fighters[v].body.pos).len() >= HAIL_MARY_M {
            self.award(k, md::HAIL_MARY, v);
        }
        if headshot && fight.nades & (1 << k) != 0 {
            self.award(k, md::NADE_SHOT, v);
        }
        // The magazine's last round (not a battery's).
        let emptied = |g: sim::fighter::Gun| g.arm == id && g.mag == 0;
        let last = emptied(*killer.gun()) || (killer.dual() && emptied(killer.left));
        if by_gun && !grave && !arm(id).hot() && last {
            self.award(k, md::LAST_SHOT, v);
        }
        let fired = self.seats[k].fired.wrapping_sub(fight.fired_from);
        let clean = fight.on && fight.clean && fight.first == k as u8 && fight.arm == id;
        if clean && fewest_pulls(id) == Some(fight.pulls) && fired == fight.pulls as u32 {
            self.award(k, md::PERFECT, v);
        }

        // When, and against whom.
        if grave {
            self.award(k, md::FROM_THE_GRAVE, v);
        }
        if !self.first_blood {
            self.first_blood = true;
            self.award(k, md::FIRST_STRIKE, v);
        }
        if victim_spree >= md::SPREE_STEP {
            self.award(k, md::KILLJOY, v);
        }
        if ks.nemesis == Some(v as u8) {
            self.seats[k].nemesis = None;
            self.award(k, md::REVENGE, v);
        }
        // The victim's own last kill, and whom else it was hurting.
        let vs = self.seats[v];
        let mate = |q: usize| q != k && q != v && !enemies(q, k) && w.fighters[q].seated;
        if vs
            .last_victim
            .is_some_and(|(q, t)| mate(q as usize) && now.wrapping_sub(t) <= ticks(AVENGE_MS))
        {
            self.award(k, md::AVENGER, v);
        }
        let mut protected = false;
        for q in 0..MAX_FIGHTERS {
            if mate(q) && w.fighters[q].alive && self.seats[q].fight.hurt_by(v, now, PROTECT_MS) {
                protected = true;
                self.award(q, md::DISTRACTION, v);
            }
        }
        if protected {
            self.award(k, md::PROTECTOR, v);
        }
        // Hurt first, killed anyway: the victim started the killer's fight,
        // and the killer's shields went down in it.
        let kf = self.seats[k].fight;
        if !grave && kf.on && kf.first == v as u8 && killer.shield < SHIELD_MAX / 2 {
            self.award(k, md::REVERSAL, v);
        }
        // The driver of the killer's ride.
        if let Some((i, seat)) = killer.riding() {
            if seat == PASSENGER {
                if let Some(d) = w.vehicles[i].driver().filter(|&d| d != k && !enemies(d, k)) {
                    self.award(d, md::WHEELMAN, v);
                }
            }
        }
        // The whole enemy side down at once.
        let side = (0..MAX_FIGHTERS).filter(|&q| w.fighters[q].seated && enemies(k, q));
        let (n, down) = side.fold((0, 0), |(n, d), q| (n + 1, d + self.seats[q].dead as usize));
        if w.rules.teams && n >= EXTERMINATION_MIN && down == n {
            self.award(k, md::EXTERMINATION, v);
        }

        // Others' real work on it.
        for q in 0..MAX_FIGHTERS {
            if q != k && enemies(q, v) && fight.on && fight.pts[q] >= ASSIST_PTS {
                self.seats[q].tally.assists += 1;
                self.award(q, md::ASSIST, v);
            }
        }
    }
}

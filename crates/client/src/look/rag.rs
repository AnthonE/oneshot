//! A body going limp: Triple T as a rag doll from the frame it dies.
//!
//! Eighteen points on its joints (the hips, the chest, the neck, the top of
//! the head, and the shoulder, elbow and wrist, the hip, knee, ankle and toe
//! of each side) under gravity, held together by sticks (Verlet, after
//! Jakobsen's *Advanced Character Physics*): rigid ones along the bones and
//! across the pelvis and the shoulders, soft ones between the two so the
//! back can give, a floor under every point (the map's ground, boxes and
//! solids, as the sim has them), and knees that bend only forward. Each
//! frame every bone of the rig is turned to lie along its points: the
//! pelvis, the chest and the head by the frames their points make, a limb
//! by the shortest turn from where it pointed to where its points are now,
//! carried down from the bone above it.
//!
//! It starts from the pose the body was drawn in, moving as the body was,
//! and pushed by how it died, as this client saw it ([`Blows`]): away from
//! whoever shot it (the head snapped back by a head shot), hard and up away
//! from a blast, shoved by a melee, thrown by a vehicle. A fall leaves it to
//! gravity.
//!
//! **Drawing only.** The dead have no hit volume and nothing here is read
//! back or sent, so a corpse that lies a little differently on two screens
//! decides nothing. A body first seen already dead (a corpse coming into
//! view) skips it and lies in `Death01`'s last pose (`anim.rs`).
//!
//! Our own body too: drawn hidden where we stand (`lib.rs`), it is shown
//! the frame we die and goes limp like anyone's; where its hips are is
//! [`OwnRag`], which the death cam watches.

use bevy::prelude::*;
use proto::EventState;
use sim::events::{Cause, Kind};
use sim::geom::V3;
use sim::Map;

use super::bodies::Body;
use crate::LinkRes;

/// The points, by index.
const HIPS: usize = 0;
const CHEST: usize = 1;
const NECK: usize = 2;
const TOP: usize = 3;
const L_ARM: usize = 4;
const L_ELBOW: usize = 5;
const L_WRIST: usize = 6;
const R_ARM: usize = 7;
const R_ELBOW: usize = 8;
const R_WRIST: usize = 9;
const L_HIP: usize = 10;
const L_KNEE: usize = 11;
const L_ANKLE: usize = 12;
const L_TOE: usize = 13;
const R_HIP: usize = 14;
const R_KNEE: usize = 15;
const R_ANKLE: usize = 16;
const R_TOE: usize = 17;
const POINTS: usize = 18;

/// The bone whose origin each point is.
const POINT_BONE: [&str; POINTS] = [
    "Hips",
    "Spine",
    "neck",
    "head_end",
    "LeftArm",
    "LeftForeArm",
    "LeftHand",
    "RightArm",
    "RightForeArm",
    "RightHand",
    "LeftUpLeg",
    "LeftLeg",
    "LeftFoot",
    "LeftToeBase",
    "RightUpLeg",
    "RightLeg",
    "RightFoot",
    "RightToeBase",
];

/// How big each point is against the floor and the walls, metres.
const RADIUS: [f32; POINTS] = [
    0.12, 0.13, 0.08, 0.12, 0.06, 0.05, 0.05, 0.06, 0.05, 0.05, 0.08, 0.06, 0.05, 0.03, 0.08, 0.06, 0.05, 0.03,
];

/// The sticks: two points, and how hard it holds its length, 0..1 per pass.
const STICKS: [(usize, usize, f32); 32] = [
    // The spine and the head.
    (HIPS, CHEST, 1.0),
    (CHEST, NECK, 1.0),
    (NECK, TOP, 1.0),
    // The pelvis and the shoulders, rigid.
    (HIPS, L_HIP, 1.0),
    (HIPS, R_HIP, 1.0),
    (L_HIP, R_HIP, 1.0),
    (CHEST, L_ARM, 1.0),
    (CHEST, R_ARM, 1.0),
    (L_ARM, R_ARM, 1.0),
    (NECK, L_ARM, 1.0),
    (NECK, R_ARM, 1.0),
    // Pelvis to chest, soft: the back gives a little, never folds.
    (L_HIP, L_ARM, 0.4),
    (R_HIP, R_ARM, 0.4),
    (L_HIP, R_ARM, 0.4),
    (R_HIP, L_ARM, 0.4),
    (L_HIP, CHEST, 0.4),
    (R_HIP, CHEST, 0.4),
    // The head on the shoulders: it lolls, it doesn't swing free.
    (L_ARM, TOP, 0.3),
    (R_ARM, TOP, 0.3),
    (CHEST, TOP, 0.3),
    // The limbs.
    (L_ARM, L_ELBOW, 1.0),
    (L_ELBOW, L_WRIST, 1.0),
    (R_ARM, R_ELBOW, 1.0),
    (R_ELBOW, R_WRIST, 1.0),
    (L_HIP, L_KNEE, 1.0),
    (L_KNEE, L_ANKLE, 1.0),
    (L_ANKLE, L_TOE, 1.0),
    (L_KNEE, L_TOE, 1.0),
    (R_HIP, R_KNEE, 1.0),
    (R_KNEE, R_ANKLE, 1.0),
    (R_ANKLE, R_TOE, 1.0),
    (R_KNEE, R_TOE, 1.0),
];

/// A joint that folds no further than this share of its two bones laid
/// straight: an elbow or a knee can't put the hand in the shoulder.
const FOLDS: [(usize, usize, f32); 4] = [
    (L_ARM, L_WRIST, 0.45),
    (R_ARM, R_WRIST, 0.45),
    (L_HIP, L_ANKLE, 0.5),
    (R_HIP, R_ANKLE, 0.5),
];

/// The knees: (hip, knee, ankle). A knee behind its leg's line is put in
/// front of it.
const KNEES: [(usize, usize, usize); 2] = [(L_HIP, L_KNEE, L_ANKLE), (R_HIP, R_KNEE, R_ANKLE)];

/// The bones turned, by name, and the one above each in this list (the
/// hips' is the rig's own parent, which nothing turns).
const BONES: [(&str, Option<usize>); 20] = [
    ("Hips", None),
    ("Spine02", Some(0)),
    ("Spine01", Some(1)),
    ("Spine", Some(2)),
    ("neck", Some(3)),
    ("Head", Some(4)),
    ("LeftShoulder", Some(3)),
    ("LeftArm", Some(6)),
    ("LeftForeArm", Some(7)),
    ("LeftHand", Some(8)),
    ("RightShoulder", Some(3)),
    ("RightArm", Some(10)),
    ("RightForeArm", Some(11)),
    ("RightHand", Some(12)),
    ("LeftUpLeg", Some(0)),
    ("LeftLeg", Some(14)),
    ("LeftFoot", Some(15)),
    ("RightUpLeg", Some(0)),
    ("RightLeg", Some(17)),
    ("RightFoot", Some(18)),
];

/// How each bone is turned: by a frame (the pelvis, the chest, the head,
/// or a share of the way between two), or along two points, carried from
/// another bone's turn, or as another bone is.
#[derive(Clone, Copy)]
enum Turn {
    Pelvis,
    Chest,
    Head,
    /// From the pelvis toward the chest, this share of the way.
    Back(f32),
    /// From the chest toward the head, this share of the way.
    Neck(f32),
    /// Along `from → to`, carried from bone `after`'s turn.
    Along {
        from: usize,
        to: usize,
        after: usize,
    },
    /// As bone `as_bone` turns.
    With(usize),
}

const TURNS: [Turn; 20] = [
    Turn::Pelvis,
    Turn::Back(1.0 / 3.0),
    Turn::Back(2.0 / 3.0),
    Turn::Chest,
    Turn::Neck(0.5),
    Turn::Head,
    Turn::With(3),
    Turn::Along {
        from: L_ARM,
        to: L_ELBOW,
        after: 3,
    },
    Turn::Along {
        from: L_ELBOW,
        to: L_WRIST,
        after: 7,
    },
    Turn::With(8),
    Turn::With(3),
    Turn::Along {
        from: R_ARM,
        to: R_ELBOW,
        after: 3,
    },
    Turn::Along {
        from: R_ELBOW,
        to: R_WRIST,
        after: 11,
    },
    Turn::With(12),
    Turn::Along {
        from: L_HIP,
        to: L_KNEE,
        after: 0,
    },
    Turn::Along {
        from: L_KNEE,
        to: L_ANKLE,
        after: 14,
    },
    Turn::Along {
        from: L_ANKLE,
        to: L_TOE,
        after: 15,
    },
    Turn::Along {
        from: R_HIP,
        to: R_KNEE,
        after: 0,
    },
    Turn::Along {
        from: R_KNEE,
        to: R_ANKLE,
        after: 17,
    },
    Turn::Along {
        from: R_ANKLE,
        to: R_TOE,
        after: 18,
    },
];

/// The step, seconds, and the most of them a frame takes (a long frame
/// runs slow rather than blowing up).
const STEP_S: f32 = 1.0 / 120.0;
const MAX_STEPS: u32 = 12;
/// Passes over the sticks a step.
const PASSES: usize = 8;
/// Velocity kept a step in the air, and along a floor while touching it.
const AIR_KEEP: f32 = 0.998;
const FLOOR_KEEP: f32 = 0.55;
/// Still for this long (no point moving faster than [`STILL_MPS`]), or
/// limp for [`LIMP_MAX_S`], it stops.
const STILL_S: f32 = 0.5;
const STILL_MPS: f32 = 0.05;
const LIMP_MAX_S: f32 = 8.0;

/// How long a blow is remembered for a body that hasn't been drawn dying
/// yet: the kill comes on the newest snapshot, the body's death a little
/// behind it.
const BLOW_KEEP_S: f32 = 2.0;
/// How hard each kind of death pushes, m/s at the chest: a round, a head
/// shot's extra on the head, a melee, a vehicle, and a blast at its middle
/// (falling off to nothing at [`BLAST_REACH_M`]) with its lift.
const SHOT_MPS: f32 = 3.2;
const HEAD_MPS: f32 = 2.5;
const MELEE_MPS: f32 = 3.5;
const SPLATTER_MPS: f32 = 8.0;
const SPLATTER_LIFT: f32 = 3.0;
const BLAST_MPS: f32 = 9.0;
const BLAST_LIFT: f32 = 5.0;
const BLAST_REACH_M: f32 = 8.0;
/// How much of a push the hips take against the chest's 1, and the legs
/// (a body shot topples over its feet rather than sliding off them).
const HIPS_SHARE: f32 = 0.5;
const LEGS_SHARE: f32 = 0.1;
/// The knees are given this much forward as the body goes, m/s, so the
/// legs fold under it rather than it toppling like a plank.
const BUCKLE_MPS: f32 = 0.5;
/// A blast's heard from this long ago is the one that killed.
const BLAST_KEEP_S: f32 = 1.0;
const BLASTS: usize = 8;

/// Where our own body lies, limp (its hips, world metres), as the last
/// step left it: what the death cam watches (`lib.rs`'s `view`). None while
/// we stand, or before the rag doll takes our body.
#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct OwnRag(pub Option<Vec3>);

/// What killed whom, as this client heard it, for the bodies that will be
/// seen falling.
#[derive(Resource, Default)]
pub struct Blows {
    by_slot: [Option<Blow>; sim::consts::MAX_FIGHTERS],
    blasts: [(Vec3, f32); BLASTS],
    next_blast: usize,
}

/// One death's push: a velocity at the chest, how much more the head
/// takes, and when it was heard.
#[derive(Clone, Copy, Debug, Default)]
pub struct Blow {
    pub chest: Vec3,
    pub head: Vec3,
    pub legs: Vec3,
    pub t: f32,
}

/// The kills and blasts of a frame's events into [`Blows`]. `me` is where
/// our own body stands, `others` everyone else as drawn.
pub fn on_events(blows: &mut Blows, events: &[EventState], others: &[client_net::core::Drawn], me: (u8, Vec3), t: f32) {
    let v = |p: V3| Vec3::new(p.x, p.y, p.z);
    let at = |slot: u8| {
        if slot == me.0 {
            Some(me.1)
        } else {
            others.iter().find(|d| d.slot == slot).map(|d| v(d.pos))
        }
    };
    for e in events {
        if e.kind == Kind::Blast as u8 {
            blows.blast(v(e.pos), t);
        } else if e.kind == Kind::Kill as u8 {
            let victim = if e.pos != V3::ZERO { Some(v(e.pos)) } else { at(e.b) };
            let from = if e.a != e.b { at(e.a) } else { None };
            match victim {
                Some(victim) => blows.kill(e.b, victim, from, e.c, e.part == 2, t),
                None => {
                    if let Some(b) = blows.by_slot.get_mut(e.b as usize) {
                        *b = None;
                    }
                }
            }
        }
    }
}

impl Blows {
    /// A blast at `at`, heard at `t`.
    pub fn blast(&mut self, at: Vec3, t: f32) {
        let i = self.next_blast % BLASTS;
        self.blasts[i] = (at, t);
        self.next_blast = self.next_blast.wrapping_add(1);
    }

    /// `victim`, standing at `at`, killed by `cause` from `from` (where the
    /// killer stood, if seen); `head`: a head shot.
    pub fn kill(&mut self, victim: u8, at: Vec3, from: Option<Vec3>, cause: u16, head: bool, t: f32) {
        let chest = at + Vec3::Y;
        let away = |o: Vec3| {
            let d = chest - o;
            let flat = Vec3::new(d.x, 0.0, d.z);
            let dir = if flat.length() > 0.05 {
                flat.normalize()
            } else {
                Vec3::ZERO
            };
            (dir, d.length())
        };
        let blasted = cause == Cause::Grenade as u16
            || cause == Cause::Sticky as u16
            || cause == Cause::Wreck as u16
            || ((cause as usize) < sim::arms::COUNT && sim::arms::arm(cause as u8).radius > 0.0);
        let (push, lift, extra) = if blasted {
            let near = self
                .blasts
                .iter()
                .filter(|(p, bt)| *bt > 0.0 && t - bt <= BLAST_KEEP_S && p.distance(chest) < BLAST_REACH_M)
                .min_by(|a, b| a.0.distance(chest).total_cmp(&b.0.distance(chest)));
            match near {
                Some(&(p, _)) => {
                    let (dir, dist) = away(p);
                    let k = (1.0 - dist / BLAST_REACH_M).clamp(0.2, 1.0);
                    (dir * BLAST_MPS * k, BLAST_LIFT * k, 0.0)
                }
                None => (Vec3::ZERO, BLAST_LIFT * 0.5, 0.0),
            }
        } else if cause == Cause::Fall as u16 {
            (Vec3::ZERO, 0.0, 0.0)
        } else {
            let dir = from.map_or(Vec3::ZERO, |o| away(o).0);
            if cause == Cause::Splatter as u16 {
                (dir * SPLATTER_MPS, SPLATTER_LIFT, 0.0)
            } else if cause == Cause::Melee as u16 || cause == Cause::Behind as u16 {
                (dir * MELEE_MPS, 0.0, 0.0)
            } else {
                (dir * SHOT_MPS, 0.0, if head { HEAD_MPS } else { 0.0 })
            }
        };
        let flat = Vec3::new(push.x, 0.0, push.z);
        let Some(slot) = self.by_slot.get_mut(victim as usize) else {
            return;
        };
        *slot = Some(Blow {
            chest: push + Vec3::Y * lift,
            head: push + Vec3::Y * lift + flat.normalize_or_zero() * extra,
            legs: flat * LEGS_SHARE + Vec3::Y * lift * LEGS_SHARE,
            t,
        });
    }
}

/// The entities of a body's points and turned bones, found by name when
/// its scene comes in (`bodies::dress`). None of them if a name is missing
/// or a bone isn't under the one this expects.
#[derive(Component, Clone, Debug)]
pub struct RagBones {
    points: [Entity; POINTS],
    bones: [Entity; 20],
    /// The hips' parent, which nothing animates.
    hips_up: Entity,
}

impl RagBones {
    /// The right hand's bone, which a gun is held in.
    pub fn right_hand(&self) -> Entity {
        self.bones[13]
    }
}

pub fn bind(
    model: Entity,
    names: &Query<&Name>,
    parents: &Query<&ChildOf>,
    children: &Query<&Children>,
) -> Option<RagBones> {
    let find = |n: &str| {
        children
            .iter_descendants(model)
            .take(512)
            .find(|&e| names.get(e).is_ok_and(|x| x.as_str() == n))
    };
    let mut points = [Entity::PLACEHOLDER; POINTS];
    for (i, n) in POINT_BONE.iter().enumerate() {
        points[i] = find(n)?;
    }
    let mut bones = [Entity::PLACEHOLDER; 20];
    for (i, (n, up)) in BONES.iter().enumerate() {
        bones[i] = find(n)?;
        if let Some(up) = up {
            if parents.get(bones[i]).map(|p| p.0) != Ok(bones[*up]) {
                return None;
            }
        }
    }
    let hips_up = parents.get(bones[0]).ok()?.0;
    Some(RagBones { points, bones, hips_up })
}

/// A body gone limp: its points, the pose it went limp from, and how long.
#[derive(Component)]
pub struct Ragdoll {
    now: [Vec3; POINTS],
    was: [Vec3; POINTS],
    /// The lengths the sticks and folds hold.
    sticks: [f32; STICKS.len()],
    folds: [f32; FOLDS.len()],
    /// Each bone's world turn when it went, and the frames and directions
    /// it is turned by.
    start: [Quat; 20],
    frames0: [Quat; 3],
    /// Which way the body faced, in the pelvis frame's terms.
    fwd: Vec3,
    along0: [Vec3; 20],
    /// The hips' parent in the world, and the body's root, both held where
    /// they were.
    hips_up: GlobalTransform,
    root: Transform,
    /// Seconds limp, still, and left over from the last frame's steps.
    age: f32,
    still: f32,
    carry: f32,
    asleep: bool,
}

/// An orthonormal turn with `aim` as its +Y and `side` (made square to it)
/// as its +Z.
fn frame(aim: Vec3, side: Vec3) -> Quat {
    let y = aim.normalize_or(Vec3::Y);
    let z = (side - y * side.dot(y)).normalize_or(Vec3::Z);
    let x = y.cross(z);
    Quat::from_mat3(&Mat3::from_cols(x, y, z)).normalize()
}

fn frames(p: &[Vec3; POINTS]) -> [Quat; 3] {
    let shoulders = p[L_ARM] - p[R_ARM];
    [
        frame(p[CHEST] - p[HIPS], p[L_HIP] - p[R_HIP]),
        frame(p[NECK] - p[CHEST], shoulders),
        frame(p[TOP] - p[NECK], shoulders),
    ]
}

impl Ragdoll {
    /// Limp from the pose last drawn (`globals`: last frame's world
    /// transforms), moving at `vel`, pushed by `blow`.
    fn new(
        bones: &RagBones,
        globals: &Query<&GlobalTransform>,
        root: Transform,
        vel: Vec3,
        blow: Option<Blow>,
    ) -> Option<Self> {
        let mut now = [Vec3::ZERO; POINTS];
        for (i, &e) in bones.points.iter().enumerate() {
            now[i] = globals.get(e).ok()?.translation();
        }
        let mut start = [Quat::IDENTITY; 20];
        for (i, &e) in bones.bones.iter().enumerate() {
            start[i] = globals.get(e).ok()?.to_scale_rotation_translation().1;
        }
        let hips_up = *globals.get(bones.hips_up).ok()?;
        Some(Self::limp(now, start, hips_up, root, vel, blow))
    }

    /// Limp from points `now` and bones turned `start` in the world.
    fn limp(
        now: [Vec3; POINTS],
        start: [Quat; 20],
        hips_up: GlobalTransform,
        root: Transform,
        vel: Vec3,
        blow: Option<Blow>,
    ) -> Self {
        let mut along0 = [Vec3::ZERO; 20];
        for (i, a) in along0.iter_mut().enumerate() {
            if let Turn::Along { from, to, .. } = TURNS[i] {
                *a = (now[to] - now[from]).normalize_or(Vec3::Y);
            }
        }
        let sticks = STICKS.map(|(a, b, _)| now[a].distance(now[b]));
        let folds = [0, 1, 2, 3].map(|k| {
            let (a, b, share) = FOLDS[k];
            let mid = match a {
                L_ARM => L_ELBOW,
                R_ARM => R_ELBOW,
                L_HIP => L_KNEE,
                _ => R_KNEE,
            };
            (now[a].distance(now[mid]) + now[mid].distance(now[b])) * share
        });
        let frames0 = frames(&now);
        // The way it faces (a body's +X), for which way the knees bend.
        let fwd = root.rotation * Vec3::X;
        let mut was = now;
        for i in 0..POINTS {
            let push = blow.map_or(Vec3::ZERO, |b| match i {
                TOP => b.head,
                HIPS | L_HIP | R_HIP => b.chest * HIPS_SHARE,
                L_KNEE | R_KNEE | L_ANKLE | R_ANKLE | L_TOE | R_TOE => b.legs,
                _ => b.chest,
            });
            let buckle = if i == L_KNEE || i == R_KNEE {
                fwd * BUCKLE_MPS
            } else {
                Vec3::ZERO
            };
            was[i] = now[i] - (vel + push + buckle) * STEP_S;
        }
        Self {
            now,
            was,
            sticks,
            folds,
            start,
            frames0,
            fwd: frames0[0].inverse() * fwd,
            along0,
            hips_up,
            root,
            age: 0.0,
            still: 0.0,
            carry: 0.0,
            asleep: false,
        }
    }

    /// One step: gravity and what it was moving at, then the sticks, the
    /// knees and the map, a few passes over.
    fn step(&mut self, map: &Map) {
        let g = Vec3::new(0.0, -sim::consts::GRAVITY, 0.0) * (STEP_S * STEP_S);
        let mut fastest = 0.0f32;
        for i in 0..POINTS {
            let v = (self.now[i] - self.was[i]) * AIR_KEEP;
            self.was[i] = self.now[i];
            self.now[i] += v + g;
            fastest = fastest.max(v.length() / STEP_S);
        }
        for _ in 0..PASSES {
            for (k, &(a, b, stiff)) in STICKS.iter().enumerate() {
                hold(&mut self.now, a, b, self.sticks[k], self.sticks[k], stiff);
            }
            for (k, &(a, b, _)) in FOLDS.iter().enumerate() {
                hold(&mut self.now, a, b, self.folds[k], f32::INFINITY, 1.0);
            }
            let fwd = frames(&self.now)[0] * self.fwd;
            for &(hip, knee, ankle) in &KNEES {
                let line = self.now[ankle] - self.now[hip];
                let l2 = line.length_squared();
                if l2 < 1e-6 {
                    continue;
                }
                let on = self.now[hip] + line * ((self.now[knee] - self.now[hip]).dot(line) / l2);
                let off = (self.now[knee] - on).dot(fwd);
                if off < 0.0 {
                    self.now[knee] -= fwd * (2.0 * off);
                }
            }
            for ((p, w), r) in self.now.iter_mut().zip(self.was.iter_mut()).zip(RADIUS) {
                collide(map, p, w, r);
            }
        }
        self.age += STEP_S;
        if fastest < STILL_MPS {
            self.still += STEP_S;
        } else {
            self.still = 0.0;
        }
        if self.still > STILL_S || self.age > LIMP_MAX_S {
            self.asleep = true;
        }
    }

    /// Every turned bone's local transform from the points.
    fn pose(&self, bones: &RagBones, at: &mut Query<&mut Transform>) {
        let f = frames(&self.now);
        let d: [Quat; 3] = [0, 1, 2].map(|k| (f[k] * self.frames0[k].inverse()).normalize());
        let mut turn = [Quat::IDENTITY; 20];
        let mut world = [Quat::IDENTITY; 20];
        for i in 0..20 {
            turn[i] = match TURNS[i] {
                Turn::Pelvis => d[0],
                Turn::Chest => d[1],
                Turn::Head => d[2],
                Turn::Back(s) => d[0].slerp(d[1], s),
                Turn::Neck(s) => d[1].slerp(d[2], s),
                Turn::With(b) => turn[b],
                Turn::Along { from, to, after } => {
                    let carried = turn[after] * self.along0[i];
                    let now = (self.now[to] - self.now[from]).normalize_or(carried);
                    Quat::from_rotation_arc(carried, now) * turn[after]
                }
            };
            world[i] = (turn[i] * self.start[i]).normalize();
            let up = match BONES[i].1 {
                Some(p) => world[p],
                None => self.hips_up.to_scale_rotation_translation().1,
            };
            if let Ok(mut t) = at.get_mut(bones.bones[i]) {
                t.rotation = (up.inverse() * world[i]).normalize();
                if i == 0 {
                    t.translation = self.hips_up.affine().inverse().transform_point3(self.now[HIPS]);
                }
            }
        }
    }
}

/// Moves `a` and `b` toward a distance in `lo..=hi`, `stiff` of the way.
fn hold(p: &mut [Vec3; POINTS], a: usize, b: usize, lo: f32, hi: f32, stiff: f32) {
    let d = p[b] - p[a];
    let l = d.length();
    if l < 1e-6 {
        return;
    }
    let want = l.clamp(lo, hi);
    if want == l {
        return;
    }
    let fix = d * ((l - want) / l * 0.5 * stiff);
    p[a] += fix;
    p[b] -= fix;
}

/// A point of radius `r` kept out of the map: on its floors (the ground, a
/// box's top, a solid's), out of its boxes' and solids' sides, and dragged
/// along a floor it touches.
fn collide(map: &Map, p: &mut Vec3, was: &mut Vec3, r: f32) {
    let mut floored = false;
    let floor = map.floor(p.x, p.z, 0.0, was.y.max(p.y) + 0.05);
    if p.y - r < floor {
        p.y = floor + r;
        floored = true;
    }
    let mut q = *p;
    map.each_box_in((q.x - r, q.z - r), (q.x + r, q.z + r), |b| {
        let out = [
            q.x - (b.min.x - r),
            (b.max.x + r) - q.x,
            q.y - (b.min.y - r),
            (b.max.y + r) - q.y,
            q.z - (b.min.z - r),
            (b.max.z + r) - q.z,
        ];
        if out.iter().any(|&o| o <= 0.0) {
            return;
        }
        let (k, _) = out
            .iter()
            .enumerate()
            .min_by(|a, b| a.1.total_cmp(b.1))
            .unwrap_or((3, &0.0));
        match k {
            0 => q.x = b.min.x - r,
            1 => q.x = b.max.x + r,
            2 => q.y = b.min.y - r,
            3 => {
                q.y = b.max.y + r;
                floored = true;
            }
            4 => q.z = b.min.z - r,
            _ => q.z = b.max.z + r,
        }
    });
    map.each_solid_in((q.x - r, q.z - r), (q.x + r, q.z + r), |s| {
        let top = s.top_at(q.x, q.z);
        if !(q.y - r < top && q.y + r > s.y0 && s.over(q.x, q.z, r)) {
            return;
        }
        if was.y - r >= top - 0.05 {
            q.y = top + r;
            floored = true;
        } else if let Some((x, z, _, _)) = s.push(q.x, q.z, r) {
            q.x = x;
            q.z = z;
        }
    });
    *p = q;
    if floored {
        // Along the floor it drags; into it, it stops.
        let v = *p - *was;
        was.x = p.x - v.x * FLOOR_KEEP;
        was.z = p.z - v.z * FLOOR_KEEP;
        if v.y < 0.0 {
            was.y = p.y;
        }
    }
}

/// Every body: one seen dying goes limp, one limp is stepped and posed,
/// one living or hidden again stands up (its clip from the top). In
/// `PostUpdate`, after the clips and the spine's turns and before the
/// transforms propagate.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn step(
    mut commands: Commands,
    link: NonSend<LinkRes>,
    time: Res<Time>,
    blows: Res<Blows>,
    mut bodies: Query<(Entity, &mut Body, Option<&RagBones>, Option<&mut Ragdoll>, &Visibility)>,
    globals: Query<&GlobalTransform>,
    mut at: Query<&mut Transform>,
    mut own: ResMut<OwnRag>,
) {
    let core = &link.0.session().core;
    let map = &core.map;
    let me = core.slot;
    own.0 = None;
    let dt = time.delta_secs();
    let t = time.elapsed_secs();
    for (e, mut body, bones, rag, vis) in &mut bodies {
        let Some(bones) = bones else {
            continue;
        };
        let shown = *vis != Visibility::Hidden;
        let (alive, was_alive) = body.life();
        match rag {
            Some(mut rag) => {
                if alive || !shown {
                    commands.entity(e).remove::<Ragdoll>();
                    body.afresh();
                    continue;
                }
                if let Ok(mut tr) = at.get_mut(e) {
                    tr.set_if_neq(rag.root);
                }
                if body.slot() == me {
                    own.0 = Some(rag.now[HIPS]);
                }
                if rag.asleep {
                    continue;
                }
                rag.carry = (rag.carry + dt).min(STEP_S * MAX_STEPS as f32);
                while rag.carry >= STEP_S {
                    rag.carry -= STEP_S;
                    rag.step(map);
                }
                rag.pose(bones, &mut at);
            }
            None => {
                if !(shown && was_alive == Some(true) && !alive && !body.is_shadow()) {
                    continue;
                }
                let blow =
                    blows.by_slot[body.slot() as usize % sim::consts::MAX_FIGHTERS].filter(|b| t - b.t <= BLOW_KEEP_S);
                let Ok(root) = at.get(e).copied() else {
                    continue;
                };
                // The root as it was last drawn: what last frame's world
                // transforms were made from.
                let root = globals.get(e).map(|g| g.compute_transform()).unwrap_or(root);
                let Some(rag) = Ragdoll::new(bones, &globals, root, body.last_vel(), blow) else {
                    continue;
                };
                if let Ok(mut tr) = at.get_mut(e) {
                    *tr = rag.root;
                }
                rag.pose(bones, &mut at);
                if body.slot() == me {
                    own.0 = Some(rag.now[HIPS]);
                }
                body.go_limp();
                commands.entity(e).insert(rag);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Triple T's joints standing at the origin facing +X, as its file has
    /// them (facing +Z there: turned by the body's quarter turn).
    fn standing() -> [Vec3; POINTS] {
        let file = [
            (0.005, 0.754, -0.044),
            (0.003, 1.113, -0.043),
            (0.002, 1.195, -0.042),
            (-0.006, 1.786, -0.044),
            (0.166, 1.205, -0.055),
            (0.329, 1.054, -0.084),
            (0.502, 0.903, -0.025),
            (-0.166, 1.205, -0.056),
            (-0.329, 1.054, -0.084),
            (-0.502, 0.903, -0.025),
            (0.083, 0.701, -0.038),
            (0.114, 0.489, -0.015),
            (0.133, 0.151, -0.039),
            (0.160, 0.039, 0.087),
            (-0.083, 0.701, -0.038),
            (-0.114, 0.489, -0.015),
            (-0.133, 0.151, -0.039),
            (-0.160, 0.039, 0.087),
        ];
        let face = Quat::from_rotation_y(core::f32::consts::FRAC_PI_2);
        file.map(|(x, y, z)| face * Vec3::new(x, y, z))
    }

    fn fall(blow: Option<Blow>, secs: f32) -> Ragdoll {
        let mut r = Ragdoll::limp(
            standing(),
            [Quat::IDENTITY; 20],
            GlobalTransform::IDENTITY,
            Transform::IDENTITY,
            Vec3::ZERO,
            blow,
        );
        let map = Map::open();
        let mut t = 0.0;
        while t < secs && !r.asleep {
            r.step(&map);
            t += STEP_S;
        }
        r
    }

    #[test]
    fn shot_from_the_front_it_falls_back_and_lies_still_on_the_floor() {
        // Shot by someone at +X: pushed toward −X.
        let push = Vec3::NEG_X * SHOT_MPS;
        let r = fall(
            Some(Blow {
                chest: push,
                head: push,
                legs: push * LEGS_SHARE,
                t: 0.0,
            }),
            LIMP_MAX_S,
        );
        assert!(r.asleep, "still moving after {:.1} s", r.age);
        for (i, p) in r.now.iter().enumerate() {
            assert!(p.is_finite(), "point {i} at {p}");
            assert!(p.y >= RADIUS[i] - 0.01, "point {i} under the floor at {p}");
        }
        assert!(r.now[TOP].y < 0.45, "the head is still up at {}", r.now[TOP].y);
        assert!(
            r.now[CHEST].x < -0.2,
            "it fell toward {}, not away from the shot",
            r.now[CHEST].x
        );
        for (k, &(a, b, stiff)) in STICKS.iter().enumerate() {
            if stiff == 1.0 {
                let l = r.now[a].distance(r.now[b]);
                assert!(
                    (l - r.sticks[k]).abs() < 0.05,
                    "stick {a}-{b} is {l:.3} of {:.3}",
                    r.sticks[k]
                );
            }
        }
    }

    #[test]
    fn with_nothing_pushing_it_folds_down_where_it_stood() {
        let r = fall(None, LIMP_MAX_S);
        assert!(r.asleep);
        assert!(r.now[TOP].y < 0.6, "the head is still up at {}", r.now[TOP].y);
        let moved = Vec3::new(r.now[HIPS].x, 0.0, r.now[HIPS].z).length();
        assert!(moved < 1.2, "it slid {moved:.2} m");
        // The knees went forward, never backward.
        for &(hip, knee, ankle) in &KNEES {
            let line = r.now[ankle] - r.now[hip];
            let on = r.now[hip] + line * ((r.now[knee] - r.now[hip]).dot(line) / line.length_squared());
            let fwd = frames(&r.now)[0] * r.fwd;
            assert!((r.now[knee] - on).dot(fwd) >= -0.02, "a knee bent backward");
        }
    }
}

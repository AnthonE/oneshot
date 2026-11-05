//! How a Triple T moves: after Gates' `crates/client/src/render/anim.rs`
//! (MIT, NOTICE), its locomotion trimmed to what a fighter does here.
//!
//! The clip a body plays is a function of what its snapshot says (alive,
//! grounded, crouched, at the wheel or riding) and of the velocity on the
//! wire, low-passed ([`Gait::observe`]). There are no strafe clips, so the
//! legs run the forward loop along the ground they cover: the hips turn
//! toward the travel (up to [`TWIST_MAX`]) and the spine turns back by as
//! much, so the chest and the head stay on the aim; travel behind the body
//! plays the loop reversed ([`BACK_BAND`] keeps a side-on strafe from
//! flipping). A standing body keeps its feet planted while its torso turns
//! and steps back round under it past [`IDLE_TWIST_MAX`]. Each loop plays at
//! the rate that keeps its feet planted at the body's speed, inside
//! [`RATE_MIN`]..[`RATE_MAX`], and a gait change starts the new loop on the
//! same foot as the old. The aim's pitch is shared down the spine, the head
//! taking the most of it ([`turn_spine`]); a crouch lifts the head by what
//! its clip bows it ([`CROUCH_HEAD_LIFT`]). A long gun or a pistol is held
//! on the arms and spine over whatever the legs do (a clip masked off the
//! hips and legs). Every change cross-fades over [`BLEND_S`].
//!
//! **Drawing only.** Nothing here is read back and no gameplay fact lives
//! in an `AnimationPlayer`: the hit volumes are the sim's own
//! (`sim::consts::HIT_*`, `HEAD_*`). It is the same on every target and
//! every rung (a skinned body is skinned on the vertex stage, which WebGL2
//! does as well as the desktop), so a body's pose never tells one player
//! more than another.
//!
//! Clips and bones are found by NAME, never by index (Gates' rule: a
//! re-export that adds one clip renumbers every one after it).

use core::f32::consts::{FRAC_PI_2, PI, TAU};
use std::time::Duration;

use bevy::animation::{AnimationTargetId, RepeatAnimation};
use bevy::gltf::{Gltf, GltfNode};
use bevy::prelude::*;

/// The clips a fighter's legs (or whole body) play.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Clip {
    Idle,
    Walk,
    Jog,
    Sprint,
    CrouchIdle,
    CrouchWalk,
    /// Off the ground: a jump, a fall, a launcher.
    Air,
    /// Killed: plays once and holds its last pose.
    Death,
    /// At the wheel.
    Drive,
}

impl Clip {
    pub const ALL: [Clip; 9] = [
        Clip::Idle,
        Clip::Walk,
        Clip::Jog,
        Clip::Sprint,
        Clip::CrouchIdle,
        Clip::CrouchWalk,
        Clip::Air,
        Clip::Death,
        Clip::Drive,
    ];

    /// The name in `triple-t.glb` (Gates' `stumpy.glb`).
    pub fn name(self) -> &'static str {
        match self {
            Clip::Idle => "Idle_Loop",
            Clip::Walk => "Walk_Loop",
            Clip::Jog => "Jog_Fwd_Loop",
            Clip::Sprint => "Sprint_Loop",
            Clip::CrouchIdle => "Crouch_Idle_Loop",
            Clip::CrouchWalk => "Crouch_Fwd_Loop",
            Clip::Air => "Jump_Loop",
            Clip::Death => "Death01",
            Clip::Drive => "Driving_Loop",
        }
    }

    /// The ground speed a locomotion loop's feet were authored for, m/s:
    /// Gates measured them off the same rig (the planted foot against the
    /// hips over the stance). `None` for what is not a locomotion loop.
    pub fn stride_mps(self) -> Option<f32> {
        match self {
            Clip::Walk => Some(1.05),
            Clip::Jog => Some(3.97),
            Clip::Sprint => Some(5.66),
            Clip::CrouchWalk => Some(0.5),
            _ => None,
        }
    }

    /// Where in each loop the left foot is furthest forward, as a share of
    /// the clip (Gates', measured with the strides): a gait change lines the
    /// incoming loop up on it, so the same foot stays in front.
    pub fn stride_mark(self) -> Option<f32> {
        match self {
            Clip::Walk => Some(0.25),
            Clip::Jog => Some(0.208),
            Clip::Sprint => Some(0.875),
            Clip::CrouchWalk => Some(0.125),
            _ => None,
        }
    }

    /// Plays once and holds its last frame.
    pub fn one_shot(self) -> bool {
        self == Clip::Death
    }

    fn slot(self) -> usize {
        match self {
            Clip::Idle => 0,
            Clip::Walk => 1,
            Clip::Jog => 2,
            Clip::Sprint => 3,
            Clip::CrouchIdle => 4,
            Clip::CrouchWalk => 5,
            Clip::Air => 6,
            Clip::Death => 7,
            Clip::Drive => 8,
        }
    }
}

/// How the arms hold what is in the hands: a long gun at the shoulder
/// (Gates' `RIFLE_HOLD_CLIP`) or a pistol up in front (`ARMS_HOLD_CLIP`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Hold {
    Long,
    Pistol,
}

impl Hold {
    pub const ALL: [Hold; 2] = [Hold::Long, Hold::Pistol];

    pub fn name(self) -> &'static str {
        match self {
            Hold::Long => "Rifle_Hold_Loop",
            Hold::Pistol => "Pistol_Idle_Loop",
        }
    }

    fn slot(self) -> usize {
        match self {
            Hold::Long => 0,
            Hold::Pistol => 1,
        }
    }
}

/// Gait thresholds, m/s, each with a dead band ([`SPEED_HYSTERESIS`]) a
/// body must cross to change its mind: the velocity on the wire steps at
/// the snapshot rate, and a bare `>` at a boundary flickers between clips.
/// The walk is for the first steps off a standstill; the jog sits between
/// the walk's and the jog's authored strides; the sprint (which is how
/// every fighter here runs, `sim::consts::RUN`) between a crouch's run and
/// a full one.
pub const WALK_MPS: f32 = 0.45;
pub const JOG_MPS: f32 = 2.5;
pub const SPRINT_MPS: f32 = (sim::consts::CROUCH_RUN + sim::consts::RUN) * 0.5;
pub const SPEED_HYSTERESIS: f32 = 0.35;

/// How fast the drawn speed follows the wire's, 1/s (Gates' one-pole).
pub const SPEED_EASE: f32 = 12.0;

/// The slowest and fastest a locomotion loop is played, as a multiple of
/// its authored rate. Inside the band the feet stay planted; past it they
/// slide, which beats a slow-motion jog or a crouch scuttling at four steps
/// a second.
pub const RATE_MIN: f32 = 0.5;
pub const RATE_MAX: f32 = 3.0;

/// The furthest the legs turn away from where the body faces, radians
/// (~69°): past it the twist reads as broken rather than athletic, and the
/// feet slide instead.
pub const TWIST_MAX: f32 = 1.2;
/// How far past side-on the travel must swing before the legs flip between
/// running forward and backpedalling, radians each way of 90°.
pub const BACK_BAND: f32 = 0.26;
/// How fast the legs swing round to a new travel direction, 1/s.
pub const TWIST_EASE: f32 = 10.0;
/// A standing body's feet stay planted while its torso turns this far,
/// radians (~46°); past it they step back round under it.
pub const IDLE_TWIST_MAX: f32 = 0.8;
/// How fast they step round once they go, rad/s.
pub const IDLE_TURN_RPS: f32 = 5.0;

/// How far a crouch clip's head is lifted back toward level, radians: the
/// difference between `Crouch_Idle_Loop`'s gaze and `Idle_Loop`'s on this
/// rig (Gates measured −49.6° against −7.9°).
pub const CROUCH_HEAD_LIFT: f32 = 0.73;

/// How long a clip change takes to cross-fade, seconds (Gates'
/// `ANIM_BLEND_S`): a walk to a run is not a snap, nor a slide.
pub const BLEND_S: f32 = 0.18;

/// How far a body may look from level, radians (~74°), against a wire that
/// carries 89°: a clamp, not a scale, so a glance is never understated.
pub const LOOK_PITCH_MAX: f32 = 1.3;

/// How the look is shared between the bones that take it, by weight: each
/// spine bone 1, the neck 1.5, the head 3, normalised over the bones the
/// rig has.
const LOOK_SPINE_W: f32 = 1.0;
const LOOK_NECK_W: f32 = 1.5;
const LOOK_HEAD_W: f32 = 3.0;

/// The most of the arms and spine a hold is given, short of 1 so its
/// weight below stays finite.
const HOLD_MAX: f32 = 0.995;

/// The model's right, in its own frame. Triple T faces +Z in its file (the
/// scene root turns it onto the body's +X, `bodies.rs`), so its right is −X;
/// a look up is a turn about it.
const BODY_RIGHT: Vec3 = Vec3::NEG_X;

/// The bones the look and the twist turn, by name.
pub const HIPS_BONE: &str = "Hips";
pub const HEAD_BONE: &str = "Head";

/// The mask group the hips and legs belong to, and the mask that keeps a
/// hold off them.
const LEGS_GROUP: u32 = 0;
const LEGS_MASK: u64 = 1 << LEGS_GROUP;

/// What a body's snapshot says this frame, as the gait reads it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Seen {
    pub alive: bool,
    pub grounded: bool,
    pub crouched: bool,
    /// At the wheel.
    pub driving: bool,
    /// Riding (a passenger, or at the wheel): the velocity is the hull's,
    /// and the legs don't walk it.
    pub riding: bool,
    /// Ground velocity, world x and z, m/s.
    pub vel: Vec2,
    /// Which way the body faces, radians: the wire's yaw (0 along +X,
    /// growing toward +Z).
    pub yaw: f32,
    /// Where it looks, radians from level, up positive.
    pub pitch: f32,
    /// What its arms hold.
    pub hold: Option<Hold>,
}

/// One body's locomotion: what its legs play, how they are turned, where
/// its head looks. Render state only: it lives on the body and never
/// travels.
///
/// The angles are Gates': 0 along the model's forward, growing toward its
/// left-hand +X, which is the wire's yaw negated ([`Seen::yaw`]).
#[derive(Clone, Copy, Debug, Default)]
pub struct Gait {
    pub clip: Option<Clip>,
    /// Ground speed, m/s, low-passed.
    pub speed: f32,
    /// Ground velocity in Gates' frame, low-passed with the speed.
    pub vel: Vec2,
    /// Which way the body faces.
    pub facing: f32,
    last_facing: Option<f32>,
    /// Where it looks, radians up, as last seen.
    pub pitch: f32,
    /// How far the legs are turned from the facing, radians about up.
    pub hip_yaw: f32,
    /// Backpedalling: the loop plays reversed.
    pub backward: bool,
    /// Standing, the planted feet are stepping back round under it.
    pub turning: bool,
    /// What the arms hold, if the body is shouldering anything.
    pub hold: Option<Hold>,
}

impl Gait {
    /// Fold this frame's snapshot in and choose a gait. `dt` is the frame's.
    pub fn observe(&mut self, s: &Seen, dt: f32) {
        // Into Gates' frame: the wire's yaw negated, and the velocity turned
        // with it (`atan2(x, z)` there is minus `atan2(z, x)` here).
        let raw = if s.riding {
            Vec2::ZERO
        } else {
            Vec2::new(-s.vel.y, s.vel.x)
        };
        if dt > 0.0 {
            let k = 1.0 - (-SPEED_EASE * dt).exp();
            self.speed += (raw.length() - self.speed) * k;
            self.vel += (raw - self.vel) * k;
        }
        self.facing = wrap_pi(-s.yaw);
        let turned = self.last_facing.map_or(0.0, |f| wrap_pi(self.facing - f));
        self.last_facing = Some(self.facing);
        self.pitch = s.pitch;
        self.clip = Some(self.gait(s));
        self.hold = s.hold.filter(|_| s.alive && !s.driving);
        self.steer(turned, dt);
    }

    /// The gait for this frame, off the flags and the speed.
    fn gait(&self, s: &Seen) -> Clip {
        if !s.alive {
            return Clip::Death;
        }
        if s.driving {
            return Clip::Drive;
        }
        let h = SPEED_HYSTERESIS;
        let now = self.clip.unwrap_or(Clip::Idle);
        let rank = |c: Clip| match c {
            Clip::Sprint => 3,
            Clip::Jog => 2,
            Clip::Walk | Clip::CrouchWalk => 1,
            _ => 0,
        };
        let up = |t: f32| self.speed > t + h;
        let down = |t: f32| self.speed < t - h;
        let moving = up(WALK_MPS) || (rank(now) >= 1 && !down(WALK_MPS));
        // A passenger stands on the hull: its feet don't run its speed.
        if s.riding {
            return Clip::Idle;
        }
        // In the air the legs are not walking, whatever the speed says; the
        // speed keeps following, so the landing picks the right gait.
        if !s.grounded {
            return Clip::Air;
        }
        if s.crouched {
            return if moving { Clip::CrouchWalk } else { Clip::CrouchIdle };
        }
        if up(SPRINT_MPS) || (rank(now) >= 3 && !down(SPRINT_MPS)) {
            Clip::Sprint
        } else if up(JOG_MPS) || (rank(now) >= 2 && !down(JOG_MPS)) {
            Clip::Jog
        } else if moving {
            Clip::Walk
        } else {
            Clip::Idle
        }
    }

    /// Turn the legs: toward the travel while a locomotion loop runs, held
    /// planted while the body stands, and back under the body otherwise.
    /// `turned` is how far the facing moved since last frame.
    fn steer(&mut self, turned: f32, dt: f32) {
        let ease = 1.0 - (-TWIST_EASE * dt).exp();
        match self.clip {
            Some(c) if c.stride_mps().is_some() => {
                self.turning = false;
                // The travel's bearing against the facing, zero straight
                // ahead.
                let rel = wrap_pi(self.vel.x.atan2(self.vel.y) - self.facing);
                self.backward = if self.backward {
                    rel.abs() > FRAC_PI_2 - BACK_BAND
                } else {
                    rel.abs() > FRAC_PI_2 + BACK_BAND
                };
                // Backpedalling turns the legs toward the opposite of the
                // travel, so the reversed loop walks them along it.
                let want = if self.backward { wrap_pi(rel - PI) } else { rel };
                let want = want.clamp(-TWIST_MAX, TWIST_MAX);
                self.hip_yaw += (want - self.hip_yaw) * ease;
            }
            Some(Clip::Idle | Clip::CrouchIdle) => {
                self.backward = false;
                // Planted: the feet keep their heading while the body turns,
                // so the twist takes up the turn...
                self.hip_yaw = wrap_pi(self.hip_yaw - turned).clamp(-TWIST_MAX, TWIST_MAX);
                // ...until it is too far, and they step back round.
                if self.hip_yaw.abs() > IDLE_TWIST_MAX {
                    self.turning = true;
                }
                if self.turning {
                    let step = IDLE_TURN_RPS * dt;
                    self.hip_yaw -= self.hip_yaw.clamp(-step, step);
                    self.turning = self.hip_yaw.abs() > 0.02;
                }
            }
            _ => {
                self.backward = false;
                self.turning = false;
                self.hip_yaw -= self.hip_yaw * ease;
            }
        }
    }

    /// The loop's playback rate: the body's speed over the speed the loop
    /// was authored for, so the feet stay planted, negative backpedalling.
    /// 1 for anything that is not a locomotion loop.
    pub fn rate(&self) -> f32 {
        let Some(stride) = self.clip.and_then(Clip::stride_mps) else {
            return 1.0;
        };
        let r = (self.speed / stride).clamp(RATE_MIN, RATE_MAX);
        if self.backward {
            -r
        } else {
            r
        }
    }

    /// The hold the arms play over the legs: none at the wheel or dead.
    pub fn holding(&self) -> Option<Hold> {
        match self.clip {
            Some(Clip::Death | Clip::Drive) | None => None,
            _ => self.hold,
        }
    }
}

/// An angle wrapped into `(-π, π]`.
pub fn wrap_pi(a: f32) -> f32 {
    let w = (a + PI).rem_euclid(TAU) - PI;
    if w <= -PI {
        w + TAU
    } else {
        w
    }
}

/// The rig's graph, shared by every body: one node per [`Clip`], one per
/// [`Hold`] masked off the legs, and each clip's length.
#[derive(Resource)]
pub struct Rig {
    pub graph: Handle<AnimationGraph>,
    nodes: [AnimationNodeIndex; Clip::ALL.len()],
    durations: [f32; Clip::ALL.len()],
    holds: [Option<AnimationNodeIndex>; Hold::ALL.len()],
    hold_durations: [f32; Hold::ALL.len()],
}

impl Rig {
    fn node(&self, c: Clip) -> AnimationNodeIndex {
        self.nodes[c.slot()]
    }
    fn duration(&self, c: Clip) -> f32 {
        self.durations[c.slot()].max(1e-3)
    }
}

/// Builds the graph once the model's clips and its node tree are in.
pub fn build(
    mut commands: Commands,
    models: Option<Res<super::guns::Models>>,
    have: Option<Res<Rig>>,
    gltfs: Res<Assets<Gltf>>,
    clips: Res<Assets<AnimationClip>>,
    gltf_nodes: Res<Assets<GltfNode>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
) {
    let (Some(models), None) = (models, have) else {
        return;
    };
    let Some(gltf) = gltfs.get(&models.body) else {
        return;
    };
    // The clips' lengths and the node tree are sub-assets and can land a
    // frame after the file: wait for both rather than build a graph that
    // can't phase-sync or mask.
    let mut durations = [1.0; Clip::ALL.len()];
    for clip in Clip::ALL {
        if let Some(h) = gltf.named_animations.get(clip.name()) {
            let Some(c) = clips.get(h) else {
                return;
            };
            durations[clip.slot()] = c.duration();
        }
    }
    let mut hold_durations = [1.0; Hold::ALL.len()];
    for hold in Hold::ALL {
        if let Some(h) = gltf.named_animations.get(hold.name()) {
            let Some(c) = clips.get(h) else {
                return;
            };
            hold_durations[hold.slot()] = c.duration();
        }
    }
    let Some(legs) = leg_targets(gltf, &gltf_nodes) else {
        return;
    };
    let mut graph = AnimationGraph::new();
    let root = graph.root;
    let mut nodes = [AnimationNodeIndex::default(); Clip::ALL.len()];
    let mut missing = Vec::new();
    for clip in Clip::ALL {
        match gltf.named_animations.get(clip.name()) {
            Some(h) => nodes[clip.slot()] = graph.add_clip(h.clone(), 1.0, root),
            None => missing.push(clip.name()),
        }
    }
    let mut holds = [None; Hold::ALL.len()];
    for hold in Hold::ALL {
        match gltf.named_animations.get(hold.name()) {
            Some(h) => holds[hold.slot()] = Some(graph.add_clip_with_mask(h.clone(), LEGS_MASK, 1.0, root)),
            None => missing.push(hold.name()),
        }
    }
    if legs.is_empty() {
        error!("anim: no {HIPS_BONE:?} bone under the rig's animation root: a hold will move the legs too");
    }
    for target in legs {
        graph.add_target_to_mask_group(target, LEGS_GROUP);
    }
    if !missing.is_empty() {
        // An unmatched name plays the graph's root, which is nothing: a body
        // frozen in its bind pose. Said once, loudly.
        error!("anim: clip(s) missing from models/triple-t.glb: {missing:?}");
    }
    commands.insert_resource(Rig {
        graph: graphs.add(graph),
        nodes,
        durations,
        holds,
        hold_durations,
    });
}

/// The animation targets of the hips and legs: the hips and every bone
/// under them except the branch that leads to the head (Gates'
/// `leg_targets`: structural, so a renamed toe stays in the legs). `None`
/// while the node sub-assets are still landing.
fn leg_targets(gltf: &Gltf, nodes: &Assets<GltfNode>) -> Option<Vec<AnimationTargetId>> {
    if gltf.nodes.iter().any(|h| !nodes.contains(h)) {
        return None;
    }
    let Some(root) = gltf
        .nodes
        .iter()
        .filter_map(|h| nodes.get(h))
        .find(|n| n.is_animation_root)
    else {
        return Some(Vec::new());
    };
    fn holds_head(n: &GltfNode, nodes: &Assets<GltfNode>, depth: u32) -> bool {
        n.name == HEAD_BONE
            || (depth < 32
                && n.children
                    .iter()
                    .filter_map(|c| nodes.get(c))
                    .any(|c| holds_head(c, nodes, depth + 1)))
    }
    let mut out = Vec::new();
    // (node, its name path from the animation root, inside the legs)
    let mut stack = vec![(root, vec![Name::new(root.name.clone())], false)];
    while let Some((n, path, legs)) = stack.pop() {
        if out.len() > 256 || path.len() > 32 {
            break;
        }
        let legs = legs || n.name == HIPS_BONE;
        if legs {
            out.push(AnimationTargetId::from_names(path.iter()));
        }
        for c in n.children.iter().filter_map(|c| nodes.get(c)) {
            // The spine is the hips' one child that leads to the head.
            if legs && holds_head(c, nodes, 0) {
                continue;
            }
            let mut p = path.clone();
            p.push(Name::new(c.name.clone()));
            stack.push((c, p, legs));
        }
    }
    Some(out)
}

/// What a body's player is playing, so [`drive`] writes on a transition and
/// not every frame (assigning every frame restarts the clip: a body frozen
/// in its first pose).
#[derive(Clone, Copy, Debug, Default)]
pub struct Playing {
    base: Option<Clip>,
    /// The loop fading out under `base`, kept stepping in time with it.
    fading: Option<Clip>,
    /// How much of the arms and spine each hold owns, 0..1.
    hold_u: [f32; Hold::ALL.len()],
}

/// A stable phase in `[0, 1)` per body, so a crowd standing still doesn't
/// breathe in unison.
pub fn desync(e: Entity) -> f32 {
    let h = e.to_bits().wrapping_mul(0x9E37_79B9_7F4A_7C15);
    (h >> 40) as f32 / (1u64 << 24) as f32
}

/// Cross-fade a body's player to what its gait wants, play its loop at the
/// rate that plants its feet, and lay its hold over the arms. `fresh`: the
/// body was hidden and is shown again, so it snaps rather than fades.
pub fn drive(
    rig: &Rig,
    gait: &Gait,
    playing: &mut Playing,
    player: &mut AnimationPlayer,
    transitions: &mut AnimationTransitions,
    (dt, fresh, phase0): (f32, bool, f32),
) {
    let Some(want) = gait.clip else {
        return;
    };
    if fresh {
        playing.base = None;
    }
    if playing.base != Some(want) {
        // Where the outgoing loop is in its stride, so the incoming one
        // starts on the same foot.
        let phase = playing.base.and_then(|from| {
            let mark = from.stride_mark()?;
            let t = player.animation(rig.node(from))?.seek_time();
            Some(t / rig.duration(from) - mark)
        });
        let first = playing.base.is_none();
        playing.fading = playing.base.filter(|c| c.stride_mark().is_some());
        playing.base = Some(want);
        let fade = if fresh {
            Duration::ZERO
        } else {
            Duration::from_secs_f32(BLEND_S)
        };
        let d = rig.duration(want);
        let active = transitions.play(player, rig.node(want), fade);
        if want.one_shot() {
            active.set_repeat(RepeatAnimation::Never);
            // Shown again already dead (a kill cam's victim): down already,
            // not falling again.
            if fresh {
                active.set_seek_time(d);
            }
        } else {
            active.repeat();
            if let Some(mark) = want.stride_mark() {
                let p = phase.unwrap_or(phase0);
                active.set_seek_time((p + mark).rem_euclid(1.0) * d);
            } else if first {
                active.set_seek_time(phase0 * d);
            }
        }
    }
    // The loop's rate, every frame: the speed changes under a loop that
    // doesn't, and backpedalling is the same loop reversed.
    if want.stride_mps().is_some() {
        let rate = gait.rate();
        if let Some(a) = player.animation_mut(rig.node(want)) {
            a.set_speed(rate);
        }
        // The loop it fades from steps at the same pace, so the two never
        // disagree about which foot is down.
        if let Some(from) = playing.fading {
            let pace = rate / rig.duration(want) * rig.duration(from);
            match player.animation_mut(rig.node(from)) {
                Some(a) if from != want => {
                    a.set_speed(pace);
                }
                _ => playing.fading = None,
            }
        }
    } else {
        playing.fading = None;
    }
    // The holds, faded in and out at the blend's pace.
    let held = gait.holding();
    for hold in Hold::ALL {
        let Some(node) = rig.holds[hold.slot()] else {
            continue;
        };
        let u = &mut playing.hold_u[hold.slot()];
        let on = held == Some(hold);
        *u = match (on, fresh) {
            (true, true) => 1.0,
            (false, true) => 0.0,
            (true, false) => (*u + dt / BLEND_S).min(1.0),
            (false, false) => (*u - dt / BLEND_S).max(0.0),
        };
        if *u <= 0.0 {
            if player.is_playing_animation(node) {
                player.stop(node);
            }
            continue;
        }
        if !player.is_playing_animation(node) {
            player
                .start(node)
                .repeat()
                .set_seek_time(phase0 * rig.hold_durations[hold.slot()].max(1e-3));
        }
        if let Some(a) = player.animation_mut(node) {
            // The gait's weights sum to 1 (the transitions hold them there),
            // so u/(1−u) hands the hold exactly u of every upper bone.
            let w = u.min(HOLD_MAX);
            a.set_weight(w / (1.0 - w));
        }
    }
}

/// One bone [`turn_spine`] turns, and what it last wrote.
#[derive(Clone, Copy, Debug)]
struct SpineBone {
    entity: Entity,
    /// Its share of the leg twist: +1 on the hips, minus a share of it on
    /// each spine bone so the chest faces the aim again.
    yaw: f32,
    /// Its share of the look.
    pitch: f32,
    /// The head: the crouch's lift, and the shadow's push, go here alone.
    head: bool,
    /// The local turn this last composed in, to take back out on a frame the
    /// animation did not rewrite the bone.
    applied: Quat,
    written: Quat,
    /// The same for the head's push (`bodies::Body::head_push`).
    moved: Vec3,
    placed: Vec3,
}

impl SpineBone {
    fn new(entity: Entity, yaw: f32, pitch: f32, head: bool) -> Self {
        Self {
            entity,
            yaw,
            pitch,
            head,
            applied: Quat::IDENTITY,
            written: Quat::IDENTITY,
            moved: Vec3::ZERO,
            placed: Vec3::NAN,
        }
    }
}

/// The bones of one body that the look and the twist turn: the hips up the
/// spine to the head. Every turn is composed in the model's frame (a twist
/// about its up, a look about its right) and carried into each bone's
/// parent frame through the rotations above it, read off the pose the
/// animation just wrote: the rig's bones are not axis-aligned.
#[derive(Component, Clone, Debug, Default)]
pub struct SpineBones {
    /// The model's frame into the first bone's parent: the scene's stand-up
    /// turn. Nothing animates it.
    root: Quat,
    /// The metres one unit of the first bone's parent is (the rig's
    /// armature is in centimetres).
    unit_m: f32,
    bones: Vec<SpineBone>,
}

/// Finds a model's spine, under `model` (a spawned Triple T scene), and how
/// the look and the twist are shared. At its scene's spawn: its entities
/// exist and nothing has posed them.
pub fn bind_spine(
    model: Entity,
    names: &Query<&Name>,
    parents: &Query<&ChildOf>,
    children: &Query<&Children>,
    transforms: &Query<&Transform>,
) -> SpineBones {
    let head = children
        .iter_descendants(model)
        .take(512)
        .find(|&e| names.get(e).is_ok_and(|n| n.as_str() == HEAD_BONE));
    let mut chain = Vec::new();
    if let Some(e) = head {
        let mut walk = vec![e];
        let mut up = e;
        let mut ours = false;
        for _ in 0..16 {
            match parents.get(up) {
                Ok(p) if p.0 == model => {
                    ours = true;
                    break;
                }
                Ok(p) => {
                    up = p.0;
                    walk.push(up);
                }
                Err(_) => break,
            }
        }
        if ours {
            walk.reverse();
            chain = walk;
        }
    }
    if chain.is_empty() {
        error!("anim: no bone named {HEAD_BONE:?} under a body: heads will not follow the aim");
        return SpineBones::default();
    }
    // From the hips: above them is the scene's own root, which nothing
    // should turn.
    let is_hips = |e: Entity| names.get(e).is_ok_and(|n| n.as_str() == HIPS_BONE);
    let hips = chain.iter().position(|&e| is_hips(e));
    let chain = &chain[hips.unwrap_or(0)..];
    // The rest rotation from the model down to the first bone's parent,
    // walked from local transforms (global ones aren't propagated yet).
    let mut root = Quat::IDENTITY;
    let mut unit_m = 1.0;
    let mut up = parents.get(chain[0]).map(|p| p.0).unwrap_or(model);
    for _ in 0..16 {
        if up == model {
            break;
        }
        if let Ok(t) = transforms.get(up) {
            root = t.rotation * root;
            unit_m *= t.scale.x;
        }
        match parents.get(up) {
            Ok(p) => up = p.0,
            Err(_) => break,
        }
    }
    // hips? | spine… | neck | head
    let rest = &chain[usize::from(hips.is_some())..];
    let n = rest.len();
    let spine = n.saturating_sub(2);
    let weight = |i: usize| match n - 1 - i {
        0 => LOOK_HEAD_W,
        1 => LOOK_NECK_W,
        _ => LOOK_SPINE_W,
    };
    let total: f32 = (0..n).map(weight).sum();
    let mut bones = Vec::with_capacity(chain.len());
    if hips.is_some() {
        bones.push(SpineBone::new(chain[0], 1.0, 0.0, false));
    }
    for (i, &e) in rest.iter().enumerate() {
        // The spine turns the chest back to the aim. No hips, no twist.
        let yaw = if hips.is_some() && i < spine {
            -1.0 / spine as f32
        } else {
            0.0
        };
        bones.push(SpineBone::new(e, yaw, weight(i) / total, i + 1 == n));
    }
    SpineBones { root, unit_m, bones }
}

/// What [`turn_spine`] reads of a body this frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct SpinePose {
    pub clip: Option<Clip>,
    pub pitch: f32,
    pub hip_yaw: f32,
    /// Where the head is moved, metres in the model's frame (the own
    /// shadow's, `own_shadow.rs`; zero for every other body).
    pub head_push: Vec3,
}

/// Turn a body's hips toward where its legs are going, its spine back
/// toward where it faces, and its chest and head to where it looks.
///
/// Between the animation and the transform propagation (`PostUpdate`): the
/// clip has posed the skeleton and nothing has made world transforms of it
/// yet, so this is a few multiplies a bone and no re-propagation. A bone
/// the clip rewrote this frame takes the turn straight; one that still
/// holds what this wrote last frame has last frame's turn taken out first,
/// so a bone no clip moves can't be wound round sixty times a second.
pub fn turn_spine(pose: SpinePose, spine: &mut SpineBones, bones: &mut Query<&mut Transform>) {
    // A corpse looks at nothing: both turns drop to nothing through the
    // same bookkeeping, so what was written is still taken out.
    let dead = matches!(pose.clip, Some(Clip::Death) | None);
    let pitch = if dead {
        0.0
    } else {
        pose.pitch.clamp(-LOOK_PITCH_MAX, LOOK_PITCH_MAX)
    };
    // The crouch clips bow the head ~42° further than the idle: lift it by
    // the difference, so a crouched body looking level looks level.
    let lift = match pose.clip {
        Some(Clip::CrouchIdle | Clip::CrouchWalk) => CROUCH_HEAD_LIFT,
        _ => 0.0,
    };
    let yaw = if dead { 0.0 } else { pose.hip_yaw };
    let unit = spine.unit_m.max(1e-6);
    let mut w = spine.root;
    for bone in spine.bones.iter_mut() {
        let Ok(mut t) = bones.get_mut(bone.entity) else {
            break;
        };
        let base = if t.rotation == bone.written {
            bone.applied.inverse() * t.rotation
        } else {
            t.rotation
        };
        let look = pitch * bone.pitch + if bone.head { lift } else { 0.0 };
        let turn = Quat::from_rotation_y(yaw * bone.yaw) * Quat::from_axis_angle(BODY_RIGHT, look);
        // A turn in the model's frame, carried into the parent's; normalised
        // at every step, or a bone no clip rewrites shrinks with rounding.
        let local = (w.inverse() * turn * w).normalize();
        t.rotation = (local * base).normalize();
        bone.applied = local;
        bone.written = t.rotation;
        if bone.head {
            // The push, in the head's parent's units.
            let at = if t.translation == bone.placed {
                t.translation - bone.moved
            } else {
                t.translation
            };
            let moved = w.inverse() * (pose.head_push / unit);
            t.translation = at + moved;
            bone.moved = moved;
            bone.placed = t.translation;
        }
        w = (w * t.rotation).normalize();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;

    fn seen(vel: Vec2, yaw: f32) -> Seen {
        Seen {
            alive: true,
            grounded: true,
            vel,
            yaw,
            ..Seen::default()
        }
    }

    /// Run a body along a world bearing (the wire's sense: 0 along +X,
    /// growing toward +Z) while it faces `yaw`.
    fn travel(g: &mut Gait, yaw: f32, bearing: f32, mps: f32, frames: usize) {
        let s = seen(Vec2::new(bearing.cos(), bearing.sin()) * mps, yaw);
        for _ in 0..frames {
            g.observe(&s, DT);
        }
    }

    #[test]
    fn standing_still_is_idle_and_running_is_the_sprint() {
        let mut g = Gait::default();
        travel(&mut g, 0.0, 0.0, 0.0, 30);
        assert_eq!(g.clip, Some(Clip::Idle));
        for (mps, want) in [
            (0.0, Clip::Idle),
            (1.2, Clip::Walk),
            (4.0, Clip::Jog),
            (sim::consts::RUN, Clip::Sprint),
        ] {
            let mut g = Gait::default();
            travel(&mut g, 0.0, 0.0, mps, 120);
            assert_eq!(g.clip, Some(want), "at {mps} m/s");
        }
        let mut g = Gait::default();
        let s = Seen {
            crouched: true,
            ..seen(Vec2::new(sim::consts::CROUCH_RUN, 0.0), 0.0)
        };
        for _ in 0..120 {
            g.observe(&s, DT);
        }
        assert_eq!(g.clip, Some(Clip::CrouchWalk));
        assert_eq!(g.rate(), RATE_MAX, "a crouch is clamped, not scuttled");
    }

    #[test]
    fn a_loop_plays_at_the_rate_that_plants_its_feet() {
        let mut g = Gait::default();
        travel(&mut g, 0.0, 0.0, sim::consts::RUN, 120);
        let stride = g.clip.and_then(Clip::stride_mps).unwrap();
        assert!((g.rate() - sim::consts::RUN / stride).abs() < 0.02, "{}", g.rate());
        let mut g = Gait::default();
        travel(&mut g, 0.0, 0.0, 0.0, 30);
        assert_eq!(g.rate(), 1.0);
    }

    #[test]
    fn a_body_moving_backwards_backpedals() {
        let mut g = Gait::default();
        travel(&mut g, 0.3, 0.3 + PI, sim::consts::RUN, 120);
        assert_eq!(g.clip, Some(Clip::Sprint));
        assert!(g.backward);
        assert!(g.rate() < 0.0, "the loop is not reversed");
        assert!(g.hip_yaw.abs() < 0.01, "a straight backpedal twisted the legs");
        // Diagonally back: the legs turn toward the travel, reversed.
        let mut g = Gait::default();
        travel(&mut g, 0.0, PI - 0.5, sim::consts::RUN, 120);
        assert!(g.backward);
        assert!((g.hip_yaw - 0.5).abs() < 0.01, "hip yaw {}", g.hip_yaw);
    }

    #[test]
    fn a_strafing_body_turns_its_legs_toward_the_travel() {
        // Facing +X and running toward +Z (to its right): Gates' angles turn
        // the other way from the wire's, so the hips go negative.
        let mut g = Gait::default();
        travel(&mut g, 0.0, 0.6, sim::consts::RUN, 120);
        assert!(!g.backward);
        assert!((g.hip_yaw + 0.6).abs() < 0.01, "hip yaw {}", g.hip_yaw);
        let mut g = Gait::default();
        travel(&mut g, 0.0, FRAC_PI_2, sim::consts::RUN, 120);
        assert!(!g.backward);
        assert!((g.hip_yaw + TWIST_MAX).abs() < 0.01, "side-on is clamped");
        // Stopping brings the legs back under the body.
        travel(&mut g, 0.0, 0.0, 0.0, 240);
        assert_eq!(g.clip, Some(Clip::Idle));
        assert!(g.hip_yaw.abs() < 0.03, "the legs stayed twisted: {}", g.hip_yaw);
    }

    #[test]
    fn a_standing_body_keeps_its_feet_until_it_turns_too_far() {
        let mut g = Gait::default();
        travel(&mut g, 0.0, 0.0, 0.0, 30);
        // A small turn: the feet stay where they were.
        travel(&mut g, 0.4, 0.0, 0.0, 30);
        assert!((g.hip_yaw - 0.4).abs() < 1e-4, "hip yaw {}", g.hip_yaw);
        // Past the limit they step back round, all the way.
        travel(&mut g, 1.4, 0.0, 0.0, 60);
        assert!(g.hip_yaw.abs() < 0.03, "hip yaw {}", g.hip_yaw);
        // Across ±π is a short turn, not a long one.
        let mut g = Gait::default();
        travel(&mut g, PI - 0.1, 0.0, 0.0, 30);
        travel(&mut g, -PI + 0.1, 0.0, 0.0, 1);
        assert!((g.hip_yaw - 0.2).abs() < 1e-3, "hip yaw {}", g.hip_yaw);
    }

    #[test]
    fn a_body_on_a_threshold_does_not_flicker() {
        let mut g = Gait::default();
        travel(&mut g, 0.0, 0.0, JOG_MPS, 120);
        let settled = g.clip;
        let mut changes = 0;
        for i in 0..240 {
            let wobble = if i % 2 == 0 { 0.2 } else { -0.2 };
            let before = g.clip;
            g.observe(&seen(Vec2::new(JOG_MPS + wobble, 0.0), 0.0), DT);
            if g.clip != before {
                changes += 1;
            }
        }
        assert_eq!(g.clip, settled);
        assert_eq!(changes, 0, "clip changed {changes} times on a threshold");
    }

    #[test]
    fn the_air_the_wheel_a_seat_and_death_outrank_the_legs() {
        let mut g = Gait::default();
        travel(&mut g, 0.0, 0.0, sim::consts::RUN, 120);
        let s = seen(Vec2::new(sim::consts::RUN, 0.0), 0.0);
        g.observe(&Seen { grounded: false, ..s }, DT);
        assert_eq!(g.clip, Some(Clip::Air));
        g.observe(&s, DT);
        assert_eq!(g.clip, Some(Clip::Sprint), "landed into the wrong gait");
        let hold = Some(Hold::Long);
        g.observe(
            &Seen {
                riding: true,
                hold,
                ..s
            },
            DT,
        );
        assert_eq!(g.clip, Some(Clip::Idle), "a passenger ran the hull's speed");
        assert_eq!(g.holding(), hold);
        g.observe(
            &Seen {
                riding: true,
                driving: true,
                hold,
                ..s
            },
            DT,
        );
        assert_eq!(g.clip, Some(Clip::Drive));
        assert_eq!(g.holding(), None, "a driver holds the wheel");
        g.observe(
            &Seen {
                alive: false,
                hold,
                ..s
            },
            DT,
        );
        assert_eq!(g.clip, Some(Clip::Death));
        assert_eq!(g.holding(), None);
    }

    #[test]
    fn angles_wrap_the_short_way() {
        for (a, w) in [
            (0.0, 0.0),
            (PI + 0.1, -PI + 0.1),
            (-PI - 0.1, PI - 0.1),
            (5.0 * TAU + 0.2, 0.2),
        ] {
            assert!((wrap_pi(a) - w).abs() < 1e-4, "wrap_pi({a}) = {}", wrap_pi(a));
        }
        assert!((wrap_pi(PI) - PI).abs() < 1e-6);
    }

    #[test]
    fn every_clip_and_hold_has_its_own_slot() {
        let mut seen = [false; Clip::ALL.len()];
        for c in Clip::ALL {
            assert!(!seen[c.slot()], "slot clash on {c:?}");
            seen[c.slot()] = true;
        }
        let mut seen = [false; Hold::ALL.len()];
        for h in Hold::ALL {
            assert!(!seen[h.slot()], "slot clash on {h:?}");
            seen[h.slot()] = true;
        }
    }
}

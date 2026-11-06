//! A gun that runs hot, drawn. Its **charge**: a green ball of plasma growing on
//! the muzzle in our own view as the trigger is held, the charge shown as size,
//! lighting the gun, sparks drawn into it, beating once it is full; let go, it
//! is the round itself that flies (`flying.rs`). Its **heat**: the gun's own
//! meshes again under a skin of light (`plasma.rs`), nothing until it is well
//! warm, then a rim and heat crawling on its plates. Its **vent**: a hard first
//! frame of white-hot jets and a crack of light as it lets go, then vapour that
//! thins as it cools; on anyone we see, not only ourselves.
//!
//! Drawing only: all of it reads what the snapshots already say, the predicted
//! fighter's charge, heat and vent, and whether anyone else is reloading a gun
//! that runs hot.

use bevy::mesh::MeshTag;
use bevy::prelude::*;
use client_net::core::Drawn;
use sim::arms::{arm, BEAM_RIFLE, HEAT_FULL, NONE};
use sim::consts::ticks;
use sim::fighter::Fighter;

use super::atlas::{GLOW, STREAK};
use super::plasma::{self, spawn_ball, tag, Kit, Tone};
use super::pool::{Orient, Particle};
use super::table::Layer;
use super::{FlashDef, Fx};
use crate::look::viewmodel::{ViewMuzzles, GUN_FOV, SHRINK};

/// The charge's ball, at full, metres at the depth our flash sits at, and the
/// share of that it starts from.
const ORB_FULL_M: f32 = 0.1;
const ORB_START: f32 = 0.2;
/// How far off the gun it burns at full, in view (`shots::clear_of_gun`).
const ORB_CLEAR_M: f32 = 0.11;
/// Its heat, starting and full; its beat once full (times a second, a share of
/// its size).
const ORB_HEAT: (f32, f32) = (0.1, 0.55);
const ORB_BEAT_HZ: f32 = 2.6;
const ORB_BEAT: f32 = 0.1;
/// The light it throws on the gun in our hands: lumens at full (the gun is
/// drawn [`SHRINK`] small and that much nearer, and so is its light), its reach.
const ORB_LM: f32 = 26_000.0;
const ORB_LIGHT_M: f32 = 0.9;
const ORB_LIGHT_OFF: Vec3 = Vec3::new(0.0, 0.018, 0.03);
/// Sparks drawn into it, a second at full, from how far, how long they take.
const ORB_SPARKS_HZ: f32 = 70.0;
const ORB_SPARK_FROM_M: f32 = 0.22;
const ORB_SPARK_S: f32 = 0.13;
const ORB_SPARK: [f32; 4] = [2.0, 11.0, 4.0, 1.0];
const ORB_SPARK_END: [f32; 4] = [0.4, 3.0, 0.8, 0.0];

/// A gun's skin starts to show at this share of full heat.
const SKIN_FROM: f32 = 0.45;

/// Where a vent blows in our own view, from the muzzle as it is drawn: off the
/// nose of the gun, and along its top. Anything nearer the eye than the gun is
/// faded out against it, and anything behind it is hidden, so what we see is
/// what clears its edge. Everything about one here against one seen on another:
/// nearer than arm's length, so slower.
const OWN_PORTS: [Vec3; 2] = [Vec3::new(-0.03, 0.03, 0.0), Vec3::new(0.12, 0.12, 0.0)];
/// How much higher the top of a long gun is, in view, than a pistol's.
const OWN_LONG_LIFT: f32 = 0.18;
const OWN_K: f32 = 0.55;
/// A jet: its hot streaks a side, how fast (m/s) and how long (s), how much they
/// lean up and ahead; its hot root, across, at its start and its end (s).
const JET_STREAKS: usize = 26;
const JET_MPS: (f32, f32) = (5.0, 15.0);
const JET_S: (f32, f32) = (0.14, 0.36);
const JET_UP: f32 = 0.65;
const JET_AHEAD: f32 = 0.2;
const JET_ROOT_M: (f32, f32) = (0.3, 0.75);
const JET_ROOT_S: f32 = 0.11;
/// The crack of light as it lets go.
const VENT_LM: f32 = 520_000.0;
const VENT_LIGHT_M: f32 = 6.0;
const VENT_LIGHT_S: f32 = 0.2;
/// The vapour: puffs in the jets, and a second while it cools, by how big.
const VENT_PUFFS: usize = 5;
const VENT_PUFF_SIZE: f32 = 2.4;
const STEAM_HZ: f32 = 22.0;
const STEAM_SIZE: f32 = 1.5;
/// Sparks still spitting off it while it cools, a second.
const STEAM_SPARKS_HZ: f32 = 16.0;
/// Where another's gun is against their feet: ahead, up.
const GUN_AHEAD_M: f32 = 0.55;
const GUN_UP_M: f32 = 1.15;

/// What a vent looks like, by what it blows out of.
struct VentLook {
    hot: [f32; 4],
    cool: [f32; 4],
    light: [f32; 3],
    vapour: Color,
}

fn tone_of(id: u8) -> Tone {
    if id == BEAM_RIFLE {
        Tone::Pink
    } else {
        Tone::Blue
    }
}

fn vent_look(id: u8) -> VentLook {
    match tone_of(id) {
        Tone::Pink => VentLook {
            hot: [12.0, 7.0, 13.0, 1.0],
            cool: [3.0, 0.6, 3.6, 0.0],
            light: [1.0, 0.45, 0.95],
            vapour: Color::srgb(0.88, 0.72, 0.95),
        },
        _ => VentLook {
            hot: [8.0, 10.0, 14.0, 1.0],
            cool: [0.7, 1.8, 4.0, 0.0],
            light: [0.5, 0.7, 1.0],
            vapour: Color::srgb(0.75, 0.84, 0.97),
        },
    }
}

/// A hand's charge's ball in our own view: its place, its size and its light.
#[derive(Component)]
pub struct ChargeOrb {
    left: bool,
    ball: Entity,
}

#[derive(Component)]
pub struct OrbLight {
    left: bool,
}

/// A mesh of a gun in our own view under its skin of heat.
#[derive(Component)]
pub struct ViewHeat {
    pub left: bool,
}

/// What our own vents were doing, by hand.
#[derive(Default)]
pub struct OwnVent {
    on: [bool; 2],
    steam: [f32; 2],
}

/// Puts the two hands' balls under the eye, once it is there.
pub fn spawn_orbs(
    mut commands: Commands,
    mut done: Local<bool>,
    kit: Option<Res<Kit>>,
    eye: Query<Entity, With<crate::Eye>>,
) {
    if *done {
        return;
    }
    let (Some(kit), Ok(eye)) = (kit, eye.single()) else {
        return;
    };
    *done = true;
    let [r, g, b] = [0.2, 1.0, 0.35];
    for left in [false, true] {
        let mut ball = Entity::PLACEHOLDER;
        let root = commands
            .spawn((Transform::default(), Visibility::Hidden, ChildOf(eye)))
            .with_children(|c| ball = spawn_ball(c, &kit, Tone::Green, ORB_FULL_M, ORB_HEAT.0))
            .id();
        commands.entity(root).insert(ChargeOrb { left, ball });
        commands.spawn((
            OrbLight { left },
            Transform::default(),
            PointLight {
                intensity: 0.0,
                range: ORB_LIGHT_M,
                color: Color::linear_rgb(r, g, b),
                shadows_enabled: false,
                ..default()
            },
            ChildOf(eye),
        ));
    }
}

/// How full a hand's charge is, 0 to 1 (0: not charging).
fn charge_of(f: &Fighter, left: bool) -> f32 {
    let (id, t) = if left {
        (f.left.arm, f.left_charge)
    } else {
        (f.gun().arm, f.charge)
    };
    let a = arm(id);
    if !f.alive || id == NONE || a.charge_ms == 0 || t == 0 {
        return 0.0;
    }
    (t as f32 / ticks(a.charge_ms) as f32).min(1.0)
}

/// Every frame, once the gun in view is posed: the balls on the muzzles.
#[allow(clippy::too_many_arguments)]
pub fn charge_up(
    link: NonSend<crate::LinkRes>,
    muzzles: Res<ViewMuzzles>,
    time: Res<Time>,
    mut fx: ResMut<Fx>,
    eye: Query<&GlobalTransform, With<crate::Eye>>,
    mut orbs: Query<(&ChargeOrb, &mut Transform, &mut Visibility)>,
    mut tags: Query<&mut MeshTag>,
    mut lights: Query<(&OrbLight, &mut PointLight, &mut Transform), Without<ChargeOrb>>,
) {
    let f = link.0.session().core.own().0;
    let t = time.elapsed_secs();
    let dt = time.delta_secs();
    for (orb, mut tr, mut vis) in &mut orbs {
        let hand = usize::from(orb.left);
        let drawn = muzzles.0[hand];
        let c = if drawn.is_some() { charge_of(f, orb.left) } else { 0.0 };
        if c <= 0.0 {
            vis.set_if_neq(Visibility::Hidden);
            continue;
        }
        vis.set_if_neq(Visibility::Inherited);
        let at = super::shots::clear_of_gun(super::shots::own_muzzle(orb.left, drawn), ORB_CLEAR_M * c);
        let full = c >= 1.0;
        let beat = if full {
            1.0 + ORB_BEAT * (t * ORB_BEAT_HZ * core::f32::consts::TAU).sin()
        } else {
            1.0
        };
        tr.translation = at;
        tr.scale = Vec3::splat((ORB_START + (1.0 - ORB_START) * c) * beat);
        if let Ok(mut tag_now) = tags.get_mut(orb.ball) {
            let want = tag(ORB_HEAT.0 + (ORB_HEAT.1 - ORB_HEAT.0) * c);
            if tag_now.0 != want.0 {
                *tag_now = want;
            }
        }
        // Sparks drawn in from round it, in the world, as the frame's are.
        let Ok(eye) = eye.single() else {
            continue;
        };
        let centre = eye.transform_point(at);
        if fx.rand() < dt * ORB_SPARKS_HZ * c {
            let d = fx.unit();
            let life = ORB_SPARK_S;
            fx.glow.spawn(Particle {
                left: life,
                life,
                pos: (centre + d * ORB_SPARK_FROM_M).into(),
                vel: (-d * (ORB_SPARK_FROM_M / life)).into(),
                size0: 0.0045,
                size1: 0.002,
                c0: ORB_SPARK,
                c1: ORB_SPARK_END,
                cell: STREAK,
                orient: Orient::Stretch,
                stretch: 0.02,
                ..Particle::default()
            });
        }
    }
    for (light, mut l, mut tr) in &mut lights {
        let hand = usize::from(light.left);
        let c = charge_of(f, light.left);
        let want = match muzzles.0[hand] {
            Some(m) if c > 0.0 => {
                // The muzzle as it looks, back to where the gun really is.
                tr.translation = m * Vec3::new(1.0 / GUN_FOV, 1.0 / GUN_FOV, 1.0) * SHRINK + ORB_LIGHT_OFF;
                ORB_LM * c * c
            }
            _ => 0.0,
        };
        if l.intensity != want {
            l.intensity = want;
        }
    }
}

/// Every frame: how hot each hand's gun looks.
pub fn heat_skin(link: NonSend<crate::LinkRes>, mut skins: Query<(&ViewHeat, &mut MeshTag, &mut Visibility)>) {
    let f = link.0.session().core.own().0;
    for (skin, mut tag_now, mut vis) in &mut skins {
        let heat = if skin.left { f.left.heat } else { f.gun().heat };
        let share = heat as f32 / HEAT_FULL as f32;
        let k = ((share - SKIN_FROM) / (1.0 - SKIN_FROM)).clamp(0.0, 1.0);
        if k <= 0.0 {
            vis.set_if_neq(Visibility::Hidden);
            continue;
        }
        vis.set_if_neq(Visibility::Inherited);
        let want = tag(k);
        if tag_now.0 != want.0 {
            *tag_now = want;
        }
    }
}

/// The skin for a gun's scene, for `id`, by which hand.
pub fn skin_of(kit: &Kit, id: u8) -> Handle<plasma::PlasmaMaterial> {
    kit.skin(tone_of(id))
}

/// A vent letting go at `at`: white-hot jets out to `side` and its opposite, a
/// crack of light, a burst of vapour. `k` is how near it is seen.
fn burst(fx: &mut Fx, at: Vec3, side: Vec3, up: Vec3, ahead: Vec3, id: u8, k: f32) {
    let look = vent_look(id);
    fx.flash(
        at,
        FlashDef {
            lumens: VENT_LM,
            range: VENT_LIGHT_M,
            life: VENT_LIGHT_S,
            color: look.light,
        },
    );
    for s in [1.0, -1.0] {
        let jet = (side * s + up * JET_UP + ahead * JET_AHEAD).normalize_or(up);
        for _ in 0..JET_STREAKS {
            let d = (jet + fx.unit() * 0.28).normalize_or(jet);
            let life = fx.range(JET_S.0, JET_S.1);
            let speed = fx.range(JET_MPS.0, JET_MPS.1) * k;
            fx.glow.spawn(Particle {
                left: life,
                life,
                pos: at.into(),
                vel: (d * speed).into(),
                gravity: 2.0 * k,
                drag: 3.5,
                size0: 0.016 * k,
                size1: 0.004 * k,
                c0: look.hot,
                c1: look.cool,
                cell: STREAK,
                orient: Orient::Stretch,
                stretch: 0.025,
                ..Particle::default()
            });
        }
        // The jet's hot root, for its first frames.
        fx.glow.spawn(Particle {
            left: JET_ROOT_S,
            life: JET_ROOT_S,
            pos: (at + jet * 0.06 * k).into(),
            vel: (jet * 2.0 * k).into(),
            size0: JET_ROOT_M.0 * k,
            size1: JET_ROOT_M.1 * k,
            c0: look.hot,
            c1: look.cool,
            cell: GLOW,
            ..Particle::default()
        });
        fx.emit_tint(
            Layer::Wisp,
            VENT_PUFFS,
            at + jet * 0.1 * k,
            jet,
            look.vapour,
            VENT_PUFF_SIZE * k,
        );
    }
    fx.emit_tint(Layer::Arcs, 3, at, up, Color::WHITE, 0.7 * k);
}

/// A second of vapour lost as it cools, `dt` of it, at `rate` of its fullest,
/// and sparks spitting off it.
#[allow(clippy::too_many_arguments)]
fn steam(fx: &mut Fx, acc: &mut f32, at: Vec3, id: u8, k: f32, rate: f32, dt: f32) {
    *acc += dt * STEAM_HZ * rate;
    let look = vent_look(id);
    while *acc >= 1.0 {
        *acc -= 1.0;
        let off = fx.unit() * 0.05 * k;
        fx.emit_tint(Layer::Wisp, 1, at + off, Vec3::Y, look.vapour, STEAM_SIZE * k);
    }
    if fx.rand() < dt * STEAM_SPARKS_HZ * rate {
        let d = (Vec3::Y + fx.unit() * 0.7).normalize_or(Vec3::Y);
        let life = fx.range(0.12, 0.3);
        let speed = fx.range(2.0, 6.0) * k;
        fx.glow.spawn(Particle {
            left: life,
            life,
            pos: at.into(),
            vel: (d * speed).into(),
            gravity: 4.0 * k,
            size0: 0.01 * k,
            size1: 0.003 * k,
            c0: look.hot,
            c1: look.cool,
            cell: STREAK,
            orient: Orient::Stretch,
            stretch: 0.02,
            ..Particle::default()
        });
    }
}

/// Our own guns' vents this frame.
pub fn own(fx: &mut Fx, f: &Fighter, eye: Vec3, cam: Quat, muzzles: [Option<Vec3>; 2], dt: f32) {
    for (hand, drawn) in muzzles.into_iter().enumerate() {
        let left = hand == 1;
        let (id, reload) = if left {
            (f.left.arm, f.left_reload)
        } else {
            (f.gun().arm, f.reload)
        };
        let a = arm(id);
        let on = f.alive && id != NONE && a.hot() && reload > 0;
        let was = core::mem::replace(&mut fx.own_vent.on[hand], on);
        if !on {
            continue;
        }
        let muzzle = super::shots::own_muzzle(left, drawn);
        let side = if left { -1.0 } else { 1.0 };
        let cooling = (reload as f32 / a.vent_ticks().max(1) as f32).min(1.0);
        let mut acc = fx.own_vent.steam[hand];
        let lift = if crate::look::guns::gun(id).is_some_and(|g| g.long) {
            OWN_LONG_LIFT
        } else {
            0.0
        };
        for (k, port) in OWN_PORTS.into_iter().enumerate() {
            let y = port.y + if k == 1 { lift } else { 0.0 };
            let at = eye + cam * (muzzle + Vec3::new(port.x * side, y, port.z));
            if !was {
                burst(fx, at, cam * Vec3::X, cam * Vec3::Y, cam * Vec3::NEG_Z, id, OWN_K);
            }
            steam(fx, &mut acc, at, id, OWN_K, cooling, dt);
        }
        fx.own_vent.steam[hand] = acc;
    }
}

/// Where another's gun is.
fn their_gun(d: &Drawn, feet: Vec3) -> Vec3 {
    let ahead = Vec3::new(d.yaw.cos(), 0.0, d.yaw.sin());
    feet + ahead * GUN_AHEAD_M + Vec3::Y * GUN_UP_M
}

/// Someone else's gun this frame: whether it is venting, and what that looks
/// like as it begins and goes on.
pub fn theirs(fx: &mut Fx, d: &Drawn, feet: Vec3, was: bool, acc: &mut f32, dt: f32) -> bool {
    let on = d.alive && d.reloading && arm(d.arm).hot();
    if !on || super::table::lod(feet.distance(fx.eye)) <= 0.0 {
        return on;
    }
    let at = their_gun(d, feet);
    if !was {
        let ahead = Vec3::new(d.yaw.cos(), 0.0, d.yaw.sin());
        let side = Vec3::new(-ahead.z, 0.0, ahead.x);
        burst(fx, at, side, Vec3::Y, ahead, d.arm, 1.0);
    }
    steam(fx, acc, at, d.arm, 1.0, 0.6, dt);
    on
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A charge fills to 1 and no further, and an empty hand has none.
    #[test]
    fn a_charge_shows_how_full_it_is() {
        let a = arm(sim::arms::ION_PISTOL);
        let mut f = Fighter {
            alive: true,
            ..Fighter::default()
        };
        f.guns[0] = sim::fighter::Gun::full(sim::arms::ION_PISTOL);
        assert_eq!(charge_of(&f, false), 0.0);
        f.charge = (ticks(a.charge_ms) / 2) as u16;
        assert!((charge_of(&f, false) - 0.5).abs() < 0.01);
        f.charge = (ticks(a.charge_ms) * 2) as u16;
        assert_eq!(charge_of(&f, false), 1.0);
        assert_eq!(charge_of(&f, true), 0.0, "no gun in the left hand");
        f.alive = false;
        assert_eq!(charge_of(&f, false), 0.0);
    }
}

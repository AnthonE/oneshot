//! Names over heads, after Gates' `crates/client/src/render/faces.rs`
//! (MIT, NOTICE; its account pictures are left out), made fair for a
//! shooter:
//!
//! - **A teammate's** name is over their head whenever their body is drawn.
//! - **An enemy's** only while your crosshair is on them (the aim ray enters
//!   their hit volume, `sim::consts::HIT_*` and `HEAD_*`, before it meets
//!   anything of the map) **and** the map says their head can be seen from
//!   your eye (`sim::map::Map::clear`). The server sends an enemy who could
//!   be seen *soon* (`room/src/fog.rs`), so being sent is not being seen: a
//!   name never shows through a wall. It does show over an enemy in a fog
//!   bank, a dark corner or the grass, inside [`REACH_M`]: what the sim's
//!   map can't see through stops it, what only the picture hides doesn't.
//!   That is Halo's red reticle, the same for every player and every rung.
//! - **At the wheel** only teammates: the chase camera is behind the hull,
//!   not where the aim leaves from, and sees round what the seat can't.
//!
//! The tests are the sim map's own raycasts, the same on every target and
//! every rung, so what a tag tells one player it tells every player looking
//! the same way. None while you are dead (the death cam watches your own
//! body) or off a match (the HUD's root hides them).
//!
//! Bevy draws, it does not decide: who is aimed at is worked out here from
//! the session's state and nothing is sent.

use bevy::prelude::*;
use sim::consts::{CROUCH_EYE, EYE, HEAD_R, HEAD_Y, HEAD_Y_CROUCH, HIT_R, HIT_TOP, HIT_TOP_CROUCH};
use sim::geom::{ray_cylinder, ray_sphere, V3};

use crate::{hud, ui, Eye, LinkRes, Look};

/// One tag a seat.
const TAGS: usize = sim::consts::MAX_FIGHTERS;
/// How far over the top of the head a tag sits, metres.
const ABOVE_M: f32 = 0.3;
/// The furthest an enemy's name is read off the crosshair, metres: past a
/// rifle's reach a name is a label on a speck.
pub const REACH_M: f32 = 80.0;
const FONT_PX: f32 = 15.0;
/// A teammate's ink, and an enemy's.
const MATE_INK: Color = Color::srgba(0.62, 0.84, 1.0, 0.95);
const FOE_INK: Color = Color::srgba(1.0, 0.55, 0.48, 0.95);
/// Off the screen's edge by this much in NDC, a tag is not drawn.
const EDGE: f32 = 1.05;
/// A tag moves when it has moved this much, percent of the screen.
const MOVE_PCT: f32 = 0.02;

/// One pooled tag, and the seat it last showed.
#[derive(Component)]
pub struct Tag {
    slot: Option<u8>,
}

/// Who gets a tag this frame, and where, from the session: the pure half.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Wanted {
    pub slot: u8,
    /// The point over the head, world.
    pub at: Vec3,
    pub mate: bool,
}

/// The aim ray from `eye` along unit `dir`: the nearest enemy it enters
/// before the map, within [`REACH_M`], whose head `eye` can see.
pub fn aimed_at(map: &sim::map::Map, eye: V3, dir: V3, foes: &[client_net::core::Drawn]) -> Option<u8> {
    let wall = map.ray(eye, dir, REACH_M);
    let mut best: Option<(f32, u8, V3)> = None;
    for d in foes.iter().filter(|d| d.alive) {
        let (top, head_y) = if d.crouched {
            (HIT_TOP_CROUCH, HEAD_Y_CROUCH)
        } else {
            (HIT_TOP, HEAD_Y)
        };
        let head = d.pos + V3::new(0.0, head_y, 0.0);
        let hit = [
            ray_cylinder(eye, dir, d.pos, HIT_R, top, wall),
            ray_sphere(eye, dir, head, HEAD_R, wall),
        ]
        .into_iter()
        .flatten()
        .reduce(f32::min);
        if let Some(t) = hit.filter(|&t| t < wall && best.is_none_or(|b| t < b.0)) {
            best = Some((t, d.slot, head));
        }
    }
    best.filter(|&(_, _, head)| map.clear(eye, head))
        .map(|(_, slot, _)| slot)
}

/// Spawn the pool under the HUD's root once it is there, then place a tag
/// over each teammate and over the enemy under the crosshair.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn draw(
    mut commands: Commands,
    link: NonSend<LinkRes>,
    look: Res<Look>,
    st: Res<hud::HudState>,
    seen: Res<crate::Seen>,
    mut built: Local<bool>,
    mut name: Local<String>,
    (mut want, mut foes): (Local<Vec<Wanted>>, Local<Vec<client_net::core::Drawn>>),
    root: Query<Entity, With<crate::visor::TagRoot>>,
    eye: Query<(&Transform, &Projection), With<Eye>>,
    mut tags: Query<(&mut Tag, &mut Node, &mut Text, &mut TextColor, &mut Visibility)>,
) {
    if !*built {
        let Ok(root) = root.single() else {
            return;
        };
        *built = true;
        for _ in 0..TAGS {
            commands.spawn((
                ChildOf(root),
                Tag { slot: None },
                Text::new(""),
                ui::bold(FONT_PX),
                TextColor(MATE_INK),
                ui::TEXT_SHADOW,
                Node {
                    position_type: PositionType::Absolute,
                    ..default()
                },
                // Centred over the point, standing on it.
                UiTransform::from_translation(Val2::percent(-50.0, -100.0)),
                Visibility::Hidden,
            ));
        }
        return;
    }
    let s = link.0.session();
    let core = &s.core;
    let (f, p) = core.own();
    want.clear();
    // At the wheel the camera is behind the hull, not at the eye the aim
    // leaves from: an enemy's name would be drawn where the crosshair
    // isn't, and over a wall the chase camera sees round. Teammates only.
    let driving = f.riding().is_some_and(|(_, seat)| seat == sim::vehicle::DRIVER);
    if s.ready && f.alive {
        let others = &seen.others;
        let side = core.rules.teams.then_some(core.team & 1);
        let mate = |d: &client_net::core::Drawn| side.is_some_and(|t| d.team & 1 == t);
        let over = |d: &client_net::core::Drawn| {
            let head = if d.crouched { HEAD_Y_CROUCH } else { HEAD_Y };
            Vec3::new(d.pos.x, d.pos.y + head + HEAD_R + ABOVE_M, d.pos.z)
        };
        for d in others.iter().filter(|d| d.alive && d.slot != core.slot && mate(d)) {
            want.push(Wanted {
                slot: d.slot,
                at: over(d),
                mate: true,
            });
        }
        let h = if f.body.crouched { CROUCH_EYE } else { EYE };
        let from = V3::new(p.x, p.y + h, p.z);
        let (sy, cy) = look.yaw.sin_cos();
        let (sp, cp) = look.pitch.sin_cos();
        let dir = V3::new(cp * cy, sp, cp * sy);
        foes.clear();
        if !driving {
            foes.extend(others.iter().filter(|d| d.slot != core.slot && !mate(d)).copied());
        }
        if let Some(slot) = aimed_at(&core.map, from, dir, &foes) {
            if let Some(d) = foes.iter().find(|d| d.slot == slot) {
                want.push(Wanted {
                    slot,
                    at: over(d),
                    mate: false,
                });
            }
        }
    }
    let placed = eye.single().ok().map(|(t, proj)| (t.compute_affine().inverse(), proj));
    let mut next = want.iter().filter_map(|w| {
        let (view_from_world, proj) = placed?;
        let ndc = to_ndc(view_from_world, proj, w.at)?;
        Some((w, ndc))
    });
    for (mut tag, mut node, mut text, mut ink, mut vis) in &mut tags {
        let Some((w, ndc)) = next.next() else {
            if tag.slot.is_some() {
                tag.slot = None;
            }
            if *vis != Visibility::Hidden {
                *vis = Visibility::Hidden;
            }
            continue;
        };
        if tag.slot != Some(w.slot) {
            tag.slot = Some(w.slot);
        }
        // The name can land after the body does (the seat's message).
        {
            use std::fmt::Write as _;
            name.clear();
            let _ = write!(name, "{}", st.shown(w.slot));
            if text.0 != *name {
                text.0.clone_from(&name);
            }
        }
        ink.set_if_neq(TextColor(if w.mate { MATE_INK } else { FOE_INK }));
        let (left, top) = ((ndc.x + 1.0) * 50.0, (1.0 - ndc.y) * 50.0);
        let moved = |v: Val, to: f32| !matches!(v, Val::Percent(at) if (at - to).abs() < MOVE_PCT);
        if moved(node.left, left) || moved(node.top, top) {
            node.left = Val::Percent(left);
            node.top = Val::Percent(top);
        }
        if *vis != Visibility::Inherited {
            *vis = Visibility::Inherited;
        }
    }
}

/// A world point into the camera's NDC (x right, y up, ±1 at the edges),
/// or `None` behind the eye or off the screen.
fn to_ndc(view_from_world: bevy::math::Affine3A, proj: &Projection, at: Vec3) -> Option<Vec2> {
    let Projection::Perspective(pp) = proj else {
        return None;
    };
    let v = view_from_world.transform_point3(at);
    if v.z > -pp.near {
        return None;
    }
    let f = 1.0 / (pp.fov * 0.5).tan();
    let ndc = Vec2::new(f / pp.aspect_ratio.max(1e-3) * v.x / -v.z, f * v.y / -v.z);
    (ndc.x.abs() <= EDGE && ndc.y.abs() <= EDGE).then_some(ndc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use client_net::core::Drawn;

    fn foe(slot: u8, x: f32, z: f32) -> Drawn {
        Drawn {
            slot,
            alive: true,
            grounded: true,
            pos: V3::new(x, 0.0, z),
            ..Drawn::default()
        }
    }

    /// The sim's open test floor with one wall across x = 10, from z = −2
    /// to 2, 1 m tall (`tall`: 3 m).
    fn walled(tall: bool) -> sim::map::Map {
        let h = if tall { 3.0 } else { 1.0 };
        let mut m = sim::map::Map::open();
        m.boxes
            .push(sim::geom::Aabb::new(V3::new(9.5, 0.0, -2.0), V3::new(10.5, h, 2.0)));
        m.looks.push(0);
        m.indexed()
    }

    #[test]
    fn a_name_needs_the_crosshair_and_a_clear_head() {
        let eye = V3::new(0.0, EYE, 0.0);
        let level = V3::new(1.0, 0.0, 0.0);
        // In the open, on the crosshair: named.
        let open = walled(false);
        assert_eq!(
            aimed_at(&open, eye, level, &[foe(3, 20.0, 6.0), foe(4, 20.0, -6.0)]),
            None
        );
        assert_eq!(aimed_at(&open, eye, level, &[foe(3, 5.0, 0.0)]), Some(3));
        // Behind a tall wall the server might still send them: never named.
        assert_eq!(aimed_at(&walled(true), eye, level, &[foe(3, 20.0, 0.0)]), None);
        // Behind a low wall, the crosshair on their head over it: named.
        let at_head = {
            let d = V3::new(20.0, HEAD_Y - EYE, 0.0);
            d * (1.0 / d.len())
        };
        assert_eq!(aimed_at(&open, eye, at_head, &[foe(3, 20.0, 0.0)]), Some(3));
        // The crosshair on the low wall in front of their legs: not named.
        let at_legs = {
            let d = V3::new(10.0, 0.5 - EYE, 0.0);
            d * (1.0 / d.len())
        };
        assert_eq!(aimed_at(&open, eye, at_legs, &[foe(3, 20.0, 0.0)]), None);
        // The dead and the far are not named.
        let mut dead = foe(3, 5.0, 0.0);
        dead.alive = false;
        assert_eq!(aimed_at(&open, eye, level, &[dead]), None);
        assert_eq!(aimed_at(&open, eye, level, &[foe(3, REACH_M + 5.0, 0.0)]), None);
        // The nearer of two in line.
        assert_eq!(
            aimed_at(&open, eye, level, &[foe(3, 8.0, 0.0), foe(4, 5.0, 0.0)]),
            Some(4)
        );
    }
}

//! The shade under a fighter: a soft dark patch on the floor at its feet.
//!
//! It is what stands a body on a floor. Under a roof nothing casts (a
//! hall's lamps throw no shadows), so on Chrysalis, Bulkhead and Cistern a
//! fighter stood on its deck like a cut-out laid over the frame; under a
//! dusk's or a blizzard's sun the shadow it does cast is a breath; and at
//! noon the shadow leaves the feet at once and says where the sun is, not
//! where the floor is. This is the light a body keeps off the floor right
//! under it, which is there in any light: strongest where the feet are,
//! gone half a metre out, fainter and wider as the fighter leaves the
//! ground, so a jump reads as a height.
//!
//! **It shows a fighter and never hides one.** A disc and a blend, drawn
//! the same on every rung and by WebGL2; as far gone as a camouflaged
//! fighter is hidden (a cast shadow would give him away, and so would
//! this); none for a corpse or for anyone in a seat. `draw_fighters`
//! says who stands where ([`Under`]), ours as well: looking down, your
//! own feet are on the floor too.
//!
//! **It lies on the floor it is on.** It takes a slope's lie, and at a
//! ledge it draws back onto what holds the fighter up and shrinks by what
//! is missing, so no half of it hangs in the air past a bridge's edge.

use bevy::asset::RenderAssetUsages;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use crate::LinkRes;

/// How wide the patch is at the feet, metres: a body's own footprint
/// (`sim::consts::BODY_R`) and a hand's width of nothing much round it, so
/// what cover hides of a fighter it hides of this. Its middle half is what
/// reads.
const ACROSS_M: f32 = 0.9;
/// How dark its middle is where nothing casts, and under an open sky, where
/// the sun's own shadow already starts at the feet. It is blended before
/// the tone curve, where a floor at a third of its light is still two
/// thirds as bright on the screen: at 0.6 under a roof it could not be
/// found in a still.
const DARK_ROOFED: f32 = 0.88;
const DARK_OPEN: f32 = 0.6;
/// How far over the floor it lies, metres: clear of the floor's own depth
/// at any range, under a sole.
const LIFT_M: f32 = 0.02;
/// Leaving the ground it fades, gone by this height, metres, and widens by
/// this much of its width a metre.
const GONE_M: f32 = 2.6;
const WIDEN: f32 = 0.2;
/// The floor round it is read at eight points this far from its middle,
/// metres. Its lie is followed while it is a slope (no steeper than this
/// rise over run): a step or a ledge's edge is not one, and the patch
/// stays level there.
const RIM_M: f32 = 0.28;
const LIE_MAX: f32 = 0.9;
/// A point of that rim holds the patch up while its floor is within this
/// of the middle's, metres (a stair's riser is; a ledge's drop is not),
/// and with none of them holding it is this share of its width.
const HELD_M: f32 = 0.32;
const UNHELD: f32 = 0.5;
/// The disc's rings and the sides of each: its alpha is a curve, and a
/// ring is a straight piece of it.
const RINGS: u32 = 6;
const SIDES: u32 = 24;
/// An alpha moved less than this is left: a standing fighter's material is
/// never written.
const ALPHA_STEP: f32 = 0.01;

const _: () = assert!(
    DARK_OPEN < DARK_ROOFED && DARK_ROOFED < 1.0 && ACROSS_M * 0.5 - sim::consts::BODY_R <= 0.1,
    "the shade never blacks a floor out, and reaches a hand's width past the body at most"
);

/// Who stands where this frame, and how much of each is seen (1, less a
/// camouflage): `draw_fighters` fills it.
#[derive(Resource, Default)]
pub struct Under(pub Vec<(Vec3, f32)>);

/// One of the pool's patches.
#[derive(Component)]
pub struct Shade(usize);

/// The patch's alpha at `r` of its radius: whole in the middle, flat to
/// nothing at the rim.
fn falloff(r: f32) -> f32 {
    let k = (1.0 - r * r).max(0.0);
    k * k
}

/// The patch: a disc a metre across lying in x and z, black, its alpha
/// in its vertices (whole in the middle, nothing at the rim), so it binds
/// no map and blends the same on every target.
fn disc() -> Mesh {
    let mut at = vec![[0.0, 0.0, 0.0]];
    let mut colour = vec![[0.0, 0.0, 0.0, falloff(0.0)]];
    for ring in 1..=RINGS {
        let r = ring as f32 / RINGS as f32;
        for k in 0..SIDES {
            let a = k as f32 / SIDES as f32 * core::f32::consts::TAU;
            at.push([0.5 * r * a.cos(), 0.0, 0.5 * r * a.sin()]);
            colour.push([0.0, 0.0, 0.0, falloff(r)]);
        }
    }
    let first = |ring: u32| 1 + (ring - 1) * SIDES;
    let mut tris = Vec::new();
    for k in 0..SIDES {
        let next = (k + 1) % SIDES;
        // Wound to face up.
        tris.extend_from_slice(&[0, first(1) + next, first(1) + k]);
        for ring in 1..RINGS {
            let (a, b) = (first(ring), first(ring + 1));
            tris.extend_from_slice(&[a + k, a + next, b + next, a + k, b + next, b + k]);
        }
    }
    let n = at.len();
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, at)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; n])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0, 0.0]; n])
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colour)
        .with_inserted_indices(Indices::U32(tris))
}

/// A patch a fighter, hidden until someone stands.
pub fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    let mesh = meshes.add(disc());
    for i in 0..sim::consts::MAX_FIGHTERS {
        commands.spawn((
            Shade(i),
            Mesh3d(mesh.clone()),
            MeshMaterial3d(mats.add(StandardMaterial {
                base_color: Color::srgba(0.0, 0.0, 0.0, 0.0),
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                depth_bias: 1.0,
                ..default()
            })),
            NotShadowCaster,
            NotShadowReceiver,
            Visibility::Hidden,
            Transform::default(),
        ));
    }
}

/// Each patch under its fighter, on the floor below it.
pub fn draw(
    link: NonSend<LinkRes>,
    under: Res<Under>,
    mut patches: Query<(
        &Shade,
        &mut Transform,
        &mut Visibility,
        &MeshMaterial3d<StandardMaterial>,
    )>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    let map = &link.0.session().core.map;
    let dark = if map.light.sun > 0.0 { DARK_OPEN } else { DARK_ROOFED };
    for (shade, mut at, mut vis, mat) in &mut patches {
        let Some(&(p, seen)) = under.0.get(shade.0) else {
            vis.set_if_neq(Visibility::Hidden);
            continue;
        };
        let reach = p.y + 0.05;
        let floor = map.floor(p.x, p.z, 0.0, reach);
        let up = (p.y - floor).max(0.0);
        let alpha = dark * seen * (1.0 - up / GONE_M).clamp(0.0, 1.0);
        if alpha < ALPHA_STEP || floor < map.void {
            vis.set_if_neq(Visibility::Hidden);
            continue;
        }
        vis.set_if_neq(Visibility::Inherited);
        // The floor round it: its lie where it is a slope, and how much of
        // it there is at a ledge.
        let rim: [(Vec2, f32); 8] = core::array::from_fn(|k| {
            let a = k as f32 * core::f32::consts::FRAC_PI_4;
            let d = Vec2::new(a.cos(), a.sin()) * RIM_M;
            (d, map.floor(p.x + d.x, p.z + d.y, 0.0, reach) - floor)
        });
        let slope = |a: f32, b: f32| {
            let s = (a - b) / (2.0 * RIM_M);
            // Both sides agree it is one plane, and it can be stood on.
            if (a + b).abs() < 0.05 && s.abs() <= LIE_MAX {
                s
            } else {
                0.0
            }
        };
        let n = Vec3::new(-slope(rim[0].1, rim[4].1), 1.0, -slope(rim[2].1, rim[6].1)).normalize();
        let (mut mid, mut held) = (Vec2::ZERO, 0.0);
        for (d, rise) in rim {
            if rise.abs() <= HELD_M {
                mid += d;
                held += 1.0;
            }
        }
        // Drawn back onto what holds it, the middle counting as one more.
        let mid = mid / (held + 1.0);
        let across = ACROSS_M * (1.0 + WIDEN * up) * (UNHELD + (1.0 - UNHELD) * held / 8.0);
        at.set_if_neq(Transform {
            translation: Vec3::new(p.x + mid.x, floor, p.z + mid.y) + n * LIFT_M,
            rotation: Quat::from_rotation_arc(Vec3::Y, n),
            scale: Vec3::new(across, 1.0, across),
        });
        let now = mats.get(&mat.0).map_or(0.0, |m| m.base_color.alpha());
        if (now - alpha).abs() >= ALPHA_STEP {
            if let Some(m) = mats.get_mut(&mat.0) {
                m.base_color.set_alpha(alpha);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The patch is whole under the feet and nothing at its rim, so its
    /// disc's edge never shows.
    #[test]
    fn it_fades_to_nothing_at_its_rim() {
        assert_eq!(falloff(0.0), 1.0);
        assert_eq!(falloff(1.0), 0.0);
        assert_eq!(falloff(1.4), 0.0);
        let mut last = 1.0;
        for k in 1..=20 {
            let a = falloff(k as f32 / 20.0);
            assert!(a <= last);
            last = a;
        }
    }

    /// Cover that hides a body hides its shade: past the body's own radius
    /// there is next to none of it.
    #[test]
    fn it_stays_under_the_body() {
        let at_the_body = falloff(sim::consts::BODY_R / (ACROSS_M * 0.5));
        assert!(DARK_ROOFED * at_the_body < 0.15, "{at_the_body}");
    }
}

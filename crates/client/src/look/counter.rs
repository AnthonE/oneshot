//! The rounds left, on the gun in our own view: a readout in the gun's
//! light lying on the display its model paints, as the HUD
//! says it (the magazine of the gun in that hand), and red when the
//! magazine runs low. The ship's guns carry none: their batteries are the
//! HUD's.
//!
//! Its digits are a seven-segment face drawn here at startup into one
//! small image (no files, as `plate.rs`'s plates are), each digit a quad
//! picking its cell by the material's UV transform. Unlit and added over
//! what is behind, so it glows the same by day and in a hall, and the
//! bloom haloes it where there is one. Drawing only: it reads the predicted
//! fighter's magazines.

use bevy::asset::RenderAssetUsages;
use bevy::light::NotShadowCaster;
use bevy::math::Affine2;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use sim::arms;

use super::guns::{gun, view_basis};
use super::viewmodel::ViewGun;

/// Each gun's readout, in its model's frame and units (x right, y from the
/// stock along the barrel, z up over the axis it is held by): the middle of
/// the display its model paints glowing (measured off each GLB's glow map),
/// the way that display faces, and how tall a digit is on it. The AR's
/// looks back at the eye from its sight; the rocket's is on its right
/// flank, the BR's a thin strip in a slot. The sniper paints none: its
/// readout lies on the receiver's left flank.
pub const PLACES: [(u8, Vec3, Vec3, f32); 7] = [
    (
        arms::AR,
        Vec3::new(0.0, 0.3434, 0.1114),
        Vec3::new(-0.065, -0.945, 0.319),
        0.022,
    ),
    (arms::BR, Vec3::new(-0.0122, 0.3163, 0.0088), Vec3::NEG_X, 0.0068),
    (arms::MAGNUM, Vec3::new(-0.0214, 0.0963, 0.0348), Vec3::NEG_X, 0.0095),
    (arms::SNIPER, Vec3::new(-0.0266, 0.36, 0.05), Vec3::NEG_X, 0.02),
    (arms::SHOTGUN, Vec3::new(-0.0209, 0.3348, 0.0543), Vec3::NEG_X, 0.016),
    (arms::ROCKET, Vec3::new(0.1103, 0.6555, 0.1005), Vec3::X, 0.024),
    (arms::SMG, Vec3::new(-0.009, 0.2765, 0.086), Vec3::NEG_X, 0.0085),
];
/// How far the readout stands off its display, in the model's units, so
/// the two never fight for depth.
const LIFT: f32 = 0.0008;
/// A digit's width for its height, and the gap between two.
const ASPECT: f32 = 0.6;
const GAP: f32 = 0.18;
/// Its light, linear and unlit (over 1 so the bloom takes it), and the
/// low magazine's; low is at or under this share of a full one.
const LIGHT: [f32; 3] = [0.35, 1.05, 2.6];
const LOW: [f32; 3] = [2.6, 0.45, 0.3];
const LOW_SHARE: f32 = 0.25;
/// The face's cells, px: ten digits side by side.
const CELL_W: u32 = 48;
const CELL_H: u32 = 80;
/// Places shown at most.
const PLACES_MAX: usize = 3;

/// The face and the quad every digit is drawn on.
#[derive(Resource)]
pub struct CounterKit {
    face: Handle<Image>,
    quad: Handle<Mesh>,
}

/// A readout under one hand's gun: the gun it was made for, and what it
/// last showed.
#[derive(Component)]
pub struct Counter {
    arm: u8,
    shown: Option<(u16, bool)>,
}

/// One digit of a readout: its place from the right.
#[derive(Component)]
pub struct Digit(u8);

pub fn setup(mut commands: Commands, mut images: ResMut<Assets<Image>>, mut meshes: ResMut<Assets<Mesh>>) {
    let face = images.add(face());
    commands.insert_resource(CounterKit {
        face,
        quad: meshes.add(Rectangle::new(ASPECT, 1.0)),
    });
}

/// Distance from the segment `a`–`b` with round ends.
fn seg(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let (pa, ba) = (p - a, b - a);
    let h = (pa.dot(ba) / ba.dot(ba)).clamp(0.0, 1.0);
    (pa - ba * h).length()
}

/// Which of the seven segments each digit lights: a top, b top right, c
/// bottom right, d bottom, e bottom left, f top left, g middle.
const SEGS: [u8; 10] = [
    0b011_1111, 0b000_0110, 0b101_1011, 0b100_1111, 0b110_0110, 0b110_1101, 0b111_1101, 0b000_0111, 0b111_1111,
    0b110_1111,
];

/// The face: white, its alpha the segments' cover and a soft glow round
/// them, digits 0 to 9 left to right.
fn face() -> Image {
    let (w, h) = (CELL_W * 10, CELL_H);
    let mut data = vec![0u8; (w * h * 4) as usize];
    // A cell's segments, in its own px with y up from the middle, leaning
    // forward a little.
    let (hx, hy, half) = (13.0, 27.0, 1.5);
    let lean = 0.12;
    let at = |x: f32, y: f32| Vec2::new(x + y * lean, y);
    let ends: [(Vec2, Vec2); 7] = [
        (at(-hx + half, hy), at(hx - half, hy)),
        (at(hx, hy - half), at(hx, half)),
        (at(hx, -half), at(hx, -hy + half)),
        (at(-hx + half, -hy), at(hx - half, -hy)),
        (at(-hx, -half), at(-hx, -hy + half)),
        (at(-hx, hy - half), at(-hx, half)),
        (at(-hx + half, 0.0), at(hx - half, 0.0)),
    ];
    let (thick, soft, glow) = (3.4, 1.0, 9.0);
    for d in 0..10u32 {
        for y in 0..h {
            for x in 0..CELL_W {
                let p = Vec2::new(
                    x as f32 + 0.5 - CELL_W as f32 * 0.5,
                    CELL_H as f32 * 0.5 - (y as f32 + 0.5),
                );
                let mut dist = f32::MAX;
                for (s, (a, b)) in ends.iter().enumerate() {
                    if SEGS[d as usize] & (1 << s) != 0 {
                        dist = dist.min(seg(p, *a, *b));
                    }
                }
                let core = (1.0 - ((dist - thick) / soft).clamp(0.0, 1.0)).max(0.0);
                let halo = (1.0 - (dist - thick).max(0.0) / glow).clamp(0.0, 1.0);
                let a = core.max(halo * halo * 0.35);
                let i = ((y * w + d * CELL_W + x) * 4) as usize;
                data[i..i + 4].copy_from_slice(&[255, 255, 255, (a * 255.0).round() as u8]);
            }
        }
    }
    Image::new(
        Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

/// The UV transform that picks digit `d`'s cell.
fn cell(d: u8) -> Affine2 {
    Affine2::from_scale_angle_translation(Vec2::new(0.1, 1.0), 0.0, Vec2::new(f32::from(d) * 0.1, 0.0))
}

fn light(low: bool) -> Color {
    let c = if low { LOW } else { LIGHT };
    Color::LinearRgba(LinearRgba::rgb(c[0], c[1], c[2]))
}

/// The digits of `n` from the right, as many as it needs (one at least).
pub fn digits(n: u16) -> Vec<u8> {
    let mut v = Vec::new();
    let mut n = n;
    loop {
        v.push((n % 10) as u8);
        n /= 10;
        if n == 0 || v.len() == PLACES_MAX {
            return v;
        }
    }
}

/// The turn that lays a digit's quad (+Z out of it, +X along the line)
/// flat on a display facing `out`, upright as the gun is and reading left
/// to right as one facing it sees it.
fn lay(out: Vec3) -> Quat {
    let up = (Vec3::Z - out * out.z).normalize();
    Quat::from_mat3(&Mat3::from_cols(up.cross(out), up, out))
}

/// Where place `k` (from the right) of `places` sits on the line, in digit
/// heights from the readout's middle.
fn place_x(k: usize, places: usize) -> f32 {
    ASPECT * (1.0 + GAP) * ((places as f32 - 1.0) * 0.5 - k as f32)
}

/// A readout under `view` (a hand's gun) for `arm`'s gun, if it carries one.
fn spawn(commands: &mut Commands, kit: &CounterKit, mats: &mut Assets<StandardMaterial>, view: Entity, arm: u8) {
    let (Some(&(_, at, out, size)), Some(g)) = (PLACES.iter().find(|(a, ..)| *a == arm), gun(arm)) else {
        return;
    };
    let out = out.normalize();
    let at = at + out * LIFT;
    // Its middle in the hand's frame: the model's, its grip at the origin,
    // turned as the gun is (`guns::view_basis`).
    let p = view_basis() * (Vec3::new(at.x, at.y - g.height * g.grip, at.z) * g.scale);
    let root = commands
        .spawn((
            Counter { arm, shown: None },
            Transform::from_translation(p)
                .with_rotation(view_basis() * lay(out))
                .with_scale(Vec3::splat(size * g.scale)),
            Visibility::Hidden,
            ChildOf(view),
        ))
        .id();
    for k in 0..PLACES_MAX as u8 {
        let m = mats.add(StandardMaterial {
            base_color: light(false),
            base_color_texture: Some(kit.face.clone()),
            uv_transform: cell(0),
            unlit: true,
            alpha_mode: AlphaMode::Add,
            double_sided: true,
            cull_mode: None,
            fog_enabled: false,
            ..default()
        });
        commands.spawn((
            Digit(k),
            Mesh3d(kit.quad.clone()),
            MeshMaterial3d(m),
            // Places run right to left about the middle (`draw` centres
            // them on as many as show).
            Transform::from_xyz(place_x(usize::from(k), 2), 0.0, 0.0),
            Visibility::Hidden,
            NotShadowCaster,
            ChildOf(root),
        ));
    }
}

/// Every frame after `draw_view`: a readout for each hand's gun as it is
/// drawn, showing that hand's magazine.
#[allow(clippy::type_complexity)]
pub fn draw(
    link: NonSend<crate::LinkRes>,
    kit: Option<Res<CounterKit>>,
    hands: Query<(Entity, &ViewGun, Option<&Children>)>,
    mut counters: Query<(&mut Counter, &mut Visibility, &Children), Without<Digit>>,
    mut digits_q: Query<
        (
            &Digit,
            &MeshMaterial3d<StandardMaterial>,
            &mut Visibility,
            &mut Transform,
        ),
        Without<Counter>,
    >,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    let Some(kit) = kit else {
        return;
    };
    let f = link.0.session().core.own().0;
    for (e, v, children) in &hands {
        let (left, drawn) = (v.is_left(), v.drawn());
        let mine = children.and_then(|c| c.iter().find(|c| counters.contains(*c)));
        let Some(c) = mine else {
            if PLACES.iter().any(|(a, ..)| *a == drawn) {
                spawn(&mut commands, &kit, &mut mats, e, drawn);
            }
            continue;
        };
        let Ok((mut counter, mut vis, kids)) = counters.get_mut(c) else {
            continue;
        };
        if counter.arm != drawn {
            commands.entity(c).despawn();
            continue;
        }
        let g = if left { &f.left } else { f.gun() };
        if g.arm != drawn {
            vis.set_if_neq(Visibility::Hidden);
            continue;
        }
        vis.set_if_neq(Visibility::Inherited);
        let full = sim::fighter::Gun::full(drawn).mag.max(1);
        let low = f32::from(g.mag) <= f32::from(full) * LOW_SHARE;
        if counter.shown == Some((g.mag, low)) {
            continue;
        }
        counter.shown = Some((g.mag, low));
        let ds = digits(g.mag);
        // Two places at least, so a 7 reads as 07 and the readout keeps its
        // width as the magazine empties.
        let places = ds.len().max(2.min(digits(full).len()));
        for kid in kids.iter() {
            let Ok((d, m, mut dv, mut t)) = digits_q.get_mut(kid) else {
                continue;
            };
            let k = usize::from(d.0);
            if k >= places {
                dv.set_if_neq(Visibility::Hidden);
                continue;
            }
            dv.set_if_neq(Visibility::Inherited);
            t.translation.x = place_x(k, places);
            if let Some(mat) = mats.get_mut(&m.0) {
                mat.uv_transform = cell(ds.get(k).copied().unwrap_or(0));
                mat.base_color = light(low);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_number_reads_right_to_left() {
        assert_eq!(digits(0), vec![0]);
        assert_eq!(digits(7), vec![7]);
        assert_eq!(digits(36), vec![6, 3]);
        assert_eq!(digits(120), vec![0, 2, 1]);
        assert_eq!(digits(4321), vec![1, 2, 3]);
    }

    /// Every readout is for a gun there is, within the model's length,
    /// facing out of it.
    #[test]
    fn every_readout_sits_on_its_gun() {
        for (arm, at, out, size) in PLACES {
            let g = gun(arm).expect("a gun");
            assert!(at.y > 0.0 && at.y < g.height, "{}", g.file);
            assert!((out.length() - 1.0).abs() < 0.01, "{}", g.file);
            assert!(size > 0.004 && size < 0.05, "{}", g.file);
        }
    }

    /// Laid on a display, a digit faces out of it, stands up and reads
    /// left to right for one looking at it: on the left flank front to
    /// back, on the AR's sight left to right from behind.
    #[test]
    fn a_readout_lies_on_its_display() {
        let q = lay(Vec3::NEG_X);
        assert!((q * Vec3::Z - Vec3::NEG_X).length() < 1e-5);
        assert!((q * Vec3::Y - Vec3::Z).length() < 1e-5);
        assert!((q * Vec3::X - Vec3::NEG_Y).length() < 1e-5);
        let q = lay(Vec3::NEG_Y);
        assert!((q * Vec3::X - Vec3::X).length() < 1e-5);
    }

    #[test]
    fn the_places_are_centred() {
        assert!(place_x(0, 1).abs() < 1e-6);
        assert!((place_x(0, 2) + place_x(1, 2)).abs() < 1e-6);
        assert!(place_x(1, 3).abs() < 1e-6);
    }
}

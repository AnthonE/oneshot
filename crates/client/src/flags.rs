//! Capture the Flag's flags: a steel pole with a cap, and a banner in the
//! side's colour that is real cloth (`look/cloth.rs`), streaming on the
//! map's wind and on the air its pole moves through; a stand and a ring at
//! each base. At home or where it fell it stands planted; carried, it is
//! over its carrier's shoulder (ours in our view, held across it as Halo
//! 3's was), and not drawn while a hidden enemy carries it. Read from the
//! session; what a flag does is the sim's (`World::flags_tick`).

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::image::Image;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use sim::geom::V3;
use sim::world::{FLAG_DOWN, FLAG_HELD};

use crate::look::cloth::{self, Air, Cloth, NY};

/// The pole: its height and radius; the cap on its top.
const POLE_H: f32 = 2.4;
const POLE_R: f32 = 0.028;
const CAP_R: f32 = 0.06;
/// The banner: how far it flies out from the pole and how deep it hangs,
/// and how far under the cap its top edge is tied.
const CLOTH_W: f32 = 1.15;
const CLOTH_H: f32 = 0.7;
const TIE: f32 = 0.08;
/// The banner's texture, texels.
const TEX: (u32, u32) = (192, 120);
/// A base's ring (radius the sim's reach) and how much it glows; its stand.
const RING_GLOW: f32 = 2.0;
const STAND: (f32, f32) = (0.32, 0.1);
/// A flag on the floor leans this much off upright (planted where it fell).
const DOWN_LEAN: f32 = 0.22;
/// Carried by someone we see: the pole's foot from their feet (forward,
/// up, right) and the way it leans (back over the left shoulder).
const CARRY_FOOT: Vec3 = Vec3::new(0.18, 0.85, 0.24);
const CARRY_LEAN: Vec3 = Vec3::new(-0.6, 1.0, -0.35);
/// Carried by us, in our view: the foot from the eye (right, up, forward)
/// and the lean (right, up, forward), so it crosses the view from the
/// bottom right with its banner up at the left, as Halo 3 held it.
const VIEW_FOOT: Vec3 = Vec3::new(0.65, -0.8, 0.55);
const VIEW_LEAN: Vec3 = Vec3::new(-0.45, 1.0, 0.42);
/// The wind a banner feels, m/s, from the map's (`Mood::wind_ms`, the
/// clouds' drift): a breeze at the least, so it flies on every map in the
/// open, and more the windier the map; a breath under a roof.
const WIND_BASE: f32 = 4.5;
const WIND_PER: f32 = 0.9;
const WIND_MAX: f32 = 11.0;
const WIND_ROOFED: f32 = 0.7;

/// One flag's parts and its sheet.
struct One {
    root: Entity,
    sheet: Entity,
    mesh: Handle<Mesh>,
    cloth: Cloth,
    pos: Vec<[f32; 3]>,
    nor: Vec<[f32; 3]>,
}

/// The link (`front::LinkGen`) and welcome the flags were made for, and
/// each flag's parts.
#[derive(Resource, Default)]
pub struct Flags {
    built: Option<(u32, u32)>,
    ones: [Option<One>; 2],
}

fn side(t: usize) -> Color {
    if t == 0 {
        Color::srgb(0.78, 0.09, 0.07)
    } else {
        Color::srgb(0.08, 0.26, 0.85)
    }
}

/// The banner's face: the side's field, darker at the sleeve round the
/// pole and at the hem, a pale band near the fly, and a ring with a
/// chevron in it. Drawn from a few lines of geometry, so there is no art
/// to license.
fn banner(t: usize) -> Image {
    let (w, h) = TEX;
    let c = side(t).to_srgba();
    let field = [c.red, c.green, c.blue];
    let pale = [0.93, 0.92, 0.88];
    let mut px = Vec::with_capacity((w * h * 4) as usize);
    let aspect = CLOTH_W / CLOTH_H;
    for y in 0..h {
        for x in 0..w {
            let u = (x as f32 + 0.5) / w as f32;
            let v = (y as f32 + 0.5) / h as f32;
            // A soft edge: coverage over about a texel.
            let px_u = 1.0 / w as f32;
            let cover = |d: f32| (0.5 - d / (1.5 * px_u)).clamp(0.0, 1.0);
            let mut col = field;
            // Shading down the field, and the sleeve and the hem.
            let shade = 1.0 - 0.12 * v;
            let sleeve = u < 0.07;
            let hem = !(0.035..=0.965).contains(&v) || u > 0.985;
            let k = if sleeve {
                0.55
            } else if hem {
                0.72
            } else {
                shade
            };
            for ch in col.iter_mut() {
                *ch *= k;
            }
            // Two pale bands near the fly.
            for b in [0.84, 0.9] {
                let d = (u - b).abs() - 0.018;
                let a = cover(d) * (!hem) as u8 as f32;
                for (ch, p) in col.iter_mut().zip(pale) {
                    *ch += (p - *ch) * a;
                }
            }
            // The ring and the chevron, centred on the field's middle.
            let (cx, cy) = (0.44, 0.5);
            let du = (u - cx) * aspect;
            let dv = v - cy;
            let r = (du * du + dv * dv).sqrt();
            let ring = (r - 0.33).abs() - 0.035;
            // A chevron pointing up: two bars meeting at its tip.
            let tip = (0.0, -0.17);
            let bar = |sx: f32| {
                let (ax, ay) = tip;
                let (bx, by) = (sx * 0.2, 0.12);
                let (px, py) = (du - ax, dv - ay);
                let (ex, ey) = (bx - ax, by - ay);
                let s = ((px * ex + py * ey) / (ex * ex + ey * ey)).clamp(0.0, 1.0);
                let (qx, qy) = (px - ex * s, py - ey * s);
                (qx * qx + qy * qy).sqrt() - 0.05
            };
            let chevron = bar(-1.0).min(bar(1.0));
            let a = cover(ring.min(chevron) / aspect);
            for (ch, p) in col.iter_mut().zip(pale) {
                *ch += (p - *ch) * a;
            }
            px.extend(col.map(|ch| (ch.clamp(0.0, 1.0) * 255.0) as u8));
            px.push(255);
        }
    }
    Image::new(
        Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        px,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

/// The sheet's mesh, its positions and normals rewritten every frame.
fn sheet_mesh() -> Mesh {
    let (idx, uv) = cloth::grid();
    let n = uv.len();
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; n])
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 0.0, 1.0]; n])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
        .with_inserted_indices(Indices::U16(idx))
}

/// Where a pole is: its foot, the unit way up it, and the floor under it.
struct Pole {
    foot: Vec3,
    up: Vec3,
    floor: f32,
}

fn v(p: V3) -> Vec3 {
    Vec3::new(p.x, p.y, p.z)
}

/// What the flags are drawn from: the match's welcome (its bases), or the
/// pose mode's one flag.
const POSED: (u32, u32) = (u32::MAX, u32::MAX);

#[allow(clippy::too_many_arguments)]
pub fn draw(
    link: NonSend<crate::LinkRes>,
    mut flags: ResMut<Flags>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut placed: Query<(&mut Transform, &mut Visibility), Without<crate::Eye>>,
    eye: Query<&Transform, With<crate::Eye>>,
    seen: Res<crate::Seen>,
    mood: Res<crate::look::mood::MoodNow>,
    time: Res<Time>,
    (film, posed): (Option<Res<crate::film::Film>>, Option<Res<crate::pose::Pose>>),
    gen: Option<Res<crate::front::LinkGen>>,
) {
    // The wind the banners feel.
    let m = &mood.mood;
    let ms = if mood.roofed {
        WIND_ROOFED
    } else {
        (WIND_BASE + m.wind_ms * WIND_PER).min(WIND_MAX)
    };
    let w2 = m.wind();
    let wind = Vec3::new(w2.x, 0.0, w2.y) * ms;
    // Where each flag's pole is this frame (None: not drawn), the bases
    // and what they were made for.
    let mut poles: [Option<Pole>; 2] = [None, None];
    let carried = |pos: V3, yaw: f32| {
        let rot = Quat::from_rotation_y(-yaw);
        Pole {
            foot: v(pos) + rot * CARRY_FOOT,
            up: (rot * CARRY_LEAN).normalize(),
            floor: pos.y,
        }
    };
    let planted = |at: V3, down: bool| Pole {
        foot: v(at),
        // Planted where it fell, it leans downwind a little.
        up: if down {
            (Vec3::Y + Vec3::new(w2.x, 0.0, w2.y) * DOWN_LEAN).normalize()
        } else {
            Vec3::Y
        },
        floor: at.y,
    };
    // Held across our view: the eye's own axes.
    let in_view = |e: &Transform, floor: f32| {
        let (r, u, fw) = (e.right().as_vec3(), e.up().as_vec3(), e.forward().as_vec3());
        let at = |k: Vec3| r * k.x + u * k.y + fw * k.z;
        Pole {
            foot: e.translation + at(VIEW_FOOT),
            up: at(VIEW_LEAN).normalize(),
            floor,
        }
    };
    let (key, bases): ((u32, u32), [Option<V3>; 2]) =
        if let Some((p, (at, held))) = posed.as_ref().and_then(|p| p.2.flag.map(|f| (p, f))) {
            // The pose mode: red's flag, planted on its stand, over the posed
            // fighter's shoulder, or across the camera's view.
            let at = V3::new(at.x, at.y, at.z);
            poles[0] = match (held, eye.single().ok()) {
                (0, _) => Some(planted(at, false)),
                (2, Some(e)) => Some(in_view(e, at.y)),
                _ => Some(carried(p.0.pos, p.0.yaw)),
            };
            (POSED, [Some(at), None])
        } else {
            let s = link.0.session();
            if !s.ready {
                return;
            }
            let core = &s.core;
            let Some(bases) = core.map.flags.filter(|_| core.rules.ctf) else {
                return;
            };
            let snap = core.latest();
            let me = core.slot;
            let third = film.is_some_and(|f| f.third());
            for (t, f) in snap.flags().iter().enumerate().take(2) {
                // Planted at its base or where it fell, over its carrier's
                // shoulder, or across our view; None while the carrier is
                // hidden from us.
                poles[t] = if f.state == FLAG_HELD {
                    if f.carrier == me {
                        let d = core.own_drawn();
                        match eye.single().ok() {
                            _ if !d.alive => None,
                            Some(e) if !third => Some(in_view(e, d.pos.y)),
                            _ => Some(carried(d.pos, d.yaw)),
                        }
                    } else {
                        seen.others
                            .iter()
                            .find(|d| d.slot == f.carrier && d.alive)
                            .map(|d| carried(d.pos, d.yaw))
                    }
                } else {
                    Some(planted(f.pos, f.state == FLAG_DOWN))
                };
            }
            // Keyed as the level is (`Built`): two CTF matches running
            // are both on welcome 1.
            ((gen.map_or(0, |g| g.0), s.welcomes), bases.map(Some))
        };
    if Some(key) != flags.built {
        // The level's parts go with the level (`level::build`).
        flags.built = Some(key);
        let steel = mats.add(StandardMaterial {
            base_color: Color::srgb(0.72, 0.73, 0.76),
            metallic: 0.85,
            perceptual_roughness: 0.3,
            ..default()
        });
        let dark = mats.add(StandardMaterial {
            base_color: Color::srgb(0.16, 0.17, 0.19),
            metallic: 0.7,
            perceptual_roughness: 0.45,
            ..default()
        });
        let pole = meshes.add(Cylinder::new(POLE_R, POLE_H));
        let cap = meshes.add(Sphere::new(CAP_R));
        let stand = meshes.add(Cylinder::new(STAND.0, STAND.1));
        flags.ones = [None, None];
        for (t, b) in bases.iter().enumerate() {
            let Some(b) = b else { continue };
            let c = side(t);
            let glow = mats.add(StandardMaterial {
                base_color: c,
                emissive: LinearRgba::from(c) * RING_GLOW,
                ..default()
            });
            commands.spawn((
                crate::level::LevelPart,
                Mesh3d(meshes.add(Torus::new(sim::consts::FLAG_REACH - 0.08, sim::consts::FLAG_REACH))),
                MeshMaterial3d(glow.clone()),
                Transform::from_xyz(b.x, b.y + 0.04, b.z),
                bevy::light::NotShadowCaster,
            ));
            // The stand stays when the flag is away: where to bring it.
            commands
                .spawn((
                    crate::level::LevelPart,
                    Mesh3d(stand.clone()),
                    MeshMaterial3d(dark.clone()),
                    Transform::from_xyz(b.x, b.y + STAND.1 / 2.0, b.z),
                ))
                .with_child((
                    Mesh3d(meshes.add(Torus::new(STAND.0 * 0.55, STAND.0 * 0.7))),
                    MeshMaterial3d(glow),
                    Transform::from_xyz(0.0, STAND.1 / 2.0 + 0.005, 0.0),
                    bevy::light::NotShadowCaster,
                ));
            let root = commands
                .spawn((
                    crate::level::LevelPart,
                    Transform::from_xyz(b.x, b.y, b.z),
                    Visibility::default(),
                ))
                .id();
            commands.spawn((
                Mesh3d(pole.clone()),
                MeshMaterial3d(steel.clone()),
                Transform::from_xyz(0.0, POLE_H / 2.0, 0.0),
                ChildOf(root),
            ));
            commands.spawn((
                Mesh3d(cap.clone()),
                MeshMaterial3d(steel.clone()),
                Transform::from_xyz(0.0, POLE_H + CAP_R * 0.5, 0.0),
                ChildOf(root),
            ));
            let mesh = meshes.add(sheet_mesh());
            let face = images.add(banner(t));
            let sheet = commands
                .spawn((
                    crate::level::LevelPart,
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(mats.add(StandardMaterial {
                        base_color_texture: Some(face),
                        emissive: LinearRgba::from(c) * 0.12,
                        perceptual_roughness: 0.85,
                        reflectance: 0.3,
                        // Lit from behind, the sun comes through it.
                        diffuse_transmission: 0.35,
                        double_sided: true,
                        cull_mode: None,
                        ..default()
                    })),
                    // Its vertices are world space, moved every frame.
                    Transform::IDENTITY,
                    Visibility::Hidden,
                    NoFrustumCulling,
                ))
                .id();
            flags.ones[t] = Some(One {
                root,
                sheet,
                mesh,
                cloth: Cloth::new(CLOTH_W, CLOTH_H),
                pos: Vec::new(),
                nor: Vec::new(),
            });
        }
    }

    let dt = time.delta_secs();
    let flags = &mut *flags;
    for (t, pole) in poles.into_iter().enumerate() {
        let Some(one) = flags.ones[t].as_mut() else {
            continue;
        };
        let shown = if pole.is_some() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if let Ok((_, mut vis)) = placed.get_mut(one.sheet) {
            vis.set_if_neq(shown);
        }
        let Ok((mut tr, mut vis)) = placed.get_mut(one.root) else {
            continue;
        };
        vis.set_if_neq(shown);
        let Some(pole) = pole else {
            continue;
        };
        let rot = Quat::from_rotation_arc(Vec3::Y, pole.up);
        if tr.translation != pole.foot || tr.rotation != rot {
            tr.translation = pole.foot;
            tr.rotation = rot;
        }
        // The banner's hoist edge, tied down the pole from under its cap;
        // if it has to be hung again, it flies downwind.
        let top = pole.foot + pole.up * (POLE_H - TIE);
        let dy = CLOTH_H / (NY - 1) as f32;
        let pins: [Vec3; NY] = std::array::from_fn(|j| top - pole.up * (dy * j as f32));
        let fly = (wind - pole.up * wind.dot(pole.up)).normalize_or(Vec3::X);
        one.cloth.step(
            dt,
            &pins,
            fly,
            Air {
                wind,
                floor: pole.floor + 0.02,
            },
        );
        one.cloth.write(&mut one.pos, &mut one.nor);
        if let Some(mesh) = meshes.get_mut(&one.mesh) {
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, one.pos.clone());
            mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, one.nor.clone());
        }
    }
}

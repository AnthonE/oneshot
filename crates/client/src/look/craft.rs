//! The one-seaters (`sim::vehicle::SKIMMER`, `FLIER`), the Veil's and our
//! own designs: `models/skimmer.glb` and `models/flier.glb`, each grown in
//! Blender by rule to the sim's hull (`tools/models/skimmer.py`, `flier.py`,
//! on `veil.py`'s and `craft.py`'s kits; `tools/craft.sh` is their loop, the
//! `craft` skill). The skimmer is a sled: two runner hulls, a gun in each
//! up-curled prow and an engine's mouth in each stern, its rider astride the
//! saddle between them, his hands on its horns. The flier is a swift: a
//! beak, a glass blister, scythe wings, a forked tail with its engine's
//! mouth between the prongs, its pilot unseen under the glass. Each is a
//! child of the entity `draw_vehicles` stands at the sim's hull (turned by
//! its yaw); its body here takes the flier's nose and roll from the sim, and
//! leans into a turn and bobs on its lift as looks of its own.
//!
//! **Its shell is a wing case.** A craft carries no map: its colours are its
//! mesh's (the paint in the shade each vertex stands in). Its paints are
//! `StandardMaterial` plus a fragment stage (`shaders/veil_shell.wgsl`): the
//! violet turns blue edge on, and the sky's photograph the guns reflect
//! (`gun_skin.rs`) lies in its coat. Its engines burn brighter while it
//! boosts and throw a flame, a short one at speed and a long one boosting,
//! and it lights the ground under and behind it. Looks only: what flies is
//! the sim's.

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::gltf::GltfMaterialName;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::scene::SceneInstanceReady;
use bevy::shader::ShaderRef;
use core::f32::consts::TAU;
use sim::vehicle::{FLIER, SKIMMER};

use super::gun_skin::GunEnv;
use super::guns::Models;

/// Where a craft's body turns about, over its hull's origin (the ground
/// under it): the skimmer's middle, the flier's fuselage.
const SKIM_PIVOT_Y: f32 = 0.6;
const FLY_PIVOT_Y: f32 = 0.75;
/// The lean into a turn, radians a radian a second of turning, at most;
/// how fast it eases there (1/s); the flier leans this share of it.
const LEAN_K: f32 = 0.16;
const LEAN_MAX: f32 = 0.4;
const LEAN_EASE: f32 = 6.0;
const FLY_LEAN_K: f32 = 0.7;
/// The skimmer's bob on its lift: metres, and cycles a second.
const BOB_M: f32 = 0.035;
const BOB_HZ: f32 = 0.9;
/// Each engine's flame while it boosts: where its mouth ends, in the
/// hull's frame (x forward, y up from the ground under it, z right),
/// measured on the model (its sheet's numbers, `traces/craft/<name>.json`),
/// then how long it burns, how wide and how tall, metres (no bigger than
/// the lit mouth it leaves, so the rim round it still shows). The skimmer's
/// two are its hulls' sterns; the flier's three lie side by side across its
/// one wide mouth, a sheet of flame.
const SKIM_FLAMES: [(Vec3, f32, f32, f32); 2] = [
    (Vec3::new(-1.5, 0.54, -0.6), 1.0, 0.26, 0.2),
    (Vec3::new(-1.5, 0.54, 0.6), 1.0, 0.26, 0.2),
];
const FLY_FLAMES: [(Vec3, f32, f32, f32); 3] = [
    (Vec3::new(-1.72, 0.69, -0.21), 1.3, 0.34, 0.2),
    (Vec3::new(-1.72, 0.69, 0.21), 1.3, 0.34, 0.2),
    (Vec3::new(-1.72, 0.69, 0.0), 1.7, 0.4, 0.22),
];
/// A flame's core inside its sheath: this share of its length and of its
/// width.
const CORE: (f32, f32) = (0.55, 0.42);
/// The light a craft throws on what is under and behind it (on a rung with
/// lights to spare, `fx::Caps::lights_in_flight`): where it hangs in the
/// hull's frame, its lumens at rest and boosting, and how far it reaches,
/// metres. The skimmer's is its lift's, under its bridge; the flier's its
/// engines', lit by the boost alone.
const SKIM_LIGHT: (Vec3, f32, f32) = (Vec3::new(-0.5, 0.3, 0.0), 70_000.0, 240_000.0);
const FLY_LIGHT: (Vec3, f32, f32) = (Vec3::new(-2.3, 0.69, 0.0), 0.0, 320_000.0);
const LIGHT_M: f32 = 5.0;
/// The skimmer's grips, left then right, in the hull's frame: where its
/// rider's hands go (`Craft::grips`), measured on the model (its bars as
/// `tools/models/skimmer.py` raised them on their stalk).
const SKIM_GRIPS: [Vec3; 2] = [Vec3::new(-0.18, 1.24, -0.285), Vec3::new(-0.18, 1.24, 0.285)];
/// The trails of light a craft draws behind it at speed: where each leaves
/// it, in the hull's frame (the tips of the flier's tail blades and of the
/// skimmer's fins, a forked wake; measured on the models), then how wide it
/// is, metres, and how long it lasts, seconds.
const FLY_TRAILS: ([Vec3; 2], f32, f32) = ([Vec3::new(-2.27, 1.19, -0.86), Vec3::new(-2.27, 1.19, 0.86)], 0.07, 0.5);
const SKIM_TRAILS: ([Vec3; 2], f32, f32) = ([Vec3::new(-1.42, 1.0, -0.44), Vec3::new(-1.42, 1.0, 0.44)], 0.05, 0.3);
/// A trail is this many points; it is drawn from `TRAIL_FROM` m/s and whole
/// by `TRAIL_FULL`, and fades along its length as this power of what is
/// left of it.
const TRAIL_N: usize = 18;
const TRAIL_FROM: f32 = 9.0;
const TRAIL_FULL: f32 = 20.0;
const TRAIL_FADE: f32 = 1.6;

/// A craft's paint with the wing case's stage over it.
pub type ShellMaterial = ExtendedMaterial<StandardMaterial, Shell>;
pub const SHADER: &str = "shaders/veil_shell.wgsl";

/// The stage's uniform; each lane is named in `veil_shell.wgsl`.
#[derive(Clone, Copy, Debug, PartialEq, ShaderType)]
pub struct ShellParams {
    pub edge: Vec4,
    pub sky: Vec4,
}

/// The extension. Not `#[bindless]`, as the truck's isn't.
#[derive(Asset, AsBindGroup, TypePath, Clone, Debug)]
pub struct Shell {
    #[uniform(100)]
    pub params: ShellParams,
    #[texture(101, dimension = "cube")]
    #[sampler(102)]
    pub env: Handle<Image>,
}

impl MaterialExtension for Shell {
    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }
    /// Forward only: this client never runs deferred.
    fn deferred_fragment_shader() -> ShaderRef {
        ShaderRef::Default
    }
}

/// What a shell's paint turns toward edge on (linear), and how far.
const EDGE: Vec4 = Vec4::new(0.035, 0.18, 0.56, 0.42);
/// The wing case over each of a craft's paints, by its name in the GLB
/// (`veil.py`'s `PAINTS`): how far it turns at the edge, how bright the
/// sky's photograph is in it, the photograph's blur (levels), how much of
/// it shows face on, and how much of its colour it keeps. A paint not named
/// (a light, a saddle's pad) is drawn as it came.
fn shell_of(name: &str) -> Option<ShellParams> {
    let (turn, sky) = match name {
        "shell" | "shell_pale" => (1.0, Vec4::new(0.38, 1.2, 0.05, 0.6)),
        "shell_deep" => (0.8, Vec4::new(0.32, 1.4, 0.05, 0.6)),
        "glass" => (0.0, Vec4::new(0.58, 0.0, 0.09, 0.8)),
        "iron" => (0.0, Vec4::new(0.14, 2.6, 0.04, 0.3)),
        _ => return None,
    };
    Some(ShellParams {
        edge: EDGE * Vec4::new(1.0, 1.0, 1.0, turn),
        sky,
    })
}
/// The paints of an engine's fire (the lit throat and its core): each craft
/// has its own, since it burns brighter while that craft boosts.
const BURNS: [&str; 2] = ["throat", "glow"];
/// How bright an engine burns: at rest, at speed, boosting (its paint's own
/// light is 1); how fast it comes to it (1/s); and its breath at rest, this
/// share either way, cycles a second.
const BURN: (f32, f32, f32) = (0.55, 1.0, 2.4);
const BURN_EASE: f32 = 5.0;
const BREATH: (f32, f32) = (0.12, 0.45);

/// Every craft's shells, one a paint: the same violet on every skimmer is
/// one material.
#[derive(Resource, Default)]
struct Shells(HashMap<AssetId<StandardMaterial>, Handle<ShellMaterial>>);

pub fn plugin(app: &mut App) {
    app.add_plugins(MaterialPlugin::<ShellMaterial>::default())
        .init_resource::<Shells>()
        .add_systems(Startup, kit)
        .add_systems(Update, pose);
}

/// A boost's flame: its shape, and the paint of its sheath and of its core.
#[derive(Resource)]
pub struct Kit {
    flame: Handle<Mesh>,
    sheath: Handle<StandardMaterial>,
    core: Handle<StandardMaterial>,
    trail: Handle<StandardMaterial>,
}

/// The flame's mesh: rings along its length, sides round it; how narrow it
/// leaves its mouth (a share of its width) and how far along it is full;
/// how fast it fades to its tip (a power of what is left), and how far
/// along it fades in from nothing at its mouth.
const FLAME_RINGS: usize = 14;
const FLAME_SIDES: usize = 14;
const FLAME_MOUTH: f32 = 0.55;
const FLAME_FULL: f32 = 0.16;
const FLAME_FADE: f32 = 3.0;
const FLAME_IN: f32 = 0.06;
/// Its flicker: this share of its length, at these two rates (radians a
/// second), so it never beats.
const FLICKER: f32 = 0.09;
const FLICKER_HZ: [f32; 2] = [61.0, 37.0];

/// A flame one long and one wide, from its mouth at the origin aft along
/// -x: a spindle that leaves its mouth narrow, fills, and thins to nothing
/// at its tip. The fade is its vertices' alpha: its paint adds, and adds
/// less of what has less, so it has no edge and no point.
fn flame() -> Mesh {
    let (mut positions, mut normals, mut colors) = (Vec::new(), Vec::new(), Vec::new());
    let mut indices = Vec::new();
    for i in 0..=FLAME_RINGS {
        let t = i as f32 / FLAME_RINGS as f32;
        let fill = FLAME_MOUTH + (1.0 - FLAME_MOUTH) * (t / FLAME_FULL).min(1.0);
        let r = 0.5 * fill * (1.0 - t).powf(0.8);
        let alpha = (1.0 - t).powf(FLAME_FADE) * (t / FLAME_IN).min(1.0);
        for j in 0..FLAME_SIDES {
            let (sin, cos) = (j as f32 / FLAME_SIDES as f32 * TAU).sin_cos();
            positions.push([-t, r * cos, r * sin]);
            normals.push([0.0, cos, sin]);
            colors.push([1.0, 1.0, 1.0, alpha]);
        }
        if i < FLAME_RINGS {
            for j in 0..FLAME_SIDES {
                let (a, b) = (
                    (i * FLAME_SIDES + j) as u32,
                    (i * FLAME_SIDES + (j + 1) % FLAME_SIDES) as u32,
                );
                let n = FLAME_SIDES as u32;
                indices.extend([a, b, b + n, a, b + n, a + n]);
            }
        }
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
        .with_inserted_indices(Indices::U32(indices))
}

fn kit(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    // Seen from inside as from out, both walls adding: a sheath thin enough
    // to see the nozzle's ring through, a core past white, so it blooms.
    let paint = |color: LinearRgba| StandardMaterial {
        base_color: color.into(),
        alpha_mode: AlphaMode::Add,
        unlit: true,
        double_sided: true,
        cull_mode: None,
        ..default()
    };
    commands.insert_resource(Kit {
        flame: meshes.add(flame()),
        sheath: mats.add(paint(LinearRgba::new(0.25, 1.1, 1.6, 0.7))),
        core: mats.add(paint(LinearRgba::new(3.0, 4.0, 4.5, 0.9))),
        trail: mats.add(paint(LinearRgba::new(0.35, 1.5, 2.2, 0.55))),
    });
}

/// One trail of light: its ribbon, where it leaves the craft (the hull's
/// frame), and the points it runs back through, newest first: where each
/// was in the world, the craft's right there, when it was left and how
/// bright.
struct Trail {
    ribbon: Entity,
    mesh: Handle<Mesh>,
    from: Vec3,
    points: Vec<(Vec3, Vec3, f32, f32)>,
}

/// A trail's ribbon before it has run anywhere: two strips crossed, flat
/// and on edge, so it shows from any side; every point at the origin.
fn ribbon() -> Mesh {
    let n = TRAIL_N * 4;
    let mut indices = Vec::with_capacity((TRAIL_N - 1) * 12);
    for i in 0..(TRAIL_N - 1) as u32 {
        for strip in [0, 2] {
            let (a, b) = (i * 4 + strip, (i + 1) * 4 + strip);
            indices.extend([a, a + 1, b, a + 1, b + 1, b]);
        }
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; n])
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 1.0, 0.0]; n])
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, vec![[1.0f32, 1.0, 1.0, 0.0]; n])
        .with_inserted_indices(Indices::U32(indices))
}

/// A drawn one-seater: which kind, what the sim says of its nose, roll and
/// boost this frame (`draw_vehicles` sets them), and its looks' own state.
#[derive(Component)]
pub struct Craft {
    pub kind: u8,
    pub pitch: f32,
    pub roll: f32,
    pub boosting: bool,
    pub yaw: f32,
    body: Entity,
    exhaust: Entity,
    /// Its flames, and how long each is.
    flames: Vec<(Entity, f32)>,
    /// Its light, and its lumens at rest and boosting.
    light: (Entity, f32, f32),
    /// Its engines' paints, its own (`BURNS`), and the light each came with.
    burns: Vec<(Handle<StandardMaterial>, LinearRgba)>,
    /// How bright they burn now (`BURN`), and how fast it goes, m/s.
    burn: f32,
    speed: f32,
    last_at: Option<Vec3>,
    /// Its trails, how wide and how long-lived.
    trails: Vec<Trail>,
    trail: (f32, f32),
    lean: f32,
    last_yaw: Option<f32>,
    t: f32,
}

/// A craft's model: whose hull it is, for when its scene is in.
#[derive(Component)]
struct Skin(Entity);

impl Craft {
    /// How its body sits this frame, over its hull's origin: turned by the
    /// flier's nose and roll and by its lean, the skimmer bobbing.
    fn body_at(&self) -> Transform {
        let (pivot, bob) = match self.kind {
            SKIMMER => (SKIM_PIVOT_Y, (self.t * BOB_HZ * TAU).sin() * BOB_M),
            _ => (FLY_PIVOT_Y, 0.0),
        };
        let turn = Quat::from_rotation_z(self.pitch) * Quat::from_rotation_x(self.roll + self.lean);
        Transform::from_xyz(0.0, pivot + bob, 0.0).with_rotation(turn)
    }

    /// Where a skimmer's grips are this frame, left then right, in its
    /// rider's own frame (the sim's seat, turned as the hull is): they lean
    /// and bob with it, and the rider's hands with them.
    pub fn grips(&self) -> [Vec3; 2] {
        let seat =
            sim::vehicle::Vehicle::new_kind(sim::geom::V3::default(), 0, 0, 0, SKIMMER).mount(sim::vehicle::DRIVER);
        let body = self.body_at();
        SKIM_GRIPS.map(|g| body.transform_point(g - Vec3::Y * SKIM_PIVOT_Y) - Vec3::new(seat.x, seat.y, seat.z))
    }
}

/// A one-seater of `kind` as a child of `hull`, an entity at the sim's
/// hull turned by its yaw.
pub fn spawn(commands: &mut Commands, meshes: &mut Assets<Mesh>, kit: &Kit, models: &Models, hull: Entity, kind: u8) {
    let (scene, pivot, burns, (lamp, rest, boost), (tails, wide, lasts)) = match kind {
        FLIER => (&models.flier, FLY_PIVOT_Y, &FLY_FLAMES[..], FLY_LIGHT, FLY_TRAILS),
        _ => (&models.skimmer, SKIM_PIVOT_Y, &SKIM_FLAMES[..], SKIM_LIGHT, SKIM_TRAILS),
    };
    let body = commands
        .spawn((
            Transform::from_xyz(0.0, pivot, 0.0),
            Visibility::default(),
            ChildOf(hull),
        ))
        .id();
    // The model is in the hull's frame, its origin on the ground under the
    // hull's middle.
    commands
        .spawn((
            SceneRoot(scene.clone()),
            Skin(hull),
            Transform::from_xyz(0.0, -pivot, 0.0),
            ChildOf(body),
        ))
        .observe(bind);
    // The boost's flames, one a nozzle, shown while it boosts.
    let exhaust = commands
        .spawn((Transform::default(), Visibility::Hidden, ChildOf(body)))
        .id();
    let mut flames = Vec::with_capacity(burns.len());
    for &(at, len, wide, tall) in burns {
        let e = commands
            .spawn((
                Mesh3d(kit.flame.clone()),
                MeshMaterial3d(kit.sheath.clone()),
                Transform {
                    translation: at - Vec3::Y * pivot,
                    scale: Vec3::new(len, tall, wide),
                    ..default()
                },
                NotShadowCaster,
                NotShadowReceiver,
                ChildOf(exhaust),
            ))
            .id();
        // Its core, inside it and flickering with it.
        commands.spawn((
            Mesh3d(kit.flame.clone()),
            MeshMaterial3d(kit.core.clone()),
            Transform::from_scale(Vec3::new(CORE.0, CORE.1, CORE.1)),
            NotShadowCaster,
            NotShadowReceiver,
            ChildOf(e),
        ));
        flames.push((e, len));
    }
    let light = commands
        .spawn((
            PointLight {
                color: Color::linear_rgb(0.2, 0.8, 1.0),
                intensity: 0.0,
                range: LIGHT_M,
                shadows_enabled: false,
                ..default()
            },
            Transform::from_translation(lamp - Vec3::Y * pivot),
            ChildOf(body),
        ))
        .id();
    // Its trails: ribbons of the hull's own, redrawn each frame from where
    // the craft has been.
    let trails = tails
        .iter()
        .map(|&from| {
            let mesh = meshes.add(ribbon());
            let ribbon = commands
                .spawn((
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(kit.trail.clone()),
                    Transform::default(),
                    Visibility::Hidden,
                    NoFrustumCulling,
                    NotShadowCaster,
                    NotShadowReceiver,
                    ChildOf(hull),
                ))
                .id();
            Trail {
                ribbon,
                mesh,
                from,
                points: Vec::with_capacity(TRAIL_N),
            }
        })
        .collect();
    commands.entity(hull).insert(Craft {
        kind,
        pitch: 0.0,
        roll: 0.0,
        boosting: false,
        yaw: 0.0,
        body,
        exhaust,
        flames,
        light: (light, rest, boost),
        burns: Vec::new(),
        burn: BURN.0,
        speed: 0.0,
        last_at: None,
        trails,
        trail: (wide, lasts),
        lean: 0.0,
        last_yaw: None,
        t: 0.0,
    });
}

/// The model is in: its paints under the wing case (one material a paint,
/// whichever craft wears it), its engines' in materials of this craft's own.
#[allow(clippy::too_many_arguments)]
fn bind(
    ev: On<SceneInstanceReady>,
    skins: Query<&Skin>,
    children: Query<&Children>,
    plain: Query<(&MeshMaterial3d<StandardMaterial>, &GltfMaterialName)>,
    warming: Query<(), With<super::prewarm::Warming>>,
    mut crafts: Query<&mut Craft>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    (mut shells, mut worn, env): (ResMut<Shells>, ResMut<Assets<ShellMaterial>>, Res<GunEnv>),
    mut commands: Commands,
) {
    let Ok(&Skin(hull)) = skins.get(ev.entity) else {
        return;
    };
    let mut burns: Vec<(AssetId<StandardMaterial>, Handle<StandardMaterial>, LinearRgba)> = Vec::new();
    for e in children.iter_descendants(ev.entity) {
        let Ok((mat, name)) = plain.get(e) else { continue };
        if let Some(params) = shell_of(&name.0) {
            let shell = match shells.0.get(&mat.0.id()) {
                Some(h) => h.clone(),
                None => {
                    let Some(base) = mats.get(&mat.0) else { continue };
                    let h = worn.add(ShellMaterial {
                        base: base.clone(),
                        extension: Shell {
                            params,
                            env: env.0.clone(),
                        },
                    });
                    shells.0.insert(mat.0.id(), h.clone());
                    h
                }
            };
            commands
                .entity(e)
                .remove::<MeshMaterial3d<StandardMaterial>>()
                .insert(MeshMaterial3d(shell));
        } else if BURNS.contains(&name.0.as_str()) {
            let own = match burns.iter().find(|(id, ..)| *id == mat.0.id()) {
                Some((_, h, _)) => h.clone(),
                None => {
                    let Some(base) = mats.get(&mat.0).cloned() else {
                        continue;
                    };
                    let lit = base.emissive;
                    let h = mats.add(base);
                    burns.push((mat.0.id(), h.clone(), lit));
                    h
                }
            };
            commands.entity(e).insert(MeshMaterial3d(own));
        }
    }
    if let Ok(mut c) = crafts.get_mut(hull) {
        c.burns = burns.into_iter().map(|(_, h, lit)| (h, lit)).collect();
    }
    // A warm craft's scene is in: its warm draw may count down (`prewarm.rs`).
    if warming.contains(hull) {
        commands.entity(hull).insert(super::prewarm::SceneDone);
    }
}

/// How fast a craft must go for its engines to throw a flame, m/s, the
/// speed that flame is longest at short of a boost, and how long it is
/// then: a share of the boost's.
const FLAME_FROM: f32 = 4.0;
const FLAME_TOP: f32 = 17.0;
const FLAME_CRUISE: f32 = 0.34;
/// How fast the speed a craft is drawn by follows the hull, 1/s; a hull that
/// moved further than this in a frame was put there (a respawn), not flown.
const SPEED_EASE: f32 = 6.0;
const JUMP_M: f32 = 6.0;

/// Each frame: the body turned by the flier's nose and roll, leaning into
/// its turn, the skimmer bobbing on its lift; its engines as bright as it
/// is going, their flames as long, flickering; its light at rest or
/// boosting.
#[allow(clippy::too_many_arguments)]
fn pose(
    time: Res<Time>,
    fx: Option<Res<super::fx::Fx>>,
    mut crafts: Query<(Entity, &mut Craft)>,
    mut at: Query<&mut Transform>,
    mut vis: Query<&mut Visibility>,
    mut lights: Query<&mut PointLight>,
    (mut mats, mut meshes): (ResMut<Assets<StandardMaterial>>, ResMut<Assets<Mesh>>),
) {
    let dt = time.delta_secs().max(1e-4);
    let lit = fx.is_some_and(|fx| fx.caps.lights_in_flight());
    for (hull, mut c) in crafts.iter_mut() {
        c.t += dt;
        let turning = c.last_yaw.map_or(0.0, |was| {
            let d = (c.yaw - was + core::f32::consts::PI).rem_euclid(core::f32::consts::TAU) - core::f32::consts::PI;
            d / dt
        });
        c.last_yaw = Some(c.yaw);
        let k = if c.kind == FLIER { FLY_LEAN_K } else { 1.0 };
        let want = (turning * LEAN_K * k).clamp(-LEAN_MAX, LEAN_MAX);
        c.lean += (want - c.lean) * (1.0 - (-LEAN_EASE * dt).exp());
        if let Ok(mut t) = at.get_mut(c.body) {
            t.set_if_neq(c.body_at());
        }
        // How fast it goes, by where its hull was a frame ago.
        if let Ok(here) = at.get(hull).map(|t| t.translation) {
            let moved = c.last_at.map_or(0.0, |was| here.distance(was));
            c.last_at = Some(here);
            let going = if moved < JUMP_M { moved / dt } else { 0.0 };
            c.speed += (going - c.speed) * (1.0 - (-SPEED_EASE * dt).exp());
        }
        let cruise = ((c.speed - FLAME_FROM) / (FLAME_TOP - FLAME_FROM)).clamp(0.0, 1.0);
        // Its engines: a breath at rest, full at speed, past it boosting.
        let breath = 1.0 + BREATH.0 * (c.t * BREATH.1 * TAU).sin();
        let want = if c.boosting {
            BURN.2
        } else {
            (BURN.0 + (BURN.1 - BURN.0) * cruise) * breath
        };
        c.burn += (want - c.burn) * (1.0 - (-BURN_EASE * dt).exp());
        for (h, own) in &c.burns {
            let glow = *own * c.burn;
            if mats
                .get(h)
                .is_some_and(|m| (m.emissive.green - glow.green).abs() > 0.02 * own.green)
            {
                if let Some(m) = mats.get_mut(h) {
                    m.emissive = glow;
                }
            }
        }
        // Their flames: none at rest, short at speed, long boosting, each a
        // little out of step with the other.
        let reach = if c.boosting { 1.0 } else { FLAME_CRUISE * cruise };
        for (k, &(e, len)) in c.flames.iter().enumerate() {
            let phase = c.t + k as f32 * 0.37;
            let wave = (phase * FLICKER_HZ[0]).sin() * 0.6 + (phase * FLICKER_HZ[1]).sin() * 0.4;
            if let Ok(mut t) = at.get_mut(e) {
                t.scale.x = (len * reach * (1.0 + FLICKER * wave)).max(1e-3);
            }
        }
        // Its trails: each runs back through where its end of the craft has
        // been, fading as it goes, drawn in the hull's own frame.
        if let Ok(stand) = at.get(hull).copied() {
            let (body, pivot) = (
                c.body_at(),
                Vec3::Y * if c.kind == FLIER { FLY_PIVOT_Y } else { SKIM_PIVOT_Y },
            );
            let right = stand.rotation * (body.rotation * Vec3::Z);
            let up = stand.rotation * (body.rotation * Vec3::Y);
            let bright = ((c.speed - TRAIL_FROM) / (TRAIL_FULL - TRAIL_FROM)).clamp(0.0, 1.0);
            let (wide, lasts) = c.trail;
            let (now, speed) = (c.t, c.speed);
            let back = stand.compute_affine().inverse();
            for trail in c.trails.iter_mut() {
                let head = stand.transform_point(body.transform_point(trail.from - pivot));
                // A point is left behind each stretch of the way; the first
                // rides with the craft.
                let stretch = (speed * lasts / (TRAIL_N - 2) as f32).max(0.25);
                if trail.points.len() < 2 || trail.points[1].0.distance(head) > stretch {
                    trail.points.insert(0, (head, right, now, bright));
                    trail.points.truncate(TRAIL_N);
                }
                trail.points[0] = (head, right, now, bright);
                let lit = trail.points.iter().any(|p| p.3 > 0.01 && now - p.2 < lasts);
                if let Ok(mut v) = vis.get_mut(trail.ribbon) {
                    v.set_if_neq(if lit { Visibility::Inherited } else { Visibility::Hidden });
                }
                if !lit {
                    continue;
                }
                let Some(mesh) = meshes.get_mut(&trail.mesh) else {
                    continue;
                };
                let (mut positions, mut colors) = (Vec::with_capacity(TRAIL_N * 4), Vec::with_capacity(TRAIL_N * 4));
                for i in 0..TRAIL_N {
                    let &(p, side, born, lit) = trail.points.get(i).unwrap_or(trail.points.last().unwrap());
                    let left = (1.0 - (now - born) / lasts).clamp(0.0, 1.0);
                    let alpha = if i < trail.points.len() {
                        lit * left.powf(TRAIL_FADE)
                    } else {
                        0.0
                    };
                    // It thins as it fades.
                    let half = wide * 0.5 * (0.35 + 0.65 * left);
                    for off in [side * -half, side * half, up * -half, up * half] {
                        positions.push(back.transform_point3(p + off).to_array());
                        colors.push([1.0, 1.0, 1.0, alpha]);
                    }
                }
                mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
                mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
            }
        }
        let (lamp, rest, boost) = c.light;
        let lumens = match (lit, c.boosting) {
            (false, _) => 0.0,
            (true, false) => rest,
            (true, true) => boost,
        };
        if let Ok(mut l) = lights.get_mut(lamp) {
            if l.intensity != lumens {
                l.intensity = lumens;
            }
        }
        if let Ok(mut v) = vis.get_mut(c.exhaust) {
            v.set_if_neq(if reach > 0.02 {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            });
        }
    }
}

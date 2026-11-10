//! The one-seaters (`sim::vehicle::SKIMMER`, `FLIER`), the Veil's and our
//! own designs: `models/skimmer.glb` and `models/flier.glb`, each
//! commissioned from Meshy by the prompt in its script and cut in Blender
//! to the sim's hull (`tools/models/skimmer.py`, `flier.py`, on `craft.py`'s
//! kit; `tools/craft.sh` is their loop, the `craft` skill). The skimmer is
//! a catamaran of a hover bike: two blade hulls, a gun in each prow and a
//! thruster in each stern, its rider astride the bridge between them. The
//! flier is a wedge of a lifting body with short swept wings, a gun pod
//! under each, its pilot unseen under the canopy. Each is a child of the
//! entity `draw_vehicles` stands at the sim's hull (turned by its yaw); its
//! body here takes the flier's nose and roll from the sim, and leans into a
//! turn and bobs on its lift as looks of its own. Its thrusters throw a
//! flame while it boosts, and it lights the ground under and behind it.
//! Looks only: what flies is the sim's.

use bevy::asset::RenderAssetUsages;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use core::f32::consts::TAU;
use sim::vehicle::{FLIER, SKIMMER};

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
/// Each thruster's flame while it boosts: where its nozzle ends, in the
/// hull's frame (x forward, y up from the ground under it, z right),
/// measured on the model (its sheet's numbers, `traces/craft/<name>.json`),
/// then how long and how wide it burns, metres (no wider than the lit
/// throat it leaves, so the ring round it still shows). The two outboard,
/// then the one on the centre line.
const SKIM_FLAMES: [(Vec3, f32, f32); 3] = [
    (Vec3::new(-1.5, 0.43, -0.5), 1.0, 0.15),
    (Vec3::new(-1.5, 0.43, 0.5), 1.0, 0.15),
    (Vec3::new(-1.28, 0.56, 0.0), 0.6, 0.11),
];
const FLY_FLAMES: [(Vec3, f32, f32); 3] = [
    (Vec3::new(-2.15, 0.635, -0.47), 1.2, 0.27),
    (Vec3::new(-2.15, 0.635, 0.47), 1.2, 0.27),
    (Vec3::new(-1.84, 0.7, 0.0), 0.8, 0.24),
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
const FLY_LIGHT: (Vec3, f32, f32) = (Vec3::new(-2.5, 0.65, 0.0), 0.0, 320_000.0);
const LIGHT_M: f32 = 5.0;
/// The skimmer's grips, left then right, in the hull's frame: where its
/// rider's hands go (`Craft::grips`), measured on the model (its bars as
/// `tools/models/skimmer.py` raised them on their stalk).
const SKIM_GRIPS: [Vec3; 2] = [Vec3::new(-0.18, 1.24, -0.285), Vec3::new(-0.18, 1.24, 0.285)];

pub fn plugin(app: &mut App) {
    app.add_systems(Startup, kit).add_systems(Update, pose);
}

/// A boost's flame: its shape, and the paint of its sheath and of its core.
#[derive(Resource)]
pub struct Kit {
    flame: Handle<Mesh>,
    sheath: Handle<StandardMaterial>,
    core: Handle<StandardMaterial>,
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
    });
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
    flames: [(Entity, f32); 3],
    /// Its light, and its lumens at rest and boosting.
    light: (Entity, f32, f32),
    lean: f32,
    last_yaw: Option<f32>,
    t: f32,
}

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
pub fn spawn(commands: &mut Commands, kit: &Kit, models: &Models, hull: Entity, kind: u8) {
    let (scene, pivot, burns, (lamp, rest, boost)) = match kind {
        FLIER => (&models.flier, FLY_PIVOT_Y, &FLY_FLAMES, FLY_LIGHT),
        _ => (&models.skimmer, SKIM_PIVOT_Y, &SKIM_FLAMES, SKIM_LIGHT),
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
    commands.spawn((
        SceneRoot(scene.clone()),
        Transform::from_xyz(0.0, -pivot, 0.0),
        ChildOf(body),
    ));
    // The boost's flames, one a nozzle, shown while it boosts.
    let exhaust = commands
        .spawn((Transform::default(), Visibility::Hidden, ChildOf(body)))
        .id();
    let flames = burns.map(|(at, len, wide)| {
        let e = commands
            .spawn((
                Mesh3d(kit.flame.clone()),
                MeshMaterial3d(kit.sheath.clone()),
                Transform {
                    translation: at - Vec3::Y * pivot,
                    scale: Vec3::new(len, wide, wide),
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
        (e, len)
    });
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
        lean: 0.0,
        last_yaw: None,
        t: 0.0,
    });
}

/// Each frame: the body turned by the flier's nose and roll, leaning into
/// its turn, the skimmer bobbing on its lift; the boost's flames on or off,
/// flickering while they are on; its light at rest or boosting.
fn pose(
    time: Res<Time>,
    fx: Option<Res<super::fx::Fx>>,
    mut crafts: Query<&mut Craft>,
    mut at: Query<&mut Transform>,
    mut vis: Query<&mut Visibility>,
    mut lights: Query<&mut PointLight>,
) {
    let dt = time.delta_secs().max(1e-4);
    let lit = fx.is_some_and(|fx| fx.caps.lights_in_flight());
    for mut c in crafts.iter_mut() {
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
        if c.boosting {
            // The flames flicker, each a little out of step with the other.
            for (k, &(e, len)) in c.flames.iter().enumerate() {
                let phase = c.t + k as f32 * 0.37;
                let wave = (phase * FLICKER_HZ[0]).sin() * 0.6 + (phase * FLICKER_HZ[1]).sin() * 0.4;
                if let Ok(mut t) = at.get_mut(e) {
                    t.scale.x = len * (1.0 + FLICKER * wave);
                }
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
            v.set_if_neq(if c.boosting {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            });
        }
    }
}

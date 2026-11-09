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
//! flame while it boosts. Looks only: what flies is the sim's.

use bevy::prelude::*;
use core::f32::consts::FRAC_PI_2;
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
/// Where each thruster's nozzle ends, in the hull's frame (x forward, y up
/// from the ground under it, z right), measured on the model (its sheet's
/// numbers, `traces/craft/<name>.json`); and a boost's flame from it, how
/// long and how wide, metres.
const SKIM_NOZZLES: [Vec3; 2] = [Vec3::new(-1.5, 0.43, -0.485), Vec3::new(-1.5, 0.43, 0.485)];
const SKIM_FLAME: (f32, f32) = (1.0, 0.2);
const FLY_NOZZLES: [Vec3; 2] = [Vec3::new(-2.15, 0.635, -0.445), Vec3::new(-2.15, 0.635, 0.445)];
const FLY_FLAME: (f32, f32) = (1.6, 0.34);

pub fn plugin(app: &mut App) {
    app.add_systems(Startup, kit).add_systems(Update, pose);
}

/// A boost's flame: its shape and its paint.
#[derive(Resource)]
pub struct Kit {
    cone: Handle<Mesh>,
    boost: Handle<StandardMaterial>,
}

fn kit(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    commands.insert_resource(Kit {
        // A unit cone along y, scaled per flame.
        cone: meshes.add(Cone::new(0.5, 1.0)),
        boost: mats.add(StandardMaterial {
            base_color: Color::srgba(0.4, 0.9, 1.0, 0.7),
            emissive: LinearRgba::rgb(2.0, 7.0, 10.0),
            alpha_mode: AlphaMode::Add,
            unlit: true,
            ..default()
        }),
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
    lean: f32,
    last_yaw: Option<f32>,
    t: f32,
}

/// Along -x (a cone's tip aft).
fn aft() -> Quat {
    Quat::from_rotation_z(FRAC_PI_2)
}

/// A one-seater of `kind` as a child of `hull`, an entity at the sim's
/// hull turned by its yaw.
pub fn spawn(commands: &mut Commands, kit: &Kit, models: &Models, hull: Entity, kind: u8) {
    let (scene, pivot, nozzles, (len, wide)) = match kind {
        FLIER => (&models.flier, FLY_PIVOT_Y, &FLY_NOZZLES, FLY_FLAME),
        _ => (&models.skimmer, SKIM_PIVOT_Y, &SKIM_NOZZLES, SKIM_FLAME),
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
    for n in nozzles {
        commands.spawn((
            Mesh3d(kit.cone.clone()),
            MeshMaterial3d(kit.boost.clone()),
            Transform {
                translation: Vec3::new(n.x - len * 0.5, n.y - pivot, n.z),
                rotation: aft(),
                scale: Vec3::new(wide, len, wide),
            },
            ChildOf(exhaust),
        ));
    }
    commands.entity(hull).insert(Craft {
        kind,
        pitch: 0.0,
        roll: 0.0,
        boosting: false,
        yaw: 0.0,
        body,
        exhaust,
        lean: 0.0,
        last_yaw: None,
        t: 0.0,
    });
}

/// Each frame: the body turned by the flier's nose and roll, leaning into
/// its turn, the skimmer bobbing on its lift; the boost's flame on or off.
fn pose(
    time: Res<Time>,
    mut crafts: Query<&mut Craft>,
    mut at: Query<&mut Transform>,
    mut vis: Query<&mut Visibility>,
) {
    let dt = time.delta_secs().max(1e-4);
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
        let (pivot, bob) = match c.kind {
            SKIMMER => (SKIM_PIVOT_Y, (c.t * BOB_HZ * core::f32::consts::TAU).sin() * BOB_M),
            _ => (FLY_PIVOT_Y, 0.0),
        };
        let turn = Quat::from_rotation_z(c.pitch) * Quat::from_rotation_x(c.roll + c.lean);
        if let Ok(mut t) = at.get_mut(c.body) {
            t.set_if_neq(Transform::from_xyz(0.0, pivot + bob, 0.0).with_rotation(turn));
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

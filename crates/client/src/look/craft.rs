//! The one-seaters (`sim::vehicle::SKIMMER`, `FLIER`), drawn in plain
//! shapes: our own designs, no model of anyone's. The skimmer is a hover
//! bike on two thruster pods, its rider astride it; the flier a swept-wing
//! attack craft with a gun pod at each wingtip, its pilot unseen under the
//! canopy. Each is a child of the entity `draw_vehicles` stands at the
//! sim's hull (turned by its yaw); its body here takes the flier's nose
//! and roll from the sim, and leans into a turn and bobs on its lift as
//! looks of its own. Its engines glow brighter while it boosts. Looks
//! only: what flies is the sim's.

use bevy::prelude::*;
use core::f32::consts::FRAC_PI_2;
use sim::vehicle::{FLIER, SKIMMER};

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

pub fn plugin(app: &mut App) {
    app.add_systems(Startup, kit).add_systems(Update, pose);
}

/// The shapes and paints both are built from.
#[derive(Resource)]
pub struct Kit {
    capsule: Handle<Mesh>,
    cone: Handle<Mesh>,
    tube: Handle<Mesh>,
    block: Handle<Mesh>,
    ball: Handle<Mesh>,
    hull: Handle<StandardMaterial>,
    trim: Handle<StandardMaterial>,
    glass: Handle<StandardMaterial>,
    glow: Handle<StandardMaterial>,
    boost: Handle<StandardMaterial>,
}

fn kit(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    commands.insert_resource(Kit {
        // Unit shapes, scaled per part: a capsule and a cone along y, a
        // tube along y, a cube, a ball.
        capsule: meshes.add(Capsule3d::new(0.5, 1.0)),
        cone: meshes.add(Cone::new(0.5, 1.0)),
        tube: meshes.add(Cylinder::new(0.5, 1.0)),
        block: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        ball: meshes.add(Sphere::new(0.5)),
        hull: mats.add(StandardMaterial {
            base_color: Color::srgb(0.16, 0.18, 0.21),
            metallic: 0.8,
            perceptual_roughness: 0.38,
            ..default()
        }),
        trim: mats.add(StandardMaterial {
            base_color: Color::srgb(0.62, 0.36, 0.18),
            metallic: 0.9,
            perceptual_roughness: 0.3,
            ..default()
        }),
        glass: mats.add(StandardMaterial {
            base_color: Color::srgb(0.04, 0.06, 0.08),
            metallic: 0.9,
            perceptual_roughness: 0.08,
            emissive: LinearRgba::rgb(0.25, 0.12, 0.02),
            ..default()
        }),
        glow: mats.add(StandardMaterial {
            base_color: Color::srgb(0.2, 0.8, 1.0),
            emissive: LinearRgba::rgb(0.6, 3.0, 4.0),
            unlit: true,
            ..default()
        }),
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

/// What a part is made of.
#[derive(Clone, Copy)]
enum Shape {
    Capsule,
    Cone,
    Tube,
    Block,
    Ball,
}

#[derive(Clone, Copy)]
enum Paint {
    Hull,
    Trim,
    Glass,
    Glow,
}

/// A part: its shape and paint, where its middle is (in the hull's frame:
/// x forward, y up, z right), its size along its own axes (a capsule's,
/// cone's and tube's length is its y), and its turn.
struct Part {
    shape: Shape,
    paint: Paint,
    at: Vec3,
    size: Vec3,
    turn: Quat,
}

/// Along x (a y-long shape laid nose forward).
fn fore() -> Quat {
    Quat::from_rotation_z(-FRAC_PI_2)
}

/// Along -x (a cone's tip aft).
fn aft() -> Quat {
    Quat::from_rotation_z(FRAC_PI_2)
}

fn p(shape: Shape, paint: Paint, at: [f32; 3], size: [f32; 3], turn: Quat) -> Part {
    Part {
        shape,
        paint,
        at: Vec3::from(at),
        size: Vec3::from(size),
        turn,
    }
}

/// The skimmer: a bike body on two thruster pods, a cowl to the nose, the
/// twin guns under it, a saddle and bars, a fin, and the pods' glow under.
fn skimmer() -> Vec<Part> {
    use Paint::*;
    use Shape::*;
    let mut parts = vec![
        // The body: a capsule 2.3 m nose to tail, low and wide.
        p(Capsule, Hull, [0.0, 0.62, 0.0], [0.62, 2.3, 1.0], fore()),
        // The cowl, tapering to the nose.
        p(Cone, Hull, [1.28, 0.66, 0.0], [0.5, 0.5, 0.7], fore()),
        // The saddle and the bars.
        p(Block, Trim, [-0.35, 0.98, 0.0], [0.7, 0.12, 0.42], Quat::IDENTITY),
        p(Block, Hull, [0.25, 1.02, 0.0], [0.08, 0.08, 0.78], Quat::IDENTITY),
        // A fin over the tail.
        p(
            Block,
            Trim,
            [-1.15, 0.98, 0.0],
            [0.55, 0.32, 0.05],
            Quat::from_rotation_z(0.35),
        ),
    ];
    for z in [-0.6f32, 0.6] {
        // A thruster pod either side, its glow under it and aft.
        parts.push(p(Tube, Hull, [-0.1, 0.42, z], [0.36, 1.9, 0.36], fore()));
        parts.push(p(Ball, Glow, [-0.1, 0.22, z], [1.5, 0.08, 0.26], Quat::IDENTITY));
        parts.push(p(Ball, Glow, [-1.06, 0.42, z], [0.06, 0.26, 0.26], Quat::IDENTITY));
        // A gun under the cowl.
        parts.push(p(Tube, Trim, [1.25, 0.5, z * 0.7], [0.08, 0.7, 0.08], fore()));
    }
    parts
}

/// The flier: a fuselage with a canopy, two wings swept back with a gun pod
/// at each tip, twin fins canted out, and the engine's glow aft.
fn flier() -> Vec<Part> {
    use Paint::*;
    use Shape::*;
    let mut parts = vec![
        p(Capsule, Hull, [0.0, 0.75, 0.0], [0.95, 3.2, 1.1], fore()),
        p(Cone, Hull, [1.75, 0.75, 0.0], [0.7, 0.5, 0.8], fore()),
        p(Ball, Glass, [0.65, 1.12, 0.0], [1.7, 0.6, 0.85], Quat::IDENTITY),
        p(Ball, Glow, [-1.95, 0.75, 0.0], [0.2, 0.55, 0.8], Quat::IDENTITY),
        p(Cone, Hull, [-1.75, 0.75, 0.0], [0.7, 0.35, 0.8], aft()),
    ];
    for side in [-1.0f32, 1.0] {
        // The wing from the fuselage out to its tip, swept back.
        let sweep = Quat::from_rotation_y(-0.38 * side);
        parts.push(p(Block, Hull, [-0.45, 0.72, 1.4 * side], [1.4, 0.14, 2.1], sweep));
        parts.push(p(Block, Trim, [-0.15, 0.74, 1.4 * side], [0.12, 0.16, 2.0], sweep));
        // The gun pod at its tip, a barrel ahead of it.
        parts.push(p(Tube, Hull, [-0.75, 0.72, 2.45 * side], [0.26, 1.5, 0.26], fore()));
        parts.push(p(Tube, Trim, [0.2, 0.72, 2.45 * side], [0.09, 0.55, 0.09], fore()));
        parts.push(p(
            Ball,
            Glow,
            [-1.5, 0.72, 2.45 * side],
            [0.06, 0.18, 0.18],
            Quat::IDENTITY,
        ));
        // A fin over the tail, canted out.
        parts.push(p(
            Block,
            Trim,
            [-1.7, 1.25, 0.32 * side],
            [0.85, 0.62, 0.06],
            Quat::from_rotation_x(0.35 * side) * Quat::from_rotation_z(0.3),
        ));
    }
    parts
}

/// A one-seater of `kind` as a child of `hull`, an entity at the sim's
/// hull turned by its yaw.
pub fn spawn(commands: &mut Commands, kit: &Kit, hull: Entity, kind: u8) {
    let (parts, pivot) = match kind {
        FLIER => (flier(), FLY_PIVOT_Y),
        _ => (skimmer(), SKIM_PIVOT_Y),
    };
    let body = commands
        .spawn((
            Transform::from_xyz(0.0, pivot, 0.0),
            Visibility::default(),
            ChildOf(hull),
        ))
        .id();
    for part in parts {
        let mesh = match part.shape {
            Shape::Capsule => &kit.capsule,
            Shape::Cone => &kit.cone,
            Shape::Tube => &kit.tube,
            Shape::Block => &kit.block,
            Shape::Ball => &kit.ball,
        };
        let mat = match part.paint {
            Paint::Hull => &kit.hull,
            Paint::Trim => &kit.trim,
            Paint::Glass => &kit.glass,
            Paint::Glow => &kit.glow,
        };
        // A unit capsule is 2 long with its caps; its size is its body's.
        let size = match part.shape {
            Shape::Capsule => Vec3::new(part.size.x, part.size.y * 0.5, part.size.z),
            _ => part.size,
        };
        commands.spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(mat.clone()),
            Transform {
                translation: part.at - Vec3::Y * pivot,
                rotation: part.turn,
                scale: size,
            },
            ChildOf(body),
        ));
    }
    // The boost's flame, aft, shown while it boosts.
    let (at, len, wide) = match kind {
        FLIER => (Vec3::new(-2.9, 0.0, 0.0), 1.6, 0.6),
        _ => (Vec3::new(-1.6, -0.18, 0.0), 1.1, 0.5),
    };
    let exhaust = commands
        .spawn((
            Mesh3d(kit.cone.clone()),
            MeshMaterial3d(kit.boost.clone()),
            Transform {
                translation: at,
                rotation: aft(),
                scale: Vec3::new(wide, len, wide),
            },
            Visibility::Hidden,
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

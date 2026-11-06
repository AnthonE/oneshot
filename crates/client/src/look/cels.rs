//! A sword's lunge, drawn in stop-motion. The server drives the body and the
//! wire says how fast it goes; nothing here decides anything, and the body's
//! hits are the server's wherever it is drawn.
//!
//! While it lunges, a fighter is drawn only on each of [`STEPS_PER_S`] steps a
//! second, snapping from pose to pose. Each pose it leaves stays a moment where
//! it was ([`Pose`]'s meshes, flat foil), in a frame of light hung across its
//! path. From inside the lunge those would all be behind the eye, so its own
//! frames hang ahead and it goes through them, with hard lines at the edge of
//! the view and the lens kicked wide (`view`, in `lib.rs`).
//!
//! The same frame and pose, held longer, freeze someone the sword kills.

use std::collections::HashMap;

use bevy::camera::visibility::NoFrustumCulling;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::skinning::SkinnedMesh;
use bevy::mesh::{MeshTag, MeshVertexBufferLayoutRef};
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError};
use bevy::shader::ShaderRef;

use super::bodies::Body;
use super::paint::BodyMaterial;

pub const SHADER: &str = "shaders/cel.wgsl";

/// The steps a second a lunge is drawn on.
pub const STEPS_PER_S: f32 = 24.0;
const STEP_S: f32 = 1.0 / STEPS_PER_S;
/// How fast across the ground is a lunge: past any run, under the sword's own
/// `LUNGE_SPEED`, seen on someone else's velocity and on our own, which
/// prediction lets fall a little between snapshots.
const LUNGE_M_S: f32 = 14.0;
const OWN_LUNGE_M_S: f32 = 11.0;
/// The most a lunge lasts (`LUNGE_MS`) and then some: past this it is something
/// else throwing a fighter along.
const LONGEST_S: f32 = 0.55;
/// How long a pose and its frame last, and someone the sword killed's.
const LIFE_S: f32 = 0.3;
const KILL_LIFE_S: f32 = 0.55;
/// A frame round a body: how wide and tall, how high its middle stands, metres.
const FRAME_M: Vec2 = Vec2::new(1.25, 2.3);
const FRAME_AT_M: f32 = 1.05;
/// One ahead of the lunger's own eye: how wide and tall, how far ahead, metres.
const OWN_FRAME_M: Vec2 = Vec2::new(2.3, 1.3);
const OWN_AHEAD_M: f32 = 2.4;
/// How long those last: it has gone through them by then.
const OWN_LIFE_S: f32 = 0.2;
/// How much a frame's filled, round a body and ahead of the eye.
const FILL: f32 = 0.05;
const OWN_FILL: f32 = 0.012;
/// Their outline's light, linear, as the blade's edge.
const ICE: Vec3 = Vec3::new(0.42, 0.9, 1.15);
/// How much the lens widens at a lunge's height, and how fast that comes and
/// goes, per second.
const WIDEN: f32 = 0.14;
const KICK_RISE_PER_S: f32 = 22.0;
const KICK_FALL_PER_S: f32 = 7.0;

/// Field for field `Cel` in the shader.
#[derive(Clone, Copy, Debug, Default, ShaderType)]
pub struct CelParams {
    pub knobs: Vec4,
    pub ice: Vec4,
}

#[derive(Asset, AsBindGroup, TypePath, Clone, Debug)]
pub struct CelMaterial {
    #[uniform(0)]
    pub params: CelParams,
}

impl Material for CelMaterial {
    fn vertex_shader() -> ShaderRef {
        SHADER.into()
    }

    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Add
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }

    // Seen from either side.
    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

/// One mesh and the few materials.
#[derive(Resource, Clone)]
pub struct Kit {
    quad: Handle<Mesh>,
    frame: Handle<CelMaterial>,
    kill_frame: Handle<CelMaterial>,
    own_frame: Handle<CelMaterial>,
    pose: Handle<CelMaterial>,
    kill_pose: Handle<CelMaterial>,
}

/// The lines over the view: its tag is how strong, of 255.
#[derive(Component)]
pub struct Lines;

/// Gone at this time.
#[derive(Component)]
pub struct Gone(f32);

pub fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<CelMaterial>>) {
    let mut one = |mode: f32, life: f32, size: Vec2, fill: f32| {
        mats.add(CelMaterial {
            params: CelParams {
                knobs: Vec4::new(mode, life, size.x, size.y),
                ice: ICE.extend(fill),
            },
        })
    };
    let lines = one(2.0, 1.0, Vec2::ONE, 0.0);
    let kit = Kit {
        quad: meshes.add(Rectangle::new(1.0, 1.0)),
        frame: one(0.0, LIFE_S, FRAME_M, FILL),
        kill_frame: one(0.0, KILL_LIFE_S, FRAME_M, FILL),
        own_frame: one(0.0, OWN_LIFE_S, OWN_FRAME_M, OWN_FILL),
        pose: one(1.0, LIFE_S, Vec2::ONE, 0.0),
        kill_pose: one(1.0, KILL_LIFE_S, Vec2::ONE, 0.0),
    };
    commands.spawn((
        Lines,
        Mesh3d(kit.quad.clone()),
        MeshMaterial3d(lines),
        MeshTag(0),
        Transform::IDENTITY,
        Visibility::Hidden,
        NoFrustumCulling,
        NotShadowCaster,
        NotShadowReceiver,
    ));
    commands.insert_resource(kit);
}

/// A fighter drawn in steps: where it was drawn last, and when the next is.
#[derive(Clone, Copy, Debug, Default)]
pub struct Step {
    on: bool,
    began: f32,
    next: f32,
    held: Vec3,
}

impl Step {
    /// Whether it lunges at `pos` at time `now`, this frame: a step fell this
    /// frame (and it left where it was: the old place), and where to draw it.
    fn advance(&mut self, lunging: bool, pos: Vec3, now: f32) -> (Option<Vec3>, Option<Vec3>) {
        if !lunging {
            self.on = false;
            return (None, None);
        }
        if self.on && now - self.began > LONGEST_S {
            return (None, None);
        }
        if !self.on {
            *self = Step {
                on: true,
                began: now,
                next: now,
                held: pos,
            };
        }
        let mut left = None;
        if now >= self.next {
            left = Some(self.held);
            self.held = pos;
            self.next = now + STEP_S;
        }
        (left, Some(self.held))
    }
}

/// What the lunger's own view is doing this frame.
#[derive(Resource, Default)]
pub struct Own {
    step: Step,
    /// Where its feet are drawn while it lunges.
    pub held: Option<Vec3>,
    /// A lunge's height in the lens and the lines, 0 to 1.
    pub kick: f32,
    /// It lunges now.
    pub on: bool,
}

impl Own {
    /// The lens at this kick.
    pub fn widen(&self, fov: f32) -> f32 {
        fov * (1.0 + WIDEN * self.kick)
    }
}

/// How far the feet go across the ground a second.
fn across(v: Vec3) -> f32 {
    Vec2::new(v.x, v.z).length()
}

fn frame_at(at: Vec3, facing: Vec3, size: Vec2) -> Transform {
    let flat = Vec3::new(facing.x, 0.0, facing.z).normalize_or(Vec3::Z);
    Transform::from_translation(at)
        .with_rotation(Quat::from_rotation_arc(Vec3::Z, flat))
        .with_scale(size.extend(1.0))
}

/// Our own lunge: the view's steps, its frames, its lines.
#[allow(clippy::too_many_arguments)]
pub fn own(
    link: NonSend<crate::LinkRes>,
    look: Res<crate::Look>,
    time: Res<Time>,
    kit: Option<Res<Kit>>,
    mut own: ResMut<Own>,
    mut lines: Query<(&mut Visibility, &mut MeshTag), With<Lines>>,
    film: Option<Res<crate::film::Film>>,
    mut commands: Commands,
) {
    let Some(kit) = kit else {
        return;
    };
    let core = &link.0.session().core;
    let (f, p) = core.own();
    let v = f.body.vel;
    // A film's camera off the eye sees our body do it instead (`bodies`).
    let inside = !film.is_some_and(|f| f.third());
    let sword = inside && f.alive && f.gun().arm == sim::arms::SWORD && f.riding().is_none();
    let on = sword && across(Vec3::new(v.x, v.y, v.z)) >= OWN_LUNGE_M_S;
    let now = time.elapsed_secs();
    let feet = Vec3::new(p.x, p.y, p.z);
    let (left, held) = own.step.advance(on, feet, now);
    own.held = held;
    own.on = held.is_some();
    if left.is_some() {
        let aim = Vec3::new(
            look.pitch.cos() * look.yaw.cos(),
            look.pitch.sin(),
            look.pitch.cos() * look.yaw.sin(),
        );
        let eye = feet + Vec3::Y * f.body.eye_h();
        commands.spawn((
            Mesh3d(kit.quad.clone()),
            MeshMaterial3d(kit.own_frame.clone()),
            MeshTag(time.elapsed_secs_wrapped().to_bits()),
            Transform::from_translation(eye + aim * OWN_AHEAD_M)
                .looking_to(aim, Vec3::Y)
                .with_scale(OWN_FRAME_M.extend(1.0)),
            Gone(now + OWN_LIFE_S),
            NoFrustumCulling,
            NotShadowCaster,
            NotShadowReceiver,
        ));
    }
    let dt = time.delta_secs();
    own.kick = if own.on {
        (own.kick + KICK_RISE_PER_S * dt).min(1.0)
    } else {
        (own.kick - KICK_FALL_PER_S * dt).max(0.0)
    };
    if let Ok((mut vis, mut tag)) = lines.single_mut() {
        vis.set_if_neq(if own.kick > 0.0 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
        tag.0 = (own.kick * 255.0) as u32;
    }
}

/// Everything a body wears that a pose copies.
type Worn<'w, 's> = Query<
    'w,
    's,
    (&'static Mesh3d, Option<&'static SkinnedMesh>, &'static GlobalTransform),
    (
        Or<(
            With<MeshMaterial3d<BodyMaterial>>,
            With<MeshMaterial3d<StandardMaterial>>,
            With<MeshMaterial3d<super::gun_skin::GunMaterial>>,
            With<super::blade::Lit>,
        )>,
        Without<Gone>,
    ),
>;

/// `root`'s body as it stands, frozen: each skinned mesh on joints of its own,
/// set where its real ones are, and whatever it holds.
#[allow(clippy::too_many_arguments)]
fn freeze(
    commands: &mut Commands,
    root: Entity,
    material: &Handle<CelMaterial>,
    until: f32,
    clock: f32,
    children: &Query<&Children>,
    worn: &Worn,
    joints: &Query<&GlobalTransform>,
) {
    let mut frozen: HashMap<Entity, Entity> = HashMap::new();
    let born = MeshTag(clock.to_bits());
    for e in children.iter_descendants(root) {
        let Ok((mesh, skin, at)) = worn.get(e) else {
            continue;
        };
        let mut pose = commands.spawn((
            mesh.clone(),
            MeshMaterial3d(material.clone()),
            born.clone(),
            Gone(until),
            NoFrustumCulling,
            NotShadowCaster,
            NotShadowReceiver,
        ));
        let Some(skin) = skin else {
            pose.insert(at.compute_transform());
            continue;
        };
        pose.insert(Transform::IDENTITY);
        let pose = pose.id();
        let mut list = Vec::with_capacity(skin.joints.len());
        for j in &skin.joints {
            let copy = *frozen.entry(*j).or_insert_with(|| {
                let at = joints
                    .get(*j)
                    .map(GlobalTransform::compute_transform)
                    .unwrap_or_default();
                commands.spawn((at, Gone(until))).id()
            });
            list.push(copy);
        }
        commands.entity(pose).insert(SkinnedMesh {
            inverse_bindposes: skin.inverse_bindposes.clone(),
            joints: list,
        });
    }
}

/// Everyone else's lunge (and ours, filmed from outside): a body in steps,
/// leaving a pose in a frame at each.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn bodies(
    time: Res<Time>,
    kit: Option<Res<Kit>>,
    mut q: Query<(Entity, &mut Body, &mut Transform, &InheritedVisibility)>,
    mut fx: ResMut<super::fx::Fx>,
    children: Query<&Children>,
    worn: Worn,
    joints: Query<&GlobalTransform>,
    mut commands: Commands,
) {
    let Some(kit) = kit else {
        return;
    };
    let now = time.elapsed_secs();
    let clock = time.elapsed_secs_wrapped();
    for (root, mut body, mut at, seen) in &mut q {
        // Its cut, shadow and all.
        if fx.sword_cuts.contains(&body.slot()) {
            body.cutting = super::bodies::CUT_S;
        }
        if body.is_shadow() {
            continue;
        }
        // Someone it killed, as they last stood.
        if seen.get() || body.life().0 {
            if let Some(i) = fx.sword_kills.iter().position(|&s| s == body.slot()) {
                fx.sword_kills.swap_remove(i);
                let facing = fx.eye - at.translation;
                killed(
                    &mut commands,
                    &kit,
                    root,
                    at.translation,
                    facing,
                    now,
                    clock,
                    &children,
                    &worn,
                    &joints,
                );
            }
        }
        if !seen.get() {
            continue;
        }
        let vel = body.last_vel();
        let lunging = body.holds_sword() && body.life().0 && across(vel) >= LUNGE_M_S;
        let (left, held) = body.step.advance(lunging, at.translation, now);
        if let Some(held) = held {
            at.translation = held;
        }
        let Some(left) = left else {
            continue;
        };
        freeze(
            &mut commands,
            root,
            &kit.pose,
            now + LIFE_S,
            clock,
            &children,
            &worn,
            &joints,
        );
        commands.spawn((
            Mesh3d(kit.quad.clone()),
            MeshMaterial3d(kit.frame.clone()),
            MeshTag(clock.to_bits()),
            frame_at(left + Vec3::Y * FRAME_AT_M, vel, FRAME_M),
            Gone(now + LIFE_S),
            NoFrustumCulling,
            NotShadowCaster,
            NotShadowReceiver,
        ));
    }
    // Anyone left is not drawn here.
    fx.sword_kills.clear();
    fx.sword_cuts.clear();
}

/// Someone the sword just killed: frozen in a frame, a moment longer.
#[allow(clippy::too_many_arguments)]
pub fn killed(
    commands: &mut Commands,
    kit: &Kit,
    root: Entity,
    feet: Vec3,
    facing: Vec3,
    now: f32,
    clock: f32,
    children: &Query<&Children>,
    worn: &Worn,
    joints: &Query<&GlobalTransform>,
) {
    freeze(
        commands,
        root,
        &kit.kill_pose,
        now + KILL_LIFE_S,
        clock,
        children,
        worn,
        joints,
    );
    commands.spawn((
        Mesh3d(kit.quad.clone()),
        MeshMaterial3d(kit.kill_frame.clone()),
        MeshTag(clock.to_bits()),
        frame_at(feet + Vec3::Y * FRAME_AT_M, facing, FRAME_M),
        Gone(now + KILL_LIFE_S),
        NoFrustumCulling,
        NotShadowCaster,
        NotShadowReceiver,
    ));
}

/// What has outlived its moment.
pub fn sweep(time: Res<Time>, gone: Query<(Entity, &Gone)>, mut commands: Commands) {
    let now = time.elapsed_secs();
    for (e, g) in &gone {
        if now >= g.0 {
            commands.entity(e).try_despawn();
        }
    }
}

/// The warm body, once it stands: its frozen self in a frame, so their pipelines
/// are built before the first lunge.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn warm(
    time: Res<Time>,
    kit: Option<Res<Kit>>,
    q: Query<(Entity, &Body, &GlobalTransform), With<super::prewarm::WarmBody>>,
    children: Query<&Children>,
    worn: Worn,
    joints: Query<&GlobalTransform>,
    mut done: Local<bool>,
    mut commands: Commands,
) {
    let (false, Some(kit)) = (*done, kit) else {
        return;
    };
    for (root, body, at) in &q {
        if !body.dressed() {
            continue;
        }
        *done = true;
        let now = time.elapsed_secs();
        let clock = time.elapsed_secs_wrapped();
        killed(
            &mut commands,
            &kit,
            root,
            at.translation(),
            Vec3::Z,
            now,
            clock,
            &children,
            &worn,
            &joints,
        );
    }
}

pub fn plugin(app: &mut App) {
    app.init_resource::<Own>()
        .add_plugins(MaterialPlugin::<CelMaterial>::default());
}

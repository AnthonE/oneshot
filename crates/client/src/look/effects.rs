//! Shots, hits, blasts and what's in flight, after the three.js page's `effects.ts`:
//! a tracer for each round fog lets us see (a shot we only hear has none),
//! a spark where it ended, a blast's fireball and its light, and rockets
//! and frags drawn with their models. Drawing only.

use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use proto::{EventState, ProjState};
use sim::events::Kind;

use super::guns::Models;

/// `EventState::part` on a shot fog lets us only hear (room's `HEARD`):
/// no tracer, no spark, its place rounded.
pub const HEARD: u8 = 1;

const TRACER_MS: f32 = 90.0;
const TRACER_W: f32 = 0.025;
/// Our own tracer starts this far out, past the gun.
const OWN_TRACER_SKIP_M: f32 = 0.8;
const SPARK_MS: f32 = 120.0;
const BLAST_MS: f32 = 450.0;
const BLAST_R: f32 = 5.0;
/// A blast's light at its brightest, lumens.
const BLAST_LM: f32 = 3_000_000.0;
const BLAST_RANGE_M: f32 = 14.0;

#[derive(Resource)]
pub struct Fx {
    cube: Handle<Mesh>,
    ball: Handle<Mesh>,
}

pub fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    commands.insert_resource(Fx {
        cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        ball: meshes.add(Sphere::new(1.0).mesh().uv(16, 12)),
    });
}

#[derive(Component)]
pub struct Fade {
    age_ms: f32,
    life_ms: f32,
    what: What,
}

enum What {
    /// Fades out.
    Tracer,
    /// Grows as it fades.
    Spark,
    /// Grows to the blast's radius, its light dimming with it.
    Blast,
}

fn glow(mats: &mut Assets<StandardMaterial>, c: Color) -> Handle<StandardMaterial> {
    mats.add(StandardMaterial {
        base_color: c,
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        ..default()
    })
}

/// One frame's events: what they look like. `me` is our slot.
pub fn on_events(commands: &mut Commands, fx: &Fx, mats: &mut Assets<StandardMaterial>, events: &[EventState], me: u8) {
    for e in events {
        let from = Vec3::new(e.pos.x, e.pos.y, e.pos.z);
        let to = Vec3::new(e.end.x, e.end.y, e.end.z);
        match e.kind {
            k if k == Kind::Shot as u8 && e.part != HEARD && sim::arms::arm(e.c as u8).kind.ray() => {
                let len = from.distance(to);
                let start = if e.a == me && len > OWN_TRACER_SKIP_M {
                    from.lerp(to, OWN_TRACER_SKIP_M / len)
                } else {
                    from
                };
                let len = start.distance(to);
                if len > 0.05 {
                    commands.spawn((
                        Mesh3d(fx.cube.clone()),
                        MeshMaterial3d(glow(mats, Color::srgb(1.0, 0.91, 0.66))),
                        Transform::from_translation((start + to) * 0.5)
                            .looking_at(to, Vec3::Y)
                            .with_scale(Vec3::new(TRACER_W, TRACER_W, len)),
                        NotShadowCaster,
                        Fade {
                            age_ms: 0.0,
                            life_ms: TRACER_MS,
                            what: What::Tracer,
                        },
                    ));
                }
                commands.spawn((
                    Mesh3d(fx.ball.clone()),
                    MeshMaterial3d(glow(mats, Color::WHITE)),
                    Transform::from_translation(to).with_scale(Vec3::splat(0.09)),
                    NotShadowCaster,
                    Fade {
                        age_ms: 0.0,
                        life_ms: SPARK_MS,
                        what: What::Spark,
                    },
                ));
            }
            k if k == Kind::Blast as u8 || k == Kind::Wreck as u8 => {
                commands.spawn((
                    Mesh3d(fx.ball.clone()),
                    MeshMaterial3d(glow(mats, Color::srgb(1.0, 0.63, 0.25))),
                    Transform::from_translation(from).with_scale(Vec3::splat(0.3)),
                    NotShadowCaster,
                    PointLight {
                        intensity: BLAST_LM,
                        range: BLAST_RANGE_M,
                        color: Color::srgb(1.0, 0.7, 0.4),
                        shadows_enabled: false,
                        ..default()
                    },
                    Fade {
                        age_ms: 0.0,
                        life_ms: BLAST_MS,
                        what: What::Blast,
                    },
                ));
            }
            _ => {}
        }
    }
}

/// Ages every effect, and drops the spent.
#[allow(clippy::type_complexity)]
pub fn fade(
    mut commands: Commands,
    time: Res<Time>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut q: Query<(
        Entity,
        &mut Fade,
        &mut Transform,
        &MeshMaterial3d<StandardMaterial>,
        Option<&mut PointLight>,
    )>,
) {
    let dt = time.delta_secs() * 1000.0;
    for (e, mut f, mut t, m, light) in &mut q {
        f.age_ms += dt;
        let u = f.age_ms / f.life_ms;
        if u >= 1.0 {
            commands.entity(e).despawn();
            mats.remove(&m.0);
            continue;
        }
        match f.what {
            What::Tracer => {}
            What::Spark => t.scale = Vec3::splat(0.09 * (1.0 + 2.0 * u)),
            What::Blast => t.scale = Vec3::splat(0.3 + BLAST_R * u),
        }
        if let Some(mut l) = light {
            l.intensity = BLAST_LM * (1.0 - u) * (1.0 - u);
        }
        if let Some(mat) = mats.get_mut(&m.0) {
            mat.base_color.set_alpha(1.0 - u);
        }
    }
}

/// What's in flight, drawn with its model.
#[derive(Component)]
pub struct Flying(u8);

pub fn projectiles(
    commands: &mut Commands,
    models: &Models,
    proj: &[ProjState],
    have: &mut Query<(Entity, &Flying, &mut Transform)>,
) {
    let mut drawn: Vec<(Entity, u8)> = have.iter().map(|(e, f, _)| (e, f.0)).collect();
    // Same count of each kind as last frame, in order: move them; else
    // respawn the lot (cheap: a handful at most).
    let same = drawn.len() == proj.len() && drawn.iter().zip(proj).all(|(d, p)| d.1 == p.kind);
    if !same {
        for (e, _) in drawn.drain(..) {
            commands.entity(e).despawn();
        }
        for p in proj {
            let scene = if p.kind == 0 { &models.rocket } else { &models.frag };
            let e = commands
                .spawn((Flying(p.kind), place(p), Visibility::default()))
                .with_child((
                    SceneRoot(scene.clone()),
                    // Both models stand on their base, +Y up: centre them.
                    Transform::from_xyz(0.0, if p.kind == 0 { -0.3 } else { -0.08 }, 0.0),
                ))
                .id();
            drawn.push((e, p.kind));
        }
        return;
    }
    for ((e, _), p) in drawn.iter().zip(proj) {
        if let Ok((_, _, mut t)) = have.get_mut(*e) {
            *t = place(p);
        }
    }
}

fn place(p: &ProjState) -> Transform {
    let at = Vec3::new(p.pos.x, p.pos.y, p.pos.z);
    let v = Vec3::new(p.vel.x, p.vel.y, p.vel.z);
    let r = if p.kind == 0 && v.length_squared() > 1e-4 {
        Quat::from_rotation_arc(Vec3::Y, v.normalize())
    } else {
        Quat::IDENTITY
    };
    Transform::from_translation(at).with_rotation(r)
}

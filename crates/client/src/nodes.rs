//! Siege's nodes (docs/DESIGN.md §18): each generator a pylon in its
//! side's colour with a ring on the floor as wide as the sim's reach, each
//! core a taller one under a shield while its generators stand. A ring being
//! overloaded pulses, faster as it climbs; a broken node goes dark. Read
//! from the session; what a node does is the sim's (`World::nodes_tick`).

use bevy::prelude::*;
use sim::consts::{NODE_REACH, SIEGE_GENS, SIEGE_NODES};
use sim::world::{World, NODE_CONTESTED, NODE_FALLING, NODE_RISING};

/// A generator's pylon (radius, height) and a core's.
const GEN: (f32, f32) = (0.45, 2.6);
const CORE: (f32, f32) = (0.9, 4.2);
/// The core's shield, a dome over its pylon.
const DOME_R: f32 = 2.4;
/// How bright a quiet pylon glows, and a pulse's top.
const GLOW: f32 = 1.2;
const PULSE: f32 = 5.0;
/// Pulses a second, slow at the start of an overload and quick near its end.
const PULSE_HZ: (f32, f32) = (1.0, 4.0);

/// One node's parts: its pylon's and ring's materials, and the shield.
struct One {
    pylon: Handle<StandardMaterial>,
    ring: Handle<StandardMaterial>,
    dome: Option<Entity>,
}

#[derive(Resource, Default)]
pub struct Nodes {
    /// The welcome they were made for (none yet: 0).
    built: u32,
    ones: Vec<One>,
}

fn side(t: usize) -> Color {
    if t == 0 {
        Color::srgb(0.78, 0.09, 0.07)
    } else {
        Color::srgb(0.08, 0.26, 0.85)
    }
}

/// What a node's state reads as: whether its core's shield is up, from its
/// side's generators (all broken) or sudden death (the clock out while
/// playing), as the sim's `World::core_open`.
pub fn shielded(snap: &proto::Snapshot, n: usize) -> bool {
    if !World::is_core(n) {
        return false;
    }
    let first = World::node_side(n) * (SIEGE_GENS + 1);
    let gens_down = snap.nodes()[first..first + SIEGE_GENS].iter().all(|g| g.broken);
    let sudden = snap.phase == sim::world::Phase::Playing as u8 && snap.clock == 0;
    !(gens_down || sudden)
}

#[allow(clippy::too_many_arguments)]
pub fn draw(
    link: NonSend<crate::LinkRes>,
    mut nodes: ResMut<Nodes>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut vis: Query<&mut Visibility>,
    time: Res<Time>,
) {
    let s = link.0.session();
    if !s.ready {
        return;
    }
    let core = &s.core;
    let Some(at) = core.map.siege.filter(|_| core.rules.siege) else {
        return;
    };
    if nodes.built != s.welcomes {
        // The level's parts go with the level (`level::build`).
        nodes.built = s.welcomes;
        nodes.ones.clear();
        let dark = mats.add(StandardMaterial {
            base_color: Color::srgb(0.16, 0.17, 0.19),
            metallic: 0.7,
            perceptual_roughness: 0.45,
            ..default()
        });
        for (n, p) in at.iter().enumerate() {
            let c = side(World::node_side(n));
            let big = World::is_core(n);
            let (r, h) = if big { CORE } else { GEN };
            let pylon = mats.add(StandardMaterial {
                base_color: c,
                emissive: LinearRgba::from(c) * GLOW,
                metallic: 0.4,
                perceptual_roughness: 0.4,
                ..default()
            });
            let ring = mats.add(StandardMaterial {
                base_color: c,
                emissive: LinearRgba::from(c) * GLOW,
                ..default()
            });
            let foot = Vec3::new(p.x, p.y, p.z);
            commands.spawn((
                crate::level::LevelPart,
                Mesh3d(meshes.add(Torus::new(NODE_REACH - 0.12, NODE_REACH))),
                MeshMaterial3d(ring.clone()),
                Transform::from_translation(foot + Vec3::Y * 0.04),
                bevy::light::NotShadowCaster,
            ));
            // A dark plinth, the pylon on it.
            commands.spawn((
                crate::level::LevelPart,
                Mesh3d(meshes.add(Cylinder::new(r * 1.6, 0.3))),
                MeshMaterial3d(dark.clone()),
                Transform::from_translation(foot + Vec3::Y * 0.15),
            ));
            commands.spawn((
                crate::level::LevelPart,
                Mesh3d(meshes.add(Cylinder::new(r, h))),
                MeshMaterial3d(pylon.clone()),
                Transform::from_translation(foot + Vec3::Y * (0.3 + h / 2.0)),
            ));
            let dome = big.then(|| {
                commands
                    .spawn((
                        crate::level::LevelPart,
                        Mesh3d(meshes.add(Sphere::new(DOME_R).mesh().uv(32, 16))),
                        MeshMaterial3d(mats.add(StandardMaterial {
                            base_color: c.with_alpha(0.18),
                            emissive: LinearRgba::from(c) * 0.6,
                            alpha_mode: AlphaMode::Blend,
                            double_sided: true,
                            cull_mode: None,
                            unlit: true,
                            ..default()
                        })),
                        Transform::from_translation(foot + Vec3::Y * 1.6).with_scale(Vec3::new(1.0, 1.4, 1.0)),
                        bevy::light::NotShadowCaster,
                    ))
                    .id()
            });
            nodes.ones.push(One { pylon, ring, dome });
        }
    }

    let snap = core.latest();
    if snap.nodes().len() != SIEGE_NODES || nodes.ones.len() != SIEGE_NODES {
        return;
    }
    let t = time.elapsed_secs();
    for (n, one) in nodes.ones.iter().enumerate() {
        let st = snap.nodes()[n];
        let shield = shielded(snap, n);
        let c = LinearRgba::from(side(World::node_side(n)));
        let frac = st.load as f32 / proto::NODE_LOAD_MAX as f32;
        // Broken: dark. Being overloaded or held: a pulse, quicker as the
        // overload climbs. Wound back or quiet: its glow.
        let glow = if st.broken {
            0.0
        } else {
            match st.held {
                NODE_RISING | NODE_CONTESTED => {
                    let hz = PULSE_HZ.0 + (PULSE_HZ.1 - PULSE_HZ.0) * frac;
                    let k = 0.5 + 0.5 * (t * hz * std::f32::consts::TAU).sin();
                    GLOW + (PULSE - GLOW) * k
                }
                NODE_FALLING => GLOW * 1.5,
                _ => GLOW,
            }
        };
        let base = if st.broken {
            Color::srgb(0.12, 0.12, 0.13)
        } else {
            side(World::node_side(n))
        };
        let e = c * glow;
        for h in [&one.pylon, &one.ring] {
            // Written only when it changes: a write uploads it again.
            if mats.get(h).is_some_and(|m| m.emissive != e || m.base_color != base) {
                if let Some(m) = mats.get_mut(h) {
                    m.emissive = e;
                    m.base_color = base;
                }
            }
        }
        if let Some(d) = one.dome {
            if let Ok(mut v) = vis.get_mut(d) {
                v.set_if_neq(if shield {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                });
            }
        }
    }
}

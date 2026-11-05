//! Capture the Flag's flags: a pole and its cloth in the side's colour, a
//! ring on each base; at home or on the floor where the snapshot puts it,
//! carried over its carrier's shoulder (ours, or whoever fog shows us), and
//! not drawn while a hidden enemy carries it. Read from the session; what
//! a flag does is the sim's (`World::flags_tick`).

use bevy::prelude::*;
use sim::geom::V3;
use sim::world::FLAG_HELD;

/// The pole's height and thickness, the cloth's size, and how far over
/// and behind a carrier's feet the pole's foot rides.
const POLE_H: f32 = 2.2;
const POLE_W: f32 = 0.06;
const CLOTH: Vec3 = Vec3::new(0.9, 0.55, 0.03);
const CARRY: Vec3 = Vec3::new(-0.35, 0.6, 0.0);
/// A base's ring: radius (the sim's reach) and how much it glows.
const RING_GLOW: f32 = 2.0;

/// The welcome the flags were made for, and each flag's root.
#[derive(Resource, Default)]
pub struct Flags {
    built: u32,
    roots: [Option<Entity>; 2],
}

fn side(t: usize) -> Color {
    if t == 0 {
        Color::srgb(0.9, 0.12, 0.1)
    } else {
        Color::srgb(0.12, 0.35, 0.95)
    }
}

pub fn draw(
    link: NonSend<crate::LinkRes>,
    mut flags: ResMut<Flags>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut placed: Query<(&mut Transform, &mut Visibility)>,
    seen: Res<crate::Seen>,
) {
    let s = link.0.session();
    if !s.ready {
        return;
    }
    let core = &s.core;
    let Some(bases) = core.map.flags.filter(|_| core.rules.ctf) else {
        return;
    };
    if s.welcomes != flags.built {
        // The level's parts go with the level (`level::build`).
        flags.built = s.welcomes;
        for (t, b) in bases.iter().enumerate() {
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
            let root = commands
                .spawn((
                    crate::level::LevelPart,
                    Transform::from_xyz(b.x, b.y, b.z),
                    Visibility::default(),
                ))
                .id();
            commands.spawn((
                Mesh3d(meshes.add(Cuboid::new(POLE_W, POLE_H, POLE_W))),
                MeshMaterial3d(mats.add(StandardMaterial {
                    base_color: Color::srgb(0.75, 0.75, 0.78),
                    metallic: 0.8,
                    perceptual_roughness: 0.35,
                    ..default()
                })),
                Transform::from_xyz(0.0, POLE_H / 2.0, 0.0),
                ChildOf(root),
            ));
            commands.spawn((
                Mesh3d(meshes.add(Cuboid::new(CLOTH.x, CLOTH.y, CLOTH.z))),
                MeshMaterial3d(glow),
                Transform::from_xyz(CLOTH.x / 2.0, POLE_H - CLOTH.y / 2.0, 0.0),
                ChildOf(root),
            ));
            flags.roots[t] = Some(root);
        }
    }
    let snap = core.latest();
    let me = core.slot;
    let others = &seen.others;
    for (t, f) in snap.flags().iter().enumerate().take(2) {
        let Some(root) = flags.roots[t] else {
            continue;
        };
        let Ok((mut tr, mut vis)) = placed.get_mut(root) else {
            continue;
        };
        // Where it is: the snapshot's place, or on its carrier's shoulder,
        // turned with them; None while the carrier is hidden from us.
        let at: Option<(V3, f32)> = if f.state == FLAG_HELD {
            if f.carrier == me {
                let d = core.own_drawn();
                d.alive.then_some((d.pos, -d.yaw))
            } else {
                others
                    .iter()
                    .find(|d| d.slot == f.carrier && d.alive)
                    .map(|d| (d.pos, -d.yaw))
            }
        } else {
            Some((f.pos, 0.0))
        };
        match at {
            Some((p, yaw)) => {
                let rot = Quat::from_rotation_y(yaw);
                let off = if f.state == FLAG_HELD { rot * CARRY } else { Vec3::ZERO };
                let at = Vec3::new(p.x, p.y, p.z) + off;
                // A flag at home or lying still isn't moved again.
                if tr.translation != at || tr.rotation != rot {
                    tr.translation = at;
                    tr.rotation = rot;
                }
                vis.set_if_neq(Visibility::Inherited);
            }
            None => {
                vis.set_if_neq(Visibility::Hidden);
            }
        }
    }
}

//! The pose mode: no server, one fighter on Proving Grounds' floor in front
//! of a still camera, for looking at the art (`ci/look.mjs` shoots it for
//! the reference board, `board/board.json`). After the three.js page's
//! `?pose`: `yaw=` turns the fighter (radians), `arm=` picks its gun,
//! `speed=` sets it running (m/s), `skin=` and `team=` dress it.

use bevy::prelude::*;
use client_net::core::Drawn;
use sim::geom::V3;

/// The fighter, as a snapshot would draw it.
#[derive(Resource, Clone, Copy)]
pub struct Pose(pub Drawn, pub u8);

/// Where the fighter stands, and the camera's eye: 3 m apart on open floor
/// in the lane north of the centre (the centre's raised pad would fill the
/// view and hide the fighter's legs).
const AT: Vec3 = Vec3::new(1.5, 0.0, 8.0);
const EYE: Vec3 = Vec3::new(-1.5, 1.6, 8.0);
const LOOK: Vec3 = Vec3::new(1.5, 1.0, 8.0);

impl Pose {
    /// From a query string (`yaw=3.14&arm=1`, a leading `?` or not).
    pub fn parse(q: &str) -> Pose {
        let get = |k: &str, or: f32| {
            q.trim_start_matches('?')
                .split('&')
                .filter_map(|kv| kv.split_once('='))
                .find(|(key, _)| *key == k)
                .and_then(|(_, v)| v.parse::<f32>().ok())
                .unwrap_or(or)
        };
        let yaw = get("yaw", core::f32::consts::PI);
        let speed = get("speed", 0.0);
        let d = Drawn {
            slot: 1,
            team: get("team", 0.0) as u8,
            alive: true,
            grounded: true,
            pos: V3::new(AT.x, AT.y, AT.z),
            // Running the way it faces.
            vel: V3::new(speed * yaw.cos(), 0.0, speed * yaw.sin()),
            yaw,
            arm: get("arm", 1.0) as u8,
            shield: 255,
            ..Drawn::default()
        };
        Pose(d, get("skin", 0.0) as u8)
    }
}

/// The still camera, over whatever `view` set.
pub fn aim(mut cam: Query<&mut Transform, With<crate::Eye>>) {
    if let Ok(mut t) = cam.single_mut() {
        *t = Transform::from_translation(EYE).looking_at(LOOK, Vec3::Y);
    }
}

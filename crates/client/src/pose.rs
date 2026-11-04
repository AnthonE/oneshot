//! The pose mode: no server, one fighter on Proving Grounds' floor in front
//! of a still camera, for looking at the art (`ci/look.mjs` shoots it for
//! the reference board, `board/board.json`). After the three.js page's
//! `?pose`: `yaw=` turns the fighter (radians), `arm=` picks its gun,
//! `speed=` sets it running (m/s) and `dir=` which way, radians from its
//! facing (1.57: strafing to its right, 3.14: backpedalling), `pitch=`
//! where it looks (radians, up positive), `crouch=1` crouches it, `skin=`,
//! `emblem=` (a `proto::Emblem`) and `team=` dress it.
//!
//! A map's view (the board's level shots): `map=4` builds that map instead,
//! `eye=x,y,z` and `look=x,y,z` place the camera, and the fighter stands
//! only when `fighter=1` (it always does on Proving Grounds' default view).

use bevy::prelude::*;
use client_net::core::Drawn;
use sim::geom::V3;

/// The fighter, as a snapshot would draw it, and its look; then the map,
/// the camera, and whether the fighter stands at all.
#[derive(Resource, Clone, Copy)]
pub struct Pose(pub Drawn, pub proto::Look, pub View);

/// Where the picture is taken.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub map: u8,
    pub eye: Vec3,
    pub look: Vec3,
    pub fighter: bool,
}

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
        let vec = |k: &str| {
            let v = q
                .trim_start_matches('?')
                .split('&')
                .filter_map(|kv| kv.split_once('='))
                .find(|(key, _)| *key == k)?
                .1
                .split(',')
                .map(|n| n.trim().parse::<f32>().ok())
                .collect::<Option<Vec<f32>>>()?;
            (v.len() == 3).then(|| Vec3::new(v[0], v[1], v[2]))
        };
        let map = get("map", 0.0) as u8;
        let moved = vec("eye").is_some();
        let view = View {
            map,
            eye: vec("eye").unwrap_or(EYE),
            look: vec("look").unwrap_or(LOOK),
            fighter: get("fighter", if map == 0 && !moved { 1.0 } else { 0.0 }) != 0.0,
        };
        let yaw = get("yaw", core::f32::consts::PI);
        let speed = get("speed", 0.0);
        let travel = yaw + get("dir", 0.0);
        let d = Drawn {
            slot: 1,
            team: get("team", 0.0) as u8,
            alive: true,
            grounded: true,
            pos: V3::new(AT.x, AT.y, AT.z),
            // Running the way it faces, or `dir` off it.
            vel: V3::new(speed * travel.cos(), 0.0, speed * travel.sin()),
            yaw,
            pitch: get("pitch", 0.0),
            crouched: get("crouch", 0.0) != 0.0,
            arm: get("arm", 1.0) as u8,
            shield: 255,
            ..Drawn::default()
        };
        let look = proto::Look {
            skin: get("skin", 0.0) as u8,
            emblem: proto::emblem_valid(get("emblem", 0.0) as u16),
        };
        Pose(d, look, view)
    }
}

/// The still camera, over whatever `view` set.
pub fn aim(pose: Res<Pose>, mut cam: Query<&mut Transform, With<crate::Eye>>) {
    if let Ok(mut t) = cam.single_mut() {
        *t = Transform::from_translation(pose.2.eye).looking_at(pose.2.look, Vec3::Y);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_map_view_parses_and_the_default_keeps_its_fighter() {
        let p = Pose::parse("?yaw=1.57&arm=3");
        assert_eq!(p.2.map, 0);
        assert!(p.2.fighter);
        assert_eq!(p.2.eye, EYE);
        let p = Pose::parse("map=4&eye=-20,6,0&look=0,2,0");
        assert_eq!(p.2.map, 4);
        assert!(!p.2.fighter);
        assert_eq!(p.2.eye, Vec3::new(-20.0, 6.0, 0.0));
        assert_eq!(p.2.look, Vec3::new(0.0, 2.0, 0.0));
        assert!(Pose::parse("map=4&fighter=1").2.fighter);
    }
}

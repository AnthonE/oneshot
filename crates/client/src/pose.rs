//! The pose mode: no server, one fighter on Proving Grounds' floor in front
//! of a still camera, for looking at the art (`ci/look.mjs` shoots it for
//! the reference board, `board/board.json`). After the three.js page's
//! `?pose`: `yaw=` turns the fighter (radians), `arm=` picks its gun,
//! `speed=` sets it running (m/s) and `dir=` which way, radians from its
//! facing (1.57: strafing to its right, 3.14: backpedalling), `pitch=`
//! where it looks (radians, up positive), `crouch=1` crouches it, `skin=`,
//! `emblem=` (a `proto::Emblem`) and `team=` dress it. `reload=1` reloads it, over and over; `die=S`
//! kills it S seconds after the screen is there, shot from the camera (a
//! head shot with `head=1`, a blast at its feet with `blast=1`), so the rag
//! doll (`look/rag.rs`) can be looked at.
//!
//! A map's view (the board's level shots): `map=4` builds that map instead,
//! `eye=x,y,z` and `look=x,y,z` place the camera, and the fighter stands
//! only when `fighter=1` (it always does on Proving Grounds' default view).
//! `truck=x,y,z` stands a truck there (`look/truck.rs`), `y` over the
//! ground, turned by `truckyaw=` (radians); `drive=` drives it from there
//! round a circle to its right, m/s. `seat=0` puts the fighter at its
//! wheel, `seat=1` in its passenger's place (the sim's mounts; on a floor,
//! standing still).

use bevy::prelude::*;
use client_net::core::Drawn;
use sim::geom::V3;

/// The fighter, as a snapshot would draw it, and its look; then the map,
/// the camera, and whether the fighter stands at all.
#[derive(Resource, Clone, Copy)]
pub struct Pose(pub Drawn, pub proto::Look, pub View, pub Acts);

/// What the posed fighter does: reloads over and over, and dies (when,
/// and how), if it does.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Acts {
    pub reload: bool,
    pub die_after_s: Option<f32>,
    pub head: bool,
    pub blast: bool,
}

/// Where the picture is taken.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub map: u8,
    pub eye: Vec3,
    pub look: Vec3,
    pub fighter: bool,
    /// A truck for the picture: where it stands (over the ground) and its
    /// yaw, and how fast it drives its circle from there.
    pub truck: Option<(Vec3, f32)>,
    pub drive: f32,
    /// The fighter rides it, in this seat.
    pub seat: Option<usize>,
}

/// The radius of the circle a posed truck drives, metres.
const CIRCLE_M: f32 = 8.0;

impl View {
    /// The truck's place and yaw `t` seconds in: where it was put, or as
    /// far round its circle as it has driven.
    pub fn truck_at(&self, t: f32) -> Option<(Vec3, f32)> {
        let (at, yaw) = self.truck?;
        let turned = yaw + self.drive * t / CIRCLE_M;
        let right = |a: f32| Vec3::new(-a.sin(), 0.0, a.cos());
        Some((at + (right(yaw) - right(turned)) * CIRCLE_M, turned))
    }
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
            truck: vec("truck").map(|at| (at, get("truckyaw", 0.0))),
            drive: get("drive", 0.0),
            seat: Some(get("seat", -1.0))
                .filter(|s| *s >= 0.0)
                .map(|s| (s as usize).min(1)),
        };
        // A rider stands where the sim mounts one.
        let mount = view.truck.zip(view.seat).map(|((at, yaw), seat)| {
            let hull = sim::vehicle::Vehicle::new(V3::new(at.x, at.y, at.z), sim::geom::rad_to_yaw(yaw), 0);
            hull.mount(seat)
        });
        let view = View {
            fighter: view.fighter || mount.is_some(),
            ..view
        };
        let yaw = get("yaw", core::f32::consts::PI);
        let speed = get("speed", 0.0);
        let travel = yaw + get("dir", 0.0);
        let d = Drawn {
            slot: 1,
            team: get("team", 0.0) as u8,
            alive: true,
            grounded: true,
            pos: mount.unwrap_or(V3::new(AT.x, AT.y, AT.z)),
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
        let acts = Acts {
            reload: get("reload", 0.0) != 0.0,
            die_after_s: Some(get("die", -1.0)).filter(|s| *s >= 0.0),
            head: get("head", 0.0) != 0.0,
            blast: get("blast", 0.0) != 0.0,
        };
        Pose(d, look, view, acts)
    }
}

/// The posed fighter's death, `die=` seconds after the screen is there:
/// down, and pushed as a shot from the camera (or a blast at its feet)
/// would push it. A reload starts over each time the gun's would end.
pub fn fall(
    mut pose: ResMut<Pose>,
    settle: Res<crate::loading::Settle>,
    time: Res<Time>,
    mut since: Local<Option<f32>>,
    mut blows: ResMut<crate::look::rag::Blows>,
) {
    if !settle.ready() {
        return;
    }
    let t = time.elapsed_secs();
    let t0 = *since.get_or_insert(t);
    if pose.3.reload {
        // The wire's bit falls between two reloads, as a gun's does.
        let len = sim::arms::arm(pose.0.arm).reload_ms as f32 / 1000.0 + 0.3;
        pose.0.reloading = ((t - t0) / len).fract() < 0.9;
    }
    let Some(after) = pose.3.die_after_s else {
        return;
    };
    if !pose.0.alive || t - t0 < after {
        return;
    }
    let at = Vec3::new(pose.0.pos.x, pose.0.pos.y, pose.0.pos.z);
    let cause = if pose.3.blast {
        // At its feet, on the camera's side.
        blows.blast(at + (pose.2.eye - at).with_y(0.0).normalize_or_zero() * 0.6, t);
        sim::events::Cause::Grenade as u16
    } else {
        sim::arms::BR as u16
    };
    blows.kill(pose.0.slot, at, Some(pose.2.eye), cause, pose.3.head, t);
    pose.0.alive = false;
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

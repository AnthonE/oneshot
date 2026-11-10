//! The pose mode: no server, one fighter on Proving Grounds' floor in front
//! of a still camera, for looking at the art (`ci/look.mjs` shoots it for
//! the reference board, `board/board.json`). After the three.js page's
//! `?pose`: `yaw=` turns the fighter (radians), `arm=` picks its gun,
//! `speed=` sets it running (m/s) and `dir=` which way, radians from its
//! facing (1.57: strafing to its right, 3.14: backpedalling), `pitch=`
//! where it looks (radians, up positive), `crouch=1` crouches it, `skin=`,
//! `headwear=`, `footwear=`, `emblem=` (a `proto::Emblem`) and `team=` dress it; `over=1` gives it an
//! overshield and `cloak=` camouflage (0..1, how hidden). `reload=1` reloads it, over and over; `die=S`
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
//! standing still). `truckhurt=` wears it as a hull with that share of its
//! health gone would be (0 to 1), `truckmarks=1` marks it as rounds and
//! blasts would; `truckturret=1` stands a minigun on its bed
//! (`look/turret.rs`), `turretheat=` drawn that hot (0 to 1), spinning. `fling=S` sets a rocket off under its right door S seconds
//! after the screen is there, throwing it as the sim would; `wreck=S` blows
//! it up then, its husk thrown. `orb=1|2|3`
//! stands a ball of plasma at `look` (`stand_orb`). `flag=x,y,z` plants
//! red's flag on its stand there (`flags.rs`, its cloth blowing on the
//! map's wind); `flagheld=1` puts it over the fighter's shoulder instead,
//! as a carrier holds it, and `flagheld=2` across the camera's view, as a
//! carrier sees it. `craft=x,y,z` stands a one-seater there (`look/craft.rs`),
//! `y` over the ground, `craftkind=1` a skimmer or `2` a flier, turned by
//! `craftyaw=`, its nose up `craftpitch=` and rolled `craftroll=` (radians),
//! `craftboost=1` boosting, `craftseat=1` the fighter at its controls.
//! `craftcam=x,y,z` puts the camera in the craft's own frame (metres ahead
//! of its middle, over the ground under it, to its right), looking at
//! `craftaim=x,y,z` in the same frame: a view that needs no height read off
//! the map (`tools/craft.sh`'s). `craftchase=1` is the camera a player has
//! at its controls, looking the way it points. `craftfly=` puts it under
//! way along its nose, metres a second, and `craftturn=` turns it, radians
//! a second (`fly`): its lean, its engines' flames at speed, its trails.

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
    /// How hurt it is drawn (0 whole, 1 nothing left), and whether marked.
    pub truck_hurt: f32,
    pub truck_marks: bool,
    /// What its bed carries (`sim::vehicle::MINIGUN`: `truckturret=1`),
    /// and how hot it is drawn, 0 to 1 (`turretheat=`): over 0, firing.
    pub truck_turret: u8,
    pub turret_heat: f32,
    /// The fighter rides it, in this seat.
    pub seat: Option<usize>,
    /// A flag's stand for the picture, and who holds it: 0 nobody, 1 the
    /// fighter, 2 the camera (as in our own view).
    pub flag: Option<(Vec3, u8)>,
    /// Seconds after the screen is there that it is thrown by a rocket,
    /// and blown up; when it was thrown (the clock `truck_at` reads), and
    /// whether it is gone.
    pub fling: Option<f32>,
    pub wreck: Option<f32>,
    pub flung_at: Option<f32>,
    pub wrecked: bool,
    /// A ball of plasma at `look`: its tone, heat and radius.
    pub orb: Option<(u8, f32, f32)>,
    /// A one-seater for the picture: where it stands (over the ground), its
    /// yaw, its kind, its nose and roll (radians), whether it boosts.
    pub craft: Option<(Vec3, f32, u8, f32, f32, bool)>,
    /// The fighter is at its controls.
    pub craft_seat: bool,
    /// The camera and what it looks at, in the craft's frame.
    pub craft_cam: Option<(Vec3, Vec3)>,
    /// The camera is the one at its controls (`craft_chase`).
    pub craft_chase: bool,
    /// It is under way: how fast along its nose, m/s, and how fast it turns,
    /// rad/s (`craftfly=`, `craftturn=`), from when the screen is there.
    pub craft_fly: (f32, f32),
}

/// A ball of plasma for the picture (`look/fx/plasma.rs`), standing at `look`:
/// `orb=1` blue, `orb=2` green, `orb=3` pink; `heat=` 0 to 1, `orbr=` its radius.
pub fn stand_orb(
    mut commands: Commands,
    pose: Res<Pose>,
    kit: Option<Res<crate::look::fx::plasma::Kit>>,
    mut done: Local<bool>,
) {
    use crate::look::fx::plasma::{spawn_ball, Tone};
    let (Some(kit), false) = (kit, *done) else {
        return;
    };
    *done = true;
    let Some((tone, heat, radius)) = pose.2.orb else {
        return;
    };
    let tone = Tone::ALL[(tone as usize).min(Tone::ALL.len() - 1)];
    commands
        .spawn((Transform::from_translation(pose.2.look), Visibility::default()))
        .with_children(|c| {
            spawn_ball(c, &kit, tone, radius, heat);
        });
}

/// The radius of the circle a posed truck drives, metres.
const CIRCLE_M: f32 = 8.0;
/// What `craftcam` looks at with no `craftaim`: the craft's middle.
const CRAFT_AIM: Vec3 = Vec3::new(0.0, 0.8, 0.0);

impl View {
    /// The camera's eye and what it looks at: as given, or in the craft's
    /// frame (`craftcam`), on the `ground` under it.
    pub fn camera(&self, ground: impl Fn(f32, f32) -> f32) -> (Vec3, Vec3) {
        let Some((at, yaw, kind, pitch, ..)) = self.craft else {
            return (self.eye, self.look);
        };
        let base = at + Vec3::Y * ground(at.x, at.z);
        if self.craft_chase {
            // From its pilot's eye, where the sim seats one.
            let hull = sim::vehicle::Vehicle::new_kind(
                V3::new(base.x, base.y, base.z),
                sim::geom::rad_to_yaw(yaw),
                0,
                0,
                kind,
            );
            let seat = hull.mount(sim::vehicle::DRIVER);
            let eye = Vec3::new(seat.x, seat.y + sim::consts::CROUCH_EYE, seat.z);
            let aim = Vec3::new(pitch.cos() * yaw.cos(), pitch.sin(), pitch.cos() * yaw.sin());
            return crate::craft_chase(kind, eye, aim);
        }
        let Some((cam, aim)) = self.craft_cam else {
            return (self.eye, self.look);
        };
        let (f, r) = (
            Vec3::new(yaw.cos(), 0.0, yaw.sin()),
            Vec3::new(-yaw.sin(), 0.0, yaw.cos()),
        );
        let world = |v: Vec3| base + f * v.x + Vec3::Y * v.y + r * v.z;
        (world(cam), world(aim))
    }

    /// The truck's place and yaw `t` seconds in: where it was put, or as
    /// far round its circle as it has driven.
    pub fn truck_at(&self, t: f32) -> Option<(Vec3, f32)> {
        let (at, yaw) = self.truck?;
        if self.wrecked {
            return None;
        }
        let turned = yaw + self.drive * t / CIRCLE_M;
        let right = |a: f32| Vec3::new(-a.sin(), 0.0, a.cos());
        let mut p = at + (right(yaw) - right(turned)) * CIRCLE_M;
        // Thrown, as the sim throws it from a rocket under its right door:
        // away to its left and up, until it is down again.
        if let Some(t0) = self.flung_at {
            let g = sim::consts::GRAVITY;
            let up = sim::consts::VEH_BLAST_LIFT;
            let air = (t - t0).clamp(0.0, 2.0 * up / g);
            let push = sim::consts::BLAST_PUSH * sim::consts::VEH_PUSH_K;
            p += -right(yaw) * push * air + Vec3::Y * (up * air - 0.5 * g * air * air);
        }
        Some((p, turned))
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
            truck_hurt: get("truckhurt", 0.0).clamp(0.0, 1.0),
            truck_marks: get("truckmarks", 0.0) != 0.0,
            truck_turret: get("truckturret", 0.0) as u8,
            turret_heat: get("turretheat", 0.0).clamp(0.0, 1.0),
            seat: Some(get("seat", -1.0))
                .filter(|s| *s >= 0.0)
                .map(|s| (s as usize).min(1)),
            flag: vec("flag").map(|at| (at, get("flagheld", 0.0) as u8)),
            fling: Some(get("fling", -1.0)).filter(|s| *s >= 0.0),
            wreck: Some(get("wreck", -1.0)).filter(|s| *s >= 0.0),
            flung_at: None,
            wrecked: false,
            orb: Some(get("orb", 0.0))
                .filter(|o| *o >= 1.0)
                .map(|o| (o as u8 - 1, get("heat", 0.3), get("orbr", 0.15))),
            craft: vec("craft").map(|at| {
                (
                    at,
                    get("craftyaw", 0.0),
                    (get("craftkind", 1.0) as u8).clamp(sim::vehicle::SKIMMER, sim::vehicle::FLIER),
                    get("craftpitch", 0.0),
                    get("craftroll", 0.0),
                    get("craftboost", 0.0) != 0.0,
                )
            }),
            craft_seat: get("craftseat", 0.0) != 0.0,
            craft_cam: vec("craftcam").map(|c| (c, vec("craftaim").unwrap_or(CRAFT_AIM))),
            craft_chase: get("craftchase", 0.0) != 0.0,
            craft_fly: (get("craftfly", 0.0), get("craftturn", 0.0)),
        };
        // A rider stands where the sim mounts one.
        let mount = view.truck.zip(view.seat).map(|((at, yaw), seat)| {
            let hull = sim::vehicle::Vehicle::new(
                V3::new(at.x, at.y, at.z),
                sim::geom::rad_to_yaw(yaw),
                0,
                view.truck_turret,
            );
            hull.mount(seat)
        });
        let view = View {
            fighter: view.fighter || mount.is_some() || (view.craft.is_some() && view.craft_seat),
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
            over: get("over", 0.0) != 0.0,
            cloak: get("cloak", 0.0).clamp(0.0, 1.0),
            ..Drawn::default()
        };
        let look = proto::Look {
            skin: get("skin", 0.0) as u8,
            emblem: proto::emblem_valid(get("emblem", 0.0) as u16),
            headwear: proto::gear_valid(&proto::HEADWEAR, get("headwear", 0.0) as u8),
            footwear: proto::gear_valid(&proto::FOOTWEAR, get("footwear", 0.0) as u8),
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

/// The posed craft under way (`craftfly=`, `craftturn=`), once the screen is
/// there: over the ground it was stood over, for the looks that read its
/// going (its lean, its engines' flames, its trails).
pub fn fly(mut pose: ResMut<Pose>, settle: Res<crate::loading::Settle>, time: Res<Time>) {
    let (speed, turn) = pose.2.craft_fly;
    if (speed == 0.0 && turn == 0.0) || !settle.ready() {
        return;
    }
    let dt = time.delta_secs();
    if let Some((at, yaw, ..)) = pose.2.craft.as_mut() {
        *yaw += turn * dt;
        at.x += speed * yaw.cos() * dt;
        at.z += speed * yaw.sin() * dt;
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
    blows.kill(pose.0.slot, at, Some(pose.2.eye), Vec3::ZERO, cause, pose.3.head, t);
    pose.0.alive = false;
}

/// The posed truck thrown by a rocket (`fling=`) and blown up (`wreck=`),
/// as the server would tell it: the blast or the wreck heard and drawn.
/// After the frame's events, before the hulls are drawn.
pub fn throw(
    mut pose: ResMut<Pose>,
    settle: Res<crate::loading::Settle>,
    time: Res<Time>,
    mut since: Local<Option<f32>>,
    link: NonSend<crate::LinkRes>,
    mut heard: ResMut<crate::sound::Heard>,
    mut fx: ResMut<crate::look::fx::Fx>,
) {
    if !settle.ready() {
        return;
    }
    let t = time.elapsed_secs();
    let t0 = *since.get_or_insert(t);
    let view = pose.2;
    let Some((at, yaw)) = view.truck_at(t) else {
        return;
    };
    let ground = link.0.session().core.map.ground(at.x, at.z);
    let at = at + Vec3::Y * ground;
    let right = Vec3::new(-yaw.sin(), 0.0, yaw.cos());
    let mut ev = None;
    if view.fling.is_some_and(|s| t - t0 >= s) && view.flung_at.is_none() {
        pose.2.flung_at = Some(t);
        let b = at + right * (sim::consts::VEH_HALF_W + 0.4) + Vec3::Y * 0.2;
        ev = Some(proto::EventState {
            kind: sim::events::Kind::Blast as u8,
            c: sim::arms::ROCKET as u16,
            pos: V3::new(b.x, b.y, b.z),
            ..proto::EventState::default()
        });
    } else if view.wreck.is_some_and(|s| t - t0 >= s) && !view.wrecked {
        pose.2.wrecked = true;
        ev = Some(proto::EventState {
            kind: sim::events::Kind::Wreck as u8,
            part: 0,
            pos: V3::new(at.x, at.y, at.z),
            ..proto::EventState::default()
        });
    }
    let Some(ev) = ev else { return };
    heard.0.push(ev);
    let scene = crate::look::fx::Scene {
        map: &link.0.session().core.map,
        others: &[],
        vehicles: &[],
        me: u8::MAX,
        aim: Vec3::X,
        t,
        on_body: None,
        arm: sim::arms::NONE,
    };
    crate::look::fx::on_events(&mut fx, &scene, &[ev]);
}

/// The still camera, over whatever `view` set.
pub fn aim(pose: Res<Pose>, link: NonSend<crate::LinkRes>, mut cam: Query<&mut Transform, With<crate::Eye>>) {
    if let Ok(mut t) = cam.single_mut() {
        let map = &link.0.session().core.map;
        let (eye, look) = pose.2.camera(|x, z| map.ground(x, z));
        *t = Transform::from_translation(eye).looking_at(look, Vec3::Y);
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

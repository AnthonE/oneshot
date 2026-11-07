//! Walking, jumping and falling: Quake's model, ported from Fireplace's
//! `$move` (`src/players.wat`). Ground friction and acceleration, capped air
//! acceleration (air strafing), gravity, then the map's boxes an axis at a
//! time: x, z, then up or down, stepping onto anything up to [`STEP`] high.
//! On a terrain, a slope steeper than [`MAX_CLIMB`] stops a step up it and
//! one too steep to stand on slides the body down; a launcher throws a
//! grounded body. All of it reads only the body, its input and the map.

use crate::consts::*;
use crate::fmath;
use crate::geom::{yaw_sincos, V3};
use crate::input::{Input, CROUCH, JUMP};
use crate::map::Map;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Body {
    /// Feet.
    pub pos: V3,
    pub vel: V3,
    pub grounded: bool,
    pub crouched: bool,
    /// Ticks into a crouch, 0 standing to [`CROUCH_TICKS`] down: how far
    /// the eye and the hit volume have gone. A tuck in the air is the legs
    /// and not the head, so it is whole at once.
    pub duck: u8,
}

/// `up` standing, `down` crouched, and between as far as `duck` has gone.
pub fn ducked(duck: u8, up: f32, down: f32) -> f32 {
    up + (down - up) * (duck.min(CROUCH_TICKS) as f32 * (1.0 / CROUCH_TICKS as f32))
}

impl Body {
    pub fn height(&self) -> f32 {
        if self.crouched {
            CROUCH_H
        } else {
            BODY_H
        }
    }

    pub fn eye(&self) -> V3 {
        V3::new(self.pos.x, self.pos.y + self.eye_h(), self.pos.z)
    }

    /// The eye over the feet, as far down as the crouch has gone.
    pub fn eye_h(&self) -> f32 {
        ducked(self.duck, EYE, CROUCH_EYE)
    }
}

/// Snap to the 1/1024 grid (consts::QUANT).
pub fn quant(v: f32) -> f32 {
    fmath::round(v * QUANT) * (1.0 / QUANT)
}

fn quant3(v: V3) -> V3 {
    V3::new(quant(v.x), quant(v.y), quant(v.z))
}

/// Quake's accelerate, speed along `w` (unit) up to `wish` gaining at most
/// `acc` this tick, but Halo's top speed: the flat speed after it is never
/// more than the greater of the speed before and `wish`. So it turns,
/// slows and brings a body up to a run, and never adds past it. Quake's
/// lets a strafe that turns with the mouse gain a little every tick: a run
/// held 5% over its top on the ground, and in the air (with its cap) a
/// jump doubled a run at 128 Hz and each hop doubled it again.
fn accelerate(vel: V3, w: V3, wish: f32, acc: f32) -> V3 {
    let add = wish - vel.dot(w);
    if add <= 0.0 {
        return vel;
    }
    let acc = acc.min(add);
    let (mut x, mut z) = (vel.x + w.x * acc, vel.z + w.z * acc);
    let cap = (vel.x * vel.x + vel.z * vel.z).max(wish * wish);
    let now = x * x + z * z;
    if now > cap {
        let k = (cap / now).sqrt();
        x *= k;
        z *= k;
    }
    V3::new(x, vel.y, z)
}

/// One tick of a body under one input.
pub fn step(b: &mut Body, inp: &Input, map: &Map) {
    step_paced(b, inp, map, 1.0);
}

/// [`step`], its top speed on the ground and in the air times `pace` (a
/// flag carrier's, `FLAG_PACE`).
pub fn step_paced(b: &mut Body, inp: &Input, map: &Map, pace: f32) {
    // Crouch while held; stand back up only where the head fits. In the
    // air (Halo's crouch jump) crouching tucks the legs up under a head
    // that stays, and letting go puts them back down where there's room.
    // On the ground the head goes down and up over CROUCH_TICKS; in the
    // air the legs move and the head stays, so the eye is there at once.
    let was = b.crouched;
    let mut legs = false;
    if inp.held(CROUCH) {
        b.crouched = true;
    } else if b.crouched && b.grounded {
        if map.ceiling(b.pos.x, b.pos.z, BODY_R, b.pos.y + 0.01) >= b.pos.y + BODY_H {
            b.crouched = false;
        }
    } else if b.crouched && map.floor_with(b.pos.x, b.pos.z, BODY_R, b.pos.y, b.pos.y) <= b.pos.y - CROUCH_LIFT {
        b.pos.y -= CROUCH_LIFT;
        b.crouched = false;
        legs = true;
    }
    if b.crouched && !was && !b.grounded {
        b.pos.y += CROUCH_LIFT;
        legs = true;
    }
    b.duck = match (legs, b.crouched) {
        (true, true) => CROUCH_TICKS,
        (true, false) => 0,
        (false, true) => (b.duck + 1).min(CROUCH_TICKS),
        (false, false) => b.duck.saturating_sub(1),
    };

    // The wish direction from the stick, turned by yaw: forward is the
    // yaw's direction, right is a quarter turn toward +z from it.
    let (s, c) = yaw_sincos(inp.yaw);
    let fwd = inp.mz as f32 * (1.0 / 127.0);
    let side = inp.mx as f32 * (1.0 / 127.0);
    let mut w = V3::new(c * fwd - s * side, 0.0, s * fwd + c * side);
    let len = w.len();
    let mut wish = 0.0;
    if len > 0.0 {
        w = w * (1.0 / len);
        // Tucked legs in the air never slow it: crouch speed is a walk's.
        let top = if b.crouched && b.grounded {
            CROUCH_RUN
        } else {
            // Halo 2's: full speed ahead, a little less backing up or to
            // the side; between, on the ellipse.
            let ahead = fwd / len * if fwd >= 0.0 { RUN } else { RUN_BACK };
            let aside = side / len * RUN_SIDE;
            (ahead * ahead + aside * aside).sqrt()
        };
        wish = top * len.min(1.0) * pace;
    }

    let mut v = b.vel;
    if b.grounded {
        let speed = (v.x * v.x + v.z * v.z).sqrt();
        if speed > 0.0 {
            let drop = speed.max(STOP) * FRICTION * DT;
            let k = (speed - drop).max(0.0) / speed;
            v.x *= k;
            v.z *= k;
        }
        v = accelerate(v, w, wish, ACCEL * wish * DT);
        // From a crouch too, as in any shooter: a crouch pressed a tick
        // before the jump (a crouch jump's two buttons, mashed) must not
        // eat it. Held, the legs stay tucked; let go, they come down in
        // the air where there's room, the head staying.
        if inp.held(JUMP) {
            v.y = JUMP_SPEED;
            b.grounded = false;
        }
    } else {
        v = accelerate(v, w, wish, AIR_ACCEL * len.min(1.0) * DT);
        v.y -= GRAVITY * DT;
    }
    let sp = v.dot(v);
    if sp > VMAX * VMAX {
        v = v * (VMAX / sp.sqrt());
    }

    let h = b.height();
    let mut p = b.pos;
    // On the ground the body's band starts a step up: it walks onto
    // anything lower and only what is taller blocks it.
    let lo = p.y + if b.grounded { STEP } else { 0.0 };
    let hi = p.y + h;

    let tx = p.x + v.x * DT;
    let mut nx = sweep(
        map,
        BODY_R,
        p.x,
        tx,
        (
            (p.x.min(tx) - BODY_R, p.z - BODY_R),
            (p.x.max(tx) + BODY_R, p.z + BODY_R),
        ),
        |bx| (bx.min.x, bx.max.x),
        |bx| bx.over_z(p.z, BODY_R) && bx.min.y < hi && bx.max.y > lo,
    );
    if map.terrain.is_some() && too_steep(map, (p.x, p.z), (nx, p.z), p.y) {
        nx = p.x;
    }
    if nx != tx {
        v.x = 0.0;
    }
    p.x = nx.clamp(map.lo.0, map.hi.0);
    let tz = p.z + v.z * DT;
    let mut nz = sweep(
        map,
        BODY_R,
        p.z,
        tz,
        (
            (p.x - BODY_R, p.z.min(tz) - BODY_R),
            (p.x + BODY_R, p.z.max(tz) + BODY_R),
        ),
        |bx| (bx.min.z, bx.max.z),
        |bx| bx.over_x(p.x, BODY_R) && bx.min.y < hi && bx.max.y > lo,
    );
    if map.terrain.is_some() && too_steep(map, (p.x, p.z), (p.x, nz), p.y) {
        nz = p.z;
    }
    if nz != tz {
        v.z = 0.0;
    }
    p.z = nz.clamp(map.lo.1, map.hi.1);
    if !map.solids.is_empty() {
        // Round and turned walls, and ramps too tall to walk onto: out of
        // them, sliding along (crate::solid).
        map.push_out(
            &mut p.x,
            &mut p.z,
            &mut v,
            BODY_R,
            p.y,
            lo,
            hi,
            p.y + STEP,
            (b.pos.x, b.pos.z),
        );
        p.x = p.x.clamp(map.lo.0, map.hi.0);
        p.z = p.z.clamp(map.lo.1, map.hi.1);
    }

    let reach = p.y + if b.grounded { STEP } else { 0.01 };
    let floor = map.floor_with(p.x, p.z, BODY_R, reach, p.y + STEP);
    let ceil = map.ceiling(p.x, p.z, BODY_R, p.y + h - 0.01);
    let mut ny = p.y + v.y * DT;
    if ny + h > ceil {
        ny = ceil - h;
        v.y = v.y.min(0.0);
    }
    if ny <= floor {
        ny = floor;
        v.y = v.y.max(0.0);
        b.grounded = v.y <= 0.0;
    } else if b.grounded && v.y <= 0.0 && ny - floor < SNAP {
        // Walking down a step stays on the ground.
        ny = floor;
        v.y = 0.0;
    } else {
        b.grounded = false;
    }
    p.y = ny;

    if map.terrain.is_some() && v.y <= 0.0 && floor == map.ground(p.x, p.z) && p.y - floor < SNAP {
        // On ground too steep to stand on: the body stays on the face and
        // slides down the fall line, never grounded (no friction, no jump).
        let n = map.ground_normal(p.x, p.z);
        if n.y < WALK_NY {
            p.y = floor;
            v.y = 0.0;
            b.grounded = false;
            v.x += n.x * (SLIDE_ACCEL * DT);
            v.z += n.z * (SLIDE_ACCEL * DT);
        }
    }
    if b.grounded {
        for l in &map.launchers {
            let a = &l.area;
            if p.x >= a.min.x && p.x <= a.max.x && p.z >= a.min.z && p.z <= a.max.z && p.y >= a.min.y && p.y <= a.max.y
            {
                v = l.vel;
                b.grounded = false;
                break;
            }
        }
    }

    b.pos = quant3(p);
    b.vel = quant3(v);
}

/// Whether a move from `a` to `b` (x, z) climbs ground steeper than
/// [`MAX_CLIMB`] to above the feet at `feet`, or onto a face too steep to
/// stand on ([`WALK_NY`]).
pub(crate) fn too_steep(map: &Map, a: (f32, f32), b: (f32, f32), feet: f32) -> bool {
    let g1 = map.ground(b.0, b.1);
    if g1 <= feet {
        return false;
    }
    // A face too steep to stand on refuses any step up it, however small
    // and from whichever side: the rise along one axis alone, under the
    // centimetre below, let a body creep along a cliff at an angle and be
    // lifted up it a few millimetres a tick.
    if map.ground_normal(b.0, b.1).y < WALK_NY {
        return true;
    }
    if g1 <= feet + 0.01 {
        return false;
    }
    let run = crate::fmath::fabs(b.0 - a.0) + crate::fmath::fabs(b.1 - a.1);
    g1 - map.ground(a.0, a.1) > run * MAX_CLIMB
}

/// Moves a coordinate from `from` toward `to` until it meets a box face
/// that `blocks` says is in the way on the other axes. "Was outside" allows
/// [`FACE_EPS`], so a body the grid left a hair inside a face it rests
/// against is still held by it. `area` (x and z, low and high) holds every
/// box that could stop the move; the answer is the nearest stop, so which
/// boxes are visited, and in what order, never changes it.
pub(crate) fn sweep(
    map: &Map,
    r: f32,
    from: f32,
    to: f32,
    area: ((f32, f32), (f32, f32)),
    span: impl Fn(&crate::geom::Aabb) -> (f32, f32),
    blocks: impl Fn(&crate::geom::Aabb) -> bool,
) -> f32 {
    let mut t = to;
    map.each_box_in(area.0, area.1, |bx| {
        if !blocks(bx) {
            return;
        }
        let (lo, hi) = span(bx);
        if to > from && from + r <= lo + FACE_EPS && t + r > lo {
            t = lo - r;
        } else if to < from && from - r >= hi - FACE_EPS && t - r < hi {
            t = hi + r;
        }
    });
    t
}

/// Far more than the grid's half step (1/2048 m), far less than a tick's
/// travel at a walk.
const FACE_EPS: f32 = 0.01;

impl crate::geom::Aabb {
    pub(crate) fn over_z(&self, z: f32, r: f32) -> bool {
        self.min.z < z + r && self.max.z > z - r
    }
    pub(crate) fn over_x(&self, x: f32, r: f32) -> bool {
        self.min.x < x + r && self.max.x > x - r
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fmath::fabs;
    use crate::input::Input;

    fn run(b: &mut Body, inp: Input, map: &Map, n: u32) {
        for _ in 0..n {
            step(b, &inp, map);
        }
    }

    #[test]
    fn runs_at_top_speed_and_stops() {
        let map = Map::proving_grounds();
        let mut b = Body {
            pos: V3::new(-22.0, 0.0, 20.0),
            grounded: true,
            ..Body::default()
        };
        run(
            &mut b,
            Input {
                mz: 127,
                ..Input::default()
            },
            &map,
            TICK_HZ,
        );
        assert!(fmath::fabs(b.vel.x - RUN) < 0.05, "{:?}", b.vel);
        assert!(b.grounded);
        run(&mut b, Input::default(), &map, TICK_HZ);
        assert_eq!(b.vel.x, 0.0);
    }

    #[test]
    fn jumps_and_lands() {
        let map = Map::proving_grounds();
        let mut b = Body {
            pos: V3::new(-22.0, 0.0, 20.0),
            grounded: true,
            ..Body::default()
        };
        step(
            &mut b,
            &Input {
                buttons: JUMP,
                ..Input::default()
            },
            &map,
        );
        assert!(!b.grounded);
        let mut top = 0.0f32;
        let mut air = 1;
        while !b.grounded && air < TICK_HZ * 2 {
            step(&mut b, &Input::default(), &map);
            top = top.max(b.pos.y);
            air += 1;
        }
        // Halo 2's: half again a body's height, 1.4 s up and down.
        assert!(b.grounded && b.pos.y == 0.0);
        assert!(top > 2.6 && top < 2.9, "apex {top}");
        let secs = air as f32 / TICK_HZ as f32;
        assert!((1.35..1.45).contains(&secs), "{secs} s in the air");
    }

    #[test]
    fn backs_up_and_strafes_slower_than_it_runs_ahead() {
        let map = Map::proving_grounds();
        let speed = |mz: i8, mx: i8| {
            let mut b = Body {
                pos: V3::new(-20.0, 0.0, 20.0),
                grounded: true,
                ..Body::default()
            };
            run(
                &mut b,
                Input {
                    mz,
                    mx,
                    ..Input::default()
                },
                &map,
                TICK_HZ / 2,
            );
            (b.vel.x * b.vel.x + b.vel.z * b.vel.z).sqrt()
        };
        assert!(fabs(speed(127, 0) - RUN) < 0.05);
        assert!(fabs(speed(-127, 0) - RUN_BACK) < 0.05);
        assert!(fabs(speed(0, 127) - RUN_SIDE) < 0.05);
        // Ahead and aside at once: on the ellipse between.
        let k = core::f32::consts::FRAC_1_SQRT_2;
        let diag = ((k * RUN) * (k * RUN) + (k * RUN_SIDE) * (k * RUN_SIDE)).sqrt();
        assert!(fabs(speed(127, 127) - diag) < 0.05, "{} vs {diag}", speed(127, 127));
    }

    /// Open floor, nothing on it: room to strafe.
    fn open() -> Map {
        crate::map::Def {
            id: 252,
            practice: true,
            modes: 0,
            name: "open",
            lo: (-500.0, -500.0),
            hi: (500.0, 500.0),
            void: f32::NEG_INFINITY,
            boxes: &[],
            looks: &[],
            spawns: &[([0.0, 0.0, 0.0], 0, 0)],
            pads: &[],
            roam: &[],
            terrain: None,
            launchers: &[],
            vehicles: &[],
            solids: &[],
            solid_looks: &[],
            dress: &[],
            dress_solids: &[],
            profiles: &[],
            dress_profiles: &[],
            nav: false,
            light: crate::map::LightDef::DAY,
            flags: &[],
        }
        .build()
    }

    fn flat_speed(b: &Body) -> f32 {
        (b.vel.x * b.vel.x + b.vel.z * b.vel.z).sqrt()
    }

    #[test]
    fn air_strafing_and_bunny_hops_never_gain_speed() {
        // Each tick in the air, of the turns and strafes a mouse and keys
        // could make, the one that leaves the body fastest; jump held, so
        // it hops the tick it lands. Quake's air took a run to 13 m/s in
        // one jump and 22 by the fourth.
        let map = open();
        let mut b = standing(&map, 0.0, 0.0);
        run(&mut b, run_x(), &map, TICK_HZ);
        let mut yaw = 0u16;
        let mut top = 0.0f32;
        for _ in 0..3 {
            step(
                &mut b,
                &Input {
                    buttons: JUMP,
                    mz: 127,
                    yaw,
                    ..Input::default()
                },
                &map,
            );
            let mut air = 0;
            while !b.grounded && air < TICK_HZ * 2 {
                let mut best = (f32::MIN, b, yaw);
                for turn in (-1500i32..=1500).step_by(100) {
                    for (mx, mz) in [(127, 0), (-127, 0), (127, 127), (-127, 127)] {
                        let y = yaw.wrapping_add(turn as u16);
                        let mut c = b;
                        step(
                            &mut c,
                            &Input {
                                buttons: JUMP,
                                yaw: y,
                                mx,
                                mz,
                                ..Input::default()
                            },
                            &map,
                        );
                        if flat_speed(&c) > best.0 {
                            best = (flat_speed(&c), c, y);
                        }
                    }
                }
                (_, b, yaw) = best;
                top = top.max(flat_speed(&b));
                air += 1;
            }
        }
        // A landing tick's Quake ground step may turn a run a hair over it;
        // nothing compounds.
        assert!(top < RUN * 1.02, "{top} m/s");
    }

    #[test]
    fn the_air_steers_a_jump_but_commits_it() {
        let map = open();
        // From a standstill, the stick ahead: it drifts to a run.
        let mut b = standing(&map, 0.0, 0.0);
        step(
            &mut b,
            &Input {
                buttons: JUMP,
                ..Input::default()
            },
            &map,
        );
        run(&mut b, run_x(), &map, TICK_HZ / 2);
        let half = b.vel.x;
        assert!(half > 3.0 && half < 4.5, "{half} m/s half a second in");
        // A running jump pulled back: slowed, not stopped dead.
        let mut b = standing(&map, 0.0, 0.0);
        run(&mut b, run_x(), &map, TICK_HZ);
        step(
            &mut b,
            &Input {
                buttons: JUMP,
                mz: 127,
                ..Input::default()
            },
            &map,
        );
        let back = Input {
            mz: -127,
            ..Input::default()
        };
        run(&mut b, back, &map, TICK_HZ / 4);
        assert!(b.vel.x > 4.0, "{} m/s a quarter second in", b.vel.x);
        // Let go of the stick: the jump keeps what it has.
        let was = b.vel.x;
        run(&mut b, Input::default(), &map, TICK_HZ / 4);
        assert_eq!(b.vel.x, was);
    }

    #[test]
    fn a_crouch_jumps() {
        let map = open();
        let mut b = standing(&map, 0.0, 0.0);
        let crouch = Input {
            buttons: CROUCH,
            ..Input::default()
        };
        run(&mut b, crouch, &map, 4);
        assert!(b.crouched && b.grounded);
        // Jump with crouch still held (pressed a tick early): it goes.
        let both = Input {
            buttons: CROUCH | JUMP,
            ..Input::default()
        };
        step(&mut b, &both, &map);
        assert!(!b.grounded && b.vel.y > 0.0, "{b:?}");
        let mut top = 0.0f32;
        while !b.grounded {
            step(&mut b, &crouch, &map);
            top = top.max(b.pos.y);
        }
        assert!(top > 2.6 && top < 2.9, "apex {top}");
    }

    /// On the ground the eye goes down over CROUCH_TICKS and comes back
    /// up over as many, a step a tick, while the walking box and the speed
    /// are the crouch's at once.
    #[test]
    fn a_crouch_takes_a_moment_down_and_up() {
        let map = open();
        let mut b = standing(&map, 0.0, 0.0);
        let crouch = Input {
            buttons: CROUCH,
            ..Input::default()
        };
        let mut eye = b.eye_h();
        assert_eq!(eye, EYE);
        for t in 1..=CROUCH_TICKS {
            step(&mut b, &crouch, &map);
            assert!(b.crouched && b.height() == CROUCH_H, "the box at once");
            assert!(b.eye_h() < eye, "tick {t}: down a step");
            eye = b.eye_h();
        }
        assert_eq!(b.eye_h(), CROUCH_EYE);
        step(&mut b, &crouch, &map);
        assert_eq!(b.eye_h(), CROUCH_EYE, "and stays there");
        for t in 1..=CROUCH_TICKS {
            step(&mut b, &Input::default(), &map);
            assert!(!b.crouched && b.eye_h() > eye, "tick {t}: up a step");
            eye = b.eye_h();
        }
        assert_eq!(b.eye_h(), EYE);
        // A tap: halfway down and straight back.
        run(&mut b, crouch, &map, (CROUCH_TICKS / 2) as u32);
        assert_eq!(b.duck, CROUCH_TICKS / 2);
        run(&mut b, Input::default(), &map, (CROUCH_TICKS / 2) as u32);
        assert_eq!(b.duck, 0);
    }

    #[test]
    fn a_crouch_jump_reaches_a_ledge_a_jump_does_not() {
        // Proving Grounds' 3 m pillar at x 7.25..8.75, z 11.25..12.75: a
        // running jump from 2 m short peaks at its face.
        let map = Map::proving_grounds();
        for crouch in [false, true] {
            // A run-up from 8 m short, the jump at 2 m.
            let mut b = Body {
                pos: V3::new(-6.0, 0.0, 12.0),
                grounded: true,
                ..Body::default()
            };
            let mut eye = b.eye().y;
            let mut jumped = None;
            for t in 0..TICK_HZ * 4 {
                let mut buttons = 0;
                if jumped.is_none() && b.pos.x >= 2.0 {
                    buttons = JUMP;
                    jumped = Some(t);
                } else if crouch && jumped.is_some() {
                    buttons = CROUCH;
                }
                step(
                    &mut b,
                    &Input {
                        buttons,
                        mz: 127,
                        ..Input::default()
                    },
                    &map,
                );
                // Tucking the legs never moves the eye.
                assert!(fabs(b.eye().y - eye) < 0.2, "tick {t}: eye {eye} to {}", b.eye().y);
                eye = b.eye().y;
                if jumped.is_some_and(|j| t > j + 4) && b.grounded {
                    break;
                }
            }
            let on_top = b.pos.y == 3.0;
            assert_eq!(on_top, crouch, "crouch {crouch}: {:?}", b);
        }
    }

    #[test]
    fn walls_stop_and_steps_climb() {
        let map = Map::proving_grounds();
        // Run into the west wall.
        let mut b = Body {
            pos: V3::new(-22.0, 0.0, 20.0),
            grounded: true,
            ..Body::default()
        };
        run(
            &mut b,
            Input {
                mz: 127,
                yaw: 32768,
                ..Input::default()
            },
            &map,
            TICK_HZ * 2,
        );
        assert!(b.pos.x >= -24.0 + BODY_R - 0.01, "{:?}", b.pos);
        // Walk up the red steps onto the platform (2.5 m up).
        let mut b = Body {
            pos: V3::new(-7.0, 0.0, 0.0),
            grounded: true,
            ..Body::default()
        };
        run(
            &mut b,
            Input {
                mz: 127,
                yaw: 32768,
                ..Input::default()
            },
            &map,
            TICK_HZ * 13 / 10,
        );
        assert!(b.pos.x < -12.5 && b.pos.x > -19.5, "{:?}", b.pos);
        assert_eq!(b.pos.y, 2.5, "{:?}", b.pos);
        assert!(b.grounded);
    }

    /// 16 m along x in 2 m cells, rising 3 m a cell over its middle (56°,
    /// too steep), flat at both ends.
    const CLIFF: [i16; 18] = [
        0, 0, 0, 384, 768, 1152, 1152, 1152, 1152, //
        0, 0, 0, 384, 768, 1152, 1152, 1152, 1152,
    ];

    fn cliff() -> Map {
        crate::map::Def {
            id: 254,
            name: "cliff",
            practice: true,
            modes: 0,
            lo: (0.5, 0.5),
            hi: (15.5, 1.5),
            void: f32::NEG_INFINITY,
            boxes: &[],
            looks: &[],
            spawns: &[([1.0, 0.0, 1.0], 0, 0)],
            pads: &[],
            roam: &[],
            terrain: Some(crate::terrain::TerrainDef {
                cell: 2.0,
                x0: 0.0,
                z0: 0.0,
                nx: 9,
                nz: 2,
                heights: &CLIFF,
            }),
            launchers: &[],
            vehicles: &[],
            solids: &[],
            solid_looks: &[],
            dress: &[],
            dress_solids: &[],
            profiles: &[],
            dress_profiles: &[],
            nav: false,
            light: crate::map::LightDef::DAY,
            flags: &[],
        }
        .build()
    }

    fn standing(map: &Map, x: f32, z: f32) -> Body {
        Body {
            pos: V3::new(x, map.ground(x, z), z),
            grounded: true,
            ..Body::default()
        }
    }

    fn run_x() -> Input {
        Input {
            mz: 127,
            ..Input::default()
        }
    }

    #[test]
    fn a_gentle_slope_is_walked_up_on_the_ground() {
        let map = crate::maps::canyon::DEF.build();
        // Up the south side of the hill at (−60, 28) from its foot.
        let mut b = standing(&map, -64.0, 15.0);
        let up = Input {
            mz: 127,
            yaw: 16384,
            ..Input::default()
        };
        let mut was = b.pos.y;
        for _ in 0..TICK_HZ {
            step(&mut b, &up, &map);
            assert!(b.pos.y >= was - 1e-3, "never down on the way up: {:?}", b.pos);
            was = b.pos.y;
        }
        assert!(b.pos.z > 20.5, "it climbed: {:?}", b.pos);
        assert!(
            b.pos.y > 1.5 && fabs(b.pos.y - map.ground(b.pos.x, b.pos.z)) < 0.01,
            "{:?}",
            b.pos
        );
        assert!(b.grounded);
    }

    #[test]
    fn a_slope_too_steep_stops_a_walk_up_it_and_slides_a_body_off() {
        let map = cliff();
        let mut b = standing(&map, 2.0, 1.0);
        run(&mut b, run_x(), &map, TICK_HZ * 2);
        assert!(b.pos.x < 6.5 && b.pos.y < 1.0, "held at the foot: {:?}", b.pos);
        // Put on the face, it slides back down.
        let mut b = standing(&map, 8.0, 1.0);
        run(&mut b, Input::default(), &map, TICK_HZ);
        assert!(b.pos.x < 6.5, "slid down: {:?}", b.pos);
        assert!(b.pos.y < 0.5 && b.grounded, "{:?}", b.pos);
    }

    /// Open floor with a round pillar, a wall turned 45° and a ramp up +x
    /// from 0 to 3 m: one of each solid (crate::solid).
    fn solids() -> Map {
        crate::map::Def {
            id: 253,
            practice: false,
            modes: 0,
            name: "solids",
            lo: (0.0, -10.0),
            hi: (40.0, 10.0),
            void: f32::NEG_INFINITY,
            boxes: &[],
            looks: &[],
            spawns: &[([1.0, 0.0, 0.0], 0, 0)],
            pads: &[],
            roam: &[],
            terrain: None,
            launchers: &[],
            vehicles: &[],
            solids: &[
                (0, [8.0, 0.3, 1.0, 0.0, 0.0, 0.0, 0.0, 3.0]),
                (1, [18.0, 0.0, 4.0, 0.2, 0.70710677, 0.70710677, 0.0, 3.0]),
                (2, [30.0, -2.0, 36.0, 2.0, 0.0, 0.0, 0.0, 3.0]),
            ],
            solid_looks: &[0, 0, 0],
            dress: &[],
            dress_solids: &[],
            profiles: &[],
            dress_profiles: &[],
            nav: false,
            light: crate::map::LightDef::DAY,
            flags: &[],
        }
        .build()
    }

    #[test]
    fn a_body_slides_round_a_pillar_and_not_through_it() {
        let map = solids();
        let mut b = standing(&map, 4.0, 0.0);
        let p = map.solids[0];
        for _ in 0..TICK_HZ * 2 {
            step(&mut b, &run_x(), &map);
            assert!(!p.over(b.pos.x, b.pos.z, BODY_R - 0.01), "in the pillar: {:?}", b.pos);
        }
        // Off its south side and on past it.
        assert!(b.pos.x > 10.0 && b.pos.z < -0.5, "{:?}", b.pos);
    }

    #[test]
    fn a_turned_wall_holds_a_body_on_its_side() {
        let map = solids();
        let w = map.solids[1];
        // Across the wall (its own z) and along it (its own x).
        let local = |b: &Body| {
            let (dx, dz) = (b.pos.x - 18.0, b.pos.z);
            (dx * 0.70710677 + dz * 0.70710677, dz * 0.70710677 - dx * 0.70710677)
        };
        let mut b = standing(&map, 13.0, 0.0);
        let mut slid = false;
        for _ in 0..TICK_HZ * 2 {
            step(&mut b, &run_x(), &map);
            let (along, across) = local(&b);
            if fabs(along) < 4.0 {
                assert!(across > 0.2 + BODY_R - 0.01, "through the wall: {:?}", b.pos);
                slid |= along > 0.0;
            }
            assert!(!w.over(b.pos.x, b.pos.z, BODY_R - 0.01));
        }
        // It slid along the wall's face, +x +z, and round its end.
        assert!(slid && b.pos.x > 21.0, "{:?}", b.pos);
    }

    #[test]
    fn a_ramp_is_walked_up_and_its_side_is_a_wall() {
        let map = solids();
        let r = map.solids[2];
        let mut b = standing(&map, 28.0, 0.0);
        let mut top = 0.0f32;
        for _ in 0..TICK_HZ * 2 {
            step(&mut b, &run_x(), &map);
            if b.pos.x > 30.5 && b.pos.x < 35.5 {
                assert!(b.grounded, "on the slope: {:?}", b);
                assert!(fabs(b.pos.y - r.top_at(b.pos.x, b.pos.z)) < 0.01, "{:?}", b.pos);
            }
            top = top.max(b.pos.y);
        }
        assert!(fabs(top - 3.0) < 0.01, "to the top: {top}");
        // Into its side where it stands 2.4 m: held off.
        let south = Input {
            mz: 127,
            yaw: 49152,
            ..Input::default()
        };
        let mut b = standing(&map, 34.8, 4.0);
        run(&mut b, south, &map, TICK_HZ);
        assert!(b.pos.z >= 2.0 + BODY_R && b.pos.y == 0.0, "{:?}", b.pos);
        // Where its side is under a step, walked onto.
        let mut b = standing(&map, 30.4, 4.0);
        let mut up = 0.0f32;
        for _ in 0..TICK_HZ / 2 {
            step(&mut b, &south, &map);
            up = up.max(b.pos.y);
        }
        assert!(b.pos.z < 1.0 && up > 0.15, "{:?} {up}", b.pos);
    }

    #[test]
    fn a_launcher_throws_a_body_where_its_map_says() {
        let map = crate::maps::canyon::DEF.build();
        let l = map.launchers[0];
        let at = (l.area.min + l.area.max) * 0.5;
        let mut b = standing(&map, at.x, at.z);
        step(&mut b, &Input::default(), &map);
        assert!(!b.grounded && b.vel.y > JUMP_SPEED, "thrown: {:?}", b);
        let mut ticks = 0;
        while !b.grounded && ticks < TICK_HZ * 4 {
            step(&mut b, &Input::default(), &map);
            ticks += 1;
        }
        // Red's lands near (−20, 0) after about 2.5 s (the creek bed there
        // is a little under where it is aimed, so a little past it).
        assert!(fabs(b.pos.x - -19.0) < 1.5 && fabs(b.pos.z) < 0.5, "{:?}", b.pos);
        assert!((TICK_HZ * 23 / 10..TICK_HZ * 27 / 10).contains(&ticks), "{ticks} ticks");
    }

    #[test]
    fn cloudbreaks_lift_lands_in_its_gallery_from_all_of_its_pad() {
        let map = crate::maps::cloudbreak::DEF.build();
        let a = map.launchers[0].area;
        for (x, z) in [
            (a.min.x, a.min.z),
            (a.max.x, a.min.z),
            (a.min.x, a.max.z),
            (a.max.x, a.max.z),
        ] {
            let mut b = standing(&map, x, z);
            step(&mut b, &Input::default(), &map);
            assert!(!b.grounded, "not thrown from {x}, {z}: {b:?}");
            let mut ticks = 0;
            while !b.grounded && ticks < TICK_HZ * 4 {
                step(&mut b, &Input::default(), &map);
                ticks += 1;
            }
            assert!(fabs(b.pos.y - 8.5) < 0.01, "from {x}, {z} it lands at {:?}", b.pos);
        }
    }

    static ZEROS: [u8; 16] = [0; 16];

    /// Open floor with these profile solids and these other solids on it.
    fn with_profiles(profiles: &'static [crate::map::ProfileDef], solids: &'static [(u8, [f32; 8])]) -> Map {
        crate::map::Def {
            id: 251,
            practice: false,
            modes: 0,
            name: "profiles",
            lo: (-40.0, -20.0),
            hi: (60.0, 20.0),
            void: f32::NEG_INFINITY,
            boxes: &[],
            looks: &[],
            spawns: &[([1.0, 0.0, 0.0], 0, 0)],
            pads: &[],
            roam: &[],
            terrain: None,
            launchers: &[],
            vehicles: &[],
            solids,
            solid_looks: &ZEROS[..solids.len()],
            dress: &[],
            dress_solids: &[],
            profiles,
            dress_profiles: &[],
            nav: false,
            light: crate::map::LightDef::DAY,
            flags: &[],
        }
        .build()
    }

    /// A low dome (a cap 3 m across its radius, 0.8 m high: 30° at its
    /// edge) and a steep one (a hemisphere of 2 m).
    const DOMES: &[crate::map::ProfileDef] = &[
        (
            3,
            [8.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            &[[0.0, 0.0, 0.8], [1.0, 0.0, 0.716], [2.0, 0.0, 0.458], [3.0, 0.0, 0.0]],
            0,
        ),
        (
            3,
            [20.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            &[
                [0.0, 0.0, 2.0],
                [1.0, 0.0, 1.732],
                [1.5, 0.0, 1.323],
                [1.8, 0.0, 0.872],
                [2.0, 0.0, 0.0],
            ],
            0,
        ),
    ];

    #[test]
    fn a_body_walks_over_a_low_dome() {
        let map = with_profiles(DOMES, &[]);
        let d = map.solids[0];
        let mut b = standing(&map, 3.0, 0.0);
        let mut top = 0.0f32;
        for _ in 0..TICK_HZ * 2 {
            step(&mut b, &run_x(), &map);
            if fabs(b.pos.x - 8.0) < 2.5 {
                assert!(b.grounded, "on the dome: {:?}", b);
                // Stood on the slope under its middle, never floating.
                assert!(fabs(b.pos.y - d.top_at(b.pos.x, b.pos.z)) < 0.02, "{:?}", b.pos);
            }
            top = top.max(b.pos.y);
        }
        assert!(
            fabs(top - 0.8) < 0.02 && b.pos.x > 12.0 && b.pos.y == 0.0,
            "over it: {top} {:?}",
            b.pos
        );
    }

    #[test]
    fn a_steep_flank_is_not_climbed_a_step_at_a_time() {
        let map = with_profiles(DOMES, &[]);
        let mut b = standing(&map, 15.0, 0.0);
        for _ in 0..TICK_HZ * 3 {
            step(&mut b, &run_x(), &map);
            assert!(b.pos.y < 0.01, "up the hemisphere: {:?}", b.pos);
        }
        let s = fabs(b.pos.x - 20.0);
        assert!(
            s > 1.8 + BODY_R - 0.02 && s < 2.0 + BODY_R + 0.02,
            "held at its foot: {:?}",
            b.pos
        );
    }

    #[test]
    fn a_jump_into_a_flat_lathe_goes_as_into_a_pillar() {
        // A lathe the shape of a pillar at z = 4, the pillar at z = −4.
        let map = with_profiles(
            &[(
                3,
                [10.0, 4.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
                &[[0.0, 0.0, 1.5], [1.0, 0.0, 1.5]],
                0,
            )],
            &[(0, [10.0, -4.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.5])],
        );
        let jump = Input {
            mz: 127,
            buttons: crate::input::JUMP,
            ..Input::default()
        };
        for start in [6.0, 7.5, 8.2] {
            let (mut a, mut p) = (standing(&map, start, 4.0), standing(&map, start, -4.0));
            for t in 0..TICK_HZ * 2 {
                let i = if t < 2 && start > 7.0 { jump } else { run_x() };
                step(&mut a, &i, &map);
                step(&mut p, &i, &map);
                assert!(
                    fabs(a.pos.y - p.pos.y) < 1e-3,
                    "{start} at {t}: {:?} vs {:?}",
                    a.pos,
                    p.pos
                );
                assert!(
                    fabs((a.pos.x - p.pos.x) + 0.0) < 0.01,
                    "{start} at {t}: {:?} vs {:?}",
                    a.pos,
                    p.pos
                );
            }
        }
    }

    #[test]
    fn a_body_stands_under_a_bulbs_undercut() {
        // A mushroom: a stem 0.3 m round, a cap 2 m round from 2.0 m at the
        // stem to 2.5 at its rim.
        let map = with_profiles(
            &[(
                3,
                [10.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
                &[[0.0, 0.0, 3.0], [0.3, 0.0, 3.0], [0.3, 2.0, 3.0], [2.0, 2.5, 2.5]],
                0,
            )],
            &[],
        );
        let mut b = standing(&map, 4.0, 0.0);
        run(&mut b, run_x(), &map, TICK_HZ * 2);
        let s = fabs(b.pos.x - 10.0);
        assert!(
            fabs(s - (0.3 + BODY_R)) < 0.02 && b.pos.y == 0.0,
            "under the cap at the stem: {:?}",
            b.pos
        );
    }

    #[test]
    fn a_jump_under_a_vault_stops_at_the_arch() {
        // A vault along x from 40 to 50 over z −3..3, springing at 1.9 m,
        // its crown 2.6.
        let map = with_profiles(
            &[(
                5,
                [45.0, 0.0, 0.0, 1.0, 0.0, 5.0, 0.0, 0.0],
                &[
                    [-3.0, 1.9, 3.0],
                    [-2.0, 2.42, 3.0],
                    [-1.0, 2.56, 3.0],
                    [0.0, 2.6, 3.0],
                    [1.0, 2.56, 3.0],
                    [2.0, 2.42, 3.0],
                    [3.0, 1.9, 3.0],
                ],
                0,
            )],
            &[],
        );
        let jump = Input {
            buttons: crate::input::JUMP,
            ..Input::default()
        };
        for (z, roof) in [(0.0, 2.6 - 0.35 * 0.04), (2.2, 2.42 - 0.55 * 0.52)] {
            let mut b = standing(&map, 45.0, z);
            let mut top = 0.0f32;
            for _ in 0..TICK_HZ {
                step(&mut b, &jump, &map);
                top = top.max(b.pos.y + BODY_H);
            }
            assert!(
                top <= roof + 0.01 && top > roof - 0.1,
                "{z}: the head to {top}, the arch at {roof}"
            );
        }
        // Outside, the same jump goes its full height.
        let mut b = standing(&map, 30.0, 0.0);
        let mut top = 0.0f32;
        for _ in 0..TICK_HZ {
            step(&mut b, &jump, &map);
            top = top.max(b.pos.y);
        }
        assert!(top > 2.5, "{top}");
    }

    #[test]
    fn a_spiral_is_walked_up() {
        // A quarter turn round (0, 0) on the circle r = 6, 2 m wide, from 0
        // to 2 m: straight ramps along it, as the compiler lays them.
        let n = 12;
        let (r, w, rise) = (6.0f32, 2.0f32, 2.0f32);
        let th = core::f32::consts::FRAC_PI_2 / n as f32;
        let mut rows: Vec<crate::map::ProfileDef> = Vec::new();
        for i in 0..n {
            let phi = th * (i as f32 + 0.5);
            let (sn, c) = crate::fmath::sin_cos(phi);
            let ymid = rise * (i as f32 + 0.5) / n as f32;
            let hx = (r + w / 2.0) * crate::fmath::tan(th / 2.0);
            let k = (rise / (r * core::f32::consts::FRAC_PI_2)) * th / (2.0 * crate::fmath::tan(th / 2.0));
            let knots: &'static [[f32; 3]] = Box::leak(Box::new([
                [-hx, 0.0, (ymid - k * hx).max(0.0)],
                [hx, 0.0, ymid + k * hx],
            ]));
            rows.push((5, [r * c, r * sn, 0.0, c, sn, w / 2.0, 0.0, 0.0], knots, 0));
        }
        let map = with_profiles(Box::leak(rows.into_boxed_slice()), &[]);
        // From the foot (6, −1.5), up +z, then round.
        let mut b = standing(&map, 6.0, -1.5);
        let (mut was, mut top) = (0.0f32, 0.0f32);
        for t in 0..TICK_HZ * 2 {
            let (x, z) = (b.pos.x, b.pos.z);
            // Along the circle (the tangent, (−z, x)), back toward it.
            let rho = (x * x + z * z).sqrt();
            let k = (rho - r) * 2.0;
            let (dx, dz) = (-z - x * k, x - z * k);
            let yaw = crate::geom::rad_to_yaw(crate::fmath::atan2(dz, dx));
            let i = Input {
                mz: 127,
                yaw,
                ..Input::default()
            };
            step(&mut b, &i, &map);
            if b.pos.z > 0.5 && b.pos.x > 0.5 {
                assert!(b.grounded && b.pos.y >= was - 0.01, "up the spiral at {t}: {:?}", b.pos);
                top = top.max(b.pos.y);
            }
            was = b.pos.y;
        }
        assert!(top > 1.8, "to its top: {top}");
    }

    #[test]
    fn a_push_ends_unblocked_on_the_side_it_came_from() {
        // A wall 0.4 m thick on the circle r = 5 round (0, 0); a body that
        // got past its middle from outside goes back out.
        let map = with_profiles(
            &[(
                3,
                [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
                &[[4.8, 0.0, 3.0], [5.2, 0.0, 3.0]],
                0,
            )],
            &[],
        );
        let s = map.solids[0];
        let band = crate::profile::Band {
            feet: 0.0,
            lo: STEP,
            hi: BODY_H,
            climb: STEP,
        };
        let (x, z, ..) = s.push_band(4.9, 0.0, BODY_R, band, (5.6, 0.0)).unwrap();
        assert!(x > 5.2 + BODY_R && fabs(z) < 1e-4, "out: {x} {z}");
        let (x, ..) = s.push_band(5.1, 0.0, BODY_R, band, (4.4, 0.0)).unwrap();
        assert!(x < 4.8 - BODY_R, "in: {x}");
    }
}

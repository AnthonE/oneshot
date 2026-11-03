//! Walking, jumping and falling: Quake's model, ported from Fireplace's
//! `$move` (`src/players.wat`). Ground friction and acceleration, capped air
//! acceleration (air strafing), gravity, then the map's boxes an axis at a
//! time: x, z, then up or down, stepping onto anything up to [`STEP`] high.

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
        let e = if self.crouched { CROUCH_EYE } else { EYE };
        V3::new(self.pos.x, self.pos.y + e, self.pos.z)
    }
}

/// Snap to the 1/1024 grid (consts::QUANT).
pub fn quant(v: f32) -> f32 {
    fmath::round(v * QUANT) * (1.0 / QUANT)
}

fn quant3(v: V3) -> V3 {
    V3::new(quant(v.x), quant(v.y), quant(v.z))
}

/// Quake's accelerate: speed along `w` (unit) up to `cap`, gaining at most
/// `rate · wish · DT` this tick.
fn accelerate(vel: V3, w: V3, cap: f32, wish: f32, rate: f32) -> V3 {
    let add = cap - vel.dot(w);
    if add <= 0.0 {
        return vel;
    }
    let acc = (rate * wish * DT).min(add);
    V3::new(vel.x + w.x * acc, vel.y, vel.z + w.z * acc)
}

/// One tick of a body under one input.
pub fn step(b: &mut Body, inp: &Input, map: &Map) {
    // Crouch while held; stand back up only where the head fits.
    if inp.held(CROUCH) {
        b.crouched = true;
    } else if b.crouched && map.ceiling(b.pos.x, b.pos.z, BODY_R, b.pos.y + 0.01) >= b.pos.y + BODY_H {
        b.crouched = false;
    }

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
        let top = if b.crouched { CROUCH_RUN } else { RUN };
        wish = top * len.min(1.0);
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
        v = accelerate(v, w, wish, wish, ACCEL);
        if inp.held(JUMP) && !b.crouched {
            v.y = JUMP_SPEED;
            b.grounded = false;
        }
    } else {
        v = accelerate(v, w, wish.min(AIR_CAP), wish, AIR_ACCEL);
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
    let nx = sweep(
        map,
        p.x,
        tx,
        (
            (p.x.min(tx) - BODY_R, p.z - BODY_R),
            (p.x.max(tx) + BODY_R, p.z + BODY_R),
        ),
        |bx| (bx.min.x, bx.max.x),
        |bx| bx.over_z(p.z) && bx.min.y < hi && bx.max.y > lo,
    );
    if nx != tx {
        v.x = 0.0;
    }
    p.x = nx.clamp(map.lo.0, map.hi.0);
    let tz = p.z + v.z * DT;
    let nz = sweep(
        map,
        p.z,
        tz,
        (
            (p.x - BODY_R, p.z.min(tz) - BODY_R),
            (p.x + BODY_R, p.z.max(tz) + BODY_R),
        ),
        |bx| (bx.min.z, bx.max.z),
        |bx| bx.over_x(p.x) && bx.min.y < hi && bx.max.y > lo,
    );
    if nz != tz {
        v.z = 0.0;
    }
    p.z = nz.clamp(map.lo.1, map.hi.1);

    let reach = p.y + if b.grounded { STEP } else { 0.01 };
    let floor = map.floor(p.x, p.z, BODY_R, reach);
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

    b.pos = quant3(p);
    b.vel = quant3(v);
}

/// Moves a coordinate from `from` toward `to` until it meets a box face
/// that `blocks` says is in the way on the other axes. "Was outside" allows
/// [`FACE_EPS`], so a body the grid left a hair inside a face it rests
/// against is still held by it. `area` (x and z, low and high) holds every
/// box that could stop the move; the answer is the nearest stop, so which
/// boxes are visited, and in what order, never changes it.
fn sweep(
    map: &Map,
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
        if to > from && from + BODY_R <= lo + FACE_EPS && t + BODY_R > lo {
            t = lo - BODY_R;
        } else if to < from && from - BODY_R >= hi - FACE_EPS && t - BODY_R < hi {
            t = hi + BODY_R;
        }
    });
    t
}

/// Far more than the grid's half step (1/2048 m), far less than a tick's
/// travel at a walk.
const FACE_EPS: f32 = 0.01;

impl crate::geom::Aabb {
    fn over_z(&self, z: f32) -> bool {
        self.min.z < z + BODY_R && self.max.z > z - BODY_R
    }
    fn over_x(&self, x: f32) -> bool {
        self.min.x < x + BODY_R && self.max.x > x - BODY_R
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        for _ in 0..TICK_HZ {
            step(&mut b, &Input::default(), &map);
            top = top.max(b.pos.y);
        }
        assert!(b.grounded && b.pos.y == 0.0);
        assert!(top > 1.1 && top < 1.3, "apex {top}");
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
}

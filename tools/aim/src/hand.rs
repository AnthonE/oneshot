//! A person's hand on the mouse, so honest aim isn't a bot's
//! (docs/ANTICHEAT.md §9: bots aim like bots). Laid over a bot's input while
//! it fights: the bot still decides whom to fight, where to walk and when
//! to pull; the hand moves the view onto the target as a person's does.
//!
//! - **Reaction:** a new target isn't turned to for `react_ms` (and a bit,
//!   each time).
//! - **Flicks:** a far target is reached by a ballistic movement on a
//!   minimum-jerk profile, timed by Fitts' law, that lands short or long by
//!   `gain` and a little off its line; what is left is another, smaller
//!   one after a short dwell.
//! - **Pursuit:** the target's motion across the view is matched (by
//!   `pursuit`, a share of it) all along, flicks or not, and a near target's
//!   offset closed with a lag (`lag_ms`).
//! - **Tremor:** a slow drift and a ~10 Hz shake.
//! - **The mouse:** the view turns in whole counts of `step_deg`.
//! - **The finger:** the trigger the bot pulls is let through only once the
//!   aim has sat on the target for `click_ms` (a person sees the crosshair
//!   on and then clicks), and held while it stays near.
//!
//! Each seat draws its own traits from the match's seed: a person, not the
//! person. With no target the bot's own input goes through, so it still
//! finds its way about the map.

use room::assist::{aim_point, angles_to, wrap_deg};
use room::genome::Draw;
use sim::consts::{ticks, DT, PITCH_MAX};
use sim::fmath;
use sim::geom::rad_to_yaw;
use sim::input::{Input, FIRE, FIRE_LEFT};
use sim::world::World;

/// Past this far off, degrees, the hand flicks; nearer, it follows.
const FLICK_DEG: f32 = 3.0;
/// Fitts' target width, degrees: about a body at mid range.
const WIDTH_DEG: f32 = 2.0;
/// Between one flick and the correction after it.
const DWELL_MS: f32 = 90.0;
/// The shake's rate, Hz.
const SHAKE_HZ: f32 = 10.0;
/// How much of a tick's change of the target's bearing the eye takes in.
const VEL_K: f32 = 0.2;
/// On target for the finger: within this much of the body's half width at
/// that distance, metres, or a degree, whichever is wider; it stays on to
/// twice that.
const ON_HALF_M: f32 = 0.35;

/// One person's hand.
#[derive(Clone, Copy, Debug)]
pub struct Traits {
    pub react_ms: f32,
    /// A flick's length over the distance, on average, and its spread.
    pub gain: f32,
    pub gain_sd: f32,
    /// A flick's line off the target's, degrees (spread).
    pub dir_sd_deg: f32,
    /// Fitts' law: a flick takes `a + b·log2(1 + D/W)`.
    pub fitts_a_ms: f32,
    pub fitts_b_ms: f32,
    pub lag_ms: f32,
    /// The share of the target's motion across the view it matches.
    pub pursuit: f32,
    pub tremor_deg: f32,
    pub step_deg: f32,
    /// From the aim settling on the target to the click.
    pub click_ms: f32,
    /// Where on the body it aims: 0 the chest, 1 the head.
    pub bone: f32,
}

impl Traits {
    pub fn json(&self) -> serde_json::Value {
        let r = |x: f32| (x * 1000.0).round() / 1000.0;
        serde_json::json!({
            "react_ms": r(self.react_ms), "gain": r(self.gain), "gain_sd": r(self.gain_sd),
            "dir_sd_deg": r(self.dir_sd_deg), "fitts_a_ms": r(self.fitts_a_ms), "fitts_b_ms": r(self.fitts_b_ms),
            "lag_ms": r(self.lag_ms), "pursuit": r(self.pursuit), "tremor_deg": r(self.tremor_deg), "step_deg": r(self.step_deg), "click_ms": r(self.click_ms), "bone": r(self.bone),
        })
    }
}

/// A ballistic movement under way.
#[derive(Clone, Copy, Debug)]
struct Flick {
    by: (f32, f32),
    len: u32,
    at: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct Hand {
    pub traits: Traits,
    rng: Draw,
    /// The target, when it was taken and how long this person takes to
    /// react to it, ticks.
    target: Option<(usize, u32, u32)>,
    flick: Option<Flick>,
    /// No new flick before this tick.
    dwell: u32,
    phase: f32,
    drift: (f32, f32),
    /// Turn not yet a whole count.
    carry: (f32, f32),
    /// The target's bearing last tick, and how fast it moves across the
    /// view (degrees a tick, smoothed: what the eye has of it).
    bearing: Option<(f32, f32)>,
    vel: (f32, f32),
    /// Since when the aim has been on the target, and how long this time
    /// before the finger clicks, ticks.
    on: Option<(u32, u32)>,
}

/// Minimum jerk: where a movement is, of its length, at `t` of its time.
fn min_jerk(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * t * (10.0 + t * (-15.0 + 6.0 * t))
}

impl Hand {
    /// A person drawn from `seed`.
    pub fn new(seed: u32) -> Hand {
        let mut rng = Draw(seed);
        let mut u = |lo: f32, hi: f32| lo + (hi - lo) * rng.draw();
        let traits = Traits {
            react_ms: u(160.0, 300.0),
            gain: u(0.85, 1.0),
            gain_sd: u(0.04, 0.12),
            dir_sd_deg: u(1.0, 6.0),
            fitts_a_ms: u(30.0, 80.0),
            fitts_b_ms: u(40.0, 90.0),
            lag_ms: u(60.0, 160.0),
            pursuit: u(0.75, 0.95),
            tremor_deg: u(0.02, 0.15),
            step_deg: u(0.005, 0.05),
            click_ms: u(120.0, 260.0),
            bone: u(0.1, 0.6),
        };
        Hand {
            traits,
            rng: Draw(seed ^ 0x6b43_a9b5),
            target: None,
            flick: None,
            dwell: 0,
            phase: 0.0,
            drift: (0.0, 0.0),
            carry: (0.0, 0.0),
            bearing: None,
            vel: (0.0, 0.0),
            on: None,
        }
    }

    /// Unit-variance noise.
    fn normal(&mut self) -> f32 {
        (self.rng.draw() + self.rng.draw() + self.rng.draw() - 1.5) * 2.0
    }

    /// Moves seat `s`'s view toward `target` (the bot's), rewriting `inp`'s
    /// aim; with no target `inp` is left as the bot made it.
    pub fn steer(&mut self, w: &World, s: usize, target: Option<usize>, inp: &mut Input) {
        let me = &w.fighters[s];
        let target = target.filter(|&t| t != s && me.active() && w.fighters[t].active());
        let Some(t) = target else {
            self.target = None;
            self.flick = None;
            self.bearing = None;
            self.on = None;
            return;
        };
        let tr = self.traits;
        if self.target.is_none_or(|(q, _, _)| q != t) {
            let jitter = 0.8 + 0.5 * self.rng.draw();
            self.target = Some((t, w.tick, ticks((tr.react_ms * jitter) as u32)));
            self.flick = None;
            self.bearing = None;
            self.vel = (0.0, 0.0);
        }
        let (_, since, react) = self.target.unwrap_or((t, w.tick, 0));
        let view = (me.yaw as f32 * (360.0 / 65536.0), me.pitch as f32 * (360.0 / 65536.0));
        let (gy, gp) = angles_to(me.eye(), aim_point(&w.fighters[t], tr.bone));
        let e = (wrap_deg(gy - view.0), gp - view.1);
        let dist = (e.0 * e.0 + e.1 * e.1).sqrt();
        if let Some((by, bp)) = self.bearing {
            self.vel.0 += (wrap_deg(gy - by) - self.vel.0) * VEL_K;
            self.vel.1 += (gp - bp - self.vel.1) * VEL_K;
        }
        self.bearing = Some((gy, gp));

        let mut d = (0.0, 0.0);
        if w.tick.saturating_sub(since) >= react {
            // Pursuit matches the target's motion, flicking or not.
            d = (self.vel.0 * tr.pursuit, self.vel.1 * tr.pursuit);
            if let Some(mut f) = self.flick {
                let (a, b) = (
                    min_jerk(f.at as f32 / f.len as f32),
                    min_jerk((f.at + 1) as f32 / f.len as f32),
                );
                d.0 += f.by.0 * (b - a);
                d.1 += f.by.1 * (b - a);
                f.at += 1;
                if f.at >= f.len {
                    self.flick = None;
                    self.dwell = w.tick + ticks((DWELL_MS * (0.7 + 0.6 * self.rng.draw())) as u32);
                } else {
                    self.flick = Some(f);
                }
            } else if dist > FLICK_DEG && w.tick >= self.dwell {
                let amp = (tr.gain + tr.gain_sd * self.normal()).max(0.3);
                let off = (tr.dir_sd_deg * self.normal()).to_radians();
                let (sn, cs) = fmath::sin_cos(off);
                let by = ((e.0 * cs - e.1 * sn) * amp, (e.0 * sn + e.1 * cs) * amp);
                let ms = tr.fitts_a_ms + tr.fitts_b_ms * fmath::log2(1.0 + dist / WIDTH_DEG);
                let len = ticks(ms as u32).max(2);
                let a = min_jerk(1.0 / len as f32);
                d.0 += by.0 * a;
                d.1 += by.1 * a;
                self.flick = Some(Flick { by, len, at: 1 });
            } else {
                // Near: the offset closed with a lag.
                let k = (DT * 1000.0 / tr.lag_ms).min(1.0);
                d.0 += e.0 * k;
                d.1 += e.1 * k;
            }
        }

        // Tremor: a drift that wanders back, and a shake.
        self.drift.0 = self.drift.0 * 0.99 + 0.02 * tr.tremor_deg * self.normal();
        self.drift.1 = self.drift.1 * 0.99 + 0.02 * tr.tremor_deg * self.normal();
        self.phase += core::f32::consts::TAU * SHAKE_HZ * DT;
        let shake = tr.tremor_deg * 0.3 * fmath::sin(self.phase);
        d.0 += self.drift.0 * 0.05 + shake;
        d.1 += self.drift.1 * 0.05 + shake * 0.5;

        // The mouse moves in whole counts.
        self.carry.0 += d.0;
        self.carry.1 += d.1;
        let counts = (
            fmath::round(self.carry.0 / tr.step_deg),
            fmath::round(self.carry.1 / tr.step_deg),
        );
        let turn = (counts.0 * tr.step_deg, counts.1 * tr.step_deg);
        self.carry.0 -= turn.0;
        self.carry.1 -= turn.1;
        inp.yaw = rad_to_yaw((view.0 + turn.0).to_radians());
        let p = fmath::round((view.1 + turn.1) * (65536.0 / 360.0));
        inp.pitch = p.clamp(-PITCH_MAX as f32, PITCH_MAX as f32) as i16;

        // The finger: the bot's pull only once the aim has sat on.
        let range = (aim_point(&w.fighters[t], tr.bone) - me.eye()).len().max(1.0);
        let cone = fmath::atan2(ON_HALF_M, range).to_degrees().max(1.0);
        let left = (e.0 - turn.0).hypot(e.1 - turn.1);
        let near = if self.on.is_some() { 2.0 * cone } else { cone };
        if left < near {
            if self.on.is_none() {
                let wait = ticks((tr.click_ms * (0.7 + 0.6 * self.rng.draw())) as u32);
                self.on = Some((w.tick, wait));
            }
        } else {
            self.on = None;
        }
        if self.on.is_none_or(|(since, wait)| w.tick - since < wait) {
            inp.buttons &= !(FIRE | FIRE_LEFT);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim::bot::Bot;
    use sim::consts::{MAX_FIGHTERS, TICK_HZ};
    use sim::map::Map;
    use sim::world::{Lag, Rules, World};

    /// A bot with a hand still gets onto whom it fights: most of the time
    /// it has an enemy in plain sight, it is within a few degrees of them.
    #[test]
    fn the_hand_gets_on_target() {
        let mut w = World::new(Map::proving_grounds(), Rules::duel(), 3, (3, 4));
        let mut bots = [Bot::new(3, 0, 0.6), Bot::new(3, 1, 0.6)];
        let mut hands = [Hand::new(11), Hand::new(12)];
        for s in 0..2 {
            w.seat(s, s as u8);
        }
        let (mut sight, mut near, mut far) = (0, 0, 0);
        for _ in 0..90 * TICK_HZ {
            let mut inputs = [Input::default(); MAX_FIGHTERS];
            for s in 0..2 {
                inputs[s] = bots[s].think(&w, s);
                hands[s].steer(&w, s, bots[s].target(), &mut inputs[s]);
            }
            w.step_with(&inputs, &Lag::past([0; MAX_FIGHTERS]));
            let me = &w.fighters[0];
            let Some(t) = bots[0].target() else { continue };
            if !me.active() || !w.fighters[t].active() || !room::fog::plainly_sees(&w.map, me, &w.fighters[t]) {
                continue;
            }
            sight += 1;
            let (gy, gp) = angles_to(me.eye(), aim_point(&w.fighters[t], 0.5));
            let e = wrap_deg(gy - me.yaw as f32 * (360.0 / 65536.0)).hypot(gp - me.pitch as f32 * (360.0 / 65536.0));
            if e < 5.0 {
                near += 1;
            }
            if e > 45.0 {
                far += 1;
            }
        }
        eprintln!("{sight} ticks in sight: {near} within 5°, {far} past 45°");
        assert!(sight > 500 && near * 2 > sight, "{near} of {sight} within 5°");
    }
}

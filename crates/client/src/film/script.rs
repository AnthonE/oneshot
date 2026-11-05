//! The shot list: JSON, as `ci/film.sh` and the clip styles (`clip.rs`)
//! write it. After Gates' `render/film.rs` (MIT): the same fields, less the
//! weather and the hour (a map's mood is the map's) and the animals.
//!
//! ```json
//! {"tape": "film/duel.tape", "out": "traces/film", "width": 1920, "height": 1080,
//!  "fps": 30, "look": {"saturation": 1.06},
//!  "shots": [
//!    {"name": "open", "at": 12, "len": 6, "keys": [
//!       {"t": 0, "pos": [-20, 6, 0], "look": [0, 1, 0]},
//!       {"t": 6, "pos": [-8, 3, 4], "look": [0, 1, 0]}], "track": {}},
//!    {"name": "chase", "at": 20, "len": 8, "follow": {"back": 3.4, "up": 1.5}},
//!    {"name": "eyes", "at": -10, "len": 9.5, "pov": true}]}
//! ```
//!
//! Heights in `pos` and `look` are metres above the ground under the point
//! (`sim::map::Map::ground`), so a path can be written from a map's plan. A
//! negative `at` counts back from the tape's end ([`Script::anchor`]), the
//! way a clip's list is written. `slot` names a seat to follow or track; none
//! is the tape's own player. Fog shaped the tape: anyone but the player is
//! there only while they could be seen, so a far camera holds the player.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy::render::view::{ColorGrading, ColorGradingGlobal, ColorGradingSection};
use serde::Deserialize;

/// Frames a shot holds its first pose before rolling, by default: what has
/// just come into view after a jump compiles its pipelines, and the
/// effects and TAA's history fill.
pub const SETTLE_FRAMES: u32 = 45;
/// The most a settle may run past [`SETTLE_FRAMES`] waiting on pipelines
/// before it rolls anyway and says so.
pub const SETTLE_MAX_FRAMES: u32 = 300;
/// Tape the boot plays before it holds the clock: the welcome and the
/// first snapshots, which are what place the eye.
pub const BOOT_TAPE_MS: f64 = 2_000.0;
/// The audio rate the film's mix is rendered at: video's, and a whole
/// number of samples a frame at 24, 25, 30 and 60 fps.
pub const FILM_RATE: u32 = 48_000;
/// Frames a still holds a stopped world before it is taken: TAA's history
/// is an exponential average, and by here it has converged.
pub const STILL_HOLD: u32 = 40;
/// The shutter a shot gets unless it names one: 180°, the cinema default.
pub const SHUTTER: f32 = 0.5;
/// The aperture a focused shot gets unless it names one.
pub const FSTOP: f32 = 2.8;
/// The graphics a film draws with unless its list names a rung.
pub const QUALITY: &str = "high";

fn default_width() -> u32 {
    1920
}
fn default_height() -> u32 {
    1080
}
fn default_fps() -> u32 {
    30
}
fn one() -> u32 {
    1
}
fn real_time() -> Span {
    Span::At(1.0)
}
fn default_settle() -> u32 {
    SETTLE_FRAMES
}
fn default_hold() -> u32 {
    STILL_HOLD
}
fn default_quality() -> String {
    QUALITY.into()
}

/// The shot list.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Script {
    /// The tape (`oneshot --record`, `ci/film.sh record`, or the clip key's).
    /// Unset for a clip, whose tape is handed over with the list.
    #[serde(default)]
    pub tape: PathBuf,
    /// Where the shots go: `NN-name.mp4` beside `NN-name.wav`, or
    /// `NN-name.png` for a still. Unset in a page, which hands them to its
    /// sink.
    #[serde(default)]
    pub out: PathBuf,
    /// The size of what is written.
    #[serde(default = "default_width")]
    pub width: u32,
    #[serde(default = "default_height")]
    pub height: u32,
    /// Draw at this many times the size and fold it down with Lanczos: 2 is
    /// four samples a pixel, and four times the frame time. The desktop's.
    #[serde(default = "one")]
    pub supersample: u32,
    #[serde(default = "default_fps")]
    pub fps: u32,
    /// Vertical field of view in degrees for shots that name none; unset,
    /// a pov shot keeps the player's own and the rest take the rig's.
    #[serde(default)]
    pub fov: Option<f32>,
    /// Frames each shot holds its first pose before rolling.
    #[serde(default = "default_settle")]
    pub settle: u32,
    /// Frames a still holds the stopped world before it is taken.
    #[serde(default = "default_hold")]
    pub hold: u32,
    /// The graphics rung: potato, low, medium or high.
    #[serde(default = "default_quality")]
    pub quality: String,
    /// The lens and grade every shot starts from; a shot's own `look` wins
    /// field by field.
    #[serde(default)]
    pub look: Look,
    pub shots: Vec<Shot>,
    /// The window to draw in, when it is not the video's size: a clip on a
    /// desktop whose screen is shorter than the video (`oneshot --clip
    /// --max-height`) is drawn smaller and scaled up by the encoder.
    #[serde(skip)]
    pub draw: Option<(u32, u32)>,
}

impl Script {
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::parse(&text).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// A shot list from its JSON.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut s: Script = serde_json::from_str(text).map_err(|e| e.to_string())?;
        if s.shots.is_empty() {
            return Err("no shots".into());
        }
        if s.fps == 0 || !FILM_RATE.is_multiple_of(s.fps) {
            return Err(format!("fps {} does not divide {FILM_RATE} Hz audio", s.fps));
        }
        if !(1..=4).contains(&s.supersample) {
            return Err(format!("supersample {} is not 1 to 4", s.supersample));
        }
        if s.width < 16 || s.height < 16 || !s.width.is_multiple_of(2) || !s.height.is_multiple_of(2) {
            return Err(format!("{}x{}: an even size, 16 or more a side", s.width, s.height));
        }
        if crate::look::quality::Quality::from_name(&s.quality).is_none() {
            return Err(format!("quality {}: potato, low, medium or high", s.quality));
        }
        for shot in &s.shots {
            shot.check()?;
        }
        s.look.check("the list")?;
        // The tape only plays forward, so the shots are taken in tape order.
        s.shots.sort_by(|a, b| a.at.total_cmp(&b.at));
        Ok(s)
    }

    /// The rung the list draws at.
    pub fn quality(&self) -> crate::look::quality::Quality {
        crate::look::quality::Quality::from_name(&self.quality).unwrap_or_default()
    }

    /// Put every shot whose `at` is negative (seconds before the tape's end,
    /// the way a clip's list is written) on the tape, no earlier than the
    /// boot and the first settle leave room for.
    pub fn anchor(&mut self, end_s: f64) {
        let floor = BOOT_TAPE_MS / 1000.0 + self.settle as f64 / self.fps as f64;
        for shot in &mut self.shots {
            if shot.at < 0.0 {
                shot.at = (end_s + shot.at).max(floor);
            }
        }
        self.shots.sort_by(|a, b| a.at.total_cmp(&b.at));
    }

    /// The window: what is drawn, before any supersample is folded down.
    pub fn window(&self) -> (u32, u32) {
        self.draw
            .unwrap_or((self.width * self.supersample, self.height * self.supersample))
    }

    /// Draw no taller than `px`: the same shape, smaller, for a screen the
    /// video's own size would not fit on. Sizes stay even, as H.264 wants.
    pub fn fit_height(&mut self, px: u32) {
        let (w, h) = (self.width * self.supersample, self.height * self.supersample);
        if px >= h {
            return;
        }
        let even = |v: f64| ((v / 2.0).round() as u32 * 2).max(2);
        self.draw = Some((even(w as f64 * px as f64 / h as f64), even(px as f64)));
    }

    /// Frames the whole list writes, and those before shot `i` (with the
    /// frame shot `i` is on), for a progress bar.
    pub fn frames_to(&self, i: usize, frame: u32) -> (u32, u32) {
        let total = self.shots.iter().map(|s| s.frames(self)).sum();
        let done = self.shots[..i.min(self.shots.len())]
            .iter()
            .map(|s| s.frames(self))
            .sum::<u32>()
            + frame;
        (done, total)
    }
}

/// One value, one eased to another across the shot, or keys at screen
/// seconds (`[[0, 1], [2.2, 1], [2.5, 0.2]]`), eased from each to the next.
#[derive(Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum Span {
    At(f32),
    Between([f32; 2]),
    Keys(Vec<[f32; 2]>),
}

impl Span {
    /// The value at shot fraction `u`, or for keys at screen second `t`.
    pub fn at(&self, u: f32, t: f32) -> f32 {
        match self {
            Span::At(v) => *v,
            Span::Between([a, b]) => a + (b - a) * u,
            Span::Keys(k) => keyed(k, t),
        }
    }

    fn check(&self, what: &str) -> Result<(), String> {
        if let Span::Keys(k) = self {
            if k.is_empty() || k.windows(2).any(|w| w[1][0] <= w[0][0]) {
                return Err(format!("{what}: keys need rising times"));
            }
        }
        Ok(())
    }

    fn min(&self) -> f32 {
        match self {
            Span::At(v) => *v,
            Span::Between([a, b]) => a.min(*b),
            Span::Keys(k) => k.iter().map(|k| k[1]).fold(f32::INFINITY, f32::min),
        }
    }
}

/// Keys joined by a smoothstep, so a ramp leaves one value and settles on
/// the next rather than lurching.
fn keyed(k: &[[f32; 2]], t: f32) -> f32 {
    let (Some(first), Some(last)) = (k.first(), k.last()) else {
        return 0.0;
    };
    if t <= first[0] {
        return first[1];
    }
    for w in k.windows(2) {
        if t < w[1][0] {
            let s = ((t - w[0][0]) / (w[1][0] - w[0][0])).clamp(0.0, 1.0);
            let s = s * s * (3.0 - 2.0 * s);
            return w[0][1] + (w[1][1] - w[0][1]) * s;
        }
    }
    last[1]
}

/// What the lens is focused on.
#[derive(Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum Focus {
    On(FocusOn),
    /// Metres from the lens.
    Metres(Span),
}

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FocusOn {
    /// The point the camera is aimed at.
    Look,
    /// The followed or tracked body, pulled after it like a focus puller.
    Subject,
}

/// How the camera sees: lens and grade. Every field is optional so that a
/// shot's `look` overrides the list's one field at a time.
#[derive(Deserialize, Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct Look {
    /// Shutter angle as a fraction of a frame: 0.5 is 180°, 0 no blur.
    pub shutter: Option<f32>,
    /// What is sharp. Unset is everything.
    pub focus: Option<Focus>,
    /// Aperture in f-stops: smaller is a shallower field.
    pub fstop: Option<f32>,
    /// Colour fringing toward the frame's edge, a fraction of its width.
    pub fringe: Option<f32>,
    /// A hand on the camera: wobble in degrees. A span, so a blast can kick.
    pub shake: Option<Span>,
    /// Dutch angle, degrees.
    pub roll: Option<Span>,
    /// Stops over (or under) the map's own exposure.
    pub exposure: Option<Span>,
    /// White balance: + warmer, − cooler (CIE x offset, ~±0.2).
    pub temperature: Option<f32>,
    /// + magenta, − green.
    pub tint: Option<f32>,
    /// After the tone curve; 1 is neutral.
    pub saturation: Option<f32>,
    /// About mid-grey; 1 is neutral.
    pub contrast: Option<f32>,
    /// CDL offset on the shadows; 0 is neutral, − crushes.
    pub lift: Option<f32>,
    /// CDL slope on the highlights; 1 is neutral.
    pub gain: Option<f32>,
    /// CDL power on the midtones; 1 is neutral, > 1 darkens.
    pub gamma: Option<f32>,
    /// Saturation of the shadows and the highlights alone.
    pub shadow_sat: Option<f32>,
    pub highlight_sat: Option<f32>,
}

impl Look {
    /// `self` where it says something, `base` where it does not.
    pub fn over(&self, base: &Look) -> Look {
        Look {
            shutter: self.shutter.or(base.shutter),
            focus: self.focus.clone().or_else(|| base.focus.clone()),
            fstop: self.fstop.or(base.fstop),
            fringe: self.fringe.or(base.fringe),
            shake: self.shake.clone().or_else(|| base.shake.clone()),
            roll: self.roll.clone().or_else(|| base.roll.clone()),
            exposure: self.exposure.clone().or_else(|| base.exposure.clone()),
            temperature: self.temperature.or(base.temperature),
            tint: self.tint.or(base.tint),
            saturation: self.saturation.or(base.saturation),
            contrast: self.contrast.or(base.contrast),
            lift: self.lift.or(base.lift),
            gain: self.gain.or(base.gain),
            gamma: self.gamma.or(base.gamma),
            shadow_sat: self.shadow_sat.or(base.shadow_sat),
            highlight_sat: self.highlight_sat.or(base.highlight_sat),
        }
    }

    fn check(&self, n: &str) -> Result<(), String> {
        for (what, s) in [
            ("shake", &self.shake),
            ("roll", &self.roll),
            ("exposure", &self.exposure),
        ] {
            if let Some(s) = s {
                s.check(&format!("`{n}` {what}"))?;
            }
        }
        if let Some(Focus::Metres(s)) = &self.focus {
            s.check(&format!("`{n}` focus"))?;
        }
        if self.fstop.is_some_and(|f| f <= 0.0) {
            return Err(format!("`{n}`: fstop must be positive"));
        }
        Ok(())
    }

    /// The grade at this moment of the shot.
    pub fn grading(&self, u: f32, t: f32) -> ColorGrading {
        let section = |sat: Option<f32>| ColorGradingSection {
            saturation: sat.unwrap_or(1.0),
            contrast: self.contrast.unwrap_or(1.0),
            ..default()
        };
        let mut g = ColorGrading {
            global: ColorGradingGlobal {
                exposure: self.exposure.as_ref().map_or(0.0, |e| e.at(u, t)),
                temperature: self.temperature.unwrap_or(0.0),
                tint: self.tint.unwrap_or(0.0),
                post_saturation: self.saturation.unwrap_or(1.0),
                ..default()
            },
            shadows: section(self.shadow_sat),
            midtones: section(None),
            highlights: section(self.highlight_sat),
        };
        g.shadows.lift = self.lift.unwrap_or(0.0);
        g.highlights.gain = self.gain.unwrap_or(1.0);
        g.midtones.gamma = self.gamma.unwrap_or(1.0);
        g
    }
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Shot {
    pub name: String,
    /// Seconds into the tape at the shot's first frame; negative counts
    /// back from its end.
    pub at: f64,
    /// Seconds of screen time. A still has none.
    #[serde(default)]
    pub len: f64,
    /// Tape seconds per screen second: 1 is real time, 4 a time-lapse, and
    /// keys ramp it (`[[0, 1], [2, 1], [2.4, 0.2]]` into slow motion).
    #[serde(default = "real_time")]
    pub speed: Span,
    /// Stop the world at `at` and write one PNG from the first pose.
    #[serde(default)]
    pub still: bool,
    /// Vertical field of view, degrees; a pair is a zoom.
    #[serde(default)]
    pub fov: Option<Span>,
    /// Smoothstep the shot clock, so the move starts and ends at rest.
    #[serde(default)]
    pub ease: bool,
    /// A camera path: at least two keys (one, for a still).
    #[serde(default)]
    pub keys: Vec<Key>,
    #[serde(default)]
    pub orbit: Option<Orbit>,
    #[serde(default)]
    pub follow: Option<Follow>,
    /// Keep a body in frame: the keys or the orbit move the camera, this
    /// aims it.
    #[serde(default)]
    pub track: Option<Track>,
    /// Through the player's own eyes: what they saw, the gun in view, the
    /// scope, the chase camera at the wheel and the kill cam.
    #[serde(default)]
    pub pov: bool,
    /// This shot's lens and grade, over the list's.
    #[serde(default)]
    pub look: Look,
}

impl Shot {
    pub fn check(&self) -> Result<(), String> {
        let n = &self.name;
        let keyed = self.keys.len() >= 2 || (self.still && self.keys.len() == 1);
        let paths = usize::from(keyed)
            + usize::from(self.orbit.is_some())
            + usize::from(self.follow.is_some())
            + usize::from(self.pov);
        if paths != 1 {
            return Err(format!(
                "shot `{n}`: give exactly one of keys (two or more), orbit, follow or pov"
            ));
        }
        if self.pov && self.track.is_some() {
            return Err(format!("shot `{n}`: a pov shot looks where the player looks"));
        }
        if self.track.is_some() && self.follow.is_some() {
            return Err(format!("shot `{n}`: track aims a camera keys or an orbit move"));
        }
        if !self.at.is_finite() || (!self.still && (self.len <= 0.0 || self.speed.min() <= 0.0)) {
            return Err(format!("shot `{n}`: len and speed must be positive"));
        }
        for (what, s) in [("speed", Some(&self.speed)), ("fov", self.fov.as_ref())] {
            if let Some(s) = s {
                s.check(&format!("shot `{n}` {what}"))?;
            }
        }
        if let Some(o) = &self.orbit {
            o.radius.check(&format!("shot `{n}` orbit radius"))?;
            o.height.check(&format!("shot `{n}` orbit height"))?;
        }
        let slot = self.follow.and_then(|f| f.slot).or(self.track.and_then(|t| t.slot));
        if slot.is_some_and(|s| s as usize >= sim::consts::MAX_FIGHTERS) {
            return Err(format!("shot `{n}`: no seat {}", slot.unwrap_or_default()));
        }
        self.look.check(&format!("shot {n}"))
    }

    /// Frames the shot writes.
    pub fn frames(&self, s: &Script) -> u32 {
        if self.still {
            1
        } else {
            (self.len * s.fps as f64).round().max(1.0) as u32
        }
    }

    /// Tape seconds the shot spans: its speed summed frame by frame.
    pub fn tape_s(&self, fps: u32) -> f64 {
        if self.still {
            return 0.0;
        }
        let frames = (self.len * fps as f64).round().max(1.0) as u32;
        (0..frames)
            .map(|f| {
                let t = f as f32 / fps as f32;
                self.speed.at(t / self.len as f32, t) as f64
            })
            .sum::<f64>()
            / fps as f64
    }

    /// A third-person shot: the player's own body is drawn, the gun in
    /// view is not.
    pub fn third(&self) -> bool {
        !self.pov
    }
}

/// A camera key.
#[derive(Deserialize, Clone, Copy, Debug)]
#[serde(deny_unknown_fields)]
pub struct Key {
    /// Screen seconds into the shot.
    pub t: f32,
    /// `[x, above ground, z]`.
    pub pos: [f32; 3],
    /// What the camera looks at, `[x, above ground, z]`.
    pub look: [f32; 3],
}

/// Circle `center` at `radius`, `height` above the centre, from one bearing
/// to another (degrees; 0 is +Z, 90 is +X). Radius and height may ease,
/// which is a push-in or a crane.
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Orbit {
    pub center: [f32; 3],
    pub radius: Span,
    pub height: Span,
    pub from: f32,
    pub to: f32,
}

/// Ride behind a seat: the tape's own player unless `slot` names another,
/// who is there only while this client could see them.
#[derive(Deserialize, Clone, Copy, Debug)]
#[serde(deny_unknown_fields)]
pub struct Follow {
    #[serde(default)]
    pub slot: Option<u8>,
    /// Metres behind, above and to the right of the body.
    #[serde(default = "Follow::default_back")]
    pub back: f32,
    #[serde(default = "Follow::default_up")]
    pub up: f32,
    #[serde(default)]
    pub side: f32,
    /// Seconds the camera takes to catch up; bigger is lazier.
    #[serde(default = "Follow::default_lag")]
    pub lag: f32,
}

impl Follow {
    fn default_back() -> f32 {
        3.6
    }
    fn default_up() -> f32 {
        1.6
    }
    fn default_lag() -> f32 {
        0.35
    }
}

/// Aim at a seat (the player's unless `slot` names another): a tripod
/// that pans. The camera stays where the keys put it, so it is never inside
/// a wall.
#[derive(Deserialize, Clone, Copy, Debug)]
#[serde(deny_unknown_fields)]
pub struct Track {
    #[serde(default)]
    pub slot: Option<u8>,
    /// Metres above the feet to aim at.
    #[serde(default = "Track::default_up")]
    pub up: f32,
    /// Seconds the aim takes to catch up; bigger is a lazier operator.
    #[serde(default = "Track::default_lag")]
    pub lag: f32,
}

impl Track {
    fn default_up() -> f32 {
        1.1
    }
    fn default_lag() -> f32 {
        0.6
    }
}

/// Catmull-Rom through the keys at shot time `t`, with the ends extrapolated
/// straight so a two-key shot is a dolly at constant speed. One key is a
/// tripod.
pub fn spline(path: &[(f32, Vec3, Vec3)], t: f32) -> (Vec3, Vec3) {
    let n = path.len();
    if n == 0 {
        return (Vec3::ZERO, Vec3::Z);
    }
    if n == 1 {
        return (path[0].1, path[0].2);
    }
    let i = path.windows(2).position(|w| t < w[1].0).unwrap_or(n.saturating_sub(2));
    let (t1, t2) = (path[i].0, path[i + 1].0);
    let u = ((t - t1) / (t2 - t1).max(1e-6)).clamp(0.0, 1.0);
    let pick = |f: fn(&(f32, Vec3, Vec3)) -> Vec3| {
        let p1 = f(&path[i]);
        let p2 = f(&path[i + 1]);
        let p0 = if i > 0 { f(&path[i - 1]) } else { 2.0 * p1 - p2 };
        let p3 = if i + 2 < n { f(&path[i + 2]) } else { 2.0 * p2 - p1 };
        let (u2, u3) = (u * u, u * u * u);
        0.5 * (2.0 * p1
            + (p2 - p0) * u
            + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * u2
            + (3.0 * p1 - p0 - 3.0 * p2 + p3) * u3)
    };
    (pick(|k| k.1), pick(|k| k.2))
}

/// Smooth noise in about ±1 from three sines whose ratios never line up:
/// a hand on the camera.
pub fn wobble(t: f32, seed: f32) -> f32 {
    use std::f32::consts::TAU;
    0.55 * (t * 0.37 * TAU + seed).sin()
        + 0.30 * (t * 0.91 * TAU + seed * 1.7).sin()
        + 0.15 * (t * 2.13 * TAU + seed * 2.3).sin()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(t: f32, x: f32) -> (f32, Vec3, Vec3) {
        (t, Vec3::new(x, 10.0, 0.0), Vec3::new(x, 0.0, 50.0))
    }

    #[test]
    fn a_two_key_path_is_a_dolly_at_constant_speed() {
        let path = [key(0.0, 0.0), key(4.0, 40.0)];
        for (t, x) in [(0.0, 0.0), (1.0, 10.0), (2.0, 20.0), (4.0, 40.0)] {
            let (p, _) = spline(&path, t);
            assert!((p.x - x).abs() < 1e-3, "t {t}: {} not {x}", p.x);
        }
    }

    #[test]
    fn a_path_passes_through_every_key() {
        let path = [key(0.0, 0.0), key(2.0, 5.0), key(3.0, 30.0), key(6.0, 31.0)];
        for k in &path {
            let (p, l) = spline(&path, k.0);
            assert!((p - k.1).length() < 1e-3 && (l - k.2).length() < 1e-3);
        }
    }

    #[test]
    fn a_list_names_exactly_one_path_per_shot() {
        let ok = r#"{"tape":"a.tape","out":"o","shots":[
            {"name":"a","at":1,"len":2,"orbit":{"center":[0,0,0],"radius":5,"height":2,"from":0,"to":90}}]}"#;
        assert!(Script::parse(ok).is_ok());
        let shot: Shot = serde_json::from_str(r#"{"name":"b","at":1,"len":2}"#).unwrap();
        assert!(shot.check().is_err());
        let still = r#"{"name":"c","at":1,"still":true,"keys":[{"t":0,"pos":[0,2,0],"look":[9,1,9]}]}"#;
        let shot: Shot = serde_json::from_str(still).unwrap();
        assert!(shot.check().is_ok());
        let both = r#"{"name":"d","at":1,"len":2,"pov":true,"track":{}}"#;
        let shot: Shot = serde_json::from_str(both).unwrap();
        assert!(shot.check().is_err());
        assert!(Script::parse(r#"{"fps":7,"shots":[{"name":"e","at":1,"len":1,"pov":true}]}"#).is_err());
        assert!(Script::parse(r#"{"quality":"ultra","shots":[{"name":"e","at":1,"len":1,"pov":true}]}"#).is_err());
    }

    #[test]
    fn a_ramp_holds_eases_and_holds() {
        let s: Span = serde_json::from_str("[[0, 1], [2, 1], [3, 0.2]]").unwrap();
        assert_eq!(s.at(0.0, 0.0), 1.0);
        assert_eq!(s.at(0.0, 1.5), 1.0);
        let mid = s.at(0.0, 2.5);
        assert!((mid - 0.6).abs() < 1e-4, "{mid}");
        assert_eq!(s.at(0.0, 9.0), 0.2);
        let pair: Span = serde_json::from_str("[10, 20]").unwrap();
        assert_eq!(pair.at(0.5, 99.0), 15.0);
    }

    #[test]
    fn a_ramp_spends_the_tape_it_says() {
        let shot: Shot = serde_json::from_str(
            r#"{"name":"r","at":0,"len":4,"speed":[[0,1],[2,1],[2.001,0.25]],
                "orbit":{"center":[0,0,0],"radius":5,"height":2,"from":0,"to":9}}"#,
        )
        .unwrap();
        let s = shot.tape_s(30);
        assert!((s - 2.5).abs() < 0.05, "{s}");
    }

    #[test]
    fn a_shot_look_wins_field_by_field() {
        let base: Look = serde_json::from_str(r#"{"fstop": 4, "saturation": 1.1}"#).unwrap();
        let shot: Look = serde_json::from_str(r#"{"fstop": 1.8, "focus": "subject"}"#).unwrap();
        let l = shot.over(&base);
        assert_eq!(l.fstop, Some(1.8));
        assert_eq!(l.saturation, Some(1.1));
        assert!(matches!(l.focus, Some(Focus::On(FocusOn::Subject))));
        let m: Look = serde_json::from_str(r#"{"focus": [3, 12]}"#).unwrap();
        assert!(matches!(m.focus, Some(Focus::Metres(Span::Between(_)))));
    }

    #[test]
    fn the_house_list_parses() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ci/film/trailer.json");
        let s = Script::load(&path).unwrap();
        assert!(s.shots.len() >= 4);
        assert!(s.shots.windows(2).all(|w| w[0].at <= w[1].at));
    }

    #[test]
    fn a_short_screen_draws_the_same_shape_smaller() {
        let mut s =
            Script::parse(r#"{"width":1080,"height":1920,"shots":[{"name":"p","at":-5,"len":4,"pov":true}]}"#).unwrap();
        assert_eq!(s.window(), (1080, 1920));
        s.fit_height(900);
        let (w, h) = s.window();
        assert_eq!(h, 900);
        assert!(w % 2 == 0 && (w as f64 / h as f64 - 1080.0 / 1920.0).abs() < 0.01);
        s.anchor(3.0);
        assert!(
            s.shots[0].at >= BOOT_TAPE_MS / 1000.0,
            "a short tape starts where the boot allows"
        );
    }
}

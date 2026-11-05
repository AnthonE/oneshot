//! The cloud deck's shadows on the ground, and the sun under the deck.
//! After Gates' `render/cloud_shadow.rs` (MIT, NOTICE).
//!
//! **The sun is dimmed by the deck over the map's middle, not the eye.**
//! Gates dims its one sun by the deck over the sun's disk as each eye sees
//! it, so two players at one campfire stand in two lights. Here the deck
//! over the sun is read from the middle of the map ([`SkyField::cover_at`]
//! at `MoodNow::middle`), at the drift the server's clock puts it at
//! (`mood::SkyClock`), so every player sees a fighter in the same light; a
//! cloud's shadow is a kilometre across and an arena is fifty metres, so on
//! a map it is the whole map going grey and back, as it does under real
//! cumulus. Only a mood with a `shade` dims the sun (Dry Fork's), by at
//! most half, and it never puts the shadows out (`quality.rs`: the sun
//! always casts). The disk the atmosphere draws over the sky is put out
//! behind cloud on every map, since the atmosphere draws it over the deck.
//!
//! **And the ground takes the difference** (`cloud_shadow.wgsl`): the sun's
//! direct term once more, scaled by how much more or less of the sun reaches
//! a point than the middle, so a long map shows the deck's shadows sweeping
//! across its slopes while everything on it stands in the light the middle
//! has. Only the ground: a fighter, a box, a rock are lit as the middle is,
//! so the shadows never hide anyone. Every rung draws them: a uniform read
//! and, wherever a point's sun differs from the middle's (on Dry Fork, much
//! of the canyon at a time), the sun's term once more with one
//! hardware-compared shadow tap (not Bevy's filtered fetch: the term is a
//! soft share of the sun and the main pass already filtered the edge).
//!
//! **One path on three targets.** The field the ground reads is a 16 × 16
//! window of [`sky::SoftField`] in the ground's uniform, moved with the
//! drift: not a texture (the ground under the atmosphere already binds the
//! sixteen sampled textures a WebGPU floor allows, `gpu::FRAGMENT_TEXTURES`)
//! and not a storage buffer (WebGL2 has none). The window is written when
//! the drift leaves it, about once a minute, and the lanes when the sky's
//! clock is re-anchored; nothing here writes the ground a frame.

use bevy::light::SunDisk;
use bevy::prelude::*;
use bevy::render::render_resource::ShaderType;

use super::deck::SkyField;
use super::ground::GroundMaterial;
use super::mood::{Anchor, Drift, MoodNow};
use super::rig::{Sun, SunBase};
use super::sky::{self, SoftField};

/// The window's texels a side.
pub const WINDOW: usize = 16;

/// The steps the sun's cover is written in: a cloud's edge crossing the sun
/// moves the light 1/64 of the shade at a time, never a frame.
pub const COVER_STEPS: f32 = 64.0;

/// How far past the map's edges the ground reaches, metres (the terrain's
/// margin and the slopes' rise along the ray to the sun).
pub const GROUND_MARGIN_M: f32 = 24.0;

/// Gates' wet ground: what a soak keeps of a colour's value, how far it
/// stretches its chroma, and the darkest it takes one to (`rain::WET_*`).
const WET_LANES: Vec3 = Vec3::new(
    super::rain::WET_VALUE,
    super::rain::WET_SATURATION,
    super::rain::WET_FLOOR,
);

/// The uniform, laid out to match `CloudShadow` in `cloud_shadow.wgsl`.
#[derive(Clone, Copy, Default, ShaderType, Debug, PartialEq)]
pub struct CloudShadowParams {
    /// x 1 where the shadows are drawn, 0 off; y unused; z the field
    /// threshold (`1 − cover`); w the coverage edge (`sky::EDGE`).
    pub cover: Vec4,
    /// xy the field's offset at `sun.w`, noise units; zw how fast it moves.
    pub drift: Vec4,
    /// xyz toward the sun (how the shader tells the sun from another
    /// light); w the `globals.time` the drift was taken at.
    pub sun: Vec4,
    /// x the deck's altitude, m; y noise units a metre; z one over the
    /// field's period; w the field's texels a side ([`SoftField::n`]).
    pub deck: Vec4,
    /// x the horizon cutoff; y the fade above it; z the mood's shade; w how
    /// far a texel's centre sits from its noise coordinate, texels.
    pub fade: Vec4,
    /// xyz the map's middle, the point the sun's light is set by.
    pub at: Vec4,
    /// xy the window's first column and row, field texels.
    pub window: Vec4,
    /// x the ground's wet; yzw [`WET_LANES`].
    pub wet: Vec4,
    /// The window, a row to an entry, four texels to a word.
    pub texels: [UVec4; WINDOW],
}

/// Where the window sits on the field, texels: its first column and row.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Window(pub IVec2);

impl Window {
    /// Whether texel coordinates `u ± e` (bilinear's next texel included)
    /// lie inside this window on a field `n` texels a side.
    pub fn holds(self, u: Vec2, e: Vec2, n: i32) -> bool {
        let half = n as f32 * 0.5;
        let rel = |c: f32, o: i32| (c - o as f32 + half).rem_euclid(n as f32) - half;
        let (x, y) = (rel(u.x, self.0.x), rel(u.y, self.0.y));
        let last = (WINDOW - 1) as f32;
        x - e.x >= 0.0 && x + e.x + 1.0 <= last && y - e.y >= 0.0 && y + e.y + 1.0 <= last
    }

    /// The window round `u ± e` with the room left over ahead of where the
    /// drift moves it (`vel`, texels a second), so it holds longest.
    pub fn around(u: Vec2, e: Vec2, vel: Vec2, n: i32) -> Window {
        let last = (WINDOW - 1) as f32;
        let place = |c: f32, e: f32, v: f32| -> i32 {
            let start = if v > 0.0 {
                // Its first column at the run's back edge.
                (c - e).floor()
            } else if v < 0.0 {
                // Its last at the run's front edge, bilinear's next included.
                (c + e + 1.0 - last).ceil()
            } else {
                (c - e - 0.5 * (last - 2.0 * e - 1.0)).floor()
            };
            (start as i32).rem_euclid(n)
        };
        Window(IVec2::new(place(u.x, e.x, vel.x), place(u.y, e.y, vel.y)))
    }

    /// The window's bytes off `soft`, a row of [`WINDOW`] to an entry.
    pub fn texels(self, soft: &SoftField) -> [UVec4; WINDOW] {
        let n = soft.n();
        let bytes = soft.bytes();
        let mut out = [UVec4::ZERO; WINDOW];
        for (r, row) in out.iter_mut().enumerate() {
            let j = (self.0.y as usize + r) % n;
            for c in 0..WINDOW {
                let i = (self.0.x as usize + c) % n;
                let b = u32::from(bytes[j * n + i]);
                row[c / 4] |= b << ((c % 4) * 8);
            }
        }
        out
    }
}

/// The field coordinate, texels of `soft`, that the deck over the sun from
/// the map's middle is read at, with the field at offset `drift`.
pub fn texel_at(soft: &SoftField, sun: Vec3, drift: Vec2) -> Vec2 {
    let q = Vec2::new(sun.x, sun.z) * (sky::CLOUD_ALT_M / sun.y.max(sky::DECK_CUTOFF)) / sky::CLOUD_SCALE_M + drift;
    q * (soft.n() as f32 / sky::FIELD_PERIOD as f32) - Vec2::splat(soft.offset())
}

impl CloudShadowParams {
    /// No shadows: the deck's geometry, the ground's `wet`, nothing drawn.
    pub fn off(wet: f32) -> Self {
        Self {
            cover: Vec4::new(0.0, 0.0, 1.0, sky::EDGE),
            deck: Vec4::new(
                sky::CLOUD_ALT_M,
                1.0 / sky::CLOUD_SCALE_M,
                1.0 / sky::FIELD_PERIOD as f32,
                1.0,
            ),
            fade: Vec4::new(sky::DECK_CUTOFF, sky::HORIZON_FADE, 0.0, 0.0),
            wet: Vec4::new(wet, WET_LANES.x, WET_LANES.y, WET_LANES.z),
            ..default()
        }
    }

    /// The shadows of `now`'s deck from `at`, read off `soft` through
    /// `window`.
    pub fn new(now: &MoodNow, at: &Anchor, soft: &SoftField, window: Window) -> Self {
        let m = &now.mood;
        let mut p = Self::off(m.wet);
        if m.shade <= 0.0 || m.cover <= 0.0 {
            return p;
        }
        p.cover = Vec4::new(1.0, 0.0, 1.0 - m.cover, sky::EDGE);
        p.drift = Vec4::new(at.off.x, at.off.y, at.vel.x, at.vel.y);
        p.sun = m.to_sun().extend(at.t);
        p.deck.w = soft.n() as f32;
        p.fade.z = m.shade;
        p.fade.w = soft.offset();
        p.at = now.middle.extend(0.0);
        p.window = Vec4::new(window.0.x as f32, window.0.y as f32, 0.0, 0.0);
        p.texels = window.texels(soft);
        p
    }
}

/// What [`drive`] keeps between frames: where the window sits.
#[derive(Default)]
pub struct Track {
    window: Option<Window>,
}

/// The extent the ground reads around the window's centre, texels of
/// `soft`: the map's half-size and its margin.
fn extent(now: &MoodNow, soft: &SoftField) -> Vec2 {
    let def = sim::maps::ALL.iter().find(|d| d.id == now.map);
    let half = def.map_or(Vec2::splat(50.0), |d| {
        Vec2::new(d.hi.0 - d.lo.0, d.hi.1 - d.lo.1) * 0.5 + Vec2::splat(GROUND_MARGIN_M)
    });
    half * (soft.n() as f32 / (sky::FIELD_PERIOD as f32 * sky::CLOUD_SCALE_M))
}

/// The sun under the deck over the map's middle, and the ground's lanes,
/// written when they move. After the drift is taken (`mood::drift`).
#[allow(clippy::too_many_arguments)]
pub fn drive(
    now: Res<MoodNow>,
    drift: Res<Drift>,
    time: Res<Time>,
    field: Option<Res<SkyField>>,
    mut track: Local<Track>,
    mut sun: Query<(&SunBase, &mut DirectionalLight, &mut SunDisk), With<Sun>>,
    mut ground: ResMut<Assets<GroundMaterial>>,
) {
    let Some(field) = field else {
        return;
    };
    let m = &now.mood;
    let at = drift.anchor;
    let t = time.elapsed_secs_wrapped();
    let toward = m.to_sun();
    // The deck over the sun from the middle, as the shader reads it there.
    let cover = (field.cover_at(toward, m.cover, at.at(t)) * COVER_STEPS).round() / COVER_STEPS;
    for (base, mut light, mut disk) in sun.iter_mut() {
        let lux = base.lux * (1.0 - m.shade * cover);
        if light.illuminance != lux {
            light.illuminance = lux;
        }
        let d = base.disk * (1.0 - cover);
        if disk.intensity != d {
            disk.intensity = d;
        }
    }

    let on = m.shade > 0.0 && m.cover > 0.0 && !now.roofed;
    let want = if on {
        let soft = &field.soft;
        let n = soft.n() as i32;
        let u = texel_at(soft, toward, at.at(t));
        let e = extent(&now, soft);
        let window = match track.window {
            Some(w) if w.holds(u, e, n) && !now.is_changed() => w,
            _ => Window::around(u, e, at.vel * (n as f32 / sky::FIELD_PERIOD as f32), n),
        };
        track.window = Some(window);
        CloudShadowParams::new(&now, &at, soft, window)
    } else {
        track.window = None;
        CloudShadowParams::off(if now.roofed { 0.0 } else { m.wet })
    };
    let stale: Vec<AssetId<GroundMaterial>> = ground
        .iter()
        .filter(|(_, g)| g.extension.params.cloud != want)
        .map(|(id, _)| id)
        .collect();
    for id in stale {
        if let Some(g) = ground.get_mut(id) {
            g.extension.params.cloud = want;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field() -> SoftField {
        sky::CloudField::new(5, 64).soft(0)
    }

    /// The window holds what it was placed round, gives up once the drift
    /// has carried that out of it, and its bytes are the field's.
    #[test]
    fn the_window_holds_the_map_until_the_drift_leaves_it() {
        let soft = field();
        let n = soft.n() as i32;
        let e = Vec2::new(3.0, 2.0);
        let u = Vec2::new(62.5, 1.2);
        let w = Window::around(u, e, Vec2::new(0.1, -0.1), n);
        assert!(w.holds(u, e, n), "{w:?}");
        // Ahead along the drift there is room; far past it there is none.
        assert!(w.holds(u + Vec2::new(4.0, -4.0), e, n));
        assert!(!w.holds(u + Vec2::new(12.0, 0.0), e, n));
        // Across the field's seam it is the same window.
        assert!(w.holds(u + Vec2::new(n as f32, 0.0), e, n));
        let t = w.texels(&soft);
        let (i, j) = (((w.0.x + 3) % n) as usize, ((w.0.y + 5) % n) as usize);
        let byte = (t[5][0] >> 24) & 0xff;
        assert_eq!(byte, u32::from(soft.bytes()[j * soft.n() + i]));
    }

    /// The point the window is placed round is the one the CPU reads the
    /// sun's cover at: `SoftField::at` of the same coordinate.
    #[test]
    fn the_window_is_read_where_the_sun_is_dimmed() {
        let soft = field();
        let sun = Vec3::new(0.3, 0.8, 0.5).normalize();
        let drift = Vec2::new(3.3, 17.9);
        let u = texel_at(&soft, sun, drift);
        let q = Vec2::new(sun.x, sun.z) * (sky::CLOUD_ALT_M / sun.y) / sky::CLOUD_SCALE_M + drift;
        let back = (u + Vec2::splat(soft.offset())) * (sky::FIELD_PERIOD as f32 / soft.n() as f32);
        assert!((back - q).length() < 1e-3);
    }
}

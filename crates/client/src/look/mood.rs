//! Each map's hour and weather: its mood, fixed per map, so every player on
//! it sees one sky. After Gates' `render/weather.rs` and `render/rig.rs`'s
//! day and night (MIT, NOTICE), with the clock taken out: Gates' sky is a
//! function of the shard's tick, a match's is a function of its map.
//!
//! A mood is the sun (where, what colour, how strong), the exposure and the
//! fill it is seen at, the cloud deck (how much, how heavy, which way it
//! drifts), the air (mist, rain, dust), the moon and the stars. The rig
//! lights the frame from it (`rig.rs`), the deck draws its sky (`deck.rs`),
//! and the rest draw their part of it: `stars.rs`, `moon.rs`, `rain.rs`,
//! `rainbow.rs`, `cloud_shadow.rs`, `shafts.rs`.
//!
//! **Readable first.** Every mood keeps a fighter as easy to see as the
//! noon it replaces: the dusk is golden hour with the exposure opened and
//! the fill kept, the mist is a fifth of the light across an arena, the
//! rain a light shower whose streaks are a few percent of the frame, and
//! the cloud shadows dim the sun by at most half. The air is the same on
//! every tier (`rig::fog_for`); what a tier changes is how it is drawn.
//!
//! **One clock for everyone.** What moves (the deck's drift, its shadows
//! on the ground) runs on the server's tick as this client estimates it
//! ([`SkyClock`]), so two players see the same cloud over the sun to within
//! the clock's slew; and the sun's light over a fighter is set by the deck
//! over the map's middle, not over the eye (`cloud_shadow.rs`).

use bevy::prelude::*;

use super::sky::{self, ComposeParams, SkyFog};

/// The sun's elevation and bearing at noon (`rig::SUN_ELEVATION`), and
/// the noon's exposure: the reference every other mood is read against.
const NOON_ELEV: f32 = super::rig::SUN_ELEVATION;
const NOON_AZIM: f32 = super::rig::SUN_AZIMUTH;
const NOON_EV: f32 = super::rig::DAY_EV100;

/// One map's sky.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mood {
    /// What it is called in the log and on the page (`__oneshot.mood`).
    pub name: &'static str,
    /// Toward the sun: its elevation and bearing, radians (`rig::to_sun_at`).
    pub sun_elev: f32,
    pub sun_azim: f32,
    /// The sun's colour over white, linear, before the air reddens it.
    pub sun_rgb: [f32; 3],
    /// The sun's illuminance, a share of full sunlight (`LightDef::sun` on top).
    pub sun: f32,
    /// What the sky's light is, a share of noon's: the deck's tops and
    /// bases, and under WebGL2 the clear sky behind them
    /// (`ComposeParams::light`).
    pub light: f32,
    /// The hemisphere fill, a share of noon's (`LightDef::fill` on top).
    pub fill: f32,
    /// The exposure, ev100. A map whose `LightDef` names one (an interior,
    /// lit by its lamps) keeps its own.
    pub ev100: f32,
    /// How much of the sky carries cloud, `0..=1`, and how heavy it is.
    pub cover: f32,
    pub dark: f32,
    /// The ground mist's extinction, per metre: what the shafts light.
    pub mist: f32,
    /// How hard it rains, `0..=1`: the streaks, their haze, the rainbow.
    pub rain: f32,
    /// The dust haze's extinction, per metre: a tint on the far ground.
    pub dust: f32,
    /// How wet the ground and the boxes are, `0..=1`.
    pub wet: f32,
    /// Toward where the wind blows, radians (the sun's convention), and
    /// how fast the deck drifts on it, m/s.
    pub wind_azim: f32,
    pub wind_ms: f32,
    /// How much of the starfield shows, `0..=1`.
    pub stars: f32,
    /// How much of the moon shows, `0..=1`, where it is (elevation and
    /// bearing, radians) and how much of its disk is lit.
    pub moon: f32,
    pub moon_elev: f32,
    pub moon_azim: f32,
    pub moon_lit: f32,
    /// How much of the direct sun a full deck over it takes: the cloud
    /// shadows' depth on the ground, and the sun's dimming under them.
    /// Zero holds the sun still, whatever drifts over it.
    pub shade: f32,
    /// How dark the deck's underside goes where it is thickest, `0..=1`:
    /// a heavy overcast seen from under it is mottled, dark where the cloud
    /// over it is thick and pale where it thins, where a cumulus's flat
    /// base wants none.
    pub deep: f32,
    /// How much of the sky a grey veil of the mood's fog lies over, `0..=1`,
    /// all the way up and whole at the horizon: under a heavy overcast the air's
    /// own blue doesn't reach the eye, however bright the sun in its breaks.
    pub veil: f32,
    /// Where in the cloud field this map's sky starts, noise units: each
    /// map its own clouds from the one field.
    pub field_at: [f32; 2],
    /// Under a roof, whose light fills it (`fill::hall::PALETTES`).
    pub hall: usize,
    /// The frame's grade: one for everyone on the map.
    pub grade: Grade,
    /// Whether what falls (`rain`, how much of it) is snow.
    pub snow: bool,
}

/// A mood's film grade (`rig::grade_for`), `Grade::NONE` the frame as lit. It only
/// colours what's already seen; a mood stays readable by its light.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grade {
    /// White balance: cooler under 0, warmer over; green to magenta.
    pub temperature: f32,
    pub tint: f32,
    /// Over everything, after the tone curve.
    pub saturation: f32,
    /// Over the shadows and the midtones, about their middle.
    pub contrast: f32,
    /// Added to the shadows and the midtones.
    pub lift: f32,
}

impl Grade {
    pub const NONE: Grade = Grade {
        temperature: 0.0,
        tint: 0.0,
        saturation: 1.0,
        contrast: 1.0,
        lift: 0.0,
    };
}

/// Proving Grounds and The Range: the reference. Noon on a clear day with
/// fair-weather cumulus, the frame every look before this one was judged at.
pub const NOON: Mood = Mood {
    name: "noon",
    sun_elev: NOON_ELEV,
    sun_azim: NOON_AZIM,
    sun_rgb: [1.0, 1.0, 1.0],
    sun: 1.0,
    light: 1.0,
    fill: 1.0,
    ev100: NOON_EV,
    cover: sky::CLOUD_COVER,
    dark: 0.0,
    mist: 0.0,
    rain: 0.0,
    dust: 0.0,
    wet: 0.0,
    wind_azim: 1.2,
    wind_ms: 3.0,
    stars: 0.0,
    moon: 0.0,
    moon_elev: -0.5,
    moon_azim: 0.0,
    moon_lit: 1.0,
    shade: 0.0,
    deep: 0.0,
    veil: 0.0,
    field_at: [0.0, 0.0],
    hall: 0,
    grade: Grade::NONE,
    snow: false,
};

/// Overpass: a sun shower late in the day. Broken cloud with the sun under
/// its edge off one long side (22°), across the line between the bases so
/// neither team looks into it ([`tests`]), low enough that the bow over the
/// far side clears the walls, a light rain and wet concrete.
pub const SUN_SHOWER: Mood = Mood {
    name: "sun shower",
    sun_elev: 0.38,
    sun_azim: core::f32::consts::PI,
    sun_rgb: [1.0, 0.96, 0.9],
    sun: 0.8,
    light: 0.8,
    fill: 1.0,
    ev100: 13.9,
    cover: 0.68,
    dark: 0.3,
    mist: 0.0,
    rain: 0.3,
    dust: 0.0,
    wet: 0.8,
    wind_azim: 1.0,
    wind_ms: 6.0,
    stars: 0.0,
    moon: 0.0,
    moon_elev: -0.5,
    moon_azim: 0.0,
    moon_lit: 1.0,
    shade: 0.0,
    deep: 0.0,
    veil: 0.0,
    field_at: [7.3, 3.1],
    hall: 0,
    grade: Grade::NONE,
    snow: false,
};

/// Cistern: a misty morning. A low sun (21°) over the wall between the two
/// sides' spawns (red holds -x, blue +x), through a ground mist, so the
/// walls and the pillars cut its light into shafts; a thin mist, a fifth
/// of the light across the arena.
pub const MIST: Mood = Mood {
    name: "misty morning",
    sun_elev: 0.37,
    sun_azim: 0.0,
    sun_rgb: [1.0, 0.93, 0.82],
    sun: 0.85,
    light: 0.85,
    fill: 1.0,
    ev100: 13.9,
    cover: 0.32,
    dark: 0.0,
    mist: 0.005,
    rain: 0.0,
    dust: 0.0,
    wet: 0.0,
    wind_azim: 2.6,
    wind_ms: 1.5,
    stars: 0.0,
    moon: 0.0,
    moon_elev: -0.5,
    moon_azim: 0.0,
    moon_lit: 1.0,
    shade: 0.0,
    deep: 0.0,
    veil: 0.0,
    field_at: [19.0, 11.5],
    hall: 0,
    grade: Grade::NONE,
    snow: false,
};

/// Dry Fork: a desert afternoon. The sun well down the sky (39°) and
/// across the canyon, so one wall glows, the other stands in
/// shade and throws it over a strip of the floor, and neither base looks
/// into it; cumulus racing over on a stiff wind so their shadows sweep it
/// end to end; a little dust in the air. The sun's colour stays near white
/// and the frame ungraded: the atmosphere already warms a 39° sun on the
/// desktop, and a warm sun with a richer, harder grade over it read as a
/// cartoon there, its shade side near black.
pub const DESERT: Mood = Mood {
    name: "desert afternoon",
    sun_elev: 0.68,
    sun_azim: 2.75,
    sun_rgb: [1.0, 0.97, 0.9],
    sun: 1.0,
    light: 1.0,
    fill: 0.95,
    ev100: 14.4,
    cover: 0.5,
    dark: 0.0,
    mist: 0.0,
    rain: 0.0,
    dust: 0.0008,
    wet: 0.0,
    wind_azim: 0.4,
    wind_ms: 7.0,
    stars: 0.0,
    moon: 0.0,
    moon_elev: -0.5,
    moon_azim: 0.0,
    moon_lit: 1.0,
    shade: 0.5,
    deep: 0.0,
    veil: 0.0,
    field_at: [3.0, 22.0],
    hall: 0,
    grade: Grade::NONE,
    snow: false,
};

/// Bulkhead: dusk. The sun low (11°) over one long side, across the line
/// between the bases (down the long axis one team would look into it all
/// match), the exposure opened a stop; a gibbous moon up over the other
/// side and the first stars.
pub const DUSK: Mood = Mood {
    name: "dusk",
    sun_elev: 0.2,
    sun_azim: core::f32::consts::PI,
    sun_rgb: [1.0, 0.92, 0.82],
    sun: 0.6,
    light: 0.55,
    fill: 0.8,
    ev100: 13.3,
    cover: 0.38,
    dark: 0.1,
    mist: 0.0,
    rain: 0.0,
    dust: 0.0,
    wet: 0.0,
    wind_azim: 5.5,
    wind_ms: 2.0,
    stars: 0.35,
    moon: 0.85,
    moon_elev: 0.3,
    moon_azim: 0.0,
    moon_lit: 0.9,
    shade: 0.0,
    deep: 0.0,
    veil: 0.0,
    field_at: [25.0, 5.0],
    hall: 0,
    grade: Grade::NONE,
    snow: false,
};

/// Overpass: a rainy dusk, the sun low and red through a break in a heavy deck,
/// across the line between the bases. Its exposure stays open: a fighter is as
/// easy to see as at the dusk it's the wet cousin of; the dark is in the sky, and
/// the lamps and the signs are what's warm in it.
pub const RAIN_DUSK: Mood = Mood {
    name: "rain dusk",
    sun_elev: 0.2,
    sun_azim: 0.0,
    sun_rgb: [1.0, 0.7, 0.48],
    sun: 0.5,
    light: 0.4,
    fill: 0.8,
    ev100: 13.3,
    cover: 0.93,
    dark: 0.93,
    mist: 0.0015,
    rain: 0.4,
    dust: 0.0,
    wet: 1.0,
    wind_azim: 0.6,
    wind_ms: 5.0,
    stars: 0.0,
    moon: 0.0,
    moon_elev: -0.5,
    moon_azim: 0.0,
    moon_lit: 1.0,
    shade: 0.3,
    deep: 0.85,
    veil: 0.46,
    field_at: [41.0, 13.0],
    hall: 0,
    grade: Grade {
        temperature: -0.045,
        tint: 0.003,
        saturation: 1.08,
        contrast: 1.06,
        lift: 0.0,
    },
    snow: false,
};

/// Proving Grounds: blue hour over a test range under snow, the sun down low and
/// rose across the line between the bases, the cloud broken, snow coming down.
pub const SNOW_DUSK: Mood = Mood {
    name: "snow dusk",
    sun_elev: 0.2,
    sun_azim: core::f32::consts::PI,
    sun_rgb: [1.0, 0.82, 0.72],
    sun: 0.62,
    light: 0.32,
    fill: 0.8,
    ev100: 13.5,
    cover: 0.84,
    dark: 0.66,
    mist: 0.0016,
    rain: 0.3,
    dust: 0.0,
    wet: 0.0,
    wind_azim: 2.2,
    wind_ms: 2.5,
    stars: 0.0,
    moon: 0.0,
    moon_elev: -0.5,
    moon_azim: 0.4,
    moon_lit: 0.75,
    shade: 0.2,
    deep: 0.55,
    veil: 0.34,
    field_at: [9.0, 21.0],
    hall: 0,
    grade: Grade {
        temperature: -0.06,
        tint: 0.004,
        saturation: 1.08,
        contrast: 1.07,
        lift: 0.0,
    },
    snow: true,
};

/// Cloudbreak: an island over a sea of cloud under a heavy grey deck. The
/// sun (35 degrees up, in the south-south-east, across the line between the
/// bases) comes through a break whole and warm, so what it lights and what it
/// shades stand far apart, as under a broken deck. The deck is mottled dark
/// where it is thick (`deep`), and a veil of the fog over the whole sky
/// (`veil`, with masses of its own) keeps the air's blue from the eye however
/// bright that sun, and thins toward it. A mist grades the stacks paler with
/// their distance and hazes the horizon the sea of cloud goes into.
pub const OVERCAST: Mood = Mood {
    name: "overcast",
    sun_elev: 0.62,
    sun_azim: 2.7,
    sun_rgb: [1.0, 0.93, 0.8],
    sun: 1.9,
    light: 0.9,
    fill: 0.7,
    ev100: 13.9,
    cover: 0.92,
    dark: 0.62,
    mist: 0.0034,
    rain: 0.0,
    dust: 0.0,
    wet: 0.0,
    wind_azim: 0.9,
    wind_ms: 5.0,
    stars: 0.0,
    moon: 0.0,
    moon_elev: -0.5,
    moon_azim: 0.0,
    moon_lit: 1.0,
    shade: 0.15,
    deep: 0.8,
    veil: 0.72,
    field_at: [33.0, 7.0],
    hall: 0,
    grade: Grade::NONE,
    snow: false,
};

/// Pithead: a mine high in the mountains in a blizzard's lull, late in the
/// day. A heavy deck the sun comes through low, pale and white (no warmth in
/// it, across the line between the bases), the sky a pale veil a step under
/// the snow's brightness, as in a whiteout, the
/// air thick enough that the peaks round it go to white, a hard wind along
/// the rock driving the snow. The only warm light is the lamps'; the cold is
/// in the grade, never in a darker frame.
pub const BLIZZARD: Mood = Mood {
    name: "blizzard",
    sun_elev: 0.3,
    // Square across the line between the sides' spawns, which runs 9° off x.
    sun_azim: 0.15,
    sun_rgb: [0.9, 0.95, 1.0],
    sun: 0.75,
    light: 0.85,
    fill: 0.95,
    ev100: 13.4,
    cover: 0.95,
    dark: 0.25,
    mist: 0.005,
    rain: 0.4,
    dust: 0.0,
    wet: 0.0,
    wind_azim: 1.25,
    wind_ms: 9.0,
    stars: 0.0,
    moon: 0.0,
    moon_elev: -0.5,
    moon_azim: 0.0,
    moon_lit: 1.0,
    shade: 0.15,
    deep: 0.1,
    veil: 0.95,
    field_at: [17.0, 29.0],
    hall: 0,
    grade: Grade {
        temperature: -0.04,
        tint: 0.012,
        saturation: 0.9,
        contrast: 1.08,
        lift: 0.0,
    },
    snow: true,
};

/// Chrysalis: inside the hull, under its roof and its lamps. The map's own
/// light (`LightDef`: its exposure, no sun, the fill all but closed) is
/// what draws it; the sky above the roof is never seen.
pub const INTERIOR: Mood = Mood {
    name: "interior",
    stars: 0.0,
    moon: 0.0,
    field_at: [12.0, 28.0],
    ..NOON
};

/// The mood of map `id` (`sim::maps::ALL`'s ids).
pub fn of(id: u8) -> Mood {
    match id {
        0 => SNOW_DUSK,
        1 => RAIN_DUSK,
        2 => Mood {
            name: "cistern",
            field_at: [31.0, 4.0],
            hall: 1,
            grade: Grade {
                temperature: -0.012,
                tint: 0.0,
                saturation: 1.03,
                contrast: 1.0,
                lift: 0.0,
            },
            ..INTERIOR
        },
        3 => DESERT,
        4 => Mood {
            name: "noon (range)",
            field_at: [14.0, 9.0],
            ..NOON
        },
        5 => Mood {
            name: "dock",
            field_at: [25.0, 5.0],
            hall: 2,
            grade: Grade {
                temperature: -0.015,
                tint: 0.0,
                saturation: 1.05,
                contrast: 1.06,
                lift: 0.0,
            },
            ..INTERIOR
        },
        6 => INTERIOR,
        7 => OVERCAST,
        8 => BLIZZARD,
        _ => NOON,
    }
}

impl Mood {
    /// Toward the sun, unit.
    pub fn to_sun(&self) -> Vec3 {
        super::rig::to_sun_at(self.sun_elev, self.sun_azim)
    }

    /// Toward the moon, unit.
    pub fn to_moon(&self) -> Vec3 {
        super::rig::to_sun_at(self.moon_elev, self.moon_azim)
    }

    /// Toward where the wind blows, world XZ, unit.
    pub fn wind(&self) -> Vec2 {
        Vec2::new(self.wind_azim.sin(), self.wind_azim.cos())
    }

    /// The rain haze's extinction, per metre (Gates' `rain_part`).
    pub fn rain_sigma(&self) -> f32 {
        self.rain * self.rain * (3.0 / 600.0)
    }

    /// Everything that fogs the ground, per metre: the mist, the rain's
    /// haze and the dust.
    pub fn fog_sigma(&self) -> f32 {
        self.mist + self.rain_sigma() + self.dust
    }

    /// The weather's air over the sky.
    pub fn sky_fog(&self) -> SkyFog {
        SkyFog::new(self.mist, self.rain_sigma()).with_dust(self.dust)
    }

    /// The deck's field offset after `t` seconds of drift, noise units,
    /// wrapped on the field's period: minus how far it has drifted
    /// downwind (`ComposeParams::drift`).
    pub fn drift_at(&self, t: f64) -> Vec2 {
        let period = f64::from(sky::FIELD_PERIOD);
        let run = |at: f32, w: f32| {
            let v = f64::from(at) - f64::from(w) * f64::from(self.wind_ms) * t / f64::from(sky::CLOUD_SCALE_M);
            v.rem_euclid(period) as f32
        };
        let w = self.wind();
        Vec2::new(run(self.field_at[0], w.x), run(self.field_at[1], w.y))
    }

    /// How fast [`Self::drift_at`] moves, noise units a second.
    pub fn drift_vel(&self) -> Vec2 {
        -self.wind() * self.wind_ms / sky::CLOUD_SCALE_M
    }

    /// The deck for this mood with its field at `drift`, its clear sky
    /// behind it where nothing else draws one (`backdrop`).
    pub fn compose(&self, drift: Vec2, backdrop: bool) -> ComposeParams {
        ComposeParams {
            cover: self.cover,
            dark: self.dark,
            drift,
            sun: self.to_sun(),
            light: self.light,
            // The deck's night is its moonlit tint and its floor, which no
            // mood here reaches: the stars and the moon show by their own.
            night: 0.0,
            moon: self.to_moon(),
            fog: self.sky_fog(),
            fel: 0.0,
            backdrop,
            moon_lit: self.moon_lit,
            moonlight: self.moon,
            moon_sun: self.to_sun(),
            deep: self.deep,
            veil: self.veil,
        }
    }

    /// The exposure this mood opens over noon's, as a factor: what a colour
    /// set in deck units at noon is lifted by to sit in this frame.
    pub fn exposure_gain(&self) -> f32 {
        (NOON_EV - self.ev100).exp2()
    }
}

/// The map being drawn and its mood. Written once a map (`set`).
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct MoodNow {
    pub map: u8,
    pub mood: Mood,
    /// The map's middle and its floor's height, m: where the sun's light
    /// over everyone is read from under a drifting deck.
    pub middle: Vec3,
    /// Whether the map is under a roof (its `LightDef` gives it no sun).
    pub roofed: bool,
}

impl Default for MoodNow {
    fn default() -> Self {
        MoodNow::of(crate::front::BACKDROP)
    }
}

impl MoodNow {
    pub fn of(map: u8) -> MoodNow {
        let def = sim::maps::ALL.iter().find(|d| d.id == map);
        let middle = def.map_or(Vec3::ZERO, |d| {
            Vec3::new((d.lo.0 + d.hi.0) * 0.5, 0.0, (d.lo.1 + d.hi.1) * 0.5)
        });
        MoodNow {
            map,
            mood: of(map),
            middle,
            roofed: def.is_some_and(|d| d.light.sun <= 0.0),
        }
    }
}

/// The sky's clock, seconds: the server's tick as this client estimates it
/// in a match (`NetCore::render_tick`), so the deck drifts the same for
/// everyone; the app's own clock where there is no server (the title).
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct SkyClock {
    pub s: f64,
}

/// How far the shaders' running drift may wander from the clock's before
/// it is taken again, metres: a fraction of a cloud's soft edge.
pub const DRIFT_SLACK_M: f32 = 2.0;

/// Where the shaders' drift runs from: the field's offset `off` (noise
/// units) at `t` (seconds of `globals.time`), moving at `vel`. Taken again
/// only when the run has wandered from the clock's drift, the clock has
/// wrapped, or the map has changed: every sky material is written then
/// and not a frame (after Gates' `cloud_shadow::Anchor`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Anchor {
    pub off: Vec2,
    pub vel: Vec2,
    pub t: f32,
    set: bool,
}

impl Anchor {
    /// Follow this frame's offset `off` moving at `vel`, at `t`. True when
    /// it anchored again.
    pub fn track(&mut self, t: f32, off: Vec2, vel: Vec2) -> bool {
        let period = sky::FIELD_PERIOD as f32;
        let run = self.off + self.vel * (t - self.t);
        // The drift wraps a whole field over, which the field does too: a
        // period apart is no distance at all.
        let gap = run - off;
        let gap = gap - (gap / period).round() * period;
        let stale = !self.set || t < self.t || self.vel != vel || gap.length() * sky::CLOUD_SCALE_M > DRIFT_SLACK_M;
        if stale {
            *self = Self { off, vel, t, set: true };
        }
        stale
    }

    /// The offset the shaders draw at `t`.
    pub fn at(&self, t: f32) -> Vec2 {
        self.off + self.vel * (t - self.t)
    }

    /// The drift lanes the sky's shaders run on (`deck_drift`): xy the
    /// velocity, z when the offset was taken.
    pub fn flow(&self) -> Vec4 {
        Vec4::new(self.vel.x, self.vel.y, self.t, 0.0)
    }
}

/// The deck's drift as the shaders have it. `gen` moves with every new
/// anchor, which is what the sky's materials watch.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct Drift {
    pub anchor: Anchor,
    pub gen: u32,
}

/// The mood for a newly built map. Called by the level's rebuild.
pub fn set(now: &mut MoodNow, map: u8) {
    let want = MoodNow::of(map);
    if *now != want {
        *now = want;
        info!("oneshot: sky {} over map {map}", want.mood.name);
    }
}

/// Take the drift again when the clock's has left the shaders' run, or the
/// map has changed. After the clock is read, before the sky is driven.
pub fn drift(clock: Res<SkyClock>, time: Res<Time>, mood: Res<MoodNow>, mut d: ResMut<Drift>) {
    let m = &mood.mood;
    let t = time.elapsed_secs_wrapped();
    let mut a = d.anchor;
    let fresh = mood.is_changed() && a.set;
    if fresh {
        a.set = false;
    }
    if a.track(t, m.drift_at(clock.s), m.drift_vel()) {
        d.anchor = a;
        d.gen = d.gen.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every mood keeps a fighter readable: the sun up, its light and the
    /// fill no lower than a golden hour's, the mist and the rain light, and
    /// the cloud shadows never taking more than half the sun.
    #[test]
    fn every_mood_keeps_a_fighter_readable() {
        for id in 0..=8u8 {
            let m = of(id);
            assert!(m.sun_elev > 0.15, "{}: the sun is up", m.name);
            assert!(m.sun >= 0.5 && m.fill >= 0.7, "{}: lit", m.name);
            // What lights a sunlit floor, against noon's, after exposure.
            let lit = (m.sun * m.sun_elev.sin() * 100.0 + m.fill * 18.0) * m.exposure_gain();
            let noon = NOON_ELEV.sin() * 100.0 + 18.0;
            assert!(lit > 0.5 * noon, "{}: {lit} against noon's {noon}", m.name);
            // Across an 80 m sightline more than three fifths gets through.
            assert!((-m.fog_sigma() * 80.0).exp() > 0.6, "{}: the air", m.name);
            assert!(m.rain <= 0.4 && m.shade <= 0.5, "{}", m.name);
        }
    }

    /// No mood's grade has a step in it: its contrast and its lift move a
    /// little at a time from black to past white, weighed as Bevy 0.18's
    /// shader weighs its three sections (`sectional_color_grading`: at the
    /// foot of the midtones what isn't theirs goes to the highlights).
    #[test]
    fn no_grade_steps() {
        use bevy::render::view::{ColorGrading, ColorGradingSection};
        const MARGIN: f32 = 0.1;
        let weights = |level: f32, mid: &core::ops::Range<f32>| {
            if level < mid.start - MARGIN {
                [1.0, 0.0, 0.0]
            } else if level < mid.start + MARGIN {
                let m = (level - mid.start) * 0.5 / MARGIN + 0.5;
                [0.0, m, 1.0 - m]
            } else if level < mid.end - MARGIN {
                [0.0, 1.0, 0.0]
            } else if level < mid.end + MARGIN {
                let h = (level - mid.end) * 0.5 / MARGIN + 0.5;
                [0.0, 1.0 - h, h]
            } else {
                [0.0, 0.0, 1.0]
            }
        };
        for id in 0..=8u8 {
            let m = of(id);
            let mut c = ColorGrading::default();
            super::super::rig::grade(m.grade, &mut c);
            let at = |level: f32| {
                let w = weights(level, &c.global.midtones_range);
                let mix = |of: fn(&ColorGradingSection) -> f32| {
                    w[0] * of(&c.shadows) + w[1] * of(&c.midtones) + w[2] * of(&c.highlights)
                };
                [mix(|s| s.contrast), mix(|s| s.lift)]
            };
            let mut last = at(0.0);
            for k in 1..=2000 {
                let level = k as f32 / 1000.0;
                let now = at(level);
                for (a, b) in now.iter().zip(last) {
                    assert!((a - b).abs() < 0.002, "{}: a step at {level}", m.name);
                }
                last = now;
            }
        }
    }

    /// On a map split between two teams neither looks into the sun at the
    /// other: no sun stands in a level look from one side's spawns toward
    /// the other's, and a low one (whose backlight is strongest) stands
    /// across the line between them, so each side sees the other in the
    /// same light.
    #[test]
    fn no_team_looks_into_the_sun() {
        let half_h = (super::super::rig::fov_tall(super::super::rig::FOV_DEG).to_radians() * 0.5).tan();
        let half_w = half_h * 16.0 / 9.0;
        for def in sim::maps::ALL {
            let mid = |team: u8| {
                let at: Vec<Vec2> = def
                    .spawns
                    .iter()
                    .filter(|s| s.2 == team)
                    .map(|s| Vec2::new(s.0[0], s.0[2]))
                    .collect();
                (!at.is_empty()).then(|| at.iter().sum::<Vec2>() / at.len() as f32)
            };
            let (Some(red), Some(blue)) = (mid(0), mid(1)) else {
                continue;
            };
            let axis = (blue - red).normalize();
            let m = of(def.id);
            let sun = m.to_sun();
            let in_view = |look: Vec2| {
                let f = Vec3::new(look.x, 0.0, look.y);
                let right = f.cross(Vec3::Y);
                let z = sun.dot(f);
                z > 0.0 && (sun.y / z).abs() < half_h && (sun.dot(right) / z).abs() < half_w
            };
            assert!(
                !in_view(axis) && !in_view(-axis),
                "{}: {} in a team's look",
                def.name,
                m.name
            );
            if m.sun_elev < 30f32.to_radians() {
                let flat = Vec2::new(sun.x, sun.z).normalize();
                assert!(
                    flat.dot(axis).abs() < 5f32.to_radians().sin(),
                    "{}: {}'s low sun is down the line between the teams",
                    def.name,
                    m.name
                );
            }
        }
    }

    /// The anchor holds while the run keeps to the clock, and is taken
    /// again when it wanders, when the clock wraps and on a new map.
    #[test]
    fn the_drift_is_taken_again_only_when_it_wanders() {
        let m = DESERT;
        let mut a = Anchor::default();
        assert!(a.track(10.0, m.drift_at(100.0), m.drift_vel()));
        // A minute on, the clock and the run agree.
        assert!(!a.track(70.0, m.drift_at(160.0), m.drift_vel()));
        let near = a.at(70.0) - m.drift_at(160.0);
        assert!(near.length() * sky::CLOUD_SCALE_M < 0.01, "{near:?}");
        // The clock jumps a second: seven metres of drift.
        assert!(a.track(71.0, m.drift_at(162.0), m.drift_vel()));
        // `globals.time` wraps.
        assert!(a.track(1.0, m.drift_at(163.0), m.drift_vel()));
        // Another mood's wind.
        assert!(a.track(2.0, DUSK.drift_at(164.0), DUSK.drift_vel()));
    }

    /// The drift wraps on the field's period and moves downwind: the
    /// offset runs against the wind (`ComposeParams::drift`).
    #[test]
    fn the_deck_drifts_downwind_and_wraps() {
        let m = DESERT;
        let a = m.drift_at(0.0);
        let b = m.drift_at(10.0);
        let moved = (b - a) * sky::CLOUD_SCALE_M;
        assert!(moved.dot(m.wind()) < 0.0);
        assert!((moved.length() - m.wind_ms * 10.0).abs() < 0.05, "{moved:?}");
        let far = m.drift_at(1.0e7);
        let p = sky::FIELD_PERIOD as f32;
        assert!((0.0..=p).contains(&far.x) && (0.0..=p).contains(&far.y));
    }
}

//! The graphics ladder: what the player chose, what this target and this
//! GPU can do with it, and what that asks of the renderer. After Gates'
//! `crates/client/src/render/quality.rs` (MIT, NOTICE), trimmed to what an
//! arena draws.
//!
//! Three things, kept apart:
//!
//! - [`Gfx`]: what the player chose, one field per row on SETTINGS >
//!   GRAPHICS. Plain values, comparable with `==`. A [`Quality`] is a
//!   preset, a named column of [`preset`]'s table that writes every row at
//!   once; a row moved off it makes the QUALITY row read CUSTOM, derived
//!   (`Settings::preset_name`), never flagged.
//! - What this target can do with it, in two layers. [`for_target`] is
//!   fixed when the module is compiled (`gpu::WEBGL2`): WebGL2 draws one
//!   cascade, has no compute and no storage textures (no ambient
//!   occlusion), no motion vectors in its prepass layout (no TAA: SMAA
//!   stands in) and can't bloom. The settings are kept through it, so a row
//!   WebGL2 can't move says so instead of pretending. [`effective_on`] is
//!   what this device has room for (`gpu::GpuCaps`, read once at startup)
//!   and is never stored: a preset stays itself whichever GPU reads the
//!   file, and the screen's "on this machine" line says what the GPU
//!   refused.
//! - [`Tier`]: the resource the renderer reads. [`apply`] puts it on the
//!   camera and the sun; `grass::cull`, `fx::retier` and `render_scale`
//!   read it, and so will the sky, the foliage and the surfaces.
//!
//! **The top rung is the frame that shipped.** HIGH is what the desktop
//! and the WebGPU module drew before this ladder (SSAO medium, TAA, bloom,
//! four cascades to 200 m at Bevy's 2048, grass full to 46 m, every spark),
//! and MEDIUM through WebGL2's clamps is what the WebGL2 module drew (SMAA,
//! one cascade to 120 m, the same grass, its smaller spark pool). A fresh
//! install opens on the rung its target drew ([`default_quality_on`]), and
//! the tests hold both columns.
//!
//! **Fairness: no rung sees anyone another hides.** Wherever something
//! can hide a fighter, every rung draws the same:
//!
//! - **Shade.** The sun always casts: there is no SHADOWS OFF, because a
//!   world without cast shadows lights a fighter standing in a wall's
//!   shade. And however short the distance row, the last cascade reaches
//!   across the whole playable area ([`MapShade`]): past it no cast shadow
//!   is drawn, and a fighter in the shade there would stand in sunlight.
//!   Cascades and the map size only sharpen the shade.
//! - **Smoke and marks** are pooled the same on every rung (`fx::Caps`);
//!   EFFECTS moves only sparks, tracers and flash lights, which show and
//!   never hide.
//! - **Grass and ground clutter** are drawn the same on every rung, every
//!   tuft and piece to `grass::DRAW_M`: there is no GRASS row. They stand
//!   under a crouching fighter's hit box on his own ground, but over a
//!   crest or at a grazing look they still cover his lower half, so no
//!   rung may thin them or draw them nearer.
//! - **The air** is the same haze both ways (`sky::browser_haze`).
//!
//! What a lower rung gives up is finish: contact shading, edges, glow,
//! resolution, the shade's sharpness, sparks. Two of those are judgement
//! calls, kept because every shooter's ladder has them: SSAO darkens the
//! fill a little where a wall meets the ground, so a fighter crouched in a
//! wall's shade is a shade lighter with AO OFF (POTATO, LOW, always on
//! WebGL2); and bloom veils a little round a bright flash on the rungs
//! that have it. Both are a few percent of fill light, never a shadow.

use bevy::anti_alias::fxaa::Fxaa;
use bevy::anti_alias::smaa::Smaa;
use bevy::anti_alias::taa::TemporalAntiAliasing;
use bevy::core_pipeline::prepass::{DepthPrepass, MotionVectorPrepass, NormalPrepass};
use bevy::ecs::system::EntityCommands;
use bevy::light::{CascadeShadowConfig, CascadeShadowConfigBuilder, DirectionalLightShadowMap};
use bevy::pbr::{ScreenSpaceAmbientOcclusion, ScreenSpaceAmbientOcclusionQualityLevel as SsaoLevel};
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use bevy::render::camera::{MipBias, TemporalJitter};

use super::gpu::{self, GpuCaps};
use super::render_scale::RENDER_SCALE_MAX;
use super::rig::{EyeCam, Sun, CASCADE_FIRST_M, CASCADE_MIN_M, CASCADE_OVERLAP, FAR_M};
use crate::settings::Settings;

/// A named ladder: its rungs cheapest first, the file's spelling of each
/// (the screen shows it in capitals), and one click along it, which stops
/// at the ends rather than wrapping.
macro_rules! named {
    ($t:ident { $($v:ident = $n:literal),+ $(,)? }) => {
        impl $t {
            /// Cheapest first.
            pub const LADDER: &'static [$t] = &[$($t::$v),+];

            /// The file's spelling.
            pub fn name(self) -> &'static str {
                match self {
                    $($t::$v => $n),+
                }
            }

            /// The inverse; `None` for anything else, so a hand-typed
            /// value nobody can read keeps what the preset says.
            pub fn from_name(s: &str) -> Option<Self> {
                match s {
                    $($n => Some($t::$v),)+
                    _ => None,
                }
            }

            /// One click along the ladder, stopping at its ends.
            pub fn step(self, delta: i32) -> Self {
                let at = Self::LADDER.iter().position(|x| *x == self).unwrap_or(0) as i32;
                Self::LADDER[(at + delta.signum()).clamp(0, Self::LADDER.len() as i32 - 1) as usize]
            }
        }
    };
}

/// A preset: a named column of [`preset`]'s table.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Hash)]
pub enum Quality {
    /// For a machine that can't hold LOW: the world at 70% and stretched,
    /// one short cascade, no grass. What `potato.rs` drops a struggling
    /// machine to.
    Potato,
    /// No ambient occlusion, no bloom, SMAA, two cascades to 90 m, thinner
    /// grass nearer, fewer sparks.
    Low,
    /// Cheap ambient occlusion, the post chain whole, three cascades to
    /// 120 m. Through WebGL2's clamps, the frame that module shipped.
    Medium,
    /// The desktop's frame as it shipped.
    #[default]
    High,
}
named!(Quality {
    Potato = "potato",
    Low = "low",
    Medium = "medium",
    High = "high",
});

/// Ambient occlusion (Bevy's SSAO), as a name a file can carry.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Hash)]
pub enum Ao {
    #[default]
    Off,
    Low,
    Medium,
    High,
    Ultra,
}
named!(Ao {
    Off = "off",
    Low = "low",
    Medium = "medium",
    High = "high",
    Ultra = "ultra",
});

/// How edges are smoothed: a post pass (FXAA, SMAA) or the history (TAA,
/// which also resolves the grass's sub-pixel blades; a little softer).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Hash)]
pub enum Aa {
    #[default]
    Off,
    Fxaa,
    Smaa,
    Taa,
}
named!(Aa {
    Off = "off",
    Fxaa = "fxaa",
    Smaa = "smaa",
    Taa = "taa",
});

/// How much of the glowing kind of effect there is: the spark pool and the
/// flash lights (`fx::Caps`). Smoke and marks don't move with it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Hash)]
pub enum Effects {
    #[default]
    Low,
    High,
}
named!(Effects { Low = "low", High = "high" });

/// How the cloud deck is drawn (`deck.rs`): flat, per pixel, or a slab of
/// cloud a kilometre thick, raymarched in more or fewer steps. Only the
/// sky moves with it (and, at its top rung where the atmosphere draws, the
/// sun's shafts through a misty map's air, `shafts.rs`): a cloud is never
/// in front of a fighter, so no rung sees anyone another hides.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Hash)]
pub enum Clouds {
    #[default]
    Low,
    Medium,
    High,
}
named!(Clouds {
    Low = "low",
    Medium = "medium",
    High = "high",
});

/// What the player chose, one field per row on SETTINGS > GRAPHICS.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Gfx {
    /// How far the last shadow cascade reaches, metres; never under the
    /// map's [`MapShade`] where it's drawn.
    pub shadow_m: f32,
    /// How many cascades that distance is cut into ([`max_cascades`]).
    pub cascades: usize,
    /// One cascade's shadow map, texels a side.
    pub shadow_map_px: usize,
    pub ao: Ao,
    pub aa: Aa,
    pub bloom: bool,
    /// Percent of each dimension the world is drawn at; the HUD stays at
    /// the window's (`render_scale.rs`).
    pub render_scale: u8,
    pub effects: Effects,
    pub clouds: Clouds,
}

/// The shadow distances the screen offers, metres, ascending. The floor is
/// above `rig::CASCADE_FIRST_M`, and that is structural: the cascade
/// builder asserts `minimum_distance < first_cascade_far_bound`, and a
/// distance under the first cascade's bound splits them in the wrong order.
pub const SHADOW_M_LADDER: [f32; 7] = [40.0, 60.0, 90.0, 120.0, 160.0, 200.0, 300.0];

const _: () = assert!(
    SHADOW_M_LADDER[0] > CASCADE_FIRST_M && CASCADE_FIRST_M > CASCADE_MIN_M,
    "the shortest shadow distance must stay above rig::CASCADE_FIRST_M: the cascade builder asserts on it"
);

/// The shadow map sizes, texels a side: powers of two, which is what keeps
/// a cascade's texel exactly representable and its edges from crawling.
pub const SHADOW_PX_LADDER: [usize; 4] = [512, 1024, 2048, 4096];

/// POTATO's render scale, percent of each dimension (half the pixels).
pub const POTATO_RENDER_SCALE: u8 = 70;

/// Room past the map's own diagonal for the shade's reach ([`MapShade`]):
/// the ground's rise and fall, and a fighter's height.
pub const SHADE_MARGIN_M: f32 = 8.0;

/// Cascades a light can have here. Bevy's `MAX_CASCADES_PER_LIGHT` is 4,
/// and 1 under WebGL2, where asking for more doesn't fail: it keeps
/// cascade 0 alone and every shadow stops at its far edge (Gates shipped
/// that once). So WebGL2 asks for one, which covers the whole distance.
pub const fn max_cascades_on(webgl2: bool) -> usize {
    if webgl2 {
        1
    } else {
        4
    }
}

/// [`max_cascades_on`] this build.
pub const fn max_cascades() -> usize {
    max_cascades_on(gpu::WEBGL2)
}

/// The largest shadow map this target is sure to allocate: WebGL2's floor
/// for `MAX_TEXTURE_SIZE` is 2048 (WebGPU's 8192).
pub const fn max_shadow_map_px_on(webgl2: bool) -> usize {
    if webgl2 {
        2048
    } else {
        4096
    }
}

/// [`max_shadow_map_px_on`] this build.
pub const fn max_shadow_map_px() -> usize {
    max_shadow_map_px_on(gpu::WEBGL2)
}

/// One preset's column, for a target (`webgl2`). Read down a column: HIGH
/// is what shipped, and each rung below is the same frame with work taken
/// out, never one cost raised to pay for another (`tests` hold that).
pub fn preset_on(q: Quality, webgl2: bool) -> Gfx {
    match q {
        Quality::High => Gfx {
            shadow_m: 200.0,
            cascades: 4,
            shadow_map_px: 2048,
            ao: Ao::Medium,
            aa: Aa::Taa,
            bloom: true,
            render_scale: RENDER_SCALE_MAX,
            effects: Effects::High,
            clouds: Clouds::High,
        },
        Quality::Medium => Gfx {
            shadow_m: 120.0,
            cascades: 3,
            shadow_map_px: 2048,
            ao: Ao::Low,
            aa: Aa::Taa,
            bloom: true,
            render_scale: RENDER_SCALE_MAX,
            effects: Effects::Low,
            // The second budget spent by target: the WebGL2 module opens
            // on this rung, on whatever GPU a browser without WebGPU has,
            // so its deck is flat here and the slab is its HIGH (or the
            // row's); the desktop and WebGPU march a short slab.
            clouds: if webgl2 { Clouds::Low } else { Clouds::Medium },
        },
        Quality::Low => Gfx {
            shadow_m: 90.0,
            cascades: 2,
            // The one value that differs by target, and it is the same
            // budget spent where the target left it: WebGL2 is given one
            // cascade whatever LOW asks for, so the map that two would have
            // cost goes into the one (Gates' LOW, 27 cm a texel at 90 m
            // against 14). A budget, not a clamp: the row can still step it.
            shadow_map_px: if webgl2 { 2048 } else { 1024 },
            ao: Ao::Off,
            aa: Aa::Smaa,
            bloom: false,
            render_scale: RENDER_SCALE_MAX,
            effects: Effects::Low,
            clouds: Clouds::Low,
        },
        // Fill rate is what such a GPU runs out of first: the world at 70%
        // (half the pixels) and stretched, one cascade, no post at all.
        Quality::Potato => Gfx {
            shadow_m: SHADOW_M_LADDER[1],
            cascades: 1,
            shadow_map_px: 1024,
            ao: Ao::Off,
            aa: Aa::Off,
            bloom: false,
            render_scale: POTATO_RENDER_SCALE,
            effects: Effects::Low,
            clouds: Clouds::Low,
        },
    }
}

/// [`preset_on`] this build.
pub fn preset(q: Quality) -> Gfx {
    preset_on(q, gpu::WEBGL2)
}

/// What a target fixed at compile time can do with a choice. Every term is
/// a clamp or a constant, so it is idempotent.
pub fn for_target_on(g: Gfx, webgl2: bool) -> Gfx {
    let g = Gfx {
        cascades: g.cascades.clamp(1, max_cascades_on(webgl2)),
        shadow_map_px: g.shadow_map_px.min(max_shadow_map_px_on(webgl2)),
        ..g
    };
    if !webgl2 {
        return g;
    }
    // WebGL2's walls, each a panic or a lost frame rather than a
    // degradation if it were asked for: SSAO's prepasses want a storage
    // buffer in the fragment stage and WebGL2 has none (measured in Gates:
    // the module dies building the layout), TAA's motion vectors the same,
    // and Bevy says bloom "is not compatible with WebGL2". A player who
    // chose TAA chose smooth edges: SMAA stands in.
    Gfx {
        ao: Ao::Off,
        aa: if g.aa == Aa::Taa { Aa::Smaa } else { g.aa },
        bloom: false,
        ..g
    }
}

/// [`for_target_on`] this build: what the settings keep.
pub fn for_target(g: Gfx) -> Gfx {
    for_target_on(g, gpu::WEBGL2)
}

/// What a device with `caps` draws of a choice: the target's clamps, then
/// what the GPU has room for. Occlusion is kept before TAA when only one
/// fits (it is what pays the sky fill back), and SMAA stands in for TAA.
pub fn effective_on(g: Gfx, webgl2: bool, caps: GpuCaps) -> Gfx {
    let g = for_target_on(g, webgl2);
    let ao = if g.ao != Ao::Off && caps.ao(false) {
        g.ao
    } else {
        Ao::Off
    };
    let aa = if g.aa == Aa::Taa && !caps.taa(ao != Ao::Off) {
        Aa::Smaa
    } else {
        g.aa
    };
    Gfx {
        ao,
        aa,
        bloom: g.bloom && caps.rg11b10_renderable,
        ..g
    }
}

/// [`effective_on`] this build on this device.
pub fn effective(g: Gfx) -> Gfx {
    effective_on(g, gpu::WEBGL2, gpu::caps())
}

/// The rung a fresh install opens on: the one its target drew before the
/// ladder. The desktop and the WebGPU module drew HIGH; the WebGL2 module
/// drew MEDIUM's frame. The machine may still take it lower (`potato.rs`).
pub const fn default_quality_on(webgl2: bool) -> Quality {
    if webgl2 {
        Quality::Medium
    } else {
        Quality::High
    }
}

/// [`default_quality_on`] this build.
pub const fn default_quality() -> Quality {
    default_quality_on(gpu::WEBGL2)
}

/// A fresh install's rows.
pub fn default_gfx() -> Gfx {
    for_target(preset(default_quality()))
}

/// One click along an ascending ladder of values, stopping at its ends: to
/// the next rung strictly past `v`, so a hand-edited value between two
/// rungs steps onto the one it sits beside, not over it.
pub fn step_ladder<T: Copy + PartialOrd>(ladder: &[T], v: T, delta: i32) -> T {
    let last = ladder.len() - 1;
    match delta.signum() {
        1 => ladder[ladder.iter().position(|r| *r > v).unwrap_or(last)],
        -1 => ladder[ladder.iter().rposition(|r| *r < v).unwrap_or(0)],
        _ => v,
    }
}

/// How far the shade must reach on a map so that every rung draws it over
/// the whole playable area (the header's fairness): the area's diagonal
/// and [`SHADE_MARGIN_M`]. Zero before a map is built.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Default)]
pub struct MapShade {
    pub floor_m: f32,
}

impl MapShade {
    pub fn of(def: &sim::map::Def) -> MapShade {
        let (w, d) = (def.hi.0 - def.lo.0, def.hi.1 - def.lo.1);
        MapShade {
            floor_m: ((w * w + d * d).sqrt() + SHADE_MARGIN_M).min(FAR_M),
        }
    }
}

/// What the renderer draws now: the player's rows through this target and
/// this device, on this map. A resource, written only when it moves.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct Tier {
    /// The rows as drawn ([`effective_on`]).
    pub gfx: Gfx,
    /// The preset the player's rows spell, if they spell one.
    pub preset: Option<Quality>,
    /// Bevy's atmosphere draws the sky and the air (`gpu::atmosphere_on`);
    /// without it the sky is baked and the air is a fog.
    pub atmosphere: bool,
    /// How far the sun's last cascade reaches: the row, or the map's
    /// [`MapShade`] if that is further.
    pub shadow_m: f32,
}

impl Default for Tier {
    fn default() -> Self {
        Tier::resolve(
            default_gfx(),
            Some(default_quality()),
            gpu::WEBGL2,
            GpuCaps::UNBOUNDED,
            MapShade::default(),
        )
    }
}

impl Tier {
    pub fn resolve(choice: Gfx, preset: Option<Quality>, webgl2: bool, caps: GpuCaps, shade: MapShade) -> Tier {
        let gfx = effective_on(choice, webgl2, caps);
        Tier {
            gfx,
            preset,
            atmosphere: gpu::atmosphere_on(webgl2, caps),
            shadow_m: gfx.shadow_m.max(shade.floor_m).min(FAR_M),
        }
    }

    /// Bevy's SSAO level, or none.
    pub fn ssao(&self) -> Option<SsaoLevel> {
        match self.gfx.ao {
            Ao::Off => None,
            Ao::Low => Some(SsaoLevel::Low),
            Ao::Medium => Some(SsaoLevel::Medium),
            Ao::High => Some(SsaoLevel::High),
            Ao::Ultra => Some(SsaoLevel::Ultra),
        }
    }

    /// The sun's cascades. How they divide the distance is not a row (it
    /// would move where the shade's resolution steps, a look rather than a
    /// budget); with one cascade there is no split and it covers it all.
    pub fn cascades(&self) -> CascadeShadowConfig {
        CascadeShadowConfigBuilder {
            num_cascades: self.gfx.cascades.max(1),
            minimum_distance: CASCADE_MIN_M,
            maximum_distance: self.shadow_m,
            first_cascade_far_bound: CASCADE_FIRST_M,
            overlap_proportion: CASCADE_OVERLAP,
        }
        .build()
    }

    /// The cloud deck's view and light steps (`deck::steps`): `(0, 0)` is
    /// the flat deck.
    pub fn cloud_steps(&self) -> (u32, u32) {
        super::deck::steps(self.gfx.clouds)
    }

    /// Whether the sun's shafts through a misty map's air are drawn: Bevy's
    /// volumetric pass, which wants what the atmosphere wants (never
    /// WebGL2), at the CLOUDS row's top rung, the same family of
    /// raymarched air (`shafts.rs`).
    pub fn shafts(&self) -> bool {
        self.atmosphere && self.gfx.clouds == Clouds::High
    }

    /// What it draws, for the log and the page (`window.__oneshot.tier`).
    pub fn describe(&self) -> String {
        let g = &self.gfx;
        format!(
            "{} ({}): {}, ao {}, aa {}, bloom {}, {} cascade(s) to {:.0} m at {}px, effects {}, clouds {}{}, {}%",
            self.preset.map_or("custom", Quality::name),
            gpu::api(),
            if self.atmosphere { "atmosphere" } else { "haze" },
            g.ao.name(),
            g.aa.name(),
            if g.bloom { "on" } else { "off" },
            g.cascades,
            self.shadow_m,
            g.shadow_map_px,
            g.effects.name(),
            g.clouds.name(),
            if self.shafts() { " with shafts" } else { "" },
            g.render_scale,
        )
    }
}

/// The aspect [`far_texel_m`] reports against: a stated 16:9, not the
/// window's, so the readout doesn't move as the window does.
pub const READOUT_ASPECT: f32 = 16.0 / 9.0;

/// How coarse the shade is at its far edge, metres of world per shadow
/// texel: Bevy's own arithmetic (`calculate_cascade` fits the last
/// cascade's box to the larger of its slice's body and far-plane diagonals
/// and divides by the map size), restated so the settings screen can say
/// it with no camera. `cascades` and `shadow_m` are what's drawn.
pub fn far_texel_m(cascades: usize, shadow_m: f32, shadow_map_px: usize, fov_deg: f32) -> f32 {
    let n = cascades.max(1);
    let (near, far) = if n == 1 {
        (CASCADE_MIN_M, shadow_m)
    } else {
        let base = (shadow_m / CASCADE_FIRST_M).powf(1.0 / (n - 1) as f32);
        let prev = CASCADE_FIRST_M * base.powf((n - 2) as f32);
        ((1.0 - CASCADE_OVERLAP) * prev, shadow_m)
    };
    let t = (fov_deg.to_radians() * 0.5).tan();
    let corner = |d: f32, sx: f32, sy: f32| Vec3::new(sx * d * t * READOUT_ASPECT, sy * d * t, -d);
    let near_br = corner(near, 1.0, -1.0);
    let far_br = corner(far, 1.0, -1.0);
    let far_tl = corner(far, -1.0, 1.0);
    let diameter = near_br.distance(far_tl).max(far_br.distance(far_tl)).ceil();
    diameter / shadow_map_px.max(1) as f32
}

/// The shadow maps' video memory, MiB: one `Depth32Float` layer a cascade.
pub fn shadow_vram_mb(cascades: usize, shadow_map_px: usize) -> f32 {
    (shadow_map_px * shadow_map_px * cascades.max(1)) as f32 * 4.0 / (1024.0 * 1024.0)
}

pub fn plugin(app: &mut App) {
    app.init_resource::<MapShade>();
    refresh(app.world_mut());
    app.add_systems(PreUpdate, (resolve, apply).chain());
}

/// The tier from the settings as they stand, now: at build, and again once
/// the device is read and the machine looked at (`gpu::GpuPlugin`), before
/// anything is spawned with it.
pub fn refresh(world: &mut World) {
    let Some(s) = world.get_resource::<Settings>() else {
        return;
    };
    let shade = world.get_resource::<MapShade>().copied().unwrap_or_default();
    let t = Tier::resolve(s.gfx, s.preset_name(), gpu::WEBGL2, gpu::caps(), shade);
    world.insert_resource(t);
}

/// The tier from the settings and the map, written only when it moves.
pub fn resolve(settings: Res<Settings>, shade: Res<MapShade>, mut tier: ResMut<Tier>) {
    if !settings.is_changed() && !shade.is_changed() {
        return;
    }
    let t = Tier::resolve(settings.gfx, settings.preset_name(), gpu::WEBGL2, gpu::caps(), *shade);
    tier.set_if_neq(t);
}

/// The tier onto the camera and the sun: when it moves, and onto a camera
/// or a sun the frame it appears (a level's sun is new with each level).
/// It reconciles rather than reacts, so a rung chosen last session reaches
/// a camera spawned later (Gates shipped that bug for ten minutes).
pub fn apply(
    mut commands: Commands,
    tier: Res<Tier>,
    cams: Query<(Entity, Ref<EyeCam>)>,
    suns: Query<(Entity, Ref<Sun>)>,
    shadow_map: Option<ResMut<DirectionalLightShadowMap>>,
) {
    let moved = tier.is_changed();
    for (e, cam) in &cams {
        if moved || cam.is_added() {
            dress(&mut commands.entity(e), &tier);
        }
    }
    for (e, sun) in &suns {
        if moved || sun.is_added() {
            commands.entity(e).insert(tier.cascades());
        }
    }
    if let Some(mut map) = shadow_map {
        if map.size != tier.gfx.shadow_map_px {
            map.size = tier.gfx.shadow_map_px;
        }
    }
}

/// The camera's post stack for a tier. What a rung leaves out is removed,
/// and on a device short of textures the prepasses only it wanted (each is
/// a sampled texture in every main-pass pipeline, `gpu::GpuCaps`); where
/// there is room they stay, as Gates keeps them.
fn dress(e: &mut EntityCommands, t: &Tier) {
    let ssao = t.ssao();
    match ssao {
        Some(quality_level) => {
            e.insert(ScreenSpaceAmbientOcclusion {
                quality_level,
                ..default()
            });
        }
        None => {
            e.remove::<ScreenSpaceAmbientOcclusion>();
        }
    }
    let taa = t.gfx.aa == Aa::Taa;
    if taa {
        e.remove::<(Smaa, Fxaa)>();
        e.insert(TemporalAntiAliasing::default());
    } else {
        // TAA's required components outlive it: left behind, the frame
        // jitters with nothing to resolve it.
        e.remove::<(TemporalAntiAliasing, TemporalJitter, MipBias, MotionVectorPrepass)>();
        match t.gfx.aa {
            Aa::Smaa => {
                e.remove::<Fxaa>();
                e.insert(Smaa::default());
            }
            Aa::Fxaa => {
                e.remove::<Smaa>();
                e.insert(Fxaa::default());
            }
            _ => {
                e.remove::<(Smaa, Fxaa)>();
            }
        }
    }
    if t.gfx.bloom {
        e.insert(Bloom::NATURAL);
    } else {
        e.remove::<Bloom>();
    }
    // A prepass's presence is in every mesh pipeline's key, so taking one
    // off re-specialises them all (a stall, in a browser's one thread):
    // only where the device is short of the textures they bind, as Gates.
    if ssao.is_none() && gpu::caps().prepass_room() < PREPASS_KEEP_ROOM {
        e.remove::<NormalPrepass>();
        if !taa {
            e.remove::<DepthPrepass>();
        }
    }
}

/// The prepass room (`gpu::GpuCaps::prepass_room`) from which a prepass a
/// rung no longer needs is kept rather than taken off (Gates' three).
pub const PREPASS_KEEP_ROOM: u32 = 3;

#[cfg(test)]
mod tests {
    use super::super::render_scale::{RENDER_SCALE_MIN, RENDER_SCALE_STEP};
    use super::*;

    /// What SwiftShader's WebGPU adapter offers: the floor.
    const FLOOR: GpuCaps = GpuCaps {
        sampled_textures: 16,
        storage_textures: 4,
        float32_filterable: true,
        rg11b10_renderable: true,
        dual_source_blending: false,
    };

    fn rank_ssao(s: Option<SsaoLevel>) -> u8 {
        match s {
            None => 0,
            Some(SsaoLevel::Low) => 1,
            Some(SsaoLevel::Medium) => 2,
            Some(SsaoLevel::High) => 3,
            Some(SsaoLevel::Ultra) => 4,
            Some(other) => panic!("a tier asks for {other:?}, which the ladder can't rank"),
        }
    }

    fn rank<T: PartialEq>(ladder: &[T], v: &T) -> usize {
        ladder.iter().position(|x| x == v).expect("on its ladder")
    }

    /// The golden: HIGH is the desktop's frame as it shipped
    /// (`gpu::tier_for` before this ladder: SSAO medium, TAA, bloom, four
    /// cascades to 200 m, no shadow map size set so Bevy's own, grass to
    /// `grass::DRAW_M`, the desktop's fx caps), and MEDIUM through WebGL2's
    /// clamps is the WebGL2 frame as it shipped (SMAA, one cascade to
    /// 120 m, the same grass, the low fx caps).
    #[test]
    fn the_top_rung_is_the_frame_that_shipped() {
        let t = Tier::resolve(
            preset_on(Quality::High, false),
            Some(Quality::High),
            false,
            GpuCaps::UNBOUNDED,
            MapShade::default(),
        );
        assert!(t.atmosphere);
        assert_eq!(t.ssao(), Some(SsaoLevel::Medium));
        assert_eq!(t.gfx.aa, Aa::Taa);
        assert!(t.gfx.bloom);
        assert_eq!((t.gfx.cascades, t.shadow_m), (4, 200.0));
        assert_eq!(t.gfx.shadow_map_px, DirectionalLightShadowMap::default().size);
        assert_eq!((t.gfx.effects, t.gfx.render_scale), (Effects::High, 100));
        assert_eq!(t.gfx.clouds, Clouds::High);
        assert!(t.shafts());

        let w = Tier::resolve(
            preset_on(Quality::Medium, true),
            Some(Quality::Medium),
            true,
            GpuCaps::UNBOUNDED,
            MapShade::default(),
        );
        assert!(!w.atmosphere && w.ssao().is_none() && !w.gfx.bloom);
        assert_eq!(w.gfx.aa, Aa::Smaa);
        assert_eq!((w.gfx.cascades, w.shadow_m), (1, 120.0));
        assert_eq!(w.gfx.shadow_map_px, DirectionalLightShadowMap::default().size);
        assert_eq!((w.gfx.effects, w.gfx.render_scale), (Effects::Low, 100));
        assert_eq!(w.gfx.clouds, Clouds::Low, "the flat deck WebGL2 drew");
        assert!(!w.shafts(), "no volumetric pass under WebGL2");

        assert_eq!(default_quality_on(false), Quality::High);
        assert_eq!(default_quality_on(true), Quality::Medium);
    }

    /// Down the ladder everything gets cheaper or stays, on both targets:
    /// a rung that raised one cost to lower another would be a look, not a
    /// budget. And no rung turns the shade off.
    #[test]
    fn every_rung_down_is_cheaper_on_every_axis() {
        for webgl2 in [false, true] {
            for pair in Quality::LADDER.windows(2) {
                let tier = |q| {
                    Tier::resolve(
                        preset_on(q, webgl2),
                        Some(q),
                        webgl2,
                        GpuCaps::UNBOUNDED,
                        MapShade::default(),
                    )
                };
                let (lo, hi) = (tier(pair[0]), tier(pair[1]));
                let (l, h) = (lo.gfx, hi.gfx);
                let at = format!("{:?} under {:?} (webgl2 {webgl2})", pair[0], pair[1]);
                assert!(rank_ssao(lo.ssao()) <= rank_ssao(hi.ssao()), "{at}: ao");
                assert!(rank(Aa::LADDER, &l.aa) <= rank(Aa::LADDER, &h.aa), "{at}: aa");
                assert!(!(l.bloom && !h.bloom), "{at}: bloom");
                assert!(l.cascades <= h.cascades, "{at}: cascades");
                assert!(lo.shadow_m <= hi.shadow_m, "{at}: shadow distance");
                assert!(l.shadow_map_px <= h.shadow_map_px, "{at}: shadow map");
                assert!(l.render_scale <= h.render_scale, "{at}: render scale");
                assert!(
                    rank(Effects::LADDER, &l.effects) <= rank(Effects::LADDER, &h.effects),
                    "{at}: fx"
                );
                assert!(
                    rank(Clouds::LADDER, &l.clouds) <= rank(Clouds::LADDER, &h.clouds),
                    "{at}: clouds"
                );
                assert!(!(lo.shafts() && !hi.shafts()), "{at}: shafts");
                assert!(lo != hi, "{at}: two rungs that draw the same frame");
                assert!(l.cascades >= 1 && l.shadow_map_px > 0, "{at}: the shade is never off");
            }
        }
    }

    /// Every cascade a rung asks for is one the engine draws, and the last
    /// one ends where the row (or the map's shade) says.
    #[test]
    fn every_cascade_asked_for_is_a_cascade_the_engine_will_draw() {
        for webgl2 in [false, true] {
            for &q in Quality::LADDER {
                let t = Tier::resolve(
                    preset_on(q, webgl2),
                    Some(q),
                    webgl2,
                    GpuCaps::UNBOUNDED,
                    MapShade::default(),
                );
                assert!(t.gfx.cascades >= 1 && t.gfx.cascades <= max_cascades_on(webgl2));
                let b = t.cascades().bounds;
                assert_eq!(b.len(), t.gfx.cascades, "{q:?}");
                assert!((b[b.len() - 1] - t.shadow_m).abs() < 1e-3, "{q:?}: {b:?}");
            }
        }
    }

    /// One cascade covers the whole distance (WebGL2's shape); the first of
    /// two ends at `CASCADE_FIRST_M`, which is where every WebGL2 shadow
    /// stopped when it asked for two.
    #[test]
    fn one_cascade_reaches_the_whole_shadow_distance() {
        let mut t = Tier::default();
        t.gfx.cascades = 1;
        t.shadow_m = 90.0;
        let b = t.cascades().bounds;
        assert_eq!(b.len(), 1);
        assert!((b[0] - 90.0).abs() < 1e-3);
        t.gfx.cascades = 2;
        assert!((t.cascades().bounds[0] - CASCADE_FIRST_M).abs() < 1e-3);
    }

    /// The ladders ascend, the map sizes are powers of two, every rung
    /// builds, and every preset sits on its ladders (or a stepper couldn't
    /// return to it).
    #[test]
    fn every_preset_is_reachable_from_the_steppers() {
        for w in SHADOW_M_LADDER.windows(2) {
            assert!(w[0] < w[1]);
        }
        for w in SHADOW_PX_LADDER.windows(2) {
            assert!(w[0] < w[1]);
        }
        assert!(SHADOW_PX_LADDER.iter().all(|p| p.is_power_of_two()));
        for m in SHADOW_M_LADDER {
            for n in 1..=4 {
                let mut t = Tier::default();
                t.gfx.cascades = n;
                t.shadow_m = m;
                assert_eq!(t.cascades().bounds.len(), n, "{n} cascades to {m} m");
            }
        }
        for webgl2 in [false, true] {
            for &q in Quality::LADDER {
                let g = preset_on(q, webgl2);
                assert!(SHADOW_M_LADDER.contains(&g.shadow_m), "{q:?}");
                assert!(SHADOW_PX_LADDER.contains(&g.shadow_map_px), "{q:?}");
                assert!(g.render_scale >= RENDER_SCALE_MIN && g.render_scale <= RENDER_SCALE_MAX);
                assert_eq!(g.render_scale % RENDER_SCALE_STEP, 0, "{q:?}");
            }
        }
    }

    /// The clamps are idempotent, which is what lets a preset be kept
    /// already clamped and still be found by name.
    #[test]
    fn the_target_clamp_is_idempotent() {
        let mut seen: Vec<Gfx> = Vec::new();
        for &q in Quality::LADDER {
            seen.push(preset_on(q, false));
        }
        for px in SHADOW_PX_LADDER {
            for n in 1..=6usize {
                for &aa in Aa::LADDER {
                    seen.push(Gfx {
                        cascades: n,
                        shadow_map_px: px,
                        aa,
                        ..preset_on(Quality::High, false)
                    });
                }
            }
        }
        for g in seen {
            for webgl2 in [false, true] {
                let once = for_target_on(g, webgl2);
                assert_eq!(for_target_on(once, webgl2), once, "{g:?}");
                for caps in [GpuCaps::UNBOUNDED, FLOOR] {
                    let e = effective_on(g, webgl2, caps);
                    assert_eq!(effective_on(e, webgl2, caps), e, "{g:?}");
                }
            }
        }
    }

    /// WebGL2 never draws what it can't, and never sees further: its
    /// shade reaches no further than the desktop's on the same rung.
    #[test]
    fn webgl2_is_clamped_and_never_sees_further() {
        for &q in Quality::LADDER {
            let w = for_target_on(preset_on(q, true), true);
            let d = for_target_on(preset_on(q, false), false);
            assert_eq!((w.ao, w.bloom, w.cascades), (Ao::Off, false, 1), "{q:?}");
            assert_ne!(w.aa, Aa::Taa, "{q:?}");
            assert!(w.shadow_map_px <= 2048);
            assert!(w.shadow_m <= d.shadow_m, "{q:?}");
        }
    }

    /// A device at WebGPU's floor keeps the rows that fit and drops the
    /// prepasses: no occlusion, SMAA for TAA. Room for one pair: occlusion.
    #[test]
    fn a_device_short_of_textures_keeps_what_fits() {
        let high = preset_on(Quality::High, false);
        let e = effective_on(high, false, FLOOR);
        assert_eq!((e.ao, e.aa), (Ao::Off, Aa::Smaa));
        assert!(e.bloom);
        let one = GpuCaps {
            sampled_textures: gpu::FRAGMENT_TEXTURES + 2,
            storage_textures: 8,
            ..FLOOR
        };
        let e = effective_on(high, false, one);
        assert_eq!((e.ao, e.aa), (Ao::Medium, Aa::Smaa));
        let hw = GpuCaps {
            sampled_textures: 48,
            storage_textures: 8,
            ..FLOOR
        };
        assert_eq!(effective_on(high, false, hw), high);
        let no_bloom = GpuCaps {
            rg11b10_renderable: false,
            ..GpuCaps::UNBOUNDED
        };
        assert!(!effective_on(high, false, no_bloom).bloom);
    }

    /// The shade reaches across every map on every rung: the fairness
    /// floor. On the biggest map POTATO's shade goes as far as HIGH's
    /// needs to.
    #[test]
    fn the_shade_reaches_across_every_map_on_every_rung() {
        for def in sim::maps::ALL.iter() {
            let shade = MapShade::of(def);
            let (w, d) = (def.hi.0 - def.lo.0, def.hi.1 - def.lo.1);
            assert!(shade.floor_m >= (w * w + d * d).sqrt(), "{}", def.name);
            for webgl2 in [false, true] {
                for &q in Quality::LADDER {
                    let t = Tier::resolve(preset_on(q, webgl2), Some(q), webgl2, GpuCaps::UNBOUNDED, shade);
                    assert!(t.shadow_m >= shade.floor_m.min(FAR_M), "{} {q:?}", def.name);
                    let b = t.cascades().bounds;
                    assert!((b[b.len() - 1] - t.shadow_m).abs() < 1e-3);
                }
            }
        }
    }

    /// The far-edge readout is Bevy's split, and the memory one is the
    /// texture Bevy makes.
    #[test]
    fn the_readouts_are_the_engines_numbers() {
        assert_eq!(shadow_vram_mb(4, 2048), 64.0);
        assert_eq!(shadow_vram_mb(1, 1024), 4.0);
        // Gates' measurements (its tests/quality.rs), same constants, 75°.
        for (n, m, px, want_cm) in [
            (4, 200.0, 2048, 30.6152),
            (2, 90.0, 1024, 27.5391),
            (1, 90.0, 2048, 13.7695),
            (4, 200.0, 4096, 15.3076),
        ] {
            let got = far_texel_m(n, m, px, 75.0) * 100.0;
            assert!(
                (got - want_cm).abs() < 0.005,
                "{n}x{m} @{px}: {got:.4} cm, measured {want_cm}"
            );
        }
        // A wider view is a bigger frustum: FIELD OF VIEW moves the shade.
        assert!(far_texel_m(4, 200.0, 2048, 110.0) > far_texel_m(4, 200.0, 2048, 60.0) * 1.5);
    }

    /// A rung chosen last session reaches a camera and a sun that spawn
    /// later, and flipping it rebuilds the post stack without leaving TAA's
    /// companions behind (both shipped in Gates once).
    #[test]
    fn the_tier_reaches_a_camera_spawned_later_and_follows_the_knob() {
        let mut app = App::new();
        app.insert_resource(Settings::with_preset(Quality::Low))
            .init_resource::<MapShade>()
            .insert_resource(Tier::default())
            .init_resource::<DirectionalLightShadowMap>()
            .add_systems(Update, (resolve, apply).chain());
        app.update();
        let cam = app.world_mut().spawn(EyeCam).id();
        let sun = app.world_mut().spawn(Sun).id();
        app.update();
        let has = |app: &App| {
            let e = app.world().entity(cam);
            (
                e.contains::<ScreenSpaceAmbientOcclusion>(),
                e.contains::<TemporalAntiAliasing>(),
                e.contains::<Smaa>(),
                e.contains::<Bloom>(),
            )
        };
        assert_eq!(has(&app), (false, false, true, false), "LOW: SMAA alone");
        let low = preset(Quality::Low);
        assert_eq!(
            app.world().get::<CascadeShadowConfig>(sun).unwrap().bounds.len(),
            low.cascades
        );
        assert_eq!(
            app.world().resource::<DirectionalLightShadowMap>().size,
            low.shadow_map_px
        );

        *app.world_mut().resource_mut::<Settings>() = Settings::with_preset(Quality::High);
        app.update();
        assert_eq!(has(&app), (true, true, false, true), "HIGH: SSAO, TAA, bloom");
        assert_eq!(app.world().get::<CascadeShadowConfig>(sun).unwrap().bounds.len(), 4);

        *app.world_mut().resource_mut::<Settings>() = Settings::with_preset(Quality::Potato);
        app.update();
        assert_eq!(has(&app), (false, false, false, false), "POTATO: no post");
        let e = app.world().entity(cam);
        assert!(!e.contains::<TemporalJitter>() && !e.contains::<MipBias>() && !e.contains::<MotionVectorPrepass>());
        assert!(!e.contains::<Fxaa>());
        // No device here, so room to spare: the prepasses HIGH brought stay
        // rather than re-specialise every pipeline.
        assert_eq!(
            e.contains::<DepthPrepass>(),
            gpu::caps().prepass_room() >= PREPASS_KEEP_ROOM,
            "the depth prepass kept where there is room"
        );
    }

    #[test]
    fn a_stepper_lands_on_the_rung_it_is_between_and_stops_at_the_ends() {
        assert_eq!(step_ladder(&SHADOW_M_LADDER, 100.0, 1), 120.0);
        assert_eq!(step_ladder(&SHADOW_M_LADDER, 100.0, -1), 90.0);
        assert_eq!(step_ladder(&SHADOW_M_LADDER, 9000.0, 1), 300.0);
        assert_eq!(step_ladder(&SHADOW_M_LADDER, 40.0, -1), 40.0);
        assert_eq!(Quality::High.step(1), Quality::High);
        assert_eq!(Quality::Potato.step(-1), Quality::Potato);
        assert_eq!(Aa::Smaa.step(1), Aa::Taa);
        for &q in Quality::LADDER {
            assert_eq!(Quality::from_name(q.name()), Some(q));
        }
        assert_eq!(Quality::from_name("ultra"), None);
    }
}

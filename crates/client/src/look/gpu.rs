//! What the GPU under this build can draw, and so which frame it gets: the
//! desktop's on the desktop and in the browser's WebGPU module, the cheap
//! one in its WebGL2 module. After Gates' `render/quality.rs` (`GpuCaps`,
//! `max_cascades`) and `render/web.rs` (`watch_gpu`) (MIT, NOTICE).
//!
//! Two layers decide it:
//!
//! - **The API, fixed when the module is compiled** (`cfg(webgl2)`,
//!   build.rs). WebGL2 has no compute and no storage buffers in the fragment
//!   stage, so no atmosphere (its layout wants one: inserted there it kills
//!   the module, though the log says it is merely off), no SSAO or TAA
//!   (their prepasses want the same), no bloom (Bevy: "not compatible with
//!   WebGL2"), and Bevy gives a light one cascade there. The sky is then
//!   baked into the cloud cube and the air is a fog of the same air
//!   (`sky::browser_haze`). Nothing in it is a removal: the WebGL2 frame is
//!   the best WebGL2 can do with what it has.
//! - **The device, read once at startup** ([`GpuPlugin`]). A WebGPU adapter
//!   guarantees 16 sampled textures per fragment stage and 4 storage
//!   textures; Chrome gives most hardware 48 and 8, SwiftShader the floor.
//!   The worst main-pass pipeline (the ground under the atmosphere) is
//!   [`FRAGMENT_TEXTURES`] with no prepass, and each prepass it reads adds
//!   one, so TAA (depth and motion) and SSAO (depth and normal; five storage
//!   textures; filterable 32-bit floats) go on only where there is room. A
//!   pipeline over the limit is refused, and under WebGPU that loses the
//!   whole frame, not one mesh. SMAA stands in where TAA can't go.
//!
//! **Fairness:** the tiers differ in light and finish, never in what hides
//! a player. The haze is the same air both ways (`sky::browser_haze` is
//! the atmosphere's own coefficients), and WebGL2's shorter shadow reach
//! shows less, never more.

use std::sync::OnceLock;

use bevy::pbr::ScreenSpaceAmbientOcclusionQualityLevel;
use bevy::prelude::*;

/// This module draws through WebGL2 (`cfg(webgl2)`, build.rs).
pub const WEBGL2: bool = cfg!(webgl2);

/// The GPU API this build draws with, as the page names it.
pub const fn api() -> &'static str {
    if cfg!(webgl2) {
        "webgl2"
    } else if cfg!(target_arch = "wasm32") {
        "webgpu"
    } else {
        "native"
    }
}

/// Shadow cascades a light can have here: Bevy's own
/// `MAX_CASCADES_PER_LIGHT`, 1 under WebGL2. Asking for more there doesn't
/// fail, it keeps cascade 0 only, and every shadow stops at its far edge
/// (Gates shipped that once): so WebGL2 asks for one.
pub const fn max_cascades() -> usize {
    if cfg!(webgl2) {
        1
    } else {
        4
    }
}

/// What the device can bind (Gates' `GpuCaps`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GpuCaps {
    /// `max_sampled_textures_per_shader_stage`.
    pub sampled_textures: u32,
    /// `max_storage_textures_per_shader_stage`.
    pub storage_textures: u32,
    /// `FLOAT32_FILTERABLE`: SSAO samples an `R32Float` depth, and the
    /// atmosphere binds its `Rgba32Float` tables as filterable without
    /// asking (the page also refuses WebGPU to an adapter without it).
    pub float32_filterable: bool,
    /// `RG11B10UFLOAT_RENDERABLE`: bloom's mip chain, which Bevy never
    /// checks it can draw into.
    pub rg11b10_renderable: bool,
    /// `DUAL_SOURCE_BLENDING`: without it Bevy's atmosphere lays the sky
    /// over the cloud deck through the MEAN of its transmittance, not per
    /// channel, so `sky::deck_hue` must not tint the deck against it (a
    /// SwiftShader browser's clouds came out periwinkle).
    pub dual_source_blending: bool,
}

/// The worst main-pass pipeline's sampled textures with no prepass: the
/// view's seven, the atmosphere's transmittance table, and the ground's
/// eight (`StandardMaterial`'s six and its two arrays, `ground.rs`).
pub const FRAGMENT_TEXTURES: u32 = 16;

/// Storage textures Bevy's SSAO wants in one stage.
pub const SSAO_STORAGE_TEXTURES: u32 = 5;

impl GpuCaps {
    /// Nothing constrained: before (or without) a device.
    pub const UNBOUNDED: Self = Self {
        sampled_textures: u32::MAX,
        storage_textures: u32::MAX,
        float32_filterable: true,
        rg11b10_renderable: true,
        dual_source_blending: true,
    };

    /// Prepass textures the main pass can still bind.
    pub fn prepass_room(self) -> u32 {
        self.sampled_textures.saturating_sub(FRAGMENT_TEXTURES)
    }

    /// Whether SSAO fits, alone or beside TAA.
    pub fn ao(self, with_taa: bool) -> bool {
        self.storage_textures >= SSAO_STORAGE_TEXTURES
            && self.float32_filterable
            && self.prepass_room() >= if with_taa { 3 } else { 2 }
    }

    /// Whether TAA fits, alone or beside SSAO.
    pub fn taa(self, with_ao: bool) -> bool {
        self.prepass_room() >= if with_ao { 3 } else { 2 }
    }
}

static CAPS: OnceLock<GpuCaps> = OnceLock::new();

/// The device's caps, or [`GpuCaps::UNBOUNDED`] before there is one.
pub fn caps() -> GpuCaps {
    CAPS.get().copied().unwrap_or(GpuCaps::UNBOUNDED)
}

/// The frame one build on one device draws.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tier {
    /// Bevy's atmosphere draws the sky and the air. Without it the sky is
    /// baked behind the clouds (`sky::BAKE_BACKDROP`) and the air is a fog.
    pub atmosphere: bool,
    /// Occlusion is kept before TAA when only one fits: it is what pays
    /// back the sky fill (Gates' `rig.rs`).
    pub ssao: Option<ScreenSpaceAmbientOcclusionQualityLevel>,
    pub taa: bool,
    /// Where TAA isn't.
    pub smaa: bool,
    pub bloom: bool,
    /// Sun shadow cascades, and how far the last reaches (metres).
    pub cascades: usize,
    pub shadow_m: f32,
}

/// The desktop's frame: four cascades to 200 m.
const HIGH_SHADOW_M: f32 = 200.0;
/// WebGL2's one cascade, to 120 m.
const LOW_SHADOW_M: f32 = 120.0;

/// The frame for an API (`webgl2` or not) on a device with `caps`.
pub fn tier_for(webgl2: bool, caps: GpuCaps) -> Tier {
    if webgl2 {
        return Tier {
            atmosphere: false,
            ssao: None,
            taa: false,
            smaa: true,
            bloom: false,
            cascades: 1,
            shadow_m: LOW_SHADOW_M,
        };
    }
    let ao = caps.ao(false);
    let taa = caps.taa(ao);
    Tier {
        atmosphere: caps.float32_filterable,
        ssao: ao.then_some(ScreenSpaceAmbientOcclusionQualityLevel::Medium),
        taa,
        smaa: !taa,
        bloom: caps.rg11b10_renderable,
        cascades: max_cascades(),
        shadow_m: HIGH_SHADOW_M,
    }
}

/// This build's frame on this device.
pub fn tier() -> Tier {
    tier_for(WEBGL2, caps())
}

impl Tier {
    /// What it draws, for the log and the page (`window.__oneshot.tier`).
    pub fn describe(&self) -> String {
        let mut on: Vec<&str> = Vec::new();
        if self.atmosphere {
            on.push("atmosphere");
        } else {
            on.push("haze");
        }
        if self.ssao.is_some() {
            on.push("ssao");
        }
        if self.taa {
            on.push("taa");
        }
        if self.smaa {
            on.push("smaa");
        }
        if self.bloom {
            on.push("bloom");
        }
        format!("{} {} cascade(s) to {} m", on.join(" "), self.cascades, self.shadow_m)
    }
}

/// Reads the device once it exists: `RenderPlugin::finish` has put it in
/// the main world by our `finish`, which runs before `Startup` spawns the
/// camera (`rig::camera`). Under WebGPU it also hands the device an error
/// hook ([`watch`]).
pub struct GpuPlugin;

impl Plugin for GpuPlugin {
    fn build(&self, _app: &mut App) {}

    fn finish(&self, app: &mut App) {
        use bevy::render::render_resource::WgpuFeatures;
        let Some(device) = app.world().get_resource::<bevy::render::renderer::RenderDevice>() else {
            return;
        };
        let (limits, features) = (device.limits(), device.features());
        let caps = GpuCaps {
            sampled_textures: limits.max_sampled_textures_per_shader_stage,
            storage_textures: limits.max_storage_textures_per_shader_stage,
            float32_filterable: features.contains(WgpuFeatures::FLOAT32_FILTERABLE),
            rg11b10_renderable: features.contains(WgpuFeatures::RG11B10UFLOAT_RENDERABLE),
            dual_source_blending: features.contains(WgpuFeatures::DUAL_SOURCE_BLENDING),
        };
        let _ = CAPS.set(caps);
        info!("oneshot: gpu {} {caps:?}: {}", api(), tier().describe());
        #[cfg(all(target_arch = "wasm32", not(webgl2)))]
        watch(device.wgpu_device());
    }
}

/// The WebGPU module's device hooks. Under WebGL2 a refused pipeline panics
/// inside wgpu and the page sees a dead module; under WebGPU the browser
/// logs a warning (a few, then stops) and the frame that held it isn't
/// drawn, and Bevy installs no handler. This one logs every error and hands
/// it to the page (`window.__oneshotGpuError(message, kind)`, index.html):
/// `"lost"` means nothing will draw again and the page reloads into the
/// WebGL2 module, `"error"` is a refusal the page reports and remembers. A
/// device destroyed on purpose (the tab closing) is neither.
#[cfg(all(target_arch = "wasm32", not(webgl2)))]
fn watch(device: &wgpu::Device) {
    device.on_uncaptured_error(std::sync::Arc::new(|e: wgpu::Error| {
        let msg = e.to_string();
        error!("webgpu: {msg}");
        tell_page("__oneshotGpuError", &msg, "error");
    }));
    device.set_device_lost_callback(|reason, msg| {
        if reason == wgpu::DeviceLostReason::Destroyed {
            return;
        }
        let msg = format!("device lost ({reason:?}): {msg}");
        error!("webgpu: {msg}");
        tell_page("__oneshotGpuError", &msg, "lost");
    });
}

/// `window[hook](message, kind)`, if the page installed it.
#[cfg(target_arch = "wasm32")]
pub fn tell_page(hook: &str, msg: &str, kind: &str) {
    use wasm_bindgen::{JsCast, JsValue};
    let global = js_sys::global();
    let f = js_sys::Reflect::get(&global, &JsValue::from_str(hook))
        .ok()
        .and_then(|v| v.dyn_into::<js_sys::Function>().ok());
    if let Some(f) = f {
        let _ = f.call2(&global, &JsValue::from_str(msg), &JsValue::from_str(kind));
    }
}

/// Every panic to the page too (`window.__oneshotPanic(message)`), after
/// whatever hook Bevy set (its console one): a WebGPU module whose renderer
/// never started (no adapter, a refused device or surface) panics before
/// the device hooks exist, and the page is the only thing left that can
/// load the WebGL2 module instead. Called once the App is built, so Bevy's
/// `PanicHandlerPlugin` has already set its hook and this one wraps it.
#[cfg(target_arch = "wasm32")]
pub fn hook_panics() {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        prev(info);
        tell_page("__oneshotPanic", &info.to_string(), "panic");
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What SwiftShader's WebGPU adapter offers (measured in headless
    /// Chromium 153): the floor.
    const FLOOR: GpuCaps = GpuCaps {
        sampled_textures: 16,
        storage_textures: 4,
        float32_filterable: true,
        rg11b10_renderable: true,
        dual_source_blending: false,
    };

    #[test]
    fn the_desktop_draws_everything() {
        let t = tier_for(false, GpuCaps::UNBOUNDED);
        assert!(t.atmosphere && t.ssao.is_some() && t.taa && t.bloom && !t.smaa);
        assert_eq!((t.cascades, t.shadow_m), (4, HIGH_SHADOW_M));
    }

    #[test]
    fn webgpu_at_the_floor_keeps_the_sky_and_drops_the_prepasses() {
        let t = tier_for(false, FLOOR);
        assert!(t.atmosphere && t.bloom && t.smaa);
        assert!(t.ssao.is_none() && !t.taa);
        assert_eq!(t.cascades, 4);
        // Chrome's usual hardware tier: room for both.
        let hw = GpuCaps {
            sampled_textures: 48,
            storage_textures: 8,
            ..FLOOR
        };
        let t = tier_for(false, hw);
        assert!(t.ssao.is_some() && t.taa && !t.smaa);
        // Room for one prepass pair: occlusion first.
        let one = GpuCaps {
            sampled_textures: FRAGMENT_TEXTURES + 2,
            storage_textures: 8,
            ..FLOOR
        };
        let t = tier_for(false, one);
        assert!(t.ssao.is_some() && !t.taa && t.smaa);
    }

    #[test]
    fn webgl2_is_the_cheap_frame_and_never_sees_further() {
        let low = tier_for(true, GpuCaps::UNBOUNDED);
        let high = tier_for(false, GpuCaps::UNBOUNDED);
        assert!(!low.atmosphere && low.ssao.is_none() && !low.taa && !low.bloom && low.smaa);
        assert_eq!(low.cascades, 1);
        // Shadows can give a player away: the cheap frame casts no further.
        assert!(low.shadow_m <= high.shadow_m);
    }
}

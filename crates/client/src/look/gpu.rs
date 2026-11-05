//! What the GPU under this build can draw. After Gates' `render/quality.rs`
//! (`GpuCaps`) and `render/web.rs` (`watch_gpu`) (MIT, NOTICE). The
//! graphics ladder (`quality.rs`) decides the frame from the player's rows
//! and this; nothing here picks a look.
//!
//! Two layers:
//!
//! - **The API, fixed when the module is compiled** (`cfg(webgl2)`,
//!   build.rs). WebGL2 has no compute and no storage buffers in the fragment
//!   stage, so no atmosphere (its layout wants one: inserted there it kills
//!   the module, though the log says it is merely off), no SSAO or TAA
//!   (their prepasses want the same), no bloom (Bevy: "not compatible with
//!   WebGL2"), and Bevy gives a light one cascade there
//!   (`quality::for_target_on`). The sky is then baked into the cloud cube
//!   and the air is a fog of the same air (`sky::browser_haze`). Nothing in
//!   it is a removal: the WebGL2 frame is the best WebGL2 can do.
//! - **The device, read once at startup** ([`GpuPlugin`]). A WebGPU adapter
//!   guarantees 16 sampled textures per fragment stage and 4 storage
//!   textures; Chrome gives most hardware 48 and 8, SwiftShader the floor.
//!   The worst main-pass pipeline (the ground under the atmosphere) is
//!   [`FRAGMENT_TEXTURES`] with no prepass, and each prepass it reads adds
//!   one, so TAA (depth and motion) and SSAO (depth and normal; five storage
//!   textures; filterable 32-bit floats) go on only where there is room
//!   (`quality::effective_on`). A pipeline over the limit is refused, and
//!   under WebGPU that loses the whole frame, not one mesh. SMAA stands in
//!   where TAA can't go.
//!
//! **Fairness:** what a GPU can't do costs it finish, never what hides a
//! player: the haze is the same air both ways (`sky::browser_haze` is the
//! atmosphere's own coefficients), and `quality.rs`' header says the rest.

use std::sync::OnceLock;

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

/// Whether Bevy's atmosphere draws the sky and the air: never under
/// WebGL2 (its layout wants a storage buffer in the fragment stage, which
/// kills the module), and only where 32-bit floats filter (it binds its
/// `Rgba32Float` tables as filterable without asking). Elsewhere the sky
/// is baked into the cloud cube and the air is a fog of the same air
/// (`sky::browser_haze`).
pub fn atmosphere_on(webgl2: bool, caps: GpuCaps) -> bool {
    !webgl2 && caps.float32_filterable
}

/// [`atmosphere_on`] this build on this device.
pub fn atmosphere() -> bool {
    atmosphere_on(WEBGL2, caps())
}

/// Reads the device once it exists: `RenderPlugin::finish` has put it in
/// the main world by our `finish`, which runs before `Startup` spawns the
/// camera (`rig::camera`). Then the machine may take the graphics lower
/// (`potato::at_boot`) and the tier is resolved against both, before
/// anything is drawn with it. Under WebGPU it also hands the device an
/// error hook ([`watch`]).
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
        #[cfg(all(target_arch = "wasm32", not(webgl2)))]
        watch(device.wgpu_device());
        super::potato::at_boot(app);
        super::quality::refresh(app.world_mut());
        if let Some(t) = app.world().get_resource::<super::quality::Tier>() {
            info!("oneshot: gpu {} {caps:?}: {}", api(), t.describe());
        }
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

/// Every panic to the console and to the page (`window.__oneshotPanic
/// (message)`): a WebGPU module whose renderer never started (no adapter, a
/// refused device or surface) panics before the device hooks exist, and the
/// page is the only thing left that can load the WebGL2 module instead.
/// Bevy's own console hook is only in its `web` feature, which this build
/// leaves out, and std's does nothing on wasm32-unknown-unknown: without
/// this line a fallback would leave no reason anywhere. Called once the App
/// is built, so whatever hook Bevy's `PanicHandlerPlugin` set is wrapped.
#[cfg(target_arch = "wasm32")]
pub fn hook_panics() {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        prev(info);
        let msg = info.to_string();
        web_sys::console::error_1(&wasm_bindgen::JsValue::from_str(&format!("oneshot: panic: {msg}")));
        tell_page("__oneshotPanic", &msg, "panic");
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
    fn the_prepasses_go_where_there_is_room() {
        assert!(GpuCaps::UNBOUNDED.ao(true) && GpuCaps::UNBOUNDED.taa(true));
        assert!(!FLOOR.ao(false) && !FLOOR.taa(false));
        let hw = GpuCaps {
            sampled_textures: 48,
            storage_textures: 8,
            ..FLOOR
        };
        assert!(hw.ao(true) && hw.taa(true));
        let one = GpuCaps {
            sampled_textures: FRAGMENT_TEXTURES + 2,
            storage_textures: 8,
            ..FLOOR
        };
        assert!(one.ao(false) && !one.ao(true) && one.taa(false) && !one.taa(true));
    }

    #[test]
    fn the_atmosphere_is_never_webgl2s() {
        assert!(atmosphere_on(false, FLOOR));
        assert!(!atmosphere_on(true, GpuCaps::UNBOUNDED));
        let no_f32 = GpuCaps {
            float32_filterable: false,
            ..GpuCaps::UNBOUNDED
        };
        assert!(!atmosphere_on(false, no_f32));
    }
}

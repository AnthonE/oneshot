//! POTATO, picked for a player whose machine needs it. After Gates'
//! `crates/client/src/render/potato.rs` (MIT, NOTICE).
//!
//! The rung is a column of `quality::preset`; this is the picking. It
//! happens only while the player hasn't touched a graphics row
//! (`Settings::quality_auto`), and it only ever moves the preset down:
//!
//! - **At boot, the machine** ([`boot_hint`]): a software rasterizer
//!   (SwiftShader, llvmpipe, WARP), a phone or a tablet, a page reporting
//!   2 GB or less, or 4 GB on 4 cores or fewer. Straight to POTATO.
//!   `?potato` in the page's address, or `--potato`, forces it whatever was
//!   chosen.
//! - **In a browser, the frames** ([`watch`]): after a warmup, a median
//!   frame slower than [`SLOW_FPS`] over [`WINDOW_S`] steps the preset down
//!   a rung, or straight to POTATO under [`CRAWL_FPS`]. The median, so a
//!   shader compiling or a level landing isn't a slow machine. It stands
//!   down for the run after [`SETTLED_WINDOWS`] good windows in a row. A
//!   desktop player has the settings and a frame cap; a browser player
//!   often never opens them.
//!
//! Either way a line at the top says what happened and where to undo it,
//! and the rung is kept, so the next start opens on it.
//!
//! Not for a picture: `--shot`, `?pose` and the fx stage draw the frame
//! they were asked for, whatever box they run on (CI's is a software one).
//! `--lite` / `?lite=1` and `--quality` / `?quality=` pick a rung for this
//! run only, and keep the player's own for the file (`Settings::run_on`).

use bevy::prelude::*;
use bevy::render::renderer::RenderAdapterInfo;
use bevy::window::PrimaryWindow;

use super::quality::{for_target, preset, Quality};
use crate::settings::Settings;
use crate::{ui, Screen};

/// Below this many frames a second (the median) the preset steps down.
pub const SLOW_FPS: f32 = 24.0;
/// Below this it goes straight to POTATO.
pub const CRAWL_FPS: f32 = 14.0;
/// Seconds in a match before the first window: shaders compile and the
/// level dresses on arrival, and none of that is the machine's rate.
pub const WARMUP_S: f32 = 12.0;
/// Seconds skipped after a step, for the pipelines it re-specializes.
pub const SETTLE_S: f32 = 6.0;
/// Seconds of frames one verdict is drawn from.
pub const WINDOW_S: f32 = 8.0;
/// Good windows in a row after which the watch stands down for the run.
pub const SETTLED_WINDOWS: u8 = 3;
/// A frame longer than this is a hidden tab or a stall, not a frame rate.
pub const STALL_S: f32 = 0.5;
/// How long the line stays up, then how long it fades.
const NOTE_S: f32 = 7.0;
const NOTE_FADE_S: f32 = 0.6;
/// Under the screenshot line (`look/shot.rs`, 16 px down), over the HUD.
const NOTE_TOP_PX: f32 = 62.0;
const NOTE_Z: i32 = 29;

/// What the boot hint reads about the machine.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Machine {
    /// The adapter is a CPU rasterizer.
    pub software_gpu: bool,
    pub phone: bool,
    /// `navigator.deviceMemory`, GB (Chromium only; rounded down to a power
    /// of two, at most 8).
    pub memory_gb: Option<f32>,
    /// `navigator.hardwareConcurrency`.
    pub cores: Option<u32>,
    /// `?potato` / `--potato`.
    pub forced: bool,
}

/// Why this machine should start on POTATO, if it should.
pub fn boot_hint(m: &Machine) -> Option<&'static str> {
    if m.forced {
        return Some("asked for at launch");
    }
    if m.software_gpu {
        return Some("no graphics card found, drawing on the CPU");
    }
    if m.phone {
        return Some("a phone or tablet");
    }
    let mem = m.memory_gb.unwrap_or(f32::MAX);
    let cores = m.cores.unwrap_or(u32::MAX);
    if mem <= 2.0 || (mem <= 4.0 && cores <= 4) {
        return Some("a low-memory machine");
    }
    None
}

/// Whether an adapter is a software rasterizer: wgpu calls it a `Cpu`
/// device; WebGL2 names it in the renderer string instead.
pub fn is_software(device_type_debug: &str, name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    device_type_debug == "Cpu"
        || [
            "swiftshader",
            "llvmpipe",
            "softpipe",
            "lavapipe",
            "software",
            "basic render",
        ]
        .iter()
        .any(|s| name.contains(s))
}

/// The rung a median frame of `median_s` seconds moves `q` to, or `None`.
pub fn verdict(q: Quality, median_s: f32) -> Option<Quality> {
    if median_s <= 1.0 / SLOW_FPS || q == Quality::Potato {
        return None;
    }
    if median_s > 1.0 / CRAWL_FPS {
        return Some(Quality::Potato);
    }
    Some(q.step(-1))
}

/// The watch's state for the run, and the line owed to the screen.
#[derive(Resource)]
pub struct Watch {
    warm_s: f32,
    frames: Vec<f32>,
    sampled_s: f32,
    good: u8,
    /// What to say, and since when it has been up (real seconds).
    note: Option<String>,
    shown_at: Option<f32>,
}

impl Default for Watch {
    fn default() -> Self {
        Self {
            warm_s: WARMUP_S,
            frames: Vec::with_capacity(1024),
            sampled_s: 0.0,
            good: 0,
            note: None,
            shown_at: None,
        }
    }
}

/// The machine this build runs on, as far as it can tell.
fn machine(adapter: Option<&RenderAdapterInfo>, forced: bool, soft: bool) -> Machine {
    // wgpu's WebGPU backend names no adapter (an empty name, device type
    // Other), so in the WebGPU module the page's word for it counts.
    let software_gpu = soft || adapter.is_some_and(|a| is_software(&format!("{:?}", a.device_type), &a.name));
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsValue;
        let get = |o: &JsValue, k: &str| js_sys::Reflect::get(o, &JsValue::from_str(k)).ok();
        let nav = web_sys::window()
            .and_then(|w| get(&w, "navigator"))
            .unwrap_or(JsValue::UNDEFINED);
        let ua = get(&nav, "userAgent").and_then(|v| v.as_string()).unwrap_or_default();
        let num = |k: &str| get(&nav, k).and_then(|v| v.as_f64()).filter(|v| *v > 0.0);
        Machine {
            software_gpu,
            phone: ["Android", "iPhone", "iPad", "Mobile"].iter().any(|s| ua.contains(s)),
            memory_gb: num("deviceMemory").map(|v| v as f32),
            cores: num("hardwareConcurrency").map(|v| v as u32),
            forced,
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        Machine {
            software_gpu,
            forced,
            ..default()
        }
    }
}

/// Once, from `GpuPlugin::finish`: the device exists, the settings are
/// loaded, and nothing has been drawn with them yet.
pub fn at_boot(app: &mut App) {
    let world = app.world_mut();
    let opts = world.get_resource::<crate::Opts>().copied().unwrap_or_default();
    let picture = opts.pose.is_some() || world.contains_resource::<super::shot::Capture>();
    // A rung for this run only, the player's own kept for the file.
    if let Some(q) = opts.quality.or(opts.lite.then_some(Quality::Potato)) {
        if let Some(mut s) = world.get_resource_mut::<Settings>() {
            s.run_on(q);
            info!("gfx: {} for this run", q.name());
        }
        return;
    }
    if picture {
        return;
    }
    let m = machine(world.get_resource::<RenderAdapterInfo>(), opts.potato, opts.softgpu);
    let Some(why) = boot_hint(&m) else {
        return;
    };
    let Some(mut s) = world.get_resource_mut::<Settings>() else {
        return;
    };
    if s.preset_name() == Some(Quality::Potato) || !(s.quality_auto || m.forced) {
        return;
    }
    s.quality = Quality::Potato;
    s.gfx = for_target(preset(Quality::Potato));
    info!("gfx: POTATO at boot ({why}); {m:?}");
    let note = format!("Set to POTATO: {why}. SETTINGS > GRAPHICS to change it");
    world.init_resource::<Watch>();
    world.resource_mut::<Watch>().note = Some(note);
}

pub fn plugin(app: &mut App) {
    app.init_resource::<Watch>()
        .add_systems(Startup, setup_note)
        .add_systems(Update, (watch, show_note).chain());
}

/// In a browser, in a match, the frame rate: the preset down a rung while
/// it stays too slow. Watched again on every level: a duel on a small map
/// settling it says nothing of a Big Team match on Dry Fork after.
#[allow(clippy::too_many_arguments)]
pub(crate) fn watch(
    time: Res<Time<Real>>,
    screen: Res<State<Screen>>,
    in_match: Option<Res<crate::front::InMatch>>,
    settle: Res<crate::loading::Settle>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut settings: ResMut<Settings>,
    mut w: ResMut<Watch>,
    (built, mut level): (Res<crate::Built>, Local<Option<(u32, u32)>>),
) {
    if *level != built.0 {
        *level = built.0;
        w.good = 0;
        w.frames.clear();
        w.sampled_s = 0.0;
        w.warm_s = WARMUP_S;
    }
    let playing = *screen.get() == Screen::Playing && in_match.is_some_and(|m| m.0) && settle.shown();
    let focused = windows.single().is_ok_and(|w| w.focused);
    if !cfg!(target_arch = "wasm32")
        || !playing
        || !focused
        || !settings.quality_auto
        || settings.quality == Quality::Potato
        || w.good >= SETTLED_WINDOWS
    {
        return;
    }
    let dt = time.delta_secs();
    if w.warm_s > 0.0 {
        w.warm_s -= dt.min(STALL_S);
        return;
    }
    if dt > STALL_S {
        return;
    }
    w.frames.push(dt);
    w.sampled_s += dt;
    if w.sampled_s < WINDOW_S {
        return;
    }
    let w = &mut *w;
    w.frames.sort_unstable_by(f32::total_cmp);
    let median = w.frames[w.frames.len() / 2];
    w.frames.clear();
    w.sampled_s = 0.0;
    let Some(next) = verdict(settings.quality, median) else {
        w.good += 1;
        return;
    };
    w.good = 0;
    w.warm_s = SETTLE_S;
    let fps = (1.0 / median).round();
    info!("gfx: {fps} fps median on {:?}, down to {next:?}", settings.quality);
    settings.quality = next;
    settings.gfx = for_target(preset(next));
    w.note = Some(format!(
        "Lowered to {} ({fps} frames a second). SETTINGS > GRAPHICS to change it",
        next.name().to_uppercase()
    ));
    w.shown_at = None;
}

#[derive(Component)]
struct NoteRoot;
#[derive(Component)]
struct NoteText;

fn setup_note(mut commands: Commands) {
    commands
        .spawn((
            NoteRoot,
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(NOTE_TOP_PX),
                width: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                ..default()
            },
            GlobalZIndex(NOTE_Z),
            Visibility::Hidden,
        ))
        .with_children(|r| {
            r.spawn((
                Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(12.0),
                    padding: UiRect::axes(Val::Px(14.0), Val::Px(8.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    border_radius: BorderRadius::all(Val::Px(2.0)),
                    ..default()
                },
                BackgroundColor(ui::PANEL_BG),
                BorderColor::all(ui::RULE),
            ))
            .with_children(|p| {
                p.spawn(ui::strong("GRAPHICS", 12.0, ui::MARK));
                p.spawn((NoteText, ui::label("", 13.0, ui::TEXT_COLOR)));
            });
        });
}

/// The line, once there is a screen to read it on (`loading::Settle`):
/// up for [`NOTE_S`], then faded out.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn show_note(
    time: Res<Time<Real>>,
    settle: Res<crate::loading::Settle>,
    mut w: ResMut<Watch>,
    mut root: Query<(&mut Visibility, &Children), With<NoteRoot>>,
    mut said: Query<&mut Text, With<NoteText>>,
    mut colours: Query<(Option<&mut BackgroundColor>, Option<&mut BorderColor>)>,
    mut inks: Query<&mut TextColor>,
    kids: Query<&Children>,
) {
    let w = &mut *w;
    let Some(note) = &w.note else {
        return;
    };
    if !settle.shown() {
        return;
    }
    let now = time.elapsed_secs();
    let first = w.shown_at.is_none();
    let at = *w.shown_at.get_or_insert_with(|| {
        for mut t in &mut said {
            t.0.clone_from(note);
        }
        now
    });
    let age = now - at;
    let a = 1.0 - ((age - NOTE_S) / NOTE_FADE_S).clamp(0.0, 1.0);
    if a <= 0.0 {
        w.note = None;
        w.shown_at = None;
    }
    // The colours only move on the first frame (back to whole, after an
    // earlier note's fade) and while fading: change-detected, so not
    // written for nothing the other seven seconds.
    if !first && age <= NOTE_S {
        return;
    }
    for (mut vis, children) in &mut root {
        vis.set_if_neq(if a > 0.0 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
        for pill in children.iter() {
            if let Ok((bg, border)) = colours.get_mut(pill) {
                if let Some(mut bg) = bg {
                    bg.0 = ui::PANEL_BG.with_alpha(ui::PANEL_BG.alpha() * a);
                }
                if let Some(mut b) = border {
                    *b = BorderColor::all(ui::RULE.with_alpha(ui::RULE.alpha() * a));
                }
            }
            for e in kids.iter_descendants(pill) {
                if let Ok(mut c) = inks.get_mut(e) {
                    let base = if said.contains(e) { ui::TEXT_COLOR } else { ui::MARK };
                    c.0 = base.with_alpha(base.alpha() * a);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slow_frame_steps_one_rung_and_a_crawl_goes_to_the_bottom() {
        let at = |fps: f32| 1.0 / fps;
        assert_eq!(verdict(Quality::High, at(60.0)), None);
        assert_eq!(verdict(Quality::High, at(25.0)), None);
        assert_eq!(verdict(Quality::High, at(20.0)), Some(Quality::Medium));
        assert_eq!(verdict(Quality::Medium, at(20.0)), Some(Quality::Low));
        assert_eq!(verdict(Quality::Low, at(20.0)), Some(Quality::Potato));
        assert_eq!(verdict(Quality::High, at(10.0)), Some(Quality::Potato));
        assert_eq!(verdict(Quality::Potato, at(5.0)), None);
    }

    #[test]
    fn the_boot_hint_reads_the_weak_machines_and_leaves_the_rest() {
        let ok = Machine {
            memory_gb: Some(8.0),
            cores: Some(8),
            ..default()
        };
        assert_eq!(boot_hint(&ok), None);
        assert_eq!(boot_hint(&Machine::default()), None, "unknown is fine");
        for weak in [
            Machine {
                software_gpu: true,
                ..ok
            },
            Machine { phone: true, ..ok },
            Machine {
                memory_gb: Some(2.0),
                ..ok
            },
            Machine {
                memory_gb: Some(4.0),
                cores: Some(4),
                ..ok
            },
            Machine { forced: true, ..ok },
        ] {
            assert!(boot_hint(&weak).is_some(), "{weak:?}");
        }
        let four_on_eight = Machine {
            memory_gb: Some(4.0),
            ..ok
        };
        assert_eq!(boot_hint(&four_on_eight), None, "4 GB on 8 cores is not a potato");
    }

    #[test]
    fn software_adapters_are_named_either_way() {
        assert!(is_software("Cpu", "llvmpipe (LLVM 15.0.7, 256 bits)"));
        assert!(is_software(
            "Other",
            "ANGLE (Google, Vulkan 1.3.0 (SwiftShader Device (Subzero)), SwiftShader driver)"
        ));
        assert!(!is_software("IntegratedGpu", "Intel(R) UHD Graphics 620"));
        assert!(!is_software("DiscreteGpu", "NVIDIA GeForce RTX 3060"));
    }
}

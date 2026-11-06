//! The player's settings: kept between runs, after Gates'
//! `render/settings.rs` (its store, load and save, and its graphics half;
//! MIT). The same `key = value` text (`config.rs`) goes to a file on the
//! desktop and to `localStorage["oneshot.settings"]` in a browser. Loaded
//! as the app is built; written whenever a setting or a bind changes, and
//! only then.
//!
//! **What comes off disk is clamped where it is read**, by the same bounds
//! the steppers keep (`clamped`, `gfx_from_file`): a hand-edited
//! `fov = 500` loads as 120, and a file can't reach a state the screen
//! can't. The graphics half is the preset, then whatever rows the file
//! overrode, so a file written before the rows existed opens on its preset.

use bevy::prelude::*;

use crate::binds::Binds;
use crate::config;
use crate::look::quality::{
    self, for_target, max_cascades, max_shadow_map_px, preset, step_ladder, Aa, Ao, Clouds, Effects, Gfx, Quality,
    SHADOW_M_LADDER, SHADOW_PX_LADDER,
};
use crate::look::render_scale;

/// Degrees a mouse count turns at sensitivity 1: Source's, so CS2's and
/// Apex's numbers mean the same here (Valorant's times 3.18, Overwatch's
/// over 3.33). Scoped, divided by the scope's zoom (`lib.rs`).
pub const SENS_DEG: f32 = 0.022;
/// What a count turned at 1 before (settings version 1): a file from then
/// is read times `SENS_V1_DEG / SENS_DEG`, so a hand keeps its speed.
const SENS_V1_DEG: f32 = 0.0022 * 180.0 / core::f32::consts::PI;
/// 0.066° a count: Valorant's default, between CS2's and Overwatch's.
pub const SENS_DEFAULT: f32 = 3.0;
/// Down to where a 3200-dpi mouse turns once in a metre; up to a trackpad.
pub const SENS_MIN: f32 = 0.1;
pub const SENS_MAX: f32 = 20.0;
/// Its step: a hundredth below `SENS_FINE`, a twentieth up to
/// `SENS_COARSE`, a quarter above, each a share of it a hand feels.
pub const SENS_STEP_FINE: f32 = 0.01;
pub const SENS_FINE: f32 = 1.0;
pub const SENS_STEP: f32 = 0.05;
pub const SENS_COARSE: f32 = 5.0;
pub const SENS_STEP_COARSE: f32 = 0.25;
/// The field of view, degrees across a 16:9 screen (`look::rig::FOV_DEG`).
pub const FOV_MIN: f32 = 70.0;
pub const FOV_MAX: f32 = 120.0;
pub const FOV_STEP: f32 = 1.0;
pub const VOL_STEP: f32 = 0.1;
/// The browser's key.
pub const BROWSER_KEY: &str = "oneshot.settings";

/// The frame caps the FRAME CAP row offers, with 0 last meaning none: a
/// panel's rate or a clean share of one, not a stepper of single frames
/// (Gates' `MAX_FPS_LADDER`).
pub const MAX_FPS_LADDER: [u16; 6] = [30, 60, 120, 144, 240, 0];
/// The lowest cap a file may hold: below it the game reads as hung.
pub const MIN_FPS_CAP: u16 = MAX_FPS_LADDER[0];

#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    /// Times [`SENS_DEG`] a count.
    pub sensitivity: f32,
    /// Mouse up looks down.
    pub invert: bool,
    /// The field of view, degrees across a 16:9 screen, unscoped (a scope
    /// divides it); drawn at its height on any screen (`rig::fov_tall`).
    pub fov: f32,
    /// Every sound, and the game's own (shots, steps, hits), 0 to 1.
    pub master: f32,
    pub effects: f32,
    /// The preset last picked: what the QUALITY row steps from while the
    /// rows are CUSTOM, and the column a file's missing rows come from.
    /// What's drawn is [`Self::gfx`].
    pub quality: Quality,
    /// The game may still pick the preset for the player
    /// (`look/potato.rs`): until they touch a graphics row.
    pub quality_auto: bool,
    /// The graphics rows as chosen, through this target's clamps
    /// (`quality::for_target`); the device's are applied where it's drawn.
    pub gfx: Gfx,
    /// Wait for the display (the desktop; a browser always does).
    pub vsync: bool,
    /// Frames a second at most, 0 for none (the desktop; a browser paces
    /// itself to the display).
    pub max_fps: u16,
    /// The desktop's borderless fullscreen; in a browser the page's own,
    /// which can only start on a click, so it isn't kept between runs.
    pub fullscreen: bool,
    /// This run draws a rung the player didn't pick (`--lite`,
    /// `?quality=`): their own, kept for the file until they touch a
    /// graphics row (`run_on`).
    pub held: Option<Held>,
}

/// The player's own graphics while a run draws another rung.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Held {
    pub quality: Quality,
    pub auto: bool,
    pub gfx: Gfx,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            sensitivity: SENS_DEFAULT,
            invert: false,
            fov: crate::look::rig::FOV_DEG,
            master: 1.0,
            effects: 1.0,
            quality: quality::default_quality(),
            quality_auto: true,
            gfx: quality::default_gfx(),
            vsync: true,
            // Uncapped (Gates, operator: "client can go faster than 60 we
            // cant stop that"): the cap is a tool, not a policy.
            max_fps: 0,
            // Windowed, as the desktop always opened: a default that moved
            // would be a change nobody chose. F11 or the row.
            fullscreen: false,
            held: None,
        }
    }
}

/// Who plays: the name others see, the skin asked for (worn only if the
/// wallet owns it; the server checks), the emblem on its back and what it
/// wears on its head. Set on the title screen, kept with the settings.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct Player {
    pub name: String,
    pub skin: u8,
    pub emblem: proto::Emblem,
    pub accessory: u8,
}

/// How long a name may be (proto's `Name`).
pub const NAME_MAX: usize = 16;

impl Player {
    /// What it asks to wear.
    pub fn look(&self) -> proto::Look {
        proto::Look {
            skin: self.skin,
            emblem: self.emblem,
            accessory: self.accessory,
        }
    }

    /// The name as sent: what was typed, or `player`.
    pub fn sent_name(&self) -> String {
        let n = self.name.trim();
        if n.is_empty() {
            "player".into()
        } else {
            n.into()
        }
    }

    fn pairs(&self) -> Vec<(String, String)> {
        vec![
            ("name".into(), format!("{:?}", self.name)),
            ("skin".into(), self.skin.to_string()),
            ("accessory".into(), self.accessory.to_string()),
            ("emblem".into(), self.emblem.to_string()),
        ]
    }

    /// A name as it may be kept: printable, no quotes or `#` (the file's
    /// comment), at most [`NAME_MAX`].
    pub fn clean(name: &str) -> String {
        name.chars()
            .filter(|c| (' '..='~').contains(c) && *c != '"' && *c != '\\' && *c != '#')
            .take(NAME_MAX)
            .collect()
    }
}

/// A knob the settings screen turns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Knob {
    Sensitivity,
    Invert,
    Fov,
    Master,
    Effects,
    /// The preset: writes every graphics row below it.
    Quality,
    ShadowDistance,
    Cascades,
    ShadowMap,
    Ambient,
    Aa,
    Bloom,
    RenderScale,
    /// The EFFECTS row (sparks and flash lights), not the game's sounds.
    Fx,
    /// How the cloud deck is drawn (`look/deck.rs`).
    Clouds,
    Fullscreen,
    Vsync,
    FrameCap,
}

impl Knob {
    /// The graphics rows: the preset and everything it writes. Touching
    /// one is the player choosing, and the game stops choosing for them.
    pub fn is_graphics(self) -> bool {
        matches!(
            self,
            Knob::Quality
                | Knob::ShadowDistance
                | Knob::Cascades
                | Knob::ShadowMap
                | Knob::Ambient
                | Knob::Aa
                | Knob::Bloom
                | Knob::RenderScale
                | Knob::Fx
                | Knob::Clouds
        )
    }
}

fn step(v: f32, s: f32) -> f32 {
    (v / s).round() * s
}

/// The sensitivity's step just above `v`.
fn sens_step(v: f32) -> f32 {
    if v < SENS_FINE - 1e-4 {
        SENS_STEP_FINE
    } else if v < SENS_COARSE - 1e-4 {
        SENS_STEP
    } else {
        SENS_STEP_COARSE
    }
}

/// `v` on the sensitivity's steps, within its bounds.
fn sens_snap(v: f32) -> f32 {
    step(v, sens_step(v)).clamp(SENS_MIN, SENS_MAX)
}

/// One click of the FRAME CAP row, stopping at its ends: uncapped is the
/// far end, never one click below 30. A value off the ladder (a 165 Hz
/// panel's, by hand) steps from the rung at or above it.
pub fn step_fps(v: u16, delta: i32) -> u16 {
    let last = MAX_FPS_LADDER.len() - 1;
    let at = MAX_FPS_LADDER
        .iter()
        .position(|&f| f == v)
        .unwrap_or_else(|| MAX_FPS_LADDER.iter().position(|&f| f >= v && f != 0).unwrap_or(last));
    MAX_FPS_LADDER[(at as i32 + delta.signum()).clamp(0, last as i32) as usize]
}

/// The rung of the shadow map ladder nearest `px`, held to what this
/// target can allocate: off a power of two the shade's edges crawl.
fn nearest_px(px: usize) -> usize {
    SHADOW_PX_LADDER
        .into_iter()
        .min_by_key(|rung| rung.abs_diff(px))
        .unwrap_or(1024)
        .min(max_shadow_map_px())
}

/// What a file said about each graphics row: `None` is a key it didn't
/// carry, not a row that is off.
#[derive(Clone, Copy, Debug, Default)]
struct GfxFile {
    shadow_m: Option<f32>,
    cascades: Option<usize>,
    shadow_map_px: Option<usize>,
    ao: Option<Ao>,
    aa: Option<Aa>,
    bloom: Option<bool>,
    render_scale: Option<u8>,
    effects: Option<Effects>,
    clouds: Option<Clouds>,
}

/// The preset, then what the file overrode, sanitized beside the steppers:
/// the distances clamped to their ladders' ends but honoured between (a
/// hand-typed 100 m is a distance someone could mean), the map size snapped
/// to a rung, the cascades held to what the target draws.
fn gfx_from_file(q: Quality, f: GfxFile) -> Gfx {
    let base = for_target(preset(q));
    let span = |ladder: &[f32], v: f32| v.clamp(ladder[0], ladder[ladder.len() - 1]);
    for_target(Gfx {
        shadow_m: span(&SHADOW_M_LADDER, f.shadow_m.unwrap_or(base.shadow_m)),
        cascades: f.cascades.unwrap_or(base.cascades).clamp(1, max_cascades()),
        shadow_map_px: nearest_px(f.shadow_map_px.unwrap_or(base.shadow_map_px)),
        ao: f.ao.unwrap_or(base.ao),
        aa: f.aa.unwrap_or(base.aa),
        bloom: f.bloom.unwrap_or(base.bloom),
        render_scale: render_scale::sanitize(f.render_scale.unwrap_or(base.render_scale)),
        effects: f.effects.unwrap_or(base.effects),
        clouds: f.clouds.unwrap_or(base.clouds),
    })
}

fn on_off(b: bool) -> String {
    if b { "ON" } else { "OFF" }.into()
}

impl Settings {
    /// What a sound plays at, times its own gain.
    pub fn volume(&self) -> f32 {
        self.master * self.effects
    }

    /// The defaults with one preset's rows.
    pub fn with_preset(q: Quality) -> Self {
        Self {
            quality: q,
            gfx: for_target(preset(q)),
            ..Self::default()
        }
    }

    /// The preset the graphics rows spell exactly, if any; `None` is
    /// CUSTOM. Derived, never flagged: a row moved off HIGH and back is
    /// HIGH again with nothing to reset.
    pub fn preset_name(&self) -> Option<Quality> {
        Quality::LADDER
            .iter()
            .copied()
            .find(|q| for_target(preset(*q)) == self.gfx)
    }

    /// This run draws `q`, and the player's own graphics are kept for the
    /// file (`--lite`, `--quality`): the game picks nothing more this run.
    pub fn run_on(&mut self, q: Quality) {
        self.held.get_or_insert(Held {
            quality: self.quality,
            auto: self.quality_auto,
            gfx: self.gfx,
        });
        self.quality = q;
        self.quality_auto = false;
        self.gfx = for_target(preset(q));
    }

    /// The graphics tab's defaults, the run's pick let go.
    pub fn reset_graphics(&mut self) {
        let d = Self::default();
        self.quality = d.quality;
        self.quality_auto = d.quality_auto;
        self.gfx = d.gfx;
        self.vsync = d.vsync;
        self.max_fps = d.max_fps;
        self.fullscreen = d.fullscreen;
        self.held = None;
    }

    /// Within bounds and on the steps, whatever came in (the view and the
    /// sound; the graphics rows are kept on their ladders by `adjust` and
    /// `gfx_from_file`).
    pub fn clamped(self) -> Self {
        let fin = |v: f32, d: f32| if v.is_finite() { v } else { d };
        let d = Self::default();
        Self {
            sensitivity: sens_snap(fin(self.sensitivity, d.sensitivity)),
            fov: step(fin(self.fov, d.fov), FOV_STEP).clamp(FOV_MIN, FOV_MAX),
            master: step(fin(self.master, d.master), VOL_STEP).clamp(0.0, 1.0),
            effects: step(fin(self.effects, d.effects), VOL_STEP).clamp(0.0, 1.0),
            ..self
        }
    }

    /// Turns `knob` by `delta` steps (a toggle flips on any).
    pub fn adjust(&mut self, knob: Knob, delta: i32) {
        let d = delta as f32;
        if knob.is_graphics() {
            self.quality_auto = false;
            self.held = None;
        }
        match knob {
            Knob::Sensitivity => {
                // A click at a time: each from where it is, on its side of a
                // tier's edge.
                for _ in 0..delta.unsigned_abs() {
                    let at = self.sensitivity;
                    let s = sens_step(if delta > 0 { at } else { at - 1e-3 });
                    self.sensitivity = sens_snap(at + s * delta.signum() as f32);
                }
            }
            Knob::Invert => self.invert = !self.invert,
            Knob::Fov => self.fov += d * FOV_STEP,
            Knob::Master => self.master += d * VOL_STEP,
            Knob::Effects => self.effects += d * VOL_STEP,
            // From the preset the rows spell if they spell one, so a moved
            // row and then + lands on the rung above the one being changed.
            Knob::Quality => {
                let at = self.preset_name().unwrap_or(self.quality);
                self.quality = at.step(delta);
                self.gfx = for_target(preset(self.quality));
            }
            _ => {
                let g = &mut self.gfx;
                match knob {
                    Knob::ShadowDistance => g.shadow_m = step_ladder(&SHADOW_M_LADDER, g.shadow_m, delta),
                    Knob::Cascades => {
                        g.cascades = (g.cascades as i32 + delta.signum()).clamp(1, max_cascades() as i32) as usize;
                    }
                    Knob::ShadowMap => {
                        g.shadow_map_px =
                            step_ladder(&SHADOW_PX_LADDER, g.shadow_map_px, delta).min(max_shadow_map_px());
                    }
                    Knob::Ambient => g.ao = g.ao.step(delta),
                    Knob::Aa => g.aa = g.aa.step(delta),
                    Knob::Bloom => g.bloom = !g.bloom,
                    Knob::RenderScale => g.render_scale = render_scale::step(g.render_scale, delta),
                    Knob::Fx => g.effects = g.effects.step(delta),
                    Knob::Clouds => g.clouds = g.clouds.step(delta),
                    Knob::Fullscreen => self.fullscreen = !self.fullscreen,
                    Knob::Vsync => self.vsync = !self.vsync,
                    Knob::FrameCap => self.max_fps = step_fps(self.max_fps, delta),
                    _ => {}
                }
            }
        }
        self.gfx = for_target(self.gfx);
        *self = self.clamped();
    }

    /// What the screen shows for `knob`.
    pub fn value(&self, knob: Knob) -> String {
        let g = &self.gfx;
        match knob {
            Knob::Sensitivity => format!("{:.2}", self.sensitivity),
            Knob::Invert => on_off(self.invert),
            Knob::Fov => format!("{:.0}°", self.fov),
            Knob::Master => format!("{:.0}%", self.master * 100.0),
            Knob::Effects => format!("{:.0}%", self.effects * 100.0),
            Knob::Quality => self.preset_name().map_or("CUSTOM", Quality::name).to_uppercase(),
            Knob::ShadowDistance => format!("{:.0} m", g.shadow_m),
            Knob::Cascades => g.cascades.to_string(),
            Knob::ShadowMap => g.shadow_map_px.to_string(),
            Knob::Ambient => g.ao.name().to_uppercase(),
            Knob::Aa => g.aa.name().to_uppercase(),
            Knob::Bloom => on_off(g.bloom),
            Knob::RenderScale => format!("{}%", g.render_scale),
            Knob::Fx => g.effects.name().to_uppercase(),
            Knob::Clouds => g.clouds.name().to_uppercase(),
            Knob::Fullscreen => on_off(self.fullscreen),
            Knob::Vsync => on_off(self.vsync),
            Knob::FrameCap if self.max_fps == 0 => "NONE".into(),
            Knob::FrameCap => self.max_fps.to_string(),
        }
    }

    fn pairs(&self) -> Vec<(String, String)> {
        // A run on a rung the player didn't pick writes theirs.
        let (q, auto, g) = match self.held {
            Some(h) => (h.quality, h.auto, h.gfx),
            None => (self.quality, self.quality_auto, self.gfx),
        };
        let mut out = vec![
            ("sensitivity", format!("{:.2}", self.sensitivity)),
            ("invert", self.invert.to_string()),
            ("fov", format!("{:.0}", self.fov)),
            ("master", format!("{:.1}", self.master)),
            ("effects", format!("{:.1}", self.effects)),
            ("quality_auto", auto.to_string()),
        ];
        // While the game still picks, its own default isn't the player's
        // pick: unwritten, each module opens on its own (the browser's two
        // share one store). A rung it took lower for the machine is kept.
        if !(auto && q == quality::default_quality() && g == for_target(preset(q))) {
            out.push(("quality", q.name().into()));
        }
        // A preset is written as its name alone, so each target reads it as
        // its own column: the browser's two modules share one store, and
        // WebGL2's clamped rows read back by the WebGPU module would be a
        // CUSTOM nobody chose. Rows moved off it are written out.
        if g != for_target(preset(q)) {
            out.extend([
                ("shadow_m", format!("{}", g.shadow_m)),
                ("shadow_cascades", g.cascades.to_string()),
                ("shadow_map_px", g.shadow_map_px.to_string()),
                ("ao", g.ao.name().into()),
                ("aa", g.aa.name().into()),
                ("bloom", g.bloom.to_string()),
                ("render_scale", g.render_scale.to_string()),
                ("fx", g.effects.name().into()),
                ("clouds", g.clouds.name().into()),
            ]);
        }
        out.extend([("vsync", self.vsync.to_string()), ("max_fps", self.max_fps.to_string())]);
        // A page can't open in fullscreen: in a browser it isn't kept.
        if !cfg!(target_arch = "wasm32") {
            out.push(("fullscreen", self.fullscreen.to_string()));
        }
        out.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
    }

    /// From a file's pairs over the defaults, and the pairs it didn't know.
    /// A known key with a value that doesn't read keeps its default.
    /// From a file's pairs; one of `version` 1 named the sensitivity on
    /// the old base and the field of view by its height.
    fn from_pairs(pairs: &[(String, String)], version: u32) -> (Self, Vec<(String, String)>) {
        let v1 = version < 2;
        let mut s = Self::default();
        let mut f = GfxFile::default();
        let mut other = Vec::new();
        for (k, v) in pairs {
            let num = v.parse::<f32>().ok().filter(|n| n.is_finite());
            let whole = v.parse::<u64>().ok();
            let flag = match v.as_str() {
                "true" => Some(true),
                "false" => Some(false),
                _ => None,
            };
            match k.as_str() {
                "sensitivity" => {
                    s.sensitivity = num.map_or(s.sensitivity, |n| if v1 { n * SENS_V1_DEG / SENS_DEG } else { n })
                }
                "fov" => s.fov = num.map_or(s.fov, |n| if v1 { crate::look::rig::fov_across(n) } else { n }),
                "master" => s.master = num.unwrap_or(s.master),
                "effects" => s.effects = num.unwrap_or(s.effects),
                "invert" => s.invert = v == "true",
                "quality" => s.quality = Quality::from_name(v).unwrap_or(s.quality),
                "quality_auto" => s.quality_auto = flag.unwrap_or(s.quality_auto),
                "shadow_m" => f.shadow_m = num,
                "shadow_cascades" => f.cascades = whole.map(|n| n.min(64) as usize),
                "shadow_map_px" => f.shadow_map_px = whole.map(|n| n.min(1 << 20) as usize),
                "ao" => f.ao = Ao::from_name(v),
                "aa" => f.aa = Aa::from_name(v),
                "bloom" => f.bloom = flag,
                "render_scale" => f.render_scale = whole.map(|n| n.min(255) as u8),
                // The GRASS rows' keys, from before the ground cover was
                // the same on every rung: dropped.
                "cover" | "cover_m" => {}
                "fx" => f.effects = Effects::from_name(v),
                "clouds" => f.clouds = Clouds::from_name(v),
                "vsync" => s.vsync = flag.unwrap_or(s.vsync),
                // Floored, not snapped: a 165 Hz panel's cap is a rate
                // someone could mean; under the floor the game reads hung.
                "max_fps" => {
                    s.max_fps = match whole {
                        Some(0) => 0,
                        Some(n) => (n.min(u64::from(u16::MAX)) as u16).max(MIN_FPS_CAP),
                        None => s.max_fps,
                    }
                }
                "fullscreen" => s.fullscreen = flag.unwrap_or(s.fullscreen) && !cfg!(target_arch = "wasm32"),
                _ => other.push((k.clone(), v.clone())),
            }
        }
        s.gfx = gfx_from_file(s.quality, f);
        (s.clamped(), other)
    }
}

/// Where the settings go.
#[derive(Clone, Debug)]
pub enum Store {
    File(std::path::PathBuf),
    Browser,
}

/// What was read, kept for the write: the store, the file's version, what
/// it held that this build doesn't know, and the text last written.
#[derive(Resource)]
pub struct Disk {
    store: Store,
    version: u32,
    other: Vec<(String, String)>,
    other_binds: Vec<(String, String)>,
    written: String,
}

impl Disk {
    /// Where the settings are kept, for the settings screen's foot.
    pub fn describe(&self) -> String {
        match &self.store {
            Store::File(p) => format!("saved to {}", p.display()),
            Store::Browser => "saved in this browser".into(),
        }
    }
}

/// The store this build writes to, and what it holds now.
fn open_store() -> Option<(Store, Option<String>)> {
    #[cfg(target_arch = "wasm32")]
    {
        let ls = web_sys::window()?.local_storage().ok()??;
        Some((Store::Browser, ls.get_item(BROWSER_KEY).ok().flatten()))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let path = config::settings_path()?;
        let text = config::read(&path);
        Some((Store::File(path), text))
    }
}

fn write_store(store: &Store, text: &str) -> Result<(), String> {
    match store {
        Store::File(p) => config::save(p, text),
        Store::Browser => {
            #[cfg(target_arch = "wasm32")]
            {
                let ls = web_sys::window()
                    .and_then(|w| w.local_storage().ok().flatten())
                    .ok_or("no localStorage")?;
                ls.set_item(BROWSER_KEY, text)
                    .map_err(|_| "localStorage refused".to_string())
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                let _ = text;
                Err("no browser here".into())
            }
        }
    }
}

/// `key = value` pairs this build doesn't know, kept for the write.
pub type Lines = Vec<(String, String)>;

/// The settings and binds from `text` (or the defaults), the file's
/// version, and the pairs and binds this build doesn't know.
pub fn read(text: Option<&str>) -> (Settings, Binds, Player, u32, Lines, Lines) {
    let doc = config::parse(text.unwrap_or(""));
    let (s, mut other) = Settings::from_pairs(&doc.pairs, doc.version);
    let mut p = Player::default();
    other.retain(|(k, v)| match k.as_str() {
        "name" => {
            p.name = Player::clean(v.trim_matches('"'));
            false
        }
        "skin" => {
            p.skin = v
                .parse()
                .ok()
                .filter(|&n| (n as usize) < proto::SKINS.len())
                .unwrap_or(0);
            false
        }
        "emblem" => {
            p.emblem = v.parse().map(proto::emblem_valid).unwrap_or(proto::EMBLEM_NONE);
            false
        }
        "accessory" => {
            p.accessory = v.parse().map(proto::accessory_valid).unwrap_or(proto::ACCESSORY_NONE);
            false
        }
        _ => true,
    });
    let (b, other_binds) = Binds::from_lines(&doc.binds);
    (s, b, p, doc.version, other, other_binds)
}

/// The text to write for `s` and `b`, keeping what this build doesn't know.
pub fn text(
    s: &Settings,
    b: &Binds,
    p: &Player,
    version: u32,
    other: &[(String, String)],
    other_binds: &[(String, String)],
) -> String {
    let mut pairs = s.pairs();
    pairs.extend(p.pairs());
    pairs.extend(other.iter().cloned());
    let mut binds = b.to_lines();
    binds.extend(other_binds.iter().cloned());
    config::serialize(version, &pairs, &binds)
}

/// The settings into the app as it is built: from the store, or the
/// defaults with nowhere to save (a browser with storage off, a desktop
/// with no home).
pub fn plugin(app: &mut App) {
    let (store, held) = open_store().map_or((None, None), |(s, t)| (Some(s), t));
    let (s, b, p, version, other, other_binds) = read(held.as_deref());
    // What's held, as this build would write it: nothing is written until
    // something changes.
    let written = text(&s, &b, &p, version, &other, &other_binds);
    app.insert_resource(s).insert_resource(b).insert_resource(p);
    if let Some(store) = store {
        app.insert_resource(Disk {
            store,
            version,
            other,
            other_binds,
            written,
        });
    }
    app.add_systems(Last, save_on_change);
}

/// Writes the settings when they changed (and what's written would
/// differ); a failed write is said once and tried again on the next change.
fn save_on_change(settings: Res<Settings>, binds: Res<Binds>, player: Res<Player>, disk: Option<ResMut<Disk>>) {
    let Some(mut disk) = disk else {
        return;
    };
    if !settings.is_changed() && !binds.is_changed() && !player.is_changed() {
        return;
    }
    let t = text(&settings, &binds, &player, disk.version, &disk.other, &disk.other_binds);
    if t == disk.written {
        return;
    }
    match write_store(&disk.store, &t) {
        Ok(()) => disk.written = t,
        Err(e) => warn!("settings not saved: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binds::{Action, Bind};

    #[test]
    fn what_is_read_is_clamped_to_its_steps() {
        let (s, ..) = read(Some(
            "sensitivity = 99\nfov = 12\nmaster = 0.33\neffects = nan\ninvert = true\n",
        ));
        assert_eq!(s.sensitivity, SENS_MAX);
        assert_eq!(s.fov, FOV_MIN);
        assert!((s.master - 0.3).abs() < 1e-6);
        assert_eq!(s.effects, Settings::default().effects);
        assert!(s.invert);
    }

    #[test]
    fn sensitivity_steps_finely_low_and_coarsely_high() {
        let mut s = Settings {
            sensitivity: 1.0,
            ..Settings::default()
        };
        let at = |s: &Settings| (s.sensitivity * 100.0).round() as i32;
        s.adjust(Knob::Sensitivity, -1);
        assert_eq!(at(&s), 99);
        s.adjust(Knob::Sensitivity, 1);
        assert_eq!(at(&s), 100);
        s.adjust(Knob::Sensitivity, 1);
        assert_eq!(at(&s), 105);
        s.sensitivity = 5.0;
        s.adjust(Knob::Sensitivity, -1);
        assert_eq!(at(&s), 495);
        s.adjust(Knob::Sensitivity, 2);
        assert_eq!(at(&s), 525);
        s.adjust(Knob::Sensitivity, -1000);
        assert_eq!(s.sensitivity, SENS_MIN);
        let (r, ..) = read(Some("sensitivity = 0.63\n"));
        assert_eq!(at(&r), 63);
        let (r, ..) = read(Some("sensitivity = 2.62\n"));
        assert_eq!(at(&r), 260);
    }

    /// A file from before the base moved turns a hand as fast as it did,
    /// and shows as wide a view, now named across.
    #[test]
    fn a_version_1_file_keeps_its_speed_and_its_view() {
        let (s, ..) = read(Some("version = 1\nsensitivity = 0.5\nfov = 75\n"));
        assert_eq!((s.sensitivity * 100.0).round(), 285.0, "0.063° a count, as it was");
        assert_eq!(s.fov, 108.0, "75° tall is 107.5° across");
        let (s, ..) = read(Some("version = 1\nsensitivity = 3\nfov = 110\n"));
        assert_eq!((s.sensitivity, s.fov), (17.25, FOV_MAX));
        let (s, ..) = read(Some("version = 2\nsensitivity = 0.5\nfov = 75\n"));
        assert_eq!((s.sensitivity, s.fov), (0.5, 75.0));
        let t = text(&s, &Binds::default(), &Player::default(), 1, &[], &[]);
        assert!(t.contains("version = 2"), "written, it says what it means");
    }

    #[test]
    fn settings_and_binds_round_trip_keeping_what_this_build_does_not_know() {
        let held = "version = 3\nfov = 95\nfuture_thing = 4\nbind.jump = \"KeyB\"\nbind.dance = \"KeyZ\"\n";
        let (mut s, mut b, mut p, v, other, other_binds) = read(Some(held));
        assert_eq!(s.fov, 95.0);
        assert_eq!(b.slots(Action::Jump)[0], Some(Bind::Key(KeyCode::KeyB)));
        s.adjust(Knob::Sensitivity, 3);
        b.set(Action::Reload, 1, Some(Bind::Mouse(MouseButton::Back)));
        p.name = Player::clean("Master \"Chief\" of the long name");
        p.skin = 3;
        p.emblem = proto::emblem(14, 4, 6, 12);
        p.accessory = 1;
        let t = text(&s, &b, &p, v, &other, &other_binds);
        assert!(t.contains("version = 3") && t.contains("future_thing = 4") && t.contains("bind.dance = \"KeyZ\""));
        let (s2, b2, p2, ..) = read(Some(&t));
        assert_eq!((s2, b2, &p2), (s, b, &p));
        assert_eq!(p2.name, "Master Chief of ");
    }

    /// What a hand-edited file says is read inside the steppers' bounds,
    /// and a file from before the graphics rows opens on its preset.
    #[test]
    fn the_graphics_half_is_clamped_where_it_is_read() {
        let (s, ..) = read(Some(
            "quality = low\nshadow_m = 9000\nshadow_cascades = 9\nshadow_map_px = 3000\nrender_scale = 13\n\
             cover_m = 1\nao = sideways\nmax_fps = 1\nvsync = maybe\n",
        ));
        let low = for_target(preset(Quality::Low));
        assert_eq!(s.quality, Quality::Low);
        assert_eq!(s.gfx.shadow_m, SHADOW_M_LADDER[SHADOW_M_LADDER.len() - 1]);
        assert_eq!(s.gfx.cascades, max_cascades());
        assert_eq!(s.gfx.shadow_map_px, 2048, "snapped to the nearest rung");
        assert_eq!(s.gfx.render_scale, render_scale::RENDER_SCALE_MIN);
        assert_eq!(s.gfx.ao, low.ao, "a name nobody can read keeps the preset's");
        assert_eq!(s.max_fps, MIN_FPS_CAP);
        assert!(s.vsync);
        assert_eq!(s.preset_name(), None, "rows off the preset read CUSTOM");

        let (old, ..) = read(Some("quality = medium\nfov = 90\n"));
        assert_eq!(old.gfx, for_target(preset(Quality::Medium)));
        assert_eq!(old.preset_name(), Some(Quality::Medium));
        let (fresh, ..) = read(None);
        assert_eq!(fresh, Settings::default());
        assert_eq!(fresh.preset_name(), Some(quality::default_quality()));
        // Between two rungs is honoured.
        let (between, ..) = read(Some("quality = high\nshadow_m = 100\n"));
        assert_eq!(between.gfx.shadow_m, 100.0);
    }

    #[test]
    fn the_graphics_rows_round_trip() {
        let mut s = Settings::with_preset(Quality::High);
        s.adjust(Knob::ShadowDistance, -1);
        s.adjust(Knob::Clouds, -1);
        s.adjust(Knob::RenderScale, -3);
        s.adjust(Knob::FrameCap, 2);
        s.adjust(Knob::Vsync, 0);
        let t = text(&s, &Binds::default(), &Player::default(), 1, &[], &[]);
        let (back, ..) = read(Some(&t));
        assert_eq!(back, s);
        assert!(!back.quality_auto, "touching a row is the player choosing");
    }

    /// A preset is kept as its name, so the other module reads its own
    /// column of it; moved rows are kept as rows.
    #[test]
    fn a_preset_is_written_as_its_name() {
        let s = Settings::with_preset(Quality::Medium);
        let t = text(&s, &Binds::default(), &Player::default(), 1, &[], &[]);
        assert!(t.contains("quality = medium") && !t.contains("shadow_m"), "{t}");
        let d = text(&Settings::default(), &Binds::default(), &Player::default(), 1, &[], &[]);
        assert!(
            !d.contains("quality ="),
            "the target's own default is left to the target: {d}"
        );
        let (back, ..) = read(Some(&d));
        assert_eq!(back, Settings::default());
        let mut c = s;
        c.adjust(Knob::Bloom, 0);
        let t = text(&c, &Binds::default(), &Player::default(), 1, &[], &[]);
        assert!(t.contains("bloom = false") && t.contains("shadow_m"), "{t}");
    }

    /// The preset row writes every row below it; a moved row reads CUSTOM
    /// and moving it back doesn't; QUALITY steps one rung a click and stops.
    #[test]
    fn a_moved_row_reads_custom_and_moving_it_back_does_not() {
        for &q in Quality::LADDER {
            let mut s = Settings::with_preset(q);
            assert_eq!(s.preset_name(), Some(q));
            s.adjust(Knob::ShadowDistance, 1);
            if s.gfx.shadow_m == for_target(preset(q)).shadow_m {
                continue; // already at the top rung
            }
            assert_eq!(s.preset_name(), None, "{q:?}");
            s.adjust(Knob::ShadowDistance, -1);
            assert_eq!(s.preset_name(), Some(q), "{q:?}");
        }
        let mut s = Settings::with_preset(Quality::High);
        s.adjust(Knob::Bloom, 0);
        s.adjust(Knob::Quality, -1);
        assert_eq!(s.gfx, for_target(preset(Quality::Medium)));
        for _ in 0..10 {
            let was = s.quality;
            s.adjust(Knob::Quality, -1);
            let moved = Quality::LADDER.iter().position(|x| *x == was).unwrap() as i32
                - Quality::LADDER.iter().position(|x| *x == s.quality).unwrap() as i32;
            assert!(moved == 0 || moved == 1, "one rung a click");
        }
        assert_eq!(s.quality, Quality::Potato, "and it rests at the end");
        s.adjust(Knob::Quality, 1);
        assert_eq!(s.quality, Quality::Low);
    }

    /// No stepper leaves its ladder, whichever way it's walked.
    #[test]
    fn no_stepper_can_walk_off_its_ladder() {
        for dir in [-1, 1] {
            let mut s = Settings::with_preset(Quality::High);
            for _ in 0..12 {
                for k in [
                    Knob::ShadowDistance,
                    Knob::Cascades,
                    Knob::ShadowMap,
                    Knob::Ambient,
                    Knob::Aa,
                    Knob::RenderScale,
                    Knob::Fx,
                    Knob::Clouds,
                    Knob::FrameCap,
                ] {
                    s.adjust(k, dir);
                }
                let g = s.gfx;
                assert!(SHADOW_M_LADDER.contains(&g.shadow_m));
                assert!(SHADOW_PX_LADDER.contains(&g.shadow_map_px) && g.shadow_map_px <= max_shadow_map_px());
                assert!(g.cascades >= 1 && g.cascades <= max_cascades());
                assert!(MAX_FPS_LADDER.contains(&s.max_fps));
                assert_eq!(g.render_scale % render_scale::RENDER_SCALE_STEP, 0);
                let _ =
                    quality::Tier::resolve(g, None, false, crate::look::gpu::GpuCaps::UNBOUNDED, Default::default())
                        .cascades();
            }
        }
        assert_eq!(step_fps(0, 1), 0, "uncapped is the end");
        assert_eq!(step_fps(30, -1), 30);
        assert_eq!(step_fps(165, 1), 0, "off the ladder steps from the rung above");
    }

    /// `--lite`: this run draws POTATO, and the file keeps the player's
    /// own until they touch a graphics row.
    #[test]
    fn a_run_on_another_rung_writes_the_players_own() {
        let mut s = Settings::with_preset(Quality::High);
        let theirs = text(&s, &Binds::default(), &Player::default(), 1, &[], &[]);
        s.run_on(Quality::Potato);
        assert_eq!(s.gfx, for_target(preset(Quality::Potato)));
        assert!(!s.quality_auto, "nothing more is picked this run");
        assert_eq!(text(&s, &Binds::default(), &Player::default(), 1, &[], &[]), theirs);
        s.adjust(Knob::Fx, 1);
        assert!(s.held.is_none(), "the player chose");
        let (back, ..) = read(Some(&text(&s, &Binds::default(), &Player::default(), 1, &[], &[])));
        assert_eq!(back.gfx, s.gfx);
    }

    #[test]
    fn a_knob_turns_by_its_step_and_stops_at_its_ends() {
        let mut s = Settings::default();
        s.adjust(Knob::Fov, 1);
        assert_eq!(s.fov, crate::look::rig::FOV_DEG + 1.0);
        for _ in 0..50 {
            s.adjust(Knob::Fov, 1);
        }
        assert_eq!(s.fov, FOV_MAX);
        s.adjust(Knob::Master, -20);
        assert_eq!(s.master, 0.0);
        s.adjust(Knob::Invert, 0);
        assert!(s.invert);
    }
}

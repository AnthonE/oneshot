//! The player's settings: kept between runs, after Gates'
//! `render/settings.rs` (its store, load and save; MIT). The same
//! `key = value` text (`config.rs`) goes to a file on the desktop and to
//! `localStorage["oneshot.settings"]` in a browser. Loaded as the app is
//! built; written whenever a setting or a bind changes, and only then.

use bevy::prelude::*;

use crate::binds::Binds;
use crate::config;

pub const SENS_MIN: f32 = 0.25;
pub const SENS_MAX: f32 = 3.0;
pub const SENS_STEP: f32 = 0.05;
pub const FOV_MIN: f32 = 60.0;
pub const FOV_MAX: f32 = 110.0;
pub const FOV_STEP: f32 = 5.0;
pub const VOL_STEP: f32 = 0.1;
/// The browser's key.
pub const BROWSER_KEY: &str = "oneshot.settings";

#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    /// Times the base mouse speed (`lib.rs` `SENS`).
    pub sensitivity: f32,
    /// Mouse up looks down.
    pub invert: bool,
    /// The field of view, degrees, unscoped (a scope divides it).
    pub fov: f32,
    /// Every sound, and the game's own (shots, steps, hits), 0 to 1.
    pub master: f32,
    pub effects: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            sensitivity: 1.0,
            invert: false,
            fov: crate::look::rig::FOV_DEG,
            master: 1.0,
            effects: 1.0,
        }
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
}

fn step(v: f32, s: f32) -> f32 {
    (v / s).round() * s
}

impl Settings {
    /// What a sound plays at, times its own gain.
    pub fn volume(&self) -> f32 {
        self.master * self.effects
    }

    /// Within bounds and on the steps, whatever came in.
    pub fn clamped(self) -> Self {
        let fin = |v: f32, d: f32| if v.is_finite() { v } else { d };
        let d = Self::default();
        Self {
            sensitivity: step(fin(self.sensitivity, d.sensitivity), SENS_STEP).clamp(SENS_MIN, SENS_MAX),
            invert: self.invert,
            fov: step(fin(self.fov, d.fov), FOV_STEP).clamp(FOV_MIN, FOV_MAX),
            master: step(fin(self.master, d.master), VOL_STEP).clamp(0.0, 1.0),
            effects: step(fin(self.effects, d.effects), VOL_STEP).clamp(0.0, 1.0),
        }
    }

    /// Turns `knob` by `delta` steps (a toggle flips on any).
    pub fn adjust(&mut self, knob: Knob, delta: i32) {
        let d = delta as f32;
        match knob {
            Knob::Sensitivity => self.sensitivity += d * SENS_STEP,
            Knob::Invert => self.invert = !self.invert,
            Knob::Fov => self.fov += d * FOV_STEP,
            Knob::Master => self.master += d * VOL_STEP,
            Knob::Effects => self.effects += d * VOL_STEP,
        }
        *self = self.clamped();
    }

    /// What the screen shows for `knob`.
    pub fn value(&self, knob: Knob) -> String {
        match knob {
            Knob::Sensitivity => format!("{:.2}", self.sensitivity),
            Knob::Invert => if self.invert { "ON" } else { "OFF" }.into(),
            Knob::Fov => format!("{:.0}°", self.fov),
            Knob::Master => format!("{:.0}%", self.master * 100.0),
            Knob::Effects => format!("{:.0}%", self.effects * 100.0),
        }
    }

    fn pairs(&self) -> Vec<(String, String)> {
        [
            ("sensitivity", format!("{:.2}", self.sensitivity)),
            ("invert", self.invert.to_string()),
            ("fov", format!("{:.0}", self.fov)),
            ("master", format!("{:.1}", self.master)),
            ("effects", format!("{:.1}", self.effects)),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect()
    }

    /// From a file's pairs over the defaults, and the pairs it didn't know.
    /// A known key with a value that doesn't read keeps its default.
    fn from_pairs(pairs: &[(String, String)]) -> (Self, Vec<(String, String)>) {
        let mut s = Self::default();
        let mut other = Vec::new();
        for (k, v) in pairs {
            let num = v.parse::<f32>().ok().filter(|n| n.is_finite());
            match (k.as_str(), num) {
                ("sensitivity", Some(n)) => s.sensitivity = n,
                ("fov", Some(n)) => s.fov = n,
                ("master", Some(n)) => s.master = n,
                ("effects", Some(n)) => s.effects = n,
                ("invert", _) => s.invert = v == "true",
                ("sensitivity" | "fov" | "master" | "effects", None) => {}
                _ => other.push((k.clone(), v.clone())),
            }
        }
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
    /// The settings and keys as last loaded or written: what this process
    /// changed is what differs from these.
    base: (Settings, Binds),
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
pub fn read(text: Option<&str>) -> (Settings, Binds, u32, Lines, Lines) {
    let doc = config::parse(text.unwrap_or(""));
    let (s, other) = Settings::from_pairs(&doc.pairs);
    let (b, other_binds) = Binds::from_lines(&doc.binds);
    (s, b, doc.version, other, other_binds)
}

/// The text to write for `s` and `b`, keeping what this build doesn't know
/// (a kept bind line goes once its action is rebound here).
pub fn text(
    s: &Settings,
    b: &Binds,
    version: u32,
    other: &[(String, String)],
    other_binds: &[(String, String)],
) -> String {
    let mut pairs = s.pairs();
    pairs.extend(other.iter().cloned());
    let mut binds = b.to_lines();
    let ours: Vec<String> = binds.iter().map(|(a, _)| a.clone()).collect();
    binds.extend(other_binds.iter().filter(|(a, _)| !ours.contains(a)).cloned());
    config::serialize(version, &pairs, &binds)
}

/// What this process changed since `base` (what it loaded or last wrote),
/// put over what the store holds now: another tab or client's changes to
/// other settings and keys survive this one's write.
fn merge(base: (&Settings, &Binds), ours: (&Settings, &Binds), now: (&Settings, &Binds)) -> (Settings, Binds) {
    let pick = |b: f32, o: f32, n: f32| if o != b { o } else { n };
    let s = Settings {
        sensitivity: pick(base.0.sensitivity, ours.0.sensitivity, now.0.sensitivity),
        invert: if ours.0.invert != base.0.invert {
            ours.0.invert
        } else {
            now.0.invert
        },
        fov: pick(base.0.fov, ours.0.fov, now.0.fov),
        master: pick(base.0.master, ours.0.master, now.0.master),
        effects: pick(base.0.effects, ours.0.effects, now.0.effects),
    };
    let mut b = now.1.clone();
    for spec in &crate::binds::SPECS {
        let a = spec.action;
        if ours.1.slots(a) != base.1.slots(a) {
            for (i, slot) in ours.1.slots(a).into_iter().enumerate() {
                b.set(a, i, slot);
            }
        }
    }
    (s, b)
}

/// The settings into the app as it is built: from the store, or the
/// defaults with nowhere to save (a browser with storage off, a desktop
/// with no home).
pub fn plugin(app: &mut App) {
    let (store, held) = open_store().map_or((None, None), |(s, t)| (Some(s), t));
    let (s, b, version, other, other_binds) = read(held.as_deref());
    // What's held, as this build would write it: nothing is written until
    // something changes.
    let written = text(&s, &b, version, &other, &other_binds);
    let base = (s, b.clone());
    app.insert_resource(s).insert_resource(b);
    if let Some(store) = store {
        app.insert_resource(Disk {
            store,
            version,
            other,
            other_binds,
            written,
            base,
        });
    }
    app.add_systems(Last, save_on_change);
}

/// Writes the settings when they changed (and what's written would
/// differ), over what the store holds now; a failed write is said once and
/// tried again on the next change.
fn save_on_change(mut settings: ResMut<Settings>, mut binds: ResMut<Binds>, disk: Option<ResMut<Disk>>) {
    let Some(mut disk) = disk else {
        return;
    };
    if !settings.is_changed() && !binds.is_changed() {
        return;
    }
    let held = open_store().and_then(|(_, t)| t);
    let (now_s, now_b, version, other, other_binds) = read(held.as_deref());
    let (s, b) = merge((&disk.base.0, &disk.base.1), (&settings, &binds), (&now_s, &now_b));
    let version = version.max(disk.version);
    let t = text(&s, &b, version, &other, &other_binds);
    disk.version = version;
    disk.other = other;
    disk.other_binds = other_binds;
    if t != disk.written && t != held.unwrap_or_default() {
        if let Err(e) = write_store(&disk.store, &t) {
            warn!("settings not saved: {e}");
            return;
        }
    }
    disk.written = t;
    disk.base = (s, b.clone());
    settings.set_if_neq(s);
    binds.set_if_neq(b);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binds::{Action, Bind};

    #[test]
    fn what_is_read_is_clamped_to_its_steps() {
        let (s, ..) = read(Some(
            "sensitivity = 9\nfov = 12\nmaster = 0.33\neffects = nan\ninvert = true\n",
        ));
        assert_eq!(s.sensitivity, SENS_MAX);
        assert_eq!(s.fov, FOV_MIN);
        assert!((s.master - 0.3).abs() < 1e-6);
        assert_eq!(s.effects, Settings::default().effects);
        assert!(s.invert);
    }

    #[test]
    fn settings_and_binds_round_trip_keeping_what_this_build_does_not_know() {
        let held = "version = 3\nfov = 95\nfuture_thing = 4\nbind.jump = \"KeyB\"\nbind.dance = \"KeyZ\"\n";
        let (mut s, mut b, v, other, other_binds) = read(Some(held));
        assert_eq!(s.fov, 95.0);
        assert_eq!(b.slots(Action::Jump)[0], Some(Bind::Key(KeyCode::KeyB)));
        s.adjust(Knob::Sensitivity, 3);
        b.set(Action::Reload, 1, Some(Bind::Mouse(MouseButton::Back)));
        let t = text(&s, &b, v, &other, &other_binds);
        assert!(t.contains("version = 3") && t.contains("future_thing = 4") && t.contains("bind.dance = \"KeyZ\""));
        let (s2, b2, ..) = read(Some(&t));
        assert_eq!((s2, b2), (s, b));
    }

    #[test]
    fn two_clients_keep_each_others_changes() {
        // Both loaded the defaults; one widened the view and wrote; the other
        // turned the volume down and writes now: both changes stay.
        let base = (Settings::default(), Binds::default());
        let mut other_tab = base.0;
        other_tab.adjust(Knob::Fov, 4);
        let mut ours = base.0;
        ours.adjust(Knob::Master, -3);
        let mut our_keys = Binds::default();
        our_keys.set(Action::Jump, 0, Some(Bind::Key(KeyCode::KeyB)));
        let (s, b) = merge((&base.0, &base.1), (&ours, &our_keys), (&other_tab, &Binds::default()));
        assert_eq!(s.fov, other_tab.fov);
        assert_eq!(s.master, ours.master);
        assert_eq!(b.slots(Action::Jump)[0], Some(Bind::Key(KeyCode::KeyB)));
    }

    #[test]
    fn a_newer_builds_bind_is_kept_until_rebound_here() {
        let held = "bind.jump = \"Mouse6 Space\"\n";
        let (s, mut b, v, other, other_binds) = read(Some(held));
        assert!(text(&s, &b, v, &other, &other_binds).contains("bind.jump = \"Mouse6 Space\""));
        b.set(Action::Jump, 0, Some(Bind::Key(KeyCode::KeyJ)));
        let t = text(&s, &b, v, &other, &other_binds);
        assert!(t.contains("bind.jump = \"KeyJ none\"") && !t.contains("Mouse6"), "{t}");
    }

    #[test]
    fn a_knob_turns_by_its_step_and_stops_at_its_ends() {
        let mut s = Settings::default();
        s.adjust(Knob::Fov, 1);
        assert_eq!(s.fov, 80.0);
        for _ in 0..20 {
            s.adjust(Knob::Fov, 1);
        }
        assert_eq!(s.fov, FOV_MAX);
        s.adjust(Knob::Master, -20);
        assert_eq!(s.master, 0.0);
        s.adjust(Knob::Invert, 0);
        assert!(s.invert);
    }
}

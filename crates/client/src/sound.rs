//! Sound, after the three.js page's `art.ts`'s `Sfx`: shots placed where they were
//! fired (each gun its own takes, a far one past 35 m, our own unplaced),
//! hits and headshots, blasts, reloads, the swap, a dry trigger, steps
//! and landings. Gates' CC0 cuts (`assets/sound`, NOTICE).
//!
//! Two halves. What to play is decided every frame on every build
//! ([`decide`]): a list of [`Cue`]s off the frame's events and our own
//! fighter. Playing them live is Bevy's audio, always in a browser and on
//! the desktop with the `sound` feature (it needs ALSA's headers on Linux);
//! a film mixes the same cues itself, on its own frame clock
//! (`film/mix.rs`), so a clip sounds the way the match did.

use bevy::prelude::*;
use sim::events::Kind;

use crate::look::fx::HEARD;
use crate::look::guns::GUNS;
use crate::LinkRes;

/// This frame's events, for the ear (`happened` in lib.rs fills it).
#[derive(Resource, Default)]
pub struct Heard(pub Vec<proto::EventState>);

/// Past this, a shot is its far take.
pub const FAR_M: f32 = 35.0;
/// Metres to the audio engine's units: its fall-off is the inverse
/// square of distance, which at 1:1 would mute a shot 10 m off.
pub const SCALE: f32 = 0.15;
pub const MASTER: f32 = 0.6;

/// One sound to start this frame: which cut (`assets/sound/<name>.ogg`),
/// where (none: ours, unplaced), how loud before the player's volume, and
/// how fast (a little pitch).
#[derive(Clone, Debug, PartialEq)]
pub struct Cue {
    pub name: String,
    pub at: Option<Vec3>,
    pub gain: f32,
    pub speed: f32,
}

/// This frame's cues, for whoever plays them.
#[derive(Resource, Default)]
pub struct Cues(pub Vec<Cue>);

/// Every cut a cue may name.
pub fn names() -> Vec<String> {
    let mut names: Vec<String> = [
        "dry_fire_0",
        "headshot_0",
        "headshot_1",
        "hit_0",
        "equip_0",
        "blast_0",
        "blast_1",
        "flesh_0",
        "flesh_1",
        "land_0",
        "step_concrete_0",
        "step_concrete_1",
        "step_concrete_2",
        "step_concrete_3",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    for (_, g) in &GUNS {
        for k in 0..g.takes {
            names.push(format!("{}_{k}", g.sound));
        }
        names.push(format!("{}_0", g.far));
        names.push(g.reload.to_string());
    }
    names
}

/// What the ear remembers between frames: our own body's last state, and
/// the take dice.
#[derive(Resource)]
pub struct Ear {
    rng: u32,
    last_reload: bool,
    last_arm: u8,
    last_grounded: bool,
    last_mag: u16,
    step: f32,
    dry: f32,
}

impl Default for Ear {
    fn default() -> Self {
        Ear {
            rng: 0x9e37_79b9,
            last_reload: false,
            last_arm: 0,
            last_grounded: false,
            last_mag: 0,
            step: 0.0,
            dry: 0.0,
        }
    }
}

impl Ear {
    fn roll(&mut self, n: u8) -> u8 {
        // Cosmetic only: which take, and a little pitch.
        self.rng = self.rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        ((self.rng >> 16) % n.max(1) as u32) as u8
    }
    fn pitch(&mut self) -> f32 {
        0.96 + self.roll(9) as f32 * 0.01
    }
    fn cue(&mut self, cues: &mut Cues, name: &str, at: Option<Vec3>, gain: f32) {
        let speed = self.pitch();
        cues.0.push(Cue {
            name: name.to_string(),
            at,
            gain,
            speed,
        });
    }
}

pub struct SoundPlugin;

impl Plugin for SoundPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Heard>()
            .init_resource::<Ear>()
            .init_resource::<Cues>()
            .add_systems(Update, decide.after(crate::happened));
        #[cfg(any(target_arch = "wasm32", feature = "sound"))]
        on::build(app);
    }
}

/// This frame's cues: the events heard, then our own body.
pub fn decide(
    link: NonSend<LinkRes>,
    heard: Res<Heard>,
    time: Res<Time>,
    mut ear: ResMut<Ear>,
    mut cues: ResMut<Cues>,
    cam: Query<&GlobalTransform, With<crate::look::rig::EyeCam>>,
) {
    cues.0.clear();
    let s = link.0.session();
    if !s.ready {
        return;
    }
    let me = s.core.slot;
    let eye = cam.iter().next().map_or(Vec3::ZERO, |g| g.translation());
    let ear = &mut *ear;
    let cues = &mut *cues;
    for e in &heard.0 {
        let at = Vec3::new(e.pos.x, e.pos.y, e.pos.z);
        if e.kind == Kind::Shot as u8 {
            let Some(g) = crate::look::guns::gun(e.c as u8) else {
                continue;
            };
            if e.a == me && e.part != HEARD {
                let k = ear.roll(g.takes);
                ear.cue(cues, &format!("{}_{k}", g.sound), None, 0.55);
            } else if at.distance(eye) > FAR_M {
                ear.cue(cues, &format!("{}_0", g.far), Some(at), 0.9);
            } else {
                let k = ear.roll(g.takes);
                ear.cue(cues, &format!("{}_{k}", g.sound), Some(at), 0.9);
            }
        } else if e.kind == Kind::Hit as u8 {
            if e.a == me && e.b != me {
                let n = if e.part == 2 {
                    format!("headshot_{}", ear.roll(2))
                } else {
                    "hit_0".into()
                };
                ear.cue(cues, &n, None, 0.5);
            }
            if e.b == me {
                let n = format!("flesh_{}", ear.roll(2));
                ear.cue(cues, &n, None, 0.6);
            }
        } else if e.kind == Kind::Blast as u8 || e.kind == Kind::Wreck as u8 {
            let n = format!("blast_{}", ear.roll(2));
            ear.cue(cues, &n, Some(at), 1.2);
        }
    }
    // Our own body: reloads, the swap, a dry trigger, steps, landing.
    let (f, _) = s.core.own();
    let g = f.gun();
    let gun = crate::look::guns::gun(g.arm);
    let dt = time.delta_secs();
    let reloading = f.reload > 0;
    if reloading && !ear.last_reload {
        let n = gun.map_or("reload_rifle_0", |g| g.reload);
        ear.cue(cues, n, None, 0.7);
    }
    ear.last_reload = reloading;
    if g.arm != ear.last_arm {
        ear.cue(cues, "equip_0", None, 0.5);
        ear.last_arm = g.arm;
    }
    ear.dry -= dt;
    let firing = f.last_buttons & sim::input::FIRE != 0;
    if f.alive && g.mag == 0 && ear.last_mag == 0 && firing && ear.dry <= 0.0 {
        ear.cue(cues, "dry_fire_0", None, 0.6);
        ear.dry = 0.3;
    }
    ear.last_mag = g.mag;
    let v = f.body.vel;
    let speed = (v.x * v.x + v.z * v.z).sqrt();
    let grounded = f.body.grounded && f.riding().is_none();
    if f.alive && grounded && speed > 2.5 {
        ear.step -= dt * speed;
        if ear.step <= 0.0 {
            let n = format!("step_concrete_{}", ear.roll(4));
            ear.cue(cues, &n, None, 0.25);
            ear.step = 2.6;
        }
    }
    if grounded && !ear.last_grounded {
        ear.cue(cues, "land_0", None, 0.35);
    }
    ear.last_grounded = grounded;
}

/// The cues through Bevy's audio, live. A film mixes its own and plays
/// none of them (`film.rs`).
#[cfg(any(target_arch = "wasm32", feature = "sound"))]
pub mod on {
    use std::collections::BTreeMap;

    use bevy::audio::{DefaultSpatialScale, SpatialScale, Volume};
    use bevy::prelude::*;

    use super::{Cues, MASTER, SCALE};

    /// The cuts, by name.
    #[derive(Resource, Default)]
    pub struct Clips(pub BTreeMap<String, Handle<AudioSource>>);

    pub fn build(app: &mut App) {
        app.init_resource::<Clips>()
            .insert_resource(DefaultSpatialScale(SpatialScale::new(SCALE)))
            .add_systems(Startup, load)
            .add_systems(
                Update,
                (listen, play)
                    .chain()
                    .after(super::decide)
                    .run_if(not(resource_exists::<crate::film::Film>)),
            );
    }

    fn load(mut clips: ResMut<Clips>, assets: Res<AssetServer>) {
        for n in super::names() {
            let h = assets.load(format!("sound/{n}.ogg"));
            clips.0.insert(n, h);
        }
    }

    /// The listener rides the camera.
    fn listen(mut commands: Commands, eye: Query<Entity, (With<crate::look::rig::EyeCam>, Without<SpatialListener>)>) {
        for e in &eye {
            commands.entity(e).insert(SpatialListener::new(0.2));
        }
    }

    fn play(cues: Res<Cues>, clips: Res<Clips>, mut commands: Commands, settings: Res<crate::settings::Settings>) {
        let volume = settings.volume();
        for c in &cues.0 {
            let Some(h) = clips.0.get(&c.name).cloned() else {
                continue;
            };
            let s = PlaybackSettings::DESPAWN
                .with_volume(Volume::Linear(c.gain * MASTER * volume))
                .with_speed(c.speed);
            match c.at {
                Some(p) => {
                    commands.spawn((
                        AudioPlayer::new(h),
                        s.with_spatial(true),
                        Transform::from_translation(p),
                    ));
                }
                None => {
                    commands.spawn((AudioPlayer::new(h), s));
                }
            }
        }
    }
}

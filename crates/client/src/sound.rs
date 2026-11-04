//! Sound, after the three.js page's `art.ts`'s `Sfx`: shots placed where they were
//! fired (each gun its own takes, a far one past 35 m, our own unplaced),
//! hits and headshots, blasts, reloads, the swap, a dry trigger, steps
//! and landings. Gates' CC0 cuts (`assets/sound`, NOTICE).
//!
//! Always in a browser; on the desktop with the `sound` feature (it needs
//! ALSA's headers on Linux). Without it the plugin does nothing.

use bevy::prelude::*;

/// This frame's events, for the ear (`happened` in lib.rs fills it).
#[derive(Resource, Default)]
pub struct Heard(pub Vec<proto::EventState>);

pub struct SoundPlugin;

#[cfg(not(any(target_arch = "wasm32", feature = "sound")))]
impl Plugin for SoundPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Heard>();
    }
}

#[cfg(any(target_arch = "wasm32", feature = "sound"))]
pub use on::*;

#[cfg(any(target_arch = "wasm32", feature = "sound"))]
mod on {
    use std::collections::BTreeMap;

    use bevy::audio::{DefaultSpatialScale, SpatialScale, Volume};
    use bevy::prelude::*;
    use sim::events::Kind;

    use super::{Heard, SoundPlugin};
    use crate::look::fx::HEARD;
    use crate::look::guns::GUNS;
    use crate::LinkRes;

    /// Past this, a shot is its far take.
    const FAR_M: f32 = 35.0;
    /// Metres to the audio engine's units: its fall-off is the inverse
    /// square of distance, which at 1:1 would mute a shot 10 m off.
    const SCALE: f32 = 0.15;
    const MASTER: f32 = 0.6;

    impl Plugin for SoundPlugin {
        fn build(&self, app: &mut App) {
            app.init_resource::<Heard>()
                .init_resource::<Ear>()
                .insert_resource(DefaultSpatialScale(SpatialScale::new(SCALE)))
                .add_systems(Startup, load)
                .add_systems(Update, (listen, play).chain().after(crate::happened));
        }
    }

    #[derive(Resource, Default)]
    pub struct Ear {
        clips: BTreeMap<String, Handle<AudioSource>>,
        rng: u32,
        /// The player's volume (`settings.rs`), this frame.
        volume: f32,
        last_reload: bool,
        last_arm: u8,
        last_grounded: bool,
        last_mag: u16,
        step: f32,
        dry: f32,
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
    }

    fn load(mut ear: ResMut<Ear>, assets: Res<AssetServer>) {
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
        for n in names {
            let h = assets.load(format!("sound/{n}.ogg"));
            ear.clips.insert(n, h);
        }
        ear.rng = 0x9e37_79b9;
    }

    /// The listener rides the camera.
    fn listen(mut commands: Commands, eye: Query<Entity, (With<Camera3d>, Without<SpatialListener>)>) {
        for e in &eye {
            commands.entity(e).insert(SpatialListener::new(0.2));
        }
    }

    fn sfx(commands: &mut Commands, ear: &mut Ear, name: &str, at: Option<Vec3>, gain: f32) {
        let Some(h) = ear.clips.get(name).cloned() else {
            return;
        };
        let speed = ear.pitch();
        let s = PlaybackSettings::DESPAWN
            .with_volume(Volume::Linear(gain * MASTER * ear.volume))
            .with_speed(speed);
        match at {
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

    fn play(
        link: NonSend<LinkRes>,
        heard: Res<Heard>,
        time: Res<Time>,
        mut ear: ResMut<Ear>,
        mut commands: Commands,
        cam: Query<&GlobalTransform, With<Camera3d>>,
        settings: Res<crate::settings::Settings>,
    ) {
        ear.volume = settings.volume();
        let s = link.0.session();
        if !s.ready {
            return;
        }
        let me = s.core.slot;
        let eye = cam.iter().next().map_or(Vec3::ZERO, |g| g.translation());
        for e in &heard.0 {
            let at = Vec3::new(e.pos.x, e.pos.y, e.pos.z);
            if e.kind == Kind::Shot as u8 {
                let Some(g) = crate::look::guns::gun(e.c as u8) else {
                    continue;
                };
                if e.a == me && e.part != HEARD {
                    let k = ear.roll(g.takes);
                    sfx(&mut commands, &mut ear, &format!("{}_{k}", g.sound), None, 0.55);
                } else if at.distance(eye) > FAR_M {
                    sfx(&mut commands, &mut ear, &format!("{}_0", g.far), Some(at), 0.9);
                } else {
                    let k = ear.roll(g.takes);
                    sfx(&mut commands, &mut ear, &format!("{}_{k}", g.sound), Some(at), 0.9);
                }
            } else if e.kind == Kind::Hit as u8 {
                if e.a == me && e.b != me {
                    let n = if e.part == 2 {
                        format!("headshot_{}", ear.roll(2))
                    } else {
                        "hit_0".into()
                    };
                    sfx(&mut commands, &mut ear, &n, None, 0.5);
                }
                if e.b == me {
                    let n = format!("flesh_{}", ear.roll(2));
                    sfx(&mut commands, &mut ear, &n, None, 0.6);
                }
            } else if e.kind == Kind::Blast as u8 || e.kind == Kind::Wreck as u8 {
                let n = format!("blast_{}", ear.roll(2));
                sfx(&mut commands, &mut ear, &n, Some(at), 1.2);
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
            sfx(&mut commands, &mut ear, n, None, 0.7);
        }
        ear.last_reload = reloading;
        if g.arm != ear.last_arm {
            sfx(&mut commands, &mut ear, "equip_0", None, 0.5);
            ear.last_arm = g.arm;
        }
        ear.dry -= dt;
        let firing = f.last_buttons & sim::input::FIRE != 0;
        if f.alive && g.mag == 0 && ear.last_mag == 0 && firing && ear.dry <= 0.0 {
            sfx(&mut commands, &mut ear, "dry_fire_0", None, 0.6);
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
                sfx(&mut commands, &mut ear, &n, None, 0.25);
                ear.step = 2.6;
            }
        }
        if grounded && !ear.last_grounded {
            sfx(&mut commands, &mut ear, "land_0", None, 0.35);
        }
        ear.last_grounded = grounded;
    }
}

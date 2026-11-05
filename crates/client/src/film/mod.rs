//! Film mode, the trailer camera and the clip's video: a tape (`client_net::
//! tape`) drawn on a fixed frame clock with a scripted camera, each shot
//! written as it is drawn. After Gates' `render/film.rs` (MIT).
//!
//! `oneshot --film shots.json` (`ci/film.sh shoot`) draws a recorded match
//! (`oneshot --record`, `ci/film.sh record`) as the list says ([`script`]),
//! each shot piped into its own `ffmpeg` with the game's mix beside it as a
//! WAV ([`encode`], [`mix`]). It exists because a box with no GPU draws on
//! lavapipe, a frame a second or so: footage of a live match would be a
//! slideshow, a film's is smooth. `oneshot --clip x.tape` is a film of the
//! clip key's tape (`clip.rs`) in one of its styles, joined into one video.
//! **In a page** the same film draws a player's clip on their own GPU in a
//! second canvas (`film()` in lib.rs, `web/clip.js`): its frames and mix go
//! to the page, which encodes them with WebCodecs.
//!
//! Three clocks, and keeping them apart is the whole design:
//!
//!   · **the frame clock**: a frame is `1/fps` of screen time however long
//!     it took to draw (`TimeUpdateStrategy::ManualDuration`, [`install`]);
//!   · **the world clock**: `Time<Virtual>` at the shot's `speed`, what the
//!     tape, the netcode's interpolation and every effect advance by;
//!   · **the shot clock**: screen seconds into the shot, which the camera
//!     path is keyed on, so a time-lapse still pans at an even pace.
//!
//! **The client still only draws.** The tape is what this client was sent,
//! fed to a watching session (`link::Tape`) through the transports' own
//! calls, so the world on screen is exactly what the live client drew, its
//! fog included: anyone but the player is there only while the player could
//! have seen them. The one thing a film writes is the eye. A pov shot is the
//! player's: their aim (off their snapshots), the gun in view, the scope,
//! the chase camera at the wheel, the death cam. Any other shot draws the
//! player's own body and hides the gun in view.
//!
//! The lens is a cinema camera's where the GPU has it: a shutter (motion
//! blur, on TAA's motion prepass, so only where the tier has TAA: the
//! desktop and WebGPU with prepass room), a focus and an aperture (depth of
//! field, which reads the view's depth: not WebGL2; Bevy's gaussian under
//! WebGPU, where its bokeh doesn't work), colour fringing, a hand on the
//! camera and a grade. WebGL2 has neither the prepass nor the depth to
//! bind, so a WebGL2 film is graded, fringed, shaken and rolled but not
//! blurred or focused: that is what WebGL2 can do, said here rather than
//! refused.

pub mod encode;
pub mod mix;
pub mod script;

use std::sync::{Arc, Mutex};

use bevy::post_process::dof::{DepthOfField, DepthOfFieldMode};
use bevy::post_process::effect_stack::ChromaticAberration;
use bevy::post_process::motion_blur::MotionBlur;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use bevy::render::view::ColorGrading;
use bevy::window::PrimaryWindow;
use client_net::core::SEND_EVERY;
use sim::consts::TICK_HZ;

use crate::look::pipelines::PipelineBacklog;
use crate::look::rig::EyeCam;
use crate::{Eye, LinkRes, Look};
use encode::Encoder;
#[cfg(not(target_arch = "wasm32"))]
use encode::Wav;
pub use script::Script;
use script::{spline, wobble, Focus, FocusOn, Shot, BOOT_TAPE_MS, FILM_RATE, FSTOP, SETTLE_MAX_FRAMES, SHUTTER};

/// A non-pov shot's field of view when neither it nor the list names one,
/// degrees (vertical): a lens, not a player's sixteen-by-nine wide angle.
pub const CINE_FOV_DEG: f32 = 60.0;
/// How close a camera may come to the ground under it, metres.
pub const CLEAR_M: f32 = 0.4;
/// How far a follow camera stays off a wall it was pulled in by, metres.
pub const WALL_GAP_M: f32 = 0.3;
/// How long a pulled focus takes to catch its subject, seconds.
const PULL_S: f32 = 0.3;
/// Frames the wrap waits for a page's sink before it gives up.
#[cfg(target_arch = "wasm32")]
const WRAP_MAX_FRAMES: u32 = 600;
/// Seconds the wrap waits for the last shot's ffmpeg to finish, on the
/// wall: a fast GPU's frames are milliseconds, an encoder's tail is not.
#[cfg(not(target_arch = "wasm32"))]
const WRAP_MAX_S: f64 = 300.0;
/// The most the world may move in a frame, seconds: a time-lapse frame is
/// several of a player's (Bevy's default cap is a quarter second).
const MAX_DELTA_S: f64 = 4.0;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Phase {
    /// The level is being built from the tape's first seconds.
    Boot,
    /// Run the tape on to the next shot's start, less its settle.
    Seek,
    /// Holding the shot's first pose while what came into view compiles.
    Settle { frames: u32 },
    /// Rolling frame `frame` of the shot.
    Roll { frame: u32 },
    /// Every shot is rolled; waiting for the last frames to be written.
    Wrap { frames: u32 },
}

/// The lens this frame, as [`aim`] worked it out for [`lens`].
#[derive(Default)]
struct Lens {
    shutter: f32,
    /// Metres to the plane of focus, and the aperture; none is all sharp.
    focus: Option<(f32, f32)>,
    fringe: f32,
    grade: ColorGrading,
}

/// The film's whole state: inserted by [`install`] with its list.
#[derive(Resource)]
pub struct Film {
    script: Script,
    shot: usize,
    phase: Phase,
    /// The shot's key path in metres, resolved at its seek.
    path: Vec<(f32, Vec3, Vec3)>,
    /// A follow or track shot's camera and aim, as last smoothed.
    chase: Option<(Vec3, Vec3)>,
    /// The focus puller's distance last frame.
    pulled: Option<f32>,
    lens: Lens,
    encoders: Vec<Arc<Mutex<Encoder>>>,
    #[cfg(not(target_arch = "wasm32"))]
    wav: Option<Wav>,
    /// Each rolled frame's slice of the mix, by shot, for the page's sink.
    #[cfg(target_arch = "wasm32")]
    audio: Vec<(usize, Vec<f32>)>,
    /// Every frame is with the sink, or the film failed: what [`post`] says.
    #[cfg(target_arch = "wasm32")]
    end: Option<Result<(), String>>,
    /// Frames the run has drawn, for the log.
    drawn: u64,
    /// When the wrap began, on the wall.
    #[cfg(not(target_arch = "wasm32"))]
    wrap_at: Option<std::time::Instant>,
    /// The film is over (written, or failed); said once.
    wrapped: bool,
}

impl Film {
    pub fn new(script: Script) -> Self {
        Self {
            script,
            shot: 0,
            phase: Phase::Boot,
            path: Vec::new(),
            chase: None,
            pulled: None,
            lens: Lens::default(),
            encoders: Vec::new(),
            #[cfg(not(target_arch = "wasm32"))]
            wav: None,
            #[cfg(target_arch = "wasm32")]
            audio: Vec::new(),
            #[cfg(target_arch = "wasm32")]
            end: None,
            drawn: 0,
            #[cfg(not(target_arch = "wasm32"))]
            wrap_at: None,
            wrapped: false,
        }
    }

    /// The wrap has waited too long for its encoders.
    fn wrap_late(&mut self, frames: u32) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = frames;
            let at = *self.wrap_at.get_or_insert_with(std::time::Instant::now);
            at.elapsed().as_secs_f64() > WRAP_MAX_S
        }
        #[cfg(target_arch = "wasm32")]
        {
            frames > WRAP_MAX_FRAMES
        }
    }

    fn cur(&self) -> &Shot {
        &self.script.shots[self.shot]
    }

    /// The camera is a film's, not the player's eye: the player's own body
    /// is drawn and the gun in view is not (draw_fighters, draw_view,
    /// `own_shadow`).
    pub fn third(&self) -> bool {
        matches!(self.phase, Phase::Settle { .. } | Phase::Roll { .. }) && self.cur().third()
    }

    /// How far the list is drawn, 0 to 1, and the shot rolling: a page's
    /// progress bar.
    pub fn progress(&self) -> (f32, usize) {
        let frame = match self.phase {
            Phase::Roll { frame } => frame,
            _ => 0,
        };
        let (done, total) = self.script.frames_to(self.shot, frame);
        let f = match self.phase {
            Phase::Wrap { .. } => 1.0,
            _ => done as f32 / total.max(1) as f32,
        };
        (f, self.shot)
    }

    /// Frames this shot rolls.
    fn frames(&self) -> u32 {
        if self.cur().still {
            return self.script.hold.max(1);
        }
        self.cur().frames(&self.script)
    }

    fn frame_ms(&self) -> f64 {
        1000.0 / self.script.fps as f64
    }

    /// The shot's look over the list's.
    fn look(&self) -> script::Look {
        self.cur().look.over(&self.script.look)
    }

    /// The film cannot go on: the desktop exits with an error, a page is
    /// told.
    fn fail(&mut self, why: String, exit: &mut MessageWriter<AppExit>) {
        if self.wrapped {
            return;
        }
        self.wrapped = true;
        error!("film: {why}");
        #[cfg(not(target_arch = "wasm32"))]
        exit.write(AppExit::error());
        #[cfg(target_arch = "wasm32")]
        {
            let _ = exit;
            self.end = Some(Err(why));
        }
    }

    /// This frame's slice of the mix, for the shot rolling.
    fn put_audio(&mut self, buf: &[f32]) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(w) = &mut self.wav {
            w.push(buf);
        }
        #[cfg(target_arch = "wasm32")]
        self.audio.push((self.shot, buf.to_vec()));
    }

    /// The shot is over: close its mix.
    fn end_audio(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(w) = self.wav.take() {
            if let Err(e) = w.finish() {
                error!("film: the mix did not close: {e}");
            }
        }
    }

    /// `NN-name.ext`. Not `with_extension`, which would cut a name with a
    /// dot in it.
    #[cfg(not(target_arch = "wasm32"))]
    fn file(&self, ext: &str) -> std::path::PathBuf {
        self.script
            .out
            .join(format!("{:02}-{}.{ext}", self.shot, self.cur().name))
    }
}

/// The film's mix, rendered a frame at a time.
#[derive(Resource, Default)]
pub struct FilmMix(pub mix::Mix);

/// The client on a tape, filmed as `script` says: the app, built and
/// installed, for `run()`. `canvas` is the page's (a page draws a clip into
/// a second canvas); the desktop's window is the list's size. A tape with
/// no welcome on a map this build knows is no match to draw.
pub fn app(script: Script, tape: client_net::tape::Tape, canvas: Option<String>) -> Result<App, String> {
    match tape.map() {
        Some(m) if sim::map::by_id(m).is_some() => {}
        Some(m) => return Err(format!("the tape is on map {m}, which this build doesn't know")),
        None => return Err("the tape has no welcome: it is not a match".into()),
    }
    let opts = crate::Opts {
        quality: Some(script.quality()),
        ..Default::default()
    };
    let mut app = crate::app(Box::new(crate::link::Tape::new(tape)), opts);
    install(&mut app, script, canvas);
    Ok(app)
}

/// The frame clock, the window and the film's systems on an app built on a
/// tape link.
pub fn install(app: &mut App, script: Script, canvas: Option<String>) {
    let (w, h) = script.window();
    let fps = script.fps;
    {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&mut Window, With<PrimaryWindow>>();
        for mut win in q.iter_mut(world) {
            // The frame is the film's: exactly the list's size, at a scale
            // of one, never resized by a window manager or the page.
            win.title = "oneshot film".into();
            win.resolution = bevy::window::WindowResolution::new(w, h).with_scale_factor_override(1.0);
            win.resizable = false;
            win.fit_canvas_to_parent = false;
            // Opened without the focus: an F9 film draws beside a match
            // still being played (a second window on the desktop, a frame
            // in the page), and the player's keys stay with the match.
            win.focused = false;
            if let Some(c) = &canvas {
                win.canvas = Some(c.clone());
            }
        }
    }
    let mut virt = Time::<Virtual>::default();
    virt.set_max_delta(std::time::Duration::from_secs_f64(MAX_DELTA_S));
    // Unfocused, as it opens, it still draws flat out.
    app.insert_resource(bevy::winit::WinitSettings::continuous());
    app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
        std::time::Duration::from_secs_f64(1.0 / fps as f64),
    ))
    .insert_resource(virt)
    .insert_resource(Film::new(script))
    .init_resource::<FilmMix>()
    .add_systems(First, steer.before(bevy::time::TimeSystems))
    .add_systems(
        Update,
        (
            feed.before(crate::pump),
            as_player.after(crate::pump).before(crate::happened),
            (aim, lens).chain().after(crate::view).before(crate::look::fx::draw),
            mixdown.after(crate::sound::decide),
            roll.after(mixdown).after(crate::hud::draw).after(crate::tags::draw),
        ),
    )
    .add_systems(
        PostUpdate,
        hide_ui.before(bevy::camera::visibility::VisibilitySystems::VisibilityPropagate),
    );
    #[cfg(target_arch = "wasm32")]
    app.add_systems(Update, post.after(roll));
}

/// This frame's world speed, set before `Time` advances: the shot's while
/// it rolls (none for a still, and none once the boot has its world),
/// real time otherwise.
fn steer(film: Res<Film>, link: NonSend<LinkRes>, mut time: ResMut<Time<Virtual>>) {
    let speed = match film.phase {
        Phase::Boot if link.0.now_ms() >= BOOT_TAPE_MS => 0.0,
        Phase::Roll { .. } if film.cur().still => 0.0,
        Phase::Roll { frame } => {
            let shot = film.cur();
            let t = frame as f32 / film.script.fps as f32;
            shot.speed.at(t / shot.len as f32, t) as f64
        }
        Phase::Wrap { .. } => 0.0,
        _ => 1.0,
    };
    if time.relative_speed_f64() != speed {
        time.set_relative_speed_f64(speed);
    }
}

/// The tape's clock on by this frame's world time, ahead of the frame
/// (`pump` feeds the session to it); on a seek, the tape run on to the next
/// shot without drawing it.
fn feed(
    mut film: ResMut<Film>,
    mut link: NonSendMut<LinkRes>,
    time: Res<Time>,
    mut mix: ResMut<FilmMix>,
    mut look: ResMut<Look>,
) {
    let Some(tape) = link.0.tape() else {
        return;
    };
    if film.phase == Phase::Seek {
        let settle_ms = film.script.settle as f64 * film.frame_ms();
        let shot = film.cur().clone();
        let target = shot.at * 1000.0 - settle_ms;
        let from = tape.replay.clock_ms;
        let step = SEND_EVERY as f64 * 1000.0 / TICK_HZ as f64;
        tape.replay.seek(&mut tape.session, target, step);
        if target + 1.0 < from {
            warn!(
                "film: `{}` asks for {:.1} s but the tape is already at {:.1} s",
                shot.name,
                shot.at,
                (from + settle_ms) / 1000.0
            );
        }
        let span_s = shot.tape_s(film.script.fps);
        if tape.replay.end_ms() < (shot.at + span_s) * 1000.0 {
            warn!(
                "film: `{}` runs past the end of the tape ({:.1} s)",
                shot.name,
                tape.replay.end_ms() / 1000.0
            );
        }
        if shot.still {
            info!("film: {} `{}`: a still at tape {:.1} s", film.shot, shot.name, shot.at);
        } else {
            info!(
                "film: {} `{}`: tape {:.1} s, {:.1} s on screen, {:.1} s of tape",
                film.shot, shot.name, shot.at, shot.len, span_s
            );
        }
        let map = &tape.session.core.map;
        film.path = shot
            .keys
            .iter()
            .map(|k| (k.t, above(map, k.pos), above(map, k.look)))
            .collect();
        film.chase = None;
        film.pulled = None;
        // What was heard before the jump is not this shot's.
        mix.0.hush();
        look.zoom = false;
        film.phase = Phase::Settle { frames: 0 };
    }
    tape.replay.clock_ms += time.delta_secs_f64() * 1000.0;
}

/// The player's own aim and scope, off their snapshots: what a pov shot
/// looks through, and the way their drawn body faces.
fn as_player(link: NonSend<LinkRes>, mut look: ResMut<Look>) {
    let f = link.0.session().core.own().0;
    let turn = core::f32::consts::TAU / 65536.0;
    look.yaw = f.yaw as f32 * turn;
    look.pitch = f.pitch as f32 * turn;
    // The wire says scoped, not how far: the first level.
    look.zoom = f.alive && f.last_buttons & sim::input::ZOOM != 0 && sim::arms::arm(f.gun().arm).zoom > 1;
    look.level = look.zoom as u8;
}

/// A point `[x, above ground, z]` in metres.
fn above(map: &sim::map::Map, p: [f32; 3]) -> Vec3 {
    Vec3::new(p[0], map.ground(p[0], p[2]) + p[1], p[2])
}

/// A seat's feet this frame, and its heading (radians, the wire's way
/// round): the player's off their snapshots, anyone else's while the tape
/// has them.
fn subject_at(link: &LinkRes, slot: Option<u8>) -> Option<(Vec3, f32)> {
    let core = &link.0.session().core;
    let slot = slot.unwrap_or(core.slot);
    let d = if slot == core.slot {
        core.own_drawn()
    } else {
        core.others(link.0.now_ms()).into_iter().find(|d| d.slot == slot)?
    };
    Some((Vec3::new(d.pos.x, d.pos.y, d.pos.z), d.yaw))
}

/// The wire's heading as a direction on the ground.
fn heading(yaw: f32) -> Vec3 {
    Vec3::new(yaw.cos(), 0.0, yaw.sin())
}

/// Point the eye for this frame and work out the lens. After `view`,
/// which put it where the player looked; before the effects face it.
fn aim(
    mut film: ResMut<Film>,
    link: NonSend<LinkRes>,
    settings: Res<crate::settings::Settings>,
    mut cam: Query<(&mut Transform, &mut Projection), With<Eye>>,
) {
    let (frame, settling) = match film.phase {
        Phase::Settle { .. } => (0, true),
        Phase::Roll { frame } => (frame, false),
        _ => return,
    };
    let Ok((mut eye, mut proj)) = cam.single_mut() else {
        return;
    };
    let fps = film.script.fps as f32;
    let shot = film.cur().clone();
    let look = film.look();
    // A still is its first pose, held: nothing on it may move with the frame.
    let t = if shot.still { 0.0 } else { frame as f32 / fps };
    let len = (shot.len as f32).max(1e-3);
    let mut u = (t / len).clamp(0.0, 1.0);
    if shot.ease {
        u = u * u * (3.0 - 2.0 * u);
    }
    let grade = look.grading(u, t);
    let shutter = if shot.still {
        0.0
    } else {
        look.shutter.unwrap_or(SHUTTER)
    };
    let fringe = look.fringe.unwrap_or(0.0);
    let fov = shot.fov.as_ref().map(|f| f.at(u, t)).or(film.script.fov);
    if shot.pov {
        // The player's eye as `view` placed it, their field of view (and
        // scope) unless the list names one.
        if let (Some(f), Projection::Perspective(p)) = (fov, &mut *proj) {
            let scoped = p.fov < crate::look::rig::fov_tall(settings.fov).to_radians() - 1e-4;
            if !scoped {
                p.fov = f.to_radians();
            }
        }
        roll_and_shake(&mut eye, &look, u, t, film.shot, shot.still);
        film.lens = Lens {
            shutter,
            focus: None,
            fringe,
            grade,
        };
        return;
    }
    let core = &link.0.session().core;
    let map = &core.map;
    let (pos, aim_at) = if let Some(o) = &shot.orbit {
        let c = above(map, o.center);
        let th = (o.from + (o.to - o.from) * u).to_radians();
        let r = o.radius.at(u, t);
        let p = c + Vec3::new(r * th.sin(), o.height.at(u, t), r * th.cos());
        tracked(&mut film, &link, &shot, settling, fps, p, c)
    } else if let Some(f) = shot.follow {
        match (subject_at(&link, f.slot), film.chase) {
            (Some((body, yaw)), last) => {
                let fwd = heading(yaw);
                let right = fwd.cross(Vec3::Y);
                let head = body + Vec3::Y * 1.5;
                let mut p = body - fwd * f.back + right * f.side + Vec3::Y * f.up;
                // Pulled in off a wall between the head and the camera.
                let to = p - head;
                let d = to.length();
                if d > 1e-3 {
                    let dir = to / d;
                    let hit = map.ray(
                        sim::geom::V3::new(head.x, head.y, head.z),
                        sim::geom::V3::new(dir.x, dir.y, dir.z),
                        d,
                    );
                    if hit < d {
                        p = head + dir * (hit - WALL_GAP_M).max(0.2);
                    }
                }
                let l = body + Vec3::Y * 1.3 + fwd * 3.0;
                let (p, l) = match (settling, last) {
                    (false, Some((cp, cl))) => {
                        let k = 1.0 - (-1.0 / (fps * f.lag.max(0.01))).exp();
                        (cp + (p - cp) * k, cl + (l - cl) * k)
                    }
                    _ => (p, l),
                };
                film.chase = Some((p, l));
                (p, l)
            }
            // The subject has left the tape (fog): hold where the camera was.
            (None, Some(last)) => last,
            (None, None) => {
                let d = core.own_drawn();
                let body = Vec3::new(d.pos.x, d.pos.y, d.pos.z);
                (body - heading(d.yaw) * f.back + Vec3::Y * f.up, body + Vec3::Y * 1.3)
            }
        }
    } else {
        let (p, l) = spline(&film.path, if shot.still { 0.0 } else { u * len });
        tracked(&mut film, &link, &shot, settling, fps, p, l)
    };
    let pos = Vec3::new(pos.x, pos.y.max(map.ground(pos.x, pos.z) + CLEAR_M), pos.z);
    *eye = Transform::from_translation(pos).looking_at(aim_at, Vec3::Y);
    roll_and_shake(&mut eye, &look, u, t, film.shot, shot.still);
    if let Projection::Perspective(p) = &mut *proj {
        p.fov = fov.unwrap_or(CINE_FOV_DEG).to_radians();
    }
    // The lens: a `subject` focus is pulled after the body at a focus
    // puller's pace; a `look` focus is wherever the camera is aimed.
    let fstop = look.fstop.unwrap_or(FSTOP);
    let subject = shot.follow.map(|f| f.slot).or(shot.track.map(|t| t.slot));
    let focus = match &look.focus {
        None => None,
        Some(Focus::Metres(m)) => Some(m.at(u, t)),
        Some(Focus::On(FocusOn::Look)) => Some(pos.distance(aim_at)),
        Some(Focus::On(FocusOn::Subject)) => {
            let want = subject
                .and_then(|s| subject_at(&link, s))
                .map_or(pos.distance(aim_at), |(b, _)| pos.distance(b + Vec3::Y * 1.2));
            let d = match (settling, film.pulled) {
                (false, Some(last)) => last + (want - last) * (1.0 - (-1.0 / (fps * PULL_S)).exp()),
                _ => want,
            };
            film.pulled = Some(d);
            Some(d)
        }
    };
    film.lens = Lens {
        shutter,
        focus: focus.map(|f| (f.max(0.3), fstop)),
        fringe,
        grade,
    };
}

/// The list's dutch angle and the hand on the camera: three slow
/// incommensurate sines an axis, seeded by the shot so two cuts in a row
/// don't wobble alike.
fn roll_and_shake(eye: &mut Transform, look: &script::Look, u: f32, t: f32, shot: usize, still: bool) {
    let amp = look.shake.as_ref().map_or(0.0, |s| s.at(u, t)).to_radians();
    let roll = look.roll.as_ref().map_or(0.0, |r| r.at(u, t)).to_radians();
    let seed = shot as f32 * 7.31;
    let (wy, wp, wr) = if still || amp == 0.0 {
        (0.0, 0.0, 0.0)
    } else {
        (
            amp * wobble(t, seed),
            amp * 0.8 * wobble(t, seed + 11.7),
            amp * 0.5 * wobble(t, seed + 23.1),
        )
    };
    if wy != 0.0 {
        eye.rotate_y(wy);
    }
    if wp != 0.0 {
        eye.rotate_local_x(wp);
    }
    if roll + wr != 0.0 {
        eye.rotate_local_z(roll + wr);
    }
}

/// A shot's aim with its `track` applied: the camera stays at `p`, and the
/// look eases toward the subject rather than snapping to its every step.
fn tracked(film: &mut Film, link: &LinkRes, shot: &Shot, settling: bool, fps: f32, p: Vec3, l: Vec3) -> (Vec3, Vec3) {
    let Some((tr, (body, _))) = shot.track.and_then(|tr| subject_at(link, tr.slot).map(|b| (tr, b))) else {
        return (p, l);
    };
    let want = body + Vec3::Y * tr.up;
    let l = match (settling, film.chase) {
        (false, Some((_, cl))) => {
            let k = 1.0 - (-1.0 / (fps * tr.lag.max(0.01))).exp();
            cl + (want - cl) * k
        }
        _ => want,
    };
    film.chase = Some((p, l));
    (p, l)
}

/// The frame's lens on the camera: shutter, focus, fringe and grade, each
/// where this GPU has it (the module header). The components come and go
/// with the shots that want them, which costs a re-specialise: inside the
/// settle, where nothing is written.
#[allow(clippy::type_complexity)]
fn lens(
    mut commands: Commands,
    film: Res<Film>,
    tier: Res<crate::look::quality::Tier>,
    window: Query<&Window, With<PrimaryWindow>>,
    mut cam: Query<
        (
            Entity,
            &mut ColorGrading,
            Option<&mut MotionBlur>,
            Option<&mut DepthOfField>,
            Option<&mut ChromaticAberration>,
        ),
        With<EyeCam>,
    >,
) {
    if !matches!(film.phase, Phase::Settle { .. } | Phase::Roll { .. }) {
        return;
    }
    let Ok((cam, mut grading, blur, dof, fringe)) = cam.single_mut() else {
        return;
    };
    let l = &film.lens;
    *grading = l.grade.clone();
    let webgl2 = crate::look::gpu::WEBGL2;
    // A shutter wants the motion prepass, which the frame has where it has
    // TAA (`quality::effective_on` put it only where it fits).
    let shutter = (!webgl2 && tier.gfx.aa == crate::look::quality::Aa::Taa).then_some(l.shutter);
    match (shutter.filter(|s| *s > 0.0), blur) {
        (Some(s), Some(mut b)) => {
            if b.shutter_angle != s {
                b.shutter_angle = s;
            }
        }
        (Some(s), None) => {
            commands.entity(cam).insert(MotionBlur {
                shutter_angle: s,
                samples: 4,
            });
        }
        (None, Some(_)) => {
            commands.entity(cam).remove::<MotionBlur>();
        }
        (None, None) => {}
    }
    match (l.focus.filter(|_| !webgl2), dof) {
        (Some((at, fstop)), Some(mut d)) => {
            d.focal_distance = at;
            d.aperture_f_stops = fstop;
        }
        (Some((at, fstop)), None) => {
            // The blur's cap scales with the drawn frame, so a supersampled
            // shot looks as one drawn at size.
            let h = window.single().map_or(1080.0, |w| w.physical_height() as f32);
            // Bevy's bokeh doesn't work on WebGPU; its gaussian does.
            let mode = if crate::look::gpu::api() == "webgpu" {
                DepthOfFieldMode::Gaussian
            } else {
                DepthOfFieldMode::Bokeh
            };
            commands.entity(cam).insert(DepthOfField {
                mode,
                focal_distance: at,
                aperture_f_stops: fstop,
                max_circle_of_confusion_diameter: 48.0 * h / 1080.0,
                max_depth: crate::look::rig::FAR_M,
                ..default()
            });
        }
        (None, Some(_)) => {
            commands.entity(cam).remove::<DepthOfField>();
        }
        (None, None) => {}
    }
    // Bevy's fringe is two textures and a uniform: WebGL2 draws it too.
    match (l.fringe > 0.0, fringe) {
        (true, Some(mut f)) => f.intensity = l.fringe,
        (true, None) => {
            commands.entity(cam).insert(ChromaticAberration {
                intensity: l.fringe,
                max_samples: 12,
                ..default()
            });
        }
        (false, Some(_)) => {
            commands.entity(cam).remove::<ChromaticAberration>();
        }
        (false, None) => {}
    }
}

/// This frame's cues into the mix, heard from the camera, and the frame's
/// slice of it for the shot rolling. Rendered every frame, so a sound
/// started in the settle plays on into the shot.
fn mixdown(
    mut film: ResMut<Film>,
    mut mix: ResMut<FilmMix>,
    cues: Res<crate::sound::Cues>,
    cam: Query<&GlobalTransform, With<EyeCam>>,
    #[cfg(target_arch = "wasm32")] clips: Option<Res<crate::sound::on::Clips>>,
    #[cfg(target_arch = "wasm32")] sources: Option<Res<Assets<crate::sound::on::Sound>>>,
) {
    let ear = cam.iter().next().copied().unwrap_or_default();
    #[cfg(not(target_arch = "wasm32"))]
    let mut bytes = |name: &str| {
        let path = bevy::asset::io::file::FileAssetReader::get_base_path()
            .join("assets")
            .join("sound")
            .join(format!("{name}.ogg"));
        std::fs::read(path).ok()
    };
    #[cfg(target_arch = "wasm32")]
    let mut bytes = |name: &str| {
        let h = clips.as_ref()?.0.get(name)?;
        Some(sources.as_ref()?.get(h)?.bytes.to_vec())
    };
    for c in &cues.0 {
        mix.0.start(c, &ear, &mut bytes);
    }
    let n = (FILM_RATE / film.script.fps) as usize;
    let buf = mix.0.render(n);
    // A still is one frame and has no sound: its hold's worth would run
    // every later shot's sound ahead of its picture.
    if matches!(film.phase, Phase::Roll { .. }) && !film.cur().still {
        film.put_audio(buf);
    }
}

/// Everything on screen but the world: hidden for the film (the HUD, the
/// names, the lines). Every root node, before visibility is worked out,
/// but the render scale's frame (`render_scale.rs`): below 100% the world
/// itself is that node.
#[allow(clippy::type_complexity)]
fn hide_ui(
    mut roots: Query<
        &mut Visibility,
        (
            With<Node>,
            Without<ChildOf>,
            Without<crate::look::render_scale::Presentation>,
        ),
    >,
) {
    for mut v in &mut roots {
        v.set_if_neq(Visibility::Hidden);
    }
}

/// End the frame: shoot it if the shot is rolling, and move the film on.
#[allow(clippy::too_many_arguments)]
fn roll(
    mut commands: Commands,
    mut film: ResMut<Film>,
    mut exit: MessageWriter<AppExit>,
    link: NonSend<LinkRes>,
    settle: Res<crate::loading::Settle>,
    backlog: Res<PipelineBacklog>,
    window: Query<&Window, With<PrimaryWindow>>,
) {
    film.drawn += 1;
    let ready = link.0.session().ready && settle.shown() && backlog.pending() == 0;
    match film.phase {
        Phase::Boot => {
            if ready {
                info!("film: the level is there at frame {}", film.drawn);
                film.phase = Phase::Seek;
            }
        }
        // `feed` always leaves a seek before this runs.
        Phase::Seek => {}
        Phase::Settle { frames } => {
            let frames = frames + 1;
            let want = film.script.settle;
            if frames >= SETTLE_MAX_FRAMES.max(want) || (frames >= want && ready) {
                if !ready {
                    warn!("film: rolling `{}` with pipelines still compiling", film.cur().name);
                }
                let drawn = window
                    .single()
                    .map_or(film.script.window(), |w| (w.physical_width(), w.physical_height()));
                if let Err(e) = start_shot(&mut film, drawn) {
                    film.fail(e, &mut exit);
                    return;
                }
                film.phase = Phase::Roll { frame: 0 };
            } else {
                film.phase = Phase::Settle { frames };
            }
        }
        Phase::Roll { frame } => {
            let total = film.frames();
            let still = film.cur().still;
            // A still is taken once, on the last frame of its hold.
            if !still || frame + 1 == total {
                let Some(enc) = film.encoders.last().cloned() else {
                    film.fail("a rolling shot has no encoder".into(), &mut exit);
                    return;
                };
                let slot = if still { 0 } else { frame };
                commands
                    .spawn(Screenshot::primary_window())
                    .observe(move |cap: On<ScreenshotCaptured>| {
                        if let Ok(mut enc) = enc.lock() {
                            enc.put(slot, &cap.image);
                        }
                    });
            }
            if frame + 1 < total {
                film.phase = Phase::Roll { frame: frame + 1 };
            } else {
                let written = if still { 1 } else { total };
                if let Some(enc) = film.encoders.last() {
                    if let Ok(mut enc) = enc.lock() {
                        enc.total = Some(written);
                        enc.flush();
                    }
                }
                film.end_audio();
                info!("film: `{}` rolled, {written} frame(s)", film.cur().name);
                if film.shot + 1 < film.script.shots.len() {
                    film.shot += 1;
                    film.phase = Phase::Seek;
                } else {
                    film.phase = Phase::Wrap { frames: 0 };
                }
            }
        }
        Phase::Wrap { frames } => {
            let done = film
                .encoders
                .iter()
                .all(|e| e.lock().map(|mut e| e.done()).unwrap_or(true));
            if done {
                if !film.wrapped {
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        let bad = film
                            .encoders
                            .iter()
                            .filter(|e| e.lock().map(|mut e| !e.ok()).unwrap_or(true))
                            .count();
                        if bad > 0 {
                            film.fail(format!("ffmpeg failed on {bad} shot(s)"), &mut exit);
                            return;
                        }
                    }
                    film.wrapped = true;
                    info!("film: {} shot(s) in {}", film.encoders.len(), film.script.out.display());
                    #[cfg(not(target_arch = "wasm32"))]
                    exit.write(AppExit::Success);
                    #[cfg(target_arch = "wasm32")]
                    {
                        film.end = Some(Ok(()));
                    }
                }
            } else if film.wrap_late(frames) {
                film.fail("frames never reached the encoder".into(), &mut exit);
            } else {
                film.phase = Phase::Wrap { frames: frames + 1 };
            }
        }
    }
}

/// The shot's encoder (and on the desktop its WAV), for a window `drawn`
/// in size.
#[cfg(not(target_arch = "wasm32"))]
fn start_shot(film: &mut Film, drawn: (u32, u32)) -> Result<(), String> {
    std::fs::create_dir_all(&film.script.out).map_err(|e| format!("{}: {e}", film.script.out.display()))?;
    let s = &film.script;
    let out = (s.width, s.height);
    let enc = if film.cur().still {
        Encoder::open(&film.file("png"), drawn, out, None)?
    } else {
        let enc = Encoder::open(&film.file("mp4"), drawn, out, Some(s.fps))?;
        film.wav = Some(Wav::create(&film.file("wav"))?);
        enc
    };
    film.encoders.push(Arc::new(Mutex::new(enc)));
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn start_shot(film: &mut Film, drawn: (u32, u32)) -> Result<(), String> {
    let s = &film.script;
    let enc = Encoder::new(
        film.shot,
        film.cur().name.clone(),
        film.cur().still,
        s.fps,
        drawn,
        (s.width, s.height),
    );
    film.encoders.push(Arc::new(Mutex::new(enc)));
    Ok(())
}

/// Hand the page what the film has made since last frame: a shot's start,
/// its frames in order, its mix, its end, the progress, and at the last
/// the verdict. The verdict ends the app (on this target a frozen canvas),
/// so the page's encoder has the machine to itself.
#[cfg(target_arch = "wasm32")]
fn post(
    mut film: ResMut<Film>,
    sink: Option<NonSend<encode::Sink>>,
    mut exit: MessageWriter<AppExit>,
    mut told: Local<bool>,
) {
    use wasm_bindgen::JsValue;
    let Some(sink) = sink else {
        return;
    };
    if *told {
        return;
    }
    for (shot, pcm) in film.audio.drain(..) {
        sink.call(
            "audio",
            &[JsValue::from(shot as u32), js_sys::Float32Array::from(&pcm[..]).into()],
        );
    }
    for enc in &film.encoders {
        let Ok(mut enc) = enc.lock() else {
            continue;
        };
        if !enc.announced {
            enc.announced = true;
            sink.call(
                "shot",
                &[
                    JsValue::from(enc.shot as u32),
                    JsValue::from_str(&enc.name),
                    JsValue::from(enc.drawn.0),
                    JsValue::from(enc.drawn.1),
                    JsValue::from(enc.out.0),
                    JsValue::from(enc.out.1),
                    JsValue::from(enc.fps),
                    JsValue::from_bool(enc.still),
                ],
            );
        }
        let shot = enc.shot as u32;
        let (w, h) = enc.drawn;
        let mut n = enc.next - enc.ready.len() as u32;
        while let Some(rgba) = enc.ready.pop_front() {
            sink.call(
                "frame",
                &[
                    JsValue::from(shot),
                    JsValue::from(n),
                    js_sys::Uint8Array::from(&rgba[..]).into(),
                    JsValue::from(w),
                    JsValue::from(h),
                ],
            );
            n += 1;
        }
        if !enc.closed && enc.total.is_some_and(|t| enc.next >= t) {
            enc.closed = true;
            sink.call("shotEnd", &[JsValue::from(shot), JsValue::from(enc.next)]);
        }
    }
    let (f, shot) = film.progress();
    sink.call("progress", &[JsValue::from(f), JsValue::from(shot as u32)]);
    match film.end.take() {
        Some(Ok(())) => {
            *told = true;
            sink.call("done", &[]);
            exit.write(AppExit::Success);
        }
        Some(Err(why)) => {
            *told = true;
            sink.call("fail", &[JsValue::from_str(&why)]);
            exit.write(AppExit::Success);
        }
        None => {}
    }
}

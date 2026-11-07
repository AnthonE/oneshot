//! The screenshot key, F12 on every screen, and `--shot`, one frame for the
//! tools: the Bevy half of `crate::shot`, after Gates'
//! `client/src/render/shot.rs` and the settle of its `render/capture.rs`
//! (MIT).
//!
//! **F12** reads the game's own frame (Bevy's `Screenshot`), so it works
//! where the desktop's key doesn't (`crate::shot`'s header). On the desktop
//! the PNG goes in the player's pictures folder (`shot::dir`), encoded and
//! written on a thread of its own so the frame doesn't hitch; in a browser,
//! which can't write a file, the browser's own encoder makes the PNG (a
//! canvas' `toBlob`, off the page's thread) and hands it over as a
//! download.
//! Either way a line at the top says where, once it is there, and says so
//! louder if it isn't. The line is hidden the frame the key is pressed, so
//! the last shot's line is never in the next shot.
//!
//! **`--shot <file.png>`** (`main.rs`, `ci/desktop_shot.sh`) waits on state,
//! never on a clock: the screen asked for, on it, with its level there and
//! nothing over it (`loading::Settle::shown`), then `--shot-at` seconds,
//! then the frame, then exit 0. A match that is refused, a level that never
//! arrives or a frame that never comes back exits non-zero, saying which.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use bevy::window::{PrimaryWindow, WindowLevel};

use crate::front::{Front, InMatch};
use crate::loading::Settle;
use crate::look::pipelines::PipelineBacklog;
use crate::{shot, ui, Screen};

/// The key. The function keys the game keeps for itself are
/// `binds::RESERVED`, which stops a player binding this one.
pub const KEY: KeyCode = KeyCode::F12;
/// Shots out at once: a cap on a key a player can lean on, and its policy is
/// refuse and say so. Each clears when its file is written, well under a
/// second.
pub const PENDING_CAP: usize = 4;
/// Seconds after the last shot asked for that the ones still out are
/// given up on.
pub const PENDING_GIVE_UP_S: f64 = 20.0;
/// How long the line stays, then how long it fades.
const LINE_S: f64 = 4.0;
const LINE_FADE_S: f64 = 0.6;
/// Over every screen.
const LINE_Z: i32 = 30;
/// The strip's word for a shot, and for one that failed.
const TAKEN: &str = "SCREENSHOT";
const FAILED: &str = "SCREENSHOT FAILED";

/// How a shot ended: saved (its name, and where in words), or not; or
/// another key's line on the same strip (the clip key, `clip.rs`).
enum Outcome {
    Saved(String, String),
    Failed(String),
    Said { word: String, what: String, bad: bool },
}

/// The line, for a key other than F12 (the clip key): what it says comes
/// up on the same strip, from any thread.
#[derive(Clone)]
pub struct Line(Arc<Mutex<Vec<Outcome>>>);

impl Line {
    /// `word` in the strip's mark (its bad colour when `bad`), then `what`.
    pub fn say(&self, word: &str, what: impl Into<String>, bad: bool) {
        if let Ok(mut v) = self.0.lock() {
            v.push(Outcome::Said {
                word: word.to_string(),
                what: what.into(),
                bad,
            });
        }
    }
}

/// Where the shots go, and the ones still on their way.
#[derive(Resource)]
pub struct Shots {
    /// Resolved once at startup (`shot::dir`); `None` turns the key off on
    /// the desktop, with a line saying so. A browser needs none.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    dir: Option<PathBuf>,
    /// Filled by the observer (or its thread), drained by [`take`].
    out: Arc<Mutex<Vec<Outcome>>>,
    pending: usize,
    /// When the last one was asked for (real seconds).
    pending_at: f64,
    /// A browser's names: the stamp last used and how many had it.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    last: (String, u32),
    /// When the line was put up (real seconds), and whether it's bad news.
    line: Option<(f64, bool)>,
    /// Shots taken this run.
    pub taken: u32,
}

impl Shots {
    /// The strip, for another key's line.
    pub fn line(&self) -> Line {
        Line(self.out.clone())
    }
}

impl Default for Shots {
    fn default() -> Self {
        let dir = if cfg!(target_arch = "wasm32") {
            None
        } else {
            shot::dir()
        };
        if dir.is_none() && cfg!(not(target_arch = "wasm32")) {
            warn!("oneshot: no home folder in the environment, so F12 saves nothing: set ONESHOT_SHOTS_DIR");
        }
        Shots {
            dir,
            out: Arc::default(),
            pending: 0,
            pending_at: 0.0,
            last: (String::new(), 0),
            line: None,
            taken: 0,
        }
    }
}

/// The line's root, its word and what it says.
#[derive(Component)]
struct ShotLine;
#[derive(Component)]
struct ShotWord;
#[derive(Component)]
struct ShotSays;

/// A picture's window stays out of the way of whoever is at the desk: under
/// their windows and never focused, a dozen shots in a row taking the screen
/// look like the machine glitching. What it draws is the same.
pub fn keep_out_of_the_way(app: &mut App) {
    let world = app.world_mut();
    let mut windows = world.query_filtered::<&mut Window, With<PrimaryWindow>>();
    for mut w in windows.iter_mut(world) {
        w.window_level = WindowLevel::AlwaysOnBottom;
        w.focused = false;
    }
}

pub fn plugin(app: &mut App) {
    app.init_resource::<Shots>()
        .add_systems(Startup, setup)
        .add_systems(Update, (take, line).chain())
        .add_systems(
            PostUpdate,
            capture.after(crate::loading::shown).run_if(resource_exists::<Capture>),
        );
}

fn setup(mut commands: Commands) {
    commands
        .spawn((
            ShotLine,
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(16.0),
                width: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                ..default()
            },
            GlobalZIndex(LINE_Z),
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
                p.spawn((ShotWord, ui::strong("SCREENSHOT", 12.0, ui::MARK)));
                p.spawn((ShotSays, ui::label("", 13.0, ui::TEXT_COLOR)));
            });
        });
}

/// What a shot's frame becomes: a PNG, without the alpha (which holds
/// brightness, not coverage, on an HDR camera).
fn encode(img: Image) -> Result<Vec<u8>, String> {
    let rgb = img.try_into_dynamic().map_err(|e| e.to_string())?.to_rgb8();
    let mut out = std::io::Cursor::new(Vec::new());
    rgb.write_to(&mut out, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(out.into_inner())
}

/// The press: a shot, its line hidden first; and the line for every shot
/// that came back.
#[allow(clippy::type_complexity)]
fn take(
    mut commands: Commands,
    mut shots: ResMut<Shots>,
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time<Real>>,
    mut word: Query<(&mut Text, &mut TextColor), (With<ShotWord>, Without<ShotSays>)>,
    mut says: Query<&mut Text, (With<ShotSays>, Without<ShotWord>)>,
) {
    let now = time.elapsed_secs_f64();
    let came: Vec<Outcome> = shots.out.lock().map(|mut v| v.drain(..).collect()).unwrap_or_default();
    let mut tell = |shots: &mut Shots, w: &str, bad: bool, what: String| {
        shots.line = Some((now, bad));
        for (mut t, mut c) in &mut word {
            t.0 = w.into();
            c.0 = if bad { ui::BAD } else { ui::MARK };
        }
        for mut t in &mut says {
            t.0 = what.clone();
        }
    };
    for o in came {
        match o {
            Outcome::Saved(name, at) => {
                shots.pending = shots.pending.saturating_sub(1);
                tell(&mut shots, TAKEN, false, format!("{name}  {at}"))
            }
            Outcome::Failed(e) => {
                shots.pending = shots.pending.saturating_sub(1);
                error!("oneshot: a screenshot failed: {e}");
                tell(&mut shots, FAILED, true, e);
            }
            Outcome::Said { word, what, bad } => tell(&mut shots, &word, bad, what),
        }
    }
    if !keys.just_pressed(KEY) {
        return;
    }
    // A frame that never came back (a lost device, a browser that never
    // answered) doesn't hold the key shut for the rest of the run.
    if shots.pending > 0 && now - shots.pending_at > PENDING_GIVE_UP_S {
        warn!(
            "oneshot: {} screenshot(s) never came back; forgetting them",
            shots.pending
        );
        shots.pending = 0;
    }
    if shots.pending >= PENDING_CAP {
        tell(&mut shots, FAILED, true, "still saving the last ones".into());
        return;
    }
    let stamp = shot::stamp(shot::now_secs());
    let out = shots.out.clone();
    #[cfg(not(target_arch = "wasm32"))]
    let observer = {
        let Some(dir) = shots.dir.clone() else {
            tell(
                &mut shots,
                FAILED,
                true,
                "no folder for them: set ONESHOT_SHOTS_DIR".into(),
            );
            return;
        };
        if let Err(e) = std::fs::create_dir_all(&dir) {
            tell(&mut shots, FAILED, true, format!("{}: {e}", shot::shown(&dir)));
            return;
        }
        let Some(path) = shot::pick(&dir, &stamp, |p| p.exists()) else {
            tell(&mut shots, FAILED, true, "too many this second".into());
            return;
        };
        info!("oneshot: screenshot to {}", path.display());
        move |ev: On<ScreenshotCaptured>| {
            let img = ev.image.clone();
            let (path, out) = (path.clone(), out.clone());
            // Encoded and written off the frame: a PNG of a big window is
            // tens of milliseconds.
            std::thread::spawn(move || {
                let r = encode(img).and_then(|png| std::fs::write(&path, png).map_err(|e| e.to_string()));
                let o = match r {
                    Ok(()) => Outcome::Saved(
                        path.file_name().unwrap_or_default().to_string_lossy().into_owned(),
                        format!("in {}", path.parent().map(shot::shown).unwrap_or_default()),
                    ),
                    Err(e) => Outcome::Failed(format!("{}: {e}", shot::shown(&path))),
                };
                if let Ok(mut v) = out.lock() {
                    v.push(o);
                }
            });
        }
    };
    #[cfg(target_arch = "wasm32")]
    let observer = {
        let n = if shots.last.0 == stamp { shots.last.1 + 1 } else { 1 };
        if n > shot::PER_STAMP {
            tell(&mut shots, FAILED, true, "too many this second".into());
            return;
        }
        shots.last = (stamp.clone(), n);
        let name = shot::name(&stamp, n);
        move |ev: On<ScreenshotCaptured>| {
            // The browser's own encoder makes the PNG, off the page's
            // thread; it says how that went when it's done.
            if let Err(e) = web::save(name.clone(), ev.image.clone(), out.clone()) {
                if let Ok(mut v) = out.lock() {
                    v.push(Outcome::Failed(e));
                }
            }
        }
    };
    // Off before the frame is read: it is this frame the shot is of.
    shots.line = None;
    commands.spawn(Screenshot::primary_window()).observe(observer);
    shots.pending += 1;
    shots.pending_at = now;
    shots.taken += 1;
}

/// The line: up for [`LINE_S`], then faded out and hidden.
#[allow(clippy::type_complexity)]
fn line(
    mut shots: ResMut<Shots>,
    time: Res<Time<Real>>,
    mut root: Query<(&mut Visibility, &Children), With<ShotLine>>,
    mut colours: Query<(Option<&mut BackgroundColor>, Option<&mut BorderColor>)>,
    mut text: Query<(&mut TextColor, Has<ShotWord>)>,
    kids: Query<&Children>,
) {
    let now = time.elapsed_secs_f64();
    let a = match shots.line {
        Some((at, _)) if now - at < LINE_S + LINE_FADE_S => {
            (1.0 - ((now - at - LINE_S) / LINE_FADE_S).clamp(0.0, 1.0)) as f32
        }
        Some(_) => {
            shots.line = None;
            0.0
        }
        None => 0.0,
    };
    let bad = shots.line.is_some_and(|(_, b)| b);
    for (mut vis, children) in &mut root {
        vis.set_if_neq(if a > 0.0 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
        if a <= 0.0 {
            continue;
        }
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
                if let Ok((mut c, is_word)) = text.get_mut(e) {
                    let base = match (is_word, bad) {
                        (true, true) => ui::BAD,
                        (true, false) => ui::MARK,
                        (false, _) => ui::TEXT_COLOR,
                    };
                    c.0 = base.with_alpha(a);
                }
            }
        }
    }
}

/// `--shot`: one frame of `want`, once it is there, and then out.
#[derive(Resource)]
pub struct Capture {
    path: PathBuf,
    /// Seconds on screen before the frame is taken.
    at_s: f64,
    want: Screen,
    /// A match was asked for (`Start::Join`): its level, not the title's.
    want_match: bool,
    /// When it was first on screen as asked (real seconds).
    ready_at: Option<f64>,
    /// When the frame was asked for.
    shot_at: Option<f64>,
    /// What the observer made of it.
    done: Option<Result<(), String>>,
}

/// Seconds a `--shot` waits for its screen before it gives up (a software
/// GPU's first frames take seconds each), and for its frame once asked.
pub const CAPTURE_GIVE_UP_S: f64 = 600.0;
const FRAME_GIVE_UP_S: f64 = 60.0;
/// Past `--shot-at`, a pipeline compiling for something just come into view
/// gets this long to finish.
const PIPELINE_GRACE_S: f64 = 3.0;

impl Capture {
    pub fn new(path: PathBuf, at_s: f64, want: Screen, want_match: bool) -> Capture {
        Capture {
            path,
            at_s: at_s.max(0.0),
            want,
            want_match,
            ready_at: None,
            shot_at: None,
            done: None,
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn capture(
    mut commands: Commands,
    mut cap: ResMut<Capture>,
    settle: Res<Settle>,
    screen: Res<State<Screen>>,
    in_match: Option<Res<InMatch>>,
    front: Option<Res<Front>>,
    backlog: Res<PipelineBacklog>,
    time: Res<Time<Real>>,
    mut exit: MessageWriter<AppExit>,
) {
    let now = time.elapsed_secs_f64();
    let path = cap.path.display().to_string();
    let mut fail = |why: String| {
        eprintln!("oneshot: --shot {path}: {why}");
        exit.write(AppExit::from_code(2));
    };
    if let Some(r) = cap.done.take() {
        match r {
            Ok(()) => {
                println!("oneshot: shot {path}");
                exit.write(AppExit::Success);
            }
            Err(e) => fail(e),
        }
        return;
    }
    if let Some(at) = cap.shot_at {
        if now - at > FRAME_GIVE_UP_S {
            fail(format!("the frame never came back in {FRAME_GIVE_UP_S} s"));
            cap.shot_at = None;
        }
        return;
    }
    let in_match = in_match.is_some_and(|m| m.0);
    if cap.want_match && !in_match {
        if let Some(why) = front.as_deref().and_then(Front::refused) {
            return fail(format!("the match never started: {why}"));
        }
    }
    if now > CAPTURE_GIVE_UP_S {
        let why = if cap.want_match && !in_match {
            "the match never started".to_string()
        } else if *screen.get() != cap.want {
            format!("never on {:?} (on {:?})", cap.want, screen.get())
        } else {
            format!("the level never got there ({})", settle.now.stage())
        };
        return fail(format!("{why}, after {CAPTURE_GIVE_UP_S} s"));
    }
    let there = settle.shown() && *screen.get() == cap.want && (!cap.want_match || in_match);
    if !there {
        cap.ready_at = None;
        return;
    }
    let at = match cap.ready_at {
        Some(at) => at,
        None => {
            info!(
                "oneshot: --shot: on screen after {now:.1} s; the frame in {:.1} s",
                cap.at_s
            );
            cap.ready_at = Some(now);
            now
        }
    };
    if now - at < cap.at_s || (backlog.pending() > 0 && now - at < cap.at_s + PIPELINE_GRACE_S) {
        return;
    }
    cap.shot_at = Some(now);
    let to = cap.path.clone();
    commands.spawn(Screenshot::primary_window()).observe(
        move |ev: On<ScreenshotCaptured>, mut cap: ResMut<Capture>| {
            let write = |png: Vec<u8>| {
                if let Some(dir) = to.parent().filter(|d| !d.as_os_str().is_empty()) {
                    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
                }
                std::fs::write(&to, png).map_err(|e| e.to_string())
            };
            cap.done = Some(encode(ev.image.clone()).and_then(write));
        },
    );
}

/// The browser's half: a file is a download.
#[cfg(target_arch = "wasm32")]
mod web {
    use std::sync::{Arc, Mutex};

    use bevy::prelude::Image;
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::{Clamped, JsCast, JsValue};

    use super::Outcome;

    /// Seconds the download's object URL is kept before it is let go.
    const REVOKE_MS: i32 = 10_000;

    fn js(e: JsValue) -> String {
        format!("{e:?}")
    }

    /// The frame onto a canvas of its own and out through the canvas' own
    /// PNG encoder (`toBlob`), which the browser runs off the page's
    /// thread: encoding a big frame in wasm was tens of milliseconds of a
    /// frame in the middle of a fight. Opaque: the frame's alpha holds
    /// brightness, not coverage (`encode`).
    pub fn save(name: String, img: Image, out: Arc<Mutex<Vec<Outcome>>>) -> Result<(), String> {
        let rgba = img.try_into_dynamic().map_err(|e| e.to_string())?.to_rgba8();
        let (w, h) = rgba.dimensions();
        let mut px = rgba.into_raw();
        for a in px.iter_mut().skip(3).step_by(4) {
            *a = 255;
        }
        let doc = web_sys::window().and_then(|w| w.document()).ok_or("no document")?;
        let canvas: web_sys::HtmlCanvasElement = doc
            .create_element("canvas")
            .map_err(js)?
            .dyn_into()
            .map_err(|_| "not a canvas")?;
        canvas.set_width(w);
        canvas.set_height(h);
        let ctx: web_sys::CanvasRenderingContext2d = canvas
            .get_context("2d")
            .map_err(js)?
            .ok_or("no 2d context")?
            .dyn_into()
            .map_err(|_| "not a 2d context")?;
        let data = web_sys::ImageData::new_with_u8_clamped_array_and_sh(Clamped(&px), w, h).map_err(js)?;
        ctx.put_image_data(&data, 0, 0).map_err(js)?;
        let done = Closure::once_into_js(move |blob: JsValue| {
            let o = match blob.dyn_into::<web_sys::Blob>() {
                Ok(b) => match download(&name, &b) {
                    Ok(()) => Outcome::Saved(name.clone(), "to your downloads".into()),
                    Err(e) => Outcome::Failed(e),
                },
                Err(_) => Outcome::Failed("the browser made no PNG of it".into()),
            };
            if let Ok(mut v) = out.lock() {
                v.push(o);
            }
        });
        canvas.to_blob_with_type(done.unchecked_ref(), "image/png").map_err(js)
    }

    fn download(name: &str, blob: &web_sys::Blob) -> Result<(), String> {
        let w = web_sys::window().ok_or("no window")?;
        let doc = w.document().ok_or("no document")?;
        let url = web_sys::Url::create_object_url_with_blob(blob).map_err(js)?;
        let a: web_sys::HtmlAnchorElement = doc
            .create_element("a")
            .map_err(js)?
            .dyn_into()
            .map_err(|_| "not an anchor")?;
        a.set_href(&url);
        a.set_download(name);
        a.click();
        // Let go of the blob once the browser has it, not before: some
        // browsers start a download after the click returns.
        let revoke = Closure::once_into_js(move || {
            let _ = web_sys::Url::revoke_object_url(&url);
        });
        let _ = w.set_timeout_with_callback_and_timeout_and_arguments_0(revoke.unchecked_ref(), REVOKE_MS);
        Ok(())
    }
}

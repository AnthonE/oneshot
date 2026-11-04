//! The clip key: `F9` makes a video of the last half minute. After Gates'
//! `client/src/render/clip.rs` and `clip.rs`'s styles (MIT).
//!
//! The session keeps its last minute as a tape (`client_net::clip`); this
//! hands it on. In a page the tape goes to the page
//! (`window.__oneshotClip`), which draws it with this same client in a
//! second canvas (`web/clip.html`, `film()` in lib.rs) and encodes it on the
//! player's GPU. On the desktop it is written beside the screenshots and,
//! where `ffmpeg` is on the path, a second `oneshot --clip` draws it into a
//! video beside it, so the match the player is in never waits on a frame of
//! it; without `ffmpeg` the line says where the tape is and how to make the
//! video later. Nothing leaves the machine either way.
//!
//! `--record <tape> <seconds>` is the same memory kept for longer, for
//! `ci/film.sh record`: once the match has run that long, or ends, the tape
//! is written and the client exits.

use bevy::prelude::*;
pub use client_net::clip::CLIP_S;

use crate::LinkRes;

/// The key. Next to the screenshot's F12 and reserved with it
/// (`binds::RESERVED`).
pub const KEY: KeyCode = KeyCode::F9;
/// The strip's word for it.
const WORD: &str = "CLIP";
const FAILED: &str = "CLIP FAILED";

/// The clip styles: a shot list each (`film::script`), its `at`s counted
/// back from the tape's end. `wide` is the player's screen; the rest are
/// 9:16, the shape a phone's feed plays.
pub const STYLES: [&str; 5] = ["wide", "pov", "chase", "mix", "long"];

/// The styles a match still being played may be clipped in: the player's
/// own eye only. A camera behind and above them sees over cover and round
/// corners, and the tape holds every enemy the fog sent because it could
/// be seen soon, not only the ones the player saw; a chase of the last
/// fifteen seconds mid-match would show where they went. Once the match
/// is over every style is open.
/// The page offers only these until the match is over (`web/index.html`'s
/// `__oneshotClip`, told by [`take`]).
pub const LIVE_STYLES: [&str; 3] = ["wide", "pov", "long"];

/// A style's shot list as JSON, `None` for a name not in [`STYLES`].
pub fn script(style: &str) -> Option<String> {
    // The camera behind and a little above the player, quick to follow: an
    // arena's fights turn fast.
    const CHASE: &str = r#"{"back": 3.2, "up": 1.4, "side": 0.55, "lag": 0.22}"#;
    let pov = |at: f64, len: f64, fov: Option<f32>| {
        let fov = fov.map_or(String::new(), |f| format!(r#", "fov": {f}"#));
        format!(r#"{{"name": "pov", "at": {at}, "len": {len}, "pov": true{fov}}}"#)
    };
    let chase =
        |at: f64, len: f64| format!(r#"{{"name": "chase", "at": {at}, "len": {len}, "follow": {CHASE}, "fov": 64}}"#);
    // A 9:16 frame's horizontal field at the player's vertical one would be
    // a keyhole: the vertical shots open it up.
    const TALL_FOV: f32 = 92.0;
    let (size, shots) = match style {
        "wide" => ((1920, 1080), vec![pov(-30.2, 30.0, None)]),
        "pov" => ((1080, 1920), vec![pov(-15.2, 15.0, Some(TALL_FOV))]),
        "chase" => ((1080, 1920), vec![chase(-15.2, 15.0)]),
        // A cut needs room for its settle between the two.
        "mix" => ((1080, 1920), vec![chase(-16.6, 8.0), pov(-8.1, 8.0, Some(TALL_FOV))]),
        "long" => ((1080, 1920), vec![pov(-30.2, 30.0, Some(TALL_FOV))]),
        _ => return None,
    };
    Some(format!(
        r#"{{"width": {}, "height": {}, "fps": 30, "settle": 12,
            "look": {{"saturation": 1.05, "contrast": 1.04, "shutter": 0.35}},
            "shots": [{}]}}"#,
        size.0,
        size.1,
        shots.join(", ")
    ))
}

pub fn plugin(app: &mut App) {
    app.add_systems(Update, take.run_if(not(resource_exists::<crate::film::Film>)))
        .add_systems(Update, record.run_if(resource_exists::<Record>));
}

/// The press: the tape out of the session, and on to whoever makes the
/// video.
fn take(
    keys: Res<ButtonInput<KeyCode>>,
    link: NonSend<LinkRes>,
    monitors: Query<&bevy::window::Monitor>,
    shots: Res<crate::look::shot::Shots>,
) {
    if !keys.just_pressed(KEY) {
        return;
    }
    let line = shots.line();
    let s = link.0.session();
    if !link.0.live() || !s.ready {
        line.say(
            FAILED,
            "play a match first: a clip is of the last half minute of one",
            true,
        );
        return;
    }
    let Some(tape) = s.clip_tape(CLIP_S) else {
        line.say(FAILED, "nothing to clip yet", true);
        return;
    };
    let held = s.clip_held_s().min(CLIP_S);
    let on = s.core.phase() != sim::world::Phase::Over;
    // The film draws in a window of its own; on a screen shorter than the
    // video it draws smaller and the encoder scales it up.
    let screen = monitors.iter().map(|m| m.physical_height).max();
    hand_over(tape, held, on, screen, line);
}

#[cfg(target_arch = "wasm32")]
fn hand_over(tape: Vec<u8>, held: f64, on: bool, _screen: Option<u32>, line: crate::look::shot::Line) {
    use wasm_bindgen::{JsCast, JsValue};
    let global = js_sys::global();
    let f = js_sys::Reflect::get(&global, &JsValue::from_str("__oneshotClip"))
        .ok()
        .and_then(|f| f.dyn_into::<js_sys::Function>().ok());
    let Some(f) = f else {
        line.say(FAILED, "this page can't make videos", true);
        return;
    };
    let bytes = js_sys::Uint8Array::from(&tape[..]);
    let args = js_sys::Array::of4(
        &bytes,
        &JsValue::from(held),
        &JsValue::from_str(crate::look::gpu::api()),
        &JsValue::from(on),
    );
    match f.apply(&global, &args) {
        Ok(_) => line.say(WORD, format!("the last {held:.0} s: making your video"), false),
        Err(e) => {
            error!("oneshot: the page's __oneshotClip threw: {e:?}");
            line.say(FAILED, "the page refused it", true);
        }
    }
}

/// The tape beside the screenshots, and a second copy of this binary to
/// draw it, on a thread: a file and a process are not a frame's work.
#[cfg(not(target_arch = "wasm32"))]
fn hand_over(tape: Vec<u8>, held: f64, _on: bool, screen: Option<u32>, line: crate::look::shot::Line) {
    let Some(dir) = crate::shot::dir() else {
        line.say(FAILED, "no folder for it: set ONESHOT_SHOTS_DIR", true);
        return;
    };
    std::thread::spawn(move || {
        let stamp = crate::shot::stamp(crate::shot::now_secs());
        let path = dir.join(format!("{}clip-{stamp}.tape", crate::shot::PREFIX));
        if let Err(e) = std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&path, &tape)) {
            error!("oneshot: cannot write {}: {e}", path.display());
            line.say(FAILED, format!("{}: {e}", crate::shot::shown(&path)), true);
            return;
        }
        info!("oneshot: clip tape to {}", path.display());
        let shown = crate::shot::shown(&path);
        if !crate::film::encode::ffmpeg_here() {
            line.say(
                WORD,
                format!("{shown} saved; with ffmpeg installed, `oneshot --clip {shown}` makes the video"),
                false,
            );
            return;
        }
        let drawn = std::env::current_exe().and_then(|exe| {
            let mut film = std::process::Command::new(exe);
            film.arg("--clip").arg(&path);
            if let Some(h) = screen {
                // Room for a title bar and a task bar.
                film.arg("--max-height").arg(h.saturating_sub(160).max(240).to_string());
            }
            film.stdout(std::process::Stdio::null()).spawn()
        });
        let mp4 = crate::shot::shown(&path.with_extension("mp4"));
        match drawn {
            Ok(_) => line.say(WORD, format!("the last {held:.0} s: making {mp4}"), false),
            Err(e) => {
                error!("oneshot: cannot start the clip's film: {e}");
                line.say(
                    WORD,
                    format!("{shown} saved; `oneshot --clip {shown}` makes the video"),
                    false,
                );
            }
        }
    });
}

/// `--record`: where the tape goes and how much match it holds.
#[derive(Resource)]
pub struct Record {
    pub path: std::path::PathBuf,
    pub secs: f64,
    /// When the match began, on the link's clock.
    began: Option<f64>,
    /// Written (or failed): the app is on its way out.
    done: bool,
}

impl Record {
    pub fn new(path: std::path::PathBuf, secs: f64) -> Record {
        Record {
            path,
            secs: secs.max(1.0),
            began: None,
            done: false,
        }
    }
}

/// Keep the whole match, and write it once it has run its seconds, or
/// ends; then out.
fn record(
    mut rec: ResMut<Record>,
    mut link: NonSendMut<LinkRes>,
    st: Res<crate::hud::HudState>,
    mut exit: MessageWriter<AppExit>,
) {
    if rec.done || !link.0.live() || !link.0.session().ready {
        return;
    }
    let now = link.0.now_ms();
    let secs = rec.secs;
    let began = *rec.began.get_or_insert_with(|| {
        info!("oneshot: recording {secs:.0} s");
        now
    });
    link.0.session_mut().clip_keep_s(secs + client_net::clip::LEAD_S);
    let over = st.ended || link.0.closed();
    if now - began < secs * 1000.0 && !over {
        return;
    }
    rec.done = true;
    let s = link.0.session();
    let Some(tape) = s.clip_tape(secs + 1.0) else {
        eprintln!("oneshot: --record: nothing was sent to record");
        exit.write(AppExit::from_code(2));
        return;
    };
    match write(&rec.path, &tape) {
        Ok(()) => {
            println!(
                "oneshot: recorded {:.0} s ({} bytes) to {}",
                s.clip_held_s(),
                tape.len(),
                rec.path.display()
            );
            exit.write(AppExit::Success);
        }
        Err(e) => {
            eprintln!("oneshot: --record {}: {e}", rec.path.display());
            exit.write(AppExit::from_code(2));
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn write(path: &std::path::Path, tape: &[u8]) -> Result<(), String> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, tape).map_err(|e| e.to_string())
}

#[cfg(target_arch = "wasm32")]
fn write(_: &std::path::Path, _: &[u8]) -> Result<(), String> {
    Err("a page can't record to a file".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::film::script::{Script, BOOT_TAPE_MS};

    #[test]
    fn every_style_lands_on_the_tape_in_order() {
        for style in STYLES {
            let mut s = Script::parse(&script(style).unwrap()).unwrap();
            let end_s = 64.0;
            s.anchor(end_s);
            let settle_s = s.settle as f64 / s.fps as f64;
            let mut end = 0.0;
            for shot in &s.shots {
                assert!(shot.at + 1e-6 >= end + settle_s, "{style}: `{}` cuts in", shot.name);
                end = shot.at + shot.tape_s(s.fps);
                assert!(end <= end_s + 1e-6, "{style}: `{}` runs off the tape", shot.name);
            }
            assert!(end > end_s - 0.5, "{style} ends at the tape's end");
            // What it asks for is inside what the key keeps.
            assert!(s.shots[0].at >= end_s - CLIP_S - client_net::clip::LEAD_S);
        }
        assert!(script("tiktok").is_none());
        // A tape too short for the style starts where the boot allows.
        let mut s = Script::parse(&script("long").unwrap()).unwrap();
        s.anchor(10.0);
        assert!(s.shots[0].at >= BOOT_TAPE_MS / 1000.0);
    }

    #[test]
    fn the_wide_style_is_the_players_own_view() {
        let s = Script::parse(&script("wide").unwrap()).unwrap();
        assert_eq!((s.width, s.height), (1920, 1080));
        assert!(s.shots.iter().all(|x| x.pov && x.fov.is_none()));
        assert!(s.fov.is_none(), "the player's field of view");
    }
}

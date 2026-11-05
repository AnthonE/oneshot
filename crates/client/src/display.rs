//! The window: fullscreen, vsync and the frame cap, from the settings.
//! After Gates' `render/settings.rs` (`apply_window`, `limit_frames`,
//! `fullscreen_key`) and `render/web.rs` (`browser_fullscreen`,
//! `request_fullscreen`) (MIT, NOTICE).
//!
//! On the desktop these are the window's present mode and mode (borderless,
//! so an alt-tab never changes a video mode) and a sleep at the end of the
//! frame. In a browser the page paces every frame to the display
//! (`requestAnimationFrame`), so VSYNC and FRAME CAP say so on the screen
//! rather than doing nothing, and FULLSCREEN is the page's own (the
//! Fullscreen API): a page may enter it only within a click's few seconds,
//! and the browser leaves it on its own (Esc held, the tab). The row asks
//! when it moves, then follows what the page did; it isn't kept between
//! runs. While the page is fullscreen it asks the browser for Esc
//! (`index.html`, Keyboard Lock), so Esc is still the game's menu.
//!
//! F11 flips FULLSCREEN anywhere: the game's on both, never the browser's
//! (the canvas keeps its keys).

use bevy::prelude::*;
#[cfg(not(target_arch = "wasm32"))]
use bevy::window::{MonitorSelection, PresentMode, PrimaryWindow, WindowMode};

use crate::settings::{Knob, Settings};

/// The key: reserved, so no bind can take it (`binds::RESERVED`).
pub const FULLSCREEN_KEY: KeyCode = KeyCode::F11;

/// Frames a page's fullscreen request may take to land before the row
/// gives up on it and follows the page (a refused request never says so).
pub const PAGE_GRACE_FRAMES: u32 = 60;

pub fn plugin(app: &mut App) {
    // A film's window is its list's size on its own frame clock
    // (`film.rs`): the player's fullscreen and frame cap are not its.
    let playing = not(resource_exists::<crate::film::Film>);
    app.add_systems(Update, (fullscreen_key, apply_window).chain().run_if(playing));
    #[cfg(not(target_arch = "wasm32"))]
    app.init_resource::<FrameDeadline>()
        .add_systems(Last, limit_frames.run_if(not(resource_exists::<crate::film::Film>)));
}

/// F11 flips the row, from any screen; through `adjust`, so an open
/// settings pane redraws it.
pub fn fullscreen_key(keys: Res<ButtonInput<KeyCode>>, mut settings: ResMut<Settings>) {
    if keys.just_pressed(FULLSCREEN_KEY) {
        settings.adjust(Knob::Fullscreen, 0);
    }
}

/// The desktop's window from the settings, written only on a change
/// (a `Window` written every frame is work for the window backend).
#[cfg(not(target_arch = "wasm32"))]
pub fn apply_window(settings: Res<Settings>, mut window: Query<&mut Window, With<PrimaryWindow>>) {
    let Ok(mut w) = window.single_mut() else {
        return;
    };
    let present = if settings.vsync {
        PresentMode::AutoVsync
    } else {
        PresentMode::AutoNoVsync
    };
    if w.present_mode != present {
        w.present_mode = present;
    }
    let mode = if settings.fullscreen {
        WindowMode::BorderlessFullscreen(MonitorSelection::Current)
    } else {
        WindowMode::Windowed
    };
    if w.mode != mode {
        w.mode = mode;
    }
}

/// What the page's fullscreen was last asked to be, and how long ago.
#[derive(Default)]
pub struct PageFull {
    want: bool,
    wait: u32,
}

impl PageFull {
    /// One frame: the row (`setting`) and the page (`page`). Returns what
    /// to ask the page for, if anything; or, with `Err`, what the row
    /// should now say because the page went its own way.
    pub fn step(&mut self, setting: bool, page: bool) -> Result<Option<bool>, bool> {
        if setting != self.want {
            self.want = setting;
            self.wait = PAGE_GRACE_FRAMES;
            return Ok((setting != page).then_some(setting));
        }
        if page == self.want {
            self.wait = 0;
            return Ok(None);
        }
        if self.wait > 0 {
            self.wait -= 1;
            return Ok(None);
        }
        self.want = page;
        Err(page)
    }
}

/// In a browser: the page's fullscreen follows the row when the row moves,
/// and the row follows the page otherwise. Vsync and the cap are the
/// browser's.
#[cfg(target_arch = "wasm32")]
pub fn apply_window(mut settings: ResMut<Settings>, mut st: Local<PageFull>) {
    let Some(page) = page_fullscreen() else {
        return;
    };
    match st.step(settings.fullscreen, page) {
        Ok(Some(on)) => ask_page(on),
        Ok(None) => {}
        Err(now) => settings.fullscreen = now,
    }
}

/// Whether the page has an element in fullscreen (`document.fullscreenElement`);
/// the browser's own F11 fullscreen isn't one.
#[cfg(target_arch = "wasm32")]
fn page_fullscreen() -> Option<bool> {
    web_sys::window()
        .and_then(|w| w.document())
        .map(|d| d.fullscreen_element().is_some())
}

/// The whole page into fullscreen, or out. Refused outside a click's few
/// seconds; the refusal (a rejected promise) is swallowed and the row
/// follows the page again ([`PageFull`]).
#[cfg(target_arch = "wasm32")]
fn ask_page(on: bool) {
    use wasm_bindgen::{closure::Closure, JsCast, JsValue};
    thread_local! {
        static IGNORE: Closure<dyn FnMut(JsValue)> = Closure::new(|_| ());
    }
    let Some(doc) = web_sys::window().and_then(|w| w.document()) else {
        return;
    };
    let (target, call): (JsValue, &str) = if on {
        match doc.document_element() {
            Some(el) => (el.into(), "requestFullscreen"),
            None => return,
        }
    } else {
        (doc.into(), "exitFullscreen")
    };
    let f = js_sys::Reflect::get(&target, &JsValue::from_str(call))
        .ok()
        .and_then(|f| f.dyn_into::<js_sys::Function>().ok());
    if let Some(Ok(p)) = f.map(|f| f.call0(&target)) {
        if let Ok(p) = p.dyn_into::<js_sys::Promise>() {
            IGNORE.with(|f| {
                let _ = p.catch(f);
            });
        }
    }
}

/// Where the frame cap is pacing to; `None` while uncapped.
#[derive(Resource, Default)]
pub struct FrameDeadline(pub Option<bevy::platform::time::Instant>);

/// Hold the desktop's loop to the cap: the last thing a frame does. From
/// the deadline, not from now, so the average holds; after a stall it
/// re-bases rather than sprinting to catch up.
#[cfg(not(target_arch = "wasm32"))]
pub fn limit_frames(settings: Res<Settings>, mut deadline: ResMut<FrameDeadline>) {
    use bevy::platform::time::Instant;
    use std::time::Duration;
    if settings.max_fps == 0 {
        deadline.0 = None;
        return;
    }
    let frame = Duration::from_nanos(1_000_000_000 / u64::from(settings.max_fps));
    let now = Instant::now();
    let target = deadline.0.unwrap_or(now);
    if now < target {
        std::thread::sleep(target - now);
    }
    let after = Instant::now();
    deadline.0 = Some((target + frame).max(after));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The row asks once; while the page catches up nothing more is asked;
    /// a page that left on its own (Esc) takes the row with it; a request
    /// the page never honoured gives up after the grace.
    #[test]
    fn the_page_and_the_row_agree() {
        let mut p = PageFull::default();
        assert_eq!(p.step(false, false), Ok(None));
        assert_eq!(p.step(true, false), Ok(Some(true)), "the click asks");
        for _ in 0..3 {
            assert_eq!(p.step(true, false), Ok(None), "the request is landing");
        }
        assert_eq!(p.step(true, true), Ok(None), "it landed");
        assert_eq!(p.step(true, false), Err(false), "Esc: the row follows the page");
        assert_eq!(p.step(false, false), Ok(None));
        assert_eq!(p.step(true, false), Ok(Some(true)));
        for _ in 0..PAGE_GRACE_FRAMES {
            assert_eq!(p.step(true, false), Ok(None));
        }
        assert_eq!(p.step(true, false), Err(false), "refused: the row says so");
        assert_eq!(p.step(false, false), Ok(None));
        assert_eq!(p.step(true, false), Ok(Some(true)));
        assert_eq!(p.step(true, true), Ok(None));
        assert_eq!(p.step(false, true), Ok(Some(false)), "OFF asks the page out");
        assert_eq!(p.step(false, false), Ok(None), "and it left");
        // The page came on its own (nothing here asked): the row follows it,
        // as it does when the page leaves on its own.
        assert_eq!(
            p.step(false, true),
            Err(true),
            "the page came on its own: the row follows"
        );
    }
}

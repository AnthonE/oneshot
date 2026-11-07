//! The browser's half of the pointer lock, after Gates'
//! `client/src/ui/pointer.rs` and `render/web.rs` (MIT).
//!
//! Bevy never hears that a page let go of the pointer: winit asks for the
//! lock and never listens for `pointerlockchange`, so after Esc (which the
//! browser keeps for itself), alt-tab, or the page's own `exitPointerLock`
//! the window still says `Locked`, hides the cursor, and the next click
//! doesn't lock again. Stepped once a frame with what the window wants and
//! what `document.pointerLockElement` says, [`BrowserLock`] puts the
//! window's grab mode right, so the next click captures again (and is
//! swallowed as a capture, not fired).

use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};

/// Frames a fresh lock request may stay ungranted before it counts as
/// refused. A browser grants within a frame or two; a refusal (no user
/// gesture behind it, or asked again too soon after Esc) never arrives as
/// anything Bevy can see.
pub const LOCK_GRACE_FRAMES: u32 = 30;
/// Frames after a request before a granted lock counts as settled, so the
/// tail of the lock just let go isn't mistaken for the player's Esc.
pub const LOCK_SETTLE_FRAMES: u32 = 4;

/// What [`BrowserLock::step`] found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockFix {
    /// The window's grab mode and the page agree, or may still come to.
    Keep,
    /// The game let go but the page still holds the lock: ask again.
    Unlock,
    /// The lock asked for never came: let the cursor go, so it isn't an
    /// invisible arrow over a view that doesn't turn.
    Refused,
    /// The page held the lock and the player took it back (Esc, the tab
    /// losing focus): the game's Esc.
    Taken,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BrowserLock {
    /// Frames the window has wanted the lock without a break.
    wanting: u32,
    /// Whether the page has held a settled lock since the window asked.
    held: bool,
}

impl BrowserLock {
    pub fn step(&mut self, want: bool, has: bool) -> LockFix {
        if !want {
            self.wanting = 0;
            self.held = false;
            return if has { LockFix::Unlock } else { LockFix::Keep };
        }
        self.wanting = self.wanting.saturating_add(1);
        if has {
            if self.wanting > LOCK_SETTLE_FRAMES {
                self.held = true;
            }
            return LockFix::Keep;
        }
        if self.held {
            LockFix::Taken
        } else if self.wanting > LOCK_GRACE_FRAMES {
            LockFix::Refused
        } else {
            LockFix::Keep
        }
    }
}

/// Whether the page holds the pointer lock (`document.pointerLockElement`);
/// `None` natively, where the window's own grab mode is the truth.
pub fn pointer_locked() -> Option<bool> {
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window()
            .and_then(|w| w.document())
            .map(|d| d.pointer_lock_element().is_some())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        None
    }
}

/// `PostUpdate`: keep the window's grab mode honest about the page's lock.
pub fn follow(
    mut cursor: Query<&mut CursorOptions, With<PrimaryWindow>>,
    mut lock: Local<BrowserLock>,
    screen: Res<State<crate::Screen>>,
    mut next: ResMut<NextState<crate::Screen>>,
    in_match: Option<Res<crate::front::InMatch>>,
) {
    let Some(has) = pointer_locked() else {
        return;
    };
    let Ok(mut c) = cursor.single_mut() else {
        return;
    };
    let want = c.grab_mode == CursorGrabMode::Locked;
    match lock.step(want, has) {
        LockFix::Keep => {}
        // Written though it already reads None: the write is the change
        // Bevy acts on, and acting is `exitPointerLock()` again.
        LockFix::Unlock => c.grab_mode = CursorGrabMode::None,
        LockFix::Refused => {
            c.grab_mode = CursorGrabMode::None;
            c.visible = true;
        }
        // The page kept the player's Esc: it's the game's, and opens the menu.
        LockFix::Taken => {
            c.grab_mode = CursorGrabMode::None;
            c.visible = true;
            // Not over a match ended this frame: the title it is.
            if *screen.get() == crate::Screen::Playing && !crate::front::InMatch::just_left(&in_match) {
                next.set(crate::Screen::Paused);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Esc in a browser: the lock was held, then the page let go while the
    /// window still wants it. That is the player's Esc, once.
    #[test]
    fn a_browser_esc_is_seen() {
        let mut b = BrowserLock::default();
        for _ in 0..2 {
            assert_eq!(b.step(true, false), LockFix::Keep, "the request is pending");
        }
        for _ in 0..10 {
            assert_eq!(b.step(true, true), LockFix::Keep);
        }
        assert_eq!(b.step(true, false), LockFix::Taken);
        assert_eq!(b.step(false, false), LockFix::Keep);
    }

    /// A request never granted gives the cursor back after the grace, and
    /// isn't taken for an Esc.
    #[test]
    fn a_refused_lock_frees_the_cursor() {
        let mut b = BrowserLock::default();
        for _ in 0..LOCK_GRACE_FRAMES {
            assert_eq!(b.step(true, false), LockFix::Keep);
        }
        assert_eq!(b.step(true, false), LockFix::Refused);
    }

    /// The old lock still draining when the new request goes out isn't an
    /// Esc.
    #[test]
    fn the_tail_of_the_last_lock_is_not_an_esc() {
        let mut b = BrowserLock::default();
        assert_eq!(b.step(true, true), LockFix::Keep, "the old lock, still held");
        assert_eq!(b.step(true, false), LockFix::Keep, "its release lands");
        assert_eq!(b.step(true, true), LockFix::Keep, "the new one is granted");
    }

    /// The game let go and the page still holds it: ask again until it does.
    #[test]
    fn a_lock_the_game_let_go_is_let_go() {
        let mut b = BrowserLock::default();
        assert_eq!(b.step(false, true), LockFix::Unlock);
        assert_eq!(b.step(false, false), LockFix::Keep);
    }
}

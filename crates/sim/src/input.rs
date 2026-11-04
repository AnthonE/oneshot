//! What a player sends: one frame a tick. Everything else is the server's
//! (docs/DESIGN.md §4).

use crate::consts::PITCH_MAX;

pub const FIRE: u16 = 1 << 0;
pub const JUMP: u16 = 1 << 1;
pub const CROUCH: u16 = 1 << 2;
pub const MELEE: u16 = 1 << 3;
pub const GRENADE: u16 = 1 << 4;
pub const RELOAD: u16 = 1 << 5;
pub const SWAP: u16 = 1 << 6;
/// Pick up the gun on the pad you stand on.
pub const USE: u16 = 1 << 7;
/// Scope in. Drawing only; the sim ignores it.
pub const ZOOM: u16 = 1 << 8;
/// The left hand's trigger, with a gun in each hand.
pub const FIRE_LEFT: u16 = 1 << 9;
/// Frags or stickies for the next throw.
pub const GREN_SWITCH: u16 = 1 << 10;
/// Buttons on the wire, and the bits they take there.
pub const BUTTON_BITS: u32 = 11;
pub const BUTTONS: u16 = (1 << BUTTON_BITS) - 1;
/// Buttons that act on the tick they go down, not while held.
pub const TAPS: u16 = MELEE | GRENADE | RELOAD | SWAP | USE | GREN_SWITCH;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Input {
    pub buttons: u16,
    /// 1/65536 turn; 0 faces +x, growing toward +z.
    pub yaw: u16,
    /// 1/65536 turn, up is positive, within ±[`PITCH_MAX`].
    pub pitch: i16,
    /// Stick, right positive.
    pub mx: i8,
    /// Stick, forward positive.
    pub mz: i8,
}

impl Input {
    /// What the server will act on: unknown buttons cleared, pitch and stick
    /// in range. Never trusts a frame as it arrives.
    pub fn sanitized(self) -> Input {
        Input {
            buttons: self.buttons & BUTTONS,
            yaw: self.yaw,
            pitch: self.pitch.clamp(-PITCH_MAX, PITCH_MAX),
            mx: self.mx.max(-127),
            mz: self.mz.max(-127),
        }
    }

    /// The frame repeated for a tick that never arrived: held buttons and
    /// aim kept, taps cleared.
    pub fn repeated(self) -> Input {
        Input {
            buttons: self.buttons & !TAPS,
            ..self
        }
    }

    pub fn held(&self, b: u16) -> bool {
        self.buttons & b != 0
    }
}

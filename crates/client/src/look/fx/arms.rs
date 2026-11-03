//! Each gun's look when it fires: its tracer, its flash, what its round
//! does to what it meets. Keyed by `sim::arms` ids; looks only.

use sim::arms::{AR, BR, MAGNUM, SHOTGUN, SMG, SNIPER};

use super::Tone;

#[derive(Clone, Copy)]
pub struct ArmFx {
    /// The tracer's colour.
    pub tone: Tone,
    /// Every how many rounds a tracer is drawn (1: each).
    pub every: u16,
    /// The bolt that runs down the line: its length (m), speed (m/s) and
    /// width (m).
    pub bolt_m: f32,
    pub bolt_mps: f32,
    pub width: f32,
    /// The line left behind, its peak alpha and how long it lasts (s).
    pub line_alpha: f32,
    pub line_s: f32,
    /// A smoke wake along the line that hangs this long (s); 0: none.
    pub wake_s: f32,
    /// The third-person flash's size (m) and its light (lumens).
    pub flash_m: f32,
    pub flash_lm: f32,
    /// Sparks and dust at the far end, and the size of the mark.
    pub sparks: u8,
    pub puff_m: f32,
    pub mark_m: f32,
}

const BASE: ArmFx = ArmFx {
    tone: Tone::Tracer,
    every: 1,
    bolt_m: 2.4,
    bolt_mps: 900.0,
    width: 0.03,
    line_alpha: 0.35,
    line_s: 0.07,
    wake_s: 0.0,
    flash_m: 0.45,
    flash_lm: 120_000.0,
    sparks: 4,
    puff_m: 0.45,
    mark_m: 0.11,
};

pub fn of(arm: u8) -> ArmFx {
    match arm {
        AR => ArmFx {
            every: 2,
            line_alpha: 0.25,
            sparks: 3,
            ..BASE
        },
        SMG => ArmFx {
            tone: Tone::TracerBlue,
            every: 2,
            bolt_m: 1.8,
            width: 0.025,
            line_alpha: 0.2,
            flash_m: 0.35,
            sparks: 3,
            puff_m: 0.35,
            mark_m: 0.09,
            ..BASE
        },
        BR => ArmFx {
            tone: Tone::TracerBlue,
            bolt_m: 3.0,
            line_alpha: 0.3,
            sparks: 4,
            ..BASE
        },
        MAGNUM => ArmFx {
            bolt_m: 2.8,
            width: 0.035,
            flash_m: 0.5,
            sparks: 5,
            puff_m: 0.55,
            mark_m: 0.13,
            ..BASE
        },
        SNIPER => ArmFx {
            tone: Tone::TracerRed,
            every: 1,
            bolt_m: 6.0,
            bolt_mps: 1400.0,
            width: 0.05,
            line_alpha: 0.7,
            line_s: 0.25,
            wake_s: 1.4,
            flash_m: 0.8,
            flash_lm: 260_000.0,
            sparks: 9,
            puff_m: 0.9,
            mark_m: 0.18,
        },
        SHOTGUN => ArmFx {
            every: 3,
            bolt_m: 1.2,
            bolt_mps: 700.0,
            width: 0.02,
            line_alpha: 0.15,
            line_s: 0.05,
            flash_m: 0.75,
            flash_lm: 220_000.0,
            sparks: 2,
            puff_m: 0.3,
            mark_m: 0.07,
            ..BASE
        },
        _ => BASE,
    }
}

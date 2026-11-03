//! The effects' materials, made once: each tone in `STEPS` alpha steps, so
//! a fade swaps a handle and nothing is made or dropped per effect. Every
//! one is unlit and soft-edged (one radial texture), additive for what
//! glows and blended for what doesn't.

use bevy::asset::RenderAssetUsages;
use bevy::image::Image;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

/// Alpha steps per tone: fine enough that a 3 s smoke fade doesn't step.
pub const STEPS: usize = 16;
/// The soft disc's side, in texels.
const DISC: u32 = 64;

/// What an effect is coloured. Glows are additive (`Add`) and may run past
/// 1.0 so the bloom catches them; the rest are blended.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tone {
    /// White-hot: a spark's head, a flash's core.
    Hot,
    /// Gun sparks off a wall.
    Spark,
    /// A muzzle's flash.
    Muzzle,
    /// Tracers, by family (`arms.rs`).
    Tracer,
    TracerBlue,
    TracerRed,
    /// Fire in a blast.
    Fire,
    /// A blast's ring along the ground.
    Shock,
    /// A shield taking a round: Halo gold.
    Shield,
    /// Smoke: a blast's, a sniper round's wake.
    Smoke,
    /// Dust kicked off the ground.
    Dirt,
    /// Dust off a box.
    Grit,
    /// A round into health.
    Hurt,
    /// A bullet's mark, a blast's scorch.
    Mark,
}

impl Tone {
    pub const ALL: [Tone; 14] = [
        Tone::Hot,
        Tone::Spark,
        Tone::Muzzle,
        Tone::Tracer,
        Tone::TracerBlue,
        Tone::TracerRed,
        Tone::Fire,
        Tone::Shock,
        Tone::Shield,
        Tone::Smoke,
        Tone::Dirt,
        Tone::Grit,
        Tone::Hurt,
        Tone::Mark,
    ];

    /// Its colour (linear; glows past 1.0) and whether it adds light.
    fn look(self) -> (LinearRgba, bool) {
        let c = |r, g, b| LinearRgba::rgb(r, g, b);
        match self {
            Tone::Hot => (c(3.2, 2.9, 2.4), true),
            Tone::Spark => (c(3.0, 1.5, 0.4), true),
            Tone::Muzzle => (c(3.4, 2.1, 0.7), true),
            Tone::Tracer => (c(2.4, 1.7, 0.5), true),
            Tone::TracerBlue => (c(0.6, 1.4, 3.0), true),
            Tone::TracerRed => (c(3.0, 0.6, 0.35), true),
            Tone::Fire => (c(2.6, 1.0, 0.22), true),
            Tone::Shock => (c(1.2, 1.0, 0.8), true),
            Tone::Shield => (c(2.8, 2.0, 0.45), true),
            Tone::Smoke => (c(0.16, 0.155, 0.15), false),
            Tone::Dirt => (c(0.30, 0.22, 0.14), false),
            Tone::Grit => (c(0.42, 0.41, 0.39), false),
            Tone::Hurt => (c(0.45, 0.05, 0.04), false),
            Tone::Mark => (c(0.02, 0.018, 0.016), false),
        }
    }

    fn index(self) -> usize {
        Tone::ALL.iter().position(|t| *t == self).unwrap_or(0)
    }
}

pub struct Pool {
    mats: Vec<[Handle<StandardMaterial>; STEPS]>,
}

impl Pool {
    pub fn new(mats: &mut Assets<StandardMaterial>, images: &mut Assets<Image>) -> Pool {
        let disc = images.add(soft_disc());
        let mats = Tone::ALL
            .iter()
            .map(|tone| {
                let (c, add) = tone.look();
                core::array::from_fn(|k| {
                    let a = (k + 1) as f32 / STEPS as f32;
                    mats.add(StandardMaterial {
                        base_color: Color::LinearRgba(LinearRgba { alpha: a, ..c }),
                        base_color_texture: Some(disc.clone()),
                        unlit: true,
                        alpha_mode: if add { AlphaMode::Add } else { AlphaMode::Blend },
                        cull_mode: None,
                        // A mark lies on its wall; nothing else minds.
                        depth_bias: if *tone == Tone::Mark { 8.0 } else { 0.0 },
                        ..default()
                    })
                })
            })
            .collect();
        Pool { mats }
    }

    /// The material for `tone` at alpha step `step` (1..=STEPS).
    pub fn get(&self, tone: Tone, step: usize) -> Handle<StandardMaterial> {
        self.mats[tone.index()][step.clamp(1, STEPS) - 1].clone()
    }
}

/// The step for an alpha in 0..=1: 0 is gone.
pub fn step(alpha: f32) -> usize {
    (alpha.clamp(0.0, 1.0) * STEPS as f32).ceil() as usize
}

/// White, its alpha a soft round falloff: every effect's shape.
fn soft_disc() -> Image {
    let n = DISC as usize;
    let mut data = Vec::with_capacity(n * n * 4);
    for y in 0..n {
        for x in 0..n {
            let dx = (x as f32 + 0.5) / n as f32 * 2.0 - 1.0;
            let dy = (y as f32 + 0.5) / n as f32 * 2.0 - 1.0;
            let r = (dx * dx + dy * dy).sqrt();
            let a = (1.0 - r).clamp(0.0, 1.0);
            // Bright through the middle, a long soft edge.
            let a = a * a * (3.0 - 2.0 * a);
            data.extend_from_slice(&[255, 255, 255, (a * 255.0) as u8]);
        }
    }
    Image::new(
        Extent3d {
            width: DISC,
            height: DISC,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

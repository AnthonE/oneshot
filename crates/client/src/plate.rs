//! The HUD's plates: angular panels with corners cut at 45°, a bright edge,
//! a soft glow outside it and a fill that fades from top to bottom, the
//! look of the arena shooters' HUDs. Drawn here into small white images at
//! startup (no files), nine-sliced so a plate stretches to any box without
//! its corners or edge stretching, and tinted by the node that wears one:
//! lit or dim, a side's colour. Compiled in, as the fonts are (`ui.rs`).

use bevy::asset::{uuid_handle, RenderAssetUsages};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::sprite::{BorderRect, SliceScaleMode, TextureSlicer};

/// The smallest a plate may be drawn, px: two of its corners. A bar is
/// [`BAR`]'s, any other [`BOX`]'s.
pub const BAR_MIN: f32 = 2.0 * (GLOW + CUT_BAR + EDGE);
pub const BOX_MIN: f32 = 2.0 * (GLOW + CUT + EDGE);

/// The plates by shape. A box cuts its top right and bottom left corners;
/// a right-hand box the other two; a fill is the box's inside alone, no
/// edge or glow (a bar's level, a swatch).
pub const BOX: Handle<Image> = uuid_handle!("2f6b4c1e-8a3d-4e57-b0c9-7d1e5a3f9b01");
pub const BOX_R: Handle<Image> = uuid_handle!("2f6b4c1e-8a3d-4e57-b0c9-7d1e5a3f9b02");
pub const BAR: Handle<Image> = uuid_handle!("2f6b4c1e-8a3d-4e57-b0c9-7d1e5a3f9b03");
pub const FILL: Handle<Image> = uuid_handle!("2f6b4c1e-8a3d-4e57-b0c9-7d1e5a3f9b04");

/// The image's side, its glow's reach outside the edge, the edge's width,
/// and a corner's cut, px.
const SIDE: u32 = 64;
const GLOW: f32 = 4.0;
const EDGE: f32 = 2.0;
const CUT: f32 = 13.0;
const CUT_BAR: f32 = 8.0;
const CUT_FILL: f32 = 6.0;
/// The fill's opacity at its top and bottom, and the glow's at the edge.
const FILL_TOP: f32 = 0.5;
const FILL_BOTTOM: f32 = 0.24;
const GLOW_A: f32 = 0.45;

/// A plate's corners: each one's cut (0: square), top left, top right,
/// bottom right, bottom left.
#[derive(Clone, Copy)]
struct Shape {
    cuts: [f32; 4],
    edged: bool,
}

/// Distance from the plate's outline, negative inside: a box `h` half wide
/// about the centre, each corner cut by a 45° plane.
fn sdf(p: Vec2, h: Vec2, cuts: [f32; 4]) -> f32 {
    let q = p.abs() - h;
    let mut d = q.max(Vec2::ZERO).length() + q.x.max(q.y).min(0.0);
    // Image rows run down: the top is -y.
    let corners = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];
    for (c, (sx, sy)) in cuts.iter().zip(corners) {
        if *c > 0.0 {
            let plane = (sx * p.x - h.x + sy * p.y - h.y + c) / core::f32::consts::SQRT_2;
            d = d.max(plane);
        }
    }
    d
}

fn smooth(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The plate as white with the alpha that draws it.
fn draw(s: Shape) -> Image {
    let n = SIDE as usize;
    let inset = if s.edged { GLOW } else { 0.0 };
    let h = Vec2::splat(SIDE as f32 / 2.0 - inset);
    let mut data = vec![255u8; n * n * 4];
    for y in 0..n {
        for x in 0..n {
            let p = Vec2::new(x as f32 + 0.5, y as f32 + 0.5) - Vec2::splat(SIDE as f32 / 2.0);
            let d = sdf(p, h, s.cuts);
            let v = (y as f32 + 0.5 - inset) / (h.y * 2.0);
            let fill = FILL_TOP + (FILL_BOTTOM - FILL_TOP) * v.clamp(0.0, 1.0);
            let inside = 1.0 - smooth(-0.5, 0.5, d);
            let a = if s.edged {
                // The edge, a light just inside it, and the glow outside.
                let edge = smooth(-EDGE - 0.75, -EDGE + 0.25, d) * inside;
                let sheen = 0.3 * (-(-d - EDGE).max(0.0) / 3.0).exp() * inside;
                let glow = GLOW_A * (-d.max(0.0) / 1.6).exp() * (1.0 - inside);
                (fill * inside).max(edge).max(sheen + fill * inside).max(glow)
            } else {
                // A fill: solid, brightest at its top.
                (0.75 + 0.25 * (1.0 - v)) * inside
            };
            data[(y * n + x) * 4 + 3] = (a.clamp(0.0, 1.0) * 255.0) as u8;
        }
    }
    Image::new(
        Extent3d {
            width: SIDE,
            height: SIDE,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

/// The plates into `Assets<Image>` as the app is built.
pub fn plugin(app: &mut App) {
    let shapes = [
        (BOX.id(), [0.0, CUT, 0.0, CUT], true),
        (BOX_R.id(), [CUT, 0.0, CUT, 0.0], true),
        (BAR.id(), [0.0, CUT_BAR, 0.0, CUT_BAR], true),
        (FILL.id(), [0.0, CUT_FILL, 0.0, CUT_FILL], false),
    ];
    let mut images = app.world_mut().resource_mut::<Assets<Image>>();
    for (id, cuts, edged) in shapes {
        images
            .insert(id, draw(Shape { cuts, edged }))
            .expect("a plate could not be registered");
    }
}

/// How a plate is cut into nine: its corners whole, its sides and middle
/// stretched.
fn slicer(plate: &Handle<Image>) -> TextureSlicer {
    let border = if *plate == FILL {
        CUT_FILL + 1.0
    } else if *plate == BAR {
        GLOW + CUT_BAR + EDGE
    } else {
        GLOW + CUT + EDGE
    };
    TextureSlicer {
        border: BorderRect::all(border),
        center_scale_mode: SliceScaleMode::Stretch,
        sides_scale_mode: SliceScaleMode::Stretch,
        max_corner_scale: 1.0,
    }
}

/// A plate behind a node's content: an absolute child filling it, first
/// so it draws under the rest, tinted `color`.
pub fn behind(plate: Handle<Image>, color: Color) -> impl Bundle {
    let mode = NodeImageMode::Sliced(slicer(&plate));
    (
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        ImageNode {
            image: plate,
            color,
            image_mode: mode,
            ..default()
        },
    )
}

/// A plate as the node itself (a bar's fill): sliced, tinted `color`.
pub fn image(plate: Handle<Image>, color: Color) -> ImageNode {
    let mode = NodeImageMode::Sliced(slicer(&plate));
    ImageNode {
        image: plate,
        color,
        image_mode: mode,
        ..default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cut_corner_is_outside_and_the_middle_inside() {
        let h = Vec2::splat(28.0);
        let cuts = [0.0, CUT, 0.0, CUT];
        // Top right is cut, top left square.
        assert!(sdf(Vec2::new(27.0, -27.0), h, cuts) > 0.0);
        assert!(sdf(Vec2::new(-27.0, -27.0), h, cuts) < 0.0);
        assert!(sdf(Vec2::ZERO, h, cuts) < -20.0);
    }
}

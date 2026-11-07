//! The ship's surfaces, ours and procedural (`textures::bake`): the hull's
//! pearl-purple plates, the deck's dark grating and the windows' honeycomb
//! glass. Each is a seamless tile: a height field (for the normal map and
//! the occlusion), a colour, and where it glows. The looks tint them
//! (`level::dress`), and the glow takes the look's colour.

use super::textures::{bake, hash2, Texel};
use bevy::prelude::*;

/// The hull's texels a side (its plates are big: a tile is 6 m).
pub const HULL_PX: usize = 1024;

/// A rounded rectangle's inward distance from its edge: positive inside,
/// for a point at (`u`, `v`) in a `w` × `h` rectangle with corners of
/// radius `r`.
fn inset(u: f32, v: f32, w: f32, h: f32, r: f32) -> f32 {
    let qx = (u - w * 0.5).abs() - (w * 0.5 - r);
    let qy = (v - h * 0.5).abs() - (h * 0.5 - r);
    let outside = Vec2::new(qx.max(0.0), qy.max(0.0)).length();
    let inside = qx.max(qy).min(0.0);
    r - (outside + inside)
}

/// Pointy-top hexagons over the unit tile, `cols` across and `rows` (even)
/// down: the hex metric at (`u`, `v`), 0 at a cell's middle and 1 on its
/// edge, with its cell's lattice key.
fn hex(u: f32, v: f32, cols: usize, rows: usize) -> (f32, (i32, i32)) {
    let w = 1.0 / cols as f32;
    let pitch = 1.0 / rows as f32;
    let r = pitch / 1.5;
    let j0 = (v / pitch).floor() as i32;
    let mut best = (f32::MAX, (0, 0));
    for j in j0 - 1..=j0 + 1 {
        // The stagger by the row's place in the tile (rows is even, so
        // the pattern wraps).
        let off = if j.rem_euclid(rows as i32) % 2 == 1 {
            w * 0.5
        } else {
            0.0
        };
        let i = ((u - off) / w).round() as i32;
        let cx = off + i as f32 * w;
        let cy = j as f32 * pitch;
        let du = ((u - cx + 0.5).rem_euclid(1.0) - 0.5).abs();
        let dv = ((v - cy + 0.5).rem_euclid(1.0) - 0.5).abs();
        let d = (du / (w * 0.5)).max((du / (w * 0.5) + 2.0 * dv / r) * 0.5);
        if d < best.0 {
            best = (d, (i.rem_euclid(cols as i32), j.rem_euclid(rows as i32)));
        }
    }
    best
}

/// Smooth value noise over the unit tile, `cells` a side, seamless.
fn noise(u: f32, v: f32, cells: usize, k: u32) -> f32 {
    let (x, y) = (u.rem_euclid(1.0) * cells as f32, v.rem_euclid(1.0) * cells as f32);
    let (x0, y0) = (x.floor() as usize, y.floor() as usize);
    let (sx, sy) = (smooth(0.0, 1.0, x - x0 as f32), smooth(0.0, 1.0, y - y0 as f32));
    let h = |i: usize, j: usize| hash2(i % cells, j % cells, k);
    let a = h(x0, y0) + (h(x0 + 1, y0) - h(x0, y0)) * sx;
    let b = h(x0, y0 + 1) + (h(x0 + 1, y0 + 1) - h(x0, y0 + 1)) * sx;
    a + (b - a) * sy
}

/// Three octaves of [`noise`], 0..1: smudges a plate across, and finer.
fn fbm(u: f32, v: f32, k: u32) -> f32 {
    0.5 * noise(u, v, 6, k) + 0.3 * noise(u, v, 20, k + 1) + 0.2 * noise(u, v, 64, k + 2)
}

fn smooth(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The hull's plates: two rows a tile of broad flat plates of uneven
/// width, a thin dark seam and a short bevel between them, and in some a
/// sunk panel of honeycomb or a line of light along the bottom edge; a
/// plain or grooved plate carries a line inlaid round it, inside its edge.
/// Near white, the look's tint is the colour: each plate a little bluer or
/// pinker than the next (the hull's sheen), its bevel pinker, its face
/// smudged in value and in gloss so a lamp's highlight breaks up on it.
pub fn hull_images(n: usize) -> [Image; 4] {
    // Each row's plates across the tile, as fractions of its width, and
    // where the row starts.
    const ROWS: [(&[f32], f32); 2] = [(&[0.38, 0.24, 0.38], 0.0), (&[0.2, 0.35, 0.25, 0.2], 0.57)];
    const ROW_H: f32 = 0.5;
    const SEAM: f32 = 0.006;
    const BEVEL: f32 = 0.022;
    const CORNER: f32 = 0.03;
    const PANEL_IN: f32 = 0.09;
    const LINE_W: f32 = 0.005;
    // The inlaid line: how far inside the plate's edge, its corners'
    // radius, its half width.
    const INLAY_IN: f32 = 0.045;
    const INLAY_CORNER: f32 = 0.06;
    const INLAY_W: f32 = 0.0035;
    let nf = n as f32;
    // A texel's plate: its row, its plate, the plate's width and the
    // inward distance from the plate's edge, in tile units.
    let plate = |u: f32, v: f32| -> (usize, usize, f32, f32, f32, f32) {
        let j = ((v / ROW_H) as usize).min(ROWS.len() - 1);
        let (widths, off) = ROWS[j];
        let lu = (u - off).rem_euclid(1.0);
        let (mut i, mut start) = (0, 0.0);
        for (k, w) in widths.iter().enumerate() {
            if lu < start + w || k == widths.len() - 1 {
                i = k;
                break;
            }
            start += w;
        }
        let w = widths[i];
        let (pu, pv) = (lu - start, v - j as f32 * ROW_H);
        (j, i, w, pu, pv, inset(pu, pv, w, ROW_H, CORNER))
    };
    // What a plate carries: 0 plain, 1 a sunk honeycomb panel, 2 a line of
    // light along its bottom, 3 a groove across its middle.
    let kind = |j: usize, i: usize| -> usize {
        let r = hash2(i + 1, j + 3, 23);
        if r < 0.16 {
            1
        } else if r < 0.34 {
            2
        } else if r < 0.52 {
            3
        } else {
            0
        }
    };
    let bevel = |d: f32| smooth(SEAM, SEAM + BEVEL, d);
    // The sunk panel: 1 inside it, soft at its edge; and the honeycomb in
    // it.
    let panel = |u: f32, v: f32, j: usize, i: usize, w: f32, pu: f32, pv: f32| -> (f32, f32) {
        if kind(j, i) != 1 {
            return (0.0, 0.0);
        }
        let d = inset(
            pu - PANEL_IN,
            pv - PANEL_IN,
            w - 2.0 * PANEL_IN,
            ROW_H - 2.0 * PANEL_IN,
            0.015,
        );
        let sunk = smooth(-0.004, 0.004, d);
        let (hd, _) = hex(u, v, 48, 56);
        (sunk, sunk * smooth(0.9, 0.8, hd))
    };
    let groove = |j: usize, i: usize, w: f32, pu: f32, pv: f32| -> f32 {
        if kind(j, i) != 3 {
            return 0.0;
        }
        let ends = smooth(0.04, 0.05, pu) * smooth(0.04, 0.05, w - pu);
        ends * smooth(0.006, 0.003, (pv - ROW_H * 0.38).abs())
    };
    // The line is never thinner than a texel, so a small bake has it too.
    let lw = LINE_W.max(0.9 / nf);
    let line = |j: usize, i: usize, w: f32, pu: f32, pv: f32| -> f32 {
        if kind(j, i) != 2 {
            return 0.0;
        }
        let ends = smooth(0.05, 0.06, pu) * smooth(0.05, 0.06, w - pu);
        ends * smooth(lw, lw * 0.6, (pv - (SEAM + BEVEL + 0.012)).abs())
    };
    let iw = INLAY_W.max(0.8 / nf);
    let inlay = |j: usize, i: usize, w: f32, pu: f32, pv: f32| -> f32 {
        if !matches!(kind(j, i), 0 | 3) {
            return 0.0;
        }
        let d = inset(
            pu - INLAY_IN,
            pv - INLAY_IN,
            w - 2.0 * INLAY_IN,
            ROW_H - 2.0 * INLAY_IN,
            INLAY_CORNER,
        );
        smooth(iw, iw * 0.3, d.abs())
    };
    let height = |x: usize, y: usize| -> f32 {
        let (u, v) = (x as f32 / nf, y as f32 / nf);
        let (j, i, w, pu, pv, d) = plate(u, v);
        let (sunk, cells) = panel(u, v, j, i, w, pu, pv);
        bevel(d)
            - 0.25 * sunk
            - 0.08 * cells
            - 0.3 * groove(j, i, w, pu, pv)
            - 0.2 * line(j, i, w, pu, pv)
            - 0.12 * inlay(j, i, w, pu, pv)
    };
    let blue = Vec3::new(0.84, 0.88, 1.0);
    let pink = Vec3::new(1.0, 0.86, 0.97);
    let edge = Vec3::new(1.0, 0.84, 0.95);
    let seam = Vec3::new(0.05, 0.04, 0.07);
    let texel = |x: usize, y: usize, _h: f32| -> Texel {
        let (u, v) = (x as f32 / nf, y as f32 / nf);
        let (j, i, w, pu, pv, d) = plate(u, v);
        let b = bevel(d);
        let (sunk, cells) = panel(u, v, j, i, w, pu, pv);
        let g = groove(j, i, w, pu, pv);
        let glow = line(j, i, w, pu, pv);
        let inl = inlay(j, i, w, pu, pv);
        let shade = 0.86 + 0.14 * hash2(i, j, 11);
        let smudge = fbm(u, v, 5);
        let face = blue.lerp(pink, hash2(i + 5, j, 41));
        // A plate's face a little lighter toward its top, as a curved
        // plate catches more of the vault.
        let sheen = 0.94 + 0.08 * (pv / ROW_H);
        let mut col = edge.lerp(face, b) * shade * sheen * (0.72 + 0.45 * smudge);
        col *= 1.0 - 0.25 * sunk - 0.1 * cells - 0.5 * g - 0.45 * inl;
        col = seam.lerp(col, smooth(0.0, 0.6, b));
        if glow > 0.5 {
            col = Vec3::new(0.7, 0.7, 0.8);
        }
        Texel {
            albedo: col.to_array(),
            ao: (0.45 + 0.55 * b) * (1.0 - 0.3 * sunk - 0.4 * g - 0.35 * inl),
            rough: 0.3 + 0.3 * smudge + 0.24 * (1.0 - b) + 0.1 * sunk + 0.1 * cells + 0.2 * inl,
            metal: 1.0,
            glow,
        }
    };
    bake(n, 5.0, &height, &texel)
}

/// The deck's texels a side (a tile is 8 m: four plates of 2 m).
pub const DECK_PX: usize = 512;

/// The deck: plates of 2 m, each a raised border round a sunk panel with
/// its corners cut. Some panels are ribbed (three low ribs along them, one
/// way or the other), some plain, and about one plate in four a grate of
/// long slots lit violet from under the deck. Dark purple metal, its gloss
/// and value smudged so a lamp's highlight breaks up across a plate.
pub fn deck_images(n: usize) -> [Image; 4] {
    const PLATES: usize = 4;
    const SEAM: f32 = 0.002;
    const FILLET: f32 = 0.004;
    // The border round a plate's panel, and the panel's cut corners, in
    // tile units.
    const BORDER: f32 = 0.022;
    const CHAMFER: f32 = 0.05;
    let nf = n as f32;
    let pw = 1.0 / PLATES as f32;
    let plate = |u: f32, v: f32| -> (usize, usize, f32, f32, f32) {
        let (i, j) = ((u / pw) as usize % PLATES, (v / pw) as usize % PLATES);
        let (lu, lv) = (u.rem_euclid(pw), v.rem_euclid(pw));
        (i, j, lu, lv, inset(lu, lv, pw, pw, 0.006))
    };
    // How far inside the panel (an octagon: the plate less its border,
    // its corners cut), tile units; negative on the border.
    let octagon = |lu: f32, lv: f32| -> f32 {
        let half = pw * 0.5 - BORDER;
        let (qx, qy) = ((lu - pw * 0.5).abs(), (lv - pw * 0.5).abs());
        (half - qx)
            .min(half - qy)
            .min((2.0 * half - CHAMFER - qx - qy) * core::f32::consts::FRAC_1_SQRT_2)
    };
    // 0 plain, 1 ribbed along u, 2 ribbed along v, 3 a grate.
    let kind = |i: usize, j: usize| -> usize {
        let r = hash2(i + 7, j + 1, 31);
        if r < 0.25 {
            3
        } else if r < 0.45 {
            1
        } else if r < 0.65 {
            2
        } else {
            0
        }
    };
    let panel = |d: f32| smooth(0.0, 0.004, d);
    // 1 on a rib.
    let rib = |k: usize, lu: f32, lv: f32| -> f32 {
        let t = match k {
            1 => lv,
            2 => lu,
            _ => return 0.0,
        };
        let f = (t / pw * 4.0).rem_euclid(1.0);
        if (t / pw) < 0.15 || (t / pw) > 0.85 {
            return 0.0;
        }
        smooth(0.12, 0.06, (f - 0.5).abs())
    };
    // 1 in a slot.
    let slot = |k: usize, lu: f32, lv: f32, d: f32| -> f32 {
        if k != 3 || d < 0.006 {
            return 0.0;
        }
        let fv = (lv / (pw / 20.0)).rem_euclid(1.0);
        let fu = (lu / (pw / 4.0)).rem_euclid(1.0);
        if (0.36..0.64).contains(&fv) && (0.08..0.92).contains(&fu) {
            1.0
        } else {
            0.0
        }
    };
    let height = |x: usize, y: usize| -> f32 {
        let (u, v) = (x as f32 / nf, y as f32 / nf);
        let (i, j, lu, lv, d) = plate(u, v);
        let e = ((d - SEAM) / FILLET).clamp(0.0, 1.0);
        let top = e * (1.0 - 0.5 * (1.0 - e));
        let k = kind(i, j);
        let o = octagon(lu, lv);
        let sunk = panel(o);
        top - 0.12 * sunk + 0.15 * rib(k, lu, lv) * sunk - 0.6 * slot(k, lu, lv, o) + 0.015 * noise(u, v, 128, 53)
    };
    let border = Vec3::new(0.05, 0.036, 0.08);
    let inner = Vec3::new(0.07, 0.045, 0.11);
    let bars = Vec3::new(0.03, 0.026, 0.045);
    let hole = Vec3::new(0.01, 0.006, 0.02);
    let texel = |x: usize, y: usize, h: f32| -> Texel {
        let (u, v) = (x as f32 / nf, y as f32 / nf);
        let (i, j, lu, lv, _) = plate(u, v);
        let k = kind(i, j);
        let o = octagon(lu, lv);
        let sunk = panel(o);
        let s = slot(k, lu, lv, o);
        let r = rib(k, lu, lv) * sunk;
        let smudge = fbm(u, v, 13);
        let shade = (0.88 + 0.12 * hash2(i, j, 3)) * (0.92 + 0.16 * smudge);
        let edge = smooth(0.0, 0.6, h.clamp(0.0, 1.0) + 0.3 * sunk);
        let top = if k == 3 {
            bars
        } else {
            border.lerp(inner, sunk) * (1.0 + 0.25 * r)
        };
        let col = Vec3::ZERO.lerp(top * shade, edge.max(sunk));
        let col = col.lerp(hole, s);
        Texel {
            albedo: col.to_array(),
            ao: if s > 0.5 {
                0.25
            } else {
                (0.75 + 0.25 * edge) * (1.0 - 0.15 * sunk)
            },
            rough: if s > 0.5 {
                0.9
            } else {
                0.5 + 0.14 * smudge + 0.1 * sunk - 0.08 * r
            },
            metal: if k == 3 { 1.0 } else { 0.9 - 0.2 * sunk },
            // Light from under the deck through a grate's slots.
            glow: s * (0.55 + 0.25 * noise(u, v, 4, 19)),
        }
    };
    bake(n, 6.0, &height, &texel)
}

/// The windows: a fine honeycomb of amber glass in dark frames, lit from
/// far behind, so it swells and dims across a pane rather than glowing
/// flat, each cell a
/// little domed and lit from behind, brightest at its middle.
pub fn gold_images(n: usize) -> [Image; 4] {
    const COLS: usize = 13;
    const ROWS: usize = 16;
    const FRAME: f32 = 0.84;
    // The light behind the glass: this share is steady, the rest wanders.
    const STEADY: f32 = 0.55;
    let nf = n as f32;
    let height = |x: usize, y: usize| -> f32 {
        let (u, v) = (x as f32 / nf, y as f32 / nf);
        let (d, _) = hex(u, v, COLS, ROWS);
        if d < FRAME {
            1.0 - 0.3 * (d / FRAME) * (d / FRAME)
        } else {
            0.55
        }
    };
    let glass = Vec3::new(0.9, 0.5, 0.12);
    let deep = Vec3::new(0.42, 0.16, 0.05);
    let frame = Vec3::new(0.03, 0.022, 0.018);
    let texel = |x: usize, y: usize, _h: f32| -> Texel {
        let (u, v) = (x as f32 / nf, y as f32 / nf);
        let (d, cell) = hex(u, v, COLS, ROWS);
        let pane = d < FRAME;
        let flicker = 0.7 + 0.3 * hash2(cell.0 as usize, cell.1 as usize, 17);
        let t = (d / FRAME).min(1.0);
        // How much light is behind this cell: broad drifts over the tile.
        let drift = smooth(0.25, 0.75, noise(u, v, 2, 29) * 0.75 + noise(u, v, 5, 31) * 0.25);
        let lit = STEADY + (1.0 - STEADY) * drift;
        Texel {
            albedo: if pane {
                (deep.lerp(glass, lit) * (1.0 - 0.3 * t)).to_array()
            } else {
                frame.to_array()
            },
            ao: if pane { 1.0 } else { 0.7 },
            rough: if pane { 0.15 } else { 0.5 },
            metal: if pane { 0.0 } else { 1.0 },
            glow: if pane {
                (1.0 - 0.75 * t * t) * flicker * lit * lit
            } else {
                0.0
            },
        }
    };
    bake(n, 6.0, &height, &texel)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hexagon_is_one_at_its_edge_and_tiles() {
        // The middle of cell (0, 0) and the far side of the tile agree.
        let (d0, _) = hex(0.0, 0.0, 7, 8);
        let (d1, _) = hex(1.0, 1.0, 7, 8);
        assert!(d0 < 1e-3 && d1 < 1e-3, "{d0} {d1}");
        // Half way to the next column is an edge.
        let (d, _) = hex(0.5 / 7.0, 0.0, 7, 8);
        assert!((d - 1.0).abs() < 1e-3, "{d}");
    }

    #[test]
    fn the_inset_is_the_distance_in() {
        assert!((inset(0.5, 0.5, 1.0, 1.0, 0.1) - 0.5).abs() < 1e-6);
        assert!(inset(0.0, 0.5, 1.0, 1.0, 0.1).abs() < 1e-6);
        assert!(inset(0.0, 0.0, 1.0, 1.0, 0.1) < 0.0);
    }
}

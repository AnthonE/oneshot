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

fn smooth(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The hull's plates: two rows a tile of broad flat plates of uneven
/// width, a thin dark seam and a short bevel between them, and in some a
/// sunk panel of honeycomb or a line of light along the bottom edge. Near
/// white, the look's tint is the colour: a touch bluer on a plate's face,
/// pinker on its bevel.
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
    let height = |x: usize, y: usize| -> f32 {
        let (u, v) = (x as f32 / nf, y as f32 / nf);
        let (j, i, w, pu, pv, d) = plate(u, v);
        let (sunk, cells) = panel(u, v, j, i, w, pu, pv);
        bevel(d) - 0.25 * sunk - 0.08 * cells - 0.3 * groove(j, i, w, pu, pv) - 0.2 * line(j, i, w, pu, pv)
    };
    let face = Vec3::new(0.92, 0.9, 1.0);
    let edge = Vec3::new(1.0, 0.86, 0.96);
    let seam = Vec3::new(0.05, 0.04, 0.07);
    let texel = |x: usize, y: usize, _h: f32| -> Texel {
        let (u, v) = (x as f32 / nf, y as f32 / nf);
        let (j, i, w, pu, pv, d) = plate(u, v);
        let b = bevel(d);
        let (sunk, cells) = panel(u, v, j, i, w, pu, pv);
        let g = groove(j, i, w, pu, pv);
        let glow = line(j, i, w, pu, pv);
        let shade = 0.9 + 0.1 * hash2(i, j, 11);
        let mottle = 0.96 + 0.04 * hash2(x / 32, y / 32, 5);
        let mut col = edge.lerp(face, b) * shade * mottle;
        col *= 1.0 - 0.25 * sunk - 0.1 * cells - 0.5 * g;
        col = seam.lerp(col, smooth(0.0, 0.6, b));
        if glow > 0.5 {
            col = Vec3::new(0.7, 0.7, 0.8);
        }
        Texel {
            albedo: col.to_array(),
            ao: (0.5 + 0.5 * b) * (1.0 - 0.3 * sunk - 0.4 * g),
            rough: 0.36 + 0.24 * (1.0 - b) + 0.1 * sunk + 0.1 * cells,
            metal: 1.0,
            glow,
        }
    };
    bake(n, 5.0, &height, &texel)
}

/// The deck: metre plates, one in four a fine mesh of slots, the rest
/// plain dark purple metal with a sunk panel, a little glossy.
pub fn deck_images(n: usize) -> [Image; 4] {
    const PLATES: usize = 4;
    const SEAM: f32 = 0.004;
    const FILLET: f32 = 0.008;
    let nf = n as f32;
    let pw = 1.0 / PLATES as f32;
    let plate = |u: f32, v: f32| -> (usize, usize, f32, f32, f32) {
        let (i, j) = ((u / pw) as usize % PLATES, (v / pw) as usize % PLATES);
        let (lu, lv) = (u.rem_euclid(pw), v.rem_euclid(pw));
        (i, j, lu, lv, inset(lu, lv, pw, pw, 0.01))
    };
    // One plate in four is meshed: rows of long slots across it, in three
    // columns. The rest are plain, the hull's purple, a panel sunk in each.
    let meshed = |i: usize, j: usize| hash2(i + 7, j + 1, 31) < 0.28;
    // 1 on a bar, 0 in a slot.
    let grate = |i: usize, j: usize, lu: f32, lv: f32, d: f32| -> f32 {
        if !meshed(i, j) || d < 0.022 {
            return 1.0;
        }
        let fv = (lv / (pw / 16.0)).rem_euclid(1.0);
        let fu = (lu / (pw / 3.0)).rem_euclid(1.0);
        let slot = (0.3..0.7).contains(&fv) && (0.1..0.9).contains(&fu);
        if slot {
            0.0
        } else {
            1.0
        }
    };
    let panel = |i: usize, j: usize, d: f32| -> f32 {
        if meshed(i, j) {
            return 0.0;
        }
        // A sunk panel inside the plate's border.
        smooth(0.025, 0.03, d)
    };
    let height = |x: usize, y: usize| -> f32 {
        let (u, v) = (x as f32 / nf, y as f32 / nf);
        let (i, j, lu, lv, d) = plate(u, v);
        let e = ((d - SEAM) / FILLET).clamp(0.0, 1.0);
        let top = e * (1.0 - 0.5 * (1.0 - e));
        top * grate(i, j, lu, lv, d) - 0.15 * panel(i, j, d)
    };
    let bar = Vec3::new(0.026, 0.027, 0.038);
    let plain = Vec3::new(0.05, 0.034, 0.082);
    let hole = Vec3::new(0.003, 0.003, 0.005);
    let texel = |x: usize, y: usize, h: f32| -> Texel {
        let (u, v) = (x as f32 / nf, y as f32 / nf);
        let (i, j, lu, lv, d) = plate(u, v);
        let m = meshed(i, j);
        let g = grate(i, j, lu, lv, d);
        let shade = 0.9 + 0.1 * hash2(i, j, 3);
        let wear = 0.95 + 0.05 * hash2(x / 8, y / 8, 9);
        let hc = h.clamp(0.0, 1.0);
        let top = if m { bar } else { plain };
        let col = hole.lerp(top * shade * wear, smooth(0.0, 0.6, hc) * g);
        Texel {
            albedo: col.to_array(),
            ao: if g < 0.5 { 0.3 } else { 0.6 + 0.4 * hc },
            rough: if g < 0.5 {
                0.9
            } else if m {
                0.45 + 0.2 * (1.0 - hc)
            } else {
                0.32 + 0.2 * (1.0 - hc)
            },
            metal: if m { 1.0 } else { 0.8 },
            glow: 0.0,
        }
    };
    bake(n, 6.0, &height, &texel)
}

/// The windows: a honeycomb of gold glass in dark frames, each cell a
/// little domed and lit from behind, brightest at its middle.
pub fn gold_images(n: usize) -> [Image; 4] {
    const COLS: usize = 7;
    const ROWS: usize = 8;
    const FRAME: f32 = 0.86;
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
    let glass = Vec3::new(0.95, 0.62, 0.18);
    let frame = Vec3::new(0.06, 0.045, 0.03);
    let texel = |x: usize, y: usize, _h: f32| -> Texel {
        let (u, v) = (x as f32 / nf, y as f32 / nf);
        let (d, cell) = hex(u, v, COLS, ROWS);
        let pane = d < FRAME;
        let flicker = 0.85 + 0.15 * hash2(cell.0 as usize, cell.1 as usize, 17);
        let t = (d / FRAME).min(1.0);
        Texel {
            albedo: if pane {
                (glass * (1.0 - 0.3 * t)).to_array()
            } else {
                frame.to_array()
            },
            ao: if pane { 1.0 } else { 0.7 },
            rough: if pane { 0.15 } else { 0.5 },
            metal: if pane { 0.0 } else { 1.0 },
            glow: if pane { (1.0 - 0.45 * t * t) * flicker } else { 0.0 },
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

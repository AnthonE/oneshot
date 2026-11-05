//! Emblems, Halo's way: a glyph on a field, each in a colour of the palette,
//! sprayed on Triple T's back (`paint.rs`). Ours: every shape is drawn here,
//! from a few lines of geometry each, so there is no art to license. What
//! the wire carries is `proto::Emblem`.
//!
//! Cosmetic only (DESIGN.md §15): lit as paint, never emissive, on the
//! back, never on the head.

/// The palette, sRGB: whites to blacks, then round the wheel. Muted, so no
/// colour reads as a light.
pub const PALETTE: [[u8; 3]; proto::EMBLEM_COLOURS as usize] = [
    [236, 236, 230],
    [160, 162, 160],
    [84, 86, 88],
    [26, 26, 28],
    [196, 52, 44],
    [222, 122, 40],
    [226, 196, 64],
    [150, 196, 60],
    [52, 140, 72],
    [40, 150, 140],
    [70, 180, 210],
    [52, 96, 196],
    [32, 44, 104],
    [118, 66, 168],
    [214, 112, 160],
    [118, 78, 48],
];

/// Samples a side of a texel (its coverage, so edges are smooth).
const SS: u32 = 4;

/// `code` drawn `size`² as straight RGBA8, rows top down: the field, then
/// the glyph over it; clear where neither is. All clear for no emblem.
pub fn rgba(code: proto::Emblem, size: u32) -> Vec<u8> {
    let (_, _, gc, fc) = proto::emblem_parts(proto::emblem_valid(code));
    let (g_rgb, f_rgb) = (PALETTE[gc as usize & 15], PALETTE[fc as usize & 15]);
    let mut out = vec![0u8; (size * size * 4) as usize];
    for (i, [ga, fa]) in layers(code, size).into_iter().enumerate() {
        // The glyph over the field: straight alpha, "over".
        let a = ga + fa * (1.0 - ga);
        let i = i * 4;
        if a > 0.0 {
            for c in 0..3 {
                let v = (g_rgb[c] as f32 * ga + f_rgb[c] as f32 * fa * (1.0 - ga)) / a;
                out[i + c] = v.round() as u8;
            }
        }
        out[i + 3] = (a * 255.0).round() as u8;
    }
    out
}

/// `code`'s shapes drawn `size`², rows top down: each texel's coverage by
/// the glyph and by the field (under the glyph too), 0 to 1. The colours
/// are left out: the body's paint takes them as uniforms (`paint.rs`). All
/// clear for no emblem.
pub fn layers(code: proto::Emblem, size: u32) -> Vec<[f32; 2]> {
    let (glyph, field, ..) = proto::emblem_parts(proto::emblem_valid(code));
    let mut out = vec![[0.0; 2]; (size * size) as usize];
    if glyph == 0 {
        return out;
    }
    let n = (SS * SS) as f32;
    // The polygons, once a bake: 409,600 samples at the body's size.
    let polys = Polys::new(glyph, field);
    for py in 0..size {
        for px in 0..size {
            let (mut f_cov, mut g_cov) = (0.0f32, 0.0f32);
            for sy in 0..SS {
                for sx in 0..SS {
                    // [-1, 1], y up.
                    let x = ((px * SS + sx) as f32 + 0.5) / (size * SS) as f32 * 2.0 - 1.0;
                    let y = 1.0 - ((py * SS + sy) as f32 + 0.5) / (size * SS) as f32 * 2.0;
                    if field != 0 && in_field(field, &polys.field, x, y) {
                        f_cov += 1.0;
                    }
                    // On a field the glyph sits a little smaller, inside it
                    // (a triangle's, smaller still and at its middle).
                    let (k, dy) = match field {
                        0 => (1.0, 0.0),
                        6 => (1.9, 0.2),
                        _ => (1.3, 0.0),
                    };
                    if in_glyph(glyph, &polys.glyph, x * k, (y + dy) * k) {
                        g_cov += 1.0;
                    }
                }
            }
            out[(py * size + px) as usize] = [g_cov / n, f_cov / n];
        }
    }
    out
}

fn in_poly(pts: &[(f32, f32)], x: f32, y: f32) -> bool {
    let mut inside = false;
    let mut j = pts.len() - 1;
    for i in 0..pts.len() {
        let ((xi, yi), (xj, yj)) = (pts[i], pts[j]);
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// A polygon of at most [`POLY_MAX`] corners.
#[derive(Clone, Copy)]
struct Poly {
    pts: [(f32, f32); POLY_MAX],
    n: usize,
}

/// The most corners a shape has: the star's ten.
const POLY_MAX: usize = 10;

impl Poly {
    const NONE: Poly = Poly {
        pts: [(0.0, 0.0); POLY_MAX],
        n: 0,
    };

    /// A regular polygon of `n` sides, radius `r`, a vertex at angle `a0`.
    fn ngon(n: usize, r: f32, a0: f32) -> Poly {
        let mut p = Poly { n, ..Poly::NONE };
        for (i, pt) in p.pts[..n].iter_mut().enumerate() {
            let a = a0 + i as f32 * core::f32::consts::TAU / n as f32;
            *pt = (r * a.cos(), r * a.sin());
        }
        p
    }

    /// The star: five points, ten corners.
    fn star() -> Poly {
        use core::f32::consts::{FRAC_PI_2, PI};
        let mut p = Poly { n: 10, ..Poly::NONE };
        for (i, pt) in p.pts.iter_mut().enumerate() {
            let a = FRAC_PI_2 + i as f32 * PI / 5.0;
            let rr = if i % 2 == 0 { 0.7 } else { 0.28 };
            *pt = (rr * a.cos(), rr * a.sin());
        }
        p
    }

    fn has(&self, x: f32, y: f32) -> bool {
        in_poly(&self.pts[..self.n], x, y)
    }
}

/// The polygons an emblem's field and glyph are cut from (none for a shape
/// that is no polygon), made once for every sample.
struct Polys {
    field: Poly,
    glyph: Poly,
}

impl Polys {
    fn new(glyph: u8, field: u8) -> Polys {
        use core::f32::consts::{FRAC_PI_2, PI};
        let field = match field {
            4 => Poly::ngon(6, 0.97, 0.0),
            6 => Poly::ngon(3, 1.0, FRAC_PI_2),
            7 => Poly::ngon(8, 0.97, PI / 8.0),
            _ => Poly::NONE,
        };
        let glyph = match glyph {
            1 => Poly::star(),
            7 => Poly::ngon(3, 0.68, FRAC_PI_2),
            _ => Poly::NONE,
        };
        Polys { field, glyph }
    }
}

/// A heater shield: straight sides, a point at the bottom.
fn in_shield(s: f32, x: f32, y: f32) -> bool {
    let (x, y) = (x / s, y / s);
    if !(-1.0..=0.85).contains(&y) || x.abs() > 0.8 {
        return false;
    }
    y > -0.1 || x.abs() < 0.8 * (1.0 - ((-0.1 - y) / 0.9).powf(1.6))
}

/// Whether `(x, y)` is on `field`; `poly` is its polygon ([`Polys`]).
fn in_field(field: u8, poly: &Poly, x: f32, y: f32) -> bool {
    let r = (x * x + y * y).sqrt();
    match field {
        1 => r < 0.96,
        2 => x.abs() < 0.86 && y.abs() < 0.86,
        3 => x.abs() + y.abs() < 0.98,
        4 => poly.has(x, y),
        5 => in_shield(1.0, x, y),
        6 => poly.has(x, y + 0.2),
        7 => poly.has(x, y),
        _ => false,
    }
}

/// Whether `(x, y)` is on `glyph`; `poly` is its polygon ([`Polys`]).
fn in_glyph(glyph: u8, poly: &Poly, x: f32, y: f32) -> bool {
    use core::f32::consts::TAU;
    let r = (x * x + y * y).sqrt();
    match glyph {
        // star
        1 => poly.has(x, y + 0.05),
        // chevron, pointing up
        2 => {
            let d = y - (0.45 - x.abs() * 0.9);
            x.abs() < 0.62 && (-0.36..0.0).contains(&d)
        }
        // bolt
        3 => in_poly(
            &[
                (0.12, 0.72),
                (-0.38, -0.02),
                (-0.04, -0.02),
                (-0.16, -0.72),
                (0.38, 0.1),
                (0.04, 0.1),
            ],
            x,
            y,
        ),
        // crosshair
        4 => {
            (0.4..0.52).contains(&r)
                || (x.abs() < 0.055 && y.abs() < 0.68 && y.abs() > 0.2)
                || (y.abs() < 0.055 && x.abs() < 0.68 && x.abs() > 0.2)
        }
        // ring
        5 => (0.36..0.6).contains(&r),
        // diamond
        6 => x.abs() / 0.45 + y.abs() / 0.68 < 1.0,
        // triangle
        7 => poly.has(x, y + 0.12),
        // cross
        8 => (x.abs() < 0.17 && y.abs() < 0.62) || (y.abs() < 0.17 && x.abs() < 0.62),
        // arrow, up
        9 => {
            (x.abs() < 0.12 && (-0.65..0.1).contains(&y)) || in_poly(&[(0.0, 0.68), (-0.42, 0.08), (0.42, 0.08)], x, y)
        }
        // moon
        10 => r < 0.58 && ((x - 0.26) * (x - 0.26) + (y - 0.14) * (y - 0.14)).sqrt() > 0.5,
        // heart
        11 => {
            let (hx, hy) = (x / 0.62, (y + 0.08) / 0.62);
            let q = hx * hx + hy * hy - 1.0;
            q * q * q - hx * hx * hy * hy * hy < 0.0
        }
        // eye: a lens, its iris a ring round the pupil
        12 => {
            let lens =
                ((x * x + (y + 0.55) * (y + 0.55)).sqrt() < 0.9) && ((x * x + (y - 0.55) * (y - 0.55)).sqrt() < 0.9);
            lens && !(0.14..=0.26).contains(&r)
        }
        // shield
        13 => in_shield(0.66, x, y + 0.04),
        // sun: a disc and eight rays
        14 => {
            let a = y.atan2(x).rem_euclid(TAU / 8.0);
            r < 0.3 || ((0.4..0.68).contains(&r) && !(0.16..=TAU / 8.0 - 0.16).contains(&a))
        }
        // wings: two, swept up and out, four feathers each, a bead between
        15 => {
            let ax = x.abs();
            if !(0.1..0.88).contains(&ax) {
                return x * x + (y - 0.05) * (y - 0.05) < 0.01;
            }
            let t = (ax - 0.1) / 0.78;
            let top = -0.05 + 0.55 * t.powf(0.6) - 0.15 * t * t;
            let thick = 0.16 + 0.26 * (1.0 - t) + 0.16 * (1.0 - (t * 4.0).fract());
            y < top && y > top - thick
        }
        // crown: a band and three points
        16 => {
            let band = x.abs() < 0.6 && (-0.45..-0.2).contains(&y);
            let t = (x.abs() / 0.3).fract();
            let k = (x.abs() / 0.3) as i32;
            let peak = if k % 2 == 0 { 0.5 - t * 0.45 } else { 0.05 + t * 0.45 };
            band || (x.abs() < 0.6 && (-0.2..peak).contains(&y))
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_glyph_and_field_draws_something_and_none_draws_nothing() {
        for g in 1..proto::EMBLEM_GLYPHS.len() as u8 {
            for f in 0..proto::EMBLEM_FIELDS.len() as u8 {
                let px = rgba(proto::emblem(g, f, 0, 4), 32);
                let shown = px.chunks(4).filter(|p| p[3] > 128).count();
                assert!(shown > 20, "glyph {g} on field {f}: {shown} texels");
            }
        }
        assert!(rgba(proto::EMBLEM_NONE, 16).iter().all(|&b| b == 0));
    }

    #[test]
    fn the_glyph_shows_in_its_own_colour_over_the_field() {
        // A star (white) on a circle (red): the middle is the glyph's, the
        // field's rim the field's, a corner clear.
        let px = rgba(proto::emblem(1, 1, 0, 4), 64);
        let at = |x: u32, y: u32| &px[((y * 64 + x) * 4) as usize..][..4];
        assert_eq!(&at(32, 30)[..3], &PALETTE[0]);
        assert_eq!(&at(32, 3)[..3], &PALETTE[4]);
        assert_eq!(at(0, 0)[3], 0);
    }
}

/// Every glyph on every field as a raw RGBA sheet in `traces/emblems.rgba`
/// (`cargo test --lib emblem_sheet -- --ignored`), for looking at the art.
#[cfg(test)]
#[test]
#[ignore]
fn emblem_sheet() {
    let (cell, gs, fs) = (
        96u32,
        proto::EMBLEM_GLYPHS.len() as u32 - 1,
        proto::EMBLEM_FIELDS.len() as u32,
    );
    let (w, h) = (cell * gs, cell * fs);
    let mut out = vec![40u8; (w * h * 4) as usize];
    for g in 1..=gs {
        for f in 0..fs {
            let px = rgba(proto::emblem(g as u8, f as u8, (g % 3) as u8, 4 + (f % 12) as u8), cell);
            for y in 0..cell {
                for x in 0..cell {
                    let s = ((y * cell + x) * 4) as usize;
                    let d = (((f * cell + y) * w + (g - 1) * cell + x) * 4) as usize;
                    let a = px[s + 3] as u32;
                    for c in 0..3 {
                        out[d + c] = ((px[s + c] as u32 * a + 40 * (255 - a)) / 255) as u8;
                    }
                    out[d + 3] = 255;
                }
            }
        }
    }
    std::fs::create_dir_all("../../traces").unwrap();
    std::fs::write("../../traces/emblems.rgba", &out).unwrap();
    println!("{w}x{h}");
}

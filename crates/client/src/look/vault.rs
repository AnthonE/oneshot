//! Curves the sim has no shape for, drawn over a box: a vault (a box whose
//! look is `vault` is drawn as a half-elliptical ceiling hung from its
//! bottom edges, ribbed, with light along the ribs) and a rib (a box drawn
//! as a rounded bar). What the box collides as stays the box: a vault's
//! map puts `hidden` boxes under it that step with the curve, and a rib is
//! never more than a few centimetres from its box.

use bevy::prelude::*;

use crate::level::Faces;

/// Ribs this far apart along a vault, and each one's width and depth.
const RIB_PITCH_M: f32 = 3.0;
const RIB_W_M: f32 = 0.5;
const RIB_DEPTH_M: f32 = 0.35;
/// The light strips beside each rib: width, and how far in from the rib.
const STRIP_W_M: f32 = 0.1;
const STRIP_GAP_M: f32 = 0.08;
/// A strip sits this far under the plates, so it never fights them.
const PROUD_M: f32 = 0.02;
/// A facet of the arc every so many metres.
const FACET_M: f32 = 0.4;

/// The vault's three meshes: its plates (the hull's set), its ribs (the
/// dark hull) and its light strips (emissive), in world space.
pub struct Vault {
    pub plates: Mesh,
    pub ribs: Mesh,
    pub strips: Mesh,
}

/// The arc across the span: `t` in 0..1 from one bottom edge over the
/// crown to the other, as (across, up) from the span's middle at the
/// bottom, and the inward (downward-facing) unit normal.
fn arc(t: f32, half: f32, rise: f32) -> (Vec2, Vec2) {
    let a = core::f32::consts::PI * t;
    let (s, c) = a.sin_cos();
    let p = Vec2::new(half * c, rise * s);
    // The ellipse's outward normal is (cos/half, sin/rise); inward is its
    // negative.
    let n = -Vec2::new(c / half, s / rise).normalize_or(Vec2::NEG_Y);
    (p, n)
}

/// The vault over the box from `min` to `max`, curving across its shorter
/// horizontal side, its bottom edges on the box's bottom and its crown at
/// the box's top. Texture coordinates in metres.
pub fn vault(min: Vec3, max: Vec3, tile: f32, dark_tile: f32) -> Vault {
    let size = max - min;
    let along_x = size.x >= size.z;
    let (len, span) = if along_x { (size.x, size.z) } else { (size.z, size.x) };
    let (half, rise) = (span * 0.5, size.y);
    let mid = (min + max) * 0.5;
    // World position for (distance along, across from the middle, up from
    // the bottom).
    let at = |l: f32, a: f32, u: f32| -> Vec3 {
        if along_x {
            Vec3::new(min.x + l, min.y + u, mid.z + a)
        } else {
            Vec3::new(mid.x + a, min.y + u, min.z + l)
        }
    };
    let dir = |a: f32, u: f32| -> Vec3 {
        if along_x {
            Vec3::new(0.0, u, a)
        } else {
            Vec3::new(a, u, 0.0)
        }
    };
    let arc_len = core::f32::consts::PI * 0.5 * (half + rise) * 1.05;
    // An even count, so a facet's edge sits on the crown.
    let n = ((arc_len / FACET_M).ceil() as usize).clamp(12, 96);
    let n = n + n % 2;
    let mut plates = Faces::default();
    let mut ribs = Faces::default();
    let mut strips = Faces::default();
    // The plates: a strip of quads the whole length, facing in.
    let mut v_at = 0.0;
    let mut last = arc(0.0, half, rise);
    for i in 0..n {
        let t0 = i as f32 / n as f32;
        let t1 = (i + 1) as f32 / n as f32;
        let (p0, n0) = arc(t0, half, rise);
        let (p1, n1) = arc(t1, half, rise);
        let seg = (p1 - p0).length();
        let v0 = v_at;
        let v1 = v_at + seg;
        let quad = [
            at(0.0, p0.x, p0.y),
            at(len, p0.x, p0.y),
            at(len, p1.x, p1.y),
            at(0.0, p1.x, p1.y),
        ];
        let nm = dir((n0 + n1).x * 0.5, (n0 + n1).y * 0.5).normalize_or(Vec3::NEG_Y);
        plates.quad_uv(quad, nm, [[0.0, v0], [len, v0], [len, v1], [0.0, v1]], tile);
        v_at = v1;
        last = (p1, n1);
    }
    let _ = last;
    // The ribs: one every RIB_PITCH_M, each an inward band with sides,
    // and a light strip to either side of it.
    let count = (len / RIB_PITCH_M).round().max(1.0) as usize;
    let pitch = len / count as f32;
    for r in 0..count {
        let l = pitch * (r as f32 + 0.5);
        for i in 0..n {
            let t0 = i as f32 / n as f32;
            let t1 = (i + 1) as f32 / n as f32;
            let (p0, n0) = arc(t0, half, rise);
            let (p1, n1) = arc(t1, half, rise);
            let q0 = p0 + n0 * RIB_DEPTH_M;
            let q1 = p1 + n1 * RIB_DEPTH_M;
            let nm = dir((n0 + n1).x * 0.5, (n0 + n1).y * 0.5).normalize_or(Vec3::NEG_Y);
            let (la, lb) = (l - RIB_W_M * 0.5, l + RIB_W_M * 0.5);
            // The band's underside.
            ribs.quad_uv(
                [
                    at(la, q0.x, q0.y),
                    at(lb, q0.x, q0.y),
                    at(lb, q1.x, q1.y),
                    at(la, q1.x, q1.y),
                ],
                nm,
                [
                    [la, t0 * arc_len],
                    [lb, t0 * arc_len],
                    [lb, t1 * arc_len],
                    [la, t1 * arc_len],
                ],
                dark_tile,
            );
            // Its two sides, from the plates down to the band.
            for (ls, sign) in [(la, -1.0), (lb, 1.0)] {
                let side = if along_x { Vec3::X * sign } else { Vec3::Z * sign };
                ribs.quad_uv(
                    [
                        at(ls, p0.x, p0.y),
                        at(ls, q0.x, q0.y),
                        at(ls, q1.x, q1.y),
                        at(ls, p1.x, p1.y),
                    ],
                    side,
                    [
                        [t0 * arc_len, 0.0],
                        [t0 * arc_len, RIB_DEPTH_M],
                        [t1 * arc_len, RIB_DEPTH_M],
                        [t1 * arc_len, 0.0],
                    ],
                    dark_tile,
                );
            }
            // The strips beside every other rib, flush with the plates and
            // a little proud.
            if !r.is_multiple_of(2) {
                continue;
            }
            for sign in [-1.0, 1.0] {
                let c = l + sign * (RIB_W_M * 0.5 + STRIP_GAP_M + STRIP_W_M * 0.5);
                let (sa, sb) = (c - STRIP_W_M * 0.5, c + STRIP_W_M * 0.5);
                let s0 = p0 + n0 * PROUD_M;
                let s1 = p1 + n1 * PROUD_M;
                strips.quad_uv(
                    [
                        at(sa, s0.x, s0.y),
                        at(sb, s0.x, s0.y),
                        at(sb, s1.x, s1.y),
                        at(sa, s1.x, s1.y),
                    ],
                    nm,
                    [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
                    1.0,
                );
            }
        }
    }
    Vault {
        plates: plates.mesh(1.0),
        ribs: ribs.mesh(1.0),
        strips: strips.mesh(1.0),
    }
}

/// A rib: the box from `min` to `max` as a bar rounded on its two shorter
/// sides (a stadium section) along its longest. Texture coordinates in
/// metres, divided by `tile`.
pub fn rib(min: Vec3, max: Vec3, tile: f32) -> Mesh {
    let size = max - min;
    let mid = (min + max) * 0.5;
    // The long axis, and the two across it.
    let axis = if size.x >= size.y && size.x >= size.z {
        0
    } else if size.y >= size.z {
        1
    } else {
        2
    };
    let (a, b) = match axis {
        0 => (1, 2),
        1 => (2, 0),
        _ => (0, 1),
    };
    let unit = |i: usize| match i {
        0 => Vec3::X,
        1 => Vec3::Y,
        _ => Vec3::Z,
    };
    let (ha, hb) = (size[a] * 0.5, size[b] * 0.5);
    let r = ha.min(hb);
    // The section: a rounded rectangle, its corners rounded by `r`.
    const SEG: usize = 6;
    let mut ring: Vec<(Vec2, Vec2)> = Vec::new();
    let corners = [
        (Vec2::new(ha - r, hb - r), 0.0),
        (Vec2::new(-(ha - r), hb - r), 90.0),
        (Vec2::new(-(ha - r), -(hb - r)), 180.0),
        (Vec2::new(ha - r, -(hb - r)), 270.0),
    ];
    for (c, start) in corners {
        for i in 0..=SEG {
            let ang = (start + 90.0 * i as f32 / SEG as f32).to_radians();
            let nm = Vec2::new(ang.cos(), ang.sin());
            ring.push((c + nm * r, nm));
        }
    }
    let mut f = Faces::default();
    let half = size[axis] * 0.5;
    let world = |p: Vec2, l: f32| mid + unit(a) * p.x + unit(b) * p.y + unit(axis) * l;
    let m = ring.len();
    for i in 0..m {
        let (p0, n0) = ring[i];
        let (p1, n1) = ring[(i + 1) % m];
        let nm = unit(a) * (n0.x + n1.x) * 0.5 + unit(b) * (n0.y + n1.y) * 0.5;
        let u0 = i as f32 / m as f32 * 2.0 * (ha + hb) * 2.0;
        let u1 = (i + 1) as f32 / m as f32 * 2.0 * (ha + hb) * 2.0;
        f.quad_uv(
            [world(p0, -half), world(p1, -half), world(p1, half), world(p0, half)],
            nm.normalize_or(Vec3::Y),
            [[u0, 0.0], [u1, 0.0], [u1, size[axis]], [u0, size[axis]]],
            tile,
        );
    }
    // The ends, as fans.
    for (l, sign) in [(-half, -1.0), (half, 1.0)] {
        let centre = world(Vec2::ZERO, l);
        let nm = unit(axis) * sign;
        for i in 0..m {
            let (p0, _) = ring[i];
            let (p1, _) = ring[(i + 1) % m];
            f.tri(centre, world(p0, l), world(p1, l), nm, tile);
        }
    }
    f.mesh(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_vault_hangs_from_its_bottom_edges_to_its_crown() {
        let v = vault(Vec3::new(-10.0, 8.0, -5.0), Vec3::new(10.0, 11.0, 5.0), 6.0, 6.0);
        let pos = match v.plates.attribute(Mesh::ATTRIBUTE_POSITION) {
            Some(bevy::mesh::VertexAttributeValues::Float32x3(p)) => p.clone(),
            _ => panic!("no positions"),
        };
        let lo = pos.iter().map(|p| p[1]).fold(f32::MAX, f32::min);
        let hi = pos.iter().map(|p| p[1]).fold(f32::MIN, f32::max);
        assert!((lo - 8.0).abs() < 1e-2 && (hi - 11.0).abs() < 1e-2, "{lo} {hi}");
        assert!(pos.iter().all(|p| p[0] >= -10.0 - 1e-3 && p[0] <= 10.0 + 1e-3));
        assert!(v.ribs.count_vertices() > 0 && v.strips.count_vertices() > 0);
    }

    #[test]
    fn a_rib_stays_inside_its_box() {
        let m = rib(Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.4, 6.0, 0.3), 2.0);
        let pos = match m.attribute(Mesh::ATTRIBUTE_POSITION) {
            Some(bevy::mesh::VertexAttributeValues::Float32x3(p)) => p.clone(),
            _ => panic!("no positions"),
        };
        for p in &pos {
            assert!(
                p[0] >= -1e-4
                    && p[0] <= 0.4 + 1e-4
                    && p[1] >= -1e-4
                    && p[1] <= 6.0 + 1e-4
                    && p[2] >= -1e-4
                    && p[2] <= 0.3 + 1e-4,
                "{p:?}"
            );
        }
    }
}

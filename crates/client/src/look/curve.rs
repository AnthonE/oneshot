//! Profile solids drawn (sim::profile): the cross-section's outline (its
//! tops out, the far end down, its bottoms back, the near end up) spun
//! round the axis or carried along it, and a sector's or a carried
//! solid's end caps. Smooth where the outline bends gently, a crease where
//! it turns more than [`CREASE_DEG`]: a rim reads as an edge, a dome as one
//! surface. The knots are the sim's, so a shot stops on what is drawn
//! (within the facets: a facet every 25 cm or so of the rim, as a round
//! pillar's, [`crate::level::round_sides`]). Texture coordinates in metres
//! (divided by the tile here): round the axis at the outer radius; up an
//! upright face by the world's height (so pieces stacked on one circle
//! line up), along the outline elsewhere.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use sim::solid::{Shape, Solid};

/// The outline bends more than this at a knot: a crease, not one surface.
const CREASE_DEG: f32 = 35.0;

#[derive(Default)]
struct Build {
    pos: Vec<[f32; 3]>,
    nor: Vec<[f32; 3]>,
    uv: Vec<[f32; 2]>,
    idx: Vec<u32>,
}

impl Build {
    fn vert(&mut self, p: Vec3, n: Vec3, uv: [f32; 2]) -> u32 {
        self.pos.push(p.to_array());
        self.nor.push(n.to_array());
        self.uv.push(uv);
        self.pos.len() as u32 - 1
    }

    /// A quad of four placed vertices, wound to face the way their normals
    /// do (or the triangle it is, where two corners meet at an axis).
    fn quad(&mut self, v: [u32; 4]) {
        let p = |i: u32| Vec3::from_array(self.pos[i as usize]);
        let n = |i: u32| Vec3::from_array(self.nor[i as usize]);
        let face = (p(v[2]) - p(v[0])).cross(p(v[3]) - p(v[1]));
        let want = n(v[0]) + n(v[1]) + n(v[2]) + n(v[3]);
        let v = if face.dot(want) >= 0.0 {
            v
        } else {
            [v[0], v[3], v[2], v[1]]
        };
        self.idx.extend_from_slice(&[v[0], v[1], v[2], v[0], v[2], v[3]]);
    }

    fn mesh(self, tile: f32) -> Mesh {
        let uv: Vec<[f32; 2]> = self.uv.iter().map(|u| [u[0] / tile, u[1] / tile]).collect();
        let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.pos)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.nor)
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
            .with_inserted_indices(Indices::U32(self.idx));
        let _ = m.generate_tangents();
        m
    }
}

/// One edge of the outline in the profile's plane (s across or out, y up):
/// its ends, each end's normal (smoothed with its neighbour's where the
/// outline bends gently), and how far along the outline each end is.
struct Edge {
    a: Vec2,
    b: Vec2,
    na: Vec2,
    nb: Vec2,
    la: f32,
    lb: f32,
    /// Upright, in a run of upright faces between creases: its texture
    /// runs by height.
    wall: bool,
}

impl Edge {
    /// The texture's run along the outline at its first or second end,
    /// `y` the solid's own height.
    fn v(&self, second: bool, y: f32) -> f32 {
        match (self.wall, second) {
            (true, false) => -(y + self.a.y),
            (true, true) => -(y + self.b.y),
            (false, false) => self.la,
            (false, true) => self.lb,
        }
    }
}

/// The outline, round the profile once: tops out, the far end down,
/// bottoms back, the near end up (unless the near end is the axis).
fn outline(k: &[[f32; 3]], axis: bool) -> Vec<Edge> {
    let mut pts: Vec<Vec2> = k.iter().map(|p| Vec2::new(p[0], p[2])).collect();
    pts.extend(k.iter().rev().map(|p| Vec2::new(p[0], p[1])));
    // Each edge from one point to the next, round the loop; the near end
    // (bottom of the first knot to its top) closes it.
    let n = pts.len();
    let mut raw: Vec<(Vec2, Vec2)> = Vec::new();
    for i in 0..n {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        if (b - a).length() < 1e-5 {
            continue;
        }
        // The near end on the axis is inside a spun solid.
        if axis && a.x <= 1e-6 && b.x <= 1e-6 {
            continue;
        }
        raw.push((a, b));
    }
    let normal = |(a, b): (Vec2, Vec2)| {
        let d = (b - a).normalize_or(Vec2::X);
        Vec2::new(-d.y, d.x)
    };
    let cos_crease = CREASE_DEG.to_radians().cos();
    let m = raw.len();
    // Whether each edge runs on smoothly from the one before it (an edge on
    // the axis was cut: no join there).
    let smooth_from_prev: Vec<bool> = (0..m)
        .map(|i| {
            let (p, e) = (raw[(i + m - 1) % m], raw[i]);
            (p.1 - e.0).length() < 1e-5 && normal(p).dot(normal(e)) >= cos_crease
        })
        .collect();
    // The smooth runs between creases: one that is all upright faces is
    // textured by height, so pieces stacked on one circle line up; one
    // that bends over (a dome, a bulb, a rolled rim) along its outline, so
    // no seam crosses a smooth surface.
    let mut run = vec![0usize; m];
    for i in 1..m {
        run[i] = run[i - 1] + usize::from(!smooth_from_prev[i]);
    }
    if m > 0 && smooth_from_prev[0] {
        // The loop closes smoothly: its last run is its first.
        let last = run[m - 1];
        for r in run.iter_mut().filter(|r| **r == last) {
            *r = 0;
        }
    }
    let upright = |e: (Vec2, Vec2)| normal(e).y.abs() < 0.7;
    let level_in = |r: usize| (0..m).any(|j| run[j] == r && !upright(raw[j]));
    let mut out = Vec::with_capacity(m);
    let mut l = 0.0;
    for i in 0..m {
        let e = raw[i];
        let n0 = normal(e);
        let prev = raw[(i + m - 1) % m];
        let next = raw[(i + 1) % m];
        let blend = |other: (Vec2, Vec2), smooth: bool| {
            if smooth {
                (normal(other) + n0).normalize_or(n0)
            } else {
                n0
            }
        };
        let na = blend(prev, smooth_from_prev[i]);
        let nb = blend(next, smooth_from_prev[(i + 1) % m]);
        let len = (e.1 - e.0).length();
        out.push(Edge {
            a: e.0,
            b: e.1,
            na,
            nb,
            la: l,
            lb: l + len,
            wall: upright(e) && !level_in(run[i]),
        });
        l += len;
    }
    out
}

/// The profile's area as quads between its knots, for an end cap.
fn cap(k: &[[f32; 3]]) -> Vec<[Vec2; 4]> {
    let mut out = Vec::new();
    for w in k.windows(2) {
        let (p, q) = (w[0], w[1]);
        if q[0] - p[0] < 1e-6 {
            continue;
        }
        out.push([
            Vec2::new(p[0], p[1]),
            Vec2::new(q[0], q[1]),
            Vec2::new(q[0], q[2]),
            Vec2::new(p[0], p[2]),
        ]);
    }
    out
}

/// A profile solid's mesh in world coordinates, or None for the other
/// kinds.
pub fn profile_mesh(sol: &Solid, tile: f32) -> Option<Mesh> {
    let mut f = Build::default();
    match sol.shape {
        Shape::Revolve { x, z, y, sector, k } => {
            let sn = k[k.len() - 1][0];
            let full = crate::level::round_sides(sn);
            let (a0, span) = match sector {
                None => (0.0, core::f32::consts::TAU),
                Some(sec) => {
                    let a0 = sec.e0[1].atan2(sec.e0[0]);
                    let mut a1 = sec.e1[1].atan2(sec.e1[0]);
                    if a1 <= a0 {
                        a1 += core::f32::consts::TAU;
                    }
                    (a0, a1 - a0)
                }
            };
            let n = ((full as f32 * span / core::f32::consts::TAU).ceil() as usize).max(2);
            let ang = |j: usize| a0 + span * j as f32 / n as f32;
            // Round the axis in metres at the outer radius.
            let ru = sn.max(0.5);
            let at = |s: f32, h: f32, a: f32| Vec3::new(x + s * a.cos(), y + h, z + s * a.sin());
            let dir = |nn: Vec2, a: f32| Vec3::new(nn.x * a.cos(), nn.y, nn.x * a.sin()).normalize_or(Vec3::Y);
            for e in outline(k, k[0][0] <= 0.0) {
                let mut ring = Vec::with_capacity((n + 1) * 2);
                for j in 0..=n {
                    let a = ang(j);
                    let u = a * ru;
                    ring.push(f.vert(at(e.a.x, e.a.y, a), dir(e.na, a), [u, e.v(false, y)]));
                    ring.push(f.vert(at(e.b.x, e.b.y, a), dir(e.nb, a), [u, e.v(true, y)]));
                }
                for j in 0..n {
                    let (i, o) = (2 * j as u32, 2 * j as u32 + 2);
                    f.quad([
                        ring[i as usize],
                        ring[i as usize + 1],
                        ring[o as usize + 1],
                        ring[o as usize],
                    ]);
                }
            }
            if let Some(sec) = sector {
                for (end, a) in [(0u8, a0), (1u8, a0 + span)] {
                    let o = sec.out(end);
                    let nn = Vec3::new(o[0], 0.0, o[1]);
                    for q in cap(k) {
                        let v = q.map(|p| f.vert(at(p.x, p.y, a), nn, [p.x, -p.y]));
                        f.quad(v);
                    }
                }
            }
        }
        Shape::Sweep { x, z, y, c, s, l, k } => {
            let along = Vec3::new(c, 0.0, s);
            let across = Vec3::new(-s, 0.0, c);
            // Along the axis in the world's own metres, so neighbours line up.
            let u0 = x * c + z * s;
            let at = |v: f32, h: f32, u: f32| Vec3::new(x, y + h, z) + along * u + across * v;
            let dir = |nn: Vec2| (across * nn.x + Vec3::Y * nn.y).normalize_or(Vec3::Y);
            for e in outline(k, false) {
                // A level face (a walkway's top, a ramp's) in the world's x
                // and z, as a box's top is: the straight pieces of a curved
                // ramp then carry one pattern.
                let level = (e.b - e.a).y.abs() < 0.7 * (e.b - e.a).length();
                let corner = |p: Vec2, u: f32, second: bool| {
                    let w = at(p.x, p.y, u);
                    if level {
                        [w.x, w.z]
                    } else {
                        [u0 + u, e.v(second, y)]
                    }
                };
                let v = [
                    f.vert(at(e.a.x, e.a.y, -l), dir(e.na), corner(e.a, -l, false)),
                    f.vert(at(e.b.x, e.b.y, -l), dir(e.nb), corner(e.b, -l, true)),
                    f.vert(at(e.b.x, e.b.y, l), dir(e.nb), corner(e.b, l, true)),
                    f.vert(at(e.a.x, e.a.y, l), dir(e.na), corner(e.a, l, false)),
                ];
                f.quad(v);
            }
            for (u, nn) in [(-l, -along), (l, along)] {
                for q in cap(k) {
                    let v = q.map(|p| f.vert(at(p.x, p.y, u), nn, [p.x, -p.y]));
                    f.quad(v);
                }
            }
        }
        _ => return None,
    }
    Some(f.mesh(tile))
}

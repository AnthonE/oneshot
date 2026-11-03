//! Ground that isn't flat: a heightfield under the boxes, for maps with
//! hills, banks and channels (docs/DESIGN.md §16, milestone 9). Heights are
//! integers (1/[`HEIGHT_Q`] m) on a square grid of `cell` metres, written
//! by `tools/maps/compile.mjs`, so the sim never sees a float the compiler
//! made. Each cell is two flat triangles split on the diagonal from its low
//! corner (x0, z0) to its high one: the surface is planar piecewise, with an
//! exact normal everywhere, and every query is `+ − × ÷ sqrt` and
//! [`crate::fmath::floor_i32`].
//!
//! The terrain has no overhangs; boxes give those. It covers the map's
//! bounds and the walls; outside it there is no ground.

use crate::fmath::floor_i32;
use crate::geom::V3;

/// Heights are stored in 1/128 m.
pub const HEIGHT_Q: f32 = 128.0;

/// A terrain as the compiler writes it.
pub struct TerrainDef {
    /// A cell's side: 0.5, 1, 2 or 4 m (a power of two, so its inverse is
    /// exact).
    pub cell: f32,
    /// The corner of vertex (0, 0).
    pub x0: f32,
    pub z0: f32,
    /// Vertices along x and z.
    pub nx: u16,
    pub nz: u16,
    /// `nx × nz` heights in 1/[`HEIGHT_Q`] m, row by row (z outer).
    pub heights: &'static [i16],
}

#[derive(Clone, Debug)]
pub struct Terrain {
    cell: f32,
    inv: f32,
    x0: f32,
    z0: f32,
    /// Vertices along x and z (cells are one fewer).
    nx: usize,
    nz: usize,
    h: Vec<f32>,
    /// Each cell's highest corner, so a ray clear above it skips it.
    top: Vec<f32>,
    /// Each block's ([`BLOCK`] × [`BLOCK`] cells) highest corner, so a ray
    /// clear above a block skips all of it.
    btop: Vec<f32>,
    /// The steepest any triangle rises, metres a metre.
    steep: f32,
}

/// Cells to a block side.
pub const BLOCK: usize = 8;

/// One triangle's plane: y = a + bx·x + bz·z, in metres.
#[derive(Clone, Copy, Debug)]
struct Plane {
    a: f32,
    bx: f32,
    bz: f32,
}

impl Plane {
    fn y(&self, x: f32, z: f32) -> f32 {
        self.a + self.bx * x + self.bz * z
    }
}

impl Terrain {
    pub fn build(d: &TerrainDef) -> Terrain {
        let (nx, nz) = (d.nx as usize, d.nz as usize);
        assert!(
            nx >= 2 && nz >= 2 && d.heights.len() == nx * nz,
            "a terrain is nx × nz heights, both 2 or more"
        );
        let h: Vec<f32> = d.heights.iter().map(|&q| q as f32 * (1.0 / HEIGHT_Q)).collect();
        let mut top = Vec::with_capacity((nx - 1) * (nz - 1));
        for j in 0..nz - 1 {
            for i in 0..nx - 1 {
                let k = j * nx + i;
                top.push(h[k].max(h[k + 1]).max(h[k + nx]).max(h[k + nx + 1]));
            }
        }
        let (ncx, ncz) = (nx - 1, nz - 1);
        let (nbx, nbz) = (ncx.div_ceil(BLOCK), ncz.div_ceil(BLOCK));
        let mut btop = vec![f32::NEG_INFINITY; nbx * nbz];
        for j in 0..ncz {
            for i in 0..ncx {
                let b = &mut btop[(j / BLOCK) * nbx + i / BLOCK];
                *b = b.max(top[j * ncx + i]);
            }
        }
        let mut t = Terrain {
            steep: 0.0,
            btop,
            cell: d.cell,
            inv: 1.0 / d.cell,
            x0: d.x0,
            z0: d.z0,
            nx,
            nz,
            h,
            top,
        };
        for j in 0..nz - 1 {
            for i in 0..nx - 1 {
                let (up, low) = t.planes(i, j);
                for p in [up, low] {
                    t.steep = t.steep.max((p.bx * p.bx + p.bz * p.bz).sqrt());
                }
            }
        }
        t
    }

    /// The steepest the ground rises anywhere, in metres a metre: how much
    /// higher it can be a metre away from any place.
    pub fn steepest(&self) -> f32 {
        self.steep
    }

    fn x1(&self) -> f32 {
        self.x0 + (self.nx - 1) as f32 * self.cell
    }

    fn z1(&self) -> f32 {
        self.z0 + (self.nz - 1) as f32 * self.cell
    }

    /// Whether (x, z) is over the terrain.
    pub fn covers(&self, x: f32, z: f32) -> bool {
        x >= self.x0 && x <= self.x1() && z >= self.z0 && z <= self.z1()
    }

    /// The cell (i, j) holding (x, z), clamped to the terrain.
    fn cell_of(&self, x: f32, z: f32) -> (usize, usize) {
        let i = floor_i32((x - self.x0) * self.inv).clamp(0, self.nx as i32 - 2) as usize;
        let j = floor_i32((z - self.z0) * self.inv).clamp(0, self.nz as i32 - 2) as usize;
        (i, j)
    }

    /// Cell (i, j)'s two planes: the one over the side where the x offset
    /// is at least the z offset, then the other.
    fn planes(&self, i: usize, j: usize) -> (Plane, Plane) {
        let k = j * self.nx + i;
        let (h00, h10, h01, h11) = (self.h[k], self.h[k + 1], self.h[k + self.nx], self.h[k + self.nx + 1]);
        let (cx, cz) = (self.x0 + i as f32 * self.cell, self.z0 + j as f32 * self.cell);
        // In the cell's own offsets (fu, fv in 0..1): upper y = h00 + (h10 −
        // h00) fu + (h11 − h10) fv; lower y = h00 + (h11 − h01) fu + (h01 −
        // h00) fv. Then in world x and z.
        let plane = |su: f32, sv: f32| {
            let bx = su * self.inv;
            let bz = sv * self.inv;
            Plane {
                a: h00 - bx * cx - bz * cz,
                bx,
                bz,
            }
        };
        (plane(h10 - h00, h11 - h10), plane(h11 - h01, h01 - h00))
    }

    /// Which of a cell's planes is under (x, z).
    fn plane_at(&self, x: f32, z: f32) -> Plane {
        let (i, j) = self.cell_of(x, z);
        let fu = (x - self.x0) * self.inv - i as f32;
        let fv = (z - self.z0) * self.inv - j as f32;
        let (up, low) = self.planes(i, j);
        if fu >= fv {
            up
        } else {
            low
        }
    }

    /// The ground's height at (x, z), or `None` off the terrain.
    pub fn height(&self, x: f32, z: f32) -> Option<f32> {
        self.covers(x, z).then(|| self.plane_at(x, z).y(x, z))
    }

    /// The ground's unit normal at (x, z) (straight up off the terrain).
    pub fn normal(&self, x: f32, z: f32) -> V3 {
        if !self.covers(x, z) {
            return V3::new(0.0, 1.0, 0.0);
        }
        let p = self.plane_at(x, z);
        V3::new(-p.bx, 1.0, -p.bz).norm()
    }

    /// Distance along unit `d` from `o` to the ground, or `tmax` if the ray
    /// stays above it that far. A ray that starts under the ground meets it
    /// at once. Walks the blocks, then the cells, the ray crosses in order;
    /// a block or a cell whose highest corner is under the ray's span over
    /// it is skipped.
    pub fn ray(&self, o: V3, d: V3, tmax: f32) -> f32 {
        // The part of the ray over the terrain.
        let mut ta = 0.0f32;
        let mut tb = tmax;
        for (o, d, lo, hi) in [(o.x, d.x, self.x0, self.x1()), (o.z, d.z, self.z0, self.z1())] {
            if d == 0.0 {
                if o < lo || o > hi {
                    return tmax;
                }
            } else {
                let a = (lo - o) / d;
                let b = (hi - o) / d;
                ta = ta.max(a.min(b));
                tb = tb.min(a.max(b));
            }
        }
        if ta > tb {
            return tmax;
        }
        // Block by block; inside a block the ray isn't clear above, cell
        // by cell.
        let (ncx, ncz) = (self.nx - 1, self.nz - 1);
        let (nbx, nbz) = (ncx.div_ceil(BLOCK), ncz.div_ceil(BLOCK));
        let bsize = self.cell * BLOCK as f32;
        let hit = walk(
            bsize,
            (self.x0, self.z0),
            (nbx, nbz),
            o,
            d,
            (ta, tb),
            |bi, bj, b0, b1| {
                if (o.y + d.y * b0).min(o.y + d.y * b1) > self.btop[bj * nbx + bi] {
                    return None;
                }
                let (ci, cj) = (bi * BLOCK, bj * BLOCK);
                let origin = (self.x0 + ci as f32 * self.cell, self.z0 + cj as f32 * self.cell);
                let n = (BLOCK.min(ncx - ci), BLOCK.min(ncz - cj));
                walk(self.cell, origin, n, o, d, (b0, b1), |i, j, t0, t1| {
                    self.ray_cell(ci + i, cj + j, o, d, t0, t1)
                })
            },
        );
        hit.unwrap_or(tmax)
    }

    /// Where the ray meets cell (i, j)'s ground within `[t0, t1]`, if it
    /// does.
    fn ray_cell(&self, i: usize, j: usize, o: V3, d: V3, t0: f32, t1: f32) -> Option<f32> {
        let y0 = o.y + d.y * t0;
        let y1 = o.y + d.y * t1;
        if y0.min(y1) > self.top[j * (self.nx - 1) + i] {
            return None;
        }
        let (up, low) = self.planes(i, j);
        // Which side of the diagonal the ray is on: g = fu − fv, linear in t.
        let (ci, cj) = (i as f32, j as f32);
        let g = |t: f32| ((o.x + d.x * t - self.x0) * self.inv - ci) - ((o.z + d.z * t - self.z0) * self.inv - cj);
        let (g0, g1) = (g(t0), g(t1));
        // The span split where it crosses the diagonal.
        let mut spans = [(t0, t1), (t1, t1)];
        if (g0 < 0.0) != (g1 < 0.0) && g0 != g1 {
            let tm = t0 + (t1 - t0) * (g0 / (g0 - g1));
            spans = [(t0, tm), (tm, t1)];
        }
        for (a, b) in spans {
            if b < a {
                continue;
            }
            let p = if g((a + b) * 0.5) >= 0.0 { up } else { low };
            // Height above the ground along the ray: linear in t.
            let above = |t: f32| o.y + d.y * t - p.y(o.x + d.x * t, o.z + d.z * t);
            let (fa, fb) = (above(a), above(b));
            if fa <= 0.0 {
                return Some(a);
            }
            if fb < 0.0 {
                return Some(a + (b - a) * (fa / (fa - fb)));
            }
        }
        None
    }
}

/// Walks the `n` (x, z) squares of side `size` from `origin` that a ray
/// from `o` along `d` crosses within `span`, in order, calling `f(i, j, t0,
/// t1)` with each one's part of the span until it answers.
fn walk(
    size: f32,
    origin: (f32, f32),
    n: (usize, usize),
    o: V3,
    d: V3,
    span: (f32, f32),
    mut f: impl FnMut(usize, usize, f32, f32) -> Option<f32>,
) -> Option<f32> {
    let (ta, tb) = span;
    let at = |v: f32, v0: f32, n: usize| floor_i32((v - v0) / size).clamp(0, n as i32 - 1) as usize;
    let mut i = at(o.x + d.x * ta, origin.0, n.0);
    let mut j = at(o.z + d.z * ta, origin.1, n.1);
    let edge = |k: usize, v0: f32, o: f32, d: f32| -> (f32, f32) {
        if d > 0.0 {
            ((v0 + (k + 1) as f32 * size - o) / d, size / d)
        } else if d < 0.0 {
            ((v0 + k as f32 * size - o) / d, -size / d)
        } else {
            (f32::INFINITY, f32::INFINITY)
        }
    };
    let (mut nxt_x, step_x) = edge(i, origin.0, o.x, d.x);
    let (mut nxt_z, step_z) = edge(j, origin.1, o.z, d.z);
    let mut t_in = ta;
    loop {
        let t_out = nxt_x.min(nxt_z).min(tb);
        if let Some(t) = f(i, j, t_in, t_out) {
            return Some(t);
        }
        if t_out >= tb {
            return None;
        }
        if nxt_x <= nxt_z {
            if d.x > 0.0 {
                if i + 1 >= n.0 {
                    return None;
                }
                i += 1;
            } else {
                if i == 0 {
                    return None;
                }
                i -= 1;
            }
            nxt_x += step_x;
        } else {
            if d.z > 0.0 {
                if j + 1 >= n.1 {
                    return None;
                }
                j += 1;
            } else {
                if j == 0 {
                    return None;
                }
                j -= 1;
            }
            nxt_z += step_z;
        }
        t_in = t_out;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fmath::fabs;

    /// 8 × 8 m: a 1 m ramp up along x on the first row of cells, then flat.
    const RAMP: [i16; 9] = [0, 128, 128, 0, 128, 128, 0, 128, 128];

    fn ramp() -> Terrain {
        Terrain::build(&TerrainDef {
            cell: 4.0,
            x0: 0.0,
            z0: 0.0,
            nx: 3,
            nz: 3,
            heights: &RAMP,
        })
    }

    #[test]
    fn heights_are_the_corners_and_planar_between() {
        let t = ramp();
        assert_eq!(t.height(0.0, 0.0), Some(0.0));
        assert_eq!(t.height(4.0, 2.0), Some(1.0));
        assert_eq!(t.height(2.0, 3.0), Some(0.5));
        assert_eq!(t.height(6.0, 6.0), Some(1.0));
        assert_eq!(t.height(-0.1, 1.0), None);
        let n = t.normal(1.0, 1.0);
        // A 1 in 4 slope rising along +x leans the normal toward −x.
        assert!(n.x < 0.0 && n.z == 0.0 && n.y > 0.9, "{n:?}");
        assert_eq!(t.normal(6.0, 6.0), V3::new(0.0, 1.0, 0.0));
    }

    #[test]
    fn a_ray_meets_the_slope_where_the_planes_say() {
        let t = ramp();
        // Level at 0.5 m along +x: meets the ramp at x = 2.
        let hit = t.ray(V3::new(-1.0, 0.5, 2.0), V3::new(1.0, 0.0, 0.0), 20.0);
        assert!(fabs(hit - 3.0) < 1e-5, "{hit}");
        // Above the plateau it never touches.
        assert_eq!(t.ray(V3::new(-1.0, 1.5, 2.0), V3::new(1.0, 0.0, 0.0), 20.0), 20.0);
        // Straight down onto the plateau.
        let down = t.ray(V3::new(6.0, 5.0, 6.0), V3::new(0.0, -1.0, 0.0), 20.0);
        assert!(fabs(down - 4.0) < 1e-6, "{down}");
        // Under the ground: at once.
        assert_eq!(t.ray(V3::new(6.0, 0.5, 6.0), V3::new(1.0, 0.0, 0.0), 20.0), 0.0);
        // Diagonally, across the split of every cell.
        let dd = V3::new(1.0, -0.1, 1.0).norm();
        let hit = t.ray(V3::new(0.5, 1.2, 0.5), dd, 20.0);
        let p = V3::new(0.5, 1.2, 0.5) + dd * hit;
        let g = t.height(p.x, p.z).unwrap();
        assert!(fabs(p.y - g) < 1e-4, "{p:?} ground {g}");
    }
}

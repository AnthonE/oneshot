//! Where a map's boxes are, by 8 m cell in x and z, so a ray or a footprint
//! tests only the boxes near it (docs/DESIGN.md §8: fog's cost at 8v8).
//! Built once with the map; the tick only reads it.
//!
//! Every query it serves takes a min or a max over the boxes it is shown
//! ([`crate::map::Map::ray`], `floor`, `ceiling`, movement's sweep), so
//! visiting fewer boxes, in another order, gives the same bits as the
//! linear scan. A box is listed in every cell its footprint, widened by
//! [`MARGIN`], touches, so a cell walk that rounds at a boundary still
//! meets it. The map tests hold the grid to the linear scan bit for bit.

use crate::consts::MAX_MAP_BOXES;
use crate::fmath::floor_i32;
use crate::geom::{Aabb, V3};

/// A cell's side. A power of two, so its inverse is exact.
pub const CELL: f32 = 8.0;
const INV: f32 = 1.0 / CELL;
/// How far past its footprint a box is listed: far more than a cell walk's
/// rounding, far less than a cell.
pub const MARGIN: f32 = 0.05;

/// Which boxes one query has met already, so a box listed in several
/// cells is tested once. On the stack; no allocation in the tick.
pub struct Seen([u64; MAX_MAP_BOXES / 64]);

impl Seen {
    pub fn new() -> Seen {
        Seen([0; MAX_MAP_BOXES / 64])
    }

    /// Marks `i`; true the first time.
    fn first(&mut self, i: usize) -> bool {
        let (w, b) = (i / 64, 1u64 << (i % 64));
        let new = self.0[w] & b == 0;
        self.0[w] |= b;
        new
    }
}

impl Default for Seen {
    fn default() -> Self {
        Seen::new()
    }
}

#[derive(Clone, Debug, Default)]
pub struct Grid {
    /// The lowest corner of cell (0, 0).
    x0: f32,
    z0: f32,
    nx: usize,
    nz: usize,
    /// Cell `c`'s boxes are `items[start[c]..start[c + 1]]`, ascending.
    start: Vec<u32>,
    items: Vec<u16>,
}

impl Grid {
    pub fn build(boxes: &[Aabb]) -> Grid {
        assert!(
            boxes.len() <= MAX_MAP_BOXES,
            "{} boxes, at most {MAX_MAP_BOXES}",
            boxes.len()
        );
        if boxes.is_empty() {
            return Grid::default();
        }
        let (mut lx, mut lz, mut hx, mut hz) = (f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
        for b in boxes {
            lx = lx.min(b.min.x - MARGIN);
            lz = lz.min(b.min.z - MARGIN);
            hx = hx.max(b.max.x + MARGIN);
            hz = hz.max(b.max.z + MARGIN);
        }
        // Whole cells from a multiple of CELL, so every map's cells line up
        // the same way.
        let x0 = floor_i32(lx * INV) as f32 * CELL;
        let z0 = floor_i32(lz * INV) as f32 * CELL;
        let nx = (floor_i32((hx - x0) * INV) + 1) as usize;
        let nz = (floor_i32((hz - z0) * INV) + 1) as usize;
        let mut start = Vec::with_capacity(nx * nz + 1);
        let mut items = Vec::new();
        for iz in 0..nz {
            for ix in 0..nx {
                start.push(items.len() as u32);
                let (cx, cz) = (x0 + ix as f32 * CELL, z0 + iz as f32 * CELL);
                for (i, b) in boxes.iter().enumerate() {
                    let over = b.min.x - MARGIN <= cx + CELL
                        && b.max.x + MARGIN >= cx
                        && b.min.z - MARGIN <= cz + CELL
                        && b.max.z + MARGIN >= cz;
                    if over {
                        items.push(i as u16);
                    }
                }
            }
        }
        start.push(items.len() as u32);
        Grid {
            x0,
            z0,
            nx,
            nz,
            start,
            items,
        }
    }

    fn cell(&self, ix: usize, iz: usize) -> &[u16] {
        let c = iz * self.nx + ix;
        &self.items[self.start[c] as usize..self.start[c + 1] as usize]
    }

    /// The cell index along one axis of coordinate `v`, clamped to the grid.
    fn index(v: f32, v0: f32, n: usize) -> usize {
        floor_i32((v - v0) * INV).clamp(0, n as i32 - 1) as usize
    }

    /// Calls `f` once for each box whose footprint may overlap the
    /// rectangle `lo..hi` in x and z (and some that don't: `f` checks), in
    /// ascending box order: the linear scan's order, less the boxes that
    /// can't matter.
    pub fn each_in(&self, lo: (f32, f32), hi: (f32, f32), mut f: impl FnMut(usize)) {
        if self.nx == 0 {
            return;
        }
        let gx1 = self.x0 + self.nx as f32 * CELL;
        let gz1 = self.z0 + self.nz as f32 * CELL;
        if hi.0 < self.x0 || lo.0 > gx1 || hi.1 < self.z0 || lo.1 > gz1 {
            return;
        }
        let (ix0, ix1) = (Self::index(lo.0, self.x0, self.nx), Self::index(hi.0, self.x0, self.nx));
        let (iz0, iz1) = (Self::index(lo.1, self.z0, self.nz), Self::index(hi.1, self.z0, self.nz));
        let mut seen = Seen::new();
        for iz in iz0..=iz1 {
            for ix in ix0..=ix1 {
                for &i in self.cell(ix, iz) {
                    seen.first(i as usize);
                }
            }
        }
        for (w, &bits) in seen.0.iter().enumerate() {
            let mut b = bits;
            while b != 0 {
                let k = b.trailing_zeros() as usize;
                f(w * 64 + k);
                b &= b - 1;
            }
        }
    }

    /// Walks the cells a ray from `o` along `d` crosses up to `tmax`, in
    /// order, calling `f(box)` once per box listed there. `f` returns the
    /// nearest hit so far; the walk stops once the next cell starts past it.
    pub fn each_on_ray(&self, o: V3, d: V3, tmax: f32, mut f: impl FnMut(usize) -> f32) {
        if self.nx == 0 {
            return;
        }
        let gx1 = self.x0 + self.nx as f32 * CELL;
        let gz1 = self.z0 + self.nz as f32 * CELL;
        // The part of the ray over the grid, in x and z.
        let mut ta = 0.0f32;
        let mut tb = tmax;
        for (o, d, lo, hi) in [(o.x, d.x, self.x0, gx1), (o.z, d.z, self.z0, gz1)] {
            if d == 0.0 {
                if o < lo || o > hi {
                    return;
                }
            } else {
                let a = (lo - o) / d;
                let b = (hi - o) / d;
                ta = ta.max(a.min(b));
                tb = tb.min(a.max(b));
            }
        }
        if ta > tb {
            return;
        }
        let mut seen = Seen::new();
        let px = o.x + d.x * ta;
        let pz = o.z + d.z * ta;
        let mut ix = Self::index(px, self.x0, self.nx);
        let mut iz = Self::index(pz, self.z0, self.nz);
        // Where the ray crosses the next x and z cell edges, and how far
        // apart those crossings are.
        let edge = |i: usize, v0: f32, o: f32, d: f32| -> (f32, f32) {
            if d > 0.0 {
                ((v0 + (i + 1) as f32 * CELL - o) / d, CELL / d)
            } else if d < 0.0 {
                ((v0 + i as f32 * CELL - o) / d, -CELL / d)
            } else {
                (f32::INFINITY, f32::INFINITY)
            }
        };
        let (mut nxt_x, step_x) = edge(ix, self.x0, o.x, d.x);
        let (mut nxt_z, step_z) = edge(iz, self.z0, o.z, d.z);
        let mut best = tmax;
        loop {
            for &i in self.cell(ix, iz) {
                if seen.first(i as usize) {
                    best = f(i as usize);
                }
            }
            let next = nxt_x.min(nxt_z);
            if next >= best || next > tb {
                return;
            }
            if nxt_x <= nxt_z {
                if d.x > 0.0 {
                    if ix + 1 >= self.nx {
                        return;
                    }
                    ix += 1;
                } else {
                    if ix == 0 {
                        return;
                    }
                    ix -= 1;
                }
                nxt_x += step_x;
            } else {
                if d.z > 0.0 {
                    if iz + 1 >= self.nz {
                        return;
                    }
                    iz += 1;
                } else {
                    if iz == 0 {
                        return;
                    }
                    iz -= 1;
                }
                nxt_z += step_z;
            }
        }
    }
}

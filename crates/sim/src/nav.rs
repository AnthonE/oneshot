//! Where a bot walks, or drives, on any map: a waypoint graph over
//! everywhere a body can stand (every level), another over everywhere a
//! truck can go on a map with vehicles, and for each of the map's spots
//! (its roam points, flag bases, vehicles, siege nodes, pads and spawns)
//! how far each waypoint is from it. A bot steers at the farthest waypoint
//! down the way it can see ([`Graph::toward`]); to a goal that moves (a
//! flag on the floor, a carrier), by the spot nearest it ([`Graph::route`]).
//!
//! Built once a map, the first time a bot asks ([`Nav::of`]), and kept:
//! the client never asks, the server builds them all as it starts. The
//! tick only reads them. The flood is the map test's
//! (`tests/maps.rs`): steps that climb at most a step and no slope too
//! steep to stand on, drops off ledges, launchers' throws (on foot).

use std::collections::{BTreeMap, VecDeque};
use std::sync::OnceLock;

use crate::consts::{BODY_H, BODY_R, MAX_LAUNCHERS, STEP, TICK_HZ, VEH_H, VEH_HALF_W, VEH_STEP};
use crate::fmath::round;
use crate::geom::V3;
use crate::input::Input;
use crate::map::Map;
use crate::movement::{step, too_steep, Body};

/// Not reachable.
pub const FAR: u16 = u16::MAX;
const NONE: u32 = u32::MAX;
/// Two waypoints are linked if one reaches the other in this many steps.
const LINK_STEPS: u8 = 10;
/// The nearest-waypoint index's cell, metres.
const CELL: f32 = 8.0;
/// How much a level's height counts for against distance across, finding
/// the waypoint a body is at.
const KY: f32 = 3.0;
/// A rise or fall this big from one waypoint to the next is a change of
/// level: a way's look ahead stops there.
const LEVEL: f32 = 1.2;

/// What finds its way over a graph: how fine its flood steps, every how
/// many steps a waypoint stands, how wide and tall it is, how high a step
/// it climbs, whether launchers throw it, and the line it looks along for
/// a clear way: how high, and how far to each side another runs.
struct Walker {
    g: f32,
    every: i32,
    r: f32,
    h: f32,
    step: f32,
    throws: bool,
    eye: f32,
    side: f32,
}

/// On foot: a way is clear for a body's width, not a line's (a line that
/// grazes a corner is a way a body can't take).
const FOOT: Walker = Walker {
    g: 0.5,
    every: 4,
    r: BODY_R,
    h: BODY_H,
    step: STEP,
    throws: true,
    eye: 0.9,
    side: BODY_R,
};

/// A truck: kept wider than it is, for its length as it turns.
const WHEELS: Walker = Walker {
    g: 1.0,
    every: 4,
    r: VEH_HALF_W + 0.5,
    h: VEH_H,
    step: VEH_STEP,
    throws: false,
    eye: 1.0,
    side: VEH_HALF_W,
};

/// One walker's waypoints, their links and every spot's field.
#[derive(Clone, Debug, Default)]
pub struct Graph {
    /// The waypoints, on the floor.
    pub nodes: Vec<V3>,
    /// Waypoint `i`'s links (to where it can go) are
    /// `links[start[i]..start[i + 1]]`.
    start: Vec<u32>,
    links: Vec<u16>,
    /// Whether each link is a launcher's throw (from the waypoint on its
    /// pad).
    throw: Vec<bool>,
    /// The links that go off a ledge (by index, in order), and where a body
    /// lands just past its edge: what a way off it heads for.
    drops: Vec<(u32, V3)>,
    /// For each spot, each waypoint's cost to go to it ([`FAR`]: it can't).
    fields: Vec<Vec<u16>>,
    /// For each spot, the waypoint its field leads to.
    ends: Vec<u16>,
    /// The waypoints by [`CELL`] across, for [`Graph::nearest`]:
    /// cell `c`'s are `by_cell[cells[c]..cells[c + 1]]`.
    cells: Vec<u32>,
    by_cell: Vec<u16>,
    cell0: (f32, f32),
    cols: usize,
    rows: usize,
    eye: f32,
    side: f32,
    /// A waypoint's spacing, about.
    spacing: f32,
}

/// A map's graphs, and the spots their fields lead to, in this order: its
/// roam points, its flag bases, its vehicles' spawns, its siege nodes,
/// its pads, its spawns.
#[derive(Clone, Debug, Default)]
pub struct Nav {
    pub foot: Graph,
    /// For trucks (and skimmers), on a map with vehicles.
    pub wheels: Option<Graph>,
    pub spots: Vec<V3>,
    flag0: usize,
    vehicle0: usize,
    siege0: usize,
}

static NAVS: [OnceLock<Option<Nav>>; 32] = [const { OnceLock::new() }; 32];

impl Nav {
    /// The waypoints of `map` if it is one of the game's (by its id and
    /// name), built the first time any map asks and kept. None: a map built
    /// in a test.
    pub fn of(map: &Map) -> Option<&'static Nav> {
        let i = map.id as usize;
        let d = crate::maps::ALL.iter().find(|d| d.id == map.id && d.name == map.name)?;
        NAVS.get(i)?.get_or_init(|| Some(Nav::build(&d.build()))).as_ref()
    }

    pub fn build(m: &Map) -> Nav {
        let mut spots: Vec<V3> = m.roam.clone();
        let flag0 = spots.len();
        spots.extend(m.flags.iter().flatten());
        let vehicle0 = spots.len();
        spots.extend(m.vehicles.iter().map(|v| v.pos));
        let siege0 = spots.len();
        spots.extend(m.siege.iter().flatten());
        spots.extend(m.pads.iter().map(|p| p.pos));
        spots.extend(m.spawns.iter().map(|s| s.pos));
        let from: Vec<V3> = m.spawns.iter().map(|s| s.pos).collect();
        let (mut foot, back) = Graph::build(m, &FOOT, &from);
        let ends: Vec<Option<usize>> = spots.iter().map(|&s| foot.nearest(s)).collect();
        foot.fields_to(&ends, &back);
        let wheels = (!m.vehicles.is_empty()).then(|| {
            let from: Vec<V3> = m
                .vehicles
                .iter()
                .map(|v| V3::new(v.pos.x, m.floor(v.pos.x, v.pos.z, WHEELS.r, v.pos.y + 1.0), v.pos.z))
                .collect();
            let (mut g, back) = Graph::build(m, &WHEELS, &from);
            // A truck's way to a spot ends where the walk on from it is
            // shortest (a flag inside its base: at the door), among those
            // near it; else at the nearest.
            let ends: Vec<Option<usize>> = spots
                .iter()
                .enumerate()
                .map(|(k, &s)| {
                    let mut best = g.nearest(s);
                    let mut bc = FAR;
                    for (i, &q) in g.nodes.iter().enumerate() {
                        if (q - s).flat().len() > TRUCK_END_M {
                            continue;
                        }
                        let c = foot.nearest(q).map_or(FAR, |f| foot.fields[k][f]);
                        if c < bc {
                            bc = c;
                            best = Some(i);
                        }
                    }
                    best
                })
                .collect();
            g.fields_to(&ends, &back);
            g
        });
        Nav {
            foot,
            wheels,
            spots,
            flag0,
            vehicle0,
            siege0,
        }
    }

    /// Roam point `i`'s spot.
    pub fn roam(&self, i: usize) -> usize {
        i
    }

    /// Side `t`'s flag base's spot.
    pub fn flag(&self, t: usize) -> usize {
        self.flag0 + t
    }

    /// Vehicle `i`'s spawn's spot.
    pub fn vehicle(&self, i: usize) -> usize {
        self.vehicle0 + i
    }

    /// Siege node `n`'s spot.
    pub fn siege(&self, n: usize) -> usize {
        self.siege0 + n
    }
}

/// The flood: every node a walker reaches, its four ways and its throws.
struct Flood {
    keys: Vec<(i32, i32, i32)>,
    next: Vec<[u32; 4]>,
    throws: Vec<(u32, u32)>,
    /// Nodes by column: the first at (x, z), then each the next at the same.
    head: Vec<u32>,
    below: Vec<u32>,
    x0: i32,
    z0: i32,
    nz: usize,
}

impl Flood {
    fn col(&self, k: (i32, i32, i32)) -> Option<usize> {
        let (x, z) = (k.0 - self.x0, k.1 - self.z0);
        (x >= 0 && z >= 0 && (z as usize) < self.nz)
            .then(|| x as usize * self.nz + z as usize)
            .filter(|&c| c < self.head.len())
    }

    fn find(&self, k: (i32, i32, i32)) -> Option<u32> {
        let mut i = self.head[self.col(k)?];
        while i != NONE {
            if self.keys[i as usize] == k {
                return Some(i);
            }
            i = self.below[i as usize];
        }
        None
    }

    /// The node at `k`, added if new (and whether it was).
    fn add(&mut self, k: (i32, i32, i32)) -> Option<(u32, bool)> {
        if let Some(i) = self.find(k) {
            return Some((i, false));
        }
        let c = self.col(k)?;
        let i = self.keys.len() as u32;
        self.keys.push(k);
        self.next.push([NONE; 4]);
        self.below.push(self.head[c]);
        self.head[c] = i;
        Some((i, true))
    }
}

/// How far from a spot a truck's way to it may end ([`Nav::build`]).
const TRUCK_END_M: f32 = 40.0;

/// A graph's links reversed, with their costs: `back[bstart[i]..bstart[i +
/// 1]]` are the waypoints that reach `i`.
struct Back {
    bstart: Vec<u32>,
    back: Vec<(u16, u8)>,
}

impl Graph {
    fn build(m: &Map, w: &Walker, from: &[V3]) -> (Graph, Back) {
        let g = w.g;
        let key = |x: f32, z: f32, y: f32| (round(x / g) as i32, round(z / g) as i32, round(y * 64.0) as i32);
        let at = |k: (i32, i32, i32)| V3::new(k.0 as f32 * g, k.2 as f32 / 64.0, k.1 as f32 * g);
        let x0 = round(m.lo.0 / g) as i32 - 2;
        let z0 = round(m.lo.1 / g) as i32 - 2;
        let nx = (round(m.hi.0 / g) as i32 - x0 + 3).max(1) as usize;
        let nz = (round(m.hi.1 / g) as i32 - z0 + 3).max(1) as usize;
        let mut fl = Flood {
            keys: Vec::new(),
            next: Vec::new(),
            throws: Vec::new(),
            head: vec![NONE; nx * nz],
            below: Vec::new(),
            x0,
            z0,
            nz,
        };
        // Where each launcher throws a body standing in it.
        let lands: Vec<(crate::geom::Aabb, V3)> = if w.throws {
            m.launchers
                .iter()
                .take(MAX_LAUNCHERS)
                .map(|l| {
                    let mid = (l.area.min + l.area.max) * 0.5;
                    let mut b = Body {
                        pos: V3::new(mid.x, m.floor(mid.x, mid.z, BODY_R, l.area.max.y), mid.z),
                        grounded: true,
                        ..Body::default()
                    };
                    step(&mut b, &Input::default(), m);
                    let mut n = 0;
                    while !b.grounded && n < TICK_HZ * 6 {
                        step(&mut b, &Input::default(), m);
                        n += 1;
                    }
                    (l.area, b.pos)
                })
                .collect()
        } else {
            Vec::new()
        };
        let mut q = VecDeque::new();
        for s in from {
            if let Some((i, true)) = fl.add(key(s.x, s.z, s.y)) {
                q.push_back(i);
            }
        }
        let r = w.r;
        while let Some(i) = q.pop_front() {
            let k = fl.keys[i as usize];
            let p = at(k);
            for (n, (dx, dz)) in [(1, 0), (-1, 0), (0, 1), (0, -1)].into_iter().enumerate() {
                let (x, z) = ((k.0 + dx) as f32 * g, (k.1 + dz) as f32 * g);
                if x - r < m.lo.0 || x + r > m.hi.0 || z - r < m.lo.1 || z + r > m.hi.1 {
                    continue;
                }
                let ny = m.floor_with(x, z, r, p.y + w.step, p.y + w.step);
                // Over the edge of a void is no way anywhere; and a drop is
                // clear all the way down from where its head was (a rail
                // along a ledge's edge stops it).
                if m.lost_floor(x, z, ny) || m.occupied(x, z, r, ny, ny.max(p.y) + w.h) {
                    continue;
                }
                // Up ground too steep to climb.
                if m.terrain.is_some() && too_steep(m, (p.x, p.z), (x, z), p.y) {
                    continue;
                }
                if let Some((j, new)) = fl.add(key(x, z, ny)) {
                    fl.next[i as usize][n] = j;
                    if new {
                        q.push_back(j);
                    }
                }
            }
            for (a, land) in &lands {
                if p.x >= a.min.x
                    && p.x <= a.max.x
                    && p.z >= a.min.z
                    && p.z <= a.max.z
                    && p.y >= a.min.y
                    && p.y <= a.max.y
                {
                    if let Some((j, new)) = fl.add(key(land.x, land.z, land.y)) {
                        fl.throws.push((i, j));
                        if new {
                            q.push_back(j);
                        }
                    }
                }
            }
        }
        fl.throws.sort_unstable();

        // Waypoints: one in every `every` by `every` block of steps on each
        // level, the one nearest the block's corner (a lattice, where the
        // floor is open; a narrow way off it still has its own).
        let e = w.every;
        let mut best: BTreeMap<(i32, i32, i32), (i32, u32)> = BTreeMap::new();
        for (i, k) in fl.keys.iter().enumerate() {
            let (bx, bz) = (k.0.div_euclid(e), k.1.div_euclid(e));
            let level = round(k.2 as f32 / 64.0 / 1.5) as i32;
            let off = (k.0 - bx * e) + (k.1 - bz * e);
            let b = best.entry((bx, bz, level)).or_insert((i32::MAX, NONE));
            if off < b.0 || (off == b.0 && (i as u32) < b.1) {
                *b = (off, i as u32);
            }
        }
        let mut way: Vec<u32> = best.values().map(|&(_, i)| i).collect();
        // A waypoint on each launcher's pad, where its throw starts: the
        // node thrown from nearest the pad's middle.
        for (a, _) in &lands {
            let mid = (a.min + a.max) * 0.5;
            let on = fl.throws.iter().map(|&(i, _)| i).min_by(|&i, &j| {
                let d = |k: u32| (at(fl.keys[k as usize]) - mid).flat().len();
                d(i).total_cmp(&d(j)).then(i.cmp(&j))
            });
            if let Some(i) = on.filter(|&i| {
                let p = at(fl.keys[i as usize]);
                p.x >= a.min.x && p.x <= a.max.x && p.z >= a.min.z && p.z <= a.max.z
            }) {
                way.push(i);
            }
        }
        way.sort_unstable();
        way.dedup();
        way.truncate(u16::MAX as usize - 1);
        let mut way_of = vec![u16::MAX; fl.keys.len()];
        for (wi, &f) in way.iter().enumerate() {
            way_of[f as usize] = wi as u16;
        }

        // Links: what each waypoint reaches within LINK_STEPS (a short
        // walk: a bot heading at it gets there), and a launcher's throw met
        // on the way as a link of its own.
        let mut start = Vec::with_capacity(way.len() + 1);
        let mut links: Vec<u16> = Vec::new();
        let mut costs: Vec<u8> = Vec::new();
        let mut throw: Vec<bool> = Vec::new();
        let mut depth = vec![0u8; fl.keys.len()];
        let mut stamp = vec![NONE; fl.keys.len()];
        let mut parent = vec![NONE; fl.keys.len()];
        let mut drops: Vec<(u32, V3)> = Vec::new();
        let mut bq: VecDeque<u32> = VecDeque::new();
        for (wi, &f) in way.iter().enumerate() {
            start.push(links.len() as u32);
            let first = links.len();
            stamp[f as usize] = wi as u32;
            depth[f as usize] = 0;
            bq.clear();
            bq.push_back(f);
            let found =
                |wj: u16, d: u8, thrown: bool, links: &mut Vec<u16>, costs: &mut Vec<u8>, throw: &mut Vec<bool>| {
                    if wj as usize != wi && !links[first..].contains(&wj) {
                        links.push(wj);
                        costs.push(d);
                        throw.push(thrown);
                    }
                };
            while let Some(i) = bq.pop_front() {
                let d = depth[i as usize];
                if d >= LINK_STEPS {
                    continue;
                }
                // A throw only from the pad's own waypoint: walked to as any
                // other, then a link of its own.
                if i == f {
                    let lo = fl.throws.partition_point(|&(a, _)| a < i);
                    for &(_, j) in fl.throws[lo..].iter().take_while(|&&(a, _)| a == i) {
                        if let Some(wj) = nearest_way(&fl, &way_of, fl.keys[j as usize], e) {
                            found(wj, 1, true, &mut links, &mut costs, &mut throw);
                        }
                    }
                }
                for j in fl.next[i as usize] {
                    if j == NONE || stamp[j as usize] == wi as u32 {
                        continue;
                    }
                    stamp[j as usize] = wi as u32;
                    depth[j as usize] = d + 1;
                    parent[j as usize] = i;
                    if way_of[j as usize] != u16::MAX {
                        let before = links.len();
                        found(way_of[j as usize], d + 1, false, &mut links, &mut costs, &mut throw);
                        // Off a ledge on the way: where the body lands
                        // past its edge, the first step down from f's side.
                        if links.len() > before {
                            let mut land = None;
                            let mut b = j;
                            while b != f && parent[b as usize] != NONE {
                                let a = parent[b as usize];
                                if at(fl.keys[a as usize]).y - at(fl.keys[b as usize]).y > LEVEL {
                                    land = Some(at(fl.keys[b as usize]));
                                }
                                b = a;
                            }
                            if let Some(l) = land {
                                drops.push((before as u32, l));
                            }
                        }
                    }
                    bq.push_back(j);
                }
            }
        }
        start.push(links.len() as u32);
        let nodes: Vec<V3> = way.iter().map(|&f| at(fl.keys[f as usize])).collect();
        drop(fl);

        // The links reversed, for each spot's field: costs to it.
        let n = nodes.len();
        let mut bstart = vec![0u32; n + 1];
        for &b in &links {
            bstart[b as usize + 1] += 1;
        }
        for i in 0..n {
            bstart[i + 1] += bstart[i];
        }
        let mut fill = bstart.clone();
        let mut back = vec![(0u16, 0u8); links.len()];
        for a in 0..n {
            for li in start[a] as usize..start[a + 1] as usize {
                let b = links[li] as usize;
                back[fill[b] as usize] = (a as u16, costs[li]);
                fill[b] += 1;
            }
        }
        let mut graph = Graph {
            nodes,
            start,
            links,
            throw,
            drops,
            eye: w.eye,
            side: w.side,
            spacing: w.g * w.every as f32,
            ..Graph::default()
        };
        graph.index();
        (graph, Back { bstart, back })
    }

    /// Each spot's field, to waypoint `ends[k]` for spot `k` (none: no way
    /// to it). Dial's: the costs are small integers, so a ring of buckets
    /// is an exact shortest walk.
    fn fields_to(&mut self, ends: &[Option<usize>], b: &Back) {
        const RING: usize = LINK_STEPS as usize + 2;
        let n = self.nodes.len();
        let mut ring: Vec<Vec<u16>> = vec![Vec::new(); RING];
        for &end in ends {
            let mut f = vec![FAR; n];
            let Some(t) = end else {
                self.fields.push(f);
                self.ends.push(0);
                continue;
            };
            f[t] = 0;
            ring[0].push(t as u16);
            let mut at_cost = 0usize;
            let mut left = 1usize;
            while left > 0 {
                let slot = at_cost % RING;
                while let Some(i) = ring[slot].pop() {
                    left -= 1;
                    let i = i as usize;
                    if f[i] as usize != at_cost {
                        continue;
                    }
                    for &(a, c) in &b.back[b.bstart[i] as usize..b.bstart[i + 1] as usize] {
                        let nd = f[i].saturating_add(c as u16);
                        if nd < f[a as usize] {
                            f[a as usize] = nd;
                            ring[nd as usize % RING].push(a);
                            left += 1;
                        }
                    }
                }
                at_cost += 1;
            }
            self.fields.push(f);
            self.ends.push(t as u16);
        }
    }

    /// The nearest-waypoint index: every waypoint by its cell.
    fn index(&mut self) {
        let (mut lo, mut hi) = ((f32::MAX, f32::MAX), (f32::MIN, f32::MIN));
        for p in &self.nodes {
            lo = (lo.0.min(p.x), lo.1.min(p.z));
            hi = (hi.0.max(p.x), hi.1.max(p.z));
        }
        if self.nodes.is_empty() {
            lo = (0.0, 0.0);
            hi = (0.0, 0.0);
        }
        self.cell0 = lo;
        self.cols = ((hi.0 - lo.0) / CELL) as usize + 1;
        self.rows = ((hi.1 - lo.1) / CELL) as usize + 1;
        let cell = |p: V3| ((p.x - lo.0) / CELL) as usize * self.rows + ((p.z - lo.1) / CELL) as usize;
        let mut cells = vec![0u32; self.cols * self.rows + 1];
        for p in &self.nodes {
            cells[cell(*p) + 1] += 1;
        }
        for c in 0..self.cols * self.rows {
            cells[c + 1] += cells[c];
        }
        let mut fill = cells.clone();
        let mut by_cell = vec![0u16; self.nodes.len()];
        for (i, p) in self.nodes.iter().enumerate() {
            let c = cell(*p);
            by_cell[fill[c] as usize] = i as u16;
            fill[c] += 1;
        }
        self.cells = cells;
        self.by_cell = by_cell;
    }

    fn out(&self, i: usize) -> &[u16] {
        &self.links[self.start[i] as usize..self.start[i + 1] as usize]
    }

    /// The waypoint a body at `p` is at: the nearest, a level's height
    /// counting for more than distance across.
    pub fn nearest(&self, p: V3) -> Option<usize> {
        if self.nodes.is_empty() {
            return None;
        }
        let d2 = |q: V3| {
            let (dx, dy, dz) = (q.x - p.x, (q.y - p.y) * KY, q.z - p.z);
            dx * dx + dy * dy + dz * dz
        };
        let cx = ((p.x - self.cell0.0) / CELL).clamp(0.0, (self.cols - 1) as f32) as i32;
        let cz = ((p.z - self.cell0.1) / CELL).clamp(0.0, (self.rows - 1) as f32) as i32;
        let (mut best, mut bd) = (None, f32::INFINITY);
        let reach = self.cols.max(self.rows) as i32;
        for ring in 0..=reach {
            for x in cx - ring..=cx + ring {
                for z in cz - ring..=cz + ring {
                    let edge = (x - cx).abs() == ring || (z - cz).abs() == ring;
                    if !edge || x < 0 || z < 0 || x >= self.cols as i32 || z >= self.rows as i32 {
                        continue;
                    }
                    let c = x as usize * self.rows + z as usize;
                    for &i in &self.by_cell[self.cells[c] as usize..self.cells[c + 1] as usize] {
                        let d = d2(self.nodes[i as usize]);
                        if d < bd || (d == bd && best.is_some_and(|b| (i as usize) < b)) {
                            bd = d;
                            best = Some(i as usize);
                        }
                    }
                }
            }
            // Nothing in the next ring can be nearer than its inner edge.
            let inner = ring as f32 * CELL;
            if best.is_some() && bd <= inner * inner {
                break;
            }
        }
        best
    }

    /// [`Self::nearest`] for a body that was nearest waypoint `hint` a
    /// moment ago: that one or one it links to, if one is within a
    /// waypoint's spacing; else every waypoint. `u16::MAX`: no hint.
    fn nearest_from(&self, p: V3, hint: u16) -> Option<usize> {
        let h = hint as usize;
        if h < self.nodes.len() {
            let d2 = |i: usize| {
                let q = self.nodes[i];
                let (dx, dy, dz) = (q.x - p.x, (q.y - p.y) * KY, q.z - p.z);
                dx * dx + dy * dy + dz * dz
            };
            let mut best = h;
            let mut bd = d2(h);
            for &j in self.out(h) {
                let d = d2(j as usize);
                if d < bd {
                    bd = d;
                    best = j as usize;
                }
            }
            if bd < self.spacing * self.spacing {
                return Some(best);
            }
        }
        self.nearest(p)
    }

    /// Whether the way from `a` to `b` is clear for what walks this graph:
    /// at its eye, and as wide as it is.
    pub fn clear(&self, map: &Map, a: V3, b: V3) -> bool {
        let up = V3::new(0.0, self.eye, 0.0);
        if !map.clear(a + up, b + up) {
            return false;
        }
        if self.side == 0.0 {
            return true;
        }
        let d = (b - a).flat();
        let l = d.len();
        if l < 0.01 {
            return true;
        }
        let across = V3::new(-d.z / l, 0.0, d.x / l) * self.side;
        map.clear(a + up + across, b + up + across) && map.clear(a + up - across, b + up - across)
    }

    /// The cost from where a body at `p` stands to spot `spot` ([`FAR`]:
    /// no way; `way` as [`Self::toward`]'s).
    pub fn cost(&self, spot: usize, p: V3, way: &mut u16) -> u16 {
        let Some(f) = self.fields.get(spot) else {
            return FAR;
        };
        match self.nearest_from(p, *way) {
            Some(cur) => {
                *way = cur as u16;
                f[cur]
            }
            None => FAR,
        }
    }

    /// Where spot `spot`'s field ends: the waypoint nearest it (for a
    /// truck, as near it as one goes).
    pub fn end(&self, spot: usize) -> Option<V3> {
        let e = *self.ends.get(spot)? as usize;
        (self.fields[spot].get(e) == Some(&0)).then(|| self.nodes[e])
    }

    /// Where a body at `p` heading for spot `spot` should steer: down the
    /// field a few waypoints, the farthest it can see. None: no field, or
    /// nowhere to go from here (head straight). `way`: the waypoint it was
    /// nearest last time, updated (`u16::MAX`: none).
    pub fn toward(&self, map: &Map, spot: usize, p: V3, way: &mut u16) -> Option<V3> {
        let f = self.fields.get(spot)?;
        let mut cur = self.nearest_from(p, *way)?;
        *way = cur as u16;
        if f[cur] == FAR {
            return None;
        }
        let mut aim = None;
        for _ in 0..4 {
            let mut best = cur;
            let mut thrown = false;
            let mut link = 0usize;
            let lo = self.start[cur] as usize;
            for (k, &j) in self.out(cur).iter().enumerate() {
                if f[j as usize] < f[best] {
                    best = j as usize;
                    thrown = self.throw[lo + k];
                    link = lo + k;
                }
            }
            if best == cur {
                break;
            }
            // A launcher next: onto its pad, never past it to where it
            // throws.
            if thrown {
                let q = self.nodes[cur];
                if aim.is_none() || self.clear(map, p, q) {
                    aim = Some(q);
                }
                break;
            }
            // A drop or a climb next: to its foot (or its edge) first,
            // never seen past from another level; off a ledge, over its
            // edge where the way goes (not at the floor under it).
            let level = crate::fmath::fabs(self.nodes[best].y - self.nodes[cur].y) > LEVEL;
            let over = level
                .then(|| self.drops.binary_search_by_key(&(link as u32), |&(l, _)| l).ok())
                .flatten()
                .map(|d| self.drops[d].1);
            cur = best;
            let q = over.unwrap_or(self.nodes[cur]);
            if aim.is_none() || (!level && self.clear(map, p, q)) {
                aim = Some(q);
            } else {
                break;
            }
            if level {
                break;
            }
        }
        aim
    }

    /// Where a body at `p` heading for `goal`, a point anywhere, should
    /// steer: straight at it if it is near and the way is clear, else down
    /// the field of the spot nearest it on foot, and straight once there.
    /// None: no way from here.
    pub fn route(&self, map: &Map, goal: V3, p: V3, way: &mut u16) -> Option<V3> {
        let near = (goal - p).flat().len() < ROUTE_STRAIGHT && crate::fmath::fabs(goal.y - p.y) < 1.5;
        if near && self.clear(map, p, goal) {
            return Some(goal);
        }
        let g = self.nearest(goal)?;
        // The spot the goal is nearest by walking: of the few nearest, the
        // first whose end has the goal in sight on its level (a walk is
        // one way off a ledge: a goal up one walks down to a spot it can't
        // be walked to from).
        let mut picked: [(u16, usize); ROUTE_PICKS] = [(FAR, usize::MAX); ROUTE_PICKS];
        for (s, f) in self.fields.iter().enumerate() {
            let c = f[g];
            if c < picked[ROUTE_PICKS - 1].0 {
                let mut k = ROUTE_PICKS - 1;
                while k > 0 && picked[k - 1].0 > c {
                    picked[k] = picked[k - 1];
                    k -= 1;
                }
                picked[k] = (c, s);
            }
        }
        let sees = |s: usize| {
            let e = self.nodes[self.ends[s] as usize];
            crate::fmath::fabs(e.y - goal.y) < LEVEL && self.clear(map, e, goal)
        };
        let s = picked
            .iter()
            .filter(|&&(c, _)| c != FAR)
            .map(|&(_, s)| s)
            .find(|&s| sees(s))
            .or(Some(picked[0].1).filter(|&s| s != usize::MAX))?;
        match self.toward(map, s, p, way) {
            Some(q) => Some(q),
            // At the spot: the rest of the way, straight.
            None => Some(goal),
        }
    }
}

/// How near a goal anywhere must be, and in clear sight, to be walked to
/// straight rather than by the waypoints; how many spots nearest it are
/// weighed to lead there.
const ROUTE_STRAIGHT: f32 = 25.0;
const ROUTE_PICKS: usize = 6;

/// The waypoint at or nearest a flood node (by its key), on its level.
fn nearest_way(fl: &Flood, way_of: &[u16], k: (i32, i32, i32), e: i32) -> Option<u16> {
    let snap = |v: i32| v.div_euclid(e) * e;
    let (bx, bz) = (snap(k.0), snap(k.1));
    let mut best: Option<(i32, u16)> = None;
    for x in bx - e..bx + 2 * e {
        for z in bz - e..bz + 2 * e {
            let Some(c) = fl.col((x, z, 0)) else {
                continue;
            };
            let mut i = fl.head[c];
            while i != NONE {
                let kk = fl.keys[i as usize];
                let wj = way_of[i as usize];
                if wj != u16::MAX && (kk.2 - k.2).abs() <= 64 {
                    let d = (kk.0 - k.0).abs() + (kk.1 - k.1).abs();
                    if best.is_none_or(|(bd, bw)| d < bd || (d == bd && wj < bw)) {
                        best = Some((d, wj));
                    }
                }
                i = fl.below[i as usize];
            }
        }
    }
    best.map(|(_, w)| w)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nearest_of_all(nodes: &[V3], p: V3) -> Option<usize> {
        let mut best = None;
        let mut bd = f32::INFINITY;
        for (i, q) in nodes.iter().enumerate() {
            let (dx, dy, dz) = (q.x - p.x, (q.y - p.y) * KY, q.z - p.z);
            let d = dx * dx + dy * dy + dz * dz;
            if d < bd {
                bd = d;
                best = Some(i);
            }
        }
        best
    }

    #[test]
    fn a_field_leads_every_waypoint_downhill_to_its_spot() {
        let m = crate::maps::bulkhead::DEF.build();
        let nav = Nav::build(&m);
        let g = &nav.foot;
        assert!(g.nodes.len() > 100, "{} waypoints", g.nodes.len());
        for (s, f) in g.fields.iter().enumerate() {
            let reach = f.iter().filter(|&&c| c != FAR).count();
            assert!(
                reach * 10 >= g.nodes.len() * 9,
                "spot {s}: {reach} of {} reach it",
                g.nodes.len()
            );
            for (i, &c) in f.iter().enumerate() {
                if c != FAR && c != 0 {
                    assert!(
                        g.out(i).iter().any(|&j| f[j as usize] < c),
                        "waypoint {i} is a dead end"
                    );
                }
            }
        }
    }

    #[test]
    fn the_nearest_waypoint_by_cell_is_the_nearest_of_all() {
        let m = crate::maps::bulkhead::DEF.build();
        let g = Nav::build(&m).foot;
        for k in 0..200 {
            let p = V3::new(
                (k % 20) as f32 * 2.3 - 23.0,
                (k % 3) as f32 * 1.7,
                (k / 20) as f32 * 3.1 - 15.0,
            );
            let all = nearest_of_all(&g.nodes, p);
            let by = g.nearest(p);
            let d = |i: usize| {
                let q = g.nodes[i];
                let (dx, dy, dz) = (q.x - p.x, (q.y - p.y) * KY, q.z - p.z);
                dx * dx + dy * dy + dz * dz
            };
            assert_eq!(by.map(d), all.map(d), "at {p:?}");
        }
    }

    #[test]
    fn trucks_have_their_own_way_on_a_map_with_them() {
        let m = crate::maps::canyon::DEF.build();
        let nav = Nav::build(&m);
        let wheels = nav.wheels.as_ref().expect("Dry Fork has trucks");
        assert!(wheels.nodes.len() > 100);
        // From one side's truck to the other side's flag.
        let [_, blue] = m.flags.unwrap();
        let spot = nav.flag(1);
        let mut way = u16::MAX;
        assert_ne!(wheels.cost(spot, m.vehicles[0].pos, &mut way), FAR);
        let end = wheels.end(spot).expect("a truck gets near blue's flag");
        assert!((end - blue).flat().len() < 40.0, "{end:?}");
    }
}

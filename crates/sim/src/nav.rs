//! Where a bot walks on a map too tight to cross in straight lines: a
//! waypoint graph over everywhere a body can stand (every level), and for
//! each roam point how far each waypoint is from it. A bot steers at the
//! farthest waypoint down the way it can see (`Bot::think`).
//!
//! Built once with the map (a map's `"nav": true`, so the maps whose bots
//! walk straight, the golden's among them, keep doing so); the tick only
//! reads it. The flood is the map test's (`tests/maps.rs`): 0.5 m steps
//! that climb at most a step, drops off ledges, launchers' throws.

use std::collections::{BTreeMap, VecDeque};

use crate::consts::{BODY_H, BODY_R, STEP, TICK_HZ};
use crate::fmath::round;
use crate::geom::V3;
use crate::input::Input;
use crate::map::Map;
use crate::movement::{step, Body};

/// The flood's step, and every how many steps a waypoint stands.
const G: f32 = 0.5;
const EVERY: i32 = 4;
/// Two waypoints are linked if one reaches the other in this many steps.
const LINK_STEPS: u16 = 10;
/// Not reachable.
pub const FAR: u16 = u16::MAX;
/// How near the last waypoint (or one it links to) must still be to count
/// as the nearest without looking at them all: a waypoint's spacing.
const NEAR_WAY: f32 = G * EVERY as f32;

type Key = (i32, i32, i32);

fn key(x: f32, z: f32, y: f32) -> Key {
    (round(x / G) as i32, round(z / G) as i32, round(y * 64.0) as i32)
}

fn at(k: Key) -> V3 {
    V3::new(k.0 as f32 * G, k.2 as f32 / 64.0, k.1 as f32 * G)
}

#[derive(Clone, Debug, Default)]
pub struct Nav {
    /// The waypoints, on the floor.
    pub nodes: Vec<V3>,
    /// Waypoint `i`'s links (to where it can walk) are
    /// `links[start[i]..start[i + 1]]`.
    start: Vec<u32>,
    links: Vec<u16>,
    /// For each roam point, each waypoint's cost to walk to it ([`FAR`]:
    /// it can't).
    fields: Vec<Vec<u16>>,
}

impl Nav {
    pub fn build(m: &Map) -> Nav {
        // The flood, 0.5 m at a time.
        let mut index: BTreeMap<Key, u32> = BTreeMap::new();
        let mut fine: Vec<Key> = Vec::new();
        let mut next: Vec<Vec<u32>> = Vec::new();
        let mut q = VecDeque::new();
        fn add(k: Key, index: &mut BTreeMap<Key, u32>, fine: &mut Vec<Key>, next: &mut Vec<Vec<u32>>) -> (u32, bool) {
            if let Some(&i) = index.get(&k) {
                return (i, false);
            }
            let i = fine.len() as u32;
            index.insert(k, i);
            fine.push(k);
            next.push(Vec::new());
            (i, true)
        }
        let throws: Vec<(crate::geom::Aabb, V3)> = m
            .launchers
            .iter()
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
            .collect();
        for s in &m.spawns {
            let (i, new) = add(key(s.pos.x, s.pos.z, s.pos.y), &mut index, &mut fine, &mut next);
            if new {
                q.push_back(i);
            }
        }
        let r = BODY_R;
        while let Some(i) = q.pop_front() {
            let k = fine[i as usize];
            let p = at(k);
            let mut to: [Option<Key>; 4 + crate::consts::MAX_LAUNCHERS] = [None; 4 + crate::consts::MAX_LAUNCHERS];
            for (n, (dx, dz)) in [(1, 0), (-1, 0), (0, 1), (0, -1)].into_iter().enumerate() {
                let (x, z) = ((k.0 + dx) as f32 * G, (k.1 + dz) as f32 * G);
                if x - r < m.lo.0 || x + r > m.hi.0 || z - r < m.lo.1 || z + r > m.hi.1 {
                    continue;
                }
                let ny = m.floor_with(x, z, r, p.y + STEP, p.y + STEP);
                // Over the edge of a void is no way anywhere.
                if m.lost_floor(x, z, ny) {
                    continue;
                }
                if !m.occupied(x, z, r, ny, ny + BODY_H) {
                    to[n] = Some(key(x, z, ny));
                }
            }
            for (n, (a, land)) in throws.iter().enumerate().take(crate::consts::MAX_LAUNCHERS) {
                if p.x >= a.min.x
                    && p.x <= a.max.x
                    && p.z >= a.min.z
                    && p.z <= a.max.z
                    && p.y >= a.min.y
                    && p.y <= a.max.y
                {
                    to[4 + n] = Some(key(land.x, land.z, land.y));
                }
            }
            for nk in to.into_iter().flatten() {
                let (j, new) = add(nk, &mut index, &mut fine, &mut next);
                next[i as usize].push(j);
                if new {
                    q.push_back(j);
                }
            }
        }

        // Waypoints: every EVERY-th step each way, and what each reaches
        // within LINK_STEPS (a short walk: a bot heading at it gets there).
        let mut way: Vec<u32> = Vec::new();
        let mut way_of: BTreeMap<u32, u16> = BTreeMap::new();
        for (i, k) in fine.iter().enumerate() {
            if k.0.rem_euclid(EVERY) == 0 && k.1.rem_euclid(EVERY) == 0 && way.len() < u16::MAX as usize {
                way_of.insert(i as u32, way.len() as u16);
                way.push(i as u32);
            }
        }
        let mut start = Vec::with_capacity(way.len() + 1);
        let mut links: Vec<u16> = Vec::new();
        let mut cost: Vec<Vec<(u16, u16)>> = vec![Vec::new(); way.len()];
        let mut depth: BTreeMap<u32, u16> = BTreeMap::new();
        for (wi, &f) in way.iter().enumerate() {
            start.push(links.len() as u32);
            depth.clear();
            depth.insert(f, 0);
            let mut q = VecDeque::from([f]);
            let mut found: Vec<(u16, u16)> = Vec::new();
            while let Some(i) = q.pop_front() {
                let d = depth[&i];
                if d >= LINK_STEPS {
                    continue;
                }
                for &j in &next[i as usize] {
                    if depth.contains_key(&j) {
                        continue;
                    }
                    // A walk moves one cell; anything farther is a throw.
                    let far = {
                        let (a, b) = (fine[i as usize], fine[j as usize]);
                        (a.0 - b.0).abs() + (a.1 - b.1).abs() > 1
                    };
                    // A launcher's throw: a link of its own, long as the
                    // walk it saves is not.
                    if far {
                        if let Some(&wj) = nearest_way(&way_of, &index, fine[j as usize]) {
                            if !found.iter().any(|&(w, _)| w == wj) {
                                found.push((wj, d + 1));
                            }
                        }
                        continue;
                    }
                    depth.insert(j, d + 1);
                    if let Some(&wj) = way_of.get(&j) {
                        if wj as usize != wi && !found.iter().any(|&(w, _)| w == wj) {
                            found.push((wj, d + 1));
                        }
                    }
                    q.push_back(j);
                }
            }
            for &(wj, d) in &found {
                links.push(wj);
                cost[wi].push((wj, d));
            }
        }
        start.push(links.len() as u32);
        let nodes: Vec<V3> = way.iter().map(|&f| at(fine[f as usize])).collect();

        // Each roam point's field: costs to it, along the links reversed.
        let mut back: Vec<Vec<(u16, u16)>> = vec![Vec::new(); way.len()];
        for (a, outs) in cost.iter().enumerate() {
            for &(b, c) in outs {
                back[b as usize].push((a as u16, c));
            }
        }
        let fields = m
            .roam
            .iter()
            .map(|r| {
                let mut f = vec![FAR; nodes.len()];
                let Some(t) = nearest(&nodes, *r, 3.0) else {
                    return f;
                };
                // Small integer costs: a bucketed walk, in order.
                f[t] = 0;
                let mut q = VecDeque::from([t]);
                while let Some(i) = q.pop_front() {
                    for &(a, c) in &back[i] {
                        let nd = f[i].saturating_add(c);
                        if nd < f[a as usize] {
                            f[a as usize] = nd;
                            q.push_back(a as usize);
                        }
                    }
                }
                f
            })
            .collect();
        Nav {
            nodes,
            start,
            links,
            fields,
        }
    }

    fn out(&self, i: usize) -> &[u16] {
        &self.links[self.start[i] as usize..self.start[i + 1] as usize]
    }

    /// The waypoint a body at `p` is at: the nearest, a level's height
    /// counting for more than distance across.
    pub fn nearest(&self, p: V3) -> Option<usize> {
        nearest(&self.nodes, p, 3.0)
    }

    /// [`Self::nearest`] for a body that was nearest waypoint `hint` a
    /// moment ago: that one or one it links to, if one is within a
    /// waypoint's spacing; else every waypoint. `u16::MAX`: no hint.
    fn nearest_from(&self, p: V3, hint: u16) -> Option<usize> {
        let h = hint as usize;
        if h < self.nodes.len() {
            let d2 = |i: usize| {
                let q = self.nodes[i];
                let (dx, dy, dz) = (q.x - p.x, (q.y - p.y) * 3.0, q.z - p.z);
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
            if bd < NEAR_WAY * NEAR_WAY {
                return Some(best);
            }
        }
        self.nearest(p)
    }

    /// Where a body at `p` heading for roam point `goal` should steer: down
    /// the field a few waypoints, the farthest it can see from its waist.
    /// None: no field, or nowhere to go from here (head straight). `way`:
    /// the waypoint it was nearest last time, updated (`u16::MAX`: none).
    pub fn toward(&self, map: &Map, goal: usize, p: V3, way: &mut u16) -> Option<V3> {
        let f = self.fields.get(goal)?;
        let mut cur = self.nearest_from(p, *way)?;
        *way = cur as u16;
        if f[cur] == FAR {
            return None;
        }
        let waist = V3::new(0.0, 0.9, 0.0);
        let mut aim = None;
        for _ in 0..4 {
            let mut best = cur;
            for &j in self.out(cur) {
                if f[j as usize] < f[best] {
                    best = j as usize;
                }
            }
            if best == cur {
                break;
            }
            cur = best;
            let q = self.nodes[cur];
            if aim.is_none() || map.clear(p + waist, q + waist) {
                aim = Some(q);
            } else {
                break;
            }
        }
        aim
    }
}

fn nearest(nodes: &[V3], p: V3, ky: f32) -> Option<usize> {
    let mut best = None;
    let mut bd = f32::INFINITY;
    for (i, q) in nodes.iter().enumerate() {
        let (dx, dy, dz) = (q.x - p.x, (q.y - p.y) * ky, q.z - p.z);
        let d = dx * dx + dy * dy + dz * dz;
        if d < bd {
            bd = d;
            best = Some(i);
        }
    }
    best
}

/// The waypoint at or nearest a flood node (by its grid key), on its level.
fn nearest_way<'a>(way_of: &'a BTreeMap<u32, u16>, index: &BTreeMap<Key, u32>, k: Key) -> Option<&'a u16> {
    let snap = |v: i32| round(v as f32 / EVERY as f32) as i32 * EVERY;
    for (dx, dz) in [(0, 0), (EVERY, 0), (-EVERY, 0), (0, EVERY), (0, -EVERY)] {
        let c = (snap(k.0) + dx, snap(k.1) + dz, k.2);
        if let Some(w) = index.get(&c).and_then(|i| way_of.get(i)) {
            return Some(w);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_field_leads_every_waypoint_downhill_to_its_roam_point() {
        let m = crate::maps::bulkhead::DEF.build();
        let nav = Nav::build(&m);
        assert!(nav.nodes.len() > 100, "{} waypoints", nav.nodes.len());
        for (g, f) in nav.fields.iter().enumerate() {
            let reach = f.iter().filter(|&&c| c != FAR).count();
            assert!(
                reach * 10 >= nav.nodes.len() * 9,
                "roam point {g}: {reach} of {} reach it",
                nav.nodes.len()
            );
            for (i, &c) in f.iter().enumerate() {
                if c != FAR && c != 0 {
                    assert!(
                        nav.out(i).iter().any(|&j| f[j as usize] < c),
                        "waypoint {i} is a dead end"
                    );
                }
            }
        }
    }
}

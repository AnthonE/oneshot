//! `oneshot-flow <map id or name> [--out DIR]`: a map's flow on paper,
//! from the sim's own geometry (the floors, solids and rays a match uses).
//!
//! - **The plan** (`DIR/<map>.svg`): top down, everything coloured by the
//!   height of its top, the places named, the spawns by side, the pads, the
//!   launchers' throws, and the longest sight lines drawn across it.
//! - **The numbers** (`DIR/<map>.md`): walk times at a run between every
//!   two places (launchers taken, no jumps), each side's time from its
//!   spawns to each place, the longest lines of sight, how many of the other
//!   side's spawns each side's spawns can see, and what share of the map
//!   each place sees.
//!
//! The places are the map's pads, each side's spawns, and the `callouts`
//! its `map.json` names (`[{ "name": "top mid", "at": [x, z, y] }]`; the
//! map compiler ignores them).

use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use sim::consts::{BODY_H, BODY_R, EYE, RUN, STEP, TICK_HZ};
use sim::geom::V3;
use sim::input::Input;
use sim::map::{Map, ANY_TEAM};
use sim::movement::{step, Body};
use sim::solid::Shape;

/// The walk grid's step, m.
const G: f32 = 0.5;
/// Sight lines are sampled on a grid this coarse, m.
const SIGHT_G: f32 = 1.0;
/// A sight line this long or longer is a lane a long gun owns.
const LONG_M: f32 = 30.0;

type Key = (i32, i32, i32);

fn key(x: f32, z: f32, y: f32) -> Key {
    (
        (x / G).round() as i32,
        (z / G).round() as i32,
        (y * 64.0).round() as i32,
    )
}

fn at(k: Key) -> V3 {
    V3::new(k.0 as f32 * G, k.2 as f32 / 64.0, k.1 as f32 * G)
}

struct Place {
    name: String,
    pos: V3,
    /// 0 red, 1 blue, 2 neither.
    side: u8,
}

/// Where a body can stand, and how long each step between two takes.
struct Walk {
    nodes: Vec<Key>,
    index: BTreeMap<Key, usize>,
    /// Neighbours and the time to each, ms.
    edges: Vec<Vec<(usize, u32)>>,
    /// Launchers' throws: from the pad's middle to where it lands, and how long.
    throws: Vec<(V3, V3, f32)>,
}

impl Walk {
    fn build(m: &Map) -> Walk {
        let mut index = BTreeMap::new();
        let mut nodes = Vec::new();
        let mut edges: Vec<Vec<(usize, u32)>> = Vec::new();
        let mut q = std::collections::VecDeque::new();
        let add =
            |k: Key, index: &mut BTreeMap<Key, usize>, nodes: &mut Vec<Key>, edges: &mut Vec<Vec<(usize, u32)>>| {
                *index.entry(k).or_insert_with(|| {
                    nodes.push(k);
                    edges.push(Vec::new());
                    nodes.len() - 1
                })
            };
        // The launchers' throws, flown by the sim's own movement.
        let mut throws = Vec::new();
        for l in &m.launchers {
            let mid = (l.area.min + l.area.max) * 0.5;
            let mut b = Body {
                pos: V3::new(mid.x, m.floor(mid.x, mid.z, BODY_R, l.area.max.y), mid.z),
                grounded: true,
                ..Body::default()
            };
            let mut ticks = 0;
            step(&mut b, &Input::default(), m);
            while !b.grounded && ticks < TICK_HZ * 6 {
                step(&mut b, &Input::default(), m);
                ticks += 1;
            }
            throws.push((
                V3::new(mid.x, b.pos.y.min(mid.y), mid.z),
                b.pos,
                ticks as f32 / TICK_HZ as f32,
            ));
        }
        for s in &m.spawns {
            let k = key(s.pos.x, s.pos.z, s.pos.y);
            if !index.contains_key(&k) {
                add(k, &mut index, &mut nodes, &mut edges);
                q.push_back(k);
            }
        }
        let r = BODY_R;
        let ms = |d: f32| (d / RUN * 1000.0).round() as u32;
        while let Some(k) = q.pop_front() {
            let i = index[&k];
            let p = at(k);
            let mut next: Vec<(Key, u32)> = Vec::new();
            for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] {
                let (x, z) = ((k.0 + dx) as f32 * G, (k.1 + dz) as f32 * G);
                if x - r < m.lo.0 || x + r > m.hi.0 || z - r < m.lo.1 || z + r > m.hi.1 {
                    continue;
                }
                let ny = m.floor_with(x, z, r, p.y + STEP, p.y + STEP);
                // Over the edge of a void: a fall, not a walk.
                if m.lost_floor(x, z, ny) || m.occupied(x, z, r, ny, ny + BODY_H) {
                    continue;
                }
                // A diagonal needs both of its sides clear (no corner-cutting).
                if dx != 0 && dz != 0 {
                    let a = m.floor_with(p.x + dx as f32 * G, p.z, r, p.y + STEP, p.y + STEP);
                    let b = m.floor_with(p.x, p.z + dz as f32 * G, r, p.y + STEP, p.y + STEP);
                    if m.occupied(p.x + dx as f32 * G, p.z, r, a, a + BODY_H)
                        || m.occupied(p.x, p.z + dz as f32 * G, r, b, b + BODY_H)
                    {
                        continue;
                    }
                }
                let d = if dx != 0 && dz != 0 {
                    G * std::f32::consts::SQRT_2
                } else {
                    G
                };
                next.push((key(x, z, ny), ms(d)));
            }
            for (l, t) in m.launchers.iter().zip(&throws) {
                let a = &l.area;
                if p.x >= a.min.x
                    && p.x <= a.max.x
                    && p.z >= a.min.z
                    && p.z <= a.max.z
                    && p.y >= a.min.y
                    && p.y <= a.max.y
                {
                    let land = t.1;
                    next.push((key(land.x, land.z, land.y), (t.2 * 1000.0).round() as u32));
                }
            }
            for (nk, cost) in next {
                let fresh = !index.contains_key(&nk);
                let j = add(nk, &mut index, &mut nodes, &mut edges);
                edges[i].push((j, cost));
                if fresh {
                    q.push_back(nk);
                }
            }
        }
        Walk {
            nodes,
            index,
            edges,
            throws,
        }
    }

    /// The node nearest a point (in x and z, then height).
    fn nearest(&self, p: V3) -> Option<usize> {
        let mut best = None;
        let mut bd = f32::INFINITY;
        for (i, &k) in self.nodes.iter().enumerate() {
            let q = at(k);
            let d = (q.x - p.x).powi(2) + (q.z - p.z).powi(2) + 4.0 * (q.y - p.y).powi(2);
            if d < bd {
                bd = d;
                best = Some(i);
            }
        }
        best
    }

    /// Times from `from` to every node, ms (u32::MAX: can't get there).
    fn times(&self, from: &[usize]) -> Vec<u32> {
        let mut t = vec![u32::MAX; self.nodes.len()];
        let mut h = BinaryHeap::new();
        for &f in from {
            t[f] = 0;
            h.push(Reverse((0u32, f)));
        }
        while let Some(Reverse((d, i))) = h.pop() {
            if d > t[i] {
                continue;
            }
            for &(j, c) in &self.edges[i] {
                let nd = d + c;
                if nd < t[j] {
                    t[j] = nd;
                    h.push(Reverse((nd, j)));
                }
            }
        }
        t
    }
}

fn map_json(id: u8) -> Option<serde_json::Value> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../maps");
    for e in std::fs::read_dir(root).ok()?.flatten() {
        let p = e.path().join("map.json");
        if let Ok(s) = std::fs::read_to_string(&p) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) {
                if v["id"].as_u64() == Some(id as u64) {
                    return Some(v);
                }
            }
        }
    }
    None
}

fn places(m: &Map, json: Option<&serde_json::Value>) -> Vec<Place> {
    let mut out = Vec::new();
    for (team, side) in [(0u8, "red"), (1, "blue")] {
        let s: Vec<V3> = m.spawns.iter().filter(|s| s.team == team).map(|s| s.pos).collect();
        if !s.is_empty() {
            let c = s.iter().fold(V3::ZERO, |a, &b| a + b) * (1.0 / s.len() as f32);
            // The spawn nearest the middle of the side's spawns.
            let near = s
                .iter()
                .copied()
                .min_by(|a, b| (*a - c).len().total_cmp(&(*b - c).len()))
                .unwrap();
            out.push(Place {
                name: format!("{side} spawns"),
                pos: near,
                side: team,
            });
        }
    }
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    for p in &m.pads {
        let n = sim::arms::arm(p.arm).name.to_lowercase();
        *seen.entry(n).or_default() += 1;
    }
    for p in &m.pads {
        let n = sim::arms::arm(p.arm).name.to_lowercase();
        let name = if seen[&n] > 1 {
            let side = if p.pos.x < -0.5 {
                "red"
            } else if p.pos.x > 0.5 {
                "blue"
            } else if p.pos.z < 0.0 {
                "south"
            } else {
                "north"
            };
            format!("{n} ({side})")
        } else {
            n
        };
        out.push(Place {
            name,
            pos: p.pos,
            side: 2,
        });
    }
    if let Some(cs) = json.and_then(|j| j["callouts"].as_array()) {
        for c in cs {
            let (Some(name), Some(a)) = (c["name"].as_str(), c["at"].as_array()) else {
                continue;
            };
            let v: Vec<f32> = a.iter().filter_map(|x| x.as_f64()).map(|x| x as f32).collect();
            if v.len() >= 2 {
                let y = v.get(2).copied().unwrap_or(0.0);
                out.push(Place {
                    name: name.into(),
                    pos: V3::new(v[0], y, v[1]),
                    side: 2,
                });
            }
        }
    }
    out
}

fn secs(ms: u32) -> String {
    if ms == u32::MAX {
        "—".into()
    } else {
        format!("{:.1}", ms as f32 / 1000.0)
    }
}

/// A height as a colour: deep blue-grey low, through teal and sand, to
/// pale at the top.
fn height_colour(y: f32, top: f32) -> String {
    let t = (y / top.max(0.5)).clamp(0.0, 1.0);
    let stops = [
        (0.0, [52, 62, 82]),
        (0.35, [52, 120, 128]),
        (0.7, [196, 170, 108]),
        (1.0, [236, 232, 220]),
    ];
    let mut c = stops[0].1;
    for w in stops.windows(2) {
        if t >= w[0].0 && t <= w[1].0 {
            let u = (t - w[0].0) / (w[1].0 - w[0].0);
            for (i, ci) in c.iter_mut().enumerate() {
                *ci = (w[0].1[i] as f32 + (w[1].1[i] as f32 - w[0].1[i] as f32) * u) as i32;
            }
        }
    }
    format!("rgb({},{},{})", c[0], c[1], c[2])
}

struct Sight {
    a: V3,
    b: V3,
    len: f32,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let Some(want) = args.get(1) else {
        eprintln!("oneshot-flow <map id or name> [--out DIR]");
        std::process::exit(2);
    };
    let out = args
        .iter()
        .position(|a| a == "--out")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("traces/flow"));
    let def = sim::maps::ALL
        .iter()
        .find(|d| {
            want.parse::<u8>().ok() == Some(d.id)
                || d.name.eq_ignore_ascii_case(want)
                || d.name.to_lowercase().replace(' ', "_") == want.to_lowercase()
        })
        .unwrap_or_else(|| {
            eprintln!(
                "no map {want}: {}",
                sim::maps::ALL
                    .iter()
                    .map(|d| format!("{} {}", d.id, d.name))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            std::process::exit(2);
        });
    let m = def.build();
    let json = map_json(def.id);
    let walk = Walk::build(&m);
    let places = places(&m, json.as_ref());
    let slug = def.name.to_lowercase().replace(' ', "_");
    std::fs::create_dir_all(&out).expect("the out directory");

    // Walk times.
    let node_of: Vec<Option<usize>> = places.iter().map(|p| walk.nearest(p.pos)).collect();
    let from: Vec<Vec<u32>> = node_of
        .iter()
        .map(|n| n.map_or_else(|| vec![u32::MAX; walk.nodes.len()], |n| walk.times(&[n])))
        .collect();
    let spawns_of = |team: u8| -> Vec<usize> {
        m.spawns
            .iter()
            .filter(|s| s.team == team || s.team == ANY_TEAM)
            .filter_map(|s| walk.index.get(&key(s.pos.x, s.pos.z, s.pos.y)).copied())
            .collect()
    };
    let side_times = [walk.times(&spawns_of(0)), walk.times(&spawns_of(1))];

    // Sight: the walkable grid thinned to SIGHT_G, eyes up.
    let mut eyes: Vec<V3> = Vec::new();
    let mut taken = std::collections::BTreeSet::new();
    let thin = (SIGHT_G / G).round() as i32;
    for &k in &walk.nodes {
        if k.0 % thin != 0 || k.1 % thin != 0 {
            continue;
        }
        if taken.insert(k) {
            let p = at(k);
            eyes.push(V3::new(p.x, p.y + EYE, p.z));
        }
    }
    let mut lines: Vec<Sight> = Vec::new();
    for i in 0..eyes.len() {
        for j in i + 1..eyes.len() {
            let (a, b) = (eyes[i], eyes[j]);
            let len = (b - a).len();
            if len >= 12.0 && m.clear(a, b) {
                lines.push(Sight { a, b, len });
            }
        }
    }
    lines.sort_by(|x, y| y.len.total_cmp(&x.len));
    // The longest, but not five copies of one lane: skip a line whose ends
    // are both near a longer one's.
    let mut shown: Vec<&Sight> = Vec::new();
    for l in &lines {
        let near = |p: V3, q: V3| (p - q).len() < 4.0;
        if shown
            .iter()
            .any(|s| (near(s.a, l.a) && near(s.b, l.b)) || (near(s.a, l.b) && near(s.b, l.a)))
        {
            continue;
        }
        shown.push(l);
        if shown.len() == 6 {
            break;
        }
    }
    let name_near = |p: V3| -> String {
        let flat = |a: V3, b: V3| ((a.x - b.x).powi(2) + (a.z - b.z).powi(2)).sqrt() + (a.y - b.y).abs();
        places
            .iter()
            .min_by(|a, b| flat(a.pos, p).total_cmp(&flat(b.pos, p)))
            .map(|pl| {
                let d = flat(pl.pos, p);
                if d < 3.0 {
                    pl.name.clone()
                } else {
                    format!("{:.0} m from {}", d, pl.name)
                }
            })
            .unwrap_or_default()
    };
    // Each place: the share of the sampled map its eye sees.
    let sees: Vec<f32> = places
        .iter()
        .map(|p| {
            let e = V3::new(p.pos.x, p.pos.y + EYE, p.pos.z);
            let n = eyes.iter().filter(|&&q| m.clear(e, q)).count();
            n as f32 / eyes.len().max(1) as f32
        })
        .collect();
    // Spawns that see the other side's spawns.
    let spawn_eye = |s: &sim::map::Spawn| V3::new(s.pos.x, s.pos.y + EYE, s.pos.z);
    let mut cross = 0;
    let mut pairs = 0;
    for a in m.spawns.iter().filter(|s| s.team == 0) {
        for b in m.spawns.iter().filter(|s| s.team == 1) {
            pairs += 1;
            if m.clear(spawn_eye(a), spawn_eye(b)) {
                cross += 1;
            }
        }
    }
    let area = walk.nodes.len() as f32 * G * G;

    // The report.
    let mut md = String::new();
    let _ = writeln!(md, "# {}: flow\n", def.name);
    let _ = writeln!(
        md,
        "{} m², {} m² of it to stand on (every level), {} launchers. From `oneshot-flow`: walk times at a run ({RUN} m/s), launchers taken, no jumps.\n",
        ((m.hi.0 - m.lo.0) * (m.hi.1 - m.lo.1)).round(),
        area.round(),
        m.launchers.len()
    );
    let _ = writeln!(md, "## From each side's spawns (s)\n");
    let _ = writeln!(md, "| place | red | blue | sees |");
    let _ = writeln!(md, "|---|---|---|---|");
    for (i, p) in places.iter().enumerate() {
        let t = |s: usize| node_of[i].map_or(u32::MAX, |n| side_times[s][n]);
        let _ = writeln!(
            md,
            "| {} | {} | {} | {:.0}% |",
            p.name,
            secs(t(0)),
            secs(t(1)),
            sees[i] * 100.0
        );
    }
    let _ = writeln!(md, "\n## Between places (s)\n");
    let _ = write!(md, "| |");
    for p in &places {
        let _ = write!(md, " {} |", p.name);
    }
    let _ = writeln!(md);
    let _ = writeln!(md, "|---|{}", "---|".repeat(places.len()));
    for (i, p) in places.iter().enumerate() {
        let _ = write!(md, "| {} |", p.name);
        for n in &node_of {
            let t = n.map_or(u32::MAX, |n| from[i][n]);
            let _ = write!(md, " {} |", secs(t));
        }
        let _ = writeln!(md);
    }
    let _ = writeln!(md, "\n## The longest sight lines\n");
    for l in &shown {
        let flag = if l.len >= LONG_M {
            " **(a long gun's lane)**"
        } else {
            ""
        };
        let _ = writeln!(md, "- {:.0} m: {} to {}{flag}", l.len, name_near(l.a), name_near(l.b));
    }
    let _ = writeln!(md, "\nRed's spawns see blue's in {cross} of {pairs} pairs.");
    // A place either side can't walk to, naming the side when the other can.
    let unreached: Vec<String> = places
        .iter()
        .zip(&node_of)
        .filter_map(|(p, n)| {
            let cant = |s: usize| n.is_none_or(|n| side_times[s][n] == u32::MAX);
            match (cant(0), cant(1)) {
                (true, true) => Some(p.name.clone()),
                (true, false) => Some(format!("{} (red)", p.name)),
                (false, true) => Some(format!("{} (blue)", p.name)),
                (false, false) => None,
            }
        })
        .collect();
    if !unreached.is_empty() {
        let _ = writeln!(md, "\nCan't be walked to: {}.", unreached.join(", "));
    }
    std::fs::write(out.join(format!("{slug}.md")), &md).expect("the report");

    // The plan.
    let pad = 2.0;
    let (x0, z0) = (m.lo.0 - pad, m.lo.1 - pad);
    let (w, h) = (m.hi.0 - m.lo.0 + 2.0 * pad, m.hi.1 - m.lo.1 + 2.0 * pad);
    let s = (1400.0 / w).min(900.0 / h);
    // North (+z) up.
    let z1 = z0 + h;
    let px = |x: f32| (x - x0) * s;
    let pz = |z: f32| (z1 - z) * s;
    let top = m
        .boxes
        .iter()
        .filter(|b| {
            b.min.x >= m.lo.0 - 0.01 && b.max.x <= m.hi.0 + 0.01 && b.min.z >= m.lo.1 - 0.01 && b.max.z <= m.hi.1 + 0.01
        })
        .map(|b| b.max.y)
        .chain(m.solids.iter().map(|s| s.y1))
        .fold(1.0f32, f32::max)
        .min(walk.nodes.iter().map(|&k| at(k).y).fold(0.0f32, f32::max) + 4.0);
    let mut svg = String::new();
    let _ = writeln!(
        svg,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{:.0}" height="{:.0}" font-family="DejaVu Sans, sans-serif">"#,
        w * s,
        h * s + 70.0
    );
    let _ = writeln!(svg, r##"<rect width="100%" height="100%" fill="#16181d"/>"##);
    let _ = writeln!(
        svg,
        r##"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" fill="{}"/>"##,
        px(m.lo.0),
        pz(m.hi.1),
        (m.hi.0 - m.lo.0) * s,
        (m.hi.1 - m.lo.1) * s,
        height_colour(0.0, top)
    );
    // The void, where there is one: nothing to stand on, the page's dark
    // (a run of half-metre cells a row).
    if m.void > f32::NEG_INFINITY {
        const V: f32 = 0.5;
        let (nx, nz) = (((m.hi.0 - m.lo.0) / V) as i32, ((m.hi.1 - m.lo.1) / V) as i32);
        for j in 0..nz {
            let z = m.lo.1 + (j as f32 + 0.5) * V;
            let mut run: Option<i32> = None;
            for i in 0..=nx {
                let x = m.lo.0 + (i as f32 + 0.5) * V;
                let lost = i < nx && m.over_void(x, z, m.ground(x, z));
                match (lost, run) {
                    (true, None) => run = Some(i),
                    (false, Some(i0)) => {
                        let _ = writeln!(
                            svg,
                            r##"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" fill="#16181d"/>"##,
                            px(m.lo.0 + i0 as f32 * V),
                            pz(z + V * 0.5),
                            (i - i0) as f32 * V * s,
                            V * s + 0.5
                        );
                        run = None;
                    }
                    _ => {}
                }
            }
        }
    }
    // Lower tops first, so higher ones cover them.
    enum Item<'a> {
        Box(&'a sim::geom::Aabb),
        Solid(&'a sim::solid::Solid),
    }
    // A roof (anything wholly over every place to stand, and a head) isn't
    // drawn: it would cover the plan.
    let over = walk.nodes.iter().map(|&k| at(k).y).fold(0.0f32, f32::max) + BODY_H + 0.5;
    let mut items: Vec<(f32, Item)> = m
        .boxes
        .iter()
        .filter(|b| b.min.y < over)
        .map(|b| (b.max.y, Item::Box(b)))
        .collect();
    items.extend(m.solids.iter().filter(|s| s.y0 < over).map(|s| (s.y1, Item::Solid(s))));
    items.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (y, it) in &items {
        let fill = height_colour(*y, top);
        match it {
            Item::Box(b) => {
                let _ = writeln!(
                    svg,
                    r##"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" fill="{fill}" stroke="#0b0c0f" stroke-width="0.8"/>"##,
                    px(b.min.x),
                    pz(b.max.z),
                    (b.max.x - b.min.x) * s,
                    (b.max.z - b.min.z) * s
                );
            }
            Item::Solid(sol) => match sol.shape {
                Shape::Round { x, z, r } => {
                    let _ = writeln!(
                        svg,
                        r##"<circle cx="{:.1}" cy="{:.1}" r="{:.1}" fill="{fill}" stroke="#0b0c0f" stroke-width="0.8"/>"##,
                        px(x),
                        pz(z),
                        r * s
                    );
                }
                Shape::Turned { x, z, hx, hz, c, s: sn } => {
                    let pts: Vec<String> = [(-hx, -hz), (hx, -hz), (hx, hz), (-hx, hz)]
                        .iter()
                        .map(|&(a, b)| format!("{:.1},{:.1}", px(x + a * c - b * sn), pz(z + a * sn + b * c)))
                        .collect();
                    let _ = writeln!(
                        svg,
                        r##"<polygon points="{}" fill="{fill}" stroke="#0b0c0f" stroke-width="0.5"/>"##,
                        pts.join(" ")
                    );
                }
                Shape::Ramp {
                    x0: a,
                    z0: b,
                    x1: c,
                    z1: d,
                    dir,
                    low,
                    high,
                } => {
                    let (lo_c, hi_c) = (height_colour(low, top), height_colour(high, top));
                    // Up the slope, on a page whose y runs south.
                    let (gx1, gy1, gx2, gy2) = match dir {
                        0 => (0, 0, 1, 0),
                        1 => (1, 0, 0, 0),
                        2 => (0, 1, 0, 0),
                        _ => (0, 0, 0, 1),
                    };
                    let id = format!("r{:.0}{:.0}{:.0}{:.0}", a * 10.0, b * 10.0, c * 10.0, d * 10.0).replace('-', "m");
                    let _ = writeln!(
                        svg,
                        r##"<defs><linearGradient id="{id}" x1="{gx1}" y1="{gy1}" x2="{gx2}" y2="{gy2}"><stop offset="0" stop-color="{lo_c}"/><stop offset="1" stop-color="{hi_c}"/></linearGradient></defs><rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" fill="url(#{id})" stroke="#0b0c0f" stroke-width="0.8"/>"##,
                        px(a),
                        pz(d),
                        (c - a) * s,
                        (d - b) * s
                    );
                    // Chevrons pointing up the slope.
                    let (cx, cz) = ((a + c) * 0.5, (b + d) * 0.5);
                    let (ux, uz) = [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)][dir as usize];
                    let len = if dir < 2 { (c - a) * 0.3 } else { (d - b) * 0.3 };
                    let wid = if dir < 2 { (d - b) * 0.3 } else { (c - a) * 0.3 };
                    let (tx, tz) = (cx + ux * len, cz + uz * len);
                    let (bx, bz) = (cx - ux * len, cz - uz * len);
                    let _ = writeln!(
                        svg,
                        r##"<polyline points="{:.1},{:.1} {:.1},{:.1} {:.1},{:.1}" fill="none" stroke="#0b0c0f" stroke-width="2" opacity="0.6"/>"##,
                        px(bx - uz * wid),
                        pz(bz + ux * wid),
                        px(tx),
                        pz(tz),
                        px(bx + uz * wid),
                        pz(bz - ux * wid)
                    );
                }
                // Its outline from above: a ring, a ring's sector, or a
                // carried solid's rectangle (the hole a second ring, cut).
                Shape::Revolve { x, z, sector, k, .. } => {
                    let (s0, sn) = (k[0][0], k[k.len() - 1][0]);
                    let (a0, a1) = match sector {
                        None => (0.0, std::f32::consts::TAU),
                        Some(sec) => (sec.e0[1].atan2(sec.e0[0]), {
                            let b = sec.e1[1].atan2(sec.e1[0]);
                            let a = sec.e0[1].atan2(sec.e0[0]);
                            if b <= a {
                                b + std::f32::consts::TAU
                            } else {
                                b
                            }
                        }),
                    };
                    let n = 48;
                    let ring = |r: f32, rev: bool| -> Vec<String> {
                        (0..=n)
                            .map(|i| {
                                let i = if rev { n - i } else { i };
                                let a = a0 + (a1 - a0) * i as f32 / n as f32;
                                format!("{:.1},{:.1}", px(x + r * a.cos()), pz(z + r * a.sin()))
                            })
                            .collect()
                    };
                    let path = if sector.is_none() {
                        let mut d = format!("M{}Z", ring(sn, false).join(" L"));
                        if s0 > 0.0 {
                            d += &format!(" M{}Z", ring(s0, true).join(" L"));
                        }
                        d
                    } else {
                        let mut pts = ring(sn, false);
                        pts.extend(ring(s0, true));
                        format!("M{}Z", pts.join(" L"))
                    };
                    let _ = writeln!(
                        svg,
                        r##"<path d="{path}" fill="{fill}" fill-rule="evenodd" stroke="#0b0c0f" stroke-width="0.6"/>"##
                    );
                }
                Shape::Sweep {
                    x, z, c, s: sn, l, k, ..
                } => {
                    let (s0, s1) = (k[0][0], k[k.len() - 1][0]);
                    let pts: Vec<String> = [(-l, s0), (l, s0), (l, s1), (-l, s1)]
                        .iter()
                        .map(|&(u, v)| format!("{:.1},{:.1}", px(x + u * c - v * sn), pz(z + u * sn + v * c)))
                        .collect();
                    let _ = writeln!(
                        svg,
                        r##"<polygon points="{}" fill="{fill}" stroke="#0b0c0f" stroke-width="0.5"/>"##,
                        pts.join(" ")
                    );
                }
            },
        }
    }
    // Sight lines.
    for (i, l) in shown.iter().enumerate() {
        let col = if l.len >= LONG_M { "#ff5a4a" } else { "#ffd24a" };
        let _ = writeln!(
            svg,
            r##"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="{col}" stroke-width="{}" stroke-dasharray="6 4" opacity="0.85"/>"##,
            px(l.a.x),
            pz(l.a.z),
            px(l.b.x),
            pz(l.b.z),
            if i == 0 { 2.5 } else { 1.5 }
        );
    }
    // Launchers' throws.
    for (a, b, _) in &walk.throws {
        let _ = writeln!(
            svg,
            r##"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="#56d8ff" stroke-width="2.5"/><circle cx="{:.1}" cy="{:.1}" r="5" fill="#56d8ff"/>"##,
            px(a.x),
            pz(a.z),
            px(b.x),
            pz(b.z),
            px(b.x),
            pz(b.z)
        );
    }
    // Spawns.
    for sp in &m.spawns {
        let col = match sp.team {
            0 => "#ff5a4a",
            1 => "#4a8bff",
            _ => "#d0d0d0",
        };
        let (sy, cy) = sim::geom::yaw_sincos(sp.yaw);
        let (x, z) = (sp.pos.x, sp.pos.z);
        let _ = writeln!(
            svg,
            r##"<circle cx="{:.1}" cy="{:.1}" r="5" fill="{col}" stroke="#000" stroke-width="1"/><line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="{col}" stroke-width="2"/>"##,
            px(x),
            pz(z),
            px(x),
            pz(z),
            px(x + cy * 1.2),
            pz(z + sy * 1.2)
        );
    }
    // Places, a label stacked under another one on the same spot.
    let mut labels: Vec<(f32, f32)> = Vec::new();
    for p in &places {
        if p.side < 2 {
            continue;
        }
        let (x, mut y) = (px(p.pos.x) + 6.0, pz(p.pos.z) - 6.0);
        while labels
            .iter()
            .any(|&(lx, ly)| (lx - x).abs() < 90.0 && (ly - y).abs() < 14.0)
        {
            y += 15.0;
        }
        labels.push((x, y));
        let _ = writeln!(
            svg,
            r##"<circle cx="{:.1}" cy="{:.1}" r="4" fill="#fff"/><text x="{x:.1}" y="{y:.1}" fill="#fff" font-size="13" stroke="#000" stroke-width="3" paint-order="stroke">{}{}</text>"##,
            px(p.pos.x),
            pz(p.pos.z),
            p.name,
            if p.pos.y > 0.25 {
                format!(" · {:.1} m", p.pos.y)
            } else {
                String::new()
            }
        );
    }
    // The key: the height scale.
    let ky = h * s + 12.0;
    for i in 0..=8 {
        let y = top * i as f32 / 8.0;
        let _ = writeln!(
            svg,
            r##"<rect x="{}" y="{ky:.0}" width="40" height="16" fill="{}"/><text x="{}" y="{:.0}" fill="#ccc" font-size="11">{y:.1} m</text>"##,
            20 + i * 46,
            height_colour(y, top),
            22 + i * 46,
            ky + 30.0
        );
    }
    let _ = writeln!(
        svg,
        r##"<text x="450" y="{:.0}" fill="#ccc" font-size="13">{}: red and blue spawns, white places, cyan launcher throws, dashed: the longest sight lines (red ≥ {LONG_M:.0} m)</text>"##,
        ky + 13.0,
        def.name
    );
    let _ = writeln!(svg, "</svg>");
    std::fs::write(out.join(format!("{slug}.svg")), &svg).expect("the plan");
    println!("{}", out.join(format!("{slug}.svg")).display());
    println!("{}", out.join(format!("{slug}.md")).display());
    print!("{md}");
}

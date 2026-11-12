//! The shade a hall lays on its floors.
//!
//! Under a roof nothing casts: a hall's lamps throw no shadows and its fill
//! is one cube for the whole map (`fill::hall`), so a crate met the deck
//! with no line under it, the pit under a bridge was as bright as the pit
//! beside it, and a room had no shape but its strips. This is the light
//! that does not reach a floor, worked out once when the map is built and
//! laid on it as a sheet: black, its alpha at each vertex the share of the
//! floor's light that something stands in the way of.
//!
//! That light is the game's own, in its own proportions. The fill reaches a
//! floor from the dome over it, less what stands in that dome within
//! [`REACH_M`] (a bridge overhead, a crate beside); each lamp reaches it as
//! the renderer lights it (`level::build`'s lamps: the same lumens, range
//! and cone), less the lamp where the way to it is blocked, as far as that
//! lamp casts at all ([`Light::cast`]: one set over a roof to fill the hall
//! under it casts nothing). Bulkhead is two thirds fill, so most of its
//! shade is the dome's; the lamps' is what lies under its bridge.
//!
//! **What it is traced against is what collides** (`sim::map::Map::ray`),
//! which is what stands on a floor: a built set is drawn inside it. Only
//! floors carry it, where the drawn plane is the collided one; a wall is
//! drawn set back from its box, and a sheet on the box's plane would float.
//!
//! **The same on every rung, and it hides nobody**: a mesh and a blend,
//! nothing a rung lacks. A fighter is not under it (it lies on the floor),
//! so in a shaded place he stands out the more.

use bevy::asset::RenderAssetUsages;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use sim::geom::V3;
use sim::map::Map;
use sim::solid::Shape;

use super::fill;

/// A sheet's cell, metres: a contact line is drawn across one.
const CELL_M: f32 = 0.35;
/// How far over its floor the sheet lies, and how far over it a vertex
/// looks from, metres.
const LIFT_M: f32 = 0.008;
const EYE_M: f32 = 0.06;
/// A vertex looks from four places this far round it, metres, a share of
/// its looks from each: one standing on a box's own face would see all of
/// it or none.
const ROUND_M: f32 = 0.03;
/// How far the dome over a floor is looked into, metres: what stands
/// further off than this takes none of the fill, and what stands nearer
/// takes more of it the nearer it is.
const REACH_M: f32 = 4.5;
/// Looks into the dome, a vertex.
const DOME: usize = 24;
/// What stands in the dome takes this much more of the fill than its share
/// of the looks: a hall's fill is its lamps' light come back off the vault
/// and the walls, and what stands beside a floor stands in the way of the
/// lamps that lit them too. At 1 a crate's foot was a tenth darker than
/// the deck beside it, which is no line at all.
const DOME_TAKES: f32 = 1.4;
/// A lamp is this wide, metres (its panel, not the renderer's point), and
/// is looked at this many times across it, so its shadow has an edge a
/// cell can hold.
const LAMP_WIDE_M: f32 = 0.7;
const LAMP_LOOKS: usize = 4;
/// A way to a lamp is clear if nothing stands in it short of this far
/// from the lamp, metres: its fixture hangs in the roof's steel.
const LAMP_CLEAR_M: f32 = 0.8;
/// The lamps are weighed over the floors at points this far apart, metres,
/// up to this far over the highest spawn (what is fought on: a ledge in
/// the vault beside a lamp would have all of its light).
const WEIGH_M: f32 = 1.5;
const WEIGH_OVER_M: f32 = 3.5;
/// A floor this far over the highest spawn with nothing over it is a
/// roof's back: nobody sees it, and it carries no sheet.
const ROOF_OVER_M: f32 = 2.5;
/// A lamp giving a vertex less than this share of its light is not looked
/// at.
const LAMP_LEAST: f32 = 0.015;
/// The darkest the sheet is: a shaded floor is never a hole.
const DARKEST: f32 = 0.75;
/// Shade fainter than this is no shade: a floor with its dome open and
/// its lamps in sight carries no sheet, and the sheet fades to nothing
/// where it stops.
const FAINT: f32 = 0.05;

const _: () = assert!(DARKEST < 0.8 && FAINT < DARKEST && ROUND_M < EYE_M);

/// A lamp as the renderer lights with it: where, how much (lumens, by its
/// colour's luminance), whether it is a cone down, and how deep a shadow
/// it casts ([`Light::cast`]).
#[derive(Clone, Copy)]
struct Lamp {
    at: Vec3,
    lumens: f32,
    down: bool,
    casts: f32,
}

/// What a floor is lit by, in the renderer's own units over π.
struct Light<'a> {
    map: &'a Map,
    lamps: Vec<Lamp>,
    /// The fill on a floor facing up with its whole dome open.
    fill: f32,
    range: f32,
    cone: (f32, f32),
}

fn v3(v: Vec3) -> V3 {
    V3::new(v.x, v.y, v.z)
}

/// The dome's `k`th look: cosine-weighted, so each carries the same share
/// of what a floor gets from its dome.
fn dome(k: usize) -> Vec3 {
    let u = (k as f32 + 0.5) / DOME as f32;
    let a = k as f32 * 2.399_963_2;
    let r = u.sqrt();
    Vec3::new(r * a.cos(), (1.0 - u).sqrt(), r * a.sin())
}

/// The `k`th of four places round a point.
fn round(k: usize) -> Vec3 {
    let (x, z) = [(1.0, 1.0), (-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0)][k % 4];
    Vec3::new(x * ROUND_M, 0.0, z * ROUND_M)
}

impl Light<'_> {
    /// What lamp `l` gives a floor at `p` with nothing in the way: Bevy's
    /// own falloff, range window and cone.
    fn lamp(&self, l: &Lamp, p: Vec3) -> f32 {
        let to = l.at - p;
        let d2 = to.length_squared().max(1e-4);
        let d = d2.sqrt();
        let window = (1.0 - (d2 / (self.range * self.range)).powi(2)).clamp(0.0, 1.0).powi(2);
        let cos = (to.y / d).max(0.0);
        let cone = if l.down {
            let (inner, outer) = (self.cone.0.cos(), self.cone.1.cos());
            ((cos - outer) / (inner - outer).max(1e-4)).clamp(0.0, 1.0).powi(2)
        } else {
            1.0
        };
        l.lumens * cos * window * cone / (4.0 * core::f32::consts::PI * core::f32::consts::PI * d2)
    }

    /// The share of a floor's light that reaches it at `p`, `0..=1`.
    fn reaches(&self, p: Vec3) -> f32 {
        let eye = p + Vec3::Y * EYE_M;
        // The dome: what stands in it, by how near.
        let mut taken = 0.0;
        for k in 0..DOME {
            let t = self.map.ray(v3(eye + round(k)), v3(dome(k)), REACH_M);
            if t < REACH_M {
                taken += 1.0 - (t / REACH_M) * (t / REACH_M);
            }
        }
        let fill = self.fill * (1.0 - (taken / DOME as f32 * DOME_TAKES).min(1.0));
        let all: f32 = self.fill + self.lamps.iter().map(|l| self.lamp(l, p)).sum::<f32>();
        if all <= 0.0 {
            return 1.0;
        }
        let mut lit = fill;
        for l in &self.lamps {
            let gives = self.lamp(l, p);
            if gives < LAMP_LEAST * all {
                lit += gives;
                continue;
            }
            let mut seen = 0;
            for k in 0..LAMP_LOOKS {
                let a = k as f32 * core::f32::consts::TAU / LAMP_LOOKS as f32 + 0.6;
                let at = l.at + Vec3::new(a.cos(), 0.0, a.sin()) * LAMP_WIDE_M;
                let from = eye + round(k);
                let to = at - from;
                let d = to.length();
                let far = (d - LAMP_CLEAR_M).max(0.0);
                if self.map.ray(v3(from), v3(to / d), far) >= far {
                    seen += 1;
                }
            }
            let seen = seen as f32 / LAMP_LOOKS as f32;
            lit += gives * (seen + (1.0 - seen) * (1.0 - l.casts));
        }
        (lit / all).clamp(0.0, 1.0)
    }

    /// How deep a shadow each lamp casts, `0..=1`: the share of its light
    /// that lands on floors it can be seen from. The lamps were set by eye
    /// with nothing in their way, and some were set to be in the way of
    /// nothing: Cistern's stand over its vault for the day through its
    /// oculi, Chrysalis's great lamps light the deck under its decks. Such
    /// a lamp fills a place and shades none of it; one hung in the open
    /// over the floor it lights casts whole.
    fn cast(&mut self) {
        let floors = floors(self.map);
        for i in 0..self.lamps.len() {
            let l = self.lamps[i];
            let (mut all, mut seen) = (0.0, 0.0);
            for &p in &floors {
                let gives = self.lamp(&l, p);
                if gives <= 0.0 {
                    continue;
                }
                let from = p + Vec3::Y * EYE_M;
                let to = l.at - from;
                let d = to.length();
                let far = (d - LAMP_CLEAR_M).max(0.0);
                all += gives;
                if self.map.ray(v3(from), v3(to / d), far) >= far {
                    seen += gives;
                }
            }
            self.lamps[i].casts = if all > 0.0 { seen / all } else { 0.0 };
        }
    }

    /// The sheet's alpha at `p`: what does not reach it, less what is too
    /// faint to call shade.
    fn shade(&self, p: Vec3) -> f32 {
        let dark = (1.0 - self.reaches(p)).min(DARKEST);
        ((dark - FAINT) / (1.0 - FAINT)).max(0.0)
    }
}

/// The highest spawn, metres.
fn spawn_top(map: &Map) -> f32 {
    map.spawns.iter().fold(0.0f32, |m, s| m.max(s.pos.y))
}

/// Whether `p` is on a roof's back: over the fight, nothing over it.
fn roof(map: &Map, p: Vec3) -> bool {
    p.y > spawn_top(map) + ROOF_OVER_M && map.ray(v3(p + Vec3::Y * EYE_M), V3::new(0.0, 1.0, 0.0), 80.0) >= 80.0
}

/// Every floor of the map that is fought on at a coarse grid's points,
/// bottom to top: what the lamps are weighed over.
fn floors(map: &Map) -> Vec<Vec3> {
    let mut at = Vec::new();
    let top = spawn_top(map) + WEIGH_OVER_M;
    let mut x = map.lo.0 + WEIGH_M * 0.5;
    while x < map.hi.0 {
        let mut z = map.lo.1 + WEIGH_M * 0.5;
        while z < map.hi.1 {
            let mut reach = top;
            for _ in 0..8 {
                let y = map.floor(x, z, 0.0, reach);
                if !covered(map, Vec3::new(x, y + EYE_M, z)) {
                    at.push(Vec3::new(x, y, z));
                }
                if y <= map.ground(x, z) {
                    break;
                }
                reach = y - 0.3;
            }
            z += WEIGH_M;
        }
        x += WEIGH_M;
    }
    at
}

/// Whether something stands on `p`: a box or a solid it is inside.
fn covered(map: &Map, p: Vec3) -> bool {
    let q = v3(p);
    let mut hit = false;
    map.each_box_in((p.x, p.z), (p.x, p.z), |b| hit |= b.contains(q));
    map.each_solid_in((p.x, p.z), (p.x, p.z), |s| {
        hit |= if s.is_profile() {
            s.contains(q)
        } else {
            s.over(p.x, p.z, 0.0) && p.y > s.bot_at(p.x, p.z) && p.y < s.top_at(p.x, p.z)
        }
    });
    hit
}

/// The sheets being laid: one mesh for the map.
#[derive(Default)]
struct Sheets {
    at: Vec<[f32; 3]>,
    colour: Vec<[f32; 4]>,
    tris: Vec<u32>,
}

impl Sheets {
    /// A floor of `nu` by `nv` cells, `at(u, v)` its place for `u`, `v` in
    /// `0..=1`, its cells kept where `keep` says so of their middle and
    /// where there is any shade to lay.
    fn patch(
        &mut self,
        light: &Light,
        (nu, nv): (usize, usize),
        at: impl Fn(f32, f32) -> Vec3,
        keep: impl Fn(Vec3) -> bool,
    ) {
        let (nu, nv) = (nu.max(1), nv.max(1));
        let place = |i: usize, j: usize| at(i as f32 / nu as f32, j as f32 / nv as f32);
        let kept: Vec<bool> = (0..nu * nv)
            .map(|c| {
                let (i, j) = (c % nu, c / nu);
                let mid = at((i as f32 + 0.5) / nu as f32, (j as f32 + 0.5) / nv as f32);
                keep(mid) && !covered(light.map, mid + Vec3::Y * EYE_M) && !roof(light.map, mid)
            })
            .collect();
        // A vertex's shade, worked out for the vertices of kept cells only.
        let w = nu + 1;
        let mut shade = vec![f32::NAN; w * (nv + 1)];
        for (c, _) in kept.iter().enumerate().filter(|(_, on)| **on) {
            let (i, j) = (c % nu, c / nu);
            for (di, dj) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let v = (j + dj) * w + i + di;
                if shade[v].is_nan() {
                    shade[v] = light.shade(place(i + di, j + dj));
                }
            }
        }
        let mut index = vec![u32::MAX; shade.len()];
        for (c, _) in kept.iter().enumerate().filter(|(_, on)| **on) {
            let (i, j) = (c % nu, c / nu);
            let corners = [j * w + i, j * w + i + 1, (j + 1) * w + i + 1, (j + 1) * w + i];
            if corners.iter().all(|&v| shade[v] <= 0.0) {
                continue;
            }
            let mut ids = [0u32; 4];
            for (k, &v) in corners.iter().enumerate() {
                if index[v] == u32::MAX {
                    index[v] = self.at.len() as u32;
                    let p = place(v % w, v / w) + Vec3::Y * LIFT_M;
                    self.at.push(p.to_array());
                    self.colour.push([0.0, 0.0, 0.0, shade[v]]);
                }
                ids[k] = index[v];
            }
            // Wound to face up for a patch whose u runs along x and v along z.
            self.tris
                .extend_from_slice(&[ids[0], ids[2], ids[1], ids[0], ids[3], ids[2]]);
        }
    }

    fn mesh(self) -> Option<Mesh> {
        if self.tris.is_empty() {
            return None;
        }
        let n = self.at.len();
        Some(
            Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
                .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.at)
                .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; n])
                .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0, 0.0]; n])
                .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.colour)
                .with_inserted_indices(Indices::U32(self.tris)),
        )
    }
}

fn cells(len: f32) -> usize {
    (len / CELL_M).ceil().max(1.0) as usize
}

/// The map's floors under its lamps and its fill, as one mesh: the ground,
/// each box's top and each solid's (a round one, a turned box, a ramp)
/// that `floor_box` and `floor_solid` call a floor. `None` under an open
/// sky (the sun casts there), or where there is no shade to lay.
///
/// `fill_units` is the fill on a floor with its dome open, in the
/// renderer's units (`rig::expose_for`'s intensity times the hall's cube
/// looking up); then the lamps' range, their cone and the height from
/// which one is a cone down (`level`'s), and whether the ground is drawn.
pub fn mesh(
    map: &Map,
    fill_units: f32,
    (range, cone, down_from): (f32, (f32, f32), f32),
    ground: bool,
    floor_box: impl Fn(usize) -> bool,
    floor_solid: impl Fn(usize) -> bool,
) -> Option<Mesh> {
    if map.light.sun > 0.0 || map.terrain.is_some() {
        return None;
    }
    let lamps: Vec<Lamp> = map
        .light
        .lamps
        .iter()
        .map(|&(p, c, lm)| Lamp {
            at: Vec3::from_array(p),
            lumens: lm * fill::luminance(c.map(fill::srgb_to_linear)),
            down: p[1] >= down_from,
            casts: 1.0,
        })
        .collect();
    let top = lamps.iter().fold(0.0f32, |m, l| m.max(l.at.y));
    let mut light = Light {
        map,
        lamps,
        fill: fill_units,
        range,
        cone,
    };
    light.cast();
    let mut sheets = Sheets::default();
    // The ground, where it is drawn (Cistern's is under its water).
    let (lo, hi) = (Vec2::new(map.lo.0, map.lo.1), Vec2::new(map.hi.0, map.hi.1));
    let size = hi - lo;
    if ground {
        sheets.patch(
            &light,
            (cells(size.x), cells(size.y)),
            |u, v| Vec3::new(lo.x + size.x * u, 0.0, lo.y + size.y * v),
            |_| true,
        );
    }
    // A top another floor at its height already lies on is that one's.
    let same = |y: f32, other: f32| (y - other).abs() < 2e-3;
    let boxes = &map.boxes;
    for (i, b) in boxes.iter().enumerate() {
        if !floor_box(i) || b.max.y > top || b.max.y <= 0.0 {
            continue;
        }
        let (min, size) = (b.min, b.max - b.min);
        let y = b.max.y;
        sheets.patch(
            &light,
            (cells(size.x), cells(size.z)),
            |u, v| Vec3::new(min.x + size.x * u, y, min.z + size.z * v),
            |mid| {
                !boxes[..i]
                    .iter()
                    .enumerate()
                    .any(|(j, o)| floor_box(j) && same(o.max.y, y) && o.over_xz(mid.x, mid.z, 0.0))
            },
        );
    }
    for (i, s) in map.solids.iter().enumerate() {
        if !floor_solid(i) || s.y1 > top {
            continue;
        }
        match s.shape {
            Shape::Round { x, z, r } => {
                // A square laid over the disc, its rim on the disc's.
                let n = cells(2.0 * r);
                sheets.patch(
                    &light,
                    (n, n),
                    |u, v| {
                        let (a, b) = (2.0 * u - 1.0, 2.0 * v - 1.0);
                        let (m, l) = (a.abs().max(b.abs()), a.hypot(b).max(1e-6));
                        Vec3::new(x + r * a * m / l, s.y1, z + r * b * m / l)
                    },
                    |_| true,
                );
            }
            Shape::Turned {
                x,
                z,
                hx,
                hz,
                c,
                s: sin,
            } => {
                sheets.patch(
                    &light,
                    (cells(2.0 * hx), cells(2.0 * hz)),
                    |u, v| {
                        let (a, b) = ((2.0 * u - 1.0) * hx, (2.0 * v - 1.0) * hz);
                        Vec3::new(x + a * c - b * sin, s.y1, z + a * sin + b * c)
                    },
                    |_| true,
                );
            }
            Shape::Ramp { x0, z0, x1, z1, .. } => {
                sheets.patch(
                    &light,
                    (cells(x1 - x0), cells(z1 - z0)),
                    |u, v| {
                        let (px, pz) = (x0 + (x1 - x0) * u, z0 + (z1 - z0) * v);
                        Vec3::new(px, s.top_at(px, pz), pz)
                    },
                    |_| true,
                );
            }
            _ => {}
        }
    }
    sheets.mesh()
}

/// The fill on a floor of map `id` with its dome open, in the renderer's
/// units: the camera's (`rig::expose_for`) times what the hall's cube shows
/// a floor looking up.
pub fn fill_units(map: &Map, id: u8) -> f32 {
    let mood = super::mood::of(id);
    let hall = fill::hall::PALETTES[mood.hall];
    fill::peak_lux()
        * map.light.fill
        * mood.fill
        * fill::hall::GAIN
        * fill::luminance(fill::hall::diffuse_at(hall, Vec3::Y))
}

/// The sheet's material: black, its vertices' alpha, lit by nothing.
pub fn material() -> StandardMaterial {
    StandardMaterial {
        base_color: Color::BLACK,
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        depth_bias: 0.5,
        ..default()
    }
}

/// What rides with the sheet's mesh and material.
pub fn bundle() -> impl Bundle {
    (NotShadowCaster, NotShadowReceiver)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The dome's looks all go up, and carry the whole dome between them:
    /// their mean height is a cosine-weighted hemisphere's, two thirds.
    #[test]
    fn the_dome_is_a_hemisphere() {
        let mut up = 0.0;
        for k in 0..DOME {
            let d = dome(k);
            assert!(d.y > 0.0 && (d.length() - 1.0).abs() < 1e-4);
            up += d.y / DOME as f32;
        }
        assert!((up - 2.0 / 3.0).abs() < 0.03, "{up}");
    }

    /// Every roofed map has shade to lay, none of it past [`DARKEST`] and
    /// none of it over all of a floor (the open deck is bare, where it
    /// would be a blend over every pixel for nothing); under the sun there
    /// is none, the sun casts there.
    #[test]
    fn a_hall_is_shaded_and_never_black() {
        for def in sim::maps::ALL.iter() {
            let map = def.build();
            let m = mesh(
                &map,
                fill_units(&map, map.id),
                (22.0, (0.6, 1.05), 13.0),
                true,
                |_| true,
                |_| true,
            );
            if map.light.sun > 0.0 {
                assert!(m.is_none(), "{}: the sun casts there", map.name);
                continue;
            }
            let m = m.unwrap_or_else(|| panic!("{}: no shade", map.name));
            let Some(bevy::mesh::VertexAttributeValues::Float32x4(c)) = m.attribute(Mesh::ATTRIBUTE_COLOR) else {
                panic!("no colours");
            };
            assert!(c.iter().all(|c| (0.0..=DARKEST).contains(&c[3])), "{}", map.name);
            let floor = (map.hi.0 - map.lo.0) * (map.hi.1 - map.lo.1);
            let cells = m.indices().map_or(0, |i| i.len() / 6) as f32;
            let deep = c.iter().filter(|c| c[3] > 0.5).count() as f32 / c.len() as f32;
            assert!(cells * CELL_M * CELL_M > 0.05 * floor, "{}: {cells} cells", map.name);
            assert!(deep < 0.4, "{}: {deep} of it is deep shade", map.name);
        }
    }

    /// A lamp set where no floor sees it fills its hall and casts nothing
    /// (Cistern's five stand over its vault, for the day through its
    /// oculi); one hung in the open over its floor casts most of a shadow.
    #[test]
    fn a_lamp_over_a_roof_casts_nothing() {
        let weigh = |id: u8| {
            let map = sim::map::by_id(id).unwrap();
            let lamps = map.light.lamps;
            let mut l = Light {
                map: &map,
                lamps: lamps
                    .iter()
                    .map(|&(p, _, lm)| Lamp {
                        at: Vec3::from_array(p),
                        lumens: lm,
                        down: p[1] >= 13.0,
                        casts: 1.0,
                    })
                    .collect(),
                fill: 0.0,
                range: 22.0,
                cone: (0.6, 1.05),
            };
            l.cast();
            l.lamps.iter().map(|l| l.casts).collect::<Vec<_>>()
        };
        let cistern = weigh(2);
        assert!(cistern[..5].iter().all(|c| *c == 0.0), "{cistern:?}");
        assert!(cistern[5..].iter().all(|c| *c > 0.5), "{cistern:?}");
        let dock = weigh(5);
        assert!(dock.iter().all(|c| *c > 0.6), "{dock:?}");
    }
}

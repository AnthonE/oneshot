//! Chips: the lit, tumbling bits of what a blow struck (concrete grit, a
//! flake of metal, a clod of soil), thrown off the face it met and gone in
//! half a second. Light reads as light; a chip is a thing in the world, so
//! it is lit by the sun like the wall it came off, and it is what makes a
//! round landing on concrete look like it broke something.
//!
//! After Gates' `crates/client/src/render/impact.rs` (MIT, © 2026 MoreRight
//! DAO): its chip pool, motion and colours by matter. Gates draws each chip
//! as an entity; ours are one mesh of [`CHIPS`] small boxes in world space,
//! written while any fly, so they are one draw on WebGL2 as on the desktop
//! (the particles' way, `pool.rs`). They bounce off the plane of the face
//! they came off and off the floor under where they were struck (a level
//! plane, found once at the throw) instead of falling through either, and
//! shrink out at the end
//! rather than fade (no sort, no blend). A fixed pool, drop-oldest: a chip
//! has half a second and the oldest is the smallest. The same on every rung:
//! a 3 cm chip can't hide anything, and a lower rung never sees more.

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::light::NotShadowCaster;
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;

use super::table::Matter;

/// Chips in flight at once: twelve blows' worth at [`BURST`].
pub const CHIPS: usize = 96;
/// A blow's default count (the table says each blow's own).
pub const BURST: usize = 8;
/// How long a chip lives, seconds: a punctuation mark, not litter.
pub const LIFE_S: f32 = 0.55;
/// A chip's edge, metres: 3 cm reads as debris, 4.5 as cardboard.
pub const SIZE_M: f32 = 0.03;
/// Its depth against its edge: a flake, not a cube.
const THIN: f32 = 0.42;
/// How fast a chip leaves (the mean; each is rolled from half to one and a
/// half of it), m/s.
pub const SPEED_MPS: f32 = 3.2;
pub const GRAVITY_MPS2: f32 = 9.81;
/// How fast a chip tumbles, rad/s.
pub const SPIN_RAD_S: f32 = 11.0;
/// The share of a chip's launch along the face's normal: the cone.
pub const SPRAY: f32 = 0.55;
/// What a bounce keeps of the speed into the face, and along it.
const BOUNCE: f32 = 0.3;
const SKID: f32 = 0.6;
/// The share of its life over which a chip shrinks out at the end.
const SHRINK: f32 = 0.34;

/// A chip's colour (sRGB): values chosen to stand clear of the surface it
/// comes off at arm's length, not to match it (Gates' `Matter::color`).
pub fn color(m: Matter) -> Color {
    match m {
        Matter::Concrete => Color::srgb(0.55, 0.54, 0.51),
        Matter::Metal | Matter::Hull => Color::srgb(0.86, 0.78, 0.55),
        Matter::Dirt => Color::srgb(0.44, 0.37, 0.28),
        Matter::Flesh => Color::srgb(0.46, 0.10, 0.10),
        Matter::Shield => Color::srgb(1.0, 0.85, 0.4),
    }
}

#[derive(Clone, Copy, Default)]
struct Chip {
    /// Seconds left; zero is a free slot.
    left: f32,
    life: f32,
    pos: Vec3,
    vel: Vec3,
    spin: Vec3,
    rot: Quat,
    /// Linear colour.
    color: [f32; 4],
    /// The face it came off: a point on it and its normal.
    at: Vec3,
    n: Vec3,
    /// The floor's height under the blow (`surface::floor_under`).
    floor: f32,
    /// Its box, metres: a flake for a chip, a cylinder's worth for a casing.
    dims: Vec3,
}

/// A casing out of a gun: brass, 9 by 25 mm; a shotgun's shell, red, 18
/// by 60. How long it lies glinting before it is gone, seconds.
pub const CASING_S: f32 = 1.6;
const BRASS: Vec3 = Vec3::new(0.009, 0.009, 0.025);
const SHELL: Vec3 = Vec3::new(0.018, 0.018, 0.06);

pub struct Chips {
    slots: Vec<Chip>,
    /// The next to steal when every slot flies.
    next: usize,
    rng: u32,
    mesh: Handle<Mesh>,
    /// Whether the mesh holds a chip that must be cleared.
    drawn: bool,
    /// Chips thrown, and live ones recycled.
    pub thrown: u64,
    pub stolen: u64,
}

impl Default for Chips {
    fn default() -> Self {
        Chips {
            slots: vec![Chip::default(); CHIPS],
            next: 0,
            rng: 0x9E37_79B9,
            mesh: Handle::default(),
            drawn: false,
            thrown: 0,
            stolen: 0,
        }
    }
}

/// The eight corners of a unit box, and its six faces (corner indices,
/// counter-clockwise from outside) with their normals.
const CORNERS: [[f32; 3]; 8] = [
    [-0.5, -0.5, -0.5],
    [0.5, -0.5, -0.5],
    [0.5, 0.5, -0.5],
    [-0.5, 0.5, -0.5],
    [-0.5, -0.5, 0.5],
    [0.5, -0.5, 0.5],
    [0.5, 0.5, 0.5],
    [-0.5, 0.5, 0.5],
];
const FACES: [([usize; 4], [f32; 3]); 6] = [
    ([4, 5, 6, 7], [0.0, 0.0, 1.0]),
    ([1, 0, 3, 2], [0.0, 0.0, -1.0]),
    ([5, 1, 2, 6], [1.0, 0.0, 0.0]),
    ([0, 4, 7, 3], [-1.0, 0.0, 0.0]),
    ([7, 6, 2, 3], [0.0, 1.0, 0.0]),
    ([0, 1, 5, 4], [0.0, -1.0, 0.0]),
];
const VERTS: usize = 24;

impl Chips {
    fn roll(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng >> 8) as f32 / (1 << 24) as f32
    }

    fn signed(&mut self) -> f32 {
        self.roll() * 2.0 - 1.0
    }

    fn claim(&mut self) -> usize {
        if let Some(i) = self.slots.iter().position(|c| c.left <= 0.0) {
            return i;
        }
        self.stolen += 1;
        let i = self.next;
        self.next = (self.next + 1) % self.slots.len();
        i
    }

    /// How many fly.
    pub fn live(&self) -> usize {
        self.slots.iter().filter(|c| c.left > 0.0).count()
    }

    /// Throw `n` chips of `matter` off the face at `at` facing `away`, over
    /// a floor at height `floor` (`f32::NEG_INFINITY`: none).
    pub fn throw(&mut self, at: Vec3, away: Vec3, matter: Matter, n: usize, floor: f32) {
        let away = away.normalize_or(Vec3::Y);
        let c = color(matter).to_linear();
        for _ in 0..n {
            // A cone about the normal: a scattered unit vector blended
            // toward it, so nothing goes into the face it came off.
            let scatter = Vec3::new(self.signed(), self.signed(), self.signed()).normalize_or(Vec3::Y);
            let dir = (away * SPRAY + scatter * (1.0 - SPRAY)).normalize_or(away);
            let speed = SPEED_MPS * (0.5 + self.roll());
            let spin = Vec3::new(self.signed(), self.signed(), self.signed()) * SPIN_RAD_S;
            let rot = Quat::from_euler(
                EulerRot::YXZ,
                self.signed() * core::f32::consts::PI,
                self.signed() * core::f32::consts::PI,
                self.signed() * core::f32::consts::PI,
            );
            let life = LIFE_S * (0.7 + 0.6 * self.roll());
            let j = 0.85 + 0.3 * self.roll();
            let edge = SIZE_M * (0.8 + 0.4 * self.roll());
            let i = self.claim();
            self.slots[i] = Chip {
                left: life,
                life,
                // Born off the face, so it doesn't z-fight it.
                pos: at + dir * (SIZE_M * 1.5),
                vel: dir * speed + Vec3::Y * (SPEED_MPS * 0.25),
                spin,
                rot,
                color: [c.red * j, c.green * j, c.blue * j, 1.0],
                at,
                n: away,
                floor,
                dims: Vec3::new(edge, edge, edge * THIN),
            };
            self.thrown += 1;
        }
    }

    /// A casing thrown out of a gun at `at`, out its port along `out`
    /// (the shooter's right, a little up), tumbling, onto a floor at
    /// `floor`. A shotgun throws its `shell`.
    pub fn casing(&mut self, at: Vec3, out: Vec3, floor: f32, shell: bool) {
        let out = out.normalize_or(Vec3::X);
        let scatter = Vec3::new(self.signed(), self.signed(), self.signed()) * 0.35;
        let dir = (out + Vec3::Y * 0.5 + scatter).normalize_or(out);
        let speed = (2.2 + 1.4 * self.roll()) * if shell { 0.8 } else { 1.0 };
        let spin = Vec3::new(self.signed(), self.signed(), self.signed()) * SPIN_RAD_S * 2.5;
        let rot = Quat::from_euler(
            EulerRot::YXZ,
            self.signed() * core::f32::consts::PI,
            self.signed() * core::f32::consts::PI,
            self.signed() * core::f32::consts::PI,
        );
        let c = if shell {
            Color::srgb(0.75, 0.12, 0.1)
        } else {
            Color::srgb(0.9, 0.7, 0.32)
        }
        .to_linear();
        let life = CASING_S * (0.85 + 0.3 * self.roll());
        let i = self.claim();
        self.slots[i] = Chip {
            left: life,
            life,
            pos: at,
            vel: dir * speed,
            spin,
            rot,
            color: [c.red, c.green, c.blue, 1.0],
            // No face to bounce off: the plane is put behind it.
            at: at - out,
            n: out,
            floor,
            dims: if shell { SHELL } else { BRASS },
        };
        self.thrown += 1;
    }

    /// Every chip on by `dt`: falling, tumbling, bouncing off its face and
    /// the floor.
    pub fn step(&mut self, dt: f32) {
        if dt <= 0.0 {
            return;
        }
        for c in &mut self.slots {
            if c.left <= 0.0 {
                continue;
            }
            c.left = (c.left - dt).max(0.0);
            c.vel.y -= GRAVITY_MPS2 * dt;
            c.pos += c.vel * dt;
            let behind = (c.pos - c.at).dot(c.n) - SIZE_M * 0.5;
            if behind < 0.0 {
                c.pos -= c.n * behind;
                let into = c.vel.dot(c.n);
                if into < 0.0 {
                    let along = c.vel - c.n * into;
                    c.vel = along * SKID - c.n * (into * BOUNCE);
                    c.spin *= SKID;
                }
            }
            let low = c.pos.y - SIZE_M * 0.5 - c.floor;
            if low < 0.0 {
                c.pos.y -= low;
                if c.vel.y < 0.0 {
                    c.vel = Vec3::new(c.vel.x * SKID, -c.vel.y * BOUNCE, c.vel.z * SKID);
                    c.spin *= SKID;
                }
            }
            c.rot = (c.rot * Quat::from_euler(EulerRot::YXZ, c.spin.x * dt, c.spin.y * dt, c.spin.z * dt)).normalize();
        }
    }

    /// Roll from `seed` (the fx stage's, so a strip repeats).
    pub fn seed(&mut self, seed: u32) {
        self.rng = (seed ^ 0x9E37_79B9).max(1);
    }

    /// Forget every chip.
    pub fn clear(&mut self) {
        for c in &mut self.slots {
            c.left = 0.0;
        }
    }

    /// The mesh the chips are drawn in.
    pub fn mesh(&self) -> &Handle<Mesh> {
        &self.mesh
    }

    /// Whether the mesh needs writing: something flies, or something did.
    pub fn needs_write(&self) -> bool {
        self.drawn || self.slots.iter().any(|c| c.left > 0.0)
    }

    /// Chip `i`'s place, turn and edge, None for a free slot.
    pub fn draw(&self, i: usize) -> Option<(Vec3, Quat, Vec3)> {
        let c = self.slots.get(i)?;
        if c.left <= 0.0 {
            return None;
        }
        let k = (c.left / (c.life * SHRINK).max(1e-4)).min(1.0);
        Some((c.pos, c.rot, c.dims * k))
    }

    /// Copy the chips into `mesh`, in place: a free slot collapsed.
    pub fn write(&mut self, mesh: &mut Mesh) {
        let (mut pos, mut nrm, mut col) = (None, None, None);
        for (attr, values) in mesh.attributes_mut() {
            let id = attr.id;
            match values {
                VertexAttributeValues::Float32x3(v) => {
                    if id == Mesh::ATTRIBUTE_POSITION.id {
                        pos = Some(v);
                    } else if id == Mesh::ATTRIBUTE_NORMAL.id {
                        nrm = Some(v);
                    }
                }
                VertexAttributeValues::Float32x4(v) if id == Mesh::ATTRIBUTE_COLOR.id => col = Some(v),
                _ => {}
            }
        }
        let (Some(pos), Some(nrm), Some(col)) = (pos, nrm, col) else {
            return;
        };
        let mut any = false;
        for i in 0..self.slots.len() {
            let base = i * VERTS;
            if base + VERTS > pos.len() {
                break;
            }
            match self.draw(i) {
                None => pos[base..base + VERTS].fill([0.0; 3]),
                Some((at, rot, scale)) => {
                    any = true;
                    for (f, (corners, n)) in FACES.iter().enumerate() {
                        let n = (rot * Vec3::from_array(*n)).to_array();
                        for (k, &ci) in corners.iter().enumerate() {
                            let p = at + rot * (Vec3::from_array(CORNERS[ci]) * scale);
                            let v = base + f * 4 + k;
                            pos[v] = p.to_array();
                            nrm[v] = n;
                            col[v] = self.slots[i].color;
                        }
                    }
                }
            }
        }
        self.drawn = any;
    }
}

/// `n` collapsed chips and their fixed indices.
pub fn chip_mesh(n: usize) -> Mesh {
    let mut idx = Vec::with_capacity(n * 36);
    for c in 0..n as u32 {
        for f in 0..6u32 {
            let b = c * VERTS as u32 + f * 4;
            idx.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
        }
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; n * VERTS]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 1.0, 0.0]; n * VERTS]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[1.0f32; 4]; n * VERTS]);
    mesh.insert_indices(Indices::U32(idx));
    mesh
}

/// The chip mesh and its lit material, always drawn (collapsed until a blow
/// lands), so its pipeline compiles at load.
pub fn setup(
    commands: &mut Commands,
    chips: &mut Chips,
    meshes: &mut Assets<Mesh>,
    mats: &mut Assets<StandardMaterial>,
) {
    chips.mesh = meshes.add(chip_mesh(chips.slots.len()));
    let mat = mats.add(StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 0.85,
        metallic: 0.0,
        reflectance: crate::look::fresnel::DIELECTRIC,
        ..default()
    });
    commands.spawn((
        Mesh3d(chips.mesh.clone()),
        MeshMaterial3d(mat),
        Transform::IDENTITY,
        Visibility::Visible,
        NoFrustumCulling,
        NotShadowCaster,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A burst stays within its pool, flies off its face, never through it,
    /// and is gone by the end of its life.
    #[test]
    fn chips_fly_off_their_face_and_go() {
        let mut c = Chips::default();
        for _ in 0..20 {
            c.throw(Vec3::ZERO, Vec3::Y, Matter::Concrete, BURST, 0.0);
        }
        assert_eq!(c.live(), CHIPS);
        assert_eq!(c.stolen, 20 * BURST as u64 - CHIPS as u64);
        for _ in 0..30 {
            c.step(1.0 / 60.0);
            for i in 0..CHIPS {
                if let Some((p, _, _)) = c.draw(i) {
                    assert!(p.y >= SIZE_M * 0.5 - 1e-4, "through the floor: {p}");
                }
            }
        }
        let mut mesh = chip_mesh(CHIPS);
        assert!(c.needs_write());
        c.write(&mut mesh);
        for _ in 0..60 {
            c.step(1.0 / 60.0);
        }
        assert_eq!(c.live(), 0);
        assert!(c.needs_write(), "the last frame's chips must be cleared");
        c.write(&mut mesh);
        assert!(!c.needs_write());
        let Some(VertexAttributeValues::Float32x3(p)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else {
            panic!("positions");
        };
        assert!(p.iter().all(|v| *v == [0.0; 3]));
    }

    /// Chips off a wall low down land on the floor under it, not through.
    #[test]
    fn chips_off_a_wall_stop_on_the_floor() {
        let mut c = Chips::default();
        c.throw(Vec3::new(0.0, 0.3, 0.0), Vec3::X, Matter::Concrete, BURST, 0.0);
        for _ in 0..40 {
            c.step(1.0 / 60.0);
            for i in 0..CHIPS {
                if let Some((p, _, _)) = c.draw(i) {
                    assert!(p.y >= SIZE_M * 0.5 - 1e-4, "through the floor: {p}");
                }
            }
        }
    }
}

//! Tracers: a round's line of light, one mesh and one draw for all of them.
//!
//! Each is a capsule in screen space that `shaders/tracer.wgsl` works out
//! per pixel: its two ends projected, a quad spread round them in pixels,
//! every pixel shaded by its distance to the segment between. So its width
//! is held between a floor and a ceiling in pixels at any distance, where a
//! streak sprite's world width was a carrot twenty pixels fat a hand's
//! length off the eye and under a pixel across the map; under the floor it
//! dims instead of thinning (Humus' phone-wire AA), and seen end on it is a
//! dot, not nothing. Across, a hot core in a glow of the round's colour;
//! along, dark at the tail and white-hot at the head.
//!
//! - a **round** flies from the muzzle at its drawn speed, never shorter
//!   than a frame's travel (no gaps on a slow frame), stops at what it met
//!   and drains into it; with a life, it fades in place over it, thinning
//!   and warming (the references' lines don't fly far: they appear nearly
//!   whole and die where they are); with a drain, its tail leaves the
//!   shooter's end at that speed instead of a length behind its head, so a
//!   line that flies out in a frame or two stays nearly whole as it fades
//!   (Halo's sniper line: gone from the shooter's end first);
//! - a **beam** stands whole for its life, thinning and dimming;
//! - a **wake** is the vapour a heavy round leaves on its line, widening
//!   and breaking up as it hangs.
//!
//! The moodboard behind the numbers is `moodboard/tracers` (private).

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::{Indices, MeshVertexAttribute, MeshVertexBufferLayoutRef, PrimitiveTopology, VertexAttributeValues};
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, RenderPipelineDescriptor, SpecializedMeshPipelineError, VertexFormat,
};
use bevy::shader::ShaderRef;

use super::Fx;

pub const SHADER: &str = "shaders/tracer.wgsl";

/// Lines alive at once: a Big Team firefight's every round for a second,
/// with room. The oldest goes first.
pub const CAP: usize = 256;

/// Nearer the eye than this a line is cut (metres, along the view): the
/// camera's near plane is 0.1.
const NEAR_M: f32 = 0.12;
/// A beam's or a fading round's width at the end of its life, times its
/// first.
const BEAM_THIN: f32 = 0.4;
const ROUND_THIN: f32 = 0.55;
/// A fading round's core at the end of its life: its glow's colour, this
/// much brighter. A line cools from white-hot to its colour as it dies.
const COOLED: f32 = 1.6;
/// How far a wake spreads over its life, metres of half-width, on a square
/// root: quick at first, then slowly; and how much of it is gone from the
/// shooter's end by then.
const WAKE_GROW_M: f32 = 0.09;
const WAKE_DRAIN: f32 = 0.9;
/// How much of what is behind it a wake hides at most: a breath, not a wall.
const WAKE_COVER: f32 = 0.4;

/// Each vertex carries its line's both ends, so all four corners work out
/// the same capsule (`tracer.wgsl`), and its own corner: `ATTRIBUTE_POSITION`
/// is the tail, relative to the mesh's anchor.
pub const ATTR_HEAD: MeshVertexAttribute =
    MeshVertexAttribute::new("Tracer_Head", 0x7AC3_0001, VertexFormat::Float32x3);
/// Side (-1, 1), end (0 the tail, 1 the head), half-width at the head (m),
/// metres off its start it takes to come up to full.
pub const ATTR_CORNER: MeshVertexAttribute =
    MeshVertexAttribute::new("Tracer_Corner", 0x7AC3_0002, VertexFormat::Float32x4);
/// Metres off its start at the tail and the head, the head's hot dot (times
/// the core), and a wake's age (0 to 1; negative for a line).
pub const ATTR_ALONG: MeshVertexAttribute =
    MeshVertexAttribute::new("Tracer_Along", 0x7AC3_0003, VertexFormat::Float32x4);
/// The core's colour (linear HDR) and how bright the whole line is; zero
/// collapses it.
pub const ATTR_CORE: MeshVertexAttribute =
    MeshVertexAttribute::new("Tracer_Core", 0x7AC3_0004, VertexFormat::Float32x4);
/// The glow's colour, and a line's widest (half, pixels at 1080p) or a
/// wake's seed.
pub const ATTR_EDGE: MeshVertexAttribute =
    MeshVertexAttribute::new("Tracer_Edge", 0x7AC3_0005, VertexFormat::Float32x4);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    #[default]
    Round,
    Beam,
    Wake,
}

/// One line.
#[derive(Clone, Copy, Debug, Default)]
pub struct Tracer {
    pub kind: Kind,
    pub from: Vec3,
    pub to: Vec3,
    /// A round's drawn speed (m/s), the streak's length (m), and how fast
    /// its tail leaves the start (m/s; 0: its length behind its head).
    pub mps: f32,
    pub len: f32,
    pub drain: f32,
    /// Half-width at the head, metres (the tail's is the shader's share),
    /// and the most it is drawn, half, in pixels at 1080p.
    pub width: f32,
    pub px: f32,
    /// Linear HDR: the core, white-hot, and the glow round it.
    pub core: [f32; 3],
    pub edge: [f32; 3],
    /// The head's hot dot, times the core.
    pub head: f32,
    /// Metres off `from` it takes to come up to full: out of the flash.
    pub fade_in: f32,
    /// A beam's or a wake's life, seconds; a round's, over which it fades
    /// (0: until it is spent).
    pub life: f32,
    /// Seconds since it was fired, and whether it still shows: `fire` and
    /// `step` keep them.
    pub(super) age: f32,
    pub(super) live: bool,
}

/// The lines, a fixed array: firing never allocates.
pub struct Tracers {
    lines: Box<[Tracer; CAP]>,
    next: usize,
    /// The last write showed something.
    drawn: bool,
    /// The last frame's length: a round is drawn at least this far of its
    /// travel, the shutter.
    dt: f32,
}

impl Default for Tracers {
    fn default() -> Self {
        Tracers {
            lines: Box::new([Tracer::default(); CAP]),
            next: 0,
            drawn: false,
            dt: 1.0 / 60.0,
        }
    }
}

/// What of a line is drawn this frame: metres off its start at the tail
/// and the head, its half-width at the head, how bright, a wake's age.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Span {
    tail: f32,
    head: f32,
    width: f32,
    bright: f32,
    hot: f32,
    wake: f32,
    /// How far it has cooled from white-hot (0) to its colour (1).
    cool: f32,
}

impl Tracers {
    /// A line from now; the oldest goes if all are flying.
    pub fn fire(&mut self, t: Tracer) {
        if t.from.distance_squared(t.to) < 1e-4 || !t.from.is_finite() || !t.to.is_finite() {
            return;
        }
        let i = (0..CAP)
            .map(|k| (self.next + k) % CAP)
            .find(|&i| !self.lines[i].live)
            .unwrap_or(self.next);
        self.lines[i] = Tracer {
            age: 0.0,
            live: true,
            ..t
        };
        self.next = (i + 1) % CAP;
    }

    pub fn live(&self) -> usize {
        self.lines.iter().filter(|t| t.live).count()
    }

    pub fn clear(&mut self) {
        for t in self.lines.iter_mut() {
            t.live = false;
        }
    }

    pub fn needs_write(&self) -> bool {
        self.drawn || self.lines.iter().any(|t| t.live)
    }

    /// What of `t` shows this frame, or none once it is spent.
    fn span(&self, t: &Tracer) -> Option<Span> {
        let total = t.from.distance(t.to);
        match t.kind {
            Kind::Round => {
                // Where its head would be, and its tail as far back as it
                // went this frame or its length, whichever is more; or as
                // far as its drain has come.
                let s = t.len + t.mps * t.age;
                let tail = if t.drain > 0.0 {
                    t.drain * t.age
                } else {
                    (s - t.len.max(t.mps * self.dt)).max(0.0)
                };
                let k = if t.life > 0.0 { t.age / t.life } else { 0.0 };
                (tail < total && k < 1.0).then(|| Span {
                    tail,
                    head: s.min(total),
                    width: t.width * (1.0 - (1.0 - ROUND_THIN) * k),
                    bright: (1.0 - k).powf(1.5),
                    hot: t.head * (1.0 - k) * (1.0 - k),
                    wake: -1.0,
                    cool: k,
                })
            }
            Kind::Beam => {
                let k = t.age / t.life.max(1e-3);
                (k < 1.0).then(|| Span {
                    tail: 0.0,
                    head: total,
                    width: t.width * (1.0 - (1.0 - BEAM_THIN) * k),
                    bright: (1.0 - k) * (1.0 - k),
                    hot: t.head * (1.0 - k).powi(4),
                    wake: -1.0,
                    cool: 0.0,
                })
            }
            Kind::Wake => {
                let k = t.age / t.life.max(1e-3);
                (k < 1.0).then(|| Span {
                    tail: total * WAKE_DRAIN * k.sqrt(),
                    head: total,
                    width: t.width + WAKE_GROW_M * k.sqrt(),
                    bright: WAKE_COVER * (1.0 - k).powf(1.5),
                    hot: 0.0,
                    wake: k,
                    cool: 0.0,
                })
            }
        }
    }

    /// On by `dt`, after they are written: a line fired this frame is
    /// drawn once at its start.
    pub fn step(&mut self, dt: f32) {
        self.dt = dt.clamp(1e-3, 0.1);
        for i in 0..CAP {
            if !self.lines[i].live {
                continue;
            }
            self.lines[i].age += dt;
            let t = self.lines[i];
            self.lines[i].live = self.span(&t).is_some();
        }
    }

    /// Every line into `mesh`, relative to `anchor`, cut at the eye's near
    /// plane (`eye`, looking along `fwd`).
    pub fn write(&mut self, mesh: &mut Mesh, eye: Vec3, fwd: Vec3, anchor: Vec3) {
        let mut cuts = [(Vec3::ZERO, Vec3::ZERO, DEAD); CAP];
        let mut any = false;
        for (i, c) in cuts.iter_mut().enumerate() {
            let t = &self.lines[i];
            if let Some(k) = t.live.then(|| self.span(t)).flatten().and_then(|s| cut(t, s, eye, fwd)) {
                *c = k;
                any = true;
            }
        }
        for (attr, values) in mesh.attributes_mut() {
            let id = attr.id;
            for (i, &(tail, head, s)) in cuts.iter().enumerate() {
                let t = self.lines[i];
                let corner = |k: usize| -> [f32; 4] {
                    let (side, end) = CORNERS[k];
                    [side, end, s.width, t.fade_in]
                };
                match values {
                    VertexAttributeValues::Float32x3(v) if id == Mesh::ATTRIBUTE_POSITION.id => {
                        v[i * 4..i * 4 + 4].fill((tail - anchor).into());
                    }
                    VertexAttributeValues::Float32x3(v) if id == ATTR_HEAD.id => {
                        v[i * 4..i * 4 + 4].fill((head - anchor).into());
                    }
                    VertexAttributeValues::Float32x4(v) if id == ATTR_CORNER.id => {
                        for k in 0..4 {
                            v[i * 4 + k] = corner(k);
                        }
                    }
                    VertexAttributeValues::Float32x4(v) if id == ATTR_ALONG.id => {
                        v[i * 4..i * 4 + 4].fill([s.tail, s.head, s.hot, s.wake]);
                    }
                    VertexAttributeValues::Float32x4(v) if id == ATTR_CORE.id => {
                        let c = Vec3::from(t.core).lerp(Vec3::from(t.edge) * COOLED, s.cool);
                        v[i * 4..i * 4 + 4].fill([c.x, c.y, c.z, s.bright]);
                    }
                    VertexAttributeValues::Float32x4(v) if id == ATTR_EDGE.id => {
                        let [r, g, b] = t.edge;
                        // A wake's seed: where along the map's width it started.
                        let w = if t.kind == Kind::Wake {
                            (t.from.x * 0.37 + t.from.z * 0.61).rem_euclid(97.0)
                        } else {
                            t.px
                        };
                        v[i * 4..i * 4 + 4].fill([r, g, b, w]);
                    }
                    _ => {}
                }
            }
        }
        // Dead slots are written too, once, so the last frame's lines go.
        self.drawn = any;
    }
}

/// A slot that draws nothing: no brightness collapses its quad.
const DEAD: Span = Span {
    tail: 0.0,
    head: 0.0,
    width: 0.0,
    bright: 0.0,
    hot: 0.0,
    wake: -1.0,
    cool: 0.0,
};

/// A quad's corners: side and end, in the order its two triangles take them.
const CORNERS: [(f32, f32); 4] = [(-1.0, 0.0), (1.0, 0.0), (1.0, 1.0), (-1.0, 1.0)];

/// The span of `t` in the world, both ends in front of the near plane (the
/// one behind moved up to it), and the metres off its start each end is.
fn cut(t: &Tracer, s: Span, eye: Vec3, fwd: Vec3) -> Option<(Vec3, Vec3, Span)> {
    let dir = (t.to - t.from).normalize_or_zero();
    let (mut a, mut b) = (t.from + dir * s.tail, t.from + dir * s.head);
    let (mut s, da, db) = (s, (a - eye).dot(fwd), (b - eye).dot(fwd));
    if da < NEAR_M && db < NEAR_M {
        return None;
    }
    if da < NEAR_M {
        let k = (NEAR_M - da) / (db - da);
        a = a.lerp(b, k);
        s.tail += (s.head - s.tail) * k;
    } else if db < NEAR_M {
        let k = (NEAR_M - db) / (da - db);
        b = b.lerp(a, k);
        s.head -= (s.head - s.tail) * k;
    }
    (s.bright > 0.0).then_some((a, b, s))
}

/// The mesh: `CAP` quads, every corner collapsed until something flies.
pub fn mesh() -> Mesh {
    let n = CAP * 4;
    let mut m = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    m.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; n]);
    m.insert_attribute(ATTR_HEAD, vec![[0.0f32; 3]; n]);
    m.insert_attribute(ATTR_CORNER, vec![[0.0f32; 4]; n]);
    m.insert_attribute(ATTR_ALONG, vec![[0.0f32; 4]; n]);
    m.insert_attribute(ATTR_CORE, vec![[0.0f32; 4]; n]);
    m.insert_attribute(ATTR_EDGE, vec![[0.0f32; 4]; n]);
    let idx: Vec<u32> = (0..CAP as u32)
        .flat_map(|q| {
            let b = q * 4;
            [b, b + 1, b + 2, b, b + 2, b + 3]
        })
        .collect();
    m.insert_indices(Indices::U32(idx));
    m
}

/// Unlit, premultiplied: a line adds light (alpha 0), a wake covers.
#[derive(Asset, AsBindGroup, TypePath, Clone, Debug, Default)]
pub struct TracerMaterial {
    /// x: every line's brightness, times; the rest unused (WebGL2 wants a
    /// uniform of 16 bytes at least).
    #[uniform(0)]
    pub knobs: Vec4,
}

impl Material for TracerMaterial {
    fn vertex_shader() -> ShaderRef {
        SHADER.into()
    }

    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Premultiplied
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }

    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.vertex.buffers = vec![layout.0.get_layout(&[
            Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
            ATTR_HEAD.at_shader_location(1),
            ATTR_CORNER.at_shader_location(2),
            ATTR_ALONG.at_shader_location(3),
            ATTR_CORE.at_shader_location(4),
            ATTR_EDGE.at_shader_location(5),
        ])?];
        // The quad is spread in screen space: either way round.
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

/// The lines' entity: its mesh, anchored at the eye.
#[derive(Component)]
pub struct TracerMesh(pub Handle<Mesh>);

/// Spawned always (collapsed quads until something flies), so the pipeline
/// compiles at load and not on the first shot.
pub fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<TracerMaterial>>) {
    let mesh = meshes.add(mesh());
    let mat = mats.add(TracerMaterial {
        knobs: Vec4::new(1.0, 0.0, 0.0, 0.0),
    });
    commands.spawn((
        TracerMesh(mesh.clone()),
        Mesh3d(mesh),
        MeshMaterial3d(mat),
        Transform::IDENTITY,
        Visibility::Visible,
        NoFrustumCulling,
        NotShadowCaster,
        NotShadowReceiver,
    ));
}

/// Every frame after the camera is placed: the lines written about the eye,
/// then stepped.
#[allow(clippy::type_complexity)]
pub fn draw(
    time: Res<Time>,
    fx: Option<ResMut<Fx>>,
    cams: Query<&Transform, (With<crate::look::rig::EyeCam>, Without<TracerMesh>)>,
    mut q: Query<(&TracerMesh, &mut Transform, &mut GlobalTransform)>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let Some(mut fx) = fx else {
        return;
    };
    let dt = time.delta_secs().min(0.1);
    if let Ok(cam) = cams.single() {
        let (eye, fwd) = (cam.translation, cam.forward().as_vec3());
        for (m, mut tf, mut gt) in &mut q {
            // Sorted among the transparent draws at the eye, as the glow
            // pool is: over smoke and dust.
            if tf.translation != eye {
                tf.translation = eye;
                *gt = GlobalTransform::from_translation(eye);
            }
            if !fx.tracers.needs_write() {
                continue;
            }
            if let Some(mesh) = meshes.get_mut(&m.0) {
                fx.tracers.write(mesh, eye, fwd, eye);
            }
        }
    }
    fx.tracers.step(dt);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round(len: f32, mps: f32, dist: f32) -> Tracer {
        Tracer {
            kind: Kind::Round,
            from: Vec3::ZERO,
            to: Vec3::new(0.0, 0.0, -dist),
            mps,
            len,
            width: 0.01,
            core: [1.0; 3],
            edge: [1.0; 3],
            ..default()
        }
    }

    /// A round's first frame runs from its start; it stops at what it met
    /// and drains into it, never past it, and is gone once its tail is there.
    #[test]
    fn a_round_flies_stops_and_drains_into_what_it_met() {
        let mut t = Tracers::default();
        t.fire(round(6.0, 600.0, 30.0));
        let dt = 1.0 / 60.0;
        let mut spans = vec![];
        for _ in 0..20 {
            if let Some(s) = t.span(&t.lines[0]).filter(|_| t.lines[0].live) {
                spans.push(s);
            }
            t.step(dt);
        }
        assert_eq!(spans[0].tail, 0.0, "it leaves the muzzle");
        assert!((spans[0].head - 6.0).abs() < 1e-3, "its length: {:?}", spans[0]);
        assert!(
            spans[1].head - spans[1].tail >= 10.0 - 1e-3,
            "a frame's travel at least: {:?}",
            spans[1]
        );
        assert!(spans.iter().all(|s| s.head <= 30.0 && s.tail < s.head));
        assert!(
            spans
                .windows(2)
                .all(|w| w[1].tail >= w[0].tail && w[1].tail <= w[0].head + 1e-3),
            "no gap between frames"
        );
        let last = spans.last().unwrap();
        assert_eq!(last.head, 30.0, "it ends at the hit");
        assert_eq!(t.live(), 0, "and is gone after");
    }

    /// Cut at the eye: a line half behind the camera keeps what is in
    /// front; one wholly behind draws nothing.
    #[test]
    fn a_line_through_the_eye_is_cut_at_the_near_plane() {
        let t = round(1.0, 1.0, 10.0);
        let s = Span {
            tail: 0.0,
            head: 10.0,
            ..DEAD
        };
        let s = Span { bright: 1.0, ..s };
        let eye = Vec3::new(0.0, 0.0, -2.0);
        let (a, b, cut_s) = cut(&t, s, eye, Vec3::NEG_Z).expect("in front");
        assert!(((a - eye).dot(Vec3::NEG_Z) - NEAR_M).abs() < 1e-4);
        assert_eq!(b, Vec3::new(0.0, 0.0, -10.0));
        assert!((cut_s.tail - (2.0 + NEAR_M)).abs() < 1e-4);
        assert!(
            cut(&t, s, Vec3::new(0.0, 0.0, 5.0), Vec3::Z).is_none(),
            "behind the eye"
        );
    }
}

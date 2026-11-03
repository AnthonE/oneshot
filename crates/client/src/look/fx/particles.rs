//! A small CPU particle layer: each particle an entity with one shared mesh
//! and a pooled material, moved, grown and faded every frame. Bevy has no
//! particles of its own, and the GPU kind needs compute, which WebGL2
//! hasn't. Live particles are capped by tier; past the cap a new one is
//! dropped, so a big fight costs a bounded frame.

use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;

use super::pool::{step, Tone};
use super::Fx;

/// How a particle is shaped and turned.
#[derive(Clone, Copy)]
pub enum Shape {
    /// A soft disc that always faces the camera.
    Billboard,
    /// A soft rod along its velocity, `len` long plus `stretch` seconds of
    /// its speed: sparks, a tracer's bolt.
    Streak { len: f32, stretch: f32 },
    /// A rod held where it was put, its scale running `from` → `to`: a
    /// tracer's line, a sniper round's wake.
    Fixed { rot: Quat, from: Vec3, to: Vec3 },
    /// A disc lying on a surface with this normal: a mark, a ring.
    Flat { normal: Vec3 },
}

#[derive(Component, Clone, Copy)]
pub struct Particle {
    pub tone: Tone,
    pub shape: Shape,
    /// Metres per second.
    pub vel: Vec3,
    /// Down, metres per second squared.
    pub gravity: f32,
    /// The share of speed lost per second.
    pub drag: f32,
    /// Seconds.
    pub age: f32,
    pub life: f32,
    /// Size at birth and at death, metres.
    pub size: (f32, f32),
    /// Peak alpha.
    pub alpha: f32,
    /// The share of its life it holds full alpha before fading.
    pub hold: f32,
    /// The share of its life it takes to fade in.
    pub fade_in: f32,
    /// A mark: hidden when spent, kept for the ring to reuse.
    pub keep: bool,
    /// The alpha step it's drawn at (0: hidden).
    shown: usize,
}

impl Particle {
    pub fn new(tone: Tone, life: f32) -> Particle {
        Particle {
            tone,
            shape: Shape::Billboard,
            vel: Vec3::ZERO,
            gravity: 0.0,
            drag: 0.0,
            age: 0.0,
            life: life.max(1e-3),
            size: (0.1, 0.1),
            alpha: 1.0,
            hold: 0.0,
            fade_in: 0.0,
            keep: false,
            shown: 0,
        }
    }
    pub fn shape(mut self, s: Shape) -> Self {
        self.shape = s;
        self
    }
    pub fn vel(mut self, v: Vec3) -> Self {
        self.vel = v;
        self
    }
    pub fn gravity(mut self, g: f32) -> Self {
        self.gravity = g;
        self
    }
    pub fn drag(mut self, d: f32) -> Self {
        self.drag = d;
        self
    }
    pub fn size(mut self, a: f32, b: f32) -> Self {
        self.size = (a, b);
        self
    }
    pub fn alpha(mut self, a: f32) -> Self {
        self.alpha = a;
        self
    }
    pub fn hold(mut self, h: f32) -> Self {
        self.hold = h;
        self
    }
    pub fn fade_in(mut self, f: f32) -> Self {
        self.fade_in = f;
        self
    }

    fn alpha_at(&self, u: f32) -> f32 {
        let fade = if u <= self.hold {
            1.0
        } else {
            1.0 - (u - self.hold) / (1.0 - self.hold).max(1e-3)
        };
        let rise = if self.fade_in > 0.0 {
            (u / self.fade_in).min(1.0)
        } else {
            1.0
        };
        self.alpha * fade * rise
    }

    /// Where it sits this frame: scale and turn.
    fn place(&self, t: &mut Transform, u: f32, cam: Quat) {
        let s = self.size.0 + (self.size.1 - self.size.0) * u;
        match self.shape {
            Shape::Billboard => {
                t.rotation = cam;
                t.scale = Vec3::splat(s);
            }
            Shape::Streak { len, stretch } => {
                let speed = self.vel.length();
                if speed > 1e-3 {
                    t.rotation = Quat::from_rotation_arc(Vec3::Z, self.vel / speed);
                }
                t.scale = Vec3::new(s, s, (len + speed * stretch).max(s));
            }
            Shape::Fixed { rot, from, to } => {
                t.rotation = rot;
                t.scale = from.lerp(to, u);
            }
            Shape::Flat { normal } => {
                t.rotation = Quat::from_rotation_arc(Vec3::Z, normal);
                t.scale = Vec3::splat(s);
            }
        }
    }
}

/// Starts `p` at `at`, if the tier's cap has room. A mark (`keep`) goes on
/// the marks' ring instead, which reuses its oldest.
pub fn emit(commands: &mut Commands, fx: &mut Fx, at: Vec3, mut p: Particle) -> Option<Entity> {
    if !p.keep && fx.live >= fx.caps.particles {
        return None;
    }
    let mesh = match p.shape {
        Shape::Billboard | Shape::Flat { .. } => fx.quad.clone(),
        Shape::Streak { .. } | Shape::Fixed { .. } => fx.rod.clone(),
    };
    let mut t = Transform::from_translation(at);
    p.place(&mut t, 0.0, fx.cam);
    p.shown = step(p.alpha_at(0.0));
    let vis = if p.shown == 0 {
        Visibility::Hidden
    } else {
        Visibility::Visible
    };
    let e = commands
        .spawn((
            Mesh3d(mesh),
            MeshMaterial3d(fx.pool.get(p.tone, p.shown)),
            t,
            vis,
            NotShadowCaster,
            NotShadowReceiver,
            p,
        ))
        .id();
    if p.keep {
        fx.marks.push_back(e);
        if fx.marks.len() > fx.caps.marks {
            if let Some(old) = fx.marks.pop_front() {
                commands.entity(old).despawn();
            }
        }
    } else {
        fx.live += 1;
    }
    Some(e)
}

/// Ages, moves, grows and fades every particle; drops the spent.
#[allow(clippy::type_complexity)]
pub fn step_all(
    commands: &mut Commands,
    fx: &mut Fx,
    dt: f32,
    q: &mut Query<(
        Entity,
        &mut Particle,
        &mut Transform,
        &mut MeshMaterial3d<StandardMaterial>,
        &mut Visibility,
    )>,
) {
    let mut live = 0;
    for (e, mut p, mut t, mut m, mut vis) in q.iter_mut() {
        if p.keep && p.shown == 0 && p.age >= p.life {
            continue;
        }
        p.age += dt;
        if p.age >= p.life {
            if p.keep {
                p.shown = 0;
                *vis = Visibility::Hidden;
            } else {
                commands.entity(e).despawn();
            }
            continue;
        }
        if !p.keep {
            live += 1;
        }
        let u = p.age / p.life;
        if p.drag > 0.0 {
            let k = (1.0 - p.drag * dt).max(0.0);
            p.vel *= k;
        }
        p.vel.y -= p.gravity * dt;
        t.translation += p.vel * dt;
        p.place(&mut t, u, fx.cam);
        let s = step(p.alpha_at(u));
        if s != p.shown {
            if s == 0 {
                *vis = Visibility::Hidden;
            } else {
                if p.shown == 0 {
                    *vis = Visibility::Visible;
                }
                m.0 = fx.pool.get(p.tone, s);
            }
            p.shown = s;
        }
    }
    fx.live = live;
}

//! A skin on a gun. The skin a wallet owns (`proto::SKINS`, checked by the
//! server) trims Triple T (`bodies.rs`) and coats his guns: the gun's own
//! paint recoloured as it is drawn (`gun_skin.wgsl`), its wear, panel lines
//! and light kept, so one coat fits all fifteen with no maps of its own.
//!
//! Cosmetic as the trim is (DESIGN.md §15): nothing here glows, and a gun on
//! a pad or on the floor wears its own paint.

use std::collections::HashMap;

use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::scene::SceneInstanceReady;
use bevy::shader::ShaderRef;
use sim::arms;

pub type GunMaterial = ExtendedMaterial<StandardMaterial, Coat>;

pub const SHADER: &str = "shaders/gun_skin.wgsl";

/// The uniform; each lane is named in `gun_skin.wgsl`.
#[derive(Clone, Copy, Debug, Default, PartialEq, ShaderType)]
pub struct CoatParams {
    pub paint: Vec4,
    pub rest: Vec4,
    pub mid: Vec4,
}

#[derive(Asset, AsBindGroup, TypePath, Debug, Clone)]
pub struct Coat {
    #[uniform(100)]
    pub params: CoatParams,
}

impl MaterialExtension for Coat {
    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }
}

/// One skin's colours, sRGB: its paint's and the rest's; the paint's metal
/// and roughness.
struct Colours {
    paint: u32,
    rest: u32,
    metal: f32,
    rough: f32,
}

const fn colours(paint: u32, rest: u32, metal: f32, rough: f32) -> Option<Colours> {
    Some(Colours {
        paint,
        rest,
        metal,
        rough,
    })
}

/// By skin (`proto::SKINS` order); none for the standard look.
const COLOURS: [Option<Colours>; 6] = [
    None,
    colours(0xd9dcdf, 0x4a4e53, 0.1, 0.45),
    colours(0xb3300f, 0x241c1a, 0.15, 0.5),
    colours(0x9fd2ea, 0x39475a, 0.25, 0.4),
    colours(0x3f9a4e, 0x1e2a20, 0.2, 0.5),
    colours(0xd9a531, 0x5a4520, 0.9, 0.42),
];

/// A gun line's paint, as the shader reads it: its paint's and the rest's
/// middle (linear luma), and how far from grey a texel becomes paint.
struct Line {
    paint_mid: f32,
    rest_mid: f32,
    chroma: [f32; 2],
}

/// The human guns' olive is a faint colour on grey metal.
const FORGE: Line = Line {
    paint_mid: 0.08,
    rest_mid: 0.08,
    chroma: [0.02, 0.06],
};
/// The ship's violet is most of the gun.
const VEIL: Line = Line {
    paint_mid: 0.05,
    rest_mid: 0.1,
    chroma: [0.066, 0.13],
};

fn line(arm: u8) -> &'static Line {
    if arm >= arms::SWORD {
        &VEIL
    } else {
        &FORGE
    }
}

fn linear(hex: u32, w: f32) -> Vec4 {
    let [_, r, g, b] = hex.to_be_bytes();
    let c = Color::srgb_u8(r, g, b).to_linear();
    Vec4::new(c.red, c.green, c.blue, w)
}

fn params(c: &Colours, l: &Line) -> CoatParams {
    CoatParams {
        paint: linear(c.paint, c.metal),
        rest: linear(c.rest, c.rough),
        mid: Vec4::new(l.paint_mid, l.rest_mid, l.chroma[0], l.chroma[1]),
    }
}

/// What a gun's scene wears, read when it has spawned.
#[derive(Component, Clone, Copy)]
pub struct Worn {
    skin: u8,
    arm: u8,
}

/// Each coated material, by the gun's own and the skin.
#[derive(Resource, Default)]
pub struct Coats(HashMap<(AssetId<StandardMaterial>, u8), Handle<GunMaterial>>);

/// Puts skin `skin` on `arm`'s gun `scene`, once it has spawned. The standard
/// look is the gun as it came.
pub fn wear(commands: &mut Commands, scene: Entity, skin: u8, arm: u8) {
    if COLOURS.get(skin as usize).is_some_and(Option::is_some) {
        commands.entity(scene).insert(Worn { skin, arm }).observe(coat);
    }
}

#[allow(clippy::too_many_arguments)]
fn coat(
    ev: On<SceneInstanceReady>,
    worn: Query<&Worn>,
    children: Query<&Children>,
    plain: Query<&MeshMaterial3d<StandardMaterial>>,
    own: Res<Assets<StandardMaterial>>,
    mut coats: ResMut<Coats>,
    mut mats: ResMut<Assets<GunMaterial>>,
    mut commands: Commands,
) {
    let Ok(&Worn { skin, arm }) = worn.get(ev.entity) else {
        return;
    };
    let Some(c) = COLOURS.get(skin as usize).and_then(Option::as_ref) else {
        return;
    };
    for e in children.iter_descendants(ev.entity) {
        let Ok(m) = plain.get(e) else {
            continue;
        };
        let Some(base) = own.get(&m.0) else {
            continue;
        };
        let handle = coats
            .0
            .entry((m.0.id(), skin))
            .or_insert_with(|| {
                mats.add(ExtendedMaterial {
                    base: base.clone(),
                    extension: Coat {
                        params: params(c, line(arm)),
                    },
                })
            })
            .clone();
        commands
            .entity(e)
            .remove::<MeshMaterial3d<StandardMaterial>>()
            .insert(MeshMaterial3d(handle));
    }
}

pub fn plugin(app: &mut App) {
    app.init_resource::<Coats>()
        .add_plugins(MaterialPlugin::<GunMaterial>::default());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_skin_but_the_standard_look_has_a_coat() {
        assert_eq!(COLOURS.len(), proto::SKINS.len());
        for (i, c) in COLOURS.iter().enumerate() {
            assert_eq!(c.is_none(), i == proto::SKIN_STANDARD as usize, "{}", proto::SKINS[i]);
        }
    }
}

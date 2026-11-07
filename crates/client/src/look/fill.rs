//! The sky fill as a **hemisphere**: sky half cool blue, earth half warm,
//! baked into a small cubemap and handed to Bevy as an
//! `EnvironmentMapLight`, whose diffuse lookup is by world normal, so the
//! cube is a hemisphere light exactly. Bevy's `AmbientLight` can't be one:
//! it never reads the normal.
//!
//! Ported from Gates' `crates/client/src/render/fill.rs` (MIT, © 2026
//! MoreRight DAO): the same transfer, the same `0.5 + 0.5·n·y` blend, the
//! same sky register; the ground's albedos are Dry Fork's (sand, grass,
//! creek bed, rock) in place of Gates' island.

use bevy::asset::RenderAssetUsages;
use bevy::image::Image;
use bevy::light::light_consts::lux;
use bevy::prelude::*;
use bevy::render::render_resource::{
    Extent3d, TextureDimension, TextureFormat, TextureViewDescriptor, TextureViewDimension,
};

use super::rig::SUN_ELEVATION;

/// The cube's face size: the fill is low frequency.
pub const FILL_FACE: u32 = 32;

/// The sky's chroma at the zenith, sRGB, and how bright it fills.
pub const SKY_FILL_SRGB: [f32; 3] = [0.80, 0.85, 0.95];
pub const SKY_FILL_SCALE: f32 = 1.8;
pub const SKY_FILL_LUX: f32 = lux::AMBIENT_DAYLIGHT * SKY_FILL_SCALE;

/// The ground the fill bounces off, linear albedo: sand, grass, the creek
/// bed's dust, rock (Gates' four identities, which Dry Fork shares), and
/// how much of the map each covers.
pub const GROUND_ALBEDO: [[f32; 3]; 4] = [
    [0.1895, 0.1775, 0.1513],
    [0.0482, 0.0648, 0.0176],
    [0.117, 0.0885, 0.0343],
    [0.2850, 0.1342, 0.0823],
];
pub const GROUND_MIX: [f32; 4] = [0.45, 0.25, 0.1, 0.2];

/// sRGB → linear, the exact piecewise transfer.
pub fn srgb_to_linear(v: f32) -> f32 {
    if v <= 0.040_45 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

/// linear → sRGB, the exact piecewise transfer.
pub fn linear_to_srgb(v: f32) -> f32 {
    if v <= 0.003_130_8 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}

/// Rec.709 luminance of a linear triple.
pub fn luminance(c: [f32; 3]) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

/// The sky half: irradiance on a zenith-facing surface, linear lux.
pub fn sky_lux() -> [f32; 3] {
    SKY_FILL_SRGB.map(|c| srgb_to_linear(c) * SKY_FILL_LUX)
}

fn bounce_albedo() -> [f32; 3] {
    let mut a = [0.0f32; 3];
    for (i, w) in GROUND_MIX.iter().enumerate() {
        for (c, a_c) in a.iter_mut().enumerate() {
            *a_c += GROUND_ALBEDO[i][c] * w;
        }
    }
    a
}

/// The earth half: what the ground reflects of the sun on it (its elevation
/// above the horizon, so the sine) and the sky.
pub fn bounce_lux() -> [f32; 3] {
    let e = lux::DIRECT_SUNLIGHT * SUN_ELEVATION.sin() + SKY_FILL_LUX;
    bounce_albedo().map(|a| a * e)
}

/// Irradiance arriving at a surface of world normal `n`, linear lux.
pub fn fill_at(n: Vec3) -> [f32; 3] {
    let w = 0.5 + 0.5 * n.y.clamp(-1.0, 1.0);
    let (sky, ground) = (sky_lux(), bounce_lux());
    [0, 1, 2].map(|c| ground[c] + (sky[c] - ground[c]) * w)
}

/// The brightest the fill gets: the cube holds `fill / peak` and the
/// light's intensity carries `peak` back.
pub fn peak_lux() -> f32 {
    sky_lux()
        .iter()
        .chain(bounce_lux().iter())
        .fold(0.0f32, |m, v| m.max(*v))
}

/// A cube face's texel as a direction (Bevy's face order and handedness).
pub fn cube_dir(face: usize, x: u32, y: u32, n: u32) -> Vec3 {
    let uc = 2.0 * (x as f32 + 0.5) / n as f32 - 1.0;
    let vc = 2.0 * (y as f32 + 0.5) / n as f32 - 1.0;
    let d = match face {
        0 => Vec3::new(1.0, -vc, -uc),
        1 => Vec3::new(-1.0, -vc, uc),
        2 => Vec3::new(uc, 1.0, vc),
        3 => Vec3::new(uc, -1.0, -vc),
        4 => Vec3::new(uc, -vc, 1.0),
        _ => Vec3::new(-uc, -vc, -1.0),
    };
    Vec3::new(d.x, d.y, -d.z).normalize()
}

/// The hemisphere, baked.
pub fn cubemap(n: u32) -> Image {
    let peak = peak_lux();
    let mut data = vec![0u8; (n * n * 6 * 4) as usize];
    for face in 0..6usize {
        for y in 0..n {
            for x in 0..n {
                let f = fill_at(cube_dir(face, x, y, n));
                let i = (((face as u32 * n + y) * n + x) * 4) as usize;
                for c in 0..3 {
                    data[i + c] = (linear_to_srgb((f[c] / peak).clamp(0.0, 1.0)) * 255.0).round() as u8;
                }
                data[i + 3] = 255;
            }
        }
    }
    cube_image(n, data)
}

/// Six faces of `n`² sRGB texels, as a cube.
fn cube_image(n: u32, data: Vec<u8>) -> Image {
    let mut image = Image::new(
        Extent3d {
            width: n,
            height: n,
            depth_or_array_layers: 6,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::Cube),
        ..default()
    });
    image
}

/// A hall's fill (a roofed map: `LightDef::sun` 0), in the hull's
/// colours: its walls a dark purple, its deck darker, the vault over it
/// lit by bands of the windows' gold, a ring of the strips' cyan round it
/// at their height. A hull is metal, and metal shows what it reflects: the
/// sky's fill made the hold a ship of lavender plastic. The diffuse cube is
/// the same hall blurred to its hemisphere (the bands averaged in); the
/// specular one keeps the bands, so a curved plate carries them.
pub mod hall {
    use super::*;

    /// How much brighter than the sky's a hall's fill is at the map's own
    /// `LightDef::fill`: the hall's cube is darker than the sky's.
    pub const GAIN: f32 = 2.0;
    /// The face size: the bands want more than the sky's 32.
    pub const FACE: u32 = 64;
    /// A roofed map's light as a cube: linear, as a share of the fill's peak.
    /// **(knobs)**
    pub struct Palette {
        /// The walls, the deck, the vault between its panes, the panes, the
        /// strips.
        pub wall: [f32; 3],
        pub deck: [f32; 3],
        pub vault: [f32; 3],
        pub pane: [f32; 3],
        pub strip: [f32; 3],
        /// Where the strips run: the sine of their elevation from the floor's
        /// middle, and half their width.
        pub strip_at: f32,
        pub strip_half: f32,
        /// The panes: bands across the hall's axis (x), this many a radian of
        /// bearing along it, and the share of each band that is glass.
        pub pane_freq: f32,
        pub pane_share: f32,
        /// What metal reflects: its walls and vault darker than what they fill
        /// a fighter with (`diffuse_at` keeps those), and a long soft highlight
        /// low in the hall, where a round form catches a lit wall.
        pub spec_wall: [f32; 3],
        pub spec_vault: [f32; 3],
        pub sheen: [f32; 3],
        pub sheen_share: f32,
        /// Its air (`rig::hall_haze`): a faint haze this colour, this much a
        /// metre squared, so the near wall is clear and the far end softens.
        pub haze: [f32; 3],
        pub haze_density: f32,
    }

    /// The ship's purple hold.
    pub const SHIP: Palette = Palette {
        wall: [0.16, 0.09, 0.3],
        deck: [0.035, 0.025, 0.055],
        vault: [0.1, 0.06, 0.2],
        pane: [1.0, 0.55, 0.16],
        strip: [0.18, 0.5, 0.55],
        strip_at: 0.42,
        strip_half: 0.03,
        pane_freq: 3.0,
        pane_share: 0.35,
        spec_wall: [0.08, 0.045, 0.15],
        spec_vault: [0.04, 0.025, 0.08],
        sheen: [0.9, 0.55, 0.95],
        sheen_share: 0.55,
        haze: [0.035, 0.02, 0.06],
        haze_density: 0.014,
    };

    /// Cistern's dark water and old brick: cold day from its oculi, amber low
    /// from its lamps.
    pub const CISTERN: Palette = Palette {
        wall: [0.1, 0.075, 0.06],
        deck: [0.012, 0.018, 0.02],
        vault: [0.05, 0.04, 0.036],
        pane: [0.72, 0.88, 1.0],
        strip: [0.42, 0.33, 0.22],
        strip_at: 0.16,
        strip_half: 0.05,
        pane_freq: 2.0,
        pane_share: 0.1,
        spec_wall: [0.045, 0.035, 0.03],
        spec_vault: [0.02, 0.022, 0.026],
        sheen: [0.56, 0.47, 0.36],
        sheen_share: 0.4,
        haze: [0.014, 0.02, 0.024],
        haze_density: 0.012,
    };

    /// By `Mood::hall`.
    pub const PALETTES: [&Palette; 2] = [&SHIP, &CISTERN];

    /// The specular cube's mips: how far a texel's look spreads in each,
    /// as the tangent of the angle. The renderer picks a mip by roughness,
    /// so without them a rough deck mirrors the strips as sharply as chrome.
    pub const SPREAD: [f32; 7] = [0.0, 0.05, 0.1, 0.2, 0.35, 0.6, 1.0];
    /// Looks averaged per texel in a blurred mip.
    const SPREAD_LOOKS: usize = 16;

    fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
        [0, 1, 2].map(|c| a[c] + (b[c] - a[c]) * t)
    }

    fn smooth(e0: f32, e1: f32, x: f32) -> f32 {
        let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }

    /// What the hall shows toward `d`.
    pub fn specular_at(p: &Palette, d: Vec3) -> [f32; 3] {
        let y = d.y;
        // The deck under the horizon, the wall to the vault's spring.
        let mut c = mix(p.deck, p.spec_wall, smooth(-0.25, 0.05, y));
        c = mix(c, p.sheen, p.sheen_share * smooth(0.02, 0.1, y) * smooth(0.3, 0.18, y));
        // The vault, darker than the wall, and its panes: bands across the
        // hall by the bearing along it.
        let over = smooth(0.5, 0.7, y);
        let along = d.x.atan2(y.max(1e-3));
        let band = (along * p.pane_freq / core::f32::consts::TAU).rem_euclid(1.0);
        let pane = smooth(p.pane_share + 0.05, p.pane_share, band) * smooth(0.6, 0.8, y);
        c = mix(c, mix(p.spec_vault, p.pane, pane), over);
        // The strips round the hall.
        let strip = smooth(p.strip_half, 0.0, (y - p.strip_at).abs());
        mix(c, p.strip, strip)
    }

    /// The hall blurred to its hemisphere: the bands as their average.
    pub fn diffuse_at(p: &Palette, d: Vec3) -> [f32; 3] {
        let up = mix(p.vault, p.pane, p.pane_share * 0.5);
        let side = mix(p.wall, p.strip, 0.15);
        let y = d.y.clamp(-1.0, 1.0);
        if y >= 0.0 {
            mix(side, up, y)
        } else {
            mix(side, p.deck, -y)
        }
    }

    fn cube(n: u32, at: impl Fn(Vec3) -> [f32; 3]) -> Image {
        let mut data = vec![0u8; (n * n * 6 * 4) as usize];
        for face in 0..6usize {
            for y in 0..n {
                for x in 0..n {
                    let f = at(cube_dir(face, x, y, n));
                    let i = (((face as u32 * n + y) * n + x) * 4) as usize;
                    for c in 0..3 {
                        data[i + c] = (linear_to_srgb(f[c].clamp(0.0, 1.0)) * 255.0).round() as u8;
                    }
                    data[i + 3] = 255;
                }
            }
        }
        cube_image(n, data)
    }

    /// `specular_at` with its mips, each a wider blur of the hall, a cube's
    /// six faces each with its whole chain.
    fn specular_cube(p: &Palette) -> Image {
        let mut data = Vec::new();
        for face in 0..6usize {
            for (m, &spread) in SPREAD.iter().enumerate() {
                let n = (FACE >> m).max(1);
                for y in 0..n {
                    for x in 0..n {
                        let d = cube_dir(face, x, y, n);
                        let f = if spread == 0.0 {
                            specular_at(p, d)
                        } else {
                            // A fixed spiral of looks round `d`, enough to
                            // smear the bands and strips evenly.
                            let t = d.any_orthonormal_vector();
                            let b = d.cross(t);
                            let mut sum = [0.0f32; 3];
                            for k in 0..SPREAD_LOOKS {
                                let a = k as f32 * 2.399_963;
                                let r = spread * ((k as f32 + 0.5) / SPREAD_LOOKS as f32).sqrt();
                                let s = specular_at(p, (d + (t * a.cos() + b * a.sin()) * r).normalize());
                                for c in 0..3 {
                                    sum[c] += s[c] / SPREAD_LOOKS as f32;
                                }
                            }
                            sum
                        };
                        for v in f {
                            data.push((linear_to_srgb(v.clamp(0.0, 1.0)) * 255.0).round() as u8);
                        }
                        data.push(255);
                    }
                }
            }
        }
        let mut image = cube_image(FACE, vec![0; (FACE * FACE * 6 * 4) as usize]);
        image.texture_descriptor.mip_level_count = SPREAD.len() as u32;
        image.data = Some(data);
        image
    }

    /// The hall's two cubes: diffuse, specular.
    pub fn cubemaps(p: &Palette) -> (Image, Image) {
        (cube(16, |d| diffuse_at(p, d)), specular_cube(p))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_transfer_round_trips_and_the_halves_are_where_they_belong() {
        for k in 0..=100 {
            let v = k as f32 / 100.0;
            assert!((srgb_to_linear(linear_to_srgb(v)) - v).abs() < 1e-5);
        }
        // Up is the sky's, down the ground's, and the sky is brighter.
        assert_eq!(fill_at(Vec3::Y), sky_lux());
        assert_eq!(fill_at(Vec3::NEG_Y), bounce_lux());
        assert!(luminance(sky_lux()) > luminance(bounce_lux()));
        // The cube's +Y face looks up.
        assert!(cube_dir(2, 16, 16, 32).y > 0.99);
    }
}

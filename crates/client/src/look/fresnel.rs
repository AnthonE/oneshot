//! The one place a `reflectance` is decided, and the arithmetic behind it.
//! After Gates' `crates/client/src/render/fresnel.rs` (MIT, NOTICE).
//!
//! **Bevy's `reflectance` is not a 0..1 "how shiny" slider.** It is a
//! remap: `F0 = 0.16 · reflectance²` for the dielectric part of a surface,
//! so its DEFAULT of 0.5 is the ordinary dielectric's 4%, and anything
//! below it is darker than any real dielectric. Gates authored its whole
//! island against the slider reading and shipped every surface 4 to 70
//! times under (a ground at 0.18 is F0 0.5%); the specular lobe its
//! roughness maps shaped had no energy in it.
//!
//! Here a site that writes a `reflectance` names the surface it claims to
//! be with one of these constants, and `tests` hold each to the F0 it says
//! and every level look's material inside the band a real dielectric
//! occupies. A metal's F0 is its base colour (`metallic`), not this.

/// Bevy's mapping from `StandardMaterial::reflectance` to the dielectric
/// normal-incidence specular it delivers: `bevy_pbr`'s `calculate_F0` is
/// `0.16 · reflectance² · (1 − metallic) + base_color · metallic`, and this
/// is its first half.
pub fn f0_of(reflectance: f32) -> f32 {
    0.16 * reflectance * reflectance
}

/// The inverse: the `reflectance` to author for a wanted F0.
pub fn reflectance_for(f0: f32) -> f32 {
    (f0 / 0.16).sqrt()
}

/// Ordinary dielectrics: concrete, stone, rock, paint, rubber, soil, cloth,
/// glass. 4% is what every real-time PBR reference converges on (Lagarde &
/// de Rousiers, *Moving Frostbite to PBR* §3.2; Filament), and it is Bevy's
/// own default: the right value for almost everything is not writing the
/// field at all, and a site that writes it says this.
pub const DIELECTRIC: f32 = 0.5;

/// A puff of smoke or dust (`fx`'s soft sprites): a billboard standing for
/// a cloud of motes, with no interface for a specular lobe to come off. A
/// sheen on it would be the quad's, not the dust's, so it has none. The one
/// value under the band, and only for what is not a surface.
pub const NO_SURFACE: f32 = 0.0;

/// The band a dielectric surface's F0 sits in: water's 2% to a dense
/// mineral's 5–6%, with the floor a little under water.
pub const F0_MIN: f32 = 0.015;
pub const F0_MAX: f32 = 0.06;

/// What a material delivers at normal incidence, Bevy's way: the dielectric
/// part from `reflectance`, the metal's from its base colour's luminance.
pub fn f0_material(reflectance: f32, metallic: f32, base_luminance: f32) -> f32 {
    f0_of(reflectance) * (1.0 - metallic) + base_luminance * metallic
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::*;

    /// The constants say what they claim, and the dielectric is Bevy's
    /// own default.
    #[test]
    fn the_constants_encode_the_f0_they_name() {
        for (name, refl, f0) in [("DIELECTRIC", DIELECTRIC, 0.04f32), ("NO_SURFACE", NO_SURFACE, 0.0)] {
            assert!(
                (reflectance_for(f0) - refl).abs() < 1e-5,
                "{name}: {refl} is not F0 {f0}"
            );
            assert!((f0_of(refl) - f0).abs() < 1e-6, "{name} round-trips to {}", f0_of(refl));
        }
        assert!(
            (DIELECTRIC - StandardMaterial::default().reflectance).abs() < 1e-6,
            "fresnel::DIELECTRIC has drifted from Bevy's default"
        );
        assert!((F0_MIN..=F0_MAX).contains(&f0_of(DIELECTRIC)));
    }

    /// Every look the level dresses a box in, and the ground, deliver a
    /// real surface's specular: the dielectric part in the band, and no
    /// look's mix of metal and dielectric darker than its floor.
    #[test]
    fn every_level_material_is_a_real_surface() {
        for look in 0..=255u8 {
            let Some(m) = crate::level::box_material(look) else {
                continue;
            };
            let f0 = f0_of(m.reflectance);
            assert!(
                (F0_MIN..=F0_MAX).contains(&f0),
                "look {look}: reflectance {} is F0 {:.2}%, outside a dielectric's {:.1}%..{:.1}%",
                m.reflectance,
                f0 * 100.0,
                F0_MIN * 100.0,
                F0_MAX * 100.0
            );
            let lum = LinearRgba::from(m.base_color).luminance();
            assert!(
                f0_material(m.reflectance, m.metallic, lum) >= F0_MIN.min(lum),
                "look {look} delivers less specular than either of its halves"
            );
        }
        let g = super::super::ground::material(Handle::default(), Handle::default(), default());
        assert!((F0_MIN..=F0_MAX).contains(&f0_of(g.base.reflectance)));
    }
}

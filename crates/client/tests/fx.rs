//! Gate: what a blow throws, and that the particle pools stay inside their
//! bounds. Arithmetic on the pools and the table: no world, no GPU. The
//! pool's own laws (a spark's width, its bounce, the sort) are the shared
//! crate's, tested in `crates/fx/tests`; these are ours.
//! After Gates' `crates/client/tests/fx.rs` (MIT, © 2026 MoreRight DAO).

use bevy::math::Vec3;
use bevy::mesh::{Mesh, VertexAttributeValues};
use oneshot_client::look::fx::marks::{kind, Kind};
use oneshot_client::look::fx::pool::{pool_mesh, Cam, MeshWrite, Pool};
use oneshot_client::look::fx::table::{effect, emit, lod, scaled, Layer, Matter, Weapon};

const WEAPONS: [Weapon; 4] = [Weapon::Bullet, Weapon::Pellet, Weapon::Sniper, Weapon::Melee];
const SOLID: [Matter; 3] = [Matter::Concrete, Matter::Metal, Matter::Dirt];

/// A firefight cannot grow a pool, and a full pool recycles its oldest
/// rather than dropping the newest blow.
#[test]
fn a_pool_is_bounded_and_recycles() {
    let mut p = Pool::new(64, false);
    for _ in 0..40 {
        emit(&mut p, Layer::Sparks, 12, Vec3::ZERO, Vec3::Y, Matter::Metal);
    }
    assert_eq!(p.live(), 64);
    assert_eq!(p.spawned, 480);
    assert_eq!(p.stolen, 480 - 64, "every overflow is counted");
    p.clear();
    assert_eq!(p.live(), 0);
}

/// The ground never sparks; metal always does; a shield crackles and never
/// bleeds or dusts; health mists; bodies and hulls take no mark.
#[test]
fn each_matter_throws_its_own() {
    for w in WEAPONS {
        assert_eq!(effect(w, Matter::Dirt).count(Layer::Sparks), 0, "{w:?} sparks off dirt");
        assert!(effect(w, Matter::Metal).count(Layer::Sparks) > 0, "{w:?} on metal");
        let shield = effect(w, Matter::Shield);
        assert!(
            shield.count(Layer::Arcs) > 0 && shield.count(Layer::Zap) > 0,
            "{w:?} on a shield"
        );
        assert_eq!(shield.count(Layer::Hurt) + shield.count(Layer::Dust), 0);
        assert!(effect(w, Matter::Flesh).count(Layer::Hurt) > 0);
        for m in [Matter::Shield, Matter::Flesh, Matter::Hull] {
            assert!(kind(w, m).is_none(), "{w:?} marks {m:?}");
        }
    }
    for m in SOLID {
        assert!(kind(Weapon::Bullet, m).is_some(), "a bullet leaves no mark on {m:?}");
        assert_eq!(kind(Weapon::Blast, m).map(|k| k.0), Some(Kind::Scorch));
    }
    // Metal showers; concrete only flashes a few; a sniper round more.
    assert!(
        effect(Weapon::Bullet, Matter::Metal).count(Layer::Sparks)
            > effect(Weapon::Bullet, Matter::Concrete).count(Layer::Sparks)
    );
    assert!(
        effect(Weapon::Sniper, Matter::Concrete).count(Layer::Grit)
            > effect(Weapon::Bullet, Matter::Concrete).count(Layer::Grit)
    );
}

/// Less with distance, nothing past 100 m, and a layer in range is never
/// rounded to nothing.
#[test]
fn particles_thin_with_distance() {
    let mut last = f32::MAX;
    for d in [0.0, 10.0, 30.0, 60.0, 99.0, 101.0, 500.0] {
        let l = lod(d);
        assert!(l <= last, "lod rose with distance at {d} m");
        last = l;
    }
    assert_eq!(lod(101.0), 0.0);
    assert_eq!(scaled(1, lod(90.0)), 1);
    assert_eq!(scaled(12, 1.0), 12);
    assert_eq!(scaled(0, 1.0), 0);
}

/// The mesh is written in place: live particles are quads, dead ones are
/// collapsed, and an empty pool writes once more to clear and then stops.
#[test]
fn the_mesh_collapses_the_dead_and_goes_quiet() {
    let mut mesh: Mesh = pool_mesh(16);
    let mut p = Pool::new(16, true);
    let eye = Vec3::new(0.0, 1.6, 5.0);
    let cam = Cam {
        pos: eye.into(),
        right: Vec3::X.into(),
        up: Vec3::Y.into(),
    };
    emit(&mut p, Layer::Dust, 3, Vec3::ZERO, Vec3::Y, Matter::Dirt);
    assert!(p.needs_write());
    assert!(p.write(&mut mesh, &cam, eye));
    let positions = |m: &Mesh| match m.attribute(Mesh::ATTRIBUTE_POSITION) {
        Some(VertexAttributeValues::Float32x3(v)) => v.clone(),
        _ => panic!("no positions"),
    };
    let quads = positions(&mesh)
        .chunks(4)
        .filter(|q| q.iter().any(|v| *v != [0.0; 3]))
        .count();
    assert_eq!(quads, 3);
    for _ in 0..200 {
        p.step(0.05);
    }
    assert_eq!(p.live(), 0);
    assert!(p.needs_write(), "the dead must be cleared once");
    assert!(p.write(&mut mesh, &cam, eye));
    assert!(positions(&mesh).iter().all(|v| *v == [0.0; 3]));
    assert!(!p.needs_write(), "and then the mesh is left alone");
}

/// Our sprite sheet's level 0, pinned: it moved onto the shared `fx` crate's
/// shapes byte for byte, and a change to a sprite is a change to every
/// effect that draws it (look at the strips, `tools/fx.sh`, then repin).
#[test]
fn the_sprite_sheet_is_unchanged() {
    let (px, w, h) = oneshot_client::look::fx::atlas::sprite_pixels();
    assert_eq!((w, h), (256, 256));
    let mut d: u64 = 0xcbf2_9ce4_8422_2325;
    for b in px {
        d = (d ^ b as u64).wrapping_mul(0x100_0000_01b3);
    }
    assert_eq!(d, 0x8cd7_5382_b029_d718, "sprite sheet digest {d:#018x}");
}

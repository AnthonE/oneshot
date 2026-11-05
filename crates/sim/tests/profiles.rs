//! Profile solids (sim::profile): rays meet them where they are, as a
//! march along the ray finds them, and the simple ones answer as the
//! shapes they look like.

use sim::fmath::fabs;
use sim::geom::V3;
use sim::profile::Piece;
use sim::rng::Pcg32;
use sim::solid::Solid;

fn unit(r: &mut Pcg32, lo: f32, hi: f32) -> f32 {
    lo + (hi - lo) * (r.next_u32() >> 8) as f32 * (1.0 / 16_777_216.0)
}

const DOME: &[[f32; 3]] = &[[0.0, 0.0, 1.6], [1.0, 0.0, 1.45], [2.0, 0.0, 0.9], [2.6, 0.0, 0.0]];
const BULB: &[[f32; 3]] = &[
    [0.0, 0.0, 2.4],
    [0.6, 0.0, 2.3],
    [0.6, 0.35, 2.3],
    [1.0, 0.6, 2.0],
    [1.2, 1.2, 1.4],
];
const TORUS: &[[f32; 3]] = &[
    [2.0, 0.0, 0.0],
    [2.1, -0.28, 0.28],
    [2.3, -0.4, 0.4],
    [2.5, -0.28, 0.28],
    [2.6, 0.0, 0.0],
];
const WALL: &[[f32; 3]] = &[
    [3.6, 1.2, 1.2],
    [3.8, 0.6, 1.8],
    [3.8, 0.0, 3.0],
    [4.2, 0.0, 3.0],
    [4.5, 1.0, 2.0],
];
const VAULT: &[[f32; 3]] = &[
    [-3.0, 3.0, 5.0],
    [-2.0, 3.8, 5.0],
    [0.0, 4.2, 5.0],
    [2.0, 3.8, 5.0],
    [3.0, 3.0, 5.0],
];
const ARCH: &[[f32; 3]] = &[
    [-2.0, 0.0, 3.0],
    [-1.2, 0.0, 3.0],
    [-1.2, 2.0, 3.0],
    [0.0, 2.6, 3.0],
    [1.2, 2.0, 3.0],
    [1.2, 0.0, 3.0],
    [2.0, 0.0, 3.0],
];

/// One of each kind: spun whole, spun in sectors, carried along and across.
fn solids() -> Vec<Solid> {
    let (s, c) = (0.6f32, 0.8f32);
    vec![
        Solid::profile(3, [1.0, -2.0, 0.5, 0.0, 0.0, 0.0, 0.0, 0.0], DOME),
        Solid::profile(3, [-1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0], BULB),
        // A quarter, and a half turn.
        Solid::profile(4, [0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 0.0], TORUS),
        Solid::profile(4, [0.5, 0.5, 0.0, c, s, -c, -s, 0.0], WALL),
        Solid::profile(5, [0.0, 0.0, 0.0, 1.0, 0.0, 4.0, 0.0, 0.0], VAULT),
        Solid::profile(5, [2.0, -1.0, 0.0, c, s, 0.4, 0.0, 0.0], ARCH),
    ]
}

/// Whether anything within 3 mm of `p` is inside.
fn near(s: &Solid, p: V3) -> bool {
    let e = 0.003;
    for i in [-e, 0.0, e] {
        for j in [-e, 0.0, e] {
            for k in [-e, 0.0, e] {
                if s.contains(p + V3::new(i, j, k)) {
                    return true;
                }
            }
        }
    }
    false
}

/// The first point of the ray inside, by steps of 5 mm then halving; and
/// how far it stays inside from there (to the step).
fn march(s: &Solid, o: V3, d: V3, tmax: f32) -> Option<(f32, f32)> {
    let step = 0.005;
    let n = (tmax / step) as i32;
    for i in 0..=n {
        let t = (i as f32 * step).min(tmax);
        if s.contains(o + d * t) {
            let (mut a, mut b) = ((t - step).max(0.0), t);
            if i == 0 {
                return Some((0.0, run(s, o, d, 0.0, tmax)));
            }
            for _ in 0..20 {
                let m = (a + b) * 0.5;
                if s.contains(o + d * m) {
                    b = m;
                } else {
                    a = m;
                }
            }
            return Some((b, run(s, o, d, t, tmax)));
        }
    }
    None
}

fn run(s: &Solid, o: V3, d: V3, from: f32, tmax: f32) -> f32 {
    let mut t = from;
    while t < tmax && s.contains(o + d * t) {
        t += 0.005;
    }
    t - from
}

#[test]
fn a_ray_enters_where_a_march_first_finds_it_inside() {
    let mut r = Pcg32::new(5, 9);
    for (i, s) in solids().iter().enumerate() {
        let b = s.bounds();
        let (lo, hi) = (b.min + V3::new(-2.0, -1.0, -2.0), b.max + V3::new(2.0, 1.0, 2.0));
        let mut hits = 0;
        for k in 0..1500 {
            let o = V3::new(
                unit(&mut r, lo.x, hi.x),
                unit(&mut r, lo.y, hi.y),
                unit(&mut r, lo.z, hi.z),
            );
            // Half aimed into the box, half anywhere.
            let d = if k % 2 == 0 {
                let q = V3::new(
                    unit(&mut r, b.min.x, b.max.x),
                    unit(&mut r, b.min.y, b.max.y),
                    unit(&mut r, b.min.z, b.max.z),
                );
                q - o
            } else {
                V3::new(
                    unit(&mut r, -1.0, 1.0),
                    unit(&mut r, -1.0, 1.0),
                    unit(&mut r, -1.0, 1.0),
                )
            };
            if d.len() < 1e-3 {
                continue;
            }
            let d = d.norm();
            let tmax = 14.0;
            let got = s.ray_hit(o, d, tmax);
            let want = march(s, o, d, tmax);
            macro_rules! what {
                () => {
                    format_args!("solid {i}: {o:?} {d:?}: ray {got:?}, march {want:?}")
                };
            }
            match (got, want) {
                (None, None) => {}
                // A sliver the ray turned away is a graze.
                (None, Some((_, len))) => assert!(len <= 0.01, "{}", what!()),
                (Some((t, _)), None) => assert!(near(s, o + d * t), "{}", what!()),
                (Some((t, piece)), Some((tm, len))) => {
                    hits += 1;
                    if fabs(t - tm) > 0.003 {
                        // Earlier: a graze the march stepped over; later:
                        // the march's own graze.
                        assert!((t < tm && near(s, o + d * t)) || (t > tm && len <= 0.01), "{}", what!());
                    }
                    if t > 0.0 && piece != Piece::Inside {
                        let n = s.normal(piece, o + d * t);
                        assert!(fabs(n.len() - 1.0) < 1e-3, "{}: {n:?}", what!());
                        assert!(n.dot(d) < 0.05, "{}: it faces the ray: {piece:?} {n:?}", what!());
                    }
                }
            }
            // The quick look never turns away what the exact test meets.
            assert_eq!(
                s.ray(o, d, tmax).map(f32::to_bits),
                s.ray_exact(o, d, tmax).map(f32::to_bits),
                "{}",
                what!()
            );
        }
        assert!(hits > 300, "solid {i}: {hits} hits");
    }
}

#[test]
fn a_one_segment_lathe_is_a_round_pillar() {
    let lathe = Solid::profile(
        3,
        [2.0, 3.0, 0.5, 0.0, 0.0, 0.0, 0.0, 0.0],
        &[[0.0, 0.0, 2.5], [1.2, 0.0, 2.5]],
    );
    let round = Solid::from_def(0, [2.0, 3.0, 1.2, 0.0, 0.0, 0.0, 0.5, 3.0]);
    assert_eq!((lathe.y0, lathe.y1), (round.y0, round.y1));
    let mut r = Pcg32::new(8, 1);
    for _ in 0..4000 {
        let o = V3::new(
            unit(&mut r, -4.0, 8.0),
            unit(&mut r, -1.0, 5.0),
            unit(&mut r, -3.0, 9.0),
        );
        let d = V3::new(
            unit(&mut r, -1.0, 1.0),
            unit(&mut r, -1.0, 1.0),
            unit(&mut r, -1.0, 1.0),
        )
        .norm();
        let (a, b) = (lathe.ray(o, d, 20.0), round.ray(o, d, 20.0));
        match (a, b) {
            (Some(a), Some(b)) => assert!(fabs(a - b) < 1e-3, "{o:?} {d:?}: {a} {b}"),
            (None, None) => {}
            _ => {
                // Only a graze of an edge may differ.
                let t = a.or(b).unwrap();
                let p = o + d * t;
                let rr = ((p.x - 2.0) * (p.x - 2.0) + (p.z - 3.0) * (p.z - 3.0)).sqrt();
                assert!(
                    fabs(rr - 1.2) < 1e-2 || fabs(p.y - 0.5) < 1e-2 || fabs(p.y - 3.0) < 1e-2,
                    "{o:?} {d:?}: {a:?} {b:?}"
                );
            }
        }
        let (x, z) = (o.x, o.z);
        assert_eq!(lathe.over(x, z, 0.35), round.over(x, z, 0.35), "{x} {z}");
    }
}

#[test]
fn a_flat_sweep_is_a_turned_box() {
    let h = core::f32::consts::FRAC_1_SQRT_2;
    let sweep = Solid::profile(
        5,
        [1.0, -1.0, 0.0, h, h, 2.0, 0.0, 0.0],
        &[[-0.5, 0.0, 3.0], [0.5, 0.0, 3.0]],
    );
    let turned = Solid::from_def(1, [1.0, -1.0, 2.0, 0.5, h, h, 0.0, 3.0]);
    let mut r = Pcg32::new(3, 4);
    for _ in 0..4000 {
        let o = V3::new(
            unit(&mut r, -5.0, 7.0),
            unit(&mut r, -1.0, 4.0),
            unit(&mut r, -7.0, 5.0),
        );
        let d = V3::new(
            unit(&mut r, -1.0, 1.0),
            unit(&mut r, -1.0, 1.0),
            unit(&mut r, -1.0, 1.0),
        )
        .norm();
        let (a, b) = (sweep.ray(o, d, 20.0), turned.ray(o, d, 20.0));
        match (a, b) {
            (Some(a), Some(b)) => assert!(fabs(a - b) < 1e-4, "{o:?} {d:?}: {a} {b}"),
            (None, None) => {}
            _ => panic!("{o:?} {d:?}: {a:?} {b:?}"),
        }
    }
}

#[test]
fn a_ray_through_the_apex_never_meets_the_mirror_cone() {
    // A cone to a point 2 m up; its mirror opens upward from the point.
    let cone = Solid::profile(
        3,
        [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        &[[0.0, 0.0, 2.0], [2.0, 0.0, 0.0]],
    );
    // Level at 3 m, straight over the point: through the mirror only.
    assert_eq!(cone.ray(V3::new(-5.0, 3.0, 0.0), V3::new(1.0, 0.0, 0.0), 10.0), None);
    // Down from over the point, through the mirror (at x = 0.5) to the
    // cone's slope at (1, 1).
    let o = V3::new(0.0, 4.0, 0.0);
    let d = V3::new(1.0, -3.0, 0.0).norm();
    let t = cone.ray(o, d, 10.0).unwrap();
    let p = o + d * t;
    assert!(fabs(p.x - 1.0) < 1e-3 && fabs(p.y - 1.0) < 1e-3, "{p:?}");
}

#[test]
fn straight_down_onto_a_lathe() {
    let s = Solid::profile(3, [1.0, -2.0, 0.5, 0.0, 0.0, 0.0, 0.0, 0.0], DOME);
    for rr in [0.0, 0.5, 1.5, 2.3] {
        let t = s
            .ray(V3::new(1.0 + rr, 10.0, -2.0), V3::new(0.0, -1.0, 0.0), 20.0)
            .unwrap();
        assert!(fabs((10.0 - t) - s.top_at(1.0 + rr, -2.0)) < 1e-4, "{rr}: {t}");
    }
    assert_eq!(s.ray(V3::new(4.0, 10.0, -2.0), V3::new(0.0, -1.0, 0.0), 20.0), None);
}

#[test]
fn a_far_ray_meets_a_lathe_where_a_near_one_does() {
    let s = Solid::profile(3, [1.0, -2.0, 0.5, 0.0, 0.0, 0.0, 0.0, 0.0], DOME);
    let mut r = Pcg32::new(2, 2);
    for _ in 0..500 {
        let at = V3::new(
            1.0 + unit(&mut r, -1.5, 1.5),
            unit(&mut r, 0.5, 1.6),
            -2.0 + unit(&mut r, -1.5, 1.5),
        );
        let d = V3::new(
            unit(&mut r, -1.0, 1.0),
            unit(&mut r, -0.3, 0.1),
            unit(&mut r, -1.0, 1.0),
        )
        .norm();
        let (near, far) = (at - d * 5.0, at - d * 150.0);
        if let (Some(a), Some(b)) = (s.ray(near, d, 20.0), s.ray(far, d, 200.0)) {
            let (pa, pb) = (near + d * a, far + d * b);
            assert!((pa - pb).len() < 3e-3, "{pa:?} {pb:?}");
        }
    }
}

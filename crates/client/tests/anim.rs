//! Gate: how Triple T moves, measured on the posed skeleton with no GPU.
//! After Kong Royale's `tools/actors/anim-test.mts` (MIT, NOTICE), which
//! drives its animator and fails on three numbers; these are ours, read off
//! `models/triple-t.glb`'s own clips, sampled here the way Bevy plays them:
//!
//! 1. **Feet.** A body travelling at a steady speed plays the loop and rate
//!    `anim::Gait` picks, its legs turned as the gait turns them. A foot is
//!    planted where its ankle is within [`PLANT_M`] of the loop's lowest; its
//!    speed over the ground there, over the body's, is the slide.
//! 2. **Reloads.** Each gun's reload clip, paced to the gun's own reload
//!    (`anim::drive`), plays at a pace a hand can believe, and starts and
//!    ends on the frame the wire's reload bit does, at any frame rate.
//! 3. **Hands.** At every key of `Rifle_Hold_Loop`, the left wrist against
//!    each long gun's own fore-end (`guns::left_wrist`): where the clip
//!    leaves it, and where `ik.rs` puts it.
//!
//! `-- --nocapture` prints the tables to tune by.

use std::collections::HashMap;

use bevy::prelude::*;
use oneshot_client::look::anim::{Clip, Gait, Hold, Seen, RATE_MAX, RATE_MIN};
use oneshot_client::look::bodies::{HELD_AT, HELD_SCALE, HELD_TURN};
use oneshot_client::look::guns::{left_wrist, GUNS};
use oneshot_client::look::ik::{foot_rise, furthest_in_reach, Legs, Limb};

/// A foot is down where its toes are this close to their lowest in the
/// loop, metres. The toes, not the ankle: Triple T's heel lifts and the foot
/// rolls over its toes through the stance, so the ankle never stands still.
const PLANT_M: f32 = 0.03;
const TOES: [&str; 2] = ["LeftToeBase", "RightToeBase"];
/// How far a loop's `Clip::stride_mps` may be off the speed its own planted
/// toes move at, as a share of it.
const STRIDE_OFF_MAX: f32 = 0.05;
/// The most a planted foot may slide, as a share of the body's speed.
const SLIDE_MAX: f32 = 0.10;
/// The most the hand may miss its fore-end once it is put there, metres.
const GRIP_MISS_M: f32 = 0.005;
/// The slowest and fastest a reload clip may be played to fit its gun: past
/// these a clip or a gun's reload time is wrong, not just quick.
const RELOAD_PACE: (f32, f32) = (0.5, 2.5);
/// Samples per loop.
const STEPS: usize = 240;

// ---------------------------------------------------------------- the file

struct Track {
    node: usize,
    path: u8, // b't', b'r', b's'
    step: bool,
    times: Vec<f32>,
    values: Vec<f32>,
}

struct Glb {
    names: Vec<String>,
    parent: Vec<Option<usize>>,
    rest: Vec<Transform>,
    clips: HashMap<String, Vec<Track>>,
}

impl Glb {
    fn load() -> Glb {
        let raw = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/models/triple-t.glb"))
            .expect("models/triple-t.glb");
        let u32_at = |o: usize| u32::from_le_bytes(raw[o..o + 4].try_into().unwrap()) as usize;
        let json_len = u32_at(12);
        let json: serde_json::Value = serde_json::from_slice(&raw[20..20 + json_len]).unwrap();
        let bin = &raw[20 + json_len + 8..];
        let floats = |acc: usize| -> Vec<f32> {
            let a = &json["accessors"][acc];
            assert_eq!(a["componentType"], 5126, "float accessors only");
            let n = a["count"].as_u64().unwrap() as usize
                * match a["type"].as_str().unwrap() {
                    "SCALAR" => 1,
                    "VEC3" => 3,
                    "VEC4" => 4,
                    t => panic!("{t}"),
                };
            let v = &json["bufferViews"][a["bufferView"].as_u64().unwrap() as usize];
            assert!(v.get("byteStride").is_none(), "packed views only");
            let o = v["byteOffset"].as_u64().unwrap_or(0) as usize + a["byteOffset"].as_u64().unwrap_or(0) as usize;
            (0..n)
                .map(|i| f32::from_le_bytes(bin[o + 4 * i..o + 4 * i + 4].try_into().unwrap()))
                .collect()
        };
        let nodes = json["nodes"].as_array().unwrap();
        let mut parent = vec![None; nodes.len()];
        let mut names = Vec::new();
        let mut rest = Vec::new();
        for (i, n) in nodes.iter().enumerate() {
            for c in n["children"].as_array().into_iter().flatten() {
                parent[c.as_u64().unwrap() as usize] = Some(i);
            }
            names.push(n["name"].as_str().unwrap_or("").to_string());
            let f = |k: &str, d: &[f32]| -> Vec<f32> {
                n[k].as_array()
                    .map(|a| a.iter().map(|x| x.as_f64().unwrap() as f32).collect())
                    .unwrap_or_else(|| d.to_vec())
            };
            let (t, r, s) = (
                f("translation", &[0.0; 3]),
                f("rotation", &[0.0, 0.0, 0.0, 1.0]),
                f("scale", &[1.0; 3]),
            );
            rest.push(Transform {
                translation: Vec3::from_slice(&t),
                rotation: Quat::from_slice(&r),
                scale: Vec3::from_slice(&s),
            });
        }
        let mut clips = HashMap::new();
        for a in json["animations"].as_array().unwrap() {
            let samplers = a["samplers"].as_array().unwrap();
            let mut tracks = Vec::new();
            for ch in a["channels"].as_array().unwrap() {
                let s = &samplers[ch["sampler"].as_u64().unwrap() as usize];
                let interp = s["interpolation"].as_str().unwrap_or("LINEAR");
                assert_ne!(interp, "CUBICSPLINE");
                tracks.push(Track {
                    node: ch["target"]["node"].as_u64().unwrap() as usize,
                    path: match ch["target"]["path"].as_str().unwrap() {
                        "translation" => b't',
                        "rotation" => b'r',
                        "scale" => b's',
                        p => panic!("{p}"),
                    },
                    step: interp == "STEP",
                    times: floats(s["input"].as_u64().unwrap() as usize),
                    values: floats(s["output"].as_u64().unwrap() as usize),
                });
            }
            clips.insert(a["name"].as_str().unwrap().to_string(), tracks);
        }
        Glb {
            names,
            parent,
            rest,
            clips,
        }
    }

    fn node(&self, name: &str) -> usize {
        self.names
            .iter()
            .position(|n| n == name)
            .unwrap_or_else(|| panic!("no {name}"))
    }

    fn duration(&self, clip: &str) -> f32 {
        self.clips[clip]
            .iter()
            .filter_map(|t| t.times.last().copied())
            .fold(0.0, f32::max)
    }

    /// Every node's own transform at `t` seconds into `clip`.
    fn pose(&self, clip: &str, t: f32) -> Vec<Transform> {
        let mut out = self.rest.clone();
        for tr in &self.clips[clip] {
            let k = tr.times.partition_point(|&x| x <= t).clamp(1, tr.times.len()) - 1;
            let n = if tr.path == b'r' { 4 } else { 3 };
            let at = |i: usize| &tr.values[i * n..i * n + n];
            let (a, b, u) = if k + 1 < tr.times.len() && !tr.step {
                let (t0, t1) = (tr.times[k], tr.times[k + 1]);
                (at(k), at(k + 1), ((t - t0) / (t1 - t0)).clamp(0.0, 1.0))
            } else {
                (at(k), at(k), 0.0)
            };
            let o = &mut out[tr.node];
            match tr.path {
                b'r' => o.rotation = Quat::from_slice(a).slerp(Quat::from_slice(b), u).normalize(),
                b't' => o.translation = Vec3::from_slice(a).lerp(Vec3::from_slice(b), u),
                _ => o.scale = Vec3::from_slice(a).lerp(Vec3::from_slice(b), u),
            }
        }
        out
    }

    /// `node`'s frame in the scene's, under `locals`.
    fn frame(&self, locals: &[Transform], node: usize) -> Transform {
        let mut out = locals[node];
        let mut at = node;
        while let Some(p) = self.parent[at] {
            out = locals[p].mul_transform(out);
            at = p;
        }
        out
    }

    fn limb(&self, locals: &[Transform], bones: [&str; 3]) -> Limb {
        let [u, l, e] = bones.map(|b| self.node(b));
        Limb {
            parent: self.frame(locals, self.parent[u].unwrap()),
            upper: locals[u],
            lower: locals[l],
            end: locals[e],
        }
    }
}

fn median(mut v: Vec<f32>) -> f32 {
    v.sort_by(|a, b| a.total_cmp(b));
    v.get(v.len() / 2).copied().unwrap_or(f32::NAN)
}

// ---------------------------------------------------------------- 1. feet

/// A body that has travelled at `speed` on `bearing` (radians off its
/// facing, the wire's sense) long enough to settle, its gait read back.
fn settled(speed: f32, bearing: f32, crouched: bool) -> Gait {
    let mut g = Gait::default();
    let s = Seen {
        alive: true,
        grounded: true,
        crouched,
        vel: Vec2::new(bearing.cos(), bearing.sin()) * speed,
        ..default()
    };
    for _ in 0..600 {
        g.observe(&s, 1.0 / 60.0);
    }
    g
}

struct Slide {
    clip: Clip,
    rate: f32,
    slide: f32,
}

/// The planted feet's slide for a body travelling at `speed` on `bearing`.
fn slide(glb: &Glb, speed: f32, bearing: f32, crouched: bool) -> Slide {
    let g = settled(speed, bearing, crouched);
    let clip = g.clip.unwrap();
    let rate = g.rate();
    let d = glb.duration(clip.name());
    let feet = [glb.node(TOES[0]), glb.node(TOES[1])];
    let at = |t: f32| {
        let p = glb.pose(clip.name(), t.rem_euclid(d));
        feet.map(|f| glb.frame(&p, f).translation)
    };
    let h = d / STEPS as f32;
    let samples: Vec<[Vec3; 2]> = (0..STEPS).map(|i| at(i as f32 * h)).collect();
    // The travel in the legs' frame: the clips face +Z, and what the twist
    // can't take up of the bearing is left over as a sideways error.
    let rel = -bearing;
    let want = if g.backward { rel - core::f32::consts::PI } else { rel };
    let err = want - g.hip_yaw;
    let sign = if g.backward { -1.0 } else { 1.0 };
    let body = Vec3::new(err.sin(), 0.0, err.cos()) * speed * sign;
    let mut slid = Vec::new();
    for f in 0..2 {
        let low = samples.iter().map(|s| s[f].y).fold(f32::INFINITY, f32::min);
        for i in 0..STEPS {
            if samples[i][f].y > low + PLANT_M {
                continue;
            }
            let v = (samples[(i + 1) % STEPS][f] - samples[(i + STEPS - 1) % STEPS][f]) / (2.0 * h);
            let ground = body + Vec3::new(v.x, 0.0, v.z) * rate;
            slid.push(ground.length() / speed);
        }
    }
    Slide {
        clip,
        rate,
        slide: median(slid),
    }
}

/// How fast a loop's planted toes go back under the body, m/s of the clip's
/// own time: the ground speed it was made for (both feet's together).
fn stride(glb: &Glb, clip: Clip) -> f32 {
    let d = glb.duration(clip.name());
    let h = d / STEPS as f32;
    let toes = TOES.map(|t| glb.node(t));
    let s: Vec<[Vec3; 2]> = (0..STEPS)
        .map(|i| {
            let p = glb.pose(clip.name(), i as f32 * h);
            toes.map(|t| glb.frame(&p, t).translation)
        })
        .collect();
    let mut v = Vec::new();
    for f in 0..2 {
        let low = s.iter().map(|x| x[f].y).fold(f32::INFINITY, f32::min);
        for i in 0..STEPS {
            if s[i][f].y <= low + PLANT_M {
                let d = (s[(i + 1) % STEPS][f] - s[(i + STEPS - 1) % STEPS][f]) / (2.0 * h);
                v.push(Vec2::new(d.x, d.z).length());
            }
        }
    }
    median(v)
}

/// Each loop's stride, the number its rate is set by, is what its feet do:
/// measured again here every run, not once by hand.
#[test]
fn every_stride_is_what_its_clip_does() {
    let glb = Glb::load();
    let mut off = Vec::new();
    for c in Clip::ALL {
        let Some(want) = c.stride_mps() else { continue };
        let got = stride(&glb, c);
        println!("{:<11} measured {got:.3} m/s, stride_mps {want:.3}", format!("{c:?}"));
        if (want - got).abs() > STRIDE_OFF_MAX * got {
            off.push(format!("{c:?}: {want} vs {got:.3}"));
        }
    }
    assert!(off.is_empty(), "stride_mps is off its clip: {off:?}");
}

/// Every gait the legs play, at speeds inside the band its loop is played
/// in, forward, backwards and on a diagonal: the planted feet hold.
#[test]
fn planted_feet_hold_at_every_gait() {
    let glb = Glb::load();
    let mut failed = Vec::new();
    let cases: &[(&str, f32, f32, bool)] = &[
        ("walk", 1.0, 0.0, false),
        ("walk-1.5", 1.5, 0.0, false),
        ("jog", 3.2, 0.0, false),
        ("jog-4", 4.0, 0.0, false),
        ("run", sim::consts::RUN, 0.0, false),
        ("run-diag", sim::consts::RUN, 0.785, false),
        ("jog-diag", 3.2, -0.785, false),
        ("backpedal", 1.2, core::f32::consts::PI, false),
        ("backpedal-run", sim::consts::RUN, core::f32::consts::PI, false),
    ];
    println!(
        "{:<14} {:>6} {:<11} {:>6} {:>7}",
        "case", "m/s", "clip", "rate", "slide"
    );
    for &(name, speed, bearing, crouched) in cases {
        let s = slide(&glb, speed, bearing, crouched);
        let in_band = s.rate.abs() > RATE_MIN + 1e-3 && s.rate.abs() < RATE_MAX - 1e-3;
        println!(
            "{name:<14} {speed:>6.2} {:<11} {:>6.2} {:>6.1}%",
            format!("{:?}", s.clip),
            s.rate,
            s.slide * 100.0
        );
        if in_band && s.slide > SLIDE_MAX {
            failed.push(format!("{name}: {:.1}%", s.slide * 100.0));
        }
    }
    assert!(
        failed.is_empty(),
        "planted feet slide past {}%: {failed:?}",
        SLIDE_MAX * 100.0
    );
}

/// What the twist and the rate band leave sliding, printed and never gated:
/// a side-on strafe past the legs' turn; the crouch run at the sim's speed,
/// past the crouch loop's top rate; and `Crouch_Fwd_Loop` at any speed, whose
/// two feet go back at different speeds (no one rate plants both: the clip
/// wants fixing, not the rate). The numbers to beat.
#[test]
fn what_still_slides() {
    let glb = Glb::load();
    for (name, speed, bearing, crouched) in [
        ("strafe", 3.2, core::f32::consts::FRAC_PI_2, false),
        ("strafe-run", sim::consts::RUN, core::f32::consts::FRAC_PI_2, false),
        ("crouch-walk", 1.0, 0.0, true),
        ("crouch-run", sim::consts::CROUCH_RUN, 0.0, true),
    ] {
        let s = slide(&glb, speed, bearing, crouched);
        println!(
            "{name:<12} {speed:>5.2} m/s {:?} at {:.2}: {:.1}% slide",
            s.clip,
            s.rate,
            s.slide * 100.0
        );
        assert!(s.slide.is_finite());
    }
}

// ---------------------------------------------------------------- 2. reloads

/// Each gun's reload clip, paced to its gun, plays at a believable pace.
#[test]
fn every_reload_clip_fits_its_gun() {
    let glb = Glb::load();
    let mut bad = Vec::new();
    for (arm, g) in GUNS.iter() {
        let a = sim::arms::arm(*arm);
        if a.hot() || *arm == sim::arms::SWORD || a.reload_ms == 0 {
            continue;
        }
        let hold = if g.long { Hold::Long } else { Hold::Pistol };
        let clip = glb.duration(hold.reload_name());
        let secs = a.reload_ms as f32 / 1000.0;
        let pace = clip / secs;
        println!(
            "{:<16} {:>5.2} s on a {:>4.2} s clip: pace {pace:.2}",
            g.file, secs, clip
        );
        if !(RELOAD_PACE.0..=RELOAD_PACE.1).contains(&pace) {
            bad.push(format!("{} {pace:.2}", g.file));
        }
    }
    assert!(bad.is_empty(), "reload clips played too fast or slow: {bad:?}");
}

/// The gait's reload is the wire's bit, the same frame, at any frame rate.
#[test]
fn a_reload_starts_and_ends_on_the_wire_s_frame() {
    for hz in [30.0, 60.0, 144.0, 240.0] {
        let dt = 1.0 / hz;
        let mut g = Gait::default();
        let mut s = Seen {
            alive: true,
            grounded: true,
            hold: Some(Hold::Long),
            ..default()
        };
        for frame in 0..(hz as usize * 3) {
            let on = (hz as usize / 2..hz as usize * 2).contains(&frame);
            s.reload = on.then_some((Hold::Long, 1.5));
            g.observe(&s, dt);
            assert_eq!(g.reload.is_some(), on, "{hz} Hz, frame {frame}");
        }
    }
}

// ---------------------------------------------------------------- 3. hands

/// At every key of the long hold: where the clip leaves the left wrist
/// against each long gun's fore-end; that the reach puts it on the gun, as
/// far up its barrel as the arm goes; and how far short of the fore-end that
/// still is (printed: the number a turned shoulder or a gun drawn back would
/// beat).
#[test]
fn the_left_hand_is_on_every_long_gun() {
    let glb = Glb::load();
    let clip = Hold::Long.name();
    let d = glb.duration(clip);
    let hand = glb.node("RightHand");
    let left = ["LeftArm", "LeftForeArm", "LeftHand"];
    let holder = Transform {
        translation: HELD_AT,
        rotation: HELD_TURN.normalize(),
        scale: Vec3::splat(HELD_SCALE),
    };
    println!("{:<16} {:>9} {:>9} {:>9}", "gun", "clip off", "on gun", "short");
    let mut worst_ar = 0.0f32;
    for (arm, g) in GUNS.iter().filter(|(_, g)| g.long) {
        let (mut off, mut miss, mut short) = (0.0f32, 0.0f32, 0.0f32);
        for i in 0..24 {
            let p = glb.pose(clip, d * i as f32 / 24.0);
            let held = glb.frame(&p, hand).mul_transform(holder);
            let fore = held.transform_point(left_wrist(g, 1.0));
            let limb = glb.limb(&p, left);
            let j = limb.joints();
            off = off.max((j.end - fore).length());
            let target = furthest_in_reach(j.root, limb.span(), |k| held.transform_point(left_wrist(g, k)));
            short = short.max((fore - target).length());
            let q = limb.reach(target, None).expect("the hold's arm is bent");
            let mut k = limb;
            (k.upper.rotation, k.lower.rotation, k.end.rotation) = (q[0], q[1], q[2]);
            let jk = k.joints();
            miss = miss.max((jk.end - target).length());
            assert!(jk.end_turn.angle_between(j.end_turn) < 1e-3, "the hand keeps its turn");
        }
        if *arm == sim::arms::AR {
            worst_ar = off;
        }
        println!(
            "{:<16} {:>7.1}cm {:>7.2}mm {:>7.1}cm",
            g.file,
            off * 100.0,
            miss * 1000.0,
            short * 100.0
        );
        assert!(
            miss < GRIP_MISS_M,
            "{}: the reach misses by {:.1} mm",
            g.file,
            miss * 1000.0
        );
    }
    // The clip was baked onto the rifle's fore-end: if it is off there, the
    // holder's frame here is not the one `tools/models/triple-t.py` used.
    assert!(
        worst_ar < 0.01,
        "the hold's wrist is {:.1} cm off the rifle",
        worst_ar * 100.0
    );
}

// ---------------------------------------------------------------- feet on a slope

/// Standing across a 20° slope, each foot meets the ground under it and the
/// hips come down for the lower one.
#[test]
fn feet_meet_a_slope() {
    let glb = Glb::load();
    let hips = glb.node("Hips");
    let bones = |side: &str| [format!("{side}UpLeg"), format!("{side}Leg"), format!("{side}Foot")];
    let slope = 20f32.to_radians().tan();
    for i in 0..8 {
        let p = glb.pose(Clip::Idle.name(), glb.duration(Clip::Idle.name()) * i as f32 / 8.0);
        let leg = |side: &str| bones(side).map(|b| p[glb.node(&b)]);
        let legs = Legs {
            hips_parent: glb.frame(&p, glb.parent[hips].unwrap()),
            hips: p[hips],
            left: leg("Left"),
            right: leg("Right"),
        };
        let a = legs.ankles();
        // The ground rises along +X; the body stands at x = 0 on it.
        let rises = a.map(|v| foot_rise(v.x * slope, 0.0));
        let planted = legs.plant(rises, 1.0);
        let mut k = legs;
        k.hips.translation = planted.hips;
        for (b, q) in [(&mut k.left, planted.left), (&mut k.right, planted.right)] {
            let q = q.expect("a standing leg is bent");
            for (t, q) in b.iter_mut().zip(q) {
                t.rotation = q;
            }
        }
        let ka = k.ankles();
        for f in 0..2 {
            let want = a[f] + Vec3::Y * rises[f];
            assert!((ka[f] - want).length() < 0.005, "foot {f}: {} vs {want}", ka[f]);
        }
    }
}

//! The kill cam: built from the snapshots this client was sent and nothing
//! else, so it can only replay what fog already let through.

use client_net::core::{NetCore, KILLCAM_MS};
use proto::{EventState, Remote, Snapshot};
use sim::consts::{ticks, TICK_HZ};
use sim::events::Kind;
use sim::fighter::Fighter;
use sim::geom::V3;
use sim::map::Map;
use sim::movement::Body;
use sim::world::Rules;

const TICK_MS: f64 = 1000.0 / TICK_HZ as f64;

/// Seat 0 stands at the origin; seat 1 (shown when `seen(tick)`) walks
/// along x and kills it at `death`.
fn feed(c: &mut NetCore, death: u32, seen: impl Fn(u32) -> bool) {
    for tick in (2..=death).step_by(2) {
        let mut s = Snapshot {
            tick,
            phase: 1,
            own: Fighter {
                seated: true,
                alive: tick < death,
                body: Body {
                    pos: V3::ZERO,
                    grounded: true,
                    ..Body::default()
                },
                ..Fighter::default()
            },
            ..Snapshot::default()
        };
        if seen(tick) {
            s.remotes[0] = Remote {
                slot: 1,
                team: 1,
                alive: true,
                pos: V3::new(10.0 + tick as f32 * 0.01, 0.0, 5.0),
                yaw: 49152,
                ..Remote::default()
            };
            s.n_remote = 1;
        }
        if tick == death {
            s.events[0] = EventState {
                kind: Kind::Kill as u8,
                a: 1,
                b: 0,
                ..EventState::default()
            };
            s.n_events = 1;
        }
        c.on_snapshot(tick as f64 * TICK_MS, &s);
    }
}

fn client() -> NetCore {
    NetCore::new(0, 0, 0, Rules::duel(), Map::proving_grounds())
}

#[test]
fn it_plays_the_run_up_from_the_killer_s_eye_then_holds() {
    let mut c = client();
    let death = 1000;
    feed(&mut c, death, |_| true);
    let at = death as f64 * TICK_MS;
    let cam = c.killcam(at).expect("a kill cam");
    assert_eq!(cam.killer, 1);
    assert_eq!(cam.progress, 0.0);
    // Starts KILLCAM_MS before the kill, where the killer stood then.
    let start = death - ticks(KILLCAM_MS);
    assert!((cam.eye.x - (10.0 + start as f32 * 0.01)).abs() < 0.03, "{cam:?}");
    assert!((cam.eye.y - sim::consts::EYE).abs() < 1e-6);
    // We are one of the bodies; the killer, whose eye this is, is not.
    assert!(cam.bodies.iter().any(|b| b.slot == 0));
    assert!(cam.bodies.iter().all(|b| b.slot != 1));
    // Halfway, and then held on the kill.
    let mid = c.killcam(at + KILLCAM_MS as f64 / 2.0).unwrap();
    assert!((mid.progress - 0.5).abs() < 0.02, "{mid:?}");
    let end = c.killcam(at + KILLCAM_MS as f64 * 3.0).unwrap();
    assert_eq!(end.progress, 1.0);
    assert!((end.eye.x - (10.0 + death as f32 * 0.01)).abs() < 0.03, "{end:?}");
}

#[test]
fn it_shows_only_what_fog_let_through() {
    // Never shown: no kill cam at all.
    let mut c = client();
    feed(&mut c, 1000, |_| false);
    assert!(c.killcam(1000.0 * TICK_MS).is_none());
    // Shown only for the last half second: the cam starts there.
    let mut c = client();
    let death = 1000;
    let shown = death - ticks(500);
    feed(&mut c, death, |t| t >= shown);
    let cam = c.killcam(death as f64 * TICK_MS).unwrap();
    assert!((cam.eye.x - (10.0 + shown as f32 * 0.01)).abs() < 0.03, "{cam:?}");
    // Shown, then hidden before the kill: held where it was last seen.
    let mut c = client();
    let gone = death - ticks(800);
    feed(&mut c, death, |t| t < gone);
    let end = c.killcam(death as f64 * TICK_MS + 10_000.0).unwrap();
    assert!((end.eye.x - (10.0 + (gone - 2) as f32 * 0.01)).abs() < 0.03, "{end:?}");
}

#[test]
fn it_ends_at_the_respawn() {
    let mut c = client();
    feed(&mut c, 1000, |_| true);
    assert!(c.killcam(1000.0 * TICK_MS).is_some());
    let s = Snapshot {
        tick: 1400,
        phase: 1,
        own: Fighter {
            seated: true,
            alive: true,
            ..Fighter::default()
        },
        ..Snapshot::default()
    };
    c.on_snapshot(1400.0 * TICK_MS, &s);
    assert!(c.killcam(1400.0 * TICK_MS).is_none());
}

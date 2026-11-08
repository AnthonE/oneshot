//! A passenger is sent the stick its driver drives the hull by, as the
//! server ran it, so it can predict the hull it rides; nobody else is.

use proto::{InputMsg, Look, Msg, Name, Snapshot};
use room::Room;
use sim::consts::TICK_HZ;
use sim::geom::V3;
use sim::input::{Input, USE};
use sim::vehicle::{DRIVER, PASSENGER};
use sim::world::{Phase, Rules};

const TICK_MS: f64 = 1000.0 / TICK_HZ as f64;

struct Run {
    room: Room,
    seats: [u8; 3],
    seq: u32,
    now: f64,
}

impl Run {
    /// One tick, each seat's frame in first: the snapshots it made.
    fn tick(&mut self, frames: [Input; 3]) -> Vec<(u8, Snapshot)> {
        for (k, &s) in self.seats.iter().enumerate() {
            let mut m = InputMsg {
                first_seq: self.seq,
                n: 1,
                ..InputMsg::default()
            };
            m.frames[0] = frames[k];
            self.room.on_input(s, &m, self.now);
        }
        self.seq += 1;
        self.now += TICK_MS;
        self.room.step(self.now).0
    }
}

fn start() -> Run {
    let map = sim::map::by_id(3).expect("Dry Fork");
    let rules = Rules {
        score_limit: 1000,
        time_limit_ms: 0,
        ..Rules::big_team()
    };
    let mut room = Room::new(map, rules, 11, (5, 6));
    room.passive_bots = true;
    let mut seats = [0u8; 3];
    for s in seats.iter_mut() {
        let (slot, out) = room.join_team(Name::new("t"), Look::default(), Some(0)).unwrap();
        assert!(matches!(out.first(), Some((_, Msg::Welcome { .. }))));
        *s = slot;
    }
    let mut r = Run {
        room,
        seats,
        seq: 0,
        now: 0.0,
    };
    while r.room.world.phase != Phase::Playing {
        r.tick([Input::default(); 3]);
    }
    // The driver and the passenger beside truck 0, the third off a way.
    let v = r.room.world.vehicles[0].pos;
    for (k, (dx, dz)) in [(-2.0, 0.0), (0.0, 2.5), (10.0, -12.0)].into_iter().enumerate() {
        let (x, z) = (v.x + dx, v.z + dz);
        let y = r.room.world.map.ground(x, z);
        r.room.world.fighters[seats[k] as usize].body.pos = V3::new(x, y, z);
    }
    r
}

#[test]
fn a_passenger_is_sent_the_stick_its_driver_drives_by() {
    let mut r = start();
    let [a, b, _] = r.seats;
    let press = Input {
        buttons: USE,
        ..Input::default()
    };
    let idle = Input::default();
    r.tick([press, idle, idle]);
    r.tick([idle, idle, idle]);
    r.tick([idle, press, idle]);
    r.tick([idle, idle, idle]);
    assert_eq!(r.room.world.fighters[a as usize].riding(), Some((0, DRIVER)));
    assert_eq!(r.room.world.fighters[b as usize].riding(), Some((0, PASSENGER)));

    // Each stick held an odd number of ticks, so some change on a snapshot's
    // tick and some between: every snapshot carries the one run on its own
    // tick, never an older. -128 is run (and sent) as -127.
    let mut sent = 0;
    for (mx, mz) in [(0i8, 127i8), (-128, 127), (90, -60), (0, 0)] {
        let drive = Input {
            mx,
            mz,
            ..Input::default()
        };
        let ran = drive.sanitized();
        for _ in 0..63 {
            for (seat, s) in r.tick([drive, idle, idle]) {
                if seat == b {
                    assert_eq!(
                        s.drive,
                        Some(sim::vehicle::stick(sim::vehicle::TRUCK, &ran)),
                        "the stick run on tick {}",
                        s.tick
                    );
                    sent += 1;
                    assert!(s.own_vehicle.is_some());
                } else {
                    assert_eq!(s.drive, None, "seat {seat} told a stick it doesn't ride behind");
                }
            }
        }
    }
    assert!(sent > 100, "{sent}");
    assert!(
        r.room.world.vehicles[0].pos.x != r.room.world.map.vehicles[0].pos.x,
        "never drove"
    );

    // The driver gets out: the hull is parked, and its passenger is told so.
    r.tick([press, idle, idle]);
    assert_eq!(r.room.world.fighters[a as usize].riding(), None);
    for _ in 0..8 {
        for (seat, s) in r.tick([idle; 3]) {
            assert_eq!(s.drive, None, "seat {seat}: a stick for a parked hull");
            if seat == b {
                assert!(s.own_vehicle.is_some());
            }
        }
    }
}

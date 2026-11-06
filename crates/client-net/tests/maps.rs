//! One link, several maps: a cup's next heat (or any second welcome) names
//! another map, and the netcode predicts on that map's walls from then on,
//! nothing of the last left; a map this build doesn't know is refused as
//! out of date, never played on another map's walls.

use client_net::session::Session;
use proto::{salt_commit, Msg};
use sim::world::Rules;

fn welcome(map: u8) -> Msg {
    Msg::Welcome {
        slot: 0,
        team: 0,
        tick: 40,
        tick_hz: 128,
        map,
        rules: Rules::doubles(),
        salt_commit: salt_commit([1, 2]),
    }
}

#[test]
fn a_welcome_on_another_map_predicts_on_its_walls() {
    let mut s = Session::new();
    s.on_msg(welcome(0));
    assert!(s.ready);
    assert_eq!((s.map_id, s.core.map.id), (0, 0));
    assert_eq!(s.core.map.void, f32::NEG_INFINITY);
    // The next heat is on Cloudbreak: its void and its terrain.
    s.on_msg(welcome(7));
    assert_eq!((s.map_id, s.welcomes, s.core.map.id), (7, 2, 7));
    assert!(s.core.map.void > f32::NEG_INFINITY);
    assert!(s.core.map.terrain.is_some());
    // And back to an arena: nothing of the island is left.
    s.on_msg(welcome(1));
    assert_eq!((s.map_id, s.welcomes, s.core.map.id), (1, 3, 1));
    assert!(s.core.map.terrain.is_none());
    assert_eq!(s.core.map.void, f32::NEG_INFINITY);
}

#[test]
fn a_map_this_build_doesnt_know_is_out_of_date() {
    let mut s = Session::new();
    s.on_msg(welcome(0));
    s.on_msg(welcome(200));
    assert_eq!((s.map_id, s.welcomes), (0, 1), "still on the map it knows");
    assert!(s
        .take_messages()
        .iter()
        .any(|m| matches!(m, Msg::Refuse { reason } if *reason == proto::REFUSE_VERSION)));
}

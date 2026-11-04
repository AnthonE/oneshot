//! `oneshot [--server URL] [--name you] [--mode casual|duel|doubles|ffa|gungame|bigteam|0..6]
//! [--autoplay] [--lite] [--capsules]` (or `oneshot --pose 'yaw=1.57&arm=3'`,
//! no server, for a picture): the desktop client. It opens on the title
//! screen (`front.rs`); PLAY reads the game server's address and
//! certificate hash from the page server's `server.json` (what the browser
//! page reads) and connects with client-net's native session. `--mode` or
//! `--autoplay` goes straight into a match.
//!
//! The server is the public one unless `--server` names another:
//! `https://game.elopros.com/oneshot` (production: a CA certificate, so
//! `server.json` sends no hash and the system's roots are trusted), or a
//! local `oneshot-server` as `127.0.0.1:8080` / `http://127.0.0.1:8080`
//! (its own certificate, pinned by the hash it sends).

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    if let Err(e) = run() {
        eprintln!("oneshot: {e}");
        std::process::exit(1);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn run() -> Result<(), String> {
    use oneshot_client::door::{Door, PUBLIC};
    use oneshot_client::front::Start;

    let args: Vec<String> = std::env::args().collect();
    let arg = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    let flag = |k: &str| args.iter().any(|a| a == k);
    if flag("--help") || flag("-h") {
        println!("{}", HELP.replace("{PUBLIC}", PUBLIC));
        return Ok(());
    }
    let lite = flag("--lite");
    if let Some(q) = arg("--pose") {
        // No server: one fighter for a picture (`--pose 'yaw=1.57&arm=3'`).
        let pose = oneshot_client::pose::Pose::parse(&q);
        let opts = oneshot_client::Opts {
            lite,
            pose: Some(pose),
            ..Default::default()
        };
        oneshot_client::app(Box::new(oneshot_client::link::Still::on(pose.2.map)), opts).run();
        return Ok(());
    }
    let server = arg("--server").unwrap_or_else(|| PUBLIC.into());
    let mode = match arg("--mode") {
        Some(m) => Some(mode(&m).ok_or_else(|| format!("--mode {m}: one of {MODES}"))?),
        None => None,
    };
    let autoplay = flag("--autoplay");
    let fxdemo = flag("--fxdemo");
    let opts = oneshot_client::Opts {
        autoplay,
        lite,
        pose: None,
        capsules: flag("--capsules"),
        fxdemo,
        fxstage: None,
        open: oneshot_client::Screen::Playing,
        front: match mode {
            Some(m) => Start::Join(m),
            None if autoplay || fxdemo => Start::Join(proto::MODE_CASUAL),
            None => Start::Title,
        },
    };
    let mut app = oneshot_client::game(Door::new(server), opts);
    if let Some(n) = arg("--name") {
        app.world_mut().resource_mut::<oneshot_client::settings::Player>().name =
            oneshot_client::settings::Player::clean(&n);
    }
    app.run();
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
const HELP: &str = "oneshot: the desktop client

  oneshot                          the title screen; play on the public server ({PUBLIC})
  oneshot --server 127.0.0.1:8080  play on a local oneshot-server
  --name NAME                      the name others see (the title's field)
  --mode MODE                      straight into a match: casual, ffa, gungame, bigteam;
                                   duel and doubles are ranked, which needs an Elo Pros
                                   sign-in (the web page)
  --lite                           the light tier: for a slow or software GPU
  --pose 'yaw=1.57&arm=3'          no server: one fighter, for a picture
  --pose 'map=6&eye=-20,6,0&look=0,2,0'  no server: a map's view

Keys (by default; rebind them, with the mouse's speed, the field of view
and the volume, in OPTIONS, saved in your config folder under
oneshot/settings.toml): WASD and the mouse, space jumps, Ctrl or C crouches,
R reloads, E uses, G throws a frag, T picks frag or sticky, Q or V melees,
X, 1 or the wheel swaps, N the net graph, Esc the menu.";

#[cfg(not(target_arch = "wasm32"))]
const MODES: &str = "casual, duel, doubles, ffa, gungame, bigteam (or 0 to 6)";

/// A mode by name or number (proto's `MODE_*`).
#[cfg(not(target_arch = "wasm32"))]
fn mode(m: &str) -> Option<u8> {
    Some(match m.to_ascii_lowercase().as_str() {
        "casual" | "bot" => proto::MODE_CASUAL,
        "duel" => proto::MODE_DUEL,
        "doubles" => proto::MODE_DOUBLES,
        "ffa" => proto::MODE_FFA,
        "gungame" | "gun-game" => proto::MODE_GUN_GAME,
        "cup" => proto::MODE_CUP,
        "bigteam" | "big-team" => proto::MODE_BIG_TEAM,
        n => return n.parse().ok().filter(|&n| n <= proto::MODE_BIG_TEAM),
    })
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    #[test]
    fn modes_by_name() {
        assert_eq!(super::mode("Duel"), Some(proto::MODE_DUEL));
        assert_eq!(super::mode("6"), Some(proto::MODE_BIG_TEAM));
        assert_eq!(super::mode("7"), None);
    }
}

#[cfg(target_arch = "wasm32")]
fn main() {}

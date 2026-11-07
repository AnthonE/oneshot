//! `oneshot [--server URL] [--name you] [--mode casual|duel|doubles|ffa|gungame|cup|bigteam|ctf|watch|0..6|8|9]
//! [--autoplay [--scope|--drill]] [--lite] [--potato] [--quality potato|low|medium|high] [--capsules] [--menu|--settings|--graphics] [--windowed]
//! [--shot out.png [--shot-at s]] [--record x.tape SECONDS]`
//! (or `oneshot --pose 'yaw=1.57&arm=3'`, no server, for a picture; or
//! `oneshot --film shots.json`, `--clip x.tape [--style S]`, `--scan x.tape`,
//! a tape drawn: `film/`, `clip.rs`): the desktop client. It opens on the title
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
    use oneshot_client::Screen;

    let args: Vec<String> = std::env::args().collect();
    let arg = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    let flag = |k: &str| args.iter().any(|a| a == k);
    if flag("--help") || flag("-h") {
        println!("{}", HELP.replace("{PUBLIC}", PUBLIC));
        return Ok(());
    }
    let lite = flag("--lite");
    let potato = flag("--potato");
    let quality = match arg("--quality") {
        Some(q) => Some(
            oneshot_client::look::quality::Quality::from_name(&q.to_ascii_lowercase())
                .ok_or_else(|| format!("--quality {q}: potato, low, medium or high"))?,
        ),
        None => None,
    };
    // `--knobs 'Sparks.speed=4,12;…'`: effects retuned for this run, as the fx
    // stage's (`tools/reel.sh`, REEL_KNOBS).
    if let Some(k) = arg("--knobs") {
        oneshot_client::look::fx::table::set_knobs(&k)?;
    }
    // `--shot x.png [--shot-at s]`: one frame once the screen is there, then out.
    let shot = match arg("--shot") {
        Some(p) if p.to_ascii_lowercase().ends_with(".png") => Some(std::path::PathBuf::from(p)),
        Some(p) => return Err(format!("--shot {p}: a .png path")),
        None => None,
    };
    let shot_at = match arg("--shot-at") {
        Some(s) => s.parse::<f64>().map_err(|_| format!("--shot-at {s}: seconds"))?,
        None => 1.0,
    };
    // A tape drawn, not a match played (`film/`, `clip.rs`).
    if let Some(p) = arg("--scan") {
        let every = arg("--every").and_then(|s| s.parse::<f64>().ok()).unwrap_or(5.0);
        return scan(std::path::Path::new(&p), every);
    }
    if let Some(p) = arg("--film") {
        let mut script = oneshot_client::film::Script::load(std::path::Path::new(&p))?;
        if script.out.as_os_str().is_empty() {
            let stem = std::path::Path::new(&p).file_stem().unwrap_or_default().to_owned();
            script.out = std::path::Path::new("traces/film").join(stem);
        }
        return film(script, quality, None);
    }
    if let Some(p) = arg("--clip") {
        use oneshot_client::clip;
        let tape = std::path::PathBuf::from(&p);
        let style = arg("--style").unwrap_or_else(|| clip::STYLES[0].into());
        let json =
            clip::script(&style).ok_or_else(|| format!("--style {style}: one of {}", clip::STYLES.join(", ")))?;
        let mut script = oneshot_client::film::Script::parse(&json)?;
        if !oneshot_client::film::encode::ffmpeg_here() {
            return Err(format!(
                "a clip's video is made with ffmpeg, and there is none on the path: install it, then `oneshot --clip {p}`"
            ));
        }
        script.tape = tape.clone();
        script.out = tape.with_extension("shots");
        if let Some(px) = arg("--max-height").and_then(|s| s.parse::<u32>().ok()) {
            script.fit_height(px);
        }
        let out = tape.with_extension("mp4");
        return film(script, quality, Some((tape.with_extension("shots"), out)));
    }
    let record = match (arg("--record"), args.iter().position(|a| a == "--record")) {
        (Some(p), Some(i)) => {
            let secs = args
                .get(i + 2)
                .and_then(|s| s.parse::<f64>().ok())
                .ok_or_else(|| format!("--record {p}: and how many seconds"))?;
            Some(oneshot_client::clip::Record::new(std::path::PathBuf::from(p), secs))
        }
        _ => None,
    };
    let open = if flag("--settings") || flag("--graphics") {
        Screen::Settings
    } else if flag("--menu") {
        Screen::Paused
    } else {
        Screen::Playing
    };
    let (mut app, want, want_match) = if let Some(q) = arg("--pose") {
        // No server: one fighter for a picture (`--pose 'yaw=1.57&arm=3'`).
        let pose = oneshot_client::pose::Pose::parse(&q);
        let opts = oneshot_client::Opts {
            lite,
            potato,
            quality,
            pose: Some(pose),
            windowed: true,
            ..Default::default()
        };
        let link = Box::new(oneshot_client::link::Still::on(pose.2.map));
        (oneshot_client::app(link, opts), Screen::Playing, false)
    } else {
        let server = arg("--server").unwrap_or_else(|| PUBLIC.into());
        let mode = match arg("--mode") {
            Some(m) => Some(mode(&m).ok_or_else(|| format!("--mode {m}: one of {MODES}"))?),
            None => None,
        };
        let autoplay = flag("--autoplay");
        // `--fxstage N`: the demo's stage N alone (`look/fx/demo.rs`).
        let fxstage = arg("--fxstage").and_then(|n| n.parse::<u32>().ok());
        let fxdemo = flag("--fxdemo") || fxstage.is_some();
        let front = match mode {
            Some(m) => Start::Join(m),
            None if autoplay || fxdemo => Start::Join(proto::MODE_CASUAL),
            None => Start::Title,
        };
        let opts = oneshot_client::Opts {
            autoplay,
            lite,
            potato,
            softgpu: false,
            quality,
            pose: None,
            capsules: flag("--capsules"),
            graphics: flag("--graphics"),
            fxdemo,
            fxstage,
            open,
            front,
            party: arg("--party").and_then(|c| proto::party::code_parse(&c)).unwrap_or(0),
            // A picture is the window's size, not the monitor's; a tool's
            // run (a tape, the autopilot) doesn't take the screen.
            windowed: flag("--windowed") || shot.is_some() || record.is_some() || autoplay,
        };
        let joins = matches!(front, Start::Join(_));
        // On the title only the settings open over it; in a match, the
        // screen asked for.
        let want = match (joins, open) {
            (true, o) => o,
            (false, Screen::Settings) => Screen::Settings,
            (false, _) => Screen::Title,
        };
        let mut app = oneshot_client::game(Door::new(server.clone()), opts);
        // After the app is made, so whatever hook Bevy set is wrapped.
        oneshot_client::crash::hook(server);
        // The autopilot holds the scope (a picture of one; `--autoplay`).
        app.world_mut().resource_mut::<oneshot_client::Autoplay>().scope = flag("--scope");
        // Or works its gun at a wall (`tools/reel.sh`).
        app.world_mut().resource_mut::<oneshot_client::Autoplay>().drill = flag("--drill");
        // For this run: the settings file keeps the player's own
        // (`settings::run_as`).
        if let Some(n) = arg("--name") {
            oneshot_client::settings::run_as(app.world_mut(), |p| {
                p.name = oneshot_client::settings::Player::clean(&n);
            });
        }
        // What Triple T wears, for a picture: `--look skin,head,feet,emblem`.
        if let Some(l) = arg("--look") {
            let v: Vec<u16> = l.split(',').filter_map(|n| n.trim().parse().ok()).collect();
            let at = |i: usize| v.get(i).copied().unwrap_or(0);
            oneshot_client::settings::run_as(app.world_mut(), |p| {
                p.skin = (at(0) as u8).min(proto::SKINS.len() as u8 - 1);
                p.headwear = proto::gear_valid(&proto::HEADWEAR, at(1) as u8);
                p.footwear = proto::gear_valid(&proto::FOOTWEAR, at(2) as u8);
                p.emblem = proto::emblem_valid(at(3));
            });
        }
        // What the wallet owns, for a picture of the armory with no server:
        // `--wardrobe skins,head,feet[,floor]`, each list a mask by id.
        if let Some(w) = arg("--wardrobe") {
            let v: Vec<u32> = w.split(',').filter_map(|n| n.trim().parse().ok()).collect();
            let at = |i: usize| v.get(i).copied().unwrap_or(0) | 1;
            app.world_mut()
                .resource_mut::<oneshot_client::party::PartyView>()
                .wardrobe = Some(proto::Wardrobe {
                wallet: true,
                known: true,
                skins: at(0),
                headwear: at(1),
                footwear: at(2),
                floor: v.get(3).map(|f| *f as u16),
            });
        }
        if let Some(t) = arg("--tab") {
            if !oneshot_client::front::open_tab(app.world_mut(), &t) {
                return Err(format!("--tab {t}: play, armory, party, friends or challenges"));
            }
        }
        (app, want, joins)
    };
    if let Some(path) = shot {
        oneshot_client::look::shot::keep_out_of_the_way(&mut app);
        app.insert_resource(oneshot_client::look::shot::Capture::new(
            path, shot_at, want, want_match,
        ));
    }
    if let Some(r) = record {
        app.insert_resource(r);
    }
    match app.run() {
        bevy::app::AppExit::Error(code) => std::process::exit(code.get().into()),
        bevy::app::AppExit::Success => Ok(()),
    }
}

/// `--film` and `--clip`: the list's tape drawn shot by shot, then out; a
/// clip's shots joined into one video beside its tape.
#[cfg(not(target_arch = "wasm32"))]
fn film(
    mut script: oneshot_client::film::Script,
    quality: Option<oneshot_client::look::quality::Quality>,
    cut: Option<(std::path::PathBuf, std::path::PathBuf)>,
) -> Result<(), String> {
    let path = script.tape.clone();
    let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let tape = client_net::tape::read(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    script.anchor(tape.end_ms() / 1000.0);
    if let Some(q) = quality {
        script.quality = q.name().into();
    }
    println!(
        "oneshot: filming {} shot(s) from {} ({:.0} s of tape)",
        script.shots.len(),
        path.display(),
        tape.end_ms() / 1000.0
    );
    let mut app = oneshot_client::film::app(script, tape, None)?;
    if let bevy::app::AppExit::Error(code) = app.run() {
        std::process::exit(code.get().into());
    }
    if let Some((shots, out)) = cut {
        oneshot_client::film::encode::cut(&shots, &out)?;
        let _ = std::fs::remove_dir_all(&shots);
        println!("oneshot: clip {}", out.display());
    }
    Ok(())
}

/// `--scan x.tape [--every S]`: what a tape holds, every few seconds: where
/// the player is, who else it was shown and where, and the kills, which is
/// how a shot list finds its moments.
#[cfg(not(target_arch = "wasm32"))]
fn scan(path: &std::path::Path, every_s: f64) -> Result<(), String> {
    use client_net::session::Session;
    use client_net::tape::Replay;
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let tape = client_net::tape::read(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    let end = tape.end_ms();
    let map = tape.map().and_then(|m| sim::maps::ALL.iter().find(|d| d.id == m));
    println!(
        "{}: {:.1} s on {}",
        path.display(),
        end / 1000.0,
        map.map_or("an unknown map", |d| d.name)
    );
    let mut names: std::collections::BTreeMap<u8, String> = std::collections::BTreeMap::new();
    let mut s = Session::replay();
    let mut r = Replay::new(tape);
    let step = 1000.0 / sim::consts::TICK_HZ as f64 * client_net::core::SEND_EVERY as f64;
    let mut next = 0.0;
    let every = every_s.max(0.1) * 1000.0;
    while r.clock_ms < end + step {
        r.clock_ms += step;
        r.frame(&mut s);
        for m in s.take_messages() {
            match m {
                proto::Msg::Seat { slot, name, .. } => {
                    names.insert(slot, name.as_str().to_string());
                }
                proto::Msg::Kill { killer, victim, .. } => {
                    let who = |k: u8| names.get(&k).cloned().unwrap_or_else(|| format!("seat {k}"));
                    println!("{:7.1} s  {} killed {}", r.clock_ms / 1000.0, who(killer), who(victim));
                }
                _ => {}
            }
        }
        s.core.drain_events(r.clock_ms);
        if r.clock_ms >= next && s.ready {
            next += every;
            let (f, p) = s.core.own();
            let seen: Vec<String> = s
                .core
                .others(r.clock_ms)
                .iter()
                .filter(|d| d.alive)
                .map(|d| format!("{}@({:.0}, {:.0}, {:.0})", d.slot, d.pos.x, d.pos.y, d.pos.z))
                .collect();
            println!(
                "{:7.1} s  seat {} at ({:.1}, {:.1}, {:.1}) {}  {}/{}  sees {}",
                r.clock_ms / 1000.0,
                s.core.slot,
                p.x,
                p.y,
                p.z,
                if f.alive { "alive" } else { "dead " },
                f.kills,
                f.deaths,
                if seen.is_empty() {
                    "nobody".into()
                } else {
                    seen.join(" ")
                }
            );
        }
    }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
const HELP: &str = "oneshot: the desktop client

  oneshot                          the title screen; play on the public server ({PUBLIC})
  oneshot --server 127.0.0.1:8080  play on a local oneshot-server
  --name NAME                      the name others see, this run (the title's field;
                                   the settings file keeps yours)
  --look SKIN,HEAD,FEET,EMBLEM     what you wear this run, by id (the armory's picks)
  --wardrobe SKINS,HEAD,FEET[,FLOOR]  what the wallet owns, each a mask by id, with
                                   no server: for a picture of the armory
  --tab NAME                       the title opens on ARMORY (armory:emblem for a page of it),
                                   PARTY, FRIENDS or CHALLENGES
  --party CODE                     join the party with that code (its leader picks the game)
  --mode MODE                      straight into a match: casual, ffa, gungame, bigteam,
                                   ctf; duel, doubles and cup are ranked, which needs an
                                   Elo Pros sign-in (the web page); watch follows a public
                                   match (--party CODE: that party's). Or 0 to 6, 8, 9
  --autoplay [--scope|--drill]     a bot plays you, straight into a match (casual
                                   unless --mode); --scope holds the scope, --drill
                                   works its gun at a wall (tools/reel.sh)
  --capsules                       everyone else as a capsule in their side's colour
                                   (a software GPU can't skin a room of Triple Ts)
  --lite                           POTATO for this run (your own graphics are kept)
  --potato                         POTATO from now on (SETTINGS > GRAPHICS to change it)
  --quality NAME                   potato, low, medium or high, for this run only
  --pose 'yaw=1.57&arm=3'          no server: one fighter, for a picture
  --pose 'map=6&eye=-20,6,0&look=0,2,0'  no server: a map's view
  --menu, --settings               open the match on the Esc menu or the settings
                                   (--settings alone: over the title)
  --graphics                       the settings, on GRAPHICS
  --windowed                       a window this run, not fullscreen (the setting is kept)
  --fxdemo                         the effects staged in front of the camera, in a
                                   match (look/fx/demo.rs); --fxstage N: its stage N
  --knobs 'Layer.key=v;...'        effects retuned for this run (tools/fx.sh, REEL_KNOBS)
  --shot out.png [--shot-at S]     save one frame once the screen is there (its
                                   level built, its shaders compiled, nothing over
                                   it) and S seconds have passed (1), then exit 0;
                                   non-zero, saying why, if it never is
                                   (ci/desktop_shot.sh)
  --record x.tape SECONDS          keep everything the match sends, and once it
                                   has run SECONDS (or ends) write it as a tape
                                   and exit (ci/film.sh record)
  --film shots.json                draw a tape as a shot list says, each shot
                                   into ffmpeg (ci/film.sh shoot; crates/client/
                                   src/film/script.rs has the fields)
  --clip x.tape [--style S]        make an F9 tape's video beside it (S: wide,
                                   pov, chase, mix, long); needs ffmpeg;
                                   --max-height PX draws it no taller
  --scan x.tape [--every S]        what a tape holds, every S seconds (5)

F12 saves a screenshot on any screen, into your pictures folder under
oneshot/ (ONESHOT_SHOTS_DIR names another). F9 in a match saves the last
half minute there as a tape and, with ffmpeg installed, makes it a video.

Keys (Halo 2's on a PC by default; rebind any of them, the mouse's buttons
and wheel too, with the mouse's speed, the field of view, the volume and
the graphics, in SETTINGS, saved in your config folder under
oneshot/settings.toml): WASD and the mouse, Mouse 1 fires, Mouse 2 zooms
(or, with a gun in each hand, fires the left one), Space jumps, Ctrl
crouches, Q melees, F throws a grenade, 2 switches grenades, R reloads,
E is the action (pick up, get in), 1 or the wheel changes weapon, Tab the
scoreboard, N the net graph, F11 fullscreen, Esc the menu.

The game opens fullscreen (borderless); F11 or SETTINGS > GRAPHICS >
FULLSCREEN, or --windowed for this run.

A slow machine (a software rasterizer) starts on POTATO by itself until you
pick a graphics row.";

#[cfg(not(target_arch = "wasm32"))]
const MODES: &str = "casual, duel, doubles, ffa, gungame, cup, bigteam, ctf, watch, siege (or 0 to 6, 8 to 10)";

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
        "ctf" => proto::MODE_CTF,
        "watch" => proto::MODE_WATCH,
        "siege" => proto::MODE_SIEGE,
        n => {
            return n
                .parse()
                .ok()
                .filter(|&n| n <= proto::MODE_BIG_TEAM || (proto::MODE_CTF..=proto::MODE_SIEGE).contains(&n))
        }
    })
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    #[test]
    fn modes_by_name() {
        assert_eq!(super::mode("Duel"), Some(proto::MODE_DUEL));
        assert_eq!(super::mode("6"), Some(proto::MODE_BIG_TEAM));
        assert_eq!(super::mode("7"), None);
        assert_eq!(super::mode("ctf"), Some(proto::MODE_CTF));
        assert_eq!(super::mode("watch"), Some(proto::MODE_WATCH));
        assert_eq!(super::mode("9"), Some(proto::MODE_WATCH));
        assert_eq!(super::mode("siege"), Some(proto::MODE_SIEGE));
    }
}

#[cfg(target_arch = "wasm32")]
fn main() {}

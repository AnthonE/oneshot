//! Stats, achievements and leaderboards on Elo Pros (`meter/gamestats.py`).
//!
//! The game declares a set once ([`declaration`], committed as
//! `ops/elo-stats.json` and saved on the dev desk), and the server reports
//! what each signed-in player did at the end of every match they played, or
//! when they leave one: `POST /api/gamestats/oneshot/report`, signed with the
//! server key the way ratings are. Elo Pros adds the counts up and unlocks an
//! achievement when its stat reaches its target, so the server never keeps a
//! record of its own. Boards keep each player's best ranked number.
//!
//! A player is their wallet, or their Elo Pros account's `acct_` id when they
//! signed in without one. A guest is nobody and is never reported.

use proto::{medal, MODE_BIG_TEAM, MODE_CASUAL, MODE_CTF, MODE_CUP, MODE_DOUBLES, MODE_DUEL, MODE_FFA, MODE_GUN_GAME};
use room::medals::Tally;
use serde_json::{json, Value};

/// How a stat's reports add up on Elo Pros.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Agg {
    Sum,
    Max,
}

pub struct Stat {
    pub id: &'static str,
    pub name: &'static str,
    pub agg: Agg,
}

const fn sum(id: &'static str, name: &'static str) -> Stat {
    Stat {
        id,
        name,
        agg: Agg::Sum,
    }
}

const fn max(id: &'static str, name: &'static str) -> Stat {
    Stat {
        id,
        name,
        agg: Agg::Max,
    }
}

/// The stats every report draws from, besides one per medal
/// ([`medal_stat`]).
pub const STATS: &[Stat] = &[
    sum("matches", "Matches finished"),
    sum("wins", "Wins"),
    sum("ranked_matches", "Ranked matches finished"),
    sum("ranked_wins", "Ranked wins"),
    sum("bot_wins", "Wins against the bot"),
    sum("duel_wins", "Ranked Duel wins"),
    sum("doubles_wins", "Ranked Doubles wins"),
    sum("ffa_wins", "Free for all wins"),
    sum("gun_game_wins", "Gun Game wins"),
    sum("big_team_wins", "Big Team wins"),
    sum("ctf_wins", "Capture the Flag wins"),
    sum("captures", "Flags captured"),
    sum("flag_returns", "Flags returned"),
    sum("cups_played", "Cups finished"),
    sum("cup_wins", "Cups won"),
    sum("kills", "Kills"),
    sum("deaths", "Deaths"),
    sum("assists", "Assists"),
    sum("damage", "Damage done"),
    sum("seconds_played", "Seconds played"),
    max("most_kills", "Most kills in a match"),
    max("best_spree", "Longest spree"),
    max("best_multi", "Longest multi-kill"),
];

/// A medal's stat: how many times it was earned.
pub fn medal_stat(id: u8) -> String {
    format!("medal.{}", medal::slug(id))
}

/// A daily or weekly challenge (Elo Pros' `challenges.py`): a goal over one
/// of our stats, counted from the same reports. Elo Pros picks each day's
/// and week's from this pool and says what they pay; we only declare them.
pub struct Challenge {
    pub id: &'static str,
    pub name: &'static str,
    pub desc: &'static str,
    /// A sum stat (added up over the period) or a max (reached in a match).
    pub stat: &'static str,
    pub goal: u64,
    pub week: bool,
}

const fn day(id: &'static str, name: &'static str, desc: &'static str, stat: &'static str, goal: u64) -> Challenge {
    Challenge {
        id,
        name,
        desc,
        stat,
        goal,
        week: false,
    }
}

const fn week(id: &'static str, name: &'static str, desc: &'static str, stat: &'static str, goal: u64) -> Challenge {
    Challenge {
        id,
        name,
        desc,
        stat,
        goal,
        week: true,
    }
}

pub const CHALLENGES: &[Challenge] = &[
    day("kills_25", "Twenty-five down", "Get 25 kills today", "kills", 25),
    day("matches_3", "Three in", "Finish 3 matches today", "matches", 3),
    day("win_1", "Take one", "Win a match today", "wins", 1),
    day("assists_8", "Set them up", "Get 8 assists today", "assists", 8),
    day(
        "spree_5",
        "On a roll",
        "Kill 5 without dying in one life",
        "best_spree",
        5,
    ),
    day("double_1", "Two at once", "Get a Double Kill", "medal.double_kill", 1),
    day(
        "heads_10",
        "Aim high",
        "Get 10 headshot kills today",
        "medal.headshot",
        10,
    ),
    day(
        "capture_1",
        "Bring it home",
        "Capture a flag in Capture the Flag",
        "captures",
        1,
    ),
    day("melee_5", "Up close", "Get 5 Beat Downs today", "medal.beat_down", 5),
    week("kills_200", "Two hundred", "Get 200 kills this week", "kills", 200),
    week("wins_10", "Ten wins", "Win 10 matches this week", "wins", 10),
    week("matches_20", "Regular", "Finish 20 matches this week", "matches", 20),
    week("captures_5", "Flag runner", "Capture 5 flags this week", "captures", 5),
    week(
        "returns_5",
        "Home guard",
        "Return your own flag 5 times this week",
        "flag_returns",
        5,
    ),
    week("triple_1", "Three at once", "Get a Triple Kill", "best_multi", 3),
    week("sticks_10", "Stuck", "Get 10 sticky kills this week", "medal.stick", 10),
];

pub struct Achievement {
    pub id: &'static str,
    pub name: &'static str,
    pub desc: &'static str,
    pub hidden: bool,
    /// A stem of Elo Pros' `art/achievements/oneshot/`.
    pub icon: &'static str,
    /// Unlocks when this stat reaches `max`.
    pub stat: &'static str,
    pub max: u64,
}

const fn ach(
    id: &'static str,
    name: &'static str,
    desc: &'static str,
    icon: &'static str,
    stat: &'static str,
    max: u64,
) -> Achievement {
    Achievement {
        id,
        name,
        desc,
        hidden: false,
        icon,
        stat,
        max,
    }
}

const fn hidden(a: Achievement) -> Achievement {
    Achievement { hidden: true, ..a }
}

/// Every achievement unlocks itself on Elo Pros when its stat gets there,
/// so a report never names one. Never remove or rename a row someone
/// holds: drop it from the list and Elo Pros keeps it as archived on the
/// profiles that have it.
pub const ACHIEVEMENTS: &[Achievement] = &[
    ach("FIRST_MATCH", "Boots on", "Finish a match", "gamepad", "matches", 1),
    ach("FIRST_WIN", "On the board", "Win a match", "trophy", "wins", 1),
    ach("WINS_25", "Regular", "Win 25 matches", "laurels", "wins", 25),
    ach("WINS_100", "Hundred up", "Win 100 matches", "podium", "wins", 100),
    ach(
        "BOT_WIN",
        "Beat the bot",
        "Win a match against the bot",
        "target",
        "bot_wins",
        1,
    ),
    ach(
        "FFA_WIN",
        "Last one standing",
        "Win a free for all",
        "star",
        "ffa_wins",
        1,
    ),
    ach(
        "GUN_GAME_WIN",
        "Through the ladder",
        "Win a game of Gun Game",
        "revolver",
        "gun_game_wins",
        1,
    ),
    ach(
        "BIG_TEAM_WIN",
        "Big win",
        "Win a Big Team match",
        "flag",
        "big_team_wins",
        1,
    ),
    ach(
        "CTF_WIN",
        "Flag day",
        "Win a game of Capture the Flag",
        "flag",
        "ctf_wins",
        1,
    ),
    ach(
        "CAPTURE_1",
        "Home run",
        "Carry the enemy's flag home",
        "arrowhead",
        "captures",
        1,
    ),
    ach(
        "CAPTURE_25",
        "Flag runner",
        "Capture 25 flags",
        "spears",
        "captures",
        25,
    ),
    ach(
        "RETURN_10",
        "Not today",
        "Return your own flag 10 times",
        "campfire",
        "flag_returns",
        10,
    ),
    ach(
        "RANKED_FIRST",
        "Rated",
        "Finish a ranked match",
        "upgrade",
        "ranked_matches",
        1,
    ),
    ach(
        "RANKED_WIN",
        "First rated win",
        "Win a ranked match",
        "medal",
        "ranked_wins",
        1,
    ),
    ach(
        "RANKED_WINS_50",
        "Climber",
        "Win 50 ranked matches",
        "upgrade",
        "ranked_wins",
        50,
    ),
    ach("DUEL_WIN", "One on one", "Win a Ranked Duel", "spears", "duel_wins", 1),
    ach(
        "DOUBLES_WIN",
        "Two on two",
        "Win a Ranked Doubles match",
        "crowd",
        "doubles_wins",
        1,
    ),
    ach("CUP_ENTER", "Cup run", "Play a cup to the end", "cup", "cups_played", 1),
    ach("CUP_WIN", "Cup winner", "Win a cup", "cup", "cup_wins", 1),
    ach("KILLS_100", "Hundred down", "Get 100 kills", "target", "kills", 100),
    ach(
        "KILLS_1000",
        "Thousand down",
        "Get 1,000 kills",
        "target",
        "kills",
        1000,
    ),
    hidden(ach(
        "KILLS_10000",
        "Ten thousand down",
        "Get 10,000 kills",
        "target",
        "kills",
        10_000,
    )),
    ach("ASSISTS_100", "Wingman", "Get 100 assists", "crowd", "assists", 100),
    ach(
        "SPREE_5",
        "Killing Spree",
        "Get 5 kills without dying",
        "embers",
        "best_spree",
        5,
    ),
    ach(
        "SPREE_10",
        "Killing Frenzy",
        "Get 10 kills without dying",
        "embers",
        "best_spree",
        10,
    ),
    ach(
        "SPREE_15",
        "Running Riot",
        "Get 15 kills without dying",
        "embers",
        "best_spree",
        15,
    ),
    hidden(ach(
        "SPREE_25",
        "Untouchable",
        "Get 25 kills without dying",
        "embers",
        "best_spree",
        25,
    )),
    ach(
        "MULTI_3",
        "Triple Kill",
        "Get three kills, each within four seconds of the last",
        "bullets",
        "best_multi",
        3,
    ),
    ach(
        "MULTI_4",
        "Overkill",
        "Get four kills, each within four seconds of the last",
        "bullets",
        "best_multi",
        4,
    ),
    hidden(ach(
        "MULTI_5",
        "Killstorm",
        "Get five kills, each within four seconds of the last",
        "bullets",
        "best_multi",
        5,
    )),
    ach(
        "PERFECT",
        "Perfect",
        "Make a precision kill in the gun's fewest shots, none missed",
        "arrowhead",
        "medal.perfect",
        1,
    ),
    hidden(ach(
        "PERFECTION",
        "Perfection",
        "Win with 15 or more kills and no deaths",
        "star",
        "medal.perfection",
        1,
    )),
    ach(
        "NO_SCOPE",
        "No Scope",
        "Kill with a sniper or beam rifle without its scope up",
        "arrowhead",
        "medal.no_scope",
        1,
    ),
    ach(
        "NO_SCOPE_360",
        "360 No Scope",
        "Turn a full circle and kill with a sniper or beam rifle without its scope up",
        "energy",
        "medal.360_no_scope",
        1,
    ),
    ach(
        "SPLATTER_10",
        "Road rage",
        "Run down 10 people with a vehicle",
        "energy",
        "medal.splatter",
        10,
    ),
    ach(
        "STICK_25",
        "Sticky fingers",
        "Kill 25 people with a sticky grenade stuck to them",
        "dynamite",
        "medal.stick",
        25,
    ),
    ach(
        "ASSASSIN_10",
        "Quiet work",
        "Kill 10 people from behind with a melee",
        "hatchet",
        "medal.assassination",
        10,
    ),
    ach(
        "FROM_THE_GRAVE",
        "From the grave",
        "Make a kill after you've died",
        "meteor",
        "medal.kill_from_the_grave",
        1,
    ),
    ach(
        "HAIL_MARY",
        "Hail Mary",
        "Make a grenade kill from far off",
        "dynamite",
        "medal.hail_mary",
        1,
    ),
    ach(
        "HOURS_1",
        "An hour in",
        "Play for an hour",
        "campfire",
        "seconds_played",
        3600,
    ),
    ach(
        "HOURS_10",
        "Ten hours in",
        "Play for ten hours",
        "campfire",
        "seconds_played",
        36_000,
    ),
    ach(
        "HOURS_100",
        "A hundred hours in",
        "Play for a hundred hours",
        "campfire",
        "seconds_played",
        360_000,
    ),
];

pub struct Board {
    pub id: &'static str,
    pub name: &'static str,
    /// Lowest first (a time, a place) rather than highest.
    pub asc: bool,
    pub time_ms: bool,
}

/// One score per player: their best.
pub const BOARDS: &[Board] = &[
    Board {
        id: "ranked_kills",
        name: "Most kills in a ranked match",
        asc: false,
        time_ms: false,
    },
    Board {
        id: "ranked_spree",
        name: "Longest spree in a ranked match",
        asc: false,
        time_ms: false,
    },
    Board {
        id: "gun_game_ms",
        name: "Fastest Gun Game win",
        asc: true,
        time_ms: true,
    },
    Board {
        id: "cup_place",
        name: "Best cup finish",
        asc: true,
        time_ms: false,
    },
];

/// The set as Elo Pros' dev desk saves it (act `gamestats`, or the `stats`
/// block of an `elo.json`). `ops/elo-stats.json` is this, written by
/// `oneshot-server --print-stats`.
pub fn declaration() -> Value {
    let mut stats: Vec<Value> = STATS
        .iter()
        .map(|s| json!({"id": s.id, "name": s.name, "agg": match s.agg { Agg::Sum => "sum", Agg::Max => "max" }}))
        .collect();
    for id in 1..medal::COUNT as u8 {
        stats.push(json!({"id": medal_stat(id), "name": medal::info(id).name, "agg": "sum"}));
    }
    let achievements: Vec<Value> = ACHIEVEMENTS
        .iter()
        .map(|a| {
            json!({"id": a.id, "name": a.name, "desc": a.desc, "hidden": a.hidden, "icon": a.icon,
                   "progress": {"stat": a.stat, "max": a.max}})
        })
        .collect();
    let boards: Vec<Value> = BOARDS
        .iter()
        .map(|b| {
            json!({"id": b.id, "name": b.name, "sort": if b.asc { "asc" } else { "desc" }, "keep": "best",
                   "format": if b.time_ms { "time_ms" } else { "number" }})
        })
        .collect();
    let challenges: Vec<Value> = CHALLENGES
        .iter()
        .map(|c| {
            json!({"id": c.id, "name": c.name, "desc": c.desc, "stat": c.stat, "goal": c.goal,
                   "every": if c.week { "week" } else { "day" }})
        })
        .collect();
    json!({"stats": stats, "achievements": achievements, "boards": boards, "challenges": challenges})
}

/// One person's part in one match, as the sim thread saw it.
#[derive(Clone, Debug)]
pub struct Played {
    /// The wallet, or the account's `acct_` id.
    pub player: String,
    pub mode: u8,
    pub ranked: bool,
    /// Played to the end (a leaver's counts still count, not the match).
    pub finished: bool,
    pub won: bool,
    /// From the seat being taken to the end or the leave.
    pub ms: u64,
    pub tally: Tally,
}

/// A cup's end, for one entrant.
#[derive(Clone, Debug)]
pub struct Cupped {
    pub player: String,
    pub place: u32,
}

/// What the server sends: matches and cups, one report each.
#[derive(Clone, Debug)]
pub enum Line {
    Match(Played),
    Cup(Cupped),
}

fn match_report(p: &Played) -> Value {
    let t = &p.tally;
    let mut stats = serde_json::Map::new();
    let mut put = |k: &str, v: u64| {
        if v > 0 {
            stats.insert(k.into(), v.into());
        }
    };
    put("matches", p.finished as u64);
    put("wins", p.won as u64);
    put("ranked_matches", (p.ranked && p.finished) as u64);
    put("ranked_wins", (p.ranked && p.won) as u64);
    if p.won {
        let by_mode = match p.mode {
            MODE_CASUAL => Some("bot_wins"),
            MODE_DUEL => Some("duel_wins"),
            MODE_DOUBLES => Some("doubles_wins"),
            MODE_FFA => Some("ffa_wins"),
            MODE_GUN_GAME => Some("gun_game_wins"),
            MODE_BIG_TEAM => Some("big_team_wins"),
            MODE_CTF => Some("ctf_wins"),
            // A cup heat is counted with its cup.
            _ => None,
        };
        if let Some(k) = by_mode {
            put(k, 1);
        }
    }
    put("kills", t.kills as u64);
    put("deaths", t.deaths as u64);
    put("assists", t.assists as u64);
    put("damage", t.damage as u64);
    put("seconds_played", p.ms / 1000);
    put("most_kills", t.kills as u64);
    put("best_spree", t.best_spree as u64);
    put("best_multi", t.best_multi as u64);
    put("captures", t.captures as u64);
    put("flag_returns", t.returns as u64);
    for (id, &n) in t.medals.iter().enumerate().skip(1) {
        put(&medal_stat(id as u8), n as u64);
    }
    let mut scores = serde_json::Map::new();
    if p.ranked && p.mode != MODE_CUP {
        if t.kills > 0 {
            scores.insert("ranked_kills".into(), (t.kills as u64).into());
        }
        if t.best_spree > 0 {
            scores.insert("ranked_spree".into(), (t.best_spree as u64).into());
        }
    }
    if p.mode == MODE_GUN_GAME && p.won && p.ms > 0 {
        scores.insert("gun_game_ms".into(), p.ms.into());
    }
    let mut r = json!({"wallet": p.player, "stats": stats});
    if !scores.is_empty() {
        r["scores"] = scores.into();
    }
    r
}

fn cup_report(c: &Cupped) -> Value {
    let mut stats = serde_json::Map::new();
    stats.insert("cups_played".into(), 1.into());
    if c.place == 1 {
        stats.insert("cup_wins".into(), 1.into());
    }
    json!({"wallet": c.player, "stats": stats, "scores": {"cup_place": c.place}})
}

/// The body of one report call (`{"reports": [...]}`), at most
/// [`MAX_REPORTS`] players.
pub fn report_body(lines: &[Line]) -> String {
    let reports: Vec<Value> = lines
        .iter()
        .map(|l| match l {
            Line::Match(p) => match_report(p),
            Line::Cup(c) => cup_report(c),
        })
        .collect();
    json!({ "reports": reports }).to_string()
}

/// Elo Pros' cap on players in one call (`gamestats.MAX_REPORTS`).
pub const MAX_REPORTS: usize = 500;

/// Whether `id` is a player Elo Pros keeps a record for: a wallet, or an
/// account's own id (`gamestats._PLAYER_RE`).
pub fn is_player(id: &str) -> bool {
    let hex = |s: &str, n: usize| s.len() == n && s.bytes().all(|b| b.is_ascii_hexdigit());
    id.strip_prefix("0x").is_some_and(|h| hex(h, 40)) || id.strip_prefix("acct_").is_some_and(|h| hex(h, 20))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn stat_ok(id: &str) -> bool {
        (1..=64).contains(&id.len())
            && id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_.:".contains(&b))
    }

    #[test]
    fn the_set_fits_what_elo_pros_takes() {
        let d = declaration();
        let stats = d["stats"].as_array().unwrap();
        let achs = d["achievements"].as_array().unwrap();
        let boards = d["boards"].as_array().unwrap();
        // gamestats.MAX_STATS, MAX_ACHIEVEMENTS, MAX_BOARDS
        assert!(stats.len() <= 100 && achs.len() <= 500 && boards.len() <= 32);
        let ids: BTreeSet<&str> = stats.iter().map(|s| s["id"].as_str().unwrap()).collect();
        assert_eq!(ids.len(), stats.len(), "a stat declared twice");
        for s in stats {
            let id = s["id"].as_str().unwrap();
            assert!(stat_ok(id), "{id}");
            assert!(s["name"].as_str().unwrap().len() <= 48);
        }
        let mut seen = BTreeSet::new();
        for a in achs {
            let id = a["id"].as_str().unwrap();
            assert!(seen.insert(id), "{id} twice");
            assert!(id
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_'));
            assert!(a["name"].as_str().unwrap().len() <= 48 && a["desc"].as_str().unwrap().len() <= 200);
            let icon = a["icon"].as_str().unwrap();
            assert!(icon
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'));
            let st = a["progress"]["stat"].as_str().unwrap();
            assert!(ids.contains(st), "{id} counts {st}, which isn't declared");
            assert!(a["progress"]["max"].as_u64().unwrap() > 0);
        }
        // A board id isn't a field every board row has (gamestats.ROW_FIELDS)
        // nor a key Elo Pros refuses (leaderboard.REFUSED_KEYS).
        let fields = [
            "wallet", "at", "agent", "rank", "name", "place", "harness", "model", "key",
        ];
        let refused = [
            "meter",
            "meter_score",
            "meter_number",
            "profile",
            "scry",
            "elo",
            "reserve",
            "balance",
            "holdings",
        ];
        for b in boards {
            let id = b["id"].as_str().unwrap();
            assert!(stat_ok(id) && !fields.contains(&id) && !refused.contains(&id), "{id}");
        }
        // Challenges (challenges.validate): over a declared sum or max stat,
        // by day or week, each id once.
        let aggs: std::collections::BTreeMap<&str, &str> = stats
            .iter()
            .map(|s| (s["id"].as_str().unwrap(), s["agg"].as_str().unwrap()))
            .collect();
        let chs = d["challenges"].as_array().unwrap();
        assert!(chs.len() <= 64);
        let mut seen = BTreeSet::new();
        for c in chs {
            let id = c["id"].as_str().unwrap();
            assert!(seen.insert(id), "{id} twice");
            assert!(
                (1..=48).contains(&id.len())
                    && id
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
            );
            assert!((1..=48).contains(&c["name"].as_str().unwrap().len()) && c["desc"].as_str().unwrap().len() <= 200);
            let st = c["stat"].as_str().unwrap();
            assert!(matches!(aggs.get(st), Some(&"sum") | Some(&"max")), "{id} counts {st}");
            assert!(c["goal"].as_u64().unwrap() >= 1);
            assert!(matches!(c["every"].as_str(), Some("day" | "week")));
        }
    }

    #[test]
    fn the_committed_set_is_this_one() {
        // `oneshot-server --print-stats > ops/elo-stats.json` after a change,
        // then save it on the dev desk.
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../ops/elo-stats.json");
        let text = std::fs::read_to_string(path).expect("ops/elo-stats.json");
        let committed: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(
            committed,
            declaration(),
            "ops/elo-stats.json is stale: oneshot-server --print-stats"
        );
    }

    fn full_tally() -> Tally {
        let mut t = Tally {
            kills: 15,
            deaths: 3,
            assists: 4,
            damage: 2400,
            best_spree: 11,
            best_multi: 4,
            captures: 2,
            returns: 1,
            ..Tally::default()
        };
        for m in t.medals.iter_mut().skip(1) {
            *m = 2;
        }
        t
    }

    #[test]
    fn a_report_names_only_what_is_declared() {
        // Elo Pros refuses a whole call naming one undeclared id: every mode,
        // won or not, finished or not, and a cup's end.
        let d = declaration();
        let stats: BTreeSet<String> = d["stats"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["id"].as_str().unwrap().into())
            .collect();
        let boards: BTreeSet<String> = d["boards"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["id"].as_str().unwrap().into())
            .collect();
        let mut lines = vec![Line::Cup(Cupped {
            player: "acct_0123456789abcdef0123".into(),
            place: 1,
        })];
        for mode in [
            MODE_CASUAL,
            MODE_DUEL,
            MODE_DOUBLES,
            MODE_FFA,
            MODE_GUN_GAME,
            MODE_CUP,
            MODE_BIG_TEAM,
            MODE_CTF,
        ] {
            for (won, finished) in [(true, true), (false, true), (false, false)] {
                lines.push(Line::Match(Played {
                    player: "0x00000000000000000000000000000000000000aa".into(),
                    mode,
                    ranked: matches!(mode, MODE_DUEL | MODE_DOUBLES | MODE_CUP),
                    finished,
                    won,
                    ms: 300_000,
                    tally: full_tally(),
                }));
            }
        }
        let body: Value = serde_json::from_str(&report_body(&lines)).unwrap();
        let reps = body["reports"].as_array().unwrap();
        assert_eq!(reps.len(), lines.len());
        let mut used = BTreeSet::new();
        for r in reps {
            assert!(is_player(r["wallet"].as_str().unwrap()));
            for (k, v) in r["stats"].as_object().unwrap() {
                assert!(stats.contains(k), "{k} isn't declared");
                assert!(v.as_u64().unwrap() > 0, "{k}: a zero is left out");
                used.insert(k.clone());
            }
            for (k, _) in r["scores"].as_object().into_iter().flatten() {
                assert!(boards.contains(k), "board {k} isn't declared");
                used.insert(k.clone());
            }
        }
        // And everything declared is something a report can say.
        for k in stats.iter().chain(&boards) {
            assert!(used.contains(k), "{k} is declared and never reported");
        }
    }

    #[test]
    fn a_win_counts_once_by_mode_and_a_leaver_no_match() {
        let p = Played {
            player: "0x00000000000000000000000000000000000000aa".into(),
            mode: MODE_DUEL,
            ranked: true,
            finished: true,
            won: true,
            ms: 61_500,
            tally: Tally::default(),
        };
        let r = match_report(&p);
        assert_eq!(r["stats"]["wins"], 1);
        assert_eq!(r["stats"]["ranked_wins"], 1);
        assert_eq!(r["stats"]["duel_wins"], 1);
        assert_eq!(r["stats"]["seconds_played"], 61);
        assert!(r.get("scores").is_none(), "no kills, no spree: nothing for a board");
        let left = match_report(&Played {
            finished: false,
            won: false,
            ..p
        });
        assert!(left["stats"].get("matches").is_none() && left["stats"].get("ranked_matches").is_none());
        assert_eq!(left["stats"]["seconds_played"], 61);
    }

    #[test]
    fn players_are_wallets_and_accounts() {
        assert!(is_player("0x00000000000000000000000000000000000000aa"));
        assert!(is_player("acct_0123456789abcdef0123"));
        assert!(!is_player("acct_0123"));
        assert!(!is_player("player"));
    }
}

//! `oneshot-aim`: aim windows for the cheat detector (docs/ANTICHEAT.md §5,
//! §7's stage 0). Plays bot matches, or replays, and at each sighting (an
//! enemy coming into a seat's plain sight) forks the match: one branch
//! plays on as it was, the other with one of the zoo's assists
//! (`room::assist`) laid over that seat's input. Both branches' windows
//! (`room::window`) are written, so a detector learns the same player, the
//! same match and the same moment with and without the cheat. Bots are not
//! people: what is learned from bot matches is plumbing, not a result.
//!
//! ```text
//! oneshot-aim zoo [n] [--seed s]          # n assists, an oa1: line each, and what each does
//! oneshot-aim knobs <oa1:…>               # what one does
//! oneshot-aim bots --out dir [--matches 40] [--seed 1] [--secs 300]
//!     [--modes duel,doubles,ffa] [--maps 0,1,2,5,6,7] [--zoo file | --zoo-n 64] [--threads 2]
//! oneshot-aim replays --out dir [--zoo file | --zoo-n 64] [--threads 2] <file.osr>…
//!     either: [--sightings 400] a match at most; [--each] every assist on every
//!     sighting, not one drawn for each (the zoo's arms race, `zoo.py`)
//!     bots: [--hands] a person's hand on each bot's aim while it fights (`hand.rs`)
//! ```
//!
//! `dir` gets `windows.npy` (f32: windows × `LEN` × `F`), `meta.npy` (i32:
//! windows × [`META`]) and `info.json` (the features, meta's columns, the
//! zoo, the matches). A sighting's windows are neighbours with one pair
//! id: the clean one first, then its cheated twins. Keep `dir` off the
//! repo (traces/ is ignored): §8.
//!
//! The matches are stepped on `World` directly, as `room::replay::verify`
//! does; nothing here is a live match or writes a replay.

mod hand;

use std::fs::File;
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use hand::Hand;
use room::assist::{Assist, Knobs, PREFIX};
use room::fog::{Fog, Plain};
use room::genome::Genome;
use room::replay::{Cursor, Next};
use room::window::{Tape, F, FEATURES, LEN, PRE};
use sim::bot::Bot;
use sim::consts::{ticks, MAX_FIGHTERS, TICK_HZ};
use sim::events::Kind as Ev;
use sim::input::Input;
use sim::world::{Lag, Phase, Rules, World};

/// Fog's look ahead for every seat, seconds: a person on a good line.
const LEAD: f32 = 0.1;
/// Sightings cut from one match, at most (`--sightings`).
const MAX_SIGHTINGS: usize = 400;

/// `meta.npy`'s columns: 1 for the cheated twin; the pair and the match;
/// the viewer's gun as the window began (an arm id) and the map; the zoo's
/// assist on the cheated twin (−1 on the clean) and its style (a place in
/// [`STYLES`]); the viewer; a bot's skill × 1000 (−1 for a person); the
/// tick the enemy came into plain sight (row `PRE`); ticks the assist
/// helped; and the viewer's hits, kills and head hits in the window.
const META: [&str; 14] = [
    "label", "pair", "match", "gun", "map", "cheat", "style", "viewer", "skill", "sight", "helped", "hits", "kills",
    "heads",
];
const STYLES: [&str; 5] = ["blatant", "closet", "smooth", "trigger", "walls"];

/// Where a match's inputs come from.
trait Feed: Clone + Send {
    /// The next tick's inputs and lag (seat changes on the way applied to
    /// `w`); `None` at the end.
    fn next(&mut self, w: &mut World) -> Option<([Input; MAX_FIGHTERS], Lag)>;
    /// The aim seat `s`'s input this tick turned from.
    fn from(&self, w: &World, s: usize) -> (u16, i16);
    /// A bot's skill, or −1.
    fn skill(&self, s: usize) -> f32;
}

/// Bots in every seat, thinking on whatever world they are in: in a
/// cheated branch they answer the cheat.
#[derive(Clone)]
struct Bots {
    bots: [Bot; MAX_FIGHTERS],
    skills: [f32; MAX_FIGHTERS],
    n: usize,
    end: u32,
    /// A person's hand on each seat's aim (`--hands`), or the bots' own.
    hands: Option<Vec<Hand>>,
}

impl Feed for Bots {
    fn next(&mut self, w: &mut World) -> Option<([Input; MAX_FIGHTERS], Lag)> {
        if w.phase == Phase::Over || w.tick >= self.end {
            return None;
        }
        let mut inputs = [Input::default(); MAX_FIGHTERS];
        for (s, i) in inputs.iter_mut().enumerate().take(self.n) {
            *i = self.bots[s].think(w, s);
            if let Some(h) = &mut self.hands {
                h[s].steer(w, s, self.bots[s].target(), i);
            }
        }
        Some((inputs, Lag::past([0; MAX_FIGHTERS])))
    }

    /// A bot turns from where its view is.
    fn from(&self, w: &World, s: usize) -> (u16, i16) {
        (w.fighters[s].yaw, w.fighters[s].pitch)
    }

    fn skill(&self, s: usize) -> f32 {
        self.skills[s]
    }
}

/// A replay's inputs as recorded: in a cheated branch everyone else does
/// what they did, whatever the cheat changed.
#[derive(Clone, Copy)]
struct Recorded<'a> {
    c: Cursor<'a>,
    prev: [Input; MAX_FIGHTERS],
}

impl Feed for Recorded<'_> {
    fn next(&mut self, w: &mut World) -> Option<([Input; MAX_FIGHTERS], Lag)> {
        self.prev = self.c.inputs;
        match self.c.next(w) {
            Ok(Next::Tick) => Some((self.c.inputs, self.c.lag)),
            _ => None,
        }
    }

    /// A person turns from where their last input left the view.
    fn from(&self, _: &World, s: usize) -> (u16, i16) {
        (self.prev[s].yaw, self.prev[s].pitch)
    }

    fn skill(&self, _: usize) -> f32 {
        -1.0
    }
}

/// A match being played: the world, its inputs and each seat's fog.
#[derive(Clone)]
struct Run<Fd: Feed> {
    w: World,
    feed: Fd,
    fogs: Vec<Fog>,
}

impl<Fd: Feed> Run<Fd> {
    fn new(w: World, feed: Fd) -> Self {
        let n = w.rules.seats as usize;
        Run {
            w,
            feed,
            fogs: vec![Fog::default(); n],
        }
    }

    /// One tick; false at the match's end. `assist` bends its seat's
    /// input (`helped`: it did); `tape` records the tick.
    fn step(&mut self, assist: Option<&mut Assist>, tape: Option<&mut Tape>, helped: &mut bool) -> bool {
        let next = self.w.tick + 1;
        for (s, f) in self.fogs.iter_mut().enumerate() {
            f.update(&self.w, s, LEAD, next);
        }
        let Some((mut inputs, lag)) = self.feed.next(&mut self.w) else {
            return false;
        };
        if let Some(a) = assist {
            let v = a.seat();
            let from = self.feed.from(&self.w, v);
            let fog = &self.fogs[v];
            *helped = a.bend(
                &self.w,
                &mut Plain::default(),
                |t| fog.shows(t, next),
                from,
                &mut inputs[v],
            );
        }
        self.w.step_with(&inputs, &lag);
        if let Some(tp) = tape {
            let (v, t) = (tp.viewer, tp.target);
            let plain = Plain::default().sees(&self.w, v, t);
            let shown = self.fogs[v].shows(t, self.w.tick);
            tp.record(&self.w, plain, shown, &inputs[v]);
        }
        true
    }
}

/// Seat `t` is an enemy of seat `v`, standing.
fn enemy(w: &World, v: usize, t: usize) -> bool {
    let (me, them) = (&w.fighters[v], &w.fighters[t]);
    t != v && me.seated && them.seated && them.active() && !(w.rules.teams && them.team == me.team)
}

/// Every sighting in a match, in order: (tick, viewer, enemy), as the aim
/// review counts them (`room::watch`).
fn sightings<Fd: Feed>(mut run: Run<Fd>) -> Vec<(u32, usize, usize)> {
    let gap = ticks(room::watch::SIGHT_GAP_MS);
    let n = run.w.rules.seats as usize;
    let mut last = [[None::<u32>; MAX_FIGHTERS]; MAX_FIGHTERS];
    let mut out = Vec::new();
    while run.step(None, None, &mut false) {
        let w = &run.w;
        if w.phase != Phase::Playing {
            continue;
        }
        let mut plain = Plain::default();
        for (v, row) in last.iter_mut().enumerate().take(n) {
            for (t, last) in row.iter_mut().enumerate().take(n) {
                if !w.fighters[v].active() || !enemy(w, v, t) {
                    *last = None;
                    continue;
                }
                if plain.sees(w, v, t) {
                    if last.is_none_or(|l| w.tick - l > gap) {
                        out.push((w.tick, v, t));
                    }
                    *last = Some(w.tick);
                }
            }
        }
    }
    out
}

/// One branch's window and what the viewer did in it.
struct Branch {
    rows: Vec<[f32; F]>,
    helped: i32,
    hits: i32,
    kills: i32,
    heads: i32,
}

/// Plays `run` on for a window of `v` at `t`, with `assist` if any.
fn branch<Fd: Feed>(mut run: Run<Fd>, v: usize, t: usize, mut assist: Option<Assist>) -> Option<Branch> {
    let mut tape = Tape::new(v, t);
    let mut b = Branch {
        rows: Vec::new(),
        helped: 0,
        hits: 0,
        kills: 0,
        heads: 0,
    };
    while !tape.full() {
        let mut helped = false;
        if !run.step(assist.as_mut(), Some(&mut tape), &mut helped) {
            return None;
        }
        b.helped += helped as i32;
        for e in run.w.events.as_slice() {
            match e.kind {
                Ev::Hit if e.a as usize == v && e.b as usize != v => {
                    b.hits += 1;
                    b.heads += (e.part == 2 || e.part == 3) as i32;
                }
                Ev::Kill if e.a as usize == v && e.b as usize != v => b.kills += 1,
                _ => {}
            }
        }
    }
    b.rows = tape.rows;
    Some(b)
}

/// A sighting's windows, ready to write: as played, and with each assist
/// laid on.
struct Sighting {
    clean: Branch,
    /// The zoo's place, the style's, and the window.
    cheats: Vec<(usize, i32, Branch)>,
    /// gun, map, viewer, skill, sight
    at: [i32; 5],
}

fn mix64(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// Up to `cap` sightings of one match, spread over it, each forked: a
/// second pass forks at each. Each gets one assist from the zoo, or with
/// `each`, every assist on the same moment (the zoo's arms race compares
/// them there, `zoo.py`).
fn cut<Fd: Feed>(start: Run<Fd>, zoo: &[Knobs], mseed: u64, each: bool, cap: usize) -> Vec<Sighting> {
    let mut sights = sightings(start.clone());
    if sights.len() > cap {
        // Spread over the match, not its first minutes.
        let every = sights.len() as f64 / cap as f64;
        sights = (0..cap).map(|i| sights[(i as f64 * every) as usize]).collect();
    }
    let style = |k: usize| STYLES.iter().position(|&s| s == zoo[k].style()).unwrap_or(0) as i32;
    let mut run = start;
    let mut out = Vec::new();
    let mut i = 0;
    loop {
        'sight: while let Some(&(st, v, t)) = sights.get(i) {
            let fork = st.saturating_sub(PRE as u32 + 1);
            if fork > run.w.tick {
                break;
            }
            i += 1;
            if fork < run.w.tick {
                continue;
            }
            let h = mix64(mseed ^ (i as u64) << 20);
            let ks: Vec<usize> = if each {
                (0..zoo.len()).collect()
            } else {
                vec![(h % zoo.len() as u64) as usize]
            };
            let Some(clean) = branch(run.clone(), v, t, None) else {
                continue;
            };
            let mut cheats = Vec::with_capacity(ks.len());
            for k in ks {
                let assist = Assist::new(zoo[k], v, (h >> 32) as u32);
                let Some(b) = branch(run.clone(), v, t, Some(assist)) else {
                    continue 'sight;
                };
                cheats.push((k, style(k), b));
            }
            let gun = run.w.fighters[v].gun().arm as i32;
            let skill = (run.feed.skill(v) * 1000.0) as i32;
            out.push(Sighting {
                clean,
                cheats,
                at: [gun, run.w.map.id as i32, v as i32, skill, st as i32],
            });
        }
        if i >= sights.len() || !run.step(None, None, &mut false) {
            break;
        }
    }
    out
}

/// A `.npy` file written as it goes, its count put in its header at the
/// end.
struct Npy {
    f: BufWriter<File>,
    descr: &'static str,
    tail: Vec<usize>,
    n: usize,
}

const HEADER: usize = 128;

impl Npy {
    fn create(path: &Path, descr: &'static str, tail: Vec<usize>) -> Npy {
        let f = File::create(path).unwrap_or_else(|e| die(&format!("{}: {e}", path.display())));
        let mut npy = Npy {
            f: BufWriter::new(f),
            descr,
            tail,
            n: 0,
        };
        let h = npy.header();
        npy.f.write_all(&h).unwrap();
        npy
    }

    fn header(&self) -> Vec<u8> {
        let mut dims = vec![self.n.to_string()];
        dims.extend(self.tail.iter().map(|d| d.to_string()));
        let dict = format!(
            "{{'descr': '{}', 'fortran_order': False, 'shape': ({}), }}",
            self.descr,
            dims.join(", ")
        );
        let mut h = b"\x93NUMPY\x01\x00".to_vec();
        h.extend_from_slice(&((HEADER - 10) as u16).to_le_bytes());
        h.extend_from_slice(dict.as_bytes());
        h.resize(HEADER - 1, b' ');
        h.push(b'\n');
        h
    }

    fn push(&mut self, bytes: &[u8]) {
        self.f.write_all(bytes).unwrap();
        self.n += 1;
    }

    fn finish(mut self) {
        let h = self.header();
        self.f.flush().unwrap();
        let mut f = self.f.into_inner().unwrap_or_else(|_| die("a write failed"));
        f.seek(SeekFrom::Start(0)).unwrap();
        f.write_all(&h).unwrap();
    }
}

/// The dataset being written.
struct Out {
    windows: Npy,
    meta: Npy,
    pairs: i32,
}

impl Out {
    fn new(dir: &Path) -> Out {
        std::fs::create_dir_all(dir).unwrap_or_else(|e| die(&format!("{}: {e}", dir.display())));
        Out {
            windows: Npy::create(&dir.join("windows.npy"), "<f4", vec![LEN, F]),
            meta: Npy::create(&dir.join("meta.npy"), "<i4", vec![META.len()]),
            pairs: 0,
        }
    }

    /// A sighting's windows: the clean one, then each cheated twin, all
    /// with the sighting's pair id.
    fn write(&mut self, m: usize, sights: Vec<Sighting>) {
        for sg in sights {
            let [gun, map, viewer, skill, sight] = sg.at;
            let rows = std::iter::once((0, -1, -1, &sg.clean))
                .chain(sg.cheats.iter().map(|(k, st, b)| (1, *k as i32, *st, b)));
            for (label, cheat, style, b) in rows {
                let mut bytes = Vec::with_capacity(LEN * F * 4);
                for row in &b.rows {
                    for x in row {
                        bytes.extend_from_slice(&x.to_le_bytes());
                    }
                }
                self.windows.push(&bytes);
                let meta = [
                    label, self.pairs, m as i32, gun, map, cheat, style, viewer, skill, sight, b.helped, b.hits,
                    b.kills, b.heads,
                ];
                let bytes: Vec<u8> = meta.iter().flat_map(|x| x.to_le_bytes()).collect();
                self.meta.push(&bytes);
            }
            self.pairs += 1;
        }
    }
}

/// The zoo: `n` assists from `seed`.
fn zoo_of(seed: u64, n: usize) -> Vec<Genome> {
    (0..n)
        .map(|i| Genome::new(mix64(seed ^ (i as u64) << 32) as u32))
        .collect()
}

fn read_zoo(path: &str) -> Vec<Genome> {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| die(&format!("{path}: {e}")));
    text.lines()
        .map(|l| l.split_whitespace().next().unwrap_or(""))
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| Genome::decode(PREFIX, l.as_bytes()).unwrap_or_else(|e| die(&format!("{path}: {l}: {e:?}"))))
        .collect()
}

fn knobs_json(k: &Knobs) -> serde_json::Value {
    serde_json::json!({
        "aim": k.aim, "trigger": k.trigger, "fov_deg": k.fov_deg, "smooth": k.smooth,
        "max_deg": k.max_deg, "delay_ms": k.delay_ms, "fire_ms": k.fire_ms, "bone": k.bone,
        "miss": k.miss, "jitter_deg": k.jitter_deg, "walls": k.walls, "style": k.style(),
    })
}

fn describe(g: &Genome) -> String {
    let k = Knobs::from_genome(g);
    format!(
        "{}  {:<7} fov {:>4.1}° smooth {:.2} max {:>5.2}°/tick delay {:>3} ms bone {:.2} miss {:.2} jitter {:.2}°{}{}",
        g.text(PREFIX),
        k.style(),
        k.fov_deg,
        k.smooth,
        k.max_deg,
        k.delay_ms,
        k.bone,
        k.miss,
        k.jitter_deg,
        if k.trigger {
            format!(" trigger {} ms", k.fire_ms)
        } else {
            String::new()
        },
        if k.walls { " walls" } else { "" },
    )
}

/// Plays each of `jobs` on `threads` threads, each match's pairs written
/// as it finishes. Returns what each match was.
fn run_all<J: Sync>(
    jobs: &[J],
    threads: usize,
    zoo: &[Knobs],
    out: &Mutex<Out>,
    play: impl Fn(&J, usize, &[Knobs]) -> (Vec<Sighting>, serde_json::Value) + Sync,
) -> Vec<serde_json::Value> {
    let next = AtomicUsize::new(0);
    let done = Mutex::new(vec![serde_json::Value::Null; jobs.len()]);
    std::thread::scope(|sc| {
        for _ in 0..threads.max(1) {
            sc.spawn(|| loop {
                let m = next.fetch_add(1, Ordering::Relaxed);
                let Some(job) = jobs.get(m) else { break };
                let (sights, about) = play(job, m, zoo);
                let n = sights.len();
                out.lock().unwrap().write(m, sights);
                eprintln!("match {m}: {n} sightings  {about}");
                done.lock().unwrap()[m] = about;
            });
        }
    });
    done.into_inner().unwrap()
}

fn main() {
    let mut out = None;
    let (mut matches, mut seed, mut secs, mut threads, mut zoo_n) = (40usize, 1u64, 300u32, 2usize, 64usize);
    let (mut each, mut cap, mut hands) = (false, MAX_SIGHTINGS, false);
    let mut zoo_file = None;
    let mut modes = "duel,doubles,ffa".to_string();
    let mut maps = "0,1,2,5,6,7".to_string();
    let mut rest = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut val = || args.next().unwrap_or_else(|| die(&format!("{a} needs a value")));
        let num = |v: String| v.parse::<u64>().unwrap_or_else(|_| die(&format!("{v}: a number")));
        match a.as_str() {
            "--out" => out = Some(PathBuf::from(val())),
            "--matches" => matches = num(val()) as usize,
            "--seed" => seed = num(val()),
            "--secs" => secs = num(val()) as u32,
            "--threads" => threads = num(val()) as usize,
            "--zoo-n" => zoo_n = num(val()) as usize,
            "--zoo" => zoo_file = Some(val()),
            "--modes" => modes = val(),
            "--maps" => maps = val(),
            "--each" => each = true,
            "--hands" => hands = true,
            "--sightings" => cap = num(val()) as usize,
            _ => rest.push(a),
        }
    }
    let rest: Vec<&str> = rest.iter().map(String::as_str).collect();
    let zoo = || zoo_file.as_deref().map_or_else(|| zoo_of(seed, zoo_n), read_zoo);
    match rest.as_slice() {
        ["zoo"] | ["zoo", _] => {
            let n = rest
                .get(1)
                .map_or(zoo_n, |n| n.parse().unwrap_or_else(|_| die("zoo [n]")));
            for g in zoo_of(seed, n) {
                println!("{}", describe(&g));
            }
        }
        ["knobs", text] => {
            let g = Genome::decode(PREFIX, text.as_bytes()).unwrap_or_else(|e| die(&format!("{e:?}")));
            println!("{}", describe(&g));
        }
        ["bots"] => {
            let dir = out.unwrap_or_else(|| die("bots --out dir"));
            let modes: Vec<&str> = modes.split(',').collect();
            let maps: Vec<u8> = maps
                .split(',')
                .map(|m| m.parse().unwrap_or_else(|_| die("--maps 0,1,…")))
                .collect();
            let jobs: Vec<(u64, &str, u8)> = (0..matches)
                .map(|m| {
                    (
                        seed.wrapping_mul(1_000_003).wrapping_add(m as u64),
                        modes[m % modes.len()],
                        maps[(m / modes.len()) % maps.len()],
                    )
                })
                .collect();
            let genomes = zoo();
            let knobs: Vec<Knobs> = genomes.iter().map(Knobs::from_genome).collect();
            let sink = Mutex::new(Out::new(&dir));
            let about = run_all(&jobs, threads, &knobs, &sink, |&(mseed, mode, map), _, zoo| {
                let mut rules = match mode {
                    "duel" => Rules::duel(),
                    "doubles" => Rules::doubles(),
                    "ffa" => Rules::ffa(),
                    m => die(&format!("{m}: duel, doubles or ffa")),
                };
                // To the clock, not the score: more sightings a match.
                rules.score_limit = 1000;
                rules.time_limit_ms = secs * 1000;
                let m = sim::map::by_id(map).unwrap_or_else(|| die(&format!("no map {map}")));
                let name = m.name;
                let mut w = World::new(m, rules, mseed, (mseed, !mseed));
                let n = rules.seats as usize;
                let mut feed = Bots {
                    bots: [Bot::new(mseed, 0, 0.5); MAX_FIGHTERS],
                    skills: [0.5; MAX_FIGHTERS],
                    n,
                    end: (secs + 10) * TICK_HZ,
                    hands: hands.then(|| {
                        (0..n)
                            .map(|s| Hand::new(mix64(mseed ^ (s as u64 + 7) << 44) as u32))
                            .collect()
                    }),
                };
                let traits: Vec<serde_json::Value> = feed.hands.iter().flatten().map(|h| h.traits.json()).collect();
                for s in 0..n {
                    w.seat(s, (s & 1) as u8);
                    let skill = 0.2 + 0.75 * (mix64(mseed ^ (s as u64 + 1) << 40) % 1000) as f32 / 1000.0;
                    feed.skills[s] = skill;
                    feed.bots[s] = Bot::new(mseed, s, skill);
                }
                let sights = cut(Run::new(w, feed), zoo, mseed, each, cap);
                (
                    sights,
                    serde_json::json!({ "mode": mode, "map": map, "map_name": name, "seed": mseed, "hands": traits }),
                )
            });
            let sink = sink.into_inner().unwrap();
            info(
                &dir,
                sink,
                &genomes,
                about,
                if hands { "bots with hands" } else { "bots" },
            );
        }
        ["replays", files @ ..] if !files.is_empty() => {
            let dir = out.unwrap_or_else(|| die("replays --out dir <file.osr>…"));
            let bytes: Vec<(String, Vec<u8>)> = files
                .iter()
                .map(|f| {
                    (
                        f.to_string(),
                        std::fs::read(f).unwrap_or_else(|e| die(&format!("{f}: {e}"))),
                    )
                })
                .collect();
            let genomes = zoo();
            let knobs: Vec<Knobs> = genomes.iter().map(Knobs::from_genome).collect();
            let sink = Mutex::new(Out::new(&dir));
            let about = run_all(&bytes, threads, &knobs, &sink, |(name, b), m, zoo| {
                match Cursor::open(b) {
                    Ok((c, w)) => {
                        let map = w.map.id;
                        let feed = Recorded {
                            c,
                            prev: [Input::default(); MAX_FIGHTERS],
                        };
                        let sights = cut(Run::new(w, feed), zoo, mix64(m as u64), each, cap);
                        (sights, serde_json::json!({ "file": name, "map": map }))
                    }
                    Err(e) => {
                        eprintln!("{name}: {e}");
                        (Vec::new(), serde_json::json!({ "file": name, "error": e }))
                    }
                }
            });
            let sink = sink.into_inner().unwrap();
            info(&dir, sink, &genomes, about, "replays");
        }
        _ => die("zoo [n] | knobs <oa1:…> | bots --out dir | replays --out dir <file.osr>… (see the source's header)"),
    }
}

/// Closes the files and writes `info.json`.
fn info(dir: &Path, out: Out, zoo: &[Genome], matches: Vec<serde_json::Value>, from: &str) {
    let (windows, pairs) = (out.windows.n, out.pairs);
    let each = windows > 2 * pairs as usize;
    out.windows.finish();
    out.meta.finish();
    let doc = serde_json::json!({
        "from": from,
        "len": LEN,
        "pre": PRE,
        "tick_hz": TICK_HZ,
        "features": FEATURES,
        "meta": META,
        "styles": STYLES,
        "maps": sim::maps::ALL.iter().map(|d| serde_json::json!({"id": d.id, "name": d.name})).collect::<Vec<_>>(),
        "guns": sim::arms::ARMS.iter().map(|a| a.name).collect::<Vec<_>>(),
        "zoo": zoo.iter().map(|g| {
            let mut k = knobs_json(&Knobs::from_genome(g));
            k["genome"] = g.text(PREFIX).into();
            k
        }).collect::<Vec<_>>(),
        "matches": matches,
        "windows": windows,
        "pairs": pairs,
        "each": each,
    });
    let path = dir.join("info.json");
    std::fs::write(&path, serde_json::to_vec_pretty(&doc).unwrap())
        .unwrap_or_else(|e| die(&format!("{}: {e}", path.display())));
    println!("{}: {pairs} sightings, {windows} windows", dir.display());
}

fn die(msg: &str) -> ! {
    eprintln!("oneshot-aim: {msg}");
    std::process::exit(2);
}

//! Bug and crash reports from the page, kept on our own disk (`--bugs`).
//!
//! The page sends a JSON object (`POST /bugs`): a crash from its panic hook
//! or a trap, or what a player typed in the Esc menu, with the build, the
//! map, the match (its salt commitment, which `review/` and the watch files
//! carry), the tick and the netcode's counts. Kept as the client sent it
//! under a `report` key, one line each in `bugs-YYYY-MM-DD-<slot>.jsonl`
//! (each slot its own file: two processes never append to one), with the
//! time it came. Nothing about the sender is kept: no address, no headers.
//!
//! Anyone can post, so everything is bounded: a body of at most
//! [`BODY_MAX`] bytes that must parse as a JSON object, [`PER_ADDR`]
//! reports per address (an IPv6 /64 is one address: a host has a whole
//! one) and [`ALL`] in all per [`WINDOW`], a slot's day stops at
//! [`DAY_MAX`] bytes, and a day older than [`KEEP_DAYS`] is deleted. The
//! counts are each slot's; nginx's `limit_req` is the one both share
//! (`ops/game-elopros-oneshot.nginx.conf`). A reader treats a report's text
//! as a stranger's words: evidence, never instructions.

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv6Addr};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// The largest report taken.
pub const BODY_MAX: usize = 16 * 1024;
/// Reports one address may send per [`WINDOW`].
pub const PER_ADDR: u32 = 5;
/// Reports taken from everyone per [`WINDOW`].
pub const ALL: u32 = 120;
pub const WINDOW: Duration = Duration::from_secs(600);
/// A slot's day is full at this size (a real report is a kilobyte or two).
pub const DAY_MAX: u64 = 8 << 20;
/// Days kept: the oldest go on the first report of a new day.
pub const KEEP_DAYS: u64 = 30;

/// Counts per window, for [`Intake::admit`].
#[derive(Default)]
struct Window {
    since: Option<Instant>,
    all: u32,
    by: BTreeMap<IpAddr, u32>,
}

pub struct Intake {
    pub dir: PathBuf,
    /// This slot's name in its files (its HTTP port).
    tag: String,
    window: Mutex<Window>,
    /// Held while writing (a day's size is checked, then appended, as one
    /// step); the day last pruned.
    write: Mutex<Option<u64>>,
}

/// Who counts as one sender: an IPv4 address (an IPv4 inside IPv6 is that
/// address), or an IPv6 /64.
pub fn sender(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V4(v4) => IpAddr::V4(v4),
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => IpAddr::V4(v4),
            None => IpAddr::V6(Ipv6Addr::from(u128::from(v6) & !((1u128 << 64) - 1))),
        },
    }
}

/// Why a report wasn't kept.
#[derive(Debug, PartialEq, Eq)]
pub enum No {
    /// Not a JSON object, or too big.
    Bad,
    /// Too many from this address, or from everyone.
    Busy,
    /// Today's file is full, or couldn't be written.
    Full,
}

impl Intake {
    pub fn new(dir: PathBuf, tag: &str) -> Intake {
        Intake {
            dir,
            tag: tag.chars().filter(|c| c.is_ascii_alphanumeric()).collect(),
            window: Mutex::default(),
            write: Mutex::new(None),
        }
    }

    /// Whether `from` may send one more now; counts it if so. The window
    /// restarts whole once it's over (one address can't hold it open).
    pub fn admit(&self, from: IpAddr, now: Instant) -> bool {
        let from = sender(from);
        let Ok(mut w) = self.window.lock() else {
            return false;
        };
        if w.since.is_none_or(|t| now.duration_since(t) >= WINDOW) {
            *w = Window {
                since: Some(now),
                ..Window::default()
            };
        }
        let mine = w.by.get(&from).copied().unwrap_or(0);
        if w.all >= ALL || mine >= PER_ADDR {
            return false;
        }
        w.all += 1;
        w.by.insert(from, mine + 1);
        true
    }

    /// Checks `body` and appends it to today's file (`unix_s` the time).
    pub fn keep(&self, from: IpAddr, body: &[u8], unix_s: u64, now: Instant) -> Result<(), No> {
        let line = line(body, unix_s)?;
        if !self.admit(from, now) {
            return Err(No::Busy);
        }
        let Ok(mut pruned) = self.write.lock() else {
            return Err(No::Full);
        };
        let today = unix_s / 86_400;
        if *pruned != Some(today) {
            *pruned = Some(today);
            prune(&self.dir, today);
        }
        append(&self.dir, &file_name(unix_s, &self.tag), &line)
    }
}

/// The line kept for `body`: `{"at":<unix s>,"report":<the object>}` and a
/// newline, re-serialised (so it is one line whatever the sender's spacing).
pub fn line(body: &[u8], unix_s: u64) -> Result<String, No> {
    if body.len() > BODY_MAX {
        return Err(No::Bad);
    }
    let mut v: serde_json::Value = serde_json::from_slice(body).map_err(|_| No::Bad)?;
    if !v.is_object() {
        return Err(No::Bad);
    }
    sane(&mut v);
    let doc = serde_json::json!({ "at": unix_s, "report": v });
    Ok(format!("{doc}\n"))
}

/// Characters that change how text around them is shown without being
/// seen: bidi overrides and isolates, marks, zero-width joiners and the
/// like (the trojan-source kind; Gates' `report.rs` `sane`). A report is
/// read by a person or an agent; what it shows must be what it says.
fn hidden(c: char) -> bool {
    matches!(c,
        '\u{061C}' | '\u{180E}' | '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}'
        | '\u{2060}'..='\u{2069}' | '\u{FEFF}' | '\u{FFF9}'..='\u{FFFB}')
}

/// Every string in `v` (keys too) without [`hidden`] characters.
fn sane(v: &mut serde_json::Value) {
    use serde_json::Value;
    let clean = |s: &str| s.chars().filter(|c| !hidden(*c)).collect::<String>();
    match v {
        Value::String(s) if s.chars().any(hidden) => *s = clean(s),
        Value::Array(a) => a.iter_mut().for_each(sane),
        Value::Object(o) => {
            let old = std::mem::take(o);
            for (k, mut x) in old {
                sane(&mut x);
                o.insert(clean(&k), x);
            }
        }
        _ => {}
    }
}

fn append(dir: &Path, name: &str, line: &str) -> Result<(), No> {
    use std::io::Write;
    std::fs::create_dir_all(dir).map_err(|_| No::Full)?;
    let path = dir.join(name);
    let have = std::fs::metadata(&path).map_or(0, |m| m.len());
    if have + line.len() as u64 > DAY_MAX {
        return Err(No::Full);
    }
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|_| No::Full)?;
    // One write of the whole line: appends from two reports don't interleave.
    f.write_all(line.as_bytes()).map_err(|_| No::Full)
}

/// `YYYY-MM-DD` for day `day` since 1970-01-01.
fn date(day: u64) -> String {
    let (y, m, d) = civil(day as i64);
    format!("{y:04}-{m:02}-{d:02}")
}

/// `bugs-YYYY-MM-DD-<tag>.jsonl` for the UTC day of `unix_s` (no `-<tag>`
/// without one).
pub fn file_name(unix_s: u64, tag: &str) -> String {
    let day = date(unix_s / 86_400);
    if tag.is_empty() {
        format!("bugs-{day}.jsonl")
    } else {
        format!("bugs-{day}-{tag}.jsonl")
    }
}

/// Deletes every slot's days older than [`KEEP_DAYS`] before `today`.
fn prune(dir: &Path, today: u64) {
    let Some(first) = today.checked_sub(KEEP_DAYS) else {
        return;
    };
    let keep_from = date(first);
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let name = e.file_name();
        let Some(name) = name.to_str() else { continue };
        // `bugs-` then the date: dates in that form sort as they read.
        let old = name.ends_with(".jsonl")
            && name.get(..5) == Some("bugs-")
            && name.get(5..15).is_some_and(|d| d < keep_from.as_str());
        if old {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// The date of day `z` since 1970-01-01 (Howard Hinnant's `civil_from_days`).
fn civil(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_are_dates() {
        assert_eq!(file_name(0, ""), "bugs-1970-01-01.jsonl");
        assert_eq!(file_name(951_782_400, ""), "bugs-2000-02-29.jsonl");
        assert_eq!(file_name(1_793_750_399, "8450"), "bugs-2026-11-03-8450.jsonl");
        assert_eq!(file_name(1_793_750_400, "8450"), "bugs-2026-11-04-8450.jsonl");
    }

    #[test]
    fn only_a_json_object_under_the_cap_is_kept() {
        assert_eq!(line(b"not json", 1), Err(No::Bad));
        assert_eq!(line(b"[1,2]", 1), Err(No::Bad));
        let big = format!("{{\"text\":\"{}\"}}", "a".repeat(BODY_MAX));
        assert_eq!(line(big.as_bytes(), 1), Err(No::Bad));
        let l = line(b"{ \"kind\" : \"crash\",\n \"text\": \"a\\nb\" }", 7).unwrap();
        assert_eq!(l, "{\"at\":7,\"report\":{\"kind\":\"crash\",\"text\":\"a\\nb\"}}\n");
    }

    #[test]
    fn what_hides_text_is_taken_out() {
        let body = "{\"text\":\"left \u{202E}thgir\u{202C} ok\u{200B}\",\"k\u{2066}ey\":[\"\u{FEFF}x\"]}";
        let l = line(body.as_bytes(), 1).unwrap();
        assert_eq!(
            l,
            "{\"at\":1,\"report\":{\"key\":[\"x\"],\"text\":\"left thgir ok\"}}\n"
        );
    }

    #[test]
    fn an_address_gets_its_share_and_the_window_restarts() {
        let i = Intake::new(PathBuf::new(), "");
        let (a, b): (IpAddr, IpAddr) = ("10.0.0.1".parse().unwrap(), "10.0.0.2".parse().unwrap());
        let t = Instant::now();
        for _ in 0..PER_ADDR {
            assert!(i.admit(a, t));
        }
        assert!(!i.admit(a, t), "past its share");
        assert!(i.admit(b, t), "another address still may");
        assert!(i.admit(a, t + WINDOW), "a new window");
    }

    #[test]
    fn everyone_together_is_capped() {
        let i = Intake::new(PathBuf::new(), "");
        let t = Instant::now();
        for n in 0..ALL {
            let ip = IpAddr::from([10, 1, (n / 256) as u8, (n % 256) as u8]);
            assert!(i.admit(ip, t));
        }
        assert!(!i.admit("10.9.9.9".parse().unwrap(), t));
    }

    #[test]
    fn reports_land_in_the_days_file() {
        let dir = std::env::temp_dir().join(format!("oneshot-bugs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let i = Intake::new(dir.clone(), "");
        let from: IpAddr = "10.0.0.1".parse().unwrap();
        let now = Instant::now();
        i.keep(from, b"{\"kind\":\"player\",\"text\":\"one\"}", 86_400, now)
            .unwrap();
        i.keep(from, b"{\"kind\":\"crash\"}", 86_401, now).unwrap();
        assert_eq!(i.keep(from, b"nope", 86_402, now), Err(No::Bad));
        let text = std::fs::read_to_string(dir.join("bugs-1970-01-02.jsonl")).unwrap();
        assert_eq!(text.lines().count(), 2, "{text}");
        assert!(text
            .lines()
            .all(|l| serde_json::from_str::<serde_json::Value>(l).is_ok()));
        let _ = std::fs::remove_dir_all(&dir);
    }
    #[test]
    fn an_ipv6_host_is_its_64_and_a_mapped_v4_its_address() {
        let a: IpAddr = "2001:db8:1:2:aaaa::1".parse().unwrap();
        let b: IpAddr = "2001:db8:1:2:ffff:1:2:3".parse().unwrap();
        let c: IpAddr = "2001:db8:1:3::1".parse().unwrap();
        assert_eq!(sender(a), sender(b));
        assert_ne!(sender(a), sender(c));
        assert_eq!(
            sender("::ffff:10.0.0.1".parse().unwrap()),
            "10.0.0.1".parse::<IpAddr>().unwrap()
        );
        // One /64 is one sender's share, however many addresses it uses.
        let i = Intake::new(PathBuf::new(), "");
        let t = Instant::now();
        for n in 0..PER_ADDR {
            let ip: IpAddr = format!("2001:db8:1:2::{}", n + 1).parse().unwrap();
            assert!(i.admit(ip, t));
        }
        assert!(!i.admit("2001:db8:1:2::99".parse().unwrap(), t));
    }

    #[test]
    fn old_days_go_on_a_new_day() {
        let dir = std::env::temp_dir().join(format!("oneshot-bugs-prune-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let today = 20_000u64; // 2024-10-04
        for (day, tag) in [
            (today - KEEP_DAYS - 1, "8450"),
            (today - KEEP_DAYS, "8451"),
            (today - 1, "8450"),
        ] {
            std::fs::write(dir.join(file_name(day * 86_400, tag)), "{}\n").unwrap();
        }
        std::fs::write(dir.join("notes.txt"), "kept").unwrap();
        let i = Intake::new(dir.clone(), "8450");
        i.keep("10.0.0.1".parse().unwrap(), b"{}", today * 86_400 + 5, Instant::now())
            .unwrap();
        let mut names: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().into_string().unwrap())
            .collect();
        names.sort();
        assert_eq!(
            names,
            [
                file_name((today - KEEP_DAYS) * 86_400, "8451"),
                file_name((today - 1) * 86_400, "8450"),
                file_name(today * 86_400, "8450"),
                "notes.txt".to_string(),
            ]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

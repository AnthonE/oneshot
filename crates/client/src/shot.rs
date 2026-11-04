//! Where a player's screenshot goes and what it is called: the arithmetic
//! half of the F12 key, after Gates' `client/src/shot.rs` (MIT). The Bevy
//! half is `look/shot.rs`.
//!
//! **Why the game takes its own screenshot at all**: on Linux the desktop's
//! own key is not one thing (X11 tools are blind under Wayland, the portal
//! a bare session may not have, a black frame from a fullscreen surface on
//! direct scanout), and none of that is the game's fault but all of it is
//! the player's problem. A client reading its own frame works on every
//! branch of it. In a browser the page can't write a file at all, so the
//! frame is handed to the browser as a download (`look/shot.rs`).
//!
//! The stamp is UTC: what a name has to be is unique and sortable, and a
//! local time would need a timezone database this client doesn't link.

use std::path::{Path, PathBuf};

use crate::config::{HostOs, HOST_OS};

/// Every screenshot starts with this, so a folder of other pictures still
/// sorts and globs cleanly.
pub const PREFIX: &str = "oneshot-";

/// How many shots one second may hold before [`pick`] gives up: a cap on a
/// key a player can lean on, and its policy is refuse and say so (walking
/// on until a name is free is an unbounded loop on the frame it was
/// pressed).
pub const PER_STAMP: u32 = 64;

/// Where this machine's screenshots go, or `None` when the environment
/// names no base to put them in (the key then says so, and saves nothing
/// beside the binary).
pub fn dir() -> Option<PathBuf> {
    dir_from(HOST_OS, |k| std::env::var(k).ok())
}

/// The rule, with the environment injected so every branch is testable on
/// one machine (`config::path_from`'s shape). An empty variable is unset.
///
/// `ONESHOT_SHOTS_DIR` wins and is taken verbatim. Then the platform's
/// pictures folder, `oneshot` inside it; with no pictures folder known, the
/// settings' folder (`config::path_from`), `shots` inside it.
pub fn dir_from(os: HostOs, get: impl Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let var = |k: &str| get(k).filter(|v| !v.is_empty());
    if let Some(d) = var("ONESHOT_SHOTS_DIR") {
        return Some(PathBuf::from(d));
    }
    let pictures = match os {
        HostOs::Windows => var("USERPROFILE").map(|h| PathBuf::from(h).join("Pictures")),
        HostOs::Mac => var("HOME").map(|h| PathBuf::from(h).join("Pictures")),
        // A user-dirs value rather than a spec'd variable, so usually
        // absent; a session that does export it has said where pictures go.
        HostOs::Unix => var("XDG_PICTURES_DIR")
            .map(PathBuf::from)
            .or_else(|| var("HOME").map(|h| PathBuf::from(h).join("Pictures"))),
    };
    if let Some(p) = pictures {
        return Some(p.join("oneshot"));
    }
    let settings = crate::config::path_from(os, get)?;
    Some(settings.parent()?.join("shots"))
}

/// `YYYYMMDD-HHMMSS`, UTC, from seconds since the Unix epoch.
pub fn stamp(secs: u64) -> String {
    let (y, m, d) = civil_from_days(secs / 86_400);
    let rem = secs % 86_400;
    format!(
        "{y:04}{m:02}{d:02}-{:02}{:02}{:02}",
        rem / 3_600,
        (rem % 3_600) / 60,
        rem % 60
    )
}

/// Days since the epoch to `(year, month, day)`, UTC: Howard Hinnant's
/// `civil_from_days`, unsigned (exact for every day after 1970-01-01).
fn civil_from_days(days: u64) -> (u64, u64, u64) {
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z % 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + u64::from(m <= 2);
    (y, m, d)
}

/// This machine's clock in seconds since the epoch (0 before 1970). Names a
/// file and nothing else.
///
/// Not `SystemTime` in a browser: on `wasm32-unknown-unknown` it doesn't
/// fail, it panics (`unreachable`, no message), so the page's own clock.
#[cfg(not(target_arch = "wasm32"))]
pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(target_arch = "wasm32")]
pub fn now_secs() -> u64 {
    let ms = js_sys::Date::now();
    if ms < 0.0 {
        0
    } else {
        (ms / 1000.0) as u64
    }
}

/// The name for a shot taken at `stamp`, the `n`th of its second: the first
/// has no suffix, the rest `-2`, `-3`...
pub fn name(stamp: &str, n: u32) -> String {
    if n <= 1 {
        format!("{PREFIX}{stamp}.png")
    } else {
        format!("{PREFIX}{stamp}-{n}.png")
    }
}

/// The first free name for this stamp in `dir`, or `None` when
/// [`PER_STAMP`] are spent. `exists` is injected so the rule needs no disk.
pub fn pick(dir: &Path, stamp: &str, exists: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    (1..=PER_STAMP).map(|n| dir.join(name(stamp, n))).find(|p| !exists(p))
}

/// `path` as a player reads it: their home folder as `~`.
pub fn shown(path: &Path) -> String {
    let home = std::env::var("HOME")
        .ok()
        .or_else(|| std::env::var("USERPROFILE").ok())
        .filter(|h| !h.is_empty());
    match home.and_then(|h| path.strip_prefix(h).ok().map(Path::to_path_buf)) {
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |k| pairs.iter().find(|(n, _)| *n == k).map(|(_, v)| v.to_string())
    }

    #[test]
    fn each_platform_puts_them_where_it_keeps_pictures() {
        assert_eq!(
            dir_from(HostOs::Unix, env(&[("HOME", "/h")])),
            Some(PathBuf::from("/h/Pictures/oneshot"))
        );
        assert_eq!(
            dir_from(HostOs::Mac, env(&[("HOME", "/Users/x")])),
            Some(PathBuf::from("/Users/x/Pictures/oneshot"))
        );
        assert_eq!(
            dir_from(HostOs::Windows, env(&[("USERPROFILE", "C:/Users/x")])),
            Some(PathBuf::from("C:/Users/x/Pictures/oneshot"))
        );
        assert_eq!(
            dir_from(HostOs::Unix, env(&[("XDG_PICTURES_DIR", "/p"), ("HOME", "/h")])),
            Some(PathBuf::from("/p/oneshot"))
        );
    }

    #[test]
    fn no_pictures_folder_falls_back_to_the_settings_folder() {
        assert_eq!(
            dir_from(HostOs::Unix, env(&[("XDG_CONFIG_HOME", "/cfg")])),
            Some(PathBuf::from("/cfg/oneshot/shots"))
        );
        assert_eq!(
            dir_from(HostOs::Windows, env(&[("APPDATA", "C:/A")])),
            Some(PathBuf::from("C:/A/oneshot/shots"))
        );
        assert_eq!(dir_from(HostOs::Unix, env(&[])), None);
        assert_eq!(dir_from(HostOs::Unix, env(&[("HOME", "")])), None);
    }

    #[test]
    fn the_override_wins_and_is_taken_verbatim() {
        for os in [HostOs::Unix, HostOs::Mac, HostOs::Windows] {
            assert_eq!(
                dir_from(
                    os,
                    env(&[("ONESHOT_SHOTS_DIR", "/shots"), ("HOME", "/h"), ("USERPROFILE", "C:/x")])
                ),
                Some(PathBuf::from("/shots"))
            );
        }
    }

    #[test]
    fn the_stamp_is_utc_and_sorts_as_it_reads() {
        assert_eq!(stamp(0), "19700101-000000");
        assert_eq!(stamp(1_786_803_123), "20260815-141203");
        // A leap day, and the century rule both ways.
        assert_eq!(stamp(1_709_164_800), "20240229-000000");
        assert_eq!(stamp(951_782_400), "20000229-000000");
        assert_eq!(stamp(4_107_456_000), "21000228-000000");
        assert_eq!(stamp(4_107_542_400), "21000301-000000");
        let base = 1_786_752_000;
        let mut last = String::new();
        for s in (0..86_400u64).step_by(7) {
            let st = stamp(base + s);
            assert!(st > last, "{st} !> {last}");
            last = st;
        }
    }

    #[test]
    fn names_count_up_within_a_second_and_stop_at_the_cap() {
        let dir = Path::new("/s");
        let mut taken: HashSet<PathBuf> = HashSet::new();
        assert_eq!(
            pick(dir, "x", |p| taken.contains(p)),
            Some(PathBuf::from("/s/oneshot-x.png"))
        );
        taken.insert(PathBuf::from("/s/oneshot-x.png"));
        assert_eq!(
            pick(dir, "x", |p| taken.contains(p)),
            Some(PathBuf::from("/s/oneshot-x-2.png"))
        );
        let mut n = 1;
        while let Some(p) = pick(dir, "x", |p| taken.contains(p)) {
            assert!(taken.insert(p));
            n += 1;
            assert!(n <= PER_STAMP);
        }
        assert_eq!(n, PER_STAMP);
        assert_eq!(pick(dir, "y", |_| true), None);
    }
}

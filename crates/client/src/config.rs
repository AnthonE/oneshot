//! The settings file, after Gates' `client/src/config.rs` (MIT), trimmed:
//! `key = value` lines, a `version`, `bind.<action> = "<primary> <alt>"`,
//! `#` comments. Keys this build doesn't know are kept and written back, so
//! an older build doesn't throw away a newer one's settings. No Bevy and no
//! serde: plain text the player can edit. The browser keeps the same text
//! in `localStorage` (`settings.rs`).

use std::path::{Path, PathBuf};

/// Bumped only when a key is renamed or changes meaning; a newer file's
/// number is kept on save.
pub const SETTINGS_VERSION: u32 = 2;

/// A file, read.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Doc {
    pub version: u32,
    /// Every `key = value` but the version and the binds, in file order,
    /// the last of a repeated key winning.
    pub pairs: Vec<(String, String)>,
    /// `bind.<action>` lines: (action, value).
    pub binds: Vec<(String, String)>,
}

/// A key worth keeping: letters, digits, `_` and `.`.
fn key_shaped(k: &str) -> bool {
    !k.is_empty() && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
}

pub fn parse(text: &str) -> Doc {
    let mut doc = Doc {
        version: SETTINGS_VERSION,
        ..Doc::default()
    };
    for line in text.lines() {
        // A comment is a whole line: a value may hold a `#` (written back
        // as it came).
        if line.trim_start().starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let (k, v) = (k.trim(), v.trim().trim_matches('"').to_string());
        if !key_shaped(k) {
            continue;
        }
        if k == "version" {
            if let Ok(n) = v.parse() {
                doc.version = n;
            }
        } else if let Some(action) = k.strip_prefix("bind.") {
            doc.binds.retain(|(a, _)| a != action);
            doc.binds.push((action.to_string(), v));
        } else {
            doc.pairs.retain(|(a, _)| a != k);
            doc.pairs.push((k.to_string(), v));
        }
    }
    doc
}

pub fn serialize(version: u32, pairs: &[(String, String)], binds: &[(String, String)]) -> String {
    let mut s = String::from("# oneshot settings. Edited in the game (Esc, Settings); kept as written.\n");
    s += &format!("version = {}\n", version.max(SETTINGS_VERSION));
    for (k, v) in pairs {
        s += &format!("{k} = {v}\n");
    }
    for (a, v) in binds {
        s += &format!("bind.{a} = \"{v}\"\n");
    }
    s
}

/// Writes `text` to `path` whole or not at all: a temporary file beside it,
/// then a rename over it.
pub fn save(path: &Path, text: &str) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    // This process's own: two clients writing at once can't truncate each
    // other's temporary file.
    let tmp = path.with_extension(format!("toml.{}.tmp", std::process::id()));
    std::fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

/// The file's text, or None if there is none.
pub fn read(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostOs {
    Windows,
    Mac,
    Unix,
}

#[cfg(target_os = "windows")]
pub const HOST_OS: HostOs = HostOs::Windows;
#[cfg(target_os = "macos")]
pub const HOST_OS: HostOs = HostOs::Mac;
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub const HOST_OS: HostOs = HostOs::Unix;

/// Where the settings live on this machine, by its convention; None if no
/// base directory is known (nothing is saved that run).
pub fn settings_path() -> Option<PathBuf> {
    path_from(HOST_OS, |k| std::env::var(k).ok())
}

/// [`settings_path`] for `os` with the environment read by `get`, so every
/// branch is testable anywhere. An empty variable is an unset one.
pub fn path_from(os: HostOs, get: impl Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let var = |k: &str| get(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    let base = match os {
        HostOs::Windows => var("APPDATA"),
        HostOs::Mac => var("HOME").map(|h| h.join("Library").join("Application Support")),
        HostOs::Unix => var("XDG_CONFIG_HOME").or_else(|| var("HOME").map(|h| h.join(".config"))),
    }?;
    Some(base.join("oneshot").join("settings.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(p: &[(&str, &str)]) -> Vec<(String, String)> {
        p.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
    }

    #[test]
    fn a_round_trip_is_identity() {
        let p = pairs(&[("sensitivity", "1.25"), ("invert", "true"), ("from_a_newer_build", "7")]);
        let b = pairs(&[("jump", "Space Mouse4")]);
        let doc = parse(&serialize(SETTINGS_VERSION, &p, &b));
        assert_eq!(doc.pairs, p);
        assert_eq!(doc.binds, b);
        assert_eq!(doc.version, SETTINGS_VERSION);
    }

    #[test]
    fn a_garbled_file_is_nothing_and_comments_go() {
        let doc = parse("\u{0}\u{1}garbage\n= no key\nnot a line\n# x = 1\n fov = 90\nnote = a#b\n");
        assert_eq!(doc.pairs, pairs(&[("fov", "90"), ("note", "a#b")]));
        assert!(doc.binds.is_empty());
    }

    #[test]
    fn the_last_of_a_repeated_key_wins() {
        let doc = parse("fov = 80\nbind.jump = \"Space\"\nfov = 95\nbind.jump = \"KeyB\"\n");
        assert_eq!(doc.pairs, pairs(&[("fov", "95")]));
        assert_eq!(doc.binds, pairs(&[("jump", "KeyB")]));
    }

    #[test]
    fn a_newer_file_keeps_its_version() {
        let doc = parse("version = 9\n");
        assert_eq!(doc.version, 9);
        assert!(serialize(doc.version, &[], &[]).contains("version = 9"));
        assert!(serialize(0, &[], &[]).contains(&format!("version = {SETTINGS_VERSION}")));
    }

    #[test]
    fn the_file_round_trips_through_disk() {
        let dir = std::env::temp_dir().join(format!("oneshot-config-{}", std::process::id()));
        let path = dir.join("oneshot").join("settings.toml");
        let text = serialize(1, &pairs(&[("fov", "100")]), &[]);
        save(&path, &text).unwrap();
        assert_eq!(read(&path).as_deref(), Some(text.as_str()));
        assert!(!path.with_extension(format!("toml.{}.tmp", std::process::id())).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_path_is_the_platform_convention() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |k: &str| pairs.iter().find(|(a, _)| *a == k).map(|(_, v)| v.to_string())
        };
        assert_eq!(
            path_from(HostOs::Windows, env(&[("APPDATA", "C:/Users/a/AppData/Roaming")])),
            Some(PathBuf::from("C:/Users/a/AppData/Roaming/oneshot/settings.toml"))
        );
        assert_eq!(
            path_from(HostOs::Mac, env(&[("HOME", "/Users/a")])),
            Some(PathBuf::from(
                "/Users/a/Library/Application Support/oneshot/settings.toml"
            ))
        );
        assert_eq!(
            path_from(HostOs::Unix, env(&[("XDG_CONFIG_HOME", ""), ("HOME", "/home/a")])),
            Some(PathBuf::from("/home/a/.config/oneshot/settings.toml"))
        );
        assert_eq!(
            path_from(HostOs::Unix, env(&[("XDG_CONFIG_HOME", "/cfg")])),
            Some(PathBuf::from("/cfg/oneshot/settings.toml"))
        );
        assert_eq!(path_from(HostOs::Unix, env(&[])), None);
    }
}

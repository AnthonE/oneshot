//! A desktop crash, reported: the default hook's words on stderr as before,
//! a copy beside the settings (`crashes/`), and the same to the server's
//! `POST /bugs` (`server/src/bugs.rs`), as the page does for a browser's
//! (`web/boot.js`, `report`). Once a run: a panic that brings others down
//! with it is one report.

use std::sync::atomic::{AtomicBool, Ordering};

/// Reported already this run.
static SENT: AtomicBool = AtomicBool::new(false);

/// Wraps the panic hook there is (`server` is where the game was found:
/// its `/bugs` takes the report).
pub fn hook(server: String) {
    let before = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        before(info);
        if SENT.swap(true, Ordering::Relaxed) {
            return;
        }
        let doc = doc(&info.to_string());
        let unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let kept = crate::config::settings_path()
            .and_then(|p| Some(p.parent()?.join("crashes")))
            .map(|dir| dir.join(format!("crash-{unix}.json")))
            .filter(|path| {
                path.parent().is_some_and(|dir| std::fs::create_dir_all(dir).is_ok())
                    && std::fs::write(path, &doc).is_ok()
            });
        if let Some(path) = &kept {
            eprintln!("oneshot: the crash is written to {}", path.display());
        }
        let url = format!("{}/bugs", server.trim_end_matches('/'));
        match crate::door::http_post(&url, doc.as_bytes()) {
            Ok(204) => eprintln!("oneshot: and reported. Thank you."),
            _ if kept.is_some() => eprintln!("oneshot: it couldn't be reported; the file has it."),
            _ => eprintln!("oneshot: it couldn't be reported."),
        }
    }));
}

/// The report: what the page sends (`web/boot.js`, `reportDoc`), as far as
/// the desktop knows it.
pub fn doc(line: &str) -> String {
    let text: String = line.chars().take(4000).collect();
    serde_json::json!({
        "kind": "crash",
        "text": text,
        "build": {
            "version": env!("CARGO_PKG_VERSION"),
            "proto": proto::PROTO_VER,
        },
        "where": {
            "desktop": true,
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
        },
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_crash_report_is_a_json_object_with_the_panics_words() {
        let doc = super::doc("panicked at src/x.rs:1:2:\n\"boom\"");
        let v: serde_json::Value = serde_json::from_str(&doc).unwrap();
        assert_eq!(v["kind"], "crash");
        assert_eq!(v["text"], "panicked at src/x.rs:1:2:\n\"boom\"");
        assert_eq!(v["where"]["desktop"], true);
        assert!(doc.len() < 16 * 1024, "the server's BODY_MAX");
    }
}

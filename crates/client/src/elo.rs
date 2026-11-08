//! The Elo Pros launcher, from the desktop: who is playing, proved to the
//! server. After Gates' `crates/client/src/elo.rs`.
//!
//! `elo_overlay.rs` beside this file is the platform's Rust SDK, **vendored
//! byte for byte** from `sdk/rust/elo_overlay.rs` in the Elo Pros repo
//! (`sdk/SHA256SUMS` there names its hash; the test below pins it). Fix it
//! there and copy it back.
//!
//! The sign-in is the launcher's `prove` (`sdk/PROTOCOL.md`): we hand it a
//! nonce from our server's `/nonce` and the name the server signs in under
//! (`server.json`'s `domain`); it writes and signs an EIP-4361 message with
//! the player's wallet, with no prompt for our own hosts. The pass is
//! `siwe.` b64url(message) `.` b64url(signature), checked by the server's
//! `auth::verify_launcher`. No launcher is a normal state: the player plays
//! as a guest, and the browser's sign-in still works on the page.

#[cfg(not(target_arch = "wasm32"))]
#[path = "elo_overlay.rs"]
#[allow(clippy::all)]
#[rustfmt::skip]
pub mod overlay;

/// This game's slug on Elo Pros: the launcher writes it into the message.
pub const SLUG: &str = "oneshot";

/// sha256 of the vendored `elo_overlay.rs`, as `sdk/SHA256SUMS` lists it.
pub const VENDORED_SHA256: &str = "a39278c38b557d7d65eb328101f67d4da44011f8c35de8365cc0b118a56a69d3";

/// Whether a launcher's door is there to knock on (its socket, or the
/// Windows pipe), without opening it.
#[cfg(not(target_arch = "wasm32"))]
pub fn launcher_here() -> bool {
    if std::env::var_os(overlay::SOCKET_ENV).is_some_and(|s| !s.is_empty()) {
        return true;
    }
    overlay::default_socket().exists()
}

/// Signs in through the launcher against the server whose page is at
/// `server` (`https://game.elopros.com/oneshot`): the pass, and its wallet.
#[cfg(not(target_arch = "wasm32"))]
pub fn prove(server: &str) -> Result<crate::door::Pass, String> {
    let base = server.trim_end_matches('/');
    let info = crate::door::http_get(&format!("{base}/server.json"))?;
    let domain = json_str(&info, "domain")
        .filter(|d| !d.is_empty())
        .unwrap_or_else(|| host_of(base).to_string());
    let nonce = json_str(&crate::door::http_get(&format!("{base}/nonce"))?, "nonce")
        .ok_or_else(|| format!("{base}/nonce: no nonce"))?;
    let mut ov = overlay::Overlay::connect(SLUG, env!("CARGO_PKG_VERSION"))
        .map_err(|_| "Open oneshot from the Elo Pros launcher to sign in".to_string())?;
    let proof = ov.prove(&domain, &nonce).map_err(|e| format!("Sign-in failed: {e}"))?;
    let sig = unhex(proof.signature.trim_start_matches("0x")).ok_or("Sign-in failed: the launcher's signature")?;
    let pass = format!("siwe.{}.{}", b64url(proof.message.as_bytes()), b64url(&sig));
    if proto::Pass::new(&pass).is_none() {
        return Err("Sign-in failed: the launcher's message is too long".into());
    }
    let wallet = proof
        .address
        .map(|a| a.to_ascii_lowercase())
        .ok_or("Sign-in failed: the launcher named no wallet")?;
    Ok(crate::door::Pass {
        pass,
        wallet: Some(wallet),
    })
}

/// `https://host:port/path` → `host:port`.
fn host_of(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    rest.split('/').next().unwrap_or(rest)
}

/// A string field of a small flat JSON object.
fn json_str(body: &str, key: &str) -> Option<String> {
    let at = body.find(&format!("\"{key}\""))? + key.len() + 2;
    let rest = body[at..]
        .trim_start()
        .strip_prefix(':')?
        .trim_start()
        .strip_prefix('"')?;
    Some(rest[..rest.find('"')?].to_string())
}

fn unhex(h: &str) -> Option<Vec<u8>> {
    h.len()
        .is_multiple_of(2)
        .then(|| {
            (0..h.len() / 2)
                .map(|i| u8::from_str_radix(h.get(2 * i..2 * i + 2)?, 16).ok())
                .collect()
        })
        .flatten()
}

fn b64url(b: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(b.len().div_ceil(3) * 4);
    for c in b.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..=c.len() {
            out.push(A[(n >> (18 - 6 * i) & 63) as usize] as char);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_vendored_sdk_has_not_been_edited_here() {
        use sha2::Digest;
        let got = sha2::Sha256::digest(include_bytes!("elo_overlay.rs"));
        let hex: String = got.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(
            hex, VENDORED_SHA256,
            "src/elo_overlay.rs is vendored from the Elo Pros repo's sdk/rust/: fix it there and copy it back"
        );
    }

    #[test]
    fn the_pieces_of_a_pass() {
        assert_eq!(b64url(b"f"), "Zg");
        assert_eq!(b64url(b"fo"), "Zm8");
        assert_eq!(b64url(b"foo"), "Zm9v");
        assert_eq!(b64url(&[0xfb, 0xff]), "-_8");
        assert_eq!(unhex("00ff1b"), Some(vec![0, 255, 27]));
        assert_eq!(unhex("0"), None);
        assert_eq!(host_of("https://game.elopros.com/oneshot"), "game.elopros.com");
        assert_eq!(host_of("http://127.0.0.1:8080"), "127.0.0.1:8080");
        let info = r#"{"addr":"a:1","hash":null,"signin":true,"proto":35,"domain":"elopros.com"}"#;
        assert_eq!(json_str(info, "domain").as_deref(), Some("elopros.com"));
        assert_eq!(json_str(info, "nonce"), None);
    }
}

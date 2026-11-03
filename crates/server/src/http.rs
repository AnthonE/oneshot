//! The page, served plain: the built `crates/client/dist` (ci/client.sh), and `/server.json` telling
//! it where the game listens and the certificate hash to pin. Plain HTTP on
//! localhost is a secure context, which WebTransport needs.
//!
//! In production nginx on the game box stands in front with the real
//! certificate (`ops/game-elopros-oneshot.nginx.conf`) and the page is on
//! elopros.com: `server.json` names the public address and no hash (the
//! browser checks a CA's certificate itself), and answers the page's origin
//! (`--allow-origin`) for CORS. `/health` says the process is up, `/ready`
//! whether it takes players (503 while it drains).

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

pub async fn serve(
    at: SocketAddr,
    dir: PathBuf,
    game: SocketAddr,
    hash: Option<[u8; 32]>,
    ctx: std::sync::Arc<crate::Ctx>,
) -> Result<SocketAddr, String> {
    let listener = TcpListener::bind(at).await.map_err(|e| format!("http {at}: {e}"))?;
    let local = listener.local_addr().map_err(|e| e.to_string())?;
    tokio::spawn(async move {
        while let Ok((sock, _)) = listener.accept().await {
            let dir = dir.clone();
            let ctx = ctx.clone();
            tokio::spawn(async move {
                let _ = handle(sock, &dir, game, hash, &ctx).await;
            });
        }
    });
    Ok(local)
}

fn mime(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript",
        "css" => "text/css",
        "json" => "application/json",
        "wasm" => "application/wasm",
        "glb" => "model/gltf-binary",
        "ktx2" => "image/ktx2",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "ogg" => "audio/ogg",
        "svg" => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

async fn handle(
    mut sock: TcpStream,
    dir: &Path,
    game: SocketAddr,
    hash: Option<[u8; 32]>,
    ctx: &crate::Ctx,
) -> std::io::Result<()> {
    let mut req = vec![0u8; 8192];
    let n = sock.read(&mut req).await?;
    let text = String::from_utf8_lossy(&req[..n]);
    let mut lines = text.lines();
    let first = lines.next().unwrap_or("");
    let mut parts = first.split_whitespace();
    let (method, target) = (parts.next().unwrap_or(""), parts.next().unwrap_or("/"));
    let header = |name: &str| {
        text.lines()
            .skip(1)
            .take_while(|l| !l.is_empty())
            .find_map(|l| l.split_once(':').filter(|(k, _)| k.trim().eq_ignore_ascii_case(name)))
            .map(|(_, v)| v.trim().to_string())
    };
    let host = header("host")
        .map(|h| h.rsplit_once(':').map_or(h.as_str(), |(h, _)| h).to_string())
        .unwrap_or_else(|| "127.0.0.1".into());
    // A page on an origin we were told to answer may read what we say.
    let cors = header("origin").filter(|o| ctx.allow_origins.iter().any(|a| a == o));
    let cors = cors.as_deref();
    let path = target.split('?').next().unwrap_or("/");
    if method == "OPTIONS" && cors.is_some() {
        return reply_cors(&mut sock, 204, "text/plain", b"", cors).await;
    }
    if method != "GET" {
        return reply(&mut sock, 405, "text/plain", b"GET only").await;
    }
    if path == "/server.json" {
        let live = match &ctx.live_file {
            Some(f) => tokio::fs::read_to_string(f).await.ok().and_then(|s| live_addr(&s)),
            None => None,
        };
        let body = server_json(ctx, &host, game, hash, live.as_deref());
        return reply_cors(&mut sock, 200, "application/json", body.as_bytes(), cors).await;
    }
    if path == "/nonce" {
        // For a pass: the page asks Elo Pros to sign this, once.
        let body = format!("{{\"nonce\":\"{}\",\"game\":\"{}\"}}", ctx.nonces.issue(), ctx.slug);
        return reply_cors(&mut sock, 200, "application/json", body.as_bytes(), cors).await;
    }
    if path == "/health" {
        return reply(&mut sock, 200, "text/plain", b"ok").await;
    }
    if path == "/ready" {
        let draining = ctx.draining.load(std::sync::atomic::Ordering::Relaxed);
        let body = format!(
            "{{\"ready\":{},\"proto\":{},\"players\":{},\"capacity\":{},\"rooms\":{}}}",
            !draining,
            proto::PROTO_VER,
            ctx.gate.busy(),
            ctx.gate.capacity,
            ctx.rooms.load(std::sync::atomic::Ordering::Relaxed)
        );
        let code = if draining { 503 } else { 200 };
        return reply_cors(&mut sock, code, "application/json", body.as_bytes(), cors).await;
    }
    let rel = path.trim_start_matches('/');
    if rel.split('/').any(|s| s == ".." || s.starts_with('.')) {
        return reply(&mut sock, 404, "text/plain", b"not found").await;
    }
    let mut file = dir.join(if rel.is_empty() { "index.html" } else { rel });
    if file.is_dir() {
        file = file.join("index.html");
    }
    match tokio::fs::read(&file).await {
        Ok(body) => reply(&mut sock, 200, mime(&file), &body).await,
        // A page served from here finds the game here (`origin.json` is what
        // a published page is given to find the game box).
        Err(_) if path == "/origin.json" => reply(&mut sock, 200, "application/json", b"{}").await,
        Err(_) => reply(&mut sock, 404, "text/plain", b"not found").await,
    }
}

/// Where the page connects: the live slot's address (`live`, read from
/// `--live-file`), else the public address if we were given one (the game
/// box behind its own name), else the host the page asked on at the bound
/// port; and the certificate's hash only when it is ours to pin.
pub fn server_json(
    ctx: &crate::Ctx,
    host: &str,
    game: SocketAddr,
    hash: Option<[u8; 32]>,
    live: Option<&str>,
) -> String {
    let addr = live
        .map(str::to_string)
        .or_else(|| ctx.public_addr.clone())
        .unwrap_or_else(|| format!("{host}:{}", game.port()));
    let hash = hash.map_or("null".to_string(), |h| format!("\"{}\"", crate::hex(&h)));
    format!(
        "{{\"addr\":\"{addr}\",\"hash\":{hash},\"game\":\"{}\",\"signin\":{},\"proto\":{}}}",
        ctx.slug,
        ctx.elo_pubkey.is_some(),
        proto::PROTO_VER
    )
}

/// A live file's `host:port`, if that's what it holds (it is written by
/// `ci/deploy_server.sh`; anything else is ignored, never sent on).
pub fn live_addr(s: &str) -> Option<String> {
    let s = s.trim();
    let (host, port) = s.rsplit_once(':')?;
    let host_ok = !host.is_empty()
        && host.len() <= 253
        && host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-');
    (host_ok && port.parse::<u16>().is_ok_and(|p| p > 0)).then(|| s.to_string())
}

async fn reply(sock: &mut TcpStream, code: u16, kind: &str, body: &[u8]) -> std::io::Result<()> {
    reply_cors(sock, code, kind, body, None).await
}

async fn reply_cors(
    sock: &mut TcpStream,
    code: u16,
    kind: &str,
    body: &[u8],
    cors: Option<&str>,
) -> std::io::Result<()> {
    let status = match code {
        200 => "OK",
        204 => "No Content",
        404 => "Not Found",
        503 => "Service Unavailable",
        _ => "Method Not Allowed",
    };
    let cors = cors.map_or(String::new(), |o| {
        format!("Access-Control-Allow-Origin: {o}\r\nVary: Origin\r\nAccess-Control-Allow-Methods: GET\r\n")
    });
    let head = format!(
        "HTTP/1.1 {code} {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nCache-Control: no-cache\r\n{cors}Connection: close\r\n\r\n",
        body.len()
    );
    sock.write_all(head.as_bytes()).await?;
    sock.write_all(body).await?;
    sock.flush().await
}

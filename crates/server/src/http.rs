//! The page, served plain: the built `crates/client/dist` (the Bevy
//! client; `--web web/dist` for the three.js page while it lasts), and
//! `/server.json` telling it where the game listens and the certificate
//! hash to pin. Plain HTTP on
//! localhost is a secure context, which WebTransport needs. In production a
//! real web server and a real certificate stand in front.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

pub async fn serve(
    at: SocketAddr,
    dir: PathBuf,
    game: SocketAddr,
    hash: [u8; 32],
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
    hash: [u8; 32],
    ctx: &crate::Ctx,
) -> std::io::Result<()> {
    let mut req = vec![0u8; 8192];
    let n = sock.read(&mut req).await?;
    let text = String::from_utf8_lossy(&req[..n]);
    let mut lines = text.lines();
    let first = lines.next().unwrap_or("");
    let mut parts = first.split_whitespace();
    let (method, target) = (parts.next().unwrap_or(""), parts.next().unwrap_or("/"));
    let host = lines
        .find_map(|l| l.strip_prefix("Host:").or_else(|| l.strip_prefix("host:")))
        .map(|h| h.trim().rsplit_once(':').map_or(h.trim(), |(h, _)| h).to_string())
        .unwrap_or_else(|| "127.0.0.1".into());
    let path = target.split('?').next().unwrap_or("/");
    if method != "GET" {
        return reply(&mut sock, 405, "text/plain", b"GET only").await;
    }
    if path == "/server.json" {
        // The game is on the same host the page came from.
        let body = format!(
            "{{\"addr\":\"{}:{}\",\"hash\":\"{}\",\"game\":\"{}\",\"signin\":{}}}",
            host,
            game.port(),
            crate::hex(&hash),
            ctx.slug,
            ctx.elo_pubkey.is_some()
        );
        return reply(&mut sock, 200, "application/json", body.as_bytes()).await;
    }
    if path == "/nonce" {
        // For a pass: the page asks Elo Pros to sign this, once.
        let body = format!("{{\"nonce\":\"{}\",\"game\":\"{}\"}}", ctx.nonces.issue(), ctx.slug);
        return reply(&mut sock, 200, "application/json", body.as_bytes()).await;
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
        Err(_) => reply(&mut sock, 404, "text/plain", b"not found").await,
    }
}

async fn reply(sock: &mut TcpStream, code: u16, kind: &str, body: &[u8]) -> std::io::Result<()> {
    let status = match code {
        200 => "OK",
        404 => "Not Found",
        _ => "Method Not Allowed",
    };
    let head = format!(
        "HTTP/1.1 {code} {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n",
        body.len()
    );
    sock.write_all(head.as_bytes()).await?;
    sock.write_all(body).await?;
    sock.flush().await
}

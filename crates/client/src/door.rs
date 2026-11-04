//! The way into a match, from inside the app. After Gates' `render/menu.rs`:
//! connecting is a state of the title screen, not a precondition of the
//! app, so a refusal or a dead server lands back on the title with the
//! reason instead of an exit.
//!
//! A [`Door`] knows where the server is (the page's `server.json`, or the
//! command line's `--server`) and, in a browser, how to sign in with Elo
//! Pros (the page's popup). [`Door::knock`] starts a connection and returns
//! a [`Knock`] the title polls each frame: lines from the stream while it
//! waits (the queue, a cup, a refusal), then the link at the welcome, or why
//! not. The wait itself is the one the page's `play()` did: tick with no
//! controls, keep what the stream says for the HUD, and once, on a server
//! going down (refused as full), follow `server.json` to the live one.

use proto::Msg;

use crate::link::Link;

/// What the title asks for.
#[derive(Clone, Debug, Default)]
pub struct Ask {
    pub name: String,
    /// An Elo Pros pass, or empty for a guest.
    pub pass: String,
    pub mode: u8,
    pub skin: u8,
}

/// What a knock heard.
pub enum News {
    /// A line for whoever is waiting.
    Line(String),
    /// The welcome: the link, and what the stream said before it.
    In(Box<dyn Link>, Vec<Msg>),
    /// No way in, and why (empty: the last line said it).
    Failed(String),
}

/// Signed in with Elo Pros: the pass, and the wallet it carries, if any.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pass {
    pub pass: String,
    pub wallet: Option<String>,
}

/// Where sign-in stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SignIn {
    /// Not here, and why (the desktop; a server with ranked off).
    Off(String),
    Ready,
    /// The popup is open.
    Asking,
    In(Pass),
    Failed(String),
}

impl SignIn {
    /// Ranked is open: signed in with a wallet.
    pub fn ranked(&self) -> bool {
        matches!(self, SignIn::In(Pass { wallet: Some(_), .. }))
    }

    pub fn pass(&self) -> String {
        match self {
            SignIn::In(p) => p.pass.clone(),
            _ => String::new(),
        }
    }
}

/// The wallet as the title shows it: `0x1234…abcd`.
pub fn short_wallet(w: &str) -> String {
    if w.len() > 10 {
        format!("{}…{}", &w[..6], &w[w.len() - 4..])
    } else {
        w.to_string()
    }
}

/// A server going down refuses as full: the live one is worth one try.
fn moving(m: &Msg) -> bool {
    *m == Msg::Refuse {
        reason: proto::REFUSE_FULL,
    }
}

const MOVING: &str = "Moving to the new server...";

#[cfg(not(target_arch = "wasm32"))]
pub use desktop::*;
#[cfg(target_arch = "wasm32")]
pub use web::*;

#[cfg(not(target_arch = "wasm32"))]
mod desktop {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::{channel, Receiver, Sender};
    use std::sync::Arc;
    use std::time::Duration;

    use client_net::native::NativeClient;
    use client_net::session::Session;

    use super::{moving, Ask, News, SignIn, MOVING};
    use crate::link::Native;

    /// Where the desktop client plays unless told otherwise (ops/README.md).
    pub const PUBLIC: &str = "https://game.elopros.com/oneshot";

    /// The page server whose `server.json` names the game server.
    pub struct Door {
        server: String,
    }

    impl Door {
        pub fn new(server: String) -> Door {
            Door { server }
        }

        /// Where we play, for the title's foot.
        pub fn place(&self) -> String {
            self.server.clone()
        }

        pub fn sign_in(&mut self) {}

        pub fn signin(&self) -> SignIn {
            SignIn::Off("Ranked needs an Elo Pros sign-in: play it in the browser".into())
        }

        pub fn quits(&self) -> bool {
            true
        }

        /// Connects on a thread of its own (`server.json` is a blocking GET).
        pub fn knock(&mut self, ask: Ask) -> Knock {
            let (tx, rx) = channel();
            let cancel = Arc::new(AtomicBool::new(false));
            let (server, stop) = (self.server.clone(), cancel.clone());
            std::thread::spawn(move || {
                if let Err(e) = run(&server, &ask, &tx, &stop) {
                    let _ = tx.send(Heard::Failed(e));
                }
            });
            Knock { rx, cancel }
        }
    }

    enum Heard {
        Line(String),
        In(Box<Native>, Vec<proto::Msg>),
        Failed(String),
    }

    /// A connection on its way.
    pub struct Knock {
        rx: Receiver<Heard>,
        cancel: Arc<AtomicBool>,
    }

    impl Knock {
        pub fn poll(&mut self) -> Vec<News> {
            self.rx
                .try_iter()
                .map(|h| match h {
                    Heard::Line(l) => News::Line(l),
                    Heard::In(link, early) => News::In(link, early),
                    Heard::Failed(e) => News::Failed(e),
                })
                .collect()
        }

        pub fn cancel(&mut self) {
            self.cancel.store(true, Ordering::Relaxed);
        }
    }

    impl Drop for Knock {
        fn drop(&mut self) {
            self.cancel();
        }
    }

    fn run(server: &str, ask: &Ask, tx: &Sender<Heard>, cancel: &AtomicBool) -> Result<(), String> {
        let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
        let say = |l: String| {
            let _ = tx.send(Heard::Line(l));
        };
        say("Connecting...".into());
        let (mut addr, mut hash) = where_is(server)?;
        let mut moved = false;
        'connect: loop {
            let hello = Session::hello(&ask.name, proto::Pass::NONE, ask.mode, ask.skin);
            let mut c = rt
                .block_on(NativeClient::connect(&addr, hash, hello))
                .map_err(|e| format!("{addr}: {e}"))?;
            let mut early = Vec::new();
            loop {
                if cancel.load(Ordering::Relaxed) {
                    c.close();
                    return Ok(());
                }
                {
                    let _g = rt.enter();
                    c.update(sim::input::Input::default());
                }
                for m in c.session.take_messages() {
                    if moving(&m) && !moved {
                        moved = true;
                        if let Ok((a, h)) = where_is(server) {
                            if a != addr {
                                say(MOVING.into());
                                c.close();
                                (addr, hash) = (a, h);
                                continue 'connect;
                            }
                        }
                    }
                    if let Some(line) = crate::hud::status_line(&m) {
                        say(line);
                    }
                    early.push(m);
                }
                if c.session.ready {
                    let _ = tx.send(Heard::In(Box::new(Native { client: c, _rt: rt }), early));
                    return Ok(());
                }
                if c.closed() {
                    return Err(String::new());
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }

    /// The game server `server.json` names, and the certificate it pins.
    fn where_is(server: &str) -> Result<(String, Option<[u8; 32]>), String> {
        let body = fetch::server_json(server)?;
        parse_server_json(&body).ok_or_else(|| format!("{server}: server.json doesn't name a game server"))
    }

    /// server.json's `addr` and, when it pins one, the certificate's hash:
    /// `{"addr": "host:port", "hash": "hex" | null, ...}`.
    pub fn parse_server_json(body: &str) -> Option<(String, Option<[u8; 32]>)> {
        let field = |k: &str| {
            let at = body.find(&format!("\"{k}\""))? + k.len() + 2;
            let rest = body[at..].trim_start().strip_prefix(':')?.trim_start();
            let rest = rest.strip_prefix('"')?;
            Some(rest[..rest.find('"')?].to_string())
        };
        let addr = field("addr")?;
        let hash = field("hash").and_then(|h| {
            let h: String = h.chars().filter(|c| c.is_ascii_hexdigit()).collect();
            (h.len() == 64).then(|| {
                let mut out = [0u8; 32];
                for (i, b) in out.iter_mut().enumerate() {
                    *b = u8::from_str_radix(&h[2 * i..2 * i + 2], 16).unwrap_or(0);
                }
                out
            })
        });
        Some((addr, hash))
    }

    /// `GET <server>/server.json`, over HTTPS (the system's roots) or plain
    /// HTTP (a local server): one request, HTTP/1.0, the body back.
    mod fetch {
        use std::io::{Read, Write};
        use std::net::TcpStream;
        use std::sync::Arc;
        use std::time::Duration;

        pub fn server_json(server: &str) -> Result<String, String> {
            let (tls, rest) = match server.split_once("://") {
                Some(("https", r)) => (true, r),
                Some(("http", r)) => (false, r),
                Some((s, _)) => return Err(format!("{server}: http:// or https://, not {s}://")),
                None => (false, server),
            };
            let (hostport, path) = match rest.find('/') {
                Some(i) => (&rest[..i], rest[i..].trim_end_matches('/')),
                None => (rest, ""),
            };
            let (host, port) = match hostport.rsplit_once(':') {
                Some((h, p)) if !h.ends_with(']') || h.starts_with('[') => {
                    (h, p.parse::<u16>().map_err(|_| format!("{server}: bad port {p}"))?)
                }
                _ => (hostport, if tls { 443 } else { 80 }),
            };
            let at = format!("{host}:{port}");
            let tcp = std::net::ToSocketAddrs::to_socket_addrs(&at)
                .map_err(|e| format!("{at}: {e}"))?
                .find_map(|a| TcpStream::connect_timeout(&a, Duration::from_secs(10)).ok())
                .ok_or_else(|| format!("{at}: can't connect (is the server up?)"))?;
            let _ = tcp.set_read_timeout(Some(Duration::from_secs(10)));
            let req = format!(
                "GET {path}/server.json HTTP/1.0\r\nHost: {host}\r\nUser-Agent: oneshot-desktop\r\nConnection: close\r\n\r\n"
            );
            let mut raw = Vec::new();
            if tls {
                let mut roots = rustls::RootCertStore::empty();
                for c in rustls_native_certs::load_native_certs().certs {
                    let _ = roots.add(c);
                }
                let cfg =
                    rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                        .with_safe_default_protocol_versions()
                        .map_err(|e| e.to_string())?
                        .with_root_certificates(roots)
                        .with_no_client_auth();
                let name =
                    rustls::pki_types::ServerName::try_from(host.to_string()).map_err(|e| format!("{host}: {e}"))?;
                let conn = rustls::ClientConnection::new(Arc::new(cfg), name).map_err(|e| e.to_string())?;
                let mut s = rustls::StreamOwned::new(conn, tcp);
                s.write_all(req.as_bytes()).map_err(|e| format!("{at}: {e}"))?;
                // A server that closes without TLS's close_notify still sent the body.
                if let Err(e) = s.read_to_end(&mut raw) {
                    if raw.is_empty() {
                        return Err(format!("{at}: {e}"));
                    }
                }
            } else {
                let mut s = tcp;
                s.write_all(req.as_bytes()).map_err(|e| format!("{at}: {e}"))?;
                s.read_to_end(&mut raw).map_err(|e| format!("{at}: {e}"))?;
            }
            let text = String::from_utf8_lossy(&raw);
            let (head, body) = text.split_once("\r\n\r\n").ok_or_else(|| format!("{at}: no reply"))?;
            let status = head.split_whitespace().nth(1).unwrap_or("");
            if status != "200" {
                return Err(format!("{server}/server.json: HTTP {status}"));
            }
            Ok(body.to_string())
        }
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn server_json_reads_both_forms() {
            let (a, h) =
                super::parse_server_json(r#"{"addr":"game.example.com:4433","hash":null,"game":"oneshot"}"#).unwrap();
            assert_eq!(a, "game.example.com:4433");
            assert!(h.is_none());
            let hex = "ab".repeat(32);
            let (a, h) =
                super::parse_server_json(&format!(r#"{{"addr": "127.0.0.1:4433", "hash": "{hex}"}}"#)).unwrap();
            assert_eq!(a, "127.0.0.1:4433");
            assert_eq!(h, Some([0xab; 32]));
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod web {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    use client_net::web::Client;
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::{spawn_local, JsFuture};

    use super::{moving, Ask, News, Pass, SignIn, MOVING};

    /// What the page gave: the game server (from `server.json`), how to
    /// find the live one (`follow`), and how to sign in (`signin`, or none).
    pub struct Door {
        at: Rc<RefCell<(String, String)>>,
        follow: js_sys::Function,
        signin_fn: Option<js_sys::Function>,
        signin: Rc<RefCell<SignIn>>,
    }

    impl Door {
        pub fn new(addr: String, hash: String, follow: js_sys::Function, signin: JsValue, off: String) -> Door {
            let signin_fn = signin.dyn_into::<js_sys::Function>().ok();
            let state = if signin_fn.is_some() {
                SignIn::Ready
            } else {
                SignIn::Off(if off.is_empty() {
                    "Ranked is off on this server".into()
                } else {
                    off
                })
            };
            Door {
                at: Rc::new(RefCell::new((addr, hash))),
                follow,
                signin_fn,
                signin: Rc::new(RefCell::new(state)),
            }
        }

        pub fn place(&self) -> String {
            web_sys::window()
                .and_then(|w| w.location().host().ok())
                .unwrap_or_default()
        }

        pub fn quits(&self) -> bool {
            false
        }

        pub fn signin(&self) -> SignIn {
            self.signin.borrow().clone()
        }

        /// Opens Elo Pros' popup (the page's function: it has the nonce
        /// ready, so the popup opens on this click).
        pub fn sign_in(&mut self) {
            let Some(f) = self.signin_fn.clone() else {
                return;
            };
            if matches!(*self.signin.borrow(), SignIn::Asking | SignIn::Off(_)) {
                return;
            }
            *self.signin.borrow_mut() = SignIn::Asking;
            let state = self.signin.clone();
            spawn_local(async move {
                let got = match f.call0(&JsValue::NULL).and_then(|p| p.dyn_into::<js_sys::Promise>()) {
                    Ok(p) => JsFuture::from(p).await,
                    Err(e) => Err(e),
                };
                let get = |o: &JsValue, k: &str| {
                    js_sys::Reflect::get(o, &JsValue::from_str(k))
                        .ok()
                        .and_then(|v| v.as_string())
                };
                *state.borrow_mut() = match got {
                    Ok(o) => match get(&o, "pass") {
                        Some(pass) => SignIn::In(Pass {
                            pass,
                            wallet: get(&o, "wallet"),
                        }),
                        None => SignIn::Ready,
                    },
                    // Closing the popup isn't a failure.
                    Err(e) if matches!(get(&e, "code").as_deref(), Some("cancelled" | "closed")) => SignIn::Ready,
                    Err(e) => SignIn::Failed(format!("Sign-in failed: {}", js_text(&e))),
                };
            });
        }

        pub fn knock(&mut self, ask: Ask) -> Knock {
            let out = Rc::new(RefCell::new(Vec::new()));
            let cancel = Rc::new(Cell::new(false));
            let (at, follow, o, c) = (self.at.clone(), self.follow.clone(), out.clone(), cancel.clone());
            spawn_local(async move {
                if let Err(e) = run(at, follow, ask, o.clone(), c).await {
                    o.borrow_mut().push(News::Failed(js_text(&e)));
                }
            });
            Knock { out, cancel }
        }
    }

    /// A connection on its way.
    pub struct Knock {
        out: Rc<RefCell<Vec<News>>>,
        cancel: Rc<Cell<bool>>,
    }

    impl Knock {
        pub fn poll(&mut self) -> Vec<News> {
            core::mem::take(&mut *self.out.borrow_mut())
        }

        pub fn cancel(&mut self) {
            self.cancel.set(true);
        }
    }

    impl Drop for Knock {
        fn drop(&mut self) {
            self.cancel();
        }
    }

    fn js_text(e: &JsValue) -> String {
        if let Some(e) = e.dyn_ref::<js_sys::Error>() {
            return String::from(e.message());
        }
        e.as_string().unwrap_or_else(|| format!("{e:?}"))
    }

    async fn sleep(ms: i32) {
        let p = js_sys::Promise::new(&mut |done, _| {
            if let Some(w) = web_sys::window() {
                let _ = w.set_timeout_with_callback_and_timeout_and_arguments_0(&done, ms);
            }
        });
        let _ = JsFuture::from(p).await;
    }

    /// The page's old `play()` loop: connect, wait for the welcome with no
    /// controls, follow a server going down once.
    async fn run(
        at: Rc<RefCell<(String, String)>>,
        follow: js_sys::Function,
        ask: Ask,
        out: Rc<RefCell<Vec<News>>>,
        cancel: Rc<Cell<bool>>,
    ) -> Result<(), JsValue> {
        let push = |n: News| out.borrow_mut().push(n);
        let connect = |(addr, hash): (String, String)| {
            Client::connect(addr, hash, ask.name.clone(), ask.pass.clone(), ask.mode, ask.skin)
        };
        push(News::Line("Connecting...".into()));
        let first = at.borrow().clone();
        let mut c = connect(first).await?;
        let mut moved = false;
        let mut early = Vec::new();
        'wait: loop {
            if cancel.get() {
                c.close();
                return Ok(());
            }
            c.tick(sim::input::Input::default());
            for m in c.session_mut().take_messages() {
                if moving(&m) && !moved {
                    moved = true;
                    let to = follow.call0(&JsValue::NULL)?.dyn_into::<js_sys::Promise>()?;
                    if let Some(a) = JsFuture::from(to).await?.dyn_ref::<js_sys::Array>() {
                        push(News::Line(MOVING.into()));
                        c.close();
                        let next = (
                            a.get(0).as_string().unwrap_or_default(),
                            a.get(1).as_string().unwrap_or_default(),
                        );
                        *at.borrow_mut() = next.clone();
                        c = connect(next).await?;
                        early.clear();
                        continue 'wait;
                    }
                }
                if let Some(line) = crate::hud::status_line(&m) {
                    push(News::Line(line));
                }
                early.push(m);
            }
            if c.ready() {
                push(News::In(Box::new(crate::link::Web(c)), early));
                return Ok(());
            }
            if c.closed() {
                push(News::Failed(String::new()));
                return Ok(());
            }
            sleep(100).await;
        }
    }
}

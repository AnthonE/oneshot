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
    pub look: proto::Look,
    /// The party it comes with (its code; 0 for none), seated with it.
    pub party: u32,
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

/// Elo Pros' queue put the match on another server: where, and the match.
fn moved_to(m: &Msg) -> Option<(String, String)> {
    match m {
        Msg::Moved { addr, queued } => Some((addr.as_str().to_string(), queued.as_str().to_string())),
        _ => None,
    }
}

pub(crate) const MATCHED: &str = "Matched: joining the match's server...";

/// The menu's session as the game holds it (`party.rs`): the party's
/// messages and voice in, its acts and our voice out. Never waits.
pub trait Lobby {
    fn take(&mut self) -> Vec<Msg>;
    fn take_voice(&mut self) -> Vec<proto::voice::VoiceFrame>;
    fn send(&mut self, m: Msg);
    fn talk(&mut self, f: &proto::voice::VoiceFrame) -> bool;
    fn closed(&self) -> bool;
    fn close(&mut self);
}

#[cfg(not(target_arch = "wasm32"))]
pub type LobbyBox = Box<dyn Lobby + Send>;
#[cfg(target_arch = "wasm32")]
pub type LobbyBox = Box<dyn Lobby>;

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

    use super::{moved_to, moving, Ask, News, SignIn, MATCHED, MOVING};
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
            SignIn::Off("Ranked plays in the browser".into())
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

    impl Door {
        /// Opens the menu's session (`hello` asks for `MODE_LOBBY`) on a
        /// thread of its own, as a knock does.
        pub fn lobby(&self, hello: proto::Msg) -> LobbyKnock {
            let (tx, rx) = channel();
            let server = self.server.clone();
            std::thread::spawn(move || {
                let got = (|| {
                    let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
                    let (addr, hash) = where_is(&server)?;
                    let lobby = rt
                        .block_on(client_net::native::NativeLobby::connect(&addr, hash, hello))
                        .map_err(|e| format!("{addr}: {e}"))?;
                    Ok(Box::new(NativeParty { lobby, _rt: rt }) as super::LobbyBox)
                })();
                let _ = tx.send(got);
            });
            LobbyKnock { rx }
        }
    }

    /// The menu's session on its way.
    pub struct LobbyKnock {
        rx: Receiver<Result<super::LobbyBox, String>>,
    }

    impl LobbyKnock {
        pub fn poll(&mut self) -> Option<Result<super::LobbyBox, String>> {
            self.rx.try_recv().ok()
        }
    }

    /// The native lobby, and the runtime its readers run on.
    struct NativeParty {
        lobby: client_net::native::NativeLobby,
        _rt: tokio::runtime::Runtime,
    }

    impl super::Lobby for NativeParty {
        fn take(&mut self) -> Vec<proto::Msg> {
            self.lobby.take()
        }
        fn take_voice(&mut self) -> Vec<proto::voice::VoiceFrame> {
            self.lobby.take_voice()
        }
        fn send(&mut self, m: proto::Msg) {
            self.lobby.send_now(m);
        }
        fn talk(&mut self, f: &proto::voice::VoiceFrame) -> bool {
            self.lobby.send_voice(f)
        }
        fn closed(&self) -> bool {
            self.lobby.closed()
        }
        fn close(&mut self) {
            self.lobby.close();
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
        // A match Elo Pros' queue put on another server: its id.
        let mut queued: Option<proto::QueueMatch> = None;
        'connect: loop {
            // The lobby's ticket, if it has one: who we are to the party.
            let pass = proto::Pass::new(&ask.pass).unwrap_or(proto::Pass::NONE);
            let hello = match queued {
                Some(q) => Session::hello_queued(&ask.name, pass, ask.mode, ask.look, q),
                None => Session::hello(&ask.name, pass, ask.mode, ask.look, ask.party),
            };
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
                    if let Some((to, id)) = moved_to(&m) {
                        if queued.is_none() {
                            say(MATCHED.into());
                            c.close();
                            // A server of the game's own list: its
                            // certificate is a CA's, never pinned.
                            (addr, hash, queued) = (to, None, proto::QueueMatch::new(&id));
                            continue 'connect;
                        }
                    }
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

    /// One GET over HTTPS or HTTP: the body (`fetch::get`).
    pub fn http_get(url: &str) -> Result<String, String> {
        fetch::get(url)
    }

    /// One POST of `body` (as text) over HTTPS or HTTP: the status.
    pub fn http_post(url: &str, body: &[u8]) -> Result<u16, String> {
        fetch::request(url, "POST", body).map(|(status, _)| status)
    }

    mod fetch {
        use std::io::{Read, Write};
        use std::net::TcpStream;
        use std::sync::Arc;
        use std::time::Duration;

        pub fn server_json(server: &str) -> Result<String, String> {
            get(&format!("{}/server.json", server.trim_end_matches('/')))
        }

        /// `GET url`, HTTP/1.0, the body back on a 200.
        pub fn get(server: &str) -> Result<String, String> {
            let (status, body) = request(server, "GET", b"")?;
            if status != 200 {
                return Err(format!("{server}: HTTP {status}"));
            }
            Ok(body)
        }

        /// `method url` over HTTPS (the system's roots) or plain HTTP (a
        /// local server), HTTP/1.0, with `body` (a POST's, as text): the
        /// status and the body.
        pub fn request(server: &str, method: &str, body: &[u8]) -> Result<(u16, String), String> {
            let (tls, rest) = match server.split_once("://") {
                Some(("https", r)) => (true, r),
                Some(("http", r)) => (false, r),
                Some((s, _)) => return Err(format!("{server}: http:// or https://, not {s}://")),
                None => (false, server),
            };
            let (hostport, path) = match rest.find('/') {
                Some(i) => (&rest[..i], &rest[i..]),
                None => (rest, "/"),
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
            let mut req = format!(
                "{method} {path} HTTP/1.0\r\nHost: {host}\r\nUser-Agent: oneshot-desktop\r\nConnection: close\r\n"
            )
            .into_bytes();
            if method == "POST" {
                req.extend_from_slice(
                    format!("Content-Type: text/plain\r\nContent-Length: {}\r\n", body.len()).as_bytes(),
                );
            }
            req.extend_from_slice(b"\r\n");
            req.extend_from_slice(body);
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
                s.write_all(&req).map_err(|e| format!("{at}: {e}"))?;
                // A server that closes without TLS's close_notify still sent the body.
                if let Err(e) = s.read_to_end(&mut raw) {
                    if raw.is_empty() {
                        return Err(format!("{at}: {e}"));
                    }
                }
            } else {
                let mut s = tcp;
                s.write_all(&req).map_err(|e| format!("{at}: {e}"))?;
                s.read_to_end(&mut raw).map_err(|e| format!("{at}: {e}"))?;
            }
            let text = String::from_utf8_lossy(&raw);
            let (head, body) = text.split_once("\r\n\r\n").ok_or_else(|| format!("{at}: no reply"))?;
            let status = head.split_whitespace().nth(1).and_then(|s| s.parse().ok()).unwrap_or(0);
            Ok((status, body.to_string()))
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

    use super::{moved_to, moving, Ask, News, Pass, SignIn, MATCHED, MOVING};

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

    impl Door {
        /// Opens the menu's session (`hello` asks for `MODE_LOBBY`).
        pub fn lobby(&self, hello: proto::Msg) -> LobbyKnock {
            let slot: Rc<RefCell<Option<Result<super::LobbyBox, String>>>> = Rc::default();
            let (addr, hash) = self.at.borrow().clone();
            let s = slot.clone();
            spawn_local(async move {
                let got = client_net::web_lobby::WebLobby::open(addr, hash, hello).await;
                // Given up on already (it took too long): close it, unheld.
                if Rc::strong_count(&s) == 1 {
                    if let Ok(l) = got {
                        l.close();
                    }
                    return;
                }
                *s.borrow_mut() = Some(got.map(|l| Box::new(l) as super::LobbyBox).map_err(|e| js_text(&e)));
            });
            LobbyKnock { slot }
        }
    }

    /// The menu's session on its way.
    pub struct LobbyKnock {
        slot: Rc<RefCell<Option<Result<super::LobbyBox, String>>>>,
    }

    impl LobbyKnock {
        pub fn poll(&mut self) -> Option<Result<super::LobbyBox, String>> {
            self.slot.borrow_mut().take()
        }
    }

    impl super::Lobby for client_net::web_lobby::WebLobby {
        fn take(&mut self) -> Vec<proto::Msg> {
            client_net::web_lobby::WebLobby::take(self)
        }
        fn take_voice(&mut self) -> Vec<proto::voice::VoiceFrame> {
            client_net::web_lobby::WebLobby::take_voice(self)
        }
        fn send(&mut self, m: proto::Msg) {
            client_net::web_lobby::WebLobby::send(self, m);
        }
        fn talk(&mut self, f: &proto::voice::VoiceFrame) -> bool {
            client_net::web_lobby::WebLobby::talk(self, f)
        }
        fn closed(&self) -> bool {
            client_net::web_lobby::WebLobby::closed(self)
        }
        fn close(&mut self) {
            client_net::web_lobby::WebLobby::close(self);
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
        let connect_for = |(addr, hash): (String, String), queued: String| {
            Client::connect_queued(
                addr,
                hash,
                ask.name.clone(),
                ask.pass.clone(),
                ask.mode,
                ask.look.skin,
                ask.look.emblem,
                ask.look.headwear,
                ask.look.footwear,
                ask.party,
                queued,
            )
        };
        let connect = |at: (String, String)| connect_for(at, String::new());
        push(News::Line("Connecting...".into()));
        let first = at.borrow().clone();
        let mut c = connect(first).await?;
        let mut moved = false;
        let mut matched = false;
        let mut early = Vec::new();
        'wait: loop {
            if cancel.get() {
                c.close();
                return Ok(());
            }
            c.tick(sim::input::Input::default());
            for m in c.session_mut().take_messages() {
                if let Some((to, id)) = moved_to(&m) {
                    if !matched {
                        matched = true;
                        push(News::Line(MATCHED.into()));
                        c.close();
                        // A server of the game's own list: its certificate is
                        // a CA's, never pinned.
                        let next = (to, String::new());
                        *at.borrow_mut() = next.clone();
                        c = connect_for(next, id).await?;
                        early.clear();
                        continue 'wait;
                    }
                }
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
                push(News::In(Box::new(crate::link::Web::new(c)), early));
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

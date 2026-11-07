//! The connection as the client sees it: a session (client-net) on
//! whichever transport this build has. The client reads the session and
//! hands it the controls; it never decides anything the session or the
//! server owns.

use client_net::session::Session;
use sim::input::Input;

pub trait Link {
    /// Once a frame: what arrived in, the controls out.
    fn update(&mut self, input: Input);
    fn session(&self) -> &Session;
    /// For draining what the session queued for the app (events, stream
    /// messages); never for steering it.
    fn session_mut(&mut self) -> &mut Session;
    /// The clock the session's interpolation runs on, in ms.
    fn now_ms(&self) -> f64;
    /// The server closed the stream (a refusal, the match's end, a drain).
    fn closed(&self) -> bool;
    /// Why the connection ended, in the player's words
    /// (`client_net::session::close_why`), once it has; None before, or
    /// for a stand-in.
    fn ended_why(&self) -> Option<String> {
        None
    }
    /// We're leaving: close the connection (the server lets the seat go).
    fn leave(&mut self) {}
    /// A real connection, not a stand-in.
    fn live(&self) -> bool {
        true
    }
    /// A tape's link (film mode, `film.rs`), whose clock the film moves.
    fn tape(&mut self) -> Option<&mut Tape> {
        None
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub struct Native {
    pub client: client_net::native::NativeClient,
    /// Runs the connection's readers; kept alive with it.
    pub _rt: tokio::runtime::Runtime,
}

#[cfg(not(target_arch = "wasm32"))]
impl Link for Native {
    fn update(&mut self, input: Input) {
        // The readers' tasks run on the runtime's threads; this only
        // drains what they queued and sends what's due.
        let _g = self._rt.enter();
        self.client.update(input);
    }
    fn session(&self) -> &Session {
        &self.client.session
    }
    fn session_mut(&mut self) -> &mut Session {
        &mut self.client.session
    }
    fn now_ms(&self) -> f64 {
        self.client.now_ms()
    }
    fn closed(&self) -> bool {
        self.client.closed()
    }
    fn ended_why(&self) -> Option<String> {
        self.client.ended_why()
    }
    fn leave(&mut self) {
        self.client.close();
    }
}

#[cfg(target_arch = "wasm32")]
pub struct Web {
    pub client: client_net::web::Client,
    /// Why it ended, once read: whether the tab was asleep is known only
    /// around when it came back.
    why: std::cell::RefCell<Option<String>>,
}

#[cfg(target_arch = "wasm32")]
impl Web {
    pub fn new(client: client_net::web::Client) -> Web {
        Web {
            client,
            why: Default::default(),
        }
    }
}

/// What a browser says when the tab was in the background as it closed.
#[cfg(target_arch = "wasm32")]
const ASLEEP: &str = "the tab was in the background, and the game can't keep up from there";

#[cfg(target_arch = "wasm32")]
impl Link for Web {
    fn update(&mut self, input: Input) {
        self.client.tick(input);
    }
    fn session(&self) -> &Session {
        self.client.session()
    }
    fn session_mut(&mut self) -> &mut Session {
        self.client.session_mut()
    }
    fn now_ms(&self) -> f64 {
        self.client.now()
    }
    fn closed(&self) -> bool {
        self.client.closed()
    }
    /// A browser is told nothing by the server's plain QUIC close
    /// (`close_why`: code 0, no reason). A tab in the background draws no
    /// frames, falls behind and is cut: the usual cause, and one the page
    /// can see (boot.js, `__oneshotAsleep`). Read once, as it ended.
    fn ended_why(&self) -> Option<String> {
        if let Some(w) = self.why.borrow().as_ref() {
            return Some(w.clone());
        }
        let (code, reason) = self.client.ended()?;
        let why = client_net::session::close_why(code, &reason);
        // Said nothing: `close_why`'s own words for a close with none.
        let said = why != client_net::session::close_why(0, "");
        let why = if !said && page_says("__oneshotAsleep") {
            ASLEEP.to_string()
        } else {
            why
        };
        *self.why.borrow_mut() = Some(why.clone());
        Some(why)
    }
    fn leave(&mut self) {
        self.client.close();
    }
}

/// `window[hook]()`, if the page installed it, is true.
#[cfg(target_arch = "wasm32")]
fn page_says(hook: &str) -> bool {
    use wasm_bindgen::{JsCast, JsValue};
    let global = js_sys::global();
    js_sys::Reflect::get(&global, &JsValue::from_str(hook))
        .ok()
        .and_then(|v| v.dyn_into::<js_sys::Function>().ok())
        .and_then(|f| f.call0(&global).ok())
        .is_some_and(|v| v.is_truthy())
}

/// No server: a stand-in session, ready on a map (Proving Grounds unless
/// told), that nothing ever reaches (the pose mode, `crate::pose`).
pub struct Still(pub Session);

impl Still {
    pub fn new() -> Still {
        Still::on(0)
    }

    pub fn on(map: u8) -> Still {
        let mut s = Session::new();
        s.ready = true;
        s.welcomes = 1;
        s.map_id = map;
        // The map itself too: what stands on its ground asks it where that
        // is (a posed truck on a terrain).
        if let Some(d) = sim::maps::ALL.iter().find(|d| d.id == map) {
            s.core.map = d.build();
        }
        Still(s)
    }

    /// Not even a map: nothing is built until a real link comes.
    pub fn idle() -> Still {
        Still(Session::new())
    }
}

impl Default for Still {
    fn default() -> Self {
        Still::new()
    }
}

impl Link for Still {
    fn update(&mut self, _: Input) {}
    fn session(&self) -> &Session {
        &self.0
    }
    fn session_mut(&mut self) -> &mut Session {
        &mut self.0
    }
    fn now_ms(&self) -> f64 {
        0.0
    }
    fn closed(&self) -> bool {
        false
    }
    fn live(&self) -> bool {
        false
    }
}

/// A tape played back (`film.rs`): a watching session fed by a
/// `client_net::tape::Replay` at the film's clock, on its own frame clock,
/// however long a frame takes to draw. What it draws is what the player
/// was sent, through client-net's interpolation; nothing goes anywhere.
pub struct Tape {
    pub session: Session,
    pub replay: client_net::tape::Replay,
}

impl Tape {
    pub fn new(tape: client_net::tape::Tape) -> Tape {
        Tape {
            session: Session::replay(),
            replay: client_net::tape::Replay::new(tape),
        }
    }
}

impl Link for Tape {
    fn update(&mut self, _: Input) {
        self.replay.frame(&mut self.session);
    }
    fn session(&self) -> &Session {
        &self.session
    }
    fn session_mut(&mut self) -> &mut Session {
        &mut self.session
    }
    fn now_ms(&self) -> f64 {
        self.replay.clock_ms
    }
    fn closed(&self) -> bool {
        false
    }
    fn live(&self) -> bool {
        false
    }
    fn tape(&mut self) -> Option<&mut Tape> {
        Some(self)
    }
}

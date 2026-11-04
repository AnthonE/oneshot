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
    /// We're leaving: close the connection (the server lets the seat go).
    fn leave(&mut self) {}
    /// Why the connection ended, in words, once it has (the close's code
    /// and reason); None if nothing was said.
    fn ended_why(&self) -> Option<String> {
        None
    }
    /// Not connected yet: a stand-in while the page warms the game up
    /// (`Pending`).
    fn pending(&self) -> bool {
        false
    }
    /// The real link, and what its stream said first, once it's here
    /// (`Pending`); the app swaps it in.
    fn take(&mut self) -> Option<(Box<dyn Link>, Vec<proto::Msg>)> {
        None
    }
}

/// Where the page's connect puts the client once it's seated, and what
/// its stream said before (the queue's lines, the welcome).
#[cfg(target_arch = "wasm32")]
#[derive(Default)]
pub struct Slot {
    pub client: Option<client_net::web::Client>,
    pub early: Vec<proto::Msg>,
}

/// A browser's link before it connects: a map and nobody, so the game
/// draws a level (warming every pipeline, `warm.rs`) behind the page's
/// menu while the page waits; then the real one, from the slot. The map is
/// the mode's kind: a Big Team map's terrain and grass are pipelines no
/// arena has.
#[cfg(target_arch = "wasm32")]
pub struct Pending {
    idle: Still,
    slot: std::rc::Rc<std::cell::RefCell<Slot>>,
}

#[cfg(target_arch = "wasm32")]
impl Pending {
    pub fn new(slot: std::rc::Rc<std::cell::RefCell<Slot>>, map: u8) -> Pending {
        Pending {
            idle: Still::on(map),
            slot,
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl Link for Pending {
    fn update(&mut self, _: Input) {}
    fn session(&self) -> &Session {
        self.idle.session()
    }
    fn session_mut(&mut self) -> &mut Session {
        self.idle.session_mut()
    }
    fn now_ms(&self) -> f64 {
        0.0
    }
    fn closed(&self) -> bool {
        false
    }
    fn pending(&self) -> bool {
        true
    }
    fn take(&mut self) -> Option<(Box<dyn Link>, Vec<proto::Msg>)> {
        let mut s = self.slot.borrow_mut();
        let client = s.client.take()?;
        Some((Box::new(Web(client)), core::mem::take(&mut s.early)))
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
    fn leave(&mut self) {
        self.client.close();
    }
    fn ended_why(&self) -> Option<String> {
        self.client.ended_why()
    }
}

#[cfg(target_arch = "wasm32")]
pub struct Web(pub client_net::web::Client);

#[cfg(target_arch = "wasm32")]
impl Link for Web {
    fn update(&mut self, input: Input) {
        self.0.tick(input);
    }
    fn session(&self) -> &Session {
        self.0.session()
    }
    fn session_mut(&mut self) -> &mut Session {
        self.0.session_mut()
    }
    fn now_ms(&self) -> f64 {
        self.0.now()
    }
    fn closed(&self) -> bool {
        self.0.closed()
    }
    fn leave(&mut self) {
        self.0.close();
    }
    fn ended_why(&self) -> Option<String> {
        self.0.ended_why()
    }
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
        Still(s)
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
}

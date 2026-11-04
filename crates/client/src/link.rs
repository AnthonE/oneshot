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

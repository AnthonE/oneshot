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
    /// The clock the session's interpolation runs on, in ms.
    fn now_ms(&self) -> f64;
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
    fn now_ms(&self) -> f64 {
        self.client.now_ms()
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
    fn now_ms(&self) -> f64 {
        self.0.now()
    }
}

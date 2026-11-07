//! The netcode on WebTransport, for the Bevy client in the browser.
//! Transport after Gates' `client/src/net/web.rs`: the certificate pinned by
//! hash, datagrams written without ever waiting (a full queue drops, a newer
//! one follows), the stream read through a buffer because a browser read
//! returns whatever arrived.
//!
//! The game calls [`Client::tick`] once a frame with the controls as they
//! are, then reads the [`Session`] to draw from.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use js_sys::Uint8Array;
use proto::frame::{FrameBuf, LEN_PREFIX_BYTES};
use proto::{decode_msg, frame_msg, Msg, MAX_STREAM_MSG};
use sim::input::Input;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::{spawn_local, JsFuture};
use web_sys::{
    ReadableStream, ReadableStreamDefaultReader, WebTransport, WebTransportDatagramDuplexStream,
    WritableStreamDefaultWriter,
};

use crate::session::{Session, Wire};

pub(crate) fn js_err(what: &str, v: &JsValue) -> JsValue {
    let msg = js_sys::Reflect::get(v, &JsValue::from_str("message"))
        .ok()
        .and_then(|m| m.as_string())
        .or_else(|| v.as_string())
        .unwrap_or_default();
    JsValue::from_str(&format!("{what}: {msg}"))
}

fn now_ms() -> f64 {
    // Read a dozen times a frame: the page's clock found once.
    thread_local! {
        static PERF: Option<web_sys::Performance> = web_sys::window().and_then(|w| w.performance());
    }
    PERF.with(|p| p.as_ref().map_or(0.0, |p| p.now()))
}

/// Lets a write's promise settle with nobody waiting on it (a rejection is
/// caught, not reported): one shared handler, where a task spawned to await
/// it was a box and two closures per datagram.
pub(crate) fn settle(p: &js_sys::Promise) {
    thread_local! {
        static IGNORE: Closure<dyn FnMut(JsValue)> = Closure::new(|_| ());
    }
    IGNORE.with(|f| {
        let _ = p.catch(f);
    });
}

/// The keys of a stream read's result, made once a reader rather than once
/// a read.
pub(crate) struct ReadKeys {
    done: JsValue,
    value: JsValue,
}

impl ReadKeys {
    pub(crate) fn new() -> ReadKeys {
        ReadKeys {
            done: JsValue::from_str("done"),
            value: JsValue::from_str("value"),
        }
    }

    /// The chunk a read gave, or None at the stream's end (or a read that
    /// wasn't one).
    pub(crate) fn chunk(&self, result: &JsValue) -> Option<Uint8Array> {
        let done = js_sys::Reflect::get(result, &self.done).ok()?.as_bool().unwrap_or(true);
        if done {
            return None;
        }
        js_sys::Reflect::get(result, &self.value)
            .ok()?
            .dyn_into::<Uint8Array>()
            .ok()
    }
}

pub(crate) fn hex32(s: &str) -> Option<[u8; 32]> {
    let s: String = s.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if s.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&s[2 * i..2 * i + 2], 16).ok()?;
    }
    Some(out)
}

/// Reads length-prefixed frames off a stream, whatever sizes reads return.
pub(crate) struct FrameReader {
    reader: ReadableStreamDefaultReader,
    buf: Box<FrameBuf<MAX_STREAM_MSG>>,
    pending: Vec<u8>,
    at: usize,
    keys: ReadKeys,
}

impl FrameReader {
    pub(crate) fn new(stream: &ReadableStream) -> Result<Self, JsValue> {
        Ok(FrameReader {
            reader: ReadableStreamDefaultReader::new(stream).map_err(|e| js_err("stream reader", &e))?,
            buf: Box::new(FrameBuf::new()),
            pending: Vec::new(),
            at: 0,
            keys: ReadKeys::new(),
        })
    }

    pub(crate) async fn next(&mut self) -> Option<Vec<u8>> {
        loop {
            while self.at < self.pending.len() {
                let (used, done) = self.buf.push(&self.pending[self.at..]).ok()?;
                self.at += used;
                if done {
                    let out = self.buf.frame().to_vec();
                    self.buf.take();
                    return Some(out);
                }
            }
            self.pending.clear();
            self.at = 0;
            let result = JsFuture::from(self.reader.read()).await.ok()?;
            let chunk = self.keys.chunk(&result)?;
            self.pending.resize(chunk.length() as usize, 0);
            chunk.copy_to(&mut self.pending);
        }
    }
}

pub(crate) async fn write_frame(w: &WritableStreamDefaultWriter, m: &Msg) -> Result<(), JsValue> {
    let mut out = [0u8; MAX_STREAM_MSG + LEN_PREFIX_BYTES];
    let n = frame_msg(m, &mut out).map_err(|_| JsValue::from_str("frame"))?;
    let chunk = Uint8Array::new_with_length(n as u32);
    chunk.copy_from(&out[..n]);
    JsFuture::from(w.write_with_chunk(&chunk))
        .await
        .map(|_| ())
        .map_err(|e| js_err("stream write", &e))
}

/// The datagram writer: Safari has only `createWritable()`.
pub(crate) fn datagram_writer(d: &WebTransportDatagramDuplexStream) -> Result<WritableStreamDefaultWriter, JsValue> {
    let create = js_sys::Reflect::get(d, &JsValue::from_str("createWritable"))
        .ok()
        .and_then(|f| f.dyn_into::<js_sys::Function>().ok());
    let stream = match create {
        Some(f) => f.call0(d)?,
        None => js_sys::Reflect::get(d, &JsValue::from_str("writable"))?,
    };
    stream
        .unchecked_into::<web_sys::WritableStream>()
        .get_writer()
        .map_err(|e| js_err("datagram writer", &e))
}

type Queue<T> = Rc<RefCell<VecDeque<T>>>;

/// Datagrams held for the game at most: a page in the background stops
/// drawing, and the oldest go.
const INBOX_MAX: usize = 256;

/// The datagram writer as a session's [`Wire`]: never waits; a datagram
/// past the path's size or behind a full queue is dropped.
struct WebWire<'a> {
    datagrams: &'a WebTransportDatagramDuplexStream,
    writer: &'a WritableStreamDefaultWriter,
}

impl Wire for WebWire<'_> {
    fn send_datagram(&mut self, payload: &[u8]) -> bool {
        let max = self.datagrams.max_datagram_size() as usize;
        if max != 0 && payload.len() > max {
            return false;
        }
        if self.writer.desired_size().ok().flatten().is_some_and(|d| d <= 0.0) {
            return false;
        }
        let chunk = Uint8Array::new_with_length(payload.len() as u32);
        chunk.copy_from(payload);
        settle(&self.writer.write_with_chunk(&chunk));
        true
    }
}

/// The browser's session, for the Rust app that holds it (the Bevy client:
/// `crates/client/src/door.rs` connects, `link.rs` plays).
pub struct Client {
    session: Session,
    transport: WebTransport,
    datagrams: WebTransportDatagramDuplexStream,
    writer: WritableStreamDefaultWriter,
    /// The stream's writer: what a watcher asks goes out on it.
    stream: Rc<WritableStreamDefaultWriter>,
    inbox: Queue<(f64, Vec<u8>)>,
    /// The inbox's buffers once read, for the reader to fill again: a
    /// datagram allocates nothing once a few are in hand.
    spare: Rc<RefCell<Vec<Vec<u8>>>>,
    msgs: Queue<Msg>,
    closed: Rc<RefCell<bool>>,
    /// Why the connection ended, once it has: the close's code and reason
    /// (`transport.closed`), or code 0 and no reason for a line that just
    /// went.
    ended: Rc<RefCell<Option<(u32, String)>>>,
}

impl Client {
    /// Connects to `addr` (host:port), pinning the certificate whose
    /// SHA-256 is `hash` (hex; empty for a certificate from a CA, which the
    /// browser checks against its own roots), and asks to play `mode`
    /// (`proto::MODE_*`) as `name` wearing `skin` (worn only if the pass's
    /// wallet owns it), `emblem` on its back (`proto::Emblem`), `headwear`
    /// and `footwear`, signed in with an Elo Pros `pass` (empty for a guest;
    /// or the server's ticket), with the `party` it comes with (its code, 0
    /// for none), for match `queued` (Elo Pros' queue's id, as a `Moved`
    /// named it; empty for none). Returns once the hello is sent: the
    /// welcome comes when a room is ready, and `ready` turns true. A
    /// refusal comes through the session's messages.
    #[allow(clippy::too_many_arguments)]
    pub async fn connect_queued(
        addr: String,
        hash: String,
        name: String,
        pass: String,
        mode: u8,
        skin: u8,
        emblem: u16,
        headwear: u8,
        footwear: u8,
        party: u32,
        queued: String,
    ) -> Result<Client, JsValue> {
        let pass = if pass.is_empty() {
            proto::Pass::NONE
        } else {
            proto::Pass::new(&pass).ok_or_else(|| JsValue::from_str("that pass is not one Elo Pros gives"))?
        };
        let opts = web_sys::WebTransportOptions::new();
        opts.set_require_unreliable(true);
        if let Some(digest) = hex32(&hash) {
            let bytes = Uint8Array::new_with_length(32);
            bytes.copy_from(&digest);
            let h = web_sys::WebTransportHash::new();
            h.set_algorithm("sha-256");
            h.set_value_u8_array(&bytes);
            opts.set_server_certificate_hashes(&[h]);
        }
        let url = format!("https://{addr}/");
        let transport = WebTransport::new_with_options(&url, &opts).map_err(|e| js_err("connect", &e))?;
        JsFuture::from(transport.ready())
            .await
            .map_err(|e| js_err("connect", &e))?;

        let bidi: web_sys::WebTransportBidirectionalStream = JsFuture::from(transport.create_bidirectional_stream())
            .await
            .map_err(|e| js_err("stream", &e))?
            .unchecked_into();
        let sw = Rc::new(bidi.writable().get_writer().map_err(|e| js_err("stream writer", &e))?);
        let look = proto::Look {
            skin,
            emblem,
            headwear,
            footwear,
        };
        let hello = match proto::QueueMatch::new(&queued) {
            Some(q) => Session::hello_queued(&name, pass, mode, look, q),
            None => Session::hello(&name, pass, mode, look, party),
        };
        write_frame(&sw, &hello).await?;
        let stream = sw.clone();
        let mut reader = FrameReader::new(&bidi.readable())?;

        let msgs: Queue<Msg> = Rc::default();
        let closed = Rc::new(RefCell::new(false));
        {
            let msgs = msgs.clone();
            let closed = closed.clone();
            spawn_local(async move {
                while let Some(f) = reader.next().await {
                    if let Ok(m) = decode_msg(&f) {
                        msgs.borrow_mut().push_back(m);
                    }
                }
                *closed.borrow_mut() = true;
                drop(sw);
            });
        }

        let ended: Rc<RefCell<Option<(u32, String)>>> = Rc::default();
        {
            let ended = ended.clone();
            let done = JsFuture::from(transport.closed());
            spawn_local(async move {
                let why = match done.await {
                    // A close with a code and a reason (the server's).
                    Ok(info) => {
                        let get = |k: &str| js_sys::Reflect::get(&info, &JsValue::from_str(k)).ok();
                        let code = get("closeCode").and_then(|v| v.as_f64()).unwrap_or(0.0) as u32;
                        let reason = get("reason").and_then(|v| v.as_string()).unwrap_or_default();
                        (code, reason)
                    }
                    // The line went.
                    Err(_) => (0, String::new()),
                };
                *ended.borrow_mut() = Some(why);
            });
        }

        let datagrams = transport.datagrams();
        let inbox: Queue<(f64, Vec<u8>)> = Rc::default();
        let spare: Rc<RefCell<Vec<Vec<u8>>>> = Rc::default();
        {
            let (inbox, spare) = (inbox.clone(), spare.clone());
            let readable = datagrams.readable();
            spawn_local(async move {
                let Ok(reader) = ReadableStreamDefaultReader::new(&readable) else {
                    return;
                };
                let keys = ReadKeys::new();
                loop {
                    let Ok(result) = JsFuture::from(reader.read()).await else {
                        break;
                    };
                    let Some(chunk) = keys.chunk(&result) else { break };
                    let mut q = inbox.borrow_mut();
                    // A page in the background stops drawing: keep the newest.
                    let mut buf = if q.len() >= INBOX_MAX {
                        q.pop_front().map(|(_, b)| b)
                    } else {
                        spare.borrow_mut().pop()
                    }
                    .unwrap_or_default();
                    buf.resize(chunk.length() as usize, 0);
                    chunk.copy_to(&mut buf);
                    q.push_back((now_ms(), buf));
                }
            });
        }
        let writer = datagram_writer(&datagrams)?;
        Ok(Client {
            session: Session::for_hello(&hello),
            transport,
            datagrams,
            writer,
            stream,
            inbox,
            spare,
            msgs,
            closed,
            ended,
        })
    }

    /// The close's code and reason, once the connection has ended: for an
    /// app that words it itself (`session::close_why` is the usual words).
    pub fn ended(&self) -> Option<(u32, String)> {
        self.ended.borrow().clone()
    }

    pub fn session(&self) -> &Session {
        &self.session
    }

    pub fn session_mut(&mut self) -> &mut Session {
        &mut self.session
    }

    /// The page's clock, ms: what `tick` runs the session on.
    pub fn now(&self) -> f64 {
        now_ms()
    }

    /// Whether the server closed the stream.
    pub fn closed(&self) -> bool {
        *self.closed.borrow()
    }

    /// Whether the welcome has come and play has begun.
    pub fn ready(&self) -> bool {
        self.session.ready
    }

    /// Takes in stream messages (the session's to read).
    fn absorb(&mut self) {
        loop {
            let Some(m) = self.msgs.borrow_mut().pop_front() else {
                break;
            };
            self.session.on_msg(m);
        }
    }

    /// Once a frame: takes in what arrived, advances the clock with the
    /// controls as they are, sends what is due.
    pub fn tick(&mut self, input: Input) {
        let now = now_ms();
        self.absorb();
        if !self.session.ready {
            let mut spare = self.spare.borrow_mut();
            spare.extend(self.inbox.borrow_mut().drain(..).map(|(_, b)| b));
            spare.truncate(INBOX_MAX);
            return;
        }
        loop {
            let Some((at, d)) = self.inbox.borrow_mut().pop_front() else {
                break;
            };
            self.session.on_datagram(at, &d);
            let mut spare = self.spare.borrow_mut();
            if spare.len() < INBOX_MAX {
                spare.push(d);
            }
        }
        let mut wire = WebWire {
            datagrams: &self.datagrams,
            writer: &self.writer,
        };
        self.session.update(now, input, &mut wire);
        for m in self.session.take_outgoing() {
            let sw = self.stream.clone();
            spawn_local(async move {
                let _ = write_frame(&sw, &m).await;
            });
        }
    }

    pub fn close(&self) {
        self.transport.close();
    }
}

impl Drop for Client {
    /// A session let go of (a link replaced) leaves its seat, as the
    /// native one does: never a still fighter until the tab closes.
    fn drop(&mut self) {
        self.transport.close();
    }
}

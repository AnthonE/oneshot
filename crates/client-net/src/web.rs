//! The netcode on WebTransport, handed to the page. Transport after Gates'
//! `client/src/net/web.rs`: the certificate pinned by hash, datagrams
//! written without ever waiting (a full queue drops, a newer one follows),
//! the stream read through a buffer because a browser read returns whatever
//! arrived.
//!
//! The page calls [`Client::update`] once a frame with the controls as they
//! are, then reads flat `Float32Array`s to draw from.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use js_sys::Uint8Array;
use proto::frame::{FrameBuf, LEN_PREFIX_BYTES};
use proto::{decode_msg, frame_msg, Msg, MAX_STREAM_MSG};
use sim::input::Input;
use sim::map::Map;
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

/// For a Rust app holding the browser's session (the Bevy client): the
/// session, and a frame's update with the clock read here.
impl Client {
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

    pub fn tick(&mut self, input: Input) {
        let i = input;
        self.update(now_ms(), i.buttons, i.yaw, i.pitch, i.mx, i.mz);
    }

    pub fn now(&self) -> f64 {
        now_ms()
    }
}

#[wasm_bindgen]
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

#[wasm_bindgen]
impl Client {
    /// Connects to `addr` (host:port), pinning the certificate whose
    /// SHA-256 is `hash` (hex; empty for a certificate from a CA, which the
    /// browser checks against its own roots), and asks to play `mode` (0 casual against
    /// a bot, 1 ranked Duel, 2 ranked Doubles, 3 free for all, 4 Gun Game,
    /// 5 a cup)
    /// as `name` wearing `skin` (worn only if the pass's wallet owns it) and
    /// `emblem` on its back (`proto::Emblem`),
    /// signed in with an
    /// Elo Pros `pass` (empty for a guest; or the server's ticket), with the
    /// `party` it comes with (its code, 0 for none). Returns once the hello is sent:
    /// the welcome comes when a room is ready (at once for casual, when the
    /// queue forms a match for ranked), and `ready` turns true. A refusal
    /// comes through `messages()`.
    #[allow(clippy::too_many_arguments)]
    pub async fn connect(
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
        let hello = Session::hello(
            &name,
            pass,
            mode,
            proto::Look {
                skin,
                emblem,
                headwear,
                footwear,
            },
            party,
        );
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

    #[wasm_bindgen(getter)]
    pub fn slot(&self) -> u8 {
        self.session.core.slot
    }

    #[wasm_bindgen(getter)]
    pub fn team(&self) -> u8 {
        self.session.core.team
    }

    /// How many welcomes have come: a cup's entrant is welcomed into each
    /// heat on the same connection, and the page redraws the level then.
    #[wasm_bindgen(getter)]
    pub fn welcomes(&self) -> u32 {
        self.session.welcomes
    }

    /// The map this match is on (`map_boxes(map)` and friends draw it).
    #[wasm_bindgen(getter)]
    pub fn map(&self) -> u8 {
        self.session.map_id
    }

    /// Whether this match is Gun Game: the score is the rung.
    #[wasm_bindgen(getter)]
    pub fn gun_game(&self) -> bool {
        self.session.core.rules.gun_game
    }

    /// Whether this match is red against blue.
    #[wasm_bindgen(getter)]
    pub fn teams(&self) -> bool {
        self.session.core.rules.teams
    }

    /// The gun on Gun Game's rung `score`.
    pub fn rung(&self, score: i32) -> u8 {
        sim::world::Rules::rung(score)
    }

    #[wasm_bindgen(getter)]
    pub fn closed(&self) -> bool {
        *self.closed.borrow()
    }

    /// Whether the welcome has come and play has begun.
    #[wasm_bindgen(getter)]
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
    pub fn update(&mut self, now: f64, buttons: u16, yaw: u16, pitch: i16, mx: i8, mz: i8) {
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
        let input = Input {
            buttons,
            yaw,
            pitch,
            mx,
            mz,
        };
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

    /// Our own fighter, drawn: x y z, eye height, yaw, pitch (radians),
    /// alive, crouched, shield, health (points), arm, mag, reserve, other
    /// arm, grenades, reloading (0..1), team, kills, deaths, respawn (s),
    /// grounded, speed, then the seat it rides in (−1 on foot, 0 driver, 1
    /// passenger) and that vehicle's slot.
    pub fn own(&self) -> Vec<f32> {
        let (f, p) = self.session.core.own();
        let g = f.gun();
        let a = sim::arms::arm(g.arm);
        let reload = if f.reload > 0 {
            1.0 - f.reload as f32 / a.reload_ticks().max(1) as f32
        } else {
            0.0
        };
        let eye = f.body.eye_h();
        let rad = core::f32::consts::TAU / 65536.0;
        vec![
            p.x,
            p.y,
            p.z,
            eye,
            f.yaw as f32 * rad,
            f.pitch as f32 * rad,
            f.alive as u8 as f32,
            f.body.crouched as u8 as f32,
            f.shield as f32 / sim::consts::PT as f32,
            f.health as f32 / sim::consts::PT as f32,
            g.arm as f32,
            g.mag as f32,
            g.reserve as f32,
            f.guns[(f.cur as usize & 1) ^ 1].arm as f32,
            f.grenades as f32,
            reload,
            f.team as f32,
            f.kills as f32,
            f.deaths as f32,
            f.respawn as f32 / sim::consts::TICK_HZ as f32,
            f.body.grounded as u8 as f32,
            (f.body.vel.x * f.body.vel.x + f.body.vel.z * f.body.vel.z).sqrt(),
            f.riding().map_or(-1.0, |(_, seat)| seat as f32),
            f.riding().map_or(-1.0, |(i, _)| i as f32),
        ]
    }

    /// The vehicles to draw, 8 numbers each: slot, x y z (the hull's middle
    /// at the ground), yaw (radians), health (0..1), and each seat's rider
    /// (a slot, or −1). The one we ride in is our prediction of it.
    pub fn vehicles(&self, now: f64) -> Vec<f32> {
        let mut v = Vec::new();
        let seat = |x: u8| if x == 0 { -1.0 } else { x as f32 - 1.0 };
        if let Some((car, p, yaw)) = self.session.core.own_vehicle() {
            let i = self.session.core.own().0.riding().map_or(0, |(i, _)| i);
            v.extend_from_slice(&[
                i as f32,
                p.x,
                p.y,
                p.z,
                yaw,
                car.health.max(0) as f32 / sim::consts::VEH_HEALTH as f32,
                seat(car.seats[0]),
                seat(car.seats[1]),
            ]);
        }
        for d in self.session.core.vehicles(now) {
            v.extend_from_slice(&[
                d.slot as f32,
                d.pos.x,
                d.pos.y,
                d.pos.z,
                d.yaw,
                d.health as f32 / 255.0,
                seat(d.seats[0]),
                seat(d.seats[1]),
            ]);
        }
        v
    }

    /// Everyone else, 14 numbers each: slot, team, alive, crouched,
    /// grounded, reloading, x, y, z, yaw, pitch, arm, shield (0..1), speed.
    pub fn others(&self, now: f64) -> Vec<f32> {
        let mut v = Vec::new();
        for d in self.session.core.others(now) {
            push_drawn(&mut v, &d);
        }
        v
    }

    /// While we are dead and saw who killed us: the kill cam, 7 numbers
    /// then the bodies as `others()` gives them (ourselves among them, the
    /// killer not): eye x y z, yaw, pitch (radians), the killer's seat, how
    /// far through the run-up (0..1). Empty otherwise. Made only of
    /// snapshots we were already sent.
    pub fn killcam(&self, now: f64) -> Vec<f32> {
        let Some(k) = self.session.core.killcam(now) else {
            return Vec::new();
        };
        let mut v = vec![k.eye.x, k.eye.y, k.eye.z, k.yaw, k.pitch, k.killer as f32, k.progress];
        for d in &k.bodies {
            push_drawn(&mut v, d);
        }
        v
    }

    /// Rounds in flight as drawn at `now`, 8 each: kind
    /// (`proto::ProjState::kind`), owner, x y z, velocity x z, and an id
    /// that holds while it flies.
    pub fn projectiles(&self, now: f64) -> Vec<f32> {
        let mut v = Vec::new();
        for p in self.session.core.projectiles(now) {
            v.extend_from_slice(&[
                p.kind as f32,
                p.owner as f32,
                p.pos.x,
                p.pos.y,
                p.pos.z,
                p.vel.x,
                p.vel.z,
                p.id as f32,
            ]);
        }
        v
    }

    /// What is due to play at `now` (`NetCore::drain_events`), 11 each:
    /// kind, a, b, part, c, pos x y z, end x y z (sim::events::Kind
    /// numbers the kinds).
    pub fn events(&mut self, now: f64) -> Vec<f32> {
        let mut v = Vec::new();
        for e in self.session.core.drain_events(now) {
            v.extend_from_slice(&[
                e.kind as f32,
                e.a as f32,
                e.b as f32,
                e.part as f32,
                e.c as f32,
                e.pos.x,
                e.pos.y,
                e.pos.z,
                e.end.x,
                e.end.y,
                e.end.z,
            ]);
        }
        v
    }

    /// The match: phase (0 countdown, 1 playing, 2 over), seconds left,
    /// winner, the score limit, the number of scores `n`, then the `n`
    /// scores (each side's with teams, else each seat's), then each pad's
    /// seconds until its gun is back.
    pub fn match_state(&self) -> Vec<f32> {
        let s = self.session.core.latest();
        let mut v = vec![
            s.phase as f32,
            s.clock as f32 / sim::consts::TICK_HZ as f32,
            s.winner as f32,
            self.session.core.rules.score_limit as f32,
        ];
        v.push(s.score.len() as f32);
        v.extend(s.score.iter().map(|&x| x as f32));
        v.extend(s.pads().iter().map(|p| p.empty_s as f32));
        v
    }

    /// Stream messages since the last call, as JS objects.
    pub fn messages(&mut self) -> js_sys::Array {
        let out = js_sys::Array::new();
        self.absorb();
        let msgs: Vec<Msg> = self.session.take_messages();
        for m in msgs {
            let o = js_sys::Object::new();
            let set = |k: &str, v: JsValue| {
                let _ = js_sys::Reflect::set(&o, &JsValue::from_str(k), &v);
            };
            match m {
                Msg::Seat {
                    slot,
                    team,
                    bot,
                    name,
                    skin,
                    emblem,
                    headwear,
                    footwear,
                } => {
                    set("type", "seat".into());
                    set("skin", skin.into());
                    set("emblem", emblem.into());
                    set("headwear", headwear.into());
                    set("footwear", footwear.into());
                    set("slot", slot.into());
                    set("team", team.into());
                    set("bot", bot.into());
                    set("name", name.as_str().into());
                }
                Msg::Kill {
                    killer,
                    victim,
                    cause,
                    head,
                } => {
                    set("type", "kill".into());
                    set("killer", killer.into());
                    set("victim", victim.into());
                    set("cause", cause.into());
                    set("head", head.into());
                }
                Msg::Phase { phase, winner } => {
                    set("type", "phase".into());
                    set("phase", phase.into());
                    set("winner", winner.into());
                }
                Msg::Reveal { .. } => {
                    set("type", "reveal".into());
                    set("verified", (self.session.core.verified == Some(true)).into());
                }
                Msg::Queued { mode, waiting_s } => {
                    set("type", "queued".into());
                    set("mode", mode.into());
                    set("waiting", waiting_s.into());
                }
                Msg::Refuse { reason } => {
                    set("type", "refused".into());
                    set("reason", reason.into());
                    set("why", crate::session::refuse_why(reason).into());
                }
                Msg::Cup {
                    round,
                    rounds,
                    entrants,
                    place,
                    points,
                    done,
                } => {
                    set("type", "cup".into());
                    set("round", round.into());
                    set("rounds", rounds.into());
                    set("entrants", entrants.into());
                    set("place", place.into());
                    set("points", points.into());
                    set("done", done.into());
                }
                Msg::Bye => set("type", "bye".into()),
                Msg::Medal { slot, medal, victim } => {
                    set("type", "medal".into());
                    set("slot", slot.into());
                    set("medal", medal.into());
                    set("name", proto::medal::info(medal).name.into());
                    set("victim", victim.into());
                }
                Msg::Carnage {
                    slot,
                    kills,
                    deaths,
                    assists,
                    shots,
                    hits,
                    damage,
                    best_spree,
                    ..
                } => {
                    set("type", "carnage".into());
                    set("slot", slot.into());
                    set("kills", kills.into());
                    set("deaths", deaths.into());
                    set("assists", assists.into());
                    set("shots", shots.into());
                    set("hits", hits.into());
                    set("damage", damage.into());
                    set("best_spree", best_spree.into());
                }
                _ => continue,
            }
            out.push(&o);
        }
        out
    }

    /// Netcode health: snapshots, confirmations, corrections, resyncs,
    /// depth, clock rate, interpolation delay (ticks), datagrams dropped,
    /// and whether the match's revealed salt matched its commitment (1 yes,
    /// 0 no, −1 not revealed yet).
    pub fn stats(&self) -> Vec<f64> {
        let s = self.session.core.stats;
        vec![
            s.snapshots as f64,
            s.confirmations as f64,
            s.corrections as f64,
            s.resyncs as f64,
            s.depth as f64,
            s.rate,
            s.delay_ticks,
            self.session.dropped as f64,
            self.session.core.verified.map_or(-1.0, |v| v as u8 as f64),
        ]
    }

    pub fn close(&self) {
        self.transport.close();
    }

    /// Why the connection ended, in words for the player; None while it
    /// hasn't, or when it ended with nothing said.
    #[wasm_bindgen(getter)]
    pub fn ended_why(&self) -> Option<String> {
        let (code, reason) = self.ended.borrow().clone()?;
        Some(crate::session::close_why(code, &reason))
    }
}

fn push_drawn(v: &mut Vec<f32>, d: &crate::core::Drawn) {
    v.extend_from_slice(&[
        d.slot as f32,
        d.team as f32,
        d.alive as u8 as f32,
        d.crouched as u8 as f32,
        d.grounded as u8 as f32,
        d.reloading as u8 as f32,
        d.pos.x,
        d.pos.y,
        d.pos.z,
        d.yaw,
        d.pitch,
        d.arm as f32,
        d.shield as f32 / 255.0,
        (d.vel.x * d.vel.x + d.vel.z * d.vel.z).sqrt(),
    ]);
}

fn map_or_first(id: u8) -> Map {
    sim::map::by_id(id).unwrap_or_else(Map::proving_grounds)
}

/// Map `id`'s boxes, 7 numbers each (min x y z, max x y z, look): the one
/// source of the level's geometry for the renderer.
#[wasm_bindgen]
pub fn map_boxes(id: u8) -> Vec<f32> {
    let m = map_or_first(id);
    let mut v = Vec::new();
    for (b, &look) in m.boxes.iter().zip(&m.looks) {
        v.extend_from_slice(&[b.min.x, b.min.y, b.min.z, b.max.x, b.max.y, b.max.z, look as f32]);
    }
    v
}

/// Map `id`'s pads, 5 each: x y z, arm, respawn seconds.
#[wasm_bindgen]
pub fn map_pads(id: u8) -> Vec<f32> {
    let m = map_or_first(id);
    let mut v = Vec::new();
    for p in &m.pads {
        v.extend_from_slice(&[p.pos.x, p.pos.y, p.pos.z, p.arm as f32, p.respawn_ms as f32 / 1000.0]);
    }
    v
}

/// Map `id`'s ground, if it isn't flat: cell size, corner x, corner z,
/// vertices along x, along z, then the heights in metres, row by row (z
/// outer). Empty for a flat floor. Each cell is two triangles split from
/// its low corner to its high one, as the sim's are (`sim::terrain`): the
/// one where the x offset is at least the z offset, then the other.
#[wasm_bindgen]
pub fn map_terrain(id: u8) -> Vec<f32> {
    let Some(d) = sim::maps::ALL.iter().find(|d| d.id == id).or(sim::maps::ALL.first()) else {
        return Vec::new();
    };
    let Some(t) = &d.terrain else {
        return Vec::new();
    };
    let mut v = vec![t.cell, t.x0, t.z0, t.nx as f32, t.nz as f32];
    v.extend(t.heights.iter().map(|&q| q as f32 * (1.0 / sim::terrain::HEIGHT_Q)));
    v
}

/// Map `id`'s launchers, 9 each: the pad's min x y z, max x y z, then the
/// velocity it throws at.
#[wasm_bindgen]
pub fn map_launchers(id: u8) -> Vec<f32> {
    let m = map_or_first(id);
    let mut v = Vec::new();
    for l in &m.launchers {
        let (a, b) = (l.area.min, l.area.max);
        v.extend_from_slice(&[a.x, a.y, a.z, b.x, b.y, b.z, l.vel.x, l.vel.y, l.vel.z]);
    }
    v
}

/// Map `id`'s bounds (min x, min z, max x, max z), then its roam points
/// (x, z each).
#[wasm_bindgen]
pub fn map_layout(id: u8) -> Vec<f32> {
    let m = map_or_first(id);
    let mut v = vec![m.lo.0, m.lo.1, m.hi.0, m.hi.1];
    for r in &m.roam {
        v.extend_from_slice(&[r.x, r.z]);
    }
    v
}

/// Map `id`'s name.
#[wasm_bindgen]
pub fn map_name(id: u8) -> String {
    map_or_first(id).name.to_string()
}

/// Skin names by id (`proto::SKINS`), for the menu.
#[wasm_bindgen]
pub fn skin_names() -> Vec<String> {
    proto::SKINS.iter().map(|s| s.to_string()).collect()
}

/// Each skin's rank lock by id (`proto::SKIN_FLOOR`; 0 is none), for the menu.
#[wasm_bindgen]
pub fn skin_floors() -> Vec<u16> {
    proto::SKIN_FLOOR.to_vec()
}

/// Arm names by id, for the HUD.
#[wasm_bindgen]
pub fn arm_name(id: u8) -> String {
    sim::arms::arm(id).name.to_string()
}

#[wasm_bindgen(start)]
pub fn start() {
    std::panic::set_hook(Box::new(|info| {
        let line = format!("oneshot: panic: {info}");
        web_sys::console::error_1(&JsValue::from_str(&line));
        // The page shows that the game stopped (it can't go on: a panic in
        // wasm is a trap), rather than leave a frozen canvas: told the
        // panic's own words (what it says to the player is read from them,
        // never from the file it was in) and the whole line (the report).
        let said = info.payload_as_str().unwrap_or("");
        if let Some(w) = web_sys::window() {
            if let Ok(f) = js_sys::Reflect::get(&w, &JsValue::from_str("oneshotStopped")) {
                if let Some(f) = f.dyn_ref::<js_sys::Function>() {
                    let _ = f.call2(&JsValue::NULL, &JsValue::from_str(said), &JsValue::from_str(&line));
                }
            }
        }
    }));
}

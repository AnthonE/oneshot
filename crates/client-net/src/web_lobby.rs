//! The menu's session in the browser: the party, its friends and its
//! voice (proto's `party` and `voice`), handed to the page as a
//! [`Lobby`]. The page keeps it open beside any match it plays.
//!
//! What arrives goes to the page's callbacks as it comes: each stream
//! message as a plain object to `on_msg`, each voice frame as
//! `(from, seq, flags, bytes)` to `on_voice`. What the page says goes out
//! at once, in order; a voice frame behind a full datagram queue is
//! dropped, never waited on.

use js_sys::{Array, Function, Object, Reflect, Uint8Array};
use proto::party::{self as pp, PlayerId};
use proto::voice::{self, VoiceFrame};
use proto::{decode_msg, Msg, Pass};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::{spawn_local, JsFuture};
use web_sys::{ReadableStreamDefaultReader, WebTransport, WritableStreamDefaultWriter};

use crate::session::{refuse_why, Session};
use crate::web::{datagram_writer, hex32, js_err, write_frame, FrameReader};

fn set(o: &Object, k: &str, v: impl Into<JsValue>) {
    let _ = Reflect::set(o, &JsValue::from_str(k), &v.into());
}

/// A stream message as the page reads it: `{kind, ...}`.
fn to_js(m: &Msg) -> Option<Object> {
    let o = Object::new();
    match m {
        Msg::Ticket { ticket } => {
            set(&o, "kind", "ticket");
            set(&o, "ticket", ticket.as_str());
        }
        Msg::Party(p) => {
            set(&o, "kind", "party");
            set(&o, "code", p.code);
            set(&o, "text", pp::code_text(p.code));
            set(&o, "privacy", p.privacy);
            set(&o, "leader", p.leader);
            set(&o, "me", p.me);
            let members = Array::new();
            for m in p.members() {
                let r = Object::new();
                set(&r, "tag", m.tag);
                set(&r, "name", m.name.as_str());
                set(&r, "player", m.player.as_str());
                set(&r, "wallet", m.wallet());
                set(&r, "away", m.away);
                set(&r, "playing", m.playing);
                members.push(&r);
            }
            set(&o, "members", members);
        }
        Msg::Go { mode, private } => {
            set(&o, "kind", "go");
            set(&o, "mode", *mode);
            set(&o, "private", *private);
        }
        Msg::Friends { n, reachable } => {
            set(&o, "kind", "friends");
            set(&o, "n", *n);
            set(&o, "reachable", *reachable);
        }
        Msg::Friend {
            i,
            name,
            player,
            at,
            party,
            size,
            playing,
        } => {
            set(&o, "kind", "friend");
            set(&o, "i", *i);
            set(&o, "name", name.as_str());
            set(&o, "player", player.as_str());
            set(&o, "at", *at);
            set(&o, "party", *party);
            set(&o, "size", *size);
            set(&o, "playing", *playing);
        }
        Msg::Invited { from, code } => {
            set(&o, "kind", "invited");
            set(&o, "from", from.as_str());
            set(&o, "code", *code);
            set(&o, "text", pp::code_text(*code));
        }
        Msg::Refuse { reason } => {
            set(&o, "kind", "refuse");
            set(&o, "reason", *reason);
            set(&o, "why", refuse_why(*reason));
        }
        _ => return None,
    }
    Some(o)
}

#[wasm_bindgen]
pub struct Lobby {
    transport: WebTransport,
    stream: WritableStreamDefaultWriter,
    datagrams: WritableStreamDefaultWriter,
    max_datagram: web_sys::WebTransportDatagramDuplexStream,
}

#[wasm_bindgen]
impl Lobby {
    /// Opens the menu's session to `addr` (pinning `hash`, or none for a
    /// CA's certificate) as `name`, with an Elo Pros `pass`, the server's
    /// ticket, or nothing (a guest), asking for party `party` (its code, 0
    /// for none). `on_msg(obj)` gets each stream message, then
    /// `{kind: "closed"}` once; `on_voice(from, seq, flags, bytes)` each
    /// voice frame.
    pub async fn connect(
        addr: String,
        hash: String,
        name: String,
        pass: String,
        party: u32,
        on_msg: Function,
        on_voice: Function,
    ) -> Result<Lobby, JsValue> {
        let pass = if pass.is_empty() {
            Pass::NONE
        } else {
            Pass::new(&pass).ok_or_else(|| JsValue::from_str("that pass is not one Elo Pros gives"))?
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
        let transport =
            WebTransport::new_with_options(&format!("https://{addr}/"), &opts).map_err(|e| js_err("connect", &e))?;
        JsFuture::from(transport.ready())
            .await
            .map_err(|e| js_err("connect", &e))?;
        let bidi: web_sys::WebTransportBidirectionalStream = JsFuture::from(transport.create_bidirectional_stream())
            .await
            .map_err(|e| js_err("stream", &e))?
            .unchecked_into();
        let stream = bidi.writable().get_writer().map_err(|e| js_err("stream writer", &e))?;
        let hello = Session::hello(&name, pass, proto::MODE_LOBBY, proto::Look::default(), party);
        write_frame(&stream, &hello).await?;

        let mut reader = FrameReader::new(&bidi.readable())?;
        let cb = on_msg.clone();
        spawn_local(async move {
            while let Some(f) = reader.next().await {
                if let Some(o) = decode_msg(&f).ok().as_ref().and_then(to_js) {
                    let _ = cb.call1(&JsValue::NULL, &o);
                }
            }
            let o = Object::new();
            set(&o, "kind", "closed");
            let _ = cb.call1(&JsValue::NULL, &o);
        });

        let dgs = transport.datagrams();
        let readable = dgs.readable();
        spawn_local(async move {
            let Ok(reader) = ReadableStreamDefaultReader::new(&readable) else {
                return;
            };
            loop {
                let Ok(result) = JsFuture::from(reader.read()).await else {
                    break;
                };
                if Reflect::get(&result, &JsValue::from_str("done"))
                    .ok()
                    .and_then(|d| d.as_bool())
                    .unwrap_or(true)
                {
                    break;
                }
                let Ok(chunk) =
                    Reflect::get(&result, &JsValue::from_str("value")).and_then(|v| v.dyn_into::<Uint8Array>())
                else {
                    break;
                };
                let d = chunk.to_vec();
                if proto::datagram_kind(&d) != Some(proto::DG_VOICE) {
                    continue;
                }
                let Ok(f) = voice::decode_voice(&d) else { continue };
                if !voice::is_one_frame(f.payload()) {
                    continue;
                }
                let bytes = Uint8Array::from(f.payload());
                let args = Array::of4(&f.from.into(), &f.seq.into(), &f.flags.into(), &bytes);
                let _ = on_voice.apply(&JsValue::NULL, &args);
            }
        });
        let datagrams = datagram_writer(&dgs)?;
        Ok(Lobby {
            transport,
            stream,
            datagrams,
            max_datagram: dgs,
        })
    }

    fn say(&self, m: Msg) {
        let w = self.stream.clone();
        spawn_local(async move {
            let _ = write_frame(&w, &m).await;
        });
    }

    /// A party act (`ACT_*` in proto's `party`).
    pub fn act(&self, act: u8, arg: u32) {
        self.say(Msg::PartyAct { act, arg });
    }

    /// Asks a friend (their Elo Pros id) into the party.
    pub fn invite(&self, player: String) {
        if let Some(p) = PlayerId::new(&player.to_ascii_lowercase()) {
            self.say(Msg::PartyInvite { player: p });
        }
    }

    /// Signs this session in with a fresh Elo Pros pass.
    pub fn sign_in(&self, pass: String) {
        if let Some(p) = Pass::new(&pass) {
            self.say(Msg::SignIn { pass: p });
        }
    }

    /// One Opus frame of ours to the party. False if it was dropped (too
    /// big, not one 20 ms frame, or the path is full).
    pub fn talk(&self, seq: u16, end: bool, payload: &[u8]) -> bool {
        let Some(f) = VoiceFrame::new(seq, if end { voice::VOICE_END } else { 0 }, payload) else {
            return false;
        };
        if !voice::is_one_frame(payload) {
            return false;
        }
        let mut buf = [0u8; 8 + voice::VOICE_MAX_BYTES];
        let Ok(n) = voice::encode_voice_up(&f, &mut buf) else {
            return false;
        };
        let max = self.max_datagram.max_datagram_size() as usize;
        if (max != 0 && n > max) || self.datagrams.desired_size().ok().flatten().is_some_and(|d| d <= 0.0) {
            return false;
        }
        let chunk = Uint8Array::from(&buf[..n]);
        let p = self.datagrams.write_with_chunk(&chunk);
        spawn_local(async move {
            let _ = JsFuture::from(p).await;
        });
        true
    }

    pub fn close(&self) {
        self.transport.close();
    }
}

/// A party code as people read it, and back (0: not a code).
#[wasm_bindgen]
pub fn party_text(code: u32) -> String {
    pp::code_text(code)
}

#[wasm_bindgen]
pub fn party_code(text: String) -> u32 {
    pp::code_parse(&text).unwrap_or(0)
}

/// A mode as a player reads it.
#[wasm_bindgen]
pub fn mode_name(mode: u8) -> String {
    proto::mode_name(mode).to_string()
}

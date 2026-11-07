//! The menu's session in the browser: the party, its friends and its
//! voice (proto's `party` and `voice`), for the game to hold beside any
//! match (`crates/client/src/party.rs`).
//!
//! What arrives is queued as it comes (the stream's messages, the voice
//! frames) and taken once a frame; what the game says goes out at once, in
//! order. A voice frame behind a full datagram queue is dropped, never
//! waited on.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;

use js_sys::Uint8Array;
use proto::voice::{self, VoiceFrame};
use proto::{decode_msg, Msg};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::{spawn_local, JsFuture};
use web_sys::{ReadableStreamDefaultReader, WebTransport, WritableStreamDefaultWriter};

use crate::web::{datagram_writer, hex32, js_err, write_frame, FrameReader};

/// Messages and voice frames kept for the game before it takes them: a
/// page in the background stops drawing, and the newest matter most.
const KEEP: usize = 256;

pub struct WebLobby {
    transport: WebTransport,
    stream: WritableStreamDefaultWriter,
    datagrams: WritableStreamDefaultWriter,
    duplex: web_sys::WebTransportDatagramDuplexStream,
    msgs: Rc<RefCell<VecDeque<Msg>>>,
    voices: Rc<RefCell<VecDeque<VoiceFrame>>>,
    closed: Rc<Cell<bool>>,
}

fn keep<T>(q: &RefCell<VecDeque<T>>, t: T) {
    let mut q = q.borrow_mut();
    if q.len() >= KEEP {
        q.pop_front();
    }
    q.push_back(t);
}

impl WebLobby {
    /// Opens the menu's session to `addr` (pinning `hash`, or none for a
    /// CA's certificate) and says `hello` (`MODE_LOBBY`).
    pub async fn open(addr: String, hash: String, hello: Msg) -> Result<WebLobby, JsValue> {
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
        write_frame(&stream, &hello).await?;

        let msgs: Rc<RefCell<VecDeque<Msg>>> = Rc::default();
        let voices: Rc<RefCell<VecDeque<VoiceFrame>>> = Rc::default();
        let closed = Rc::new(Cell::new(false));
        let mut reader = FrameReader::new(&bidi.readable())?;
        {
            let (msgs, closed) = (msgs.clone(), closed.clone());
            spawn_local(async move {
                while let Some(f) = reader.next().await {
                    if let Ok(m) = decode_msg(&f) {
                        keep(&msgs, m);
                    }
                }
                closed.set(true);
            });
        }
        let duplex = transport.datagrams();
        {
            let (voices, readable) = (voices.clone(), duplex.readable());
            spawn_local(async move {
                let Ok(reader) = ReadableStreamDefaultReader::new(&readable) else {
                    return;
                };
                let keys = crate::web::ReadKeys::new();
                // One buffer for every frame heard: decoding copies out of it.
                let mut d = Vec::new();
                loop {
                    let Ok(result) = JsFuture::from(reader.read()).await else {
                        break;
                    };
                    let Some(chunk) = keys.chunk(&result) else { break };
                    d.resize(chunk.length() as usize, 0);
                    chunk.copy_to(&mut d);
                    if proto::datagram_kind(&d) != Some(proto::DG_VOICE) {
                        continue;
                    }
                    if let Ok(f) = voice::decode_voice(&d) {
                        if voice::is_one_frame(f.payload()) {
                            keep(&voices, f);
                        }
                    }
                }
            });
        }
        let datagrams = datagram_writer(&duplex)?;
        Ok(WebLobby {
            transport,
            stream,
            datagrams,
            duplex,
            msgs,
            voices,
            closed,
        })
    }

    /// Stream messages since the last call.
    pub fn take(&self) -> Vec<Msg> {
        self.msgs.borrow_mut().drain(..).collect()
    }

    /// Voice frames since the last call.
    pub fn take_voice(&self) -> Vec<VoiceFrame> {
        self.voices.borrow_mut().drain(..).collect()
    }

    /// A message to the server, after every one before it.
    pub fn send(&self, m: Msg) {
        let w = self.stream.clone();
        spawn_local(async move {
            let _ = write_frame(&w, &m).await;
        });
    }

    /// One Opus frame of ours to the party. False if it was dropped (too
    /// big, not one 20 ms frame, or the path is full).
    pub fn talk(&self, f: &VoiceFrame) -> bool {
        if !voice::is_one_frame(f.payload()) {
            return false;
        }
        let mut buf = [0u8; 8 + voice::VOICE_MAX_BYTES];
        let Ok(n) = voice::encode_voice_up(f, &mut buf) else {
            return false;
        };
        let max = self.duplex.max_datagram_size() as usize;
        if (max != 0 && n > max) || self.datagrams.desired_size().ok().flatten().is_some_and(|d| d <= 0.0) {
            return false;
        }
        crate::web::settle(&self.datagrams.write_with_chunk(&Uint8Array::from(&buf[..n])));
        true
    }

    pub fn closed(&self) -> bool {
        self.closed.get()
    }

    pub fn close(&self) {
        self.transport.close();
    }
}

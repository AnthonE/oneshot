//! The menu's sessions: a page's party, its friends and its voice, beside
//! whatever match it plays (`party.rs` holds the rules; this is their
//! wiring to the network).
//!
//! A lobby session is a WebTransport session whose hello asked for
//! `MODE_LOBBY`. Its stream carries party acts up and the party, friends
//! and invites down; its datagrams carry voice, relayed straight from the
//! talker's reader to the rest of the party with `send_datagram` (which
//! drops the oldest when a path is full, never waits). None of it touches
//! the sim thread.

use std::collections::BTreeMap;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use proto::party as pp;
use proto::voice::{self, Bucket};
use proto::{Msg, Name, Pass};
use wtransport::{Connection, RecvStream, SendStream};

use crate::auth::Who;
use crate::outlet::{self, Ended, Outlet};
use crate::party::{Outbox, Parties};
use crate::Ctx;

/// How often a session's friends are shown again (their presence moves),
/// and the sweep for those away too long.
const FRIENDS_EVERY: Duration = Duration::from_secs(15);
const SWEEP_EVERY: Duration = Duration::from_secs(5);

#[derive(Default)]
pub struct Lobby {
    pub parties: Mutex<Parties>,
    /// Each lobby session's stream lane and connection (for its voice).
    outs: Mutex<BTreeMap<u64, (Outlet, Connection)>>,
}

impl Lobby {
    pub fn parties(&self) -> MutexGuard<'_, Parties> {
        self.parties.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn outs(&self) -> MutexGuard<'_, BTreeMap<u64, (Outlet, Connection)>> {
        self.outs.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Hands each message to its session.
    pub fn send(&self, out: Outbox) {
        if out.is_empty() {
            return;
        }
        let outs = self.outs();
        for (s, m) in out {
            if let Some((tx, _)) = outs.get(&s) {
                crate::send_msg(tx, &m);
            }
        }
    }

    /// A talker's frame to the rest of their party.
    fn relay(&self, session: u64, f: &voice::VoiceFrame) {
        let Some((tag, to)) = self.parties().voice(session) else {
            return;
        };
        let mut down = *f;
        down.from = tag;
        let mut buf = [0u8; 8 + voice::VOICE_MAX_BYTES];
        let Ok(n) = voice::encode_voice(&down, &mut buf) else {
            return;
        };
        // Their connections under the lock, the sends after it: every
        // session's talk goes through this one map.
        let mut conns: [Option<Connection>; pp::MAX_PARTY] = Default::default();
        {
            let outs = self.outs();
            for (c, s) in conns.iter_mut().zip(to.as_slice()) {
                *c = outs.get(s).map(|(_, conn)| conn.clone());
            }
        }
        for conn in conns.iter().flatten() {
            let _ = conn.send_datagram(&buf[..n]);
        }
    }

    /// Once in a while: those away too long are dropped.
    pub async fn sweeper(self: Arc<Self>) {
        let mut every = tokio::time::interval(SWEEP_EVERY);
        loop {
            every.tick().await;
            let out = self.parties().sweep(Instant::now());
            self.send(out);
        }
    }
}

/// Reads frames off the stream into a channel: a select over the channel
/// never loses half a frame.
fn reader(mut recv: RecvStream) -> tokio::sync::mpsc::Receiver<Msg> {
    let (tx, rx) = tokio::sync::mpsc::channel(64);
    tokio::spawn(async move {
        while let Some(f) = crate::net::read_frame(&mut recv).await {
            let Ok(m) = proto::decode_msg(&f) else { break };
            if tx.send(m).await.is_err() {
                break;
            }
        }
    });
    rx
}

/// Tells `session` its friends and where they are, asking Elo Pros if the
/// list it has is older than its cache (or `fresh`).
async fn friends(ctx: &Ctx, session: u64, who: &Who, fresh: bool) {
    if !who.signed() {
        return;
    }
    let rows = ctx.elo.friends(&who.id, fresh).await;
    let msgs = {
        let mut ps = ctx.lobby.parties();
        if let Some(r) = &rows {
            ps.set_friends(session, r.iter().map(|f| f.id.clone()));
        }
        let list = rows.as_deref().unwrap_or(&[]);
        let mut msgs = vec![Msg::Friends {
            n: list.len().min(pp::MAX_FRIENDS) as u8,
            reachable: rows.is_some(),
        }];
        msgs.extend(ps.friend_rows(session, list));
        msgs
    };
    ctx.lobby.send(msgs.into_iter().map(|m| (session, m)).collect());
}

/// Tells `session` what its wallet owns and its floor, for the armory
/// (a guest, or a pass with no wallet: the standard look and no gear).
async fn wardrobe(ctx: &Ctx, session: u64, who: &Who) {
    let d = match who.wallet.as_deref() {
        None => proto::Wardrobe::BARE,
        Some(w) => {
            let owned = ctx.elo.owned(w).await;
            let (skins, headwear, footwear) = proto::Wardrobe::from_names(owned.iter().flatten().map(String::as_str));
            proto::Wardrobe {
                wallet: true,
                known: owned.is_some(),
                skins,
                headwear,
                footwear,
                // Down to the whole point, as the hello's check compares.
                floor: ctx.elo.floor(w).await.map(|f| f.max(0.0) as u16),
            }
        }
    };
    ctx.lobby.send(vec![(session, Msg::Wardrobe(d))]);
}

/// One page's lobby: it is `who` (checked by the caller), named `name`,
/// asking for the party `want` (0: none). Returns when the page goes.
#[allow(clippy::too_many_arguments)]
pub async fn session(
    conn: Connection,
    mut send: SendStream,
    recv: RecvStream,
    who: Who,
    name: Name,
    want: u32,
    ctx: Arc<Ctx>,
) {
    let session = ctx.sessions.fetch_add(1, Ordering::Relaxed);
    let (tx, inlet) = outlet::outlet(ctx.counts.clone());
    let writer = tokio::spawn(async move {
        let mut streams = inlet.streams;
        let ended = streams.write_stream(&mut send).await;
        if ended == Ended::Closed {
            let _ = send.finish().await;
        }
        ended
    });
    let ticket = |w: &Who| Msg::Ticket {
        ticket: Pass::new(&ctx.nonces.ticket(w)).unwrap_or(Pass::NONE),
    };
    crate::send_msg(&tx, &ticket(&who));
    ctx.lobby.outs().insert(session, (tx, conn.clone()));

    // Friends first: a friends-only party they ask for needs them.
    let rows = if who.signed() {
        ctx.elo.friends(&who.id, false).await
    } else {
        None
    };
    let (out, replaced) = {
        let mut ps = ctx.lobby.parties();
        let ids = rows.iter().flatten().map(|f| f.id.clone());
        ps.connect(session, who.clone(), name, want, ids.collect(), Instant::now())
    };
    for s in replaced {
        // Another tab took over: this one is told so and let go.
        if let Some((tx, _)) = ctx.lobby.outs().remove(&s) {
            crate::send_msg(
                &tx,
                &Msg::Refuse {
                    reason: proto::REFUSE_BUSY,
                },
            );
            tx.close();
        }
    }
    ctx.lobby.send(out);
    friends(&ctx, session, &who, false).await;
    wardrobe(&ctx, session, &who).await;

    // Voice, straight from this reader to the party.
    let vctx = ctx.clone();
    let vconn = conn.clone();
    let voices = tokio::spawn(async move {
        let mut bucket = Bucket::default();
        let t0 = Instant::now();
        while let Ok(d) = vconn.receive_datagram().await {
            if proto::datagram_kind(&d) != Some(proto::DG_VOICE) {
                continue;
            }
            let Ok(f) = voice::decode_voice_up(&d) else { continue };
            if !voice::is_one_frame(f.payload()) || !bucket.allow(t0.elapsed().as_secs_f64() * 1000.0) {
                continue;
            }
            vctx.lobby.relay(session, &f);
        }
    });

    let mut who = who;
    let mut msgs = reader(recv);
    let mut writer = writer;
    let mut every = tokio::time::interval(FRIENDS_EVERY);
    every.tick().await;
    let mut drain_check = tokio::time::interval(Duration::from_secs(1));
    loop {
        tokio::select! {
            m = msgs.recv() => {
                let Some(m) = m else { break };
                match m {
                    Msg::Bye => break,
                    Msg::PartyAct { act: pp::ACT_FRIENDS, .. } => friends(&ctx, session, &who, true).await,
                    Msg::PartyAct { act, arg } => {
                        let out = ctx.lobby.parties().act(session, act, arg);
                        ctx.lobby.send(out);
                    }
                    Msg::PartyInvite { player } => {
                        let out = ctx.lobby.parties().invite(session, player.as_str(), Instant::now());
                        ctx.lobby.send(out);
                    }
                    Msg::SignIn { pass } => {
                        let Some(key) = ctx.elo_pubkey else { continue };
                        let checked = crate::auth::verify_pass(
                            pass.as_str(),
                            &key,
                            &ctx.slug,
                            &ctx.domains,
                            |n| ctx.nonces.take(n),
                            crate::auth::now_unix(),
                        );
                        let Ok(p) = checked else {
                            ctx.lobby.send(vec![(session, Msg::Refuse { reason: proto::REFUSE_PASS })]);
                            continue;
                        };
                        who = Who::from(p);
                        let out = ctx.lobby.parties().sign_in(session, who.clone());
                        ctx.lobby.send(vec![(session, ticket(&who))]);
                        ctx.lobby.send(out);
                        friends(&ctx, session, &who, true).await;
                        wardrobe(&ctx, session, &who).await;
                    }
                    _ => {}
                }
            }
            _ = every.tick() => friends(&ctx, session, &who, false).await,
            _ = drain_check.tick() => {
                // Going down: the page moves to the live slot, and takes
                // the party's code there.
                if ctx.draining.load(Ordering::Relaxed) {
                    ctx.lobby.send(vec![(session, Msg::Refuse { reason: proto::REFUSE_FULL })]);
                    if let Some((tx, _)) = ctx.lobby.outs().get(&session) {
                        tx.close();
                    }
                }
            }
            _ = &mut writer => break,
        }
    }
    voices.abort();
    writer.abort();
    ctx.lobby.outs().remove(&session);
    let out = ctx.lobby.parties().disconnect(session, Instant::now());
    ctx.lobby.send(out);
    conn.close(0u32.into(), b"");
}

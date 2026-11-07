//! Elo Pros' queue for this server's ranked players (`--elo-queue`): a
//! ranked hello is queued there (`elo::Elo::serve`, signed with the server
//! key), so every server of the game shares one queue, and Elo Pros puts
//! each match on whichever server in the game's list has room for it
//! (`meter/lobbies.py`). Every waiting player is asked about in one call
//! every [`POLL`] ([`poller`]); each session watches its own answer.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::sync::watch;

use crate::elo::QueueState;
use crate::Ctx;

/// How often Elo Pros is asked where the waiting stand (its matcher runs at
/// most this often a game too).
pub const POLL: Duration = Duration::from_secs(2);
/// Players one poll asks about (Elo Pros' `SERVE_PLAYERS_MAX`).
pub const POLL_MAX: usize = 256;

/// A wallet as Elo Pros spells it: an EVM one lowercased.
pub fn norm(w: &str) -> String {
    if w.starts_with("0x") {
        w.to_ascii_lowercase()
    } else {
        w.to_string()
    }
}

/// One wait's place: which wait of its wallet it is, when it began (a poll
/// asked before then says nothing of it), and where it hears.
type Place = (u64, Instant, watch::Sender<Option<QueueState>>);

/// The players waiting on Elo Pros' queue, each with where it last stood.
#[derive(Default)]
pub struct Waiters {
    by: Mutex<BTreeMap<String, Place>>,
    next: AtomicU64,
}

/// One session's wait: what the polls say of its wallet, and which wait it
/// is, so it ends its own and never a newer one's.
pub struct Wait {
    pub rx: watch::Receiver<Option<QueueState>>,
    pub id: u64,
}

impl Waiters {
    fn by(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, Place>> {
        self.by.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// `wallet` waits: what each poll says of it, from the next one on. A
    /// second wait for the same wallet (a reload) replaces the first, whose
    /// receiver then ends.
    pub fn wait(&self, wallet: &str) -> Wait {
        let (tx, rx) = watch::channel(None);
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        self.by().insert(norm(wallet), (id, Instant::now(), tx));
        Wait { rx, id }
    }

    /// `wallet`'s wait `id` is over. False if a newer wait took its place
    /// (and is left alone: so is the wallet's place in Elo Pros' queue).
    pub fn done(&self, wallet: &str, id: u64) -> bool {
        let mut by = self.by();
        let key = norm(wallet);
        let ours = by.get(&key).is_some_and(|(w, _, _)| *w == id);
        if ours {
            by.remove(&key);
        }
        ours
    }

    #[cfg(test)]
    fn is_empty(&self) -> bool {
        self.by().is_empty()
    }

    fn wallets(&self) -> Vec<String> {
        self.by().keys().cloned().collect()
    }

    /// What a poll `asked` at says of a wallet: to its wait, if that wait
    /// began before the poll was asked (an answer about a queue the player
    /// left, crossing a requeue, is the old one's).
    fn tell(&self, s: QueueState, asked: Instant) {
        if let Some((_, since, tx)) = self.by().get(&norm(&s.player)) {
            if *since <= asked {
                tx.send_replace(Some(s));
            }
        }
    }
}

/// Asks Elo Pros where every waiting player stands, every [`POLL`].
pub async fn poller(ctx: Arc<Ctx>) {
    let mut every = tokio::time::interval(POLL);
    every.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut failing = false;
    loop {
        every.tick().await;
        let wallets = ctx.waiters.wallets();
        for chunk in wallets.chunks(POLL_MAX) {
            let asked = Instant::now();
            match ctx
                .elo
                .serve(&serde_json::json!({"op": "poll", "players": chunk}))
                .await
            {
                Ok(states) => {
                    failing = false;
                    for s in states {
                        ctx.waiters.tell(s, asked);
                    }
                }
                Err(e) => {
                    // Once a run of failures, not every two seconds.
                    if !failing {
                        eprintln!("oneshot: Elo Pros' queue: {e}");
                    }
                    failing = true;
                }
            }
        }
    }
}

/// How a wait on Elo Pros' queue ended.
#[derive(Debug, PartialEq, Eq)]
pub enum Waited {
    /// Matched: Elo Pros' match id and the server it was put on.
    Matched { id: String, server: String },
    /// Out of the queue without a match, and why if Elo Pros said.
    Out(Option<String>),
}

/// How long a party member waits for whoever queues the party, before it is
/// in the queue at all.
pub const QUEUED_WITHIN: Duration = Duration::from_secs(60);
/// Waits of [`QUEUED_WITHIN`] in a row with no poll answering, once queued:
/// Elo Pros' queue has stopped answering, and the wait ends (the player is
/// told, not left at a frozen count).
pub const SILENT_WAITS: u32 = 3;

/// Waits on what the polls say of one player, calling `waiting(seconds)`
/// on each that says it still waits, until it is matched or out.
pub async fn wait(rx: &mut watch::Receiver<Option<QueueState>>, mut waiting: impl FnMut(u64)) -> Waited {
    let start = tokio::time::Instant::now();
    let mut seen_queued = false;
    let mut silent = 0;
    loop {
        let changed = tokio::time::timeout(QUEUED_WITHIN, rx.changed()).await;
        if changed.is_err() && !seen_queued {
            return Waited::Out(Some("the party never reached the queue".into()));
        }
        if changed.is_err() {
            silent += 1;
            if silent >= SILENT_WAITS {
                return Waited::Out(Some("the queue stopped answering".into()));
            }
            continue;
        }
        silent = 0;
        if matches!(changed, Ok(Err(_))) {
            return Waited::Out(None);
        }
        let Some(s) = rx.borrow_and_update().clone() else {
            continue;
        };
        match (s.state.as_deref(), &s.match_id, &s.server) {
            (Some("launched"), Some(id), Some(server)) => {
                return Waited::Matched {
                    id: id.clone(),
                    server: server.clone(),
                }
            }
            (Some("queued"), ..) => {
                seen_queued = true;
                waiting(s.waited_s.unwrap_or(0));
            }
            // Not there yet: a party member before its party is queued.
            (None, ..) if !seen_queued && start.elapsed() < QUEUED_WITHIN => {}
            _ => return Waited::Out(s.unqueued_why),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(state: Option<&str>, m: Option<&str>) -> QueueState {
        QueueState {
            player: "0xAB".into(),
            state: state.map(str::to_string),
            match_id: m.map(str::to_string),
            server: m.map(|_| "eu2.example:61250".to_string()),
            waited_s: Some(3),
            unqueued_why: None,
        }
    }

    #[tokio::test]
    async fn a_wait_ends_matched_or_out() {
        let w = Waiters::default();
        let Wait { mut rx, .. } = w.wait("0xAB");
        w.tell(st(Some("queued"), None), Instant::now());
        let mut ticks = Vec::new();
        let waiting = tokio::spawn(async move {
            let got = wait(&mut rx, |s| ticks.push(s)).await;
            (got, ticks)
        });
        tokio::task::yield_now().await;
        w.tell(st(Some("launched"), Some("q-1")), Instant::now());
        let (got, _) = waiting.await.unwrap();
        assert_eq!(
            got,
            Waited::Matched {
                id: "q-1".into(),
                server: "eu2.example:61250".into()
            }
        );
        // Taken out after queueing: out.
        let Wait { mut rx, id } = w.wait("0xab");
        w.tell(st(Some("queued"), None), Instant::now());
        w.tell(st(Some("closed"), None), Instant::now());
        assert_eq!(wait(&mut rx, |_| {}).await, Waited::Out(None));
        // A second wait (a reload) takes the wallet's place: the first one
        // ending leaves it be, and its receiver ends.
        let Wait {
            rx: mut again,
            id: newer,
        } = w.wait("0xAB");
        assert!(!w.done("0xAB", id), "an older wait ends only itself");
        assert!(!w.is_empty());
        let Wait { id: third, .. } = w.wait("0xab");
        assert_eq!(wait(&mut again, |_| {}).await, Waited::Out(None), "replaced");
        assert!(!w.done("0xab", newer));
        assert!(w.done("0xab", third));
        assert!(w.is_empty());
    }
}

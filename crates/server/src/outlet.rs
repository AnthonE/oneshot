//! What the sim thread hands a session, without ever waiting on it.
//!
//! Two lanes. Datagrams (snapshots) may drop: the next one carries
//! everything a dropped one did (events ride until acked), so a full lane
//! loses the newest and the writer always sends the newest it has. Stream
//! frames (names, the queue, the end of a match, `Close`) never drop
//! silently: a stream lane that fills means the client can't keep up, and
//! the session is cut with a reason rather than go on missing what it was
//! told (Gates' writer: never a silent drop). Every drop and cut is
//! counted in [`Counts`].

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::sync::{mpsc, Notify};

/// Snapshots waiting for one session: a quarter of a second of them.
pub const DATAGRAM_QUEUE: usize = 16;
/// Stream frames waiting for one session before it is cut. They are few
/// (a seat's name, the queue's wait, a result), so a full lane is a client
/// that stopped reading.
pub const STREAM_QUEUE: usize = 256;
/// The QUIC close code for a session cut for falling behind.
pub const CLOSE_BEHIND: u32 = 1;

/// The server's counts, across every session.
#[derive(Debug, Default)]
pub struct Counts {
    /// Snapshots never sent: the lane was full, or a newer one replaced it.
    pub datagrams_dropped: AtomicU64,
    /// Snapshots larger than the path's datagram size.
    pub datagrams_oversize: AtomicU64,
    /// Sessions cut because their stream lane filled.
    pub stream_overflows: AtomicU64,
}

/// A stream frame, or the end of the session after every frame before it.
#[derive(Debug)]
pub enum Frame {
    Bytes(Vec<u8>),
    Close,
}

#[derive(Debug, Default)]
struct Cut {
    flag: AtomicBool,
    wake: Notify,
}

/// The sim thread's end: every call returns at once.
#[derive(Debug)]
pub struct Outlet {
    dg: mpsc::Sender<Vec<u8>>,
    st: mpsc::Sender<Frame>,
    cut: Arc<Cut>,
    counts: Arc<Counts>,
}

/// The session's end: its snapshots and its [`Streams`], each for a
/// writer of its own.
#[derive(Debug)]
pub struct Inlet {
    pub datagrams: mpsc::Receiver<Vec<u8>>,
    pub streams: Streams,
}

/// The stream lane and the cut.
#[derive(Debug)]
pub struct Streams {
    stream: mpsc::Receiver<Frame>,
    cut: Arc<Cut>,
}

pub fn outlet(counts: Arc<Counts>) -> (Outlet, Inlet) {
    let (dg, datagrams) = mpsc::channel(DATAGRAM_QUEUE);
    let (st, stream) = mpsc::channel(STREAM_QUEUE);
    let cut = Arc::new(Cut::default());
    (
        Outlet {
            dg,
            st,
            cut: cut.clone(),
            counts,
        },
        Inlet {
            datagrams,
            streams: Streams { stream, cut },
        },
    )
}

impl Outlet {
    /// A snapshot: sent if there's room, else counted and dropped.
    pub fn datagram(&self, d: Vec<u8>) {
        if self.dg.try_send(d).is_err() {
            self.counts.datagrams_dropped.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// A stream frame: sent in order, or the session is cut.
    pub fn stream(&self, f: Vec<u8>) {
        self.push(Frame::Bytes(f));
    }

    /// Ends the session once every frame before this is written.
    pub fn close(&self) {
        self.push(Frame::Close);
    }

    /// Whether the session was cut for falling behind.
    pub fn is_cut(&self) -> bool {
        self.cut.flag.load(Ordering::Relaxed)
    }

    fn push(&self, f: Frame) {
        match self.st.try_send(f) {
            Ok(()) => {}
            // The session is already gone: nothing to tell.
            Err(mpsc::error::TrySendError::Closed(_)) => {}
            Err(mpsc::error::TrySendError::Full(_)) => {
                if !self.cut.flag.swap(true, Ordering::Relaxed) {
                    self.counts.stream_overflows.fetch_add(1, Ordering::Relaxed);
                    self.cut.wake.notify_one();
                }
            }
        }
    }
}

/// How a stream writer ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ended {
    /// `Close`, or the sim let the session go.
    Closed,
    /// The client stopped taking the stream.
    Failed,
    /// Cut: a frame couldn't be queued.
    Behind,
}

impl Streams {
    /// Writes stream frames in order until `Close`, a failed write or a
    /// cut, which also ends a write the client is stalling.
    pub async fn write_stream<W: AsyncWrite + Unpin>(&mut self, w: &mut W) -> Ended {
        loop {
            if self.cut.flag.load(Ordering::Relaxed) {
                return Ended::Behind;
            }
            let f = tokio::select! {
                biased;
                _ = self.cut.wake.notified() => return Ended::Behind,
                f = self.stream.recv() => f,
            };
            match f {
                Some(Frame::Bytes(b)) => {
                    tokio::select! {
                        biased;
                        _ = self.cut.wake.notified() => return Ended::Behind,
                        r = w.write_all(&b) => if r.is_err() { return Ended::Failed },
                    }
                }
                Some(Frame::Close) | None => return Ended::Closed,
            }
        }
    }
}

/// The newest of what's waiting, counting the ones it replaces.
pub fn newest(rx: &mut mpsc::Receiver<Vec<u8>>, mut d: Vec<u8>, counts: &Counts) -> Vec<u8> {
    while let Ok(n) = rx.try_recv() {
        counts.datagrams_dropped.fetch_add(1, Ordering::Relaxed);
        d = n;
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncReadExt;

    fn pair() -> (Outlet, Inlet, Arc<Counts>) {
        let counts = Arc::new(Counts::default());
        let (o, i) = outlet(counts.clone());
        (o, i, counts)
    }

    #[tokio::test]
    async fn frames_then_close_arrive_in_order() {
        let (o, mut i, counts) = pair();
        o.stream(b"ab".to_vec());
        o.stream(b"cd".to_vec());
        o.close();
        o.stream(b"after".to_vec());
        let (mut w, mut r) = tokio::io::duplex(1024);
        assert_eq!(i.streams.write_stream(&mut w).await, Ended::Closed);
        drop(w);
        let mut got = Vec::new();
        r.read_to_end(&mut got).await.unwrap();
        assert_eq!(got, b"abcd");
        assert_eq!(counts.stream_overflows.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn a_full_stream_lane_cuts_the_session_once() {
        let (o, mut i, counts) = pair();
        for _ in 0..STREAM_QUEUE + 5 {
            o.stream(vec![1]);
        }
        assert!(o.is_cut());
        assert_eq!(counts.stream_overflows.load(Ordering::Relaxed), 1);
        let (mut w, _r) = tokio::io::duplex(1 << 16);
        assert_eq!(i.streams.write_stream(&mut w).await, Ended::Behind);
    }

    #[tokio::test]
    async fn a_cut_ends_a_write_the_client_stalls() {
        let (o, mut i, _) = pair();
        o.stream(vec![7; 4096]);
        // Nobody reads: the write blocks once 16 bytes are buffered.
        let (mut w, _r) = tokio::io::duplex(16);
        let writer = tokio::spawn(async move { i.streams.write_stream(&mut w).await });
        tokio::task::yield_now().await;
        for _ in 0..STREAM_QUEUE + 1 {
            o.stream(vec![1]);
        }
        let ended = tokio::time::timeout(std::time::Duration::from_secs(5), writer).await;
        assert_eq!(ended.unwrap().unwrap(), Ended::Behind);
    }

    #[tokio::test]
    async fn datagrams_drop_counted_and_the_newest_wins() {
        let (o, mut i, counts) = pair();
        for k in 0..DATAGRAM_QUEUE as u8 + 3 {
            o.datagram(vec![k]);
        }
        assert_eq!(counts.datagrams_dropped.load(Ordering::Relaxed), 3);
        let first = i.datagrams.recv().await.unwrap();
        let d = newest(&mut i.datagrams, first, &counts);
        assert_eq!(d, vec![DATAGRAM_QUEUE as u8 - 1]);
        assert_eq!(
            counts.datagrams_dropped.load(Ordering::Relaxed),
            3 + DATAGRAM_QUEUE as u64 - 1
        );
        assert!(!o.is_cut());
    }

    #[tokio::test]
    async fn a_gone_session_is_not_a_cut() {
        let (o, i, counts) = pair();
        drop(i);
        for _ in 0..STREAM_QUEUE + 5 {
            o.stream(vec![1]);
        }
        assert!(!o.is_cut());
        assert_eq!(counts.stream_overflows.load(Ordering::Relaxed), 0);
    }
}

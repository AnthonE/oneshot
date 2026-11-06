//! What the server counted since it started, for `GET /health.json` and a
//! journal line every ten seconds (after Gates' `server/src/stats.rs` and
//! `status.rs`, MIT, trimmed): joins, refusals by cause, ticks run late or
//! skipped, how long a tick takes, and the connections' loss and round
//! trip. Atomics only: the sim thread and every session write without
//! waiting on each other, and a reader takes what is there.
//!
//! Counters are totals since start (a rate is two reads and a
//! subtraction); gauges (`players`, `rooms`) are what is true now. Every
//! value is a number.

use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::time::Duration;

use crate::outlet::Counts;

/// Log2 buckets: index 0 is under one unit, index `b` is `[2^(b-1), 2^b)`,
/// the last everything above. In µs that is up to ~0.5 s; in ms, ~9 min.
pub const BUCKETS: usize = 21;
/// Refusal causes by `proto::REFUSE_*` code (0 unused; past the last known
/// code is counted in the last).
pub const REFUSALS: usize = 15;
/// The journal's window.
pub const JOURNAL: Duration = Duration::from_secs(10);

/// A log2 histogram, and its largest sample.
#[derive(Debug, Default)]
pub struct Hist {
    pub buckets: [AtomicU64; BUCKETS],
    pub max: AtomicU64,
}

impl Hist {
    pub fn record(&self, v: u64) {
        let b = ((u64::BITS - v.leading_zeros()) as usize).min(BUCKETS - 1);
        self.buckets[b].fetch_add(1, Relaxed);
        self.max.fetch_max(v, Relaxed);
    }

    pub fn read(&self) -> [u64; BUCKETS] {
        core::array::from_fn(|b| self.buckets[b].load(Relaxed))
    }
}

/// The upper edge of the bucket below which `pct` percent of `counts` fall
/// (an over-estimate by at most 2×); 0 with none.
pub fn pct(counts: &[u64; BUCKETS], pct: u64) -> u64 {
    let total: u64 = counts.iter().sum();
    if total == 0 {
        return 0;
    }
    let want = (total * pct).div_ceil(100);
    let mut seen = 0;
    for (b, n) in counts.iter().enumerate() {
        seen += n;
        if seen >= want {
            return 1 << b;
        }
    }
    1 << (BUCKETS - 1)
}

#[derive(Debug, Default)]
pub struct Health {
    /// Connections refused before a session: the server full, or an address
    /// over its share (`net::admit`, `Gate::take`).
    pub conn_refused: AtomicU64,
    /// Connections asked to prove their address first (a crowd).
    pub conn_retried: AtomicU64,
    /// Sessions that never said hello in time, or said something else.
    pub handshake_failed: AtomicU64,
    /// Hellos turned away, by `proto::REFUSE_*` code.
    pub refused: [AtomicU64; REFUSALS],
    /// Hellos the sim seated in casual play, and took into a ranked queue
    /// or cup (counted there, not at the hello: a busy wallet's isn't).
    pub joins_casual: AtomicU64,
    pub joins_ranked: AtomicU64,
    /// Ranked players waiting when a drain began, let go to the new slot.
    pub let_go: AtomicU64,
    /// Matches opened.
    pub matches: AtomicU64,
    /// Ticks run.
    pub ticks: AtomicU64,
    /// Ticks whose work took longer than a tick.
    pub ticks_late: AtomicU64,
    /// Ticks never run: the loop fell so far behind (a stall) that it
    /// started again from now rather than sprint.
    pub ticks_skipped: AtomicU64,
    /// One tick's work, in µs.
    pub tick_us: Hist,
    /// The largest tick in the journal's window, taken by the journal.
    pub tick_window_max_us: AtomicU64,
    /// Snapshots the QUIC connection refused to send (closed, too big).
    pub send_errors: AtomicU64,
    /// Input datagrams dropped: a session over its rate, or the sim's input
    /// lane full (`net::INPUT_LANE`). Each carries the frames before it,
    /// so an honest client loses nothing to one.
    pub inputs_dropped: AtomicU64,
    /// QUIC packets sent and lost, summed over sessions as they're sampled.
    pub packets_sent: AtomicU64,
    pub packets_lost: AtomicU64,
    /// UDP bytes sent to match sessions (headers and all), summed as above.
    pub bytes_sent: AtomicU64,
    /// Sessions' round trips as sampled (every journal window and at the
    /// end), in ms.
    pub rtt_ms: Hist,
}

impl Health {
    pub fn refuse(&self, reason: u8) {
        self.refused[(reason as usize).min(REFUSALS - 1)].fetch_add(1, Relaxed);
    }

    /// A tick's work took `took`, with `tick` the budget.
    pub fn tick(&self, took: Duration, tick: Duration) {
        let us = took.as_micros().min(u64::MAX as u128) as u64;
        self.ticks.fetch_add(1, Relaxed);
        if took > tick {
            self.ticks_late.fetch_add(1, Relaxed);
        }
        self.tick_us.record(us);
        self.tick_window_max_us.fetch_max(us, Relaxed);
    }

    /// A QUIC connection's numbers since the last sample (`last` holds
    /// what was seen before, so each packet and byte is counted once).
    pub fn sample(&self, rtt: Duration, sent: u64, lost: u64, bytes: u64, last: &mut (u64, u64, u64)) {
        self.rtt_ms.record(rtt.as_millis().min(u64::MAX as u128) as u64);
        self.packets_sent.fetch_add(sent.saturating_sub(last.0), Relaxed);
        self.packets_lost.fetch_add(lost.saturating_sub(last.1), Relaxed);
        self.bytes_sent.fetch_add(bytes.saturating_sub(last.2), Relaxed);
        *last = (sent.max(last.0), lost.max(last.1), bytes.max(last.2));
    }

    /// Everything, as numbers.
    pub fn read(&self, counts: &Counts) -> Read {
        let g = |a: &AtomicU64| a.load(Relaxed);
        Read {
            conn_refused: g(&self.conn_refused),
            conn_retried: g(&self.conn_retried),
            handshake_failed: g(&self.handshake_failed),
            refused: core::array::from_fn(|i| g(&self.refused[i])),
            joins_casual: g(&self.joins_casual),
            joins_ranked: g(&self.joins_ranked),
            let_go: g(&self.let_go),
            matches: g(&self.matches),
            ticks: g(&self.ticks),
            ticks_late: g(&self.ticks_late),
            ticks_skipped: g(&self.ticks_skipped),
            tick_us: self.tick_us.read(),
            tick_max_us: g(&self.tick_us.max),
            send_errors: g(&self.send_errors),
            inputs_dropped: g(&self.inputs_dropped),
            packets_sent: g(&self.packets_sent),
            packets_lost: g(&self.packets_lost),
            bytes_sent: g(&self.bytes_sent),
            rtt_ms: self.rtt_ms.read(),
            rtt_max_ms: g(&self.rtt_ms.max),
            snapshots_dropped: g(&counts.datagrams_dropped),
            snapshots_oversize: g(&counts.datagrams_oversize),
            cut_behind: g(&counts.stream_overflows),
        }
    }
}

/// One read of [`Health`] and the outlet's [`Counts`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Read {
    pub conn_refused: u64,
    pub conn_retried: u64,
    pub handshake_failed: u64,
    pub refused: [u64; REFUSALS],
    pub joins_casual: u64,
    pub joins_ranked: u64,
    pub let_go: u64,
    pub matches: u64,
    pub ticks: u64,
    pub ticks_late: u64,
    pub ticks_skipped: u64,
    pub tick_us: [u64; BUCKETS],
    pub tick_max_us: u64,
    pub send_errors: u64,
    pub inputs_dropped: u64,
    pub packets_sent: u64,
    pub packets_lost: u64,
    pub bytes_sent: u64,
    pub rtt_ms: [u64; BUCKETS],
    pub rtt_max_ms: u64,
    pub snapshots_dropped: u64,
    pub snapshots_oversize: u64,
    pub cut_behind: u64,
}

/// What `/health.json` carries beside the counters: what is true now.
#[derive(Clone, Copy, Debug, Default)]
pub struct Now {
    pub ready: bool,
    pub proto: u16,
    pub players: usize,
    pub capacity: usize,
    pub rooms: usize,
    pub uptime_s: u64,
    /// The threads rooms step on (`Config::sim_threads`).
    pub sim_threads: usize,
}

fn list(v: &[u64]) -> String {
    let parts: Vec<String> = v.iter().map(u64::to_string).collect();
    format!("[{}]", parts.join(","))
}

/// The `/health.json` document: one line, numbers and one boolean. Beside
/// the tick's percentiles since start, its buckets (`tick_us`, as
/// [`Hist`]'s): two reads give a window's (a load test's).
pub fn json(now: Now, r: &Read) -> String {
    format!(
        "{{\"ready\":{},\"proto\":{},\"players\":{},\"capacity\":{},\"rooms\":{},\"uptime_s\":{},\"sim_threads\":{},\
         \"joins_casual\":{},\"joins_ranked\":{},\"let_go\":{},\"matches\":{},\
         \"conn_refused\":{},\"conn_retried\":{},\"handshake_failed\":{},\"refused\":{},\
         \"ticks\":{},\"ticks_late\":{},\"ticks_skipped\":{},\
         \"tick_p50_us\":{},\"tick_p99_us\":{},\"tick_max_us\":{},\"tick_us\":{},\
         \"snapshots_dropped\":{},\"snapshots_oversize\":{},\"cut_behind\":{},\"send_errors\":{},\"inputs_dropped\":{},\
         \"packets_sent\":{},\"packets_lost\":{},\"bytes_sent\":{},\
         \"rtt_p50_ms\":{},\"rtt_p99_ms\":{},\"rtt_max_ms\":{}}}",
        now.ready,
        now.proto,
        now.players,
        now.capacity,
        now.rooms,
        now.uptime_s,
        now.sim_threads,
        r.joins_casual,
        r.joins_ranked,
        r.let_go,
        r.matches,
        r.conn_refused,
        r.conn_retried,
        r.handshake_failed,
        list(&r.refused),
        r.ticks,
        r.ticks_late,
        r.ticks_skipped,
        pct(&r.tick_us, 50),
        pct(&r.tick_us, 99),
        r.tick_max_us,
        list(&r.tick_us),
        r.snapshots_dropped,
        r.snapshots_oversize,
        r.cut_behind,
        r.send_errors,
        r.inputs_dropped,
        r.packets_sent,
        r.packets_lost,
        r.bytes_sent,
        pct(&r.rtt_ms, 50),
        pct(&r.rtt_ms, 99),
        r.rtt_max_ms,
    )
}

/// Whether a window is worth a line: someone was on, or something other
/// than an idle tick happened (a stalled idle server still says so).
pub fn eventful(now: Now, was: &Read, is: &Read) -> bool {
    let mut quiet = is.clone();
    quiet.ticks = was.ticks;
    quiet.tick_us = was.tick_us;
    quiet.tick_max_us = was.tick_max_us;
    now.players > 0 || now.rooms > 0 || quiet != *was
}

/// The journal's line for a window: what changed since `was`, the tick's
/// percentiles over the window alone, and its largest tick.
pub fn line(now: Now, was: &Read, is: &Read, window_max_us: u64) -> String {
    let d = |a: u64, b: u64| b.saturating_sub(a);
    let window: [u64; BUCKETS] = core::array::from_fn(|b| d(was.tick_us[b], is.tick_us[b]));
    let refused = d(was.refused.iter().sum(), is.refused.iter().sum());
    let (sent, lost) = (
        d(was.packets_sent, is.packets_sent),
        d(was.packets_lost, is.packets_lost),
    );
    let loss = if sent == 0 {
        0.0
    } else {
        100.0 * lost as f64 / sent as f64
    };
    format!(
        "oneshot: status players={} rooms={} joins=+{} refused=+{} conn_refused=+{} \
         ticks=+{} late=+{} skipped=+{} tick_p50={}us tick_p99={}us tick_max={}us \
         dropped=+{} cut=+{} send_err=+{} inputs_dropped=+{} loss={loss:.2}%",
        now.players,
        now.rooms,
        d(was.joins_casual + was.joins_ranked, is.joins_casual + is.joins_ranked),
        refused,
        d(was.conn_refused, is.conn_refused),
        d(was.ticks, is.ticks),
        d(was.ticks_late, is.ticks_late),
        d(was.ticks_skipped, is.ticks_skipped),
        pct(&window, 50),
        pct(&window, 99),
        window_max_us,
        d(was.snapshots_dropped, is.snapshots_dropped),
        d(was.cut_behind, is.cut_behind),
        d(was.send_errors, is.send_errors),
        d(was.inputs_dropped, is.inputs_dropped),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buckets_and_percentiles() {
        let h = Hist::default();
        // 0 → bucket 0; 1 → 1; 2..3 → 2; 1000 → 10 (512..1023).
        for v in [0, 1, 3, 3, 1000] {
            h.record(v);
        }
        let c = h.read();
        assert_eq!((c[0], c[1], c[2], c[10]), (1, 1, 2, 1));
        assert_eq!(h.max.load(Relaxed), 1000);
        assert_eq!(pct(&c, 50), 4);
        assert_eq!(pct(&c, 99), 1024);
        assert_eq!(pct(&[0; BUCKETS], 50), 0);
        h.record(u64::MAX);
        assert_eq!(h.read()[BUCKETS - 1], 1);
    }

    #[test]
    fn a_tick_over_budget_is_late() {
        let s = Health::default();
        let tick = Duration::from_micros(7812);
        s.tick(Duration::from_micros(500), tick);
        s.tick(Duration::from_micros(9000), tick);
        let r = s.read(&Counts::default());
        assert_eq!((r.ticks, r.ticks_late, r.tick_max_us), (2, 1, 9000));
    }

    #[test]
    fn packets_are_counted_once_across_samples() {
        let s = Health::default();
        let mut last = (0, 0, 0);
        s.sample(Duration::from_millis(40), 100, 2, 9000, &mut last);
        s.sample(Duration::from_millis(42), 250, 3, 20000, &mut last);
        let r = s.read(&Counts::default());
        assert_eq!((r.packets_sent, r.packets_lost, r.bytes_sent), (250, 3, 20000));
        assert_eq!(r.rtt_max_ms, 42);
    }

    #[test]
    fn refusals_land_by_code_and_odd_codes_in_the_last() {
        let s = Health::default();
        s.refuse(proto::REFUSE_FULL);
        s.refuse(200);
        let r = s.read(&Counts::default());
        assert_eq!(r.refused[proto::REFUSE_FULL as usize], 1);
        assert_eq!(r.refused[REFUSALS - 1], 1);
    }

    #[test]
    fn the_document_is_json_of_numbers() {
        let s = Health::default();
        s.tick(Duration::from_micros(300), Duration::from_millis(8));
        let doc = json(
            Now {
                ready: true,
                proto: 9,
                players: 3,
                capacity: 64,
                rooms: 1,
                uptime_s: 5,
                sim_threads: 4,
            },
            &s.read(&Counts::default()),
        );
        assert!(doc.starts_with("{\"ready\":true,\"proto\":9,\"players\":3,"));
        assert!(doc.contains("\"refused\":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]"));
        assert!(doc.contains("\"tick_p50_us\":512"));
        assert!(doc.contains("\"sim_threads\":4,"));
        assert!(doc.contains("\"tick_us\":[0,0,0,0,0,0,0,0,0,1,0,"), "{doc}");
        assert!(!doc.contains('\n'));
    }

    #[test]
    fn the_journal_line_is_the_window_only() {
        let s = Health::default();
        let tick = Duration::from_millis(8);
        s.tick(Duration::from_micros(4000), tick);
        let was = s.read(&Counts::default());
        s.tick(Duration::from_micros(100), tick);
        s.joins_casual.fetch_add(2, Relaxed);
        let is = s.read(&Counts::default());
        let l = line(Now::default(), &was, &is, 100);
        assert!(l.contains("joins=+2"), "{l}");
        assert!(l.contains("ticks=+1"), "{l}");
        assert!(l.contains("tick_p99=128us"), "{l}");
        assert!(l.contains("tick_max=100us"), "{l}");
    }

    #[test]
    fn an_idle_window_says_nothing_but_a_late_tick_does() {
        let s = Health::default();
        let tick = Duration::from_millis(8);
        let was = s.read(&Counts::default());
        s.tick(Duration::from_micros(5), tick);
        let idle = s.read(&Counts::default());
        assert!(!eventful(Now::default(), &was, &idle));
        s.tick(Duration::from_millis(20), tick);
        assert!(eventful(Now::default(), &idle, &s.read(&Counts::default())));
        let on = Now {
            players: 1,
            ..Now::default()
        };
        assert!(eventful(on, &was, &idle));
    }
}

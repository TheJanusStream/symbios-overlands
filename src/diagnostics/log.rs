//! `SessionLog` — the single funnel every subsystem records diagnostic events
//! into (Pillar A-2). It owns the in-memory ring buffer that backs both the
//! in-game event log (a bounded tail view) and the wasm "Download log" button;
//! the native NDJSON file sink is bolted on in A-3, and the flush / panic-hook
//! plumbing in A-5.
//!
//! Timestamps are dual: `t_mono_secs` is session-relative (the caller passes
//! `Time::elapsed_secs_f64`, the same session-relative clock the HUD uses) and
//! `wall_ms` is an absolute unix-epoch stamp for cross-run correlation. The
//! wall clock is read through [`wall_now_ms`], which is cfg-split so it never
//! calls `std::time` on wasm32 (that panics — see the WASM time gotcha).

use std::collections::VecDeque;

use bevy::prelude::Resource;

use crate::diagnostics::event::{EventPayload, SessionEvent, Severity};
use crate::diagnostics::sink::Sink;

/// The append-only session-event stream, resident as a Bevy resource.
///
/// `seq` is a gap-free per-process counter that keeps advancing across segment
/// resets, so the analyzer can tell a torn tail from a fresh segment. The ring
/// is bounded ([`RING_CAPACITY`](crate::config::diagnostics::RING_CAPACITY)):
/// once the native sink (A-3) is appending every event to disk the ring is only
/// a recent-history window, so dropping the oldest in-memory event loses
/// nothing durable.
#[derive(Resource)]
pub struct SessionLog {
    next_seq: u64,
    /// Wall-clock millis captured at the first recorded event — used by the
    /// native sink (A-3) to name the per-session file.
    session_start_wall_ms: Option<u64>,
    ring: VecDeque<SessionEvent>,
    ring_cap: usize,
    /// The durable sink events are mirrored to (native NDJSON file; no-op when
    /// disabled or on wasm). Attached by the plugin (A-5) via [`set_sink`].
    ///
    /// [`set_sink`]: SessionLog::set_sink
    sink: Sink,
    /// Events appended to the sink since the last [`flush`](SessionLog::flush) —
    /// lets the flush scheduler (A-5) flush every N events.
    since_flush: usize,
}

impl Default for SessionLog {
    fn default() -> Self {
        Self::with_capacity(crate::config::diagnostics::RING_CAPACITY)
    }
}

impl SessionLog {
    /// Construct with an explicit ring capacity (used by tests; production uses
    /// [`Default`] which reads the config constant).
    pub fn with_capacity(ring_cap: usize) -> Self {
        SessionLog {
            next_seq: 0,
            session_start_wall_ms: None,
            ring: VecDeque::with_capacity(ring_cap.min(1024)),
            ring_cap: ring_cap.max(1),
            sink: Sink::disabled(),
            since_flush: 0,
        }
    }

    /// Record an event with the given session-relative time, severity and
    /// payload. The subsystem/category are derived from the payload
    /// ([`SessionEvent::new`]); the sequence number and wall clock are stamped
    /// here. Goes to both the durable sink and the in-memory ring (GUI tail).
    /// Returns the assigned `seq`.
    pub fn record(&mut self, t_mono_secs: f64, severity: Severity, payload: EventPayload) -> u64 {
        self.write(t_mono_secs, severity, payload, true, false)
    }

    /// Record an event to the durable sink **only**, skipping the in-memory
    /// ring — for high-frequency file/analyzer-only telemetry (metric
    /// snapshots) that would otherwise crowd the GUI tail and evict real events
    /// from the bounded ring. On wasm (no file sink) it falls back to the ring,
    /// since there the ring *is* the downloadable log.
    pub fn record_file_only(
        &mut self,
        t_mono_secs: f64,
        severity: Severity,
        payload: EventPayload,
    ) -> u64 {
        let to_ring = matches!(self.sink, Sink::Disabled);
        self.write(t_mono_secs, severity, payload, to_ring, true)
    }

    fn write(
        &mut self,
        t_mono_secs: f64,
        severity: Severity,
        payload: EventPayload,
        to_ring: bool,
        file_only: bool,
    ) -> u64 {
        let wall = wall_now_ms();
        if self.session_start_wall_ms.is_none() {
            self.session_start_wall_ms = wall;
        }
        let seq = self.next_seq;
        self.next_seq += 1;
        let ev = SessionEvent::new(seq, t_mono_secs, wall, severity, payload);
        // Append to the durable sink first, so an over-capacity ring drop can
        // never lose a line from the on-disk file. The serialize is skipped
        // entirely when the sink is disabled (tests / wasm / SYMBIOS_DIAG=0),
        // where the ring is the only store.
        // Serialize when there is a durable sink to append to, and always on
        // wasm — there the crash tail (#1145) is the only capture, so its
        // rolling NDJSON store needs the line whether or not a sink exists.
        // One serialize per event either way: the wasm tail used to be rebuilt
        // from the whole ring on a 5 s timer instead.
        let has_sink = !matches!(self.sink, Sink::Disabled);
        if (has_sink || cfg!(target_arch = "wasm32"))
            && let Ok(line) = serde_json::to_string(&ev)
        {
            if has_sink {
                self.sink.append_line(&line);
                self.since_flush += 1;
            }
            // Mirror into the process-global shadow so a hook can write the
            // recent tail the BufWriter (native) or the persist timer (wasm)
            // hasn't stored. High-frequency file-only telemetry (the 1 Hz
            // `MetricsSnapshot`) is kept apart from the real events either
            // way, so it can't evict the pre-crash evidence: an overwrite slot
            // on native (#633), a budget of its own in the wasm tail (#1180).
            // `seq` rides along because the wasm tail evicts the two classes
            // independently and has to merge them back in order.
            if file_only {
                crate::diagnostics::panic::shadow_push_snapshot(seq, &line);
            } else {
                crate::diagnostics::panic::shadow_push(seq, &line);
            }
            // The clock is unreachable from a hook, so the shadow keeps the
            // last stamp it saw and a terminal marker borrows it (#1142).
            crate::diagnostics::panic::note_timestamp(t_mono_secs);
        }
        if to_ring {
            self.ring.push_back(ev);
            while self.ring.len() > self.ring_cap {
                self.ring.pop_front();
            }
        }
        seq
    }

    /// Convenience: record at [`Severity::Info`].
    pub fn info(&mut self, t_mono_secs: f64, payload: EventPayload) -> u64 {
        self.record(t_mono_secs, Severity::Info, payload)
    }

    /// Convenience: record at [`Severity::Warn`].
    pub fn warn(&mut self, t_mono_secs: f64, payload: EventPayload) -> u64 {
        self.record(t_mono_secs, Severity::Warn, payload)
    }

    /// Convenience: record at [`Severity::Error`].
    pub fn error(&mut self, t_mono_secs: f64, payload: EventPayload) -> u64 {
        self.record(t_mono_secs, Severity::Error, payload)
    }

    /// The most recent `n` events (oldest→newest), for the GUI tail view. The
    /// GUI passes [`MAX_DIAGNOSTICS_ENTRIES`](crate::config::state::MAX_DIAGNOSTICS_ENTRIES)
    /// so the on-screen log stays bounded independently of the ring size.
    pub fn tail(&self, n: usize) -> impl Iterator<Item = &SessionEvent> {
        let skip = self.ring.len().saturating_sub(n);
        self.ring.iter().skip(skip)
    }

    /// Every event currently in the ring (oldest→newest).
    pub fn iter(&self) -> impl Iterator<Item = &SessionEvent> {
        self.ring.iter()
    }

    /// Number of events currently held in the ring.
    pub fn len(&self) -> usize {
        self.ring.len()
    }

    /// Whether the ring is currently empty.
    pub fn is_empty(&self) -> bool {
        self.ring.is_empty()
    }

    /// The next sequence number that will be assigned.
    pub fn next_seq(&self) -> u64 {
        self.next_seq
    }

    /// Wall-clock millis of the first recorded event (session start).
    pub fn session_start_wall_ms(&self) -> Option<u64> {
        self.session_start_wall_ms
    }

    /// Begin a fresh session segment (logout / room change): record a
    /// [`EventPayload::SessionSegmentReset`] marker, then clear the in-memory
    /// ring so the GUI tail view starts blank and no prior-session events leak
    /// into the next user's HUD. `seq` keeps advancing across the boundary.
    /// The native sink (A-3) appends the marker to the durable file *before*
    /// this clear — the process keeps writing the same per-run file, with
    /// segments delimited by these markers, so on-disk history is preserved.
    pub fn reset_segment(&mut self, t_mono_secs: f64, reason: impl Into<String>) {
        self.record(
            t_mono_secs,
            Severity::Info,
            EventPayload::SessionSegmentReset {
                reason: reason.into(),
            },
        );
        self.ring.clear();
    }

    /// Attach (or replace) the durable sink. The plugin (A-5) builds a native
    /// [`Sink`] at startup and injects it here; tests inject an in-temp-dir
    /// sink. A [`Sink::Disabled`] leaves the log in-memory only.
    pub fn set_sink(&mut self, sink: Sink) {
        self.sink = sink;
    }

    /// Flush the durable sink (refreshing `session-latest.jsonl`) and reset the
    /// since-last-flush counter. No-op when the sink is disabled.
    pub fn flush(&mut self) {
        self.sink.flush();
        self.since_flush = 0;
    }

    /// Events appended to the sink since the last flush — the flush scheduler
    /// (A-5) flushes once this crosses `FLUSH_EVERY_N_EVENTS`.
    pub fn pending_since_flush(&self) -> usize {
        self.since_flush
    }

    /// The stable `session-latest.jsonl` path, for the native path label in the
    /// Diagnostics panel (A-8). `None` when the sink is disabled / on wasm.
    pub fn sink_path(&self) -> Option<String> {
        self.sink.latest_path_display()
    }

    /// Serialize the whole ring as newline-delimited JSON — the payload for the
    /// wasm "Download session log" button (A-8). Byte-compatible with the
    /// native `.jsonl` file so both feed the same `--analyze-session` analyzer.
    ///
    /// Reserved from the first line rather than grown from empty (#1136). A
    /// full ring is several megabytes of NDJSON, and a `String` growing into
    /// that from zero doubles a dozen times, each step allocating the new
    /// buffer while the old one is still live — so the peak is around 1.5x the
    /// result and the intermediate buffers are pure churn. That is worth
    /// avoiding here specifically because this is the wasm path, where the
    /// allocator never gives any of it back to the browser (#565), and the
    /// button exists to be pressed on a session that is already in trouble.
    ///
    /// The estimate does not have to be right — `push_str` still grows if it
    /// is short. Events vary in size, so this is one sample times the count
    /// with headroom, not a computed total, which would cost a second
    /// serialize of the whole ring to learn.
    pub fn drain_ndjson(&self) -> String {
        let mut lines = self
            .ring
            .iter()
            .filter_map(|ev| serde_json::to_string(ev).ok());
        let Some(first) = lines.next() else {
            return String::new();
        };
        let mut out = String::with_capacity((first.len() + 1) * self.ring.len() * 5 / 4);
        out.push_str(&first);
        out.push('\n');
        for line in lines {
            out.push_str(&line);
            out.push('\n');
        }
        out
    }
}

/// Current unix-epoch time in milliseconds, or `None` if unavailable. Cfg-split
/// so wasm never touches `std::time` (which panics on wasm32).
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn wall_now_ms() -> Option<u64> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_millis() as u64)
}

/// Current unix-epoch time in milliseconds via `Date.now()` (wasm build).
#[cfg(target_arch = "wasm32")]
pub(crate) fn wall_now_ms() -> Option<u64> {
    Some(js_sys::Date::now() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::event::EventPayload;

    fn ev(text: &str) -> EventPayload {
        EventPayload::SessionEnd {
            reason: text.into(),
        }
    }

    #[test]
    fn seq_is_monotonic_and_gapfree() {
        let mut log = SessionLog::with_capacity(8);
        for i in 0..5 {
            let seq = log.info(i as f64, ev(&format!("e{i}")));
            assert_eq!(seq, i);
        }
        assert_eq!(log.next_seq(), 5);
    }

    #[test]
    fn ring_drops_oldest_over_capacity() {
        let mut log = SessionLog::with_capacity(3);
        for i in 0..6 {
            log.info(i as f64, ev(&format!("e{i}")));
        }
        assert_eq!(log.len(), 3, "ring bounded to capacity");
        // seq keeps advancing even though older events were dropped.
        assert_eq!(log.next_seq(), 6);
        let seqs: Vec<u64> = log.iter().map(|e| e.seq).collect();
        assert_eq!(seqs, vec![3, 4, 5], "kept the newest three");
    }

    #[test]
    fn tail_is_bounded_and_newest_last() {
        let mut log = SessionLog::with_capacity(100);
        for i in 0..10 {
            log.info(i as f64, ev(&format!("e{i}")));
        }
        let tail: Vec<u64> = log.tail(3).map(|e| e.seq).collect();
        assert_eq!(tail, vec![7, 8, 9]);
        // Asking for more than we have returns everything, not padding.
        assert_eq!(log.tail(1000).count(), 10);
    }

    #[test]
    fn reset_segment_records_marker_then_clears_ring_but_keeps_seq() {
        let mut log = SessionLog::with_capacity(100);
        log.info(0.0, ev("before"));
        log.reset_segment(1.0, "logout");
        // Ring cleared (GUI starts fresh)...
        assert!(log.is_empty());
        // ...but seq advanced past the marker (before=0, marker=1, next=2).
        assert_eq!(log.next_seq(), 2);
        log.info(2.0, ev("after"));
        assert_eq!(log.iter().next().unwrap().seq, 2);
    }

    #[test]
    fn drain_ndjson_is_one_parseable_line_per_event() {
        let mut log = SessionLog::with_capacity(100);
        log.info(0.0, ev("a"));
        log.warn(1.0, ev("b"));
        let dump = log.drain_ndjson();
        let lines: Vec<&str> = dump.lines().collect();
        assert_eq!(lines.len(), 2);
        for line in lines {
            let _: SessionEvent = serde_json::from_str(line).expect("each line parses");
        }
    }

    /// The empty ring must not fall off the reserve path — a session that
    /// pressed Download before anything was recorded would otherwise index
    /// a first line that does not exist.
    #[test]
    fn drain_ndjson_of_an_empty_ring_is_empty() {
        let log = SessionLog::with_capacity(8);
        assert!(log.drain_ndjson().is_empty());
    }

    /// #1136: the dump is reserved from a sample rather than grown from zero,
    /// so a full ring is one allocation instead of a dozen doublings on a heap
    /// that never shrinks. The sample is an estimate, so this asserts the
    /// contract that matters — that the reserve is at least the result, i.e.
    /// nothing reallocated — rather than an exact figure.
    #[test]
    fn drain_ndjson_reserves_enough_not_to_regrow() {
        let mut log = SessionLog::with_capacity(64);
        for i in 0..64 {
            log.info(i as f64, ev("padding-event-name"));
        }
        let dump = log.drain_ndjson();
        assert!(
            dump.capacity() >= dump.len(),
            "capacity {} under length {} — the reserve was short and it regrew",
            dump.capacity(),
            dump.len()
        );
        assert_eq!(dump.lines().count(), 64);
    }

    #[test]
    fn wall_clock_is_populated_on_native() {
        let mut log = SessionLog::with_capacity(4);
        log.info(0.0, ev("x"));
        // Native test target: the std clock is available and non-zero.
        assert!(log.session_start_wall_ms().unwrap_or(0) > 0);
    }
}

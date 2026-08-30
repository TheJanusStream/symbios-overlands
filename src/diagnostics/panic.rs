//! Process-global panic shadow (Pillar A-5).
//!
//! A Bevy `Resource` cannot be reached from a panic hook (the hook runs on the
//! panicking thread with no `World` access), so [`SessionLog::record`] mirrors
//! each serialized line into a small global ring here. On a native crash the
//! installed hook dumps that ring — plus a synthetic crash marker — to
//! `session-panic-<pid>-<millis>.jsonl` next to the session log, so the last
//! events before the fault survive even though the `BufWriter`'s unflushed tail
//! would otherwise be lost.
//!
//! [`SessionLog::record`]: crate::diagnostics::log::SessionLog::record

/// Human-readable crash reason from a panic message + optional source location.
/// Pulled out as a pure fn so it can be unit-tested without a real panic, and
/// shared across targets since #1145 — the wasm hook writes the same string
/// into its `localStorage` terminal record that the native hook writes into
/// the panic file, so one panic reads identically in either capture.
pub(crate) fn format_panic_reason(msg: &str, location: Option<(&str, u32)>) -> String {
    match location {
        Some((file, line)) => format!("panic at {file}:{line}: {msg}"),
        None => format!("panic: {msg}"),
    }
}

/// The message carried by a panic payload, or `"panic"` when it is neither of
/// the two shapes `std` boxes.
pub(crate) fn panic_message<'a>(info: &'a std::panic::PanicHookInfo<'a>) -> &'a str {
    info.payload()
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str))
        .unwrap_or("panic")
}

/// Session-relative time of the most recent recorded event, as raw `f64` bits
/// (#1142).
///
/// A panic hook has no `World`, so `Time::elapsed_secs_f64` is out of reach
/// and the crash marker used to be stamped `0.0`. That is not a harmless
/// placeholder: the analyzer reads the last timestamp to decide whether a
/// start event ever got its end, so a zero at the end of the file made
/// `last - start` negative and turned off `LoadingGateStall`,
/// `AmbientBakeStall`, `TaskNeverResolves` and `GlareSuspected` — on exactly
/// the log where a job hanging before the fault is the likeliest story. An
/// atomic beside the log is reachable from a hook and costs one relaxed store
/// per recorded event.
static LAST_T_BITS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Note the session-relative time of an event as it is recorded, so a
/// terminal marker written from a hook can be stamped with it.
pub fn note_timestamp(t_mono_secs: f64) {
    if t_mono_secs.is_finite() && t_mono_secs >= 0.0 {
        LAST_T_BITS.store(t_mono_secs.to_bits(), std::sync::atomic::Ordering::Relaxed);
    }
}

/// The stamp for a terminal marker: the last event the capture actually
/// contains. Deliberately equal to it rather than nudged past it — a hook
/// knows the exit came after that event and nothing more, and an invented
/// epsilon would read as precision the timestamp does not have.
pub(crate) fn marker_ts() -> f64 {
    f64::from_bits(LAST_T_BITS.load(std::sync::atomic::Ordering::Relaxed))
}

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use std::collections::VecDeque;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Mutex, OnceLock};

    use crate::diagnostics::event::{CRASH_MARKER_SEQ, EventPayload, SessionEvent, Severity};
    use crate::diagnostics::log::wall_now_ms;

    /// Recent serialized NDJSON lines mirrored for the panic hook.
    const SHADOW_CAP: usize = 512;

    struct Shadow {
        dir: PathBuf,
        lines: VecDeque<String>,
        /// The single most-recent high-frequency snapshot line (metric vitals).
        /// Held in an overwrite slot rather than the `lines` ring so the 1 Hz
        /// `MetricsSnapshot` drip can't evict the real pre-crash events the
        /// panic file exists to preserve (#633) — while the last known vitals
        /// (RSS, image/mesh handle counts, CPU) still reach the dump for an
        /// OOM / leak post-mortem.
        last_snapshot: Option<String>,
    }

    static SHADOW: OnceLock<Mutex<Shadow>> = OnceLock::new();
    static INSTALLED: AtomicBool = AtomicBool::new(false);

    /// Arm the shadow with the directory panic files are written to. Idempotent
    /// after the first call (the `OnceLock` keeps the first dir).
    pub fn arm(dir: PathBuf) {
        let _ = SHADOW.set(Mutex::new(Shadow {
            dir,
            lines: VecDeque::with_capacity(SHADOW_CAP),
            last_snapshot: None,
        }));
    }

    /// Mirror one already-serialized real-event line into the shadow ring.
    ///
    /// `seq` is unused here — the native ring renders in push order, which is
    /// seq order — but it is part of the signature because the wasm side needs
    /// it to merge two independently-evicting queues (#1180), and
    /// `SessionLog::write` has one call site per target.
    pub fn shadow_push(_seq: u64, line: &str) {
        if let Some(m) = SHADOW.get()
            && let Ok(mut s) = m.lock()
        {
            s.lines.push_back(line.to_string());
            while s.lines.len() > SHADOW_CAP {
                s.lines.pop_front();
            }
        }
    }

    /// Record the most-recent high-frequency snapshot line, *overwriting* any
    /// prior one. Unlike [`shadow_push`] this never grows the ring, so the
    /// periodic metric-snapshot drip can't evict real events (#633) — yet the
    /// latest vitals still land in the panic file.
    pub fn shadow_push_snapshot(_seq: u64, line: &str) {
        if let Some(m) = SHADOW.get()
            && let Ok(mut s) = m.lock()
        {
            s.last_snapshot = Some(line.to_string());
        }
    }

    /// Install the crash-dump panic hook, chaining the previous hook so the
    /// normal panic message still prints. Idempotent.
    pub fn install_hook() {
        if INSTALLED.swap(true, Ordering::SeqCst) {
            return;
        }
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            write_panic_file(info);
            prev(info);
        }));
    }

    fn write_panic_file(info: &std::panic::PanicHookInfo) {
        use std::io::Write;
        let Some(m) = SHADOW.get() else { return };
        let Ok(shadow) = m.lock() else { return };
        let millis = wall_now_ms().unwrap_or(0);
        let path = shadow.dir.join(format!(
            "session-panic-{}-{millis}.jsonl",
            std::process::id()
        ));
        let Ok(mut f) = std::fs::File::create(&path) else {
            return;
        };
        for line in &shadow.lines {
            let _ = writeln!(f, "{line}");
        }
        // The last known metric snapshot (vitals just before the fault). Kept
        // out of the event ring above so it couldn't evict real events, appended
        // here so an OOM / leak post-mortem still sees final RSS / handle counts.
        if let Some(snap) = &shadow.last_snapshot {
            let _ = writeln!(f, "{snap}");
        }
        // Final synthetic marker. `CRASH_MARKER_SEQ` is the sentinel (the real
        // sequence isn't reachable from the hook), and the timestamp is the
        // last one an event carried — see `LAST_T_BITS`.
        let reason = super::format_panic_reason(
            super::panic_message(info),
            info.location().map(|l| (l.file(), l.line())),
        );
        let ev = SessionEvent::new(
            CRASH_MARKER_SEQ,
            super::marker_ts(),
            wall_now_ms(),
            Severity::Critical,
            EventPayload::SessionEnd { reason },
        );
        if let Ok(line) = serde_json::to_string(&ev) {
            let _ = writeln!(f, "{line}");
        }
        let _ = f.flush();
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn snapshots_never_enter_the_real_event_ring() {
            // #633: real events accumulate in the bounded ring; the 1 Hz
            // snapshot drip goes to a single overwrite slot instead, so it can't
            // evict them. Asserting "no snapshot ever lands in `lines`" is robust
            // to the process-global shadow being touched by other tests: only
            // this test emits the `r633-` markers, and a `snap` marker can reach
            // `last_snapshot` but never `lines`.
            //
            // Arm into the temp dir, NOT ".": the shadow (incl. its dump dir)
            // is process-global, so arming CWD here made any later
            // intentionally-panicking test in the same run drop a
            // `session-panic-*.jsonl` into the repo root (#676).
            arm(std::env::temp_dir());
            shadow_push(0, "r633-real-1");
            shadow_push_snapshot(1, "r633-snap-1");
            shadow_push(2, "r633-real-2");
            shadow_push_snapshot(3, "r633-snap-2");

            let s = SHADOW.get().unwrap().lock().unwrap();
            assert!(
                s.last_snapshot.is_some(),
                "the latest snapshot is retained for the panic dump"
            );
            assert!(
                !s.lines.iter().any(|l| l.starts_with("r633-snap-")),
                "snapshots must never enter the real-event ring (#633)"
            );
            assert!(
                s.lines.iter().any(|l| l == "r633-real-1")
                    && s.lines.iter().any(|l| l == "r633-real-2"),
                "real events still accumulate in the ring"
            );
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use imp::{arm, install_hook, shadow_push, shadow_push_snapshot};

// Wasm has no filesystem, so there is no panic FILE to write — but there is a
// capture, and since #1145 it gets a terminal record too. The rolling NDJSON
// tail lives in `crash_log` beside the `localStorage` slot it is written to;
// these forward so `SessionLog::write` has one call site per target.
#[cfg(target_arch = "wasm32")]
pub fn arm(_dir: std::path::PathBuf) {}
#[cfg(target_arch = "wasm32")]
pub fn shadow_push(seq: u64, line: &str) {
    crate::diagnostics::crash_log::push_tail_line(seq, line);
}
/// On wasm the 1 Hz metric snapshot is part of the downloadable log (the ring
/// IS the capture there — see `SessionLog::record_file_only`), so unlike
/// native it cannot go to an overwrite slot: the vitals SERIES is the OOM
/// post-mortem, and one sample of it shows the crash but not the climb.
///
/// It goes into the tail, in a budget of its own, so it can neither be
/// reduced to a single sample nor evict the real events beside it (#1180).
#[cfg(target_arch = "wasm32")]
pub fn shadow_push_snapshot(seq: u64, line: &str) {
    crate::diagnostics::crash_log::push_tail_snapshot(seq, line);
}
#[cfg(target_arch = "wasm32")]
pub fn install_hook() {
    crate::diagnostics::crash_log::install_terminal_hooks();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #1142. A hook cannot read the clock, so the marker used to go out at
    /// 0.0 — which the analyzer reads as "the session ended before its first
    /// event". The stamp now comes from the last event the capture holds.
    #[test]
    fn a_terminal_marker_is_stamped_at_the_last_event_the_capture_holds() {
        note_timestamp(41.5);
        assert_eq!(marker_ts(), 41.5);
        // A clock that never advanced, or a log with no events at all, still
        // yields something a duration can be taken from.
        note_timestamp(f64::NAN);
        assert_eq!(
            marker_ts(),
            41.5,
            "a non-finite reading must not overwrite a good stamp"
        );
    }

    #[test]
    fn panic_reason_with_and_without_location() {
        assert_eq!(
            format_panic_reason("boom", Some(("src/x.rs", 42))),
            "panic at src/x.rs:42: boom"
        );
        assert_eq!(format_panic_reason("boom", None), "panic: boom");
    }
}

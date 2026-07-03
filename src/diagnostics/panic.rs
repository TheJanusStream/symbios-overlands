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
/// Pulled out as a pure fn so it can be unit-tested without a real panic.
/// (Native-only: the wasm target has no file-sink panic snapshot to format for.)
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn format_panic_reason(msg: &str, location: Option<(&str, u32)>) -> String {
    match location {
        Some((file, line)) => format!("panic at {file}:{line}: {msg}"),
        None => format!("panic: {msg}"),
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use std::collections::VecDeque;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Mutex, OnceLock};

    use crate::diagnostics::event::{EventPayload, SessionEvent, Severity};
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
    pub fn shadow_push(line: &str) {
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
    pub fn shadow_push_snapshot(line: &str) {
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
        // Final synthetic marker. seq = u64::MAX is the crash sentinel (the
        // real sequence isn't reachable from the hook).
        let msg = info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str))
            .unwrap_or("panic");
        let reason = super::format_panic_reason(msg, info.location().map(|l| (l.file(), l.line())));
        let ev = SessionEvent::new(
            u64::MAX,
            0.0,
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
            arm(std::path::PathBuf::from("."));
            shadow_push("r633-real-1");
            shadow_push_snapshot("r633-snap-1");
            shadow_push("r633-real-2");
            shadow_push_snapshot("r633-snap-2");

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

// Wasm has no filesystem and its panics already route to the console via
// `console_error_panic_hook` (installed in `run()`), so these are no-ops.
#[cfg(target_arch = "wasm32")]
pub fn arm(_dir: std::path::PathBuf) {}
#[cfg(target_arch = "wasm32")]
pub fn shadow_push(_line: &str) {}
#[cfg(target_arch = "wasm32")]
pub fn shadow_push_snapshot(_line: &str) {}
#[cfg(target_arch = "wasm32")]
pub fn install_hook() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panic_reason_with_and_without_location() {
        assert_eq!(
            format_panic_reason("boom", Some(("src/x.rs", 42))),
            "panic at src/x.rs:42: boom"
        );
        assert_eq!(format_panic_reason("boom", None), "panic: boom");
    }
}

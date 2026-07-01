//! The durable sink for the session-event stream (Pillar A-3).
//!
//! Native builds append one NDJSON line per
//! [`SessionEvent`](crate::diagnostics::event::SessionEvent) to a per-session
//! file under `diagnostics/` (repo-ignored, and unlike `target/` it survives
//! `cargo clean`), and refresh a stable `session-latest.jsonl` copy on every
//! flush so a coding agent can always read the newest run without knowing its
//! timestamp. The wasm build has no filesystem, so [`Sink`] collapses to
//! [`Sink::Disabled`] there and the in-memory ring plus the "Download log"
//! button (A-8) are the whole story.
//!
//! The module is `#[cfg]`-split so the wasm build never links `std::fs`.

/// Where recorded events are durably written. On wasm only [`Sink::Disabled`]
/// exists; on native it is normally [`Sink::Native`] unless persistence was
/// turned off (via `SYMBIOS_DIAG=0`) or the file could not be opened.
pub enum Sink {
    /// In-memory only — the ring buffer is the whole log (tests, wasm, or
    /// `SYMBIOS_DIAG=0`).
    Disabled,
    #[cfg(not(target_arch = "wasm32"))]
    Native(NativeSink),
}

impl Sink {
    /// A no-op sink (the default until one is attached).
    pub fn disabled() -> Self {
        Sink::Disabled
    }

    /// The directory the native sink writes to, honouring `SYMBIOS_DIAG` (set
    /// to `0` to disable → `None`) and `SYMBIOS_DIAG_DIR` (override), else the
    /// repo-root `diagnostics/` default. Shared with the panic module so the
    /// crash file lands beside the session log.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn resolve_dir() -> Option<std::path::PathBuf> {
        use crate::config::diagnostics as cfg;
        if std::env::var(cfg::DISABLE_ENV).as_deref() == Ok("0") {
            return None;
        }
        Some(
            std::env::var(cfg::DIR_ENV)
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|_| std::path::PathBuf::from(cfg::DEFAULT_DIR)),
        )
    }

    /// Open the native session file honouring the env vars above, keyed to
    /// `session_did` if known. Returns [`Sink::Disabled`] on wasm, when
    /// disabled, or if the file can't be created (best-effort — persistence
    /// must never take the app down).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn open(session_did: Option<&str>) -> Self {
        match Self::resolve_dir() {
            Some(dir) => Self::open_in(&dir, session_did),
            None => Sink::Disabled,
        }
    }

    /// Open the native session file in an explicit directory (used by tests to
    /// avoid touching the process-wide env). Falls back to [`Sink::Disabled`]
    /// with a one-line warning if the directory or file cannot be created.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn open_in(dir: &std::path::Path, session_did: Option<&str>) -> Self {
        match NativeSink::open_in(dir, session_did) {
            Ok(s) => Sink::Native(s),
            Err(e) => {
                eprintln!("diagnostics: session log persistence disabled ({e})");
                Sink::Disabled
            }
        }
    }

    /// Wasm has no filesystem — persistence is always the ring buffer.
    #[cfg(target_arch = "wasm32")]
    pub fn open(_session_did: Option<&str>) -> Self {
        Sink::Disabled
    }

    /// Append one already-serialized JSON line (no trailing newline).
    /// Best-effort: a write error is swallowed rather than propagated so a full
    /// disk never crashes the game.
    pub fn append_line(&mut self, line: &str) {
        match self {
            Sink::Disabled => {}
            #[cfg(not(target_arch = "wasm32"))]
            Sink::Native(s) => s.append_line(line),
        }
    }

    /// Flush buffered writes to disk and refresh `session-latest.jsonl`.
    pub fn flush(&mut self) {
        match self {
            Sink::Disabled => {}
            #[cfg(not(target_arch = "wasm32"))]
            Sink::Native(s) => s.flush(),
        }
    }

    /// The stable `session-latest.jsonl` path, for the native "copy path"
    /// affordance in the Diagnostics panel (A-8). `None` when disabled/wasm.
    pub fn latest_path_display(&self) -> Option<String> {
        match self {
            Sink::Disabled => None,
            #[cfg(not(target_arch = "wasm32"))]
            Sink::Native(s) => Some(s.latest_path.display().to_string()),
        }
    }
}

/// Native NDJSON writer over a per-session file plus a `session-latest.jsonl`
/// copy refreshed on flush.
#[cfg(not(target_arch = "wasm32"))]
pub struct NativeSink {
    writer: std::io::BufWriter<std::fs::File>,
    session_path: std::path::PathBuf,
    latest_path: std::path::PathBuf,
}

#[cfg(not(target_arch = "wasm32"))]
impl NativeSink {
    fn open_in(dir: &std::path::Path, session_did: Option<&str>) -> std::io::Result<Self> {
        use std::io::Write;
        std::fs::create_dir_all(dir)?;
        let start = super::log::wall_now_ms().unwrap_or(0);
        // Keep the DID (when known) in the filename for quick eyeballing; the
        // authoritative DID also lives in the StartupSnapshot record inside.
        let name = match session_did {
            Some(did) => format!("session-{start}-{}.jsonl", slug(did)),
            None => format!("session-{start}.jsonl"),
        };
        let session_path = dir.join(name);
        let latest_path = dir.join(crate::config::diagnostics::LATEST_FILENAME);
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&session_path)?;
        let mut writer = std::io::BufWriter::new(file);
        writer.flush()?; // touch the file so it exists even before the first event
        Ok(NativeSink {
            writer,
            session_path,
            latest_path,
        })
    }

    fn append_line(&mut self, line: &str) {
        use std::io::Write;
        let _ = self.writer.write_all(line.as_bytes());
        let _ = self.writer.write_all(b"\n");
    }

    fn flush(&mut self) {
        use std::io::Write;
        if self.writer.flush().is_ok() {
            // Refresh the stable "latest" copy from the just-flushed bytes.
            let _ = std::fs::copy(&self.session_path, &self.latest_path);
        }
    }
}

/// Make a DID safe to embed in a filename (drop path separators / colons).
#[cfg(not(target_arch = "wasm32"))]
fn slug(did: &str) -> String {
    did.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::diagnostics::event::{EventPayload, SessionEvent, Severity};
    use std::sync::atomic::{AtomicU64, Ordering};

    static N: AtomicU64 = AtomicU64::new(0);

    /// A unique temp dir per test invocation, cleaned up on drop.
    struct TempDir(std::path::PathBuf);
    impl TempDir {
        fn new() -> Self {
            let n = N.fetch_add(1, Ordering::Relaxed);
            let p = std::env::temp_dir().join(format!("symbios-diag-{}-{n}", std::process::id()));
            let _ = std::fs::remove_dir_all(&p);
            TempDir(p)
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn line(seq: u64) -> String {
        let ev = SessionEvent::new(
            seq,
            seq as f64,
            Some(1000 + seq),
            Severity::Info,
            EventPayload::SessionEnd {
                reason: format!("e{seq}"),
            },
        );
        serde_json::to_string(&ev).unwrap()
    }

    #[test]
    fn native_sink_writes_session_and_latest_that_parse_back() {
        let dir = TempDir::new();
        let mut sink = Sink::open_in(&dir.0, Some("did:plc:abc"));
        assert!(matches!(sink, Sink::Native(_)), "should open natively");
        for i in 0..3 {
            sink.append_line(&line(i));
        }
        sink.flush();

        // The per-session file carries the slugged DID.
        let session_file = std::fs::read_dir(&dir.0)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .find(|n| n.starts_with("session-") && n.contains("did_plc_abc"))
            .expect("per-session file exists");
        assert!(session_file.ends_with(".jsonl"));

        // Both the per-session file and the stable latest copy parse cleanly.
        let latest = dir.0.join(crate::config::diagnostics::LATEST_FILENAME);
        let body = std::fs::read_to_string(&latest).expect("latest copied on flush");
        let parsed: Vec<SessionEvent> = body
            .lines()
            .map(|l| serde_json::from_str(l).expect("each latest line parses"))
            .collect();
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[2].seq, 2);
    }

    #[test]
    fn durable_file_keeps_lines_the_ring_dropped() {
        use crate::diagnostics::log::SessionLog;
        let dir = TempDir::new();
        // Ring holds only 2 events, but all 5 must reach the durable file.
        let mut log = SessionLog::with_capacity(2);
        log.set_sink(Sink::open_in(&dir.0, None));
        for i in 0..5 {
            log.info(
                i as f64,
                EventPayload::SessionEnd {
                    reason: format!("e{i}"),
                },
            );
        }
        assert_eq!(log.len(), 2, "in-memory ring is bounded");
        assert_eq!(log.pending_since_flush(), 5);
        log.flush();
        assert_eq!(log.pending_since_flush(), 0);

        let latest = dir.0.join(crate::config::diagnostics::LATEST_FILENAME);
        let body = std::fs::read_to_string(&latest).expect("latest written");
        let seqs: Vec<u64> = body
            .lines()
            .map(|l| serde_json::from_str::<SessionEvent>(l).unwrap().seq)
            .collect();
        assert_eq!(seqs, vec![0, 1, 2, 3, 4], "all durable, none dropped");
    }

    #[test]
    fn file_only_writes_to_disk_but_not_the_ring() {
        use crate::diagnostics::log::SessionLog;
        let dir = TempDir::new();
        let mut log = SessionLog::with_capacity(8);
        log.set_sink(Sink::open_in(&dir.0, None));
        log.record_file_only(
            0.0,
            Severity::Trace,
            EventPayload::SessionEnd {
                reason: "snap".into(),
            },
        );
        assert_eq!(
            log.len(),
            0,
            "file-only telemetry stays out of the GUI ring"
        );
        log.flush();
        let latest = dir.0.join(crate::config::diagnostics::LATEST_FILENAME);
        let body = std::fs::read_to_string(&latest).expect("written to disk");
        assert_eq!(body.lines().count(), 1, "but it is durably persisted");
    }

    #[test]
    fn disable_env_value_yields_disabled_sink() {
        // open_in bypasses env; the disabled path is exercised via disabled().
        let mut sink = Sink::disabled();
        sink.append_line(&line(0)); // no-op, must not panic
        sink.flush();
        assert!(sink.latest_path_display().is_none());
    }
}

//! NDJSON → [`ParsedLog`]: line-tolerant parsing plus the small
//! whole-log accessors (startup snapshot, session end, severity tally,
//! duration) every report section reads.

use crate::diagnostics::event::{EventPayload, SessionEvent, Severity, StartupInfo};

/// A session log parsed from NDJSON, plus the count of lines that failed to
/// deserialize (an unknown/renamed variant from a newer build, or a torn final
/// line from a crash) — surfaced in the report so a truncated log is never
/// silently analyzed as if it were complete.
pub struct ParsedLog {
    pub events: Vec<SessionEvent>,
    pub unparseable: usize,
}

/// Parse an NDJSON session log line-by-line. Each line is deserialized
/// independently, so one bad line (an unknown `kind` from a newer schema, or a
/// half-written tail line after a crash) is skipped and counted rather than
/// aborting the whole analysis. Blank lines are ignored (not counted).
pub fn parse_ndjson(text: &str) -> ParsedLog {
    let mut events = Vec::new();
    let mut unparseable = 0;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<SessionEvent>(line) {
            Ok(ev) => events.push(ev),
            Err(_) => unparseable += 1,
        }
    }
    ParsedLog {
        events,
        unparseable,
    }
}

/// The most informative startup snapshot: prefer a `Session`-phase record (its
/// `session_did` / relay are filled in), else fall back to the first `Boot`
/// snapshot (build info is identical, only the DID differs).
pub(super) fn startup(events: &[SessionEvent]) -> Option<&StartupInfo> {
    let mut boot = None;
    for e in events {
        if let EventPayload::StartupSnapshot(info) = &e.payload {
            if info.session_did.is_some() {
                return Some(info);
            }
            boot.get_or_insert(info.as_ref());
        }
    }
    boot
}

/// How a session ended, as far as the capture can tell (#1142, #1145).
///
/// Three outcomes, and the difference between them is the whole reason the
/// terminal record exists. A `SessionEnd` the app recorded means the process
/// ran its own teardown. A marker means a shutdown hook wrote the last thing
/// anyone will know about the run. And no record at all — [`session_end`]'s
/// `None` — means neither got to run, which on wasm is the OOM trap the crash
/// tail was built for and is the case worth escalating.
pub(super) enum Exit<'a> {
    /// The running app recorded it.
    Clean(&'a str),
    /// A shutdown hook wrote it; `crashed` distinguishes the panic hook from
    /// a clean tab close.
    Hook { reason: &'a str, crashed: bool },
}

/// The last `SessionEnd` record and who wrote it. `None` means the capture
/// carries no terminal record at all.
pub(super) fn session_end(events: &[SessionEvent]) -> Option<Exit<'_>> {
    events.iter().rev().find_map(|e| match &e.payload {
        EventPayload::SessionEnd { reason } if e.is_hook_marker() => Some(Exit::Hook {
            reason: reason.as_str(),
            crashed: e.is_crash_marker(),
        }),
        EventPayload::SessionEnd { reason } => Some(Exit::Clean(reason.as_str())),
        _ => None,
    })
}

/// Count events at `Warn` / `Error` / `Critical` — the top-line health signal.
pub(super) fn severity_tally(events: &[SessionEvent]) -> [usize; 3] {
    let mut t = [0usize; 3];
    for e in events {
        match e.severity {
            Severity::Warn => t[0] += 1,
            Severity::Error => t[1] += 1,
            Severity::Critical => t[2] += 1,
            _ => {}
        }
    }
    t
}

/// Session wall-clock span (session-relative, so the first event is ~0 and
/// this is effectively the session length).
///
/// Widest minus narrowest rather than last minus first: a panic file's crash
/// marker is appended last and older builds stamped it `0.0`, which made this
/// report a NEGATIVE duration in the header (#1142). See
/// [`crate::diagnostics::event::span_secs`].
pub(super) fn duration_secs(events: &[SessionEvent]) -> f64 {
    crate::diagnostics::event::span_secs(events)
}

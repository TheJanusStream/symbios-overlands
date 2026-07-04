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

/// The reason of the last `SessionEnd` record, if the session ended cleanly.
pub(super) fn session_end(events: &[SessionEvent]) -> Option<&str> {
    events.iter().rev().find_map(|e| match &e.payload {
        EventPayload::SessionEnd { reason } => Some(reason.as_str()),
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

/// Session wall-clock span: last minus first `t_mono_secs` (session-relative,
/// so the first event is ~0 and this is effectively the session length).
pub(super) fn duration_secs(events: &[SessionEvent]) -> f64 {
    match (events.first(), events.last()) {
        (Some(a), Some(b)) => b.t_mono_secs - a.t_mono_secs,
        _ => 0.0,
    }
}

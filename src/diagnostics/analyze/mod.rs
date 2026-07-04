//! Offline session-log analyzer (Pillar B) — the agent-facing post-mortem the
//! native `render` bin prints for `--analyze-session <log.jsonl>`.
//!
//! It reads a captured NDJSON session log (the durable
//! `diagnostics/session-latest.jsonl` a live run writes, or the wasm
//! "Download log" dump — byte-identical formats), deserializes it back into
//! [`SessionEvent`]s tolerating unknown/torn lines, and folds it into a
//! human-readable report: the header + verdict (B-1), the per-subsystem
//! `[Event Tallies]` + `[Metric Trends]` (B-3), the `[Timeline]` +
//! `[Loading Gate]` per-stage timings (B-2), and the replayed
//! `[Invariant Violations]` section (D-5). [`diff_report`] adds the
//! `--diff-sessions <a> <b>` before/after comparison (B-4), and [`Filters`] /
//! [`report_with`] add the `--analyze-session` subsystem / category / severity /
//! time-window filters (B-5).
//!
//! The report builders are pure over `&[SessionEvent]`, so they unit-test
//! without any file IO or a Bevy `App`; the native `render_tool` supplies the
//! file read. The report format follows the `urban/diagnostics.rs` road-dump
//! idiom (`=== header ===` + labelled sections).

use std::fmt::Write;

use crate::diagnostics::event::SessionEvent;

mod diff;
mod filters;
mod parse;
mod sections;
#[cfg(test)]
mod tests;

pub use diff::diff_report;
pub use filters::Filters;
pub use parse::{ParsedLog, parse_ndjson};

use parse::*;
use sections::*;

/// Round to one decimal, so a delta is taken between the *displayed* (rounded)
/// operands and the shown `A → B  (Δ)` always adds up (no `0.6 → 0.3  (-0.2)`
/// where the rounded operands imply `-0.3`).
pub(super) fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

pub(super) fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

/// Build the post-mortem report for a parsed session log. `path` is echoed in
/// the header so a multi-log analysis is self-labelling. Pure over its inputs.
pub fn report(path: &str, log: &ParsedLog) -> String {
    report_with(path, log, &Filters::default())
}

/// Like [`report`], but restricts the analysis sections to the events matching
/// `filters` (B-5). The header (session id, build, duration, exit) is always
/// derived from the *full* log — it identifies the run — while `[Verdict]` …
/// `[Invariant Violations]` fold only the matching subset, with a `[Filter]` line
/// documenting the lens and how many events matched. Pure over its inputs.
pub fn report_with(path: &str, log: &ParsedLog, filters: &Filters) -> String {
    let full = &log.events;
    let mut s = String::new();
    let _ = writeln!(s, "=== session analysis — {path} ===");

    if full.is_empty() {
        let _ = writeln!(
            s,
            "empty log: no parseable events ({} unparseable line(s))",
            log.unparseable
        );
        return s;
    }

    // -- header (always the FULL log — it identifies the session) -------------
    let start = startup(full);
    // The session id mirrors `SessionLog::session_start_wall_ms` — the wall
    // stamp of the *first* recorded event, which keys the per-session file. If
    // the first event carried no clock, there is no session id (`—`); we don't
    // borrow a later event's stamp, which would misidentify the run.
    let session_id = full
        .first()
        .and_then(|e| e.wall_ms)
        .map(|ms| ms.to_string())
        .unwrap_or_else(|| "—".to_string());
    let did = start
        .and_then(|s| s.session_did.clone().or_else(|| s.boot_target_did.clone()))
        .unwrap_or_else(|| "—".to_string());
    let _ = writeln!(s, "session-id: {session_id}    did: {did}");

    match start {
        Some(info) => {
            let _ = writeln!(
                s,
                "build:      v{} ({}) {}/{}{}",
                info.version,
                info.git_sha,
                info.target_arch,
                info.profile,
                if info.wasm { " wasm" } else { "" }
            );
        }
        None => {
            let _ = writeln!(s, "build:      — (no StartupSnapshot record)");
        }
    }

    let unparseable = if log.unparseable > 0 {
        format!(", {} unparseable", log.unparseable)
    } else {
        String::new()
    };
    let _ = writeln!(
        s,
        "duration:   {:.1}s   ({} events{})",
        duration_secs(full),
        full.len(),
        unparseable
    );

    match session_end(full) {
        Some(reason) => {
            let _ = writeln!(s, "exit:       {reason}");
        }
        None => {
            let _ = writeln!(
                s,
                "exit:       — no SessionEnd record (crash or truncated log)"
            );
        }
    }

    // -- filter (B-5): the analysis sections fold only the matching subset -----
    let filtered: Vec<SessionEvent> = if filters.is_active() {
        full.iter()
            .filter(|e| filters.matches(e))
            .cloned()
            .collect()
    } else {
        Vec::new() // unused when inactive; avoids cloning the whole log
    };
    let events: &[SessionEvent] = if filters.is_active() { &filtered } else { full };
    if filters.is_active() {
        let _ = writeln!(s);
        let _ = writeln!(
            s,
            "[Filter]  {}  —  {} of {} events match",
            filters.describe(),
            events.len(),
            full.len()
        );
        if events.is_empty() {
            let _ = writeln!(s, "  (no events match — nothing to analyze)");
            return s;
        }
    }

    write_sections(&mut s, events);
    s
}

//! Offline session-log analyzer (Pillar B) — the agent-facing post-mortem the
//! native `render` bin prints for `--analyze-session <log.jsonl>`.
//!
//! It reads a captured NDJSON session log (the durable
//! `diagnostics/session-latest.jsonl` a live run writes, or the wasm
//! "Download log" dump — byte-identical formats), deserializes it back into
//! [`SessionEvent`]s tolerating unknown/torn lines, and folds it into a
//! human-readable report: the header + verdict (B-1), the `[Timeline]` +
//! `[Loading Gate]` per-stage timings (B-2), and the replayed
//! `[Invariant Violations]` section (D-5). Later slices add the per-subsystem
//! tallies (B-3) and the analyzer filters (B-5).
//!
//! The report builders are pure over `&[SessionEvent]`, so they unit-test
//! without any file IO or a Bevy `App`; the native `render_tool` supplies the
//! file read. The report format follows the `urban/diagnostics.rs` road-dump
//! idiom (`=== header ===` + labelled sections).

use std::fmt::Write;

use crate::diagnostics::event::{EventPayload, FetchStatus, SessionEvent, Severity, StartupInfo};

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
fn startup(events: &[SessionEvent]) -> Option<&StartupInfo> {
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
fn session_end(events: &[SessionEvent]) -> Option<&str> {
    events.iter().rev().find_map(|e| match &e.payload {
        EventPayload::SessionEnd { reason } => Some(reason.as_str()),
        _ => None,
    })
}

/// Count events at `Warn` / `Error` / `Critical` — the top-line health signal.
fn severity_tally(events: &[SessionEvent]) -> [usize; 3] {
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
fn duration_secs(events: &[SessionEvent]) -> f64 {
    match (events.first(), events.last()) {
        (Some(a), Some(b)) => b.t_mono_secs - a.t_mono_secs,
        _ => 0.0,
    }
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

/// Cap on `[Timeline]` rows so a pathological log (thousands of portal hops)
/// can't grow the report without bound; the overflow is summarised.
const TIMELINE_MAX: usize = 60;

/// A short label for the events that define the session's *shape* — the ones the
/// `[Timeline]` renders at their timestamp. `None` for the high-frequency /
/// detail events (metric snapshots, per-peer transforms, chat, …) that would
/// bury the milestones.
fn timeline_label(p: &EventPayload) -> Option<String> {
    use EventPayload::*;
    Some(match p {
        StartupSnapshot(s) => format!("startup ({:?})", s.phase),
        LoadingPhaseStarted => "loading gate opened".to_string(),
        RecordFetchCompleted { record, status, .. } => format!("{record:?} fetch {status:?}"),
        HeightmapGenCompleted { .. } => "heightmap generated".to_string(),
        AmbientBakeCompleted { .. } => "ambient bake done".to_string(),
        AmbientBakeFallback { .. } => "ambient bake fell back to silence".to_string(),
        WorldCompileCompleted { entity_count, .. } => {
            format!("world compiled ({entity_count} entities)")
        }
        LoadingGateTransitionToInGame { elapsed_secs } => format!("→ InGame ({elapsed_secs:.1}s)"),
        PortalTravelInitiated { target_did } => format!("portal → {target_did}"),
        PortalTravelCompleted { target_did } => format!("portal arrived {target_did}"),
        PortalTravelFailed { target_did, .. } => format!("portal → {target_did} FAILED"),
        SessionSegmentReset { reason } => format!("segment reset ({reason})"),
        SessionEnd { reason } => format!("session end ({reason})"),
        _ => return None,
    })
}

/// The `[Timeline]` section: the milestone events (see [`timeline_label`]) at
/// their session-relative timestamps, so an agent can see the run's arc at a
/// glance. Capped at [`TIMELINE_MAX`] with an explicit overflow line — never a
/// silent truncation.
fn write_timeline(s: &mut String, events: &[SessionEvent]) {
    let rows: Vec<(f64, String)> = events
        .iter()
        .filter_map(|e| timeline_label(&e.payload).map(|l| (e.t_mono_secs, l)))
        .collect();
    let _ = writeln!(s);
    let _ = writeln!(s, "[Timeline]");
    if rows.is_empty() {
        let _ = writeln!(s, "  (no milestone events)");
        return;
    }
    let shown = rows.len().min(TIMELINE_MAX);
    for (t, label) in rows.iter().take(shown) {
        let _ = writeln!(s, "  {t:>8.1}s  {label}");
    }
    if rows.len() > shown {
        let _ = writeln!(s, "  … {} more milestone(s)", rows.len() - shown);
    }
}

/// One stage-timing line: the `min/p50/p90/max/mean` distro of every
/// `duration_secs` `pick` extracts, via the shared [`distro`] reducer, or `—`
/// when the stage never ran in this log.
///
/// [`distro`]: crate::diagnostics::registry::distro
fn write_stage_distro(
    s: &mut String,
    label: &str,
    events: &[SessionEvent],
    pick: fn(&EventPayload) -> Option<f64>,
) {
    let v: Vec<f64> = events.iter().filter_map(|e| pick(&e.payload)).collect();
    match crate::diagnostics::registry::distro(&v) {
        Some(d) => {
            let _ = writeln!(s, "  {label:<14} {d}  (n={})", d.n);
        }
        None => {
            let _ = writeln!(s, "  {label:<14} —");
        }
    }
}

/// The `[Loading Gate]` section: the Login → Loading → InGame gate time plus a
/// per-stage duration distro for the four heavy loading stages the session log
/// times (record fetch / heightmap / ambient bake / world compile).
fn write_loading_gate(s: &mut String, events: &[SessionEvent]) {
    use EventPayload::*;
    let _ = writeln!(s);
    let _ = writeln!(s, "[Loading Gate]");

    // The gate elapsed is stamped on the Loading → InGame transition event.
    let gate = events.iter().find_map(|e| match &e.payload {
        LoadingGateTransitionToInGame { elapsed_secs } => Some(*elapsed_secs),
        _ => None,
    });
    match gate {
        Some(secs) => {
            let _ = writeln!(s, "  Loading → InGame:  {secs:.1}s");
        }
        None => {
            let started = events
                .iter()
                .any(|e| matches!(e.payload, LoadingPhaseStarted));
            let why = if started {
                "did not reach InGame (stalled or truncated log)"
            } else {
                "no loading gate in this log"
            };
            let _ = writeln!(s, "  Loading → InGame:  — ({why})");
        }
    }

    write_stage_distro(s, "record fetch", events, |p| match p {
        // Success-only, to match the other three stages: a decode-failure /
        // exhausted / best-effort fetch also emits a `RecordFetchCompleted`, but
        // its (often near-timeout) latency would skew a "how long did fetches
        // take" distro — the failure surfaces in the verdict + invariants instead.
        RecordFetchCompleted {
            duration_secs,
            status: FetchStatus::Ok | FetchStatus::NotFound,
            ..
        } => Some(*duration_secs),
        _ => None,
    });
    write_stage_distro(s, "heightmap", events, |p| match p {
        HeightmapGenCompleted { duration_secs, .. } => Some(*duration_secs),
        _ => None,
    });
    write_stage_distro(s, "ambient bake", events, |p| match p {
        AmbientBakeCompleted { duration_secs, .. } => Some(*duration_secs),
        _ => None,
    });
    write_stage_distro(s, "world compile", events, |p| match p {
        WorldCompileCompleted { duration_secs, .. } => Some(*duration_secs),
        _ => None,
    });
}

/// Build the post-mortem report for a parsed session log. `path` is echoed in
/// the header so a multi-log analysis is self-labelling. Pure over its inputs.
pub fn report(path: &str, log: &ParsedLog) -> String {
    let events = &log.events;
    let mut s = String::new();
    let _ = writeln!(s, "=== session analysis — {path} ===");

    if events.is_empty() {
        let _ = writeln!(
            s,
            "empty log: no parseable events ({} unparseable line(s))",
            log.unparseable
        );
        return s;
    }

    // -- header ---------------------------------------------------------------
    let start = startup(events);
    // The session id mirrors `SessionLog::session_start_wall_ms` — the wall
    // stamp of the *first* recorded event, which keys the per-session file. If
    // the first event carried no clock, there is no session id (`—`); we don't
    // borrow a later event's stamp, which would misidentify the run.
    let session_id = events
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
        duration_secs(events),
        events.len(),
        unparseable
    );

    match session_end(events) {
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

    // -- verdict --------------------------------------------------------------
    let [warn, error, crit] = severity_tally(events);
    let _ = writeln!(s);
    let _ = writeln!(s, "[Verdict]");
    if warn + error + crit == 0 {
        let _ = writeln!(s, "  HEALTHY — no warnings, errors or critical events");
    } else {
        let mut parts = Vec::new();
        if crit > 0 {
            parts.push(format!("{crit} critical"));
        }
        if error > 0 {
            parts.push(format!("{error} error{}", plural(error)));
        }
        if warn > 0 {
            parts.push(format!("{warn} warning{}", plural(warn)));
        }
        let _ = writeln!(s, "  {}", parts.join(", "));
    }

    // -- timeline + loading-gate stage timing (B-2) ---------------------------
    write_timeline(&mut s, events);
    write_loading_gate(&mut s, events);

    // -- invariant violations (D-5) -------------------------------------------
    // The offline counterpart to the live anomaly engine: replay the shared rule
    // set over the stream + surface captured live-only fires.
    let _ = writeln!(s);
    let _ = write!(
        s,
        "{}",
        crate::diagnostics::anomaly::replay::replay_invariants(events)
    );

    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::event::{FetchStatus, RecordKind, SnapshotPhase, StartupInfo};

    fn ev(t: f64, sev: Severity, payload: EventPayload) -> SessionEvent {
        SessionEvent::new(0, t, Some(1_700_000_000_000), sev, payload)
    }

    fn startup_info(session_did: Option<&str>) -> EventPayload {
        EventPayload::StartupSnapshot(Box::new(StartupInfo {
            phase: if session_did.is_some() {
                SnapshotPhase::Session
            } else {
                SnapshotPhase::Boot
            },
            version: "0.1.0".into(),
            git_sha: "deadbee".into(),
            target_arch: "x86_64".into(),
            profile: "debug".into(),
            wasm: false,
            boot_target_did: None,
            boot_pos: None,
            boot_yaw_deg: None,
            pds: None,
            relay: None,
            session_did: session_did.map(str::to_string),
        }))
    }

    /// A well-formed NDJSON log serialized from real events round-trips, and
    /// blank lines + one unknown-variant line are tolerated (skipped/counted).
    #[test]
    fn parse_ndjson_tolerates_blank_and_unknown_lines() {
        let events = vec![
            ev(0.0, Severity::Info, startup_info(Some("did:plc:me"))),
            ev(1.0, Severity::Info, EventPayload::LoadingPhaseStarted),
            ev(
                2.0,
                Severity::Info,
                EventPayload::SessionEnd {
                    reason: "app_exit".into(),
                },
            ),
        ];
        let mut text = String::new();
        for e in &events {
            text.push_str(&serde_json::to_string(e).unwrap());
            text.push('\n');
        }
        // A blank line (ignored) and a future-schema line (counted).
        text.push('\n');
        text.push_str("{\"seq\":9,\"t_mono_secs\":3.0,\"wall_ms\":null,\"subsystem\":\"Runtime\",\"category\":\"Perf\",\"severity\":\"Info\",\"payload\":{\"kind\":\"SomeFutureVariant\"}}\n");

        let parsed = parse_ndjson(&text);
        assert_eq!(parsed.events.len(), 3, "3 good events, blank line skipped");
        assert_eq!(parsed.unparseable, 1, "unknown variant counted, not fatal");
    }

    #[test]
    fn report_healthy_session_has_header_and_healthy_verdict() {
        let parsed = ParsedLog {
            events: vec![
                ev(0.0, Severity::Info, startup_info(Some("did:plc:me"))),
                ev(1.5, Severity::Info, EventPayload::LoadingPhaseStarted),
                ev(
                    120.0,
                    Severity::Info,
                    EventPayload::SessionEnd {
                        reason: "app_exit".into(),
                    },
                ),
            ],
            unparseable: 0,
        };
        let r = report("session-latest.jsonl", &parsed);
        assert!(r.contains("=== session analysis — session-latest.jsonl ==="));
        assert!(r.contains("did: did:plc:me"));
        assert!(r.contains("v0.1.0 (deadbee) x86_64/debug"));
        assert!(r.contains("120.0s   (3 events)"));
        assert!(r.contains("exit:       app_exit"));
        assert!(r.contains("HEALTHY"));
    }

    #[test]
    fn report_tallies_severities_and_flags_missing_exit() {
        let parsed = ParsedLog {
            events: vec![
                ev(0.0, Severity::Info, startup_info(Some("did:plc:me"))),
                ev(
                    1.0,
                    Severity::Warn,
                    EventPayload::LoadingGateWarning {
                        stage: "heightmap".into(),
                        message: "slow".into(),
                    },
                ),
                ev(
                    2.0,
                    Severity::Critical,
                    EventPayload::InvariantViolation {
                        rule: "loading.gate_stall".into(),
                        detail: "200s".into(),
                    },
                ),
                // No SessionEnd → torn/crashed log.
            ],
            unparseable: 2,
        };
        let r = report("crash.jsonl", &parsed);
        assert!(r.contains("1 critical, 1 warning"), "verdict tally: {r}");
        assert!(r.contains("no SessionEnd record"));
        assert!(r.contains("2 unparseable"));
    }

    #[test]
    fn session_id_is_the_first_events_wall_stamp_not_a_later_one() {
        // The first event has no wall clock; a later one does. The session id
        // must stay `—` (it mirrors `SessionLog::session_start_wall_ms`), not
        // borrow the later stamp and misidentify the run.
        let parsed = ParsedLog {
            events: vec![
                SessionEvent::new(
                    0,
                    0.0,
                    None,
                    Severity::Info,
                    startup_info(Some("did:plc:me")),
                ),
                SessionEvent::new(
                    1,
                    1.0,
                    Some(1_700_000_000_000),
                    Severity::Info,
                    EventPayload::LoadingPhaseStarted,
                ),
            ],
            unparseable: 0,
        };
        let r = report("x.jsonl", &parsed);
        assert!(
            r.contains("session-id: —"),
            "session-id must be — when the first event has no wall_ms: {r}"
        );
    }

    #[test]
    fn report_empty_log_is_graceful() {
        let parsed = ParsedLog {
            events: Vec::new(),
            unparseable: 0,
        };
        let r = report("empty.jsonl", &parsed);
        assert!(r.contains("empty log: no parseable events"));
    }

    #[test]
    fn report_renders_timeline_and_loading_gate_stage_timings() {
        use EventPayload::*;
        let did = "did:plc:me".to_string();
        let parsed = ParsedLog {
            events: vec![
                ev(0.0, Severity::Info, startup_info(Some("did:plc:me"))),
                ev(0.5, Severity::Info, LoadingPhaseStarted),
                ev(
                    1.0,
                    Severity::Info,
                    RecordFetchCompleted {
                        record: RecordKind::Room,
                        did: did.clone(),
                        status: FetchStatus::Ok,
                        duration_secs: 0.8,
                    },
                ),
                ev(
                    1.2,
                    Severity::Info,
                    RecordFetchCompleted {
                        record: RecordKind::Avatar,
                        did: did.clone(),
                        status: FetchStatus::NotFound,
                        duration_secs: 0.4,
                    },
                ),
                // A FAILED fetch: shows in the timeline but must NOT pollute the
                // success-only record-fetch stage distro (its ~timeout latency
                // would skew the percentiles).
                ev(
                    1.5,
                    Severity::Warn,
                    RecordFetchCompleted {
                        record: RecordKind::Inventory,
                        did: did.clone(),
                        status: FetchStatus::Exhausted,
                        duration_secs: 30.0,
                    },
                ),
                ev(
                    2.0,
                    Severity::Info,
                    HeightmapGenCompleted {
                        duration_secs: 1.5,
                        width: 256,
                        height: 256,
                    },
                ),
                ev(
                    2.5,
                    Severity::Info,
                    AmbientBakeCompleted {
                        bytes: 44_100,
                        duration_secs: 0.3,
                    },
                ),
                ev(
                    3.0,
                    Severity::Info,
                    WorldCompileCompleted {
                        entity_count: 1200,
                        duration_secs: 0.9,
                    },
                ),
                ev(
                    3.2,
                    Severity::Info,
                    LoadingGateTransitionToInGame { elapsed_secs: 2.7 },
                ),
                ev(
                    10.0,
                    Severity::Info,
                    SessionEnd {
                        reason: "app_exit".into(),
                    },
                ),
            ],
            unparseable: 0,
        };
        let r = report("s.jsonl", &parsed);

        // Timeline milestones render at their timestamps.
        assert!(r.contains("[Timeline]"), "{r}");
        assert!(r.contains("loading gate opened"), "{r}");
        assert!(r.contains("Room fetch Ok"), "{r}");
        // The failed fetch is a timeline milestone even though it's distro-excluded.
        assert!(r.contains("Inventory fetch Exhausted"), "{r}");
        assert!(r.contains("→ InGame (2.7s)"), "{r}");
        assert!(r.contains("world compiled (1200 entities)"), "{r}");
        assert!(r.contains("session end (app_exit)"), "{r}");

        // Loading-gate section: the gate total + per-stage distros.
        assert!(r.contains("[Loading Gate]"), "{r}");
        assert!(r.contains("Loading → InGame:  2.7s"), "{r}");
        // record-fetch distro folds only the two SUCCESSFUL completions (0.8,
        // 0.4) — the Exhausted failure (30.0s) is excluded, so n=2 not 3 and the
        // max stays 0.8.
        assert!(r.contains("record fetch"), "{r}");
        assert!(
            r.contains("(n=2)"),
            "record-fetch distro should exclude the failure (n=2): {r}"
        );
        assert!(
            !r.contains("max 30.0"),
            "the 30s failure latency must not appear in any distro: {r}"
        );
        assert!(r.contains("heightmap"), "{r}");
        assert!(r.contains("ambient bake"), "{r}");
        assert!(r.contains("world compile"), "{r}");
    }

    #[test]
    fn loading_gate_section_marks_a_missing_gate() {
        let parsed = ParsedLog {
            events: vec![
                ev(0.0, Severity::Info, startup_info(Some("did:plc:me"))),
                ev(
                    1.0,
                    Severity::Info,
                    EventPayload::SessionEnd {
                        reason: "app_exit".into(),
                    },
                ),
            ],
            unparseable: 0,
        };
        let r = report("s.jsonl", &parsed);
        assert!(
            r.contains("Loading → InGame:  — (no loading gate in this log)"),
            "{r}"
        );
        // Every stage row still renders, but as an empty distro — so no `(n=…)`
        // count appears anywhere when nothing ran.
        assert!(r.contains("[Loading Gate]"), "{r}");
        assert!(r.contains("record fetch"), "{r}");
        assert!(r.contains("world compile"), "{r}");
        assert!(
            !r.contains("(n="),
            "no stage distro should render when nothing ran: {r}"
        );
    }
}

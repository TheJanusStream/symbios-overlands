//! Unit tests for the analyzer — kept as one module because the fixture
//! helpers (`ev`, `startup_info`, `snapshot`, …) are shared across the
//! parse / sections / filters / diff groups.

use super::diff::*;
use super::filters::*;
use super::parse::*;
use super::sections::*;
use super::*;
use crate::diagnostics::event::{Category, EventPayload, SessionEvent, Severity, Subsystem};
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

fn snapshot(at: f64) -> EventPayload {
    use crate::diagnostics::registry::MetricSnapshot;
    EventPayload::MetricsSnapshot(Box::new(MetricSnapshot {
        at_secs: at,
        gauges: Vec::new(),
        counters: Vec::new(),
        histograms: Vec::new(),
    }))
}

/// The order arrays and their index functions must agree: the index of a
/// variant equals its position in the array. The index fns are exhaustive
/// matches (a new variant is a compile error there); this pins the arrays to
/// them, so a reorder or mid-array insert can't silently misalign a
/// matrix/category column. (A pure *append* left out of the array is caught
/// at compile time by the index-fn match, and — belt-and-suspenders —
/// surfaced as `unclassified` at runtime rather than panicking.)
#[test]
fn order_arrays_match_their_index() {
    for (i, s) in SUBSYSTEM_ORDER.iter().enumerate() {
        assert_eq!(subsystem_index(*s), i, "subsystem order/index drift at {i}");
    }
    for (i, s) in SEVERITY_ORDER.iter().enumerate() {
        assert_eq!(severity_index(*s), i, "severity order/index drift at {i}");
    }
    for (i, c) in CATEGORY_ORDER.iter().enumerate() {
        assert_eq!(category_index(*c), i, "category order/index drift at {i}");
    }
}

/// The tally matrix counts non-snapshot events by subsystem × severity and
/// by category, and the 1 Hz metric snapshots are excluded (noted) so they
/// don't bury the notable-event counts.
#[test]
fn event_tallies_matrix_and_categories_exclude_snapshots() {
    let events = vec![
        ev(0.0, Severity::Info, startup_info(Some("did:plc:me"))), // Session / Snapshot
        ev(0.5, Severity::Info, EventPayload::LoadingPhaseStarted), // Loading / Lifecycle
        ev(
            1.0,
            Severity::Warn,
            EventPayload::PeerIdentitySpoofRejected {
                peer: "p".into(),
                claimed_did: "a".into(),
                authenticated_did: "b".into(),
            },
        ), // Network / Peer
        ev(
            1.2,
            Severity::Error,
            EventPayload::OffloadJobFailed {
                job: "heightmap".into(),
                reason: "gone".into(),
            },
        ), // Offload / Job
        ev(
            1.5,
            Severity::Critical,
            EventPayload::RespawnTriggered {
                fell_to_y: -30.0,
                ground_y: 4.0,
            },
        ), // Runtime / Physics
        ev(2.0, Severity::Trace, snapshot(2.0)),                   // excluded
        ev(3.0, Severity::Trace, snapshot(3.0)),                   // excluded
        ev(
            4.0,
            Severity::Info,
            EventPayload::SessionEnd {
                reason: "app_exit".into(),
            },
        ), // Session / Lifecycle
    ];
    let mut out = String::new();
    write_event_tallies(&mut out, &events);

    assert!(
        out.contains("[Event Tallies]  (excluding 2 metric snapshots)"),
        "{out}"
    );
    // Matrix header + one row per subsystem that had events.
    assert!(out.contains("subsystem"), "{out}");
    assert!(out.contains("crit"), "{out}");
    assert!(out.contains("Network"), "{out}");
    assert!(out.contains("Offload"), "{out}");
    assert!(out.contains("Runtime"), "{out}");
    assert!(out.contains("Session"), "{out}");
    assert!(out.contains("total"), "{out}");
    // By category: Lifecycle (loading + session end) = 2 is busiest; the two
    // MetricsSnapshot events are excluded, so Snapshot is 1 (startup only),
    // not 3 — the proof that snapshots stayed out of the tally.
    assert!(out.contains("by category:"), "{out}");
    assert!(out.contains("Lifecycle 2"), "{out}");
    assert!(out.contains("Snapshot 1"), "{out}");
}

/// A log of nothing but metric snapshots has no notable events to tally.
#[test]
fn event_tallies_only_snapshots_reports_no_events() {
    let events = vec![
        ev(1.0, Severity::Trace, snapshot(1.0)),
        ev(2.0, Severity::Trace, snapshot(2.0)),
        ev(3.0, Severity::Trace, snapshot(3.0)),
    ];
    let mut out = String::new();
    write_event_tallies(&mut out, &events);
    assert!(out.contains("(excluding 3 metric snapshots)"), "{out}");
    assert!(out.contains("(no non-snapshot events)"), "{out}");
}

fn metric_snapshot(
    at: f64,
    frame_ms: f64,
    entities: f64,
    mem_bytes: f64,
    peers: u64,
) -> EventPayload {
    use crate::diagnostics::registry::{CounterPoint, GaugePoint, HistPoint, MetricSnapshot};
    EventPayload::MetricsSnapshot(Box::new(MetricSnapshot {
        at_secs: at,
        gauges: vec![
            GaugePoint {
                name: "runtime.frame_time.ms".into(),
                last: frame_ms,
            },
            GaugePoint {
                name: "runtime.entity.count".into(),
                last: entities,
            },
            GaugePoint {
                name: "runtime.memory.process_rss_bytes".into(),
                last: mem_bytes,
            },
        ],
        counters: vec![CounterPoint {
            name: "net.peer.connected_count".into(),
            value: peers,
        }],
        histograms: vec![HistPoint {
            name: "net.jitter.playout_latency_ms".into(),
            min: 8.0,
            p50: 12.0,
            p90: 24.0,
            max: 40.0,
            mean: 14.0,
            n: 100,
        }],
    }))
}

#[test]
fn fmt_bytes_humanizes_sizes() {
    assert_eq!(fmt_bytes(512.0), "512 B");
    assert_eq!(fmt_bytes(1024.0), "1.0 KB");
    assert_eq!(fmt_bytes(256.0 * 1024.0 * 1024.0), "256.0 MB");
    assert_eq!(fmt_bytes(3.0 * 1024.0 * 1024.0 * 1024.0), "3.0 GB");
}

/// Metric trends chart the snapshot series: gauge first → last + a run distro,
/// counter growth, and the histogram's final distribution.
#[test]
fn metric_trends_chart_gauge_counter_and_histogram_series() {
    let parsed = ParsedLog {
        events: vec![
            ev(0.0, Severity::Info, startup_info(Some("did:plc:me"))),
            ev(
                1.0,
                Severity::Trace,
                metric_snapshot(1.0, 16.0, 1200.0, 256.0 * 1024.0 * 1024.0, 1),
            ),
            ev(
                2.0,
                Severity::Trace,
                metric_snapshot(2.0, 22.0, 1240.0, 336.0 * 1024.0 * 1024.0, 3),
            ),
            ev(
                3.0,
                Severity::Info,
                EventPayload::SessionEnd {
                    reason: "app_exit".into(),
                },
            ),
        ],
        unparseable: 0,
    };
    let r = report("s.jsonl", &parsed);

    assert!(r.contains("[Metric Trends]"), "{r}");
    assert!(r.contains("over 2 snapshots"), "{r}");
    // Gauge trend: first → last + a distro over both samples.
    assert!(r.contains("runtime.frame_time.ms"), "{r}");
    assert!(r.contains("16.0 →"), "gauge first value: {r}");
    assert!(r.contains("(n=2)"), "gauge run distro n: {r}");
    // Entity-count drift (the leak signal) is charted.
    assert!(r.contains("runtime.entity.count"), "{r}");
    // The memory-growth curve is humanized (MB), not raw bytes.
    assert!(r.contains("runtime.memory.process_rss_bytes"), "{r}");
    assert!(r.contains("256.0 MB → 336.0 MB"), "humanized memory: {r}");
    assert!(
        !r.contains("268435456"),
        "raw byte counts must not appear: {r}"
    );
    // Counter growth 1 → 3.
    assert!(r.contains("net.peer.connected_count"), "{r}");
    assert!(r.contains("(+2)"), "counter growth: {r}");
    // Histogram final distribution.
    assert!(r.contains("histograms (final distribution)"), "{r}");
    assert!(r.contains("net.jitter.playout_latency_ms"), "{r}");
}

/// A log with no metric snapshots (e.g. a crash before the first scrape)
/// renders the section but marks it empty — never a silent omission.
#[test]
fn metric_trends_absent_when_no_snapshots() {
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
    assert!(r.contains("[Metric Trends]"), "{r}");
    assert!(r.contains("(no metric snapshots in this log)"), "{r}");
}

// --- B-4: --diff-sessions -----------------------------------------------

fn spoof() -> EventPayload {
    EventPayload::PeerIdentitySpoofRejected {
        peer: "p".into(),
        claimed_did: "did:plc:evil".into(),
        authenticated_did: "did:plc:real".into(),
    }
}

/// A full baseline log: a gate stall + a spoof burst + slow record fetches,
/// with metric snapshots showing high memory + peers.
fn baseline_log() -> ParsedLog {
    use crate::diagnostics::event::RecordKind;
    let did = "did:plc:me".to_string();
    ParsedLog {
        events: vec![
            ev(0.0, Severity::Info, startup_info(Some("did:plc:me"))),
            ev(0.4, Severity::Info, EventPayload::LoadingPhaseStarted),
            ev(
                1.0,
                Severity::Info,
                EventPayload::RecordFetchCompleted {
                    record: RecordKind::Room,
                    did: did.clone(),
                    status: FetchStatus::Ok,
                    duration_secs: 2.0,
                },
            ),
            // 3 spoofs → net.identity_spoof_burst fires (replayable).
            ev(2.0, Severity::Warn, spoof()),
            ev(2.1, Severity::Warn, spoof()),
            ev(2.2, Severity::Warn, spoof()),
            ev(
                3.0,
                Severity::Trace,
                metric_snapshot(3.0, 30.0, 1300.0, 384.0 * 1024.0 * 1024.0, 3),
            ),
            // No InGame transition, and the log runs long → gate_stall replays.
            ev(200.0, Severity::Info, EventPayload::RoomStateApplied),
        ],
        unparseable: 0,
    }
}

/// The candidate log: the fix landed — gate reaches InGame fast, no spoofs,
/// faster fetch, lower memory.
fn candidate_log() -> ParsedLog {
    use crate::diagnostics::event::RecordKind;
    ParsedLog {
        events: vec![
            ev(0.0, Severity::Info, startup_info(Some("did:plc:me"))),
            ev(0.4, Severity::Info, EventPayload::LoadingPhaseStarted),
            ev(
                0.9,
                Severity::Info,
                EventPayload::RecordFetchCompleted {
                    record: RecordKind::Room,
                    did: "did:plc:me".into(),
                    status: FetchStatus::Ok,
                    duration_secs: 0.5,
                },
            ),
            ev(
                2.5,
                Severity::Trace,
                metric_snapshot(2.5, 17.0, 1200.0, 260.0 * 1024.0 * 1024.0, 2),
            ),
            ev(
                3.0,
                Severity::Info,
                EventPayload::LoadingGateTransitionToInGame { elapsed_secs: 2.6 },
            ),
            ev(
                10.0,
                Severity::Info,
                EventPayload::SessionEnd {
                    reason: "app_exit".into(),
                },
            ),
        ],
        unparseable: 0,
    }
}

#[test]
fn diff_report_surfaces_verdict_gate_metric_and_invariant_deltas() {
    let a = baseline_log();
    let b = candidate_log();
    let r = diff_report("baseline.jsonl", &a, "candidate.jsonl", &b);

    // Header labels both sessions.
    assert!(r.contains("=== session diff — A vs B ==="), "{r}");
    assert!(r.contains("A: baseline.jsonl"), "{r}");
    assert!(r.contains("B: candidate.jsonl"), "{r}");

    // Verdict delta: B has fewer warnings (3 spoofs → 0) → improved.
    assert!(r.contains("[Verdict Delta]"), "{r}");
    assert!(r.contains("warnings      3 → 0"), "{r}");
    assert!(r.contains("B improved"), "{r}");

    // Gate delta: A never reached InGame; B reached it in 2.6s.
    assert!(r.contains("[Loading Gate Delta]"), "{r}");
    assert!(r.contains("Loading → InGame:  — → 2.6s"), "{r}");
    // Record-fetch mean improved 2.0s → 0.5s.
    assert!(
        r.contains("record fetch") && r.contains("2.0s → 0.5s"),
        "{r}"
    );
    assert!(r.contains("(-1.5)"), "fetch delta: {r}");

    // Metric delta: memory peak dropped, humanized + signed.
    assert!(r.contains("[Metric Delta]"), "{r}");
    assert!(r.contains("runtime.memory.process_rss_bytes"), "{r}");
    assert!(r.contains("384.0 MB → 260.0 MB"), "{r}");
    assert!(r.contains("(-124.0 MB)"), "memory delta: {r}");
    // Counter delta present.
    assert!(
        r.contains("net.peer.connected_count") && r.contains("3 → 2"),
        "{r}"
    );

    // Invariant delta: gate_stall + spoof_burst fired in A, resolved in B.
    assert!(r.contains("[Invariant Delta]"), "{r}");
    assert!(
        r.contains("[resolved]") && r.contains("loading.gate_stall"),
        "{r}"
    );
    assert!(r.contains("net.identity_spoof_burst"), "{r}");
    assert!(r.contains("resolved,"), "summary line: {r}");
}

#[test]
fn diff_report_flags_a_regression() {
    // A is clean; B introduces a spoof burst → NEW invariant + more warnings.
    let a = candidate_log();
    let b = baseline_log();
    let r = diff_report("good.jsonl", &a, "bad.jsonl", &b);
    assert!(r.contains("B regressed"), "{r}");
    assert!(
        r.contains("[NEW"),
        "a rule that fired only in B is tagged NEW: {r}"
    );
}

#[test]
fn diff_report_handles_two_empty_logs() {
    let empty = ParsedLog {
        events: Vec::new(),
        unparseable: 0,
    };
    let r = diff_report("a.jsonl", &empty, "b.jsonl", &empty);
    assert!(r.contains("both logs empty"), "{r}");
}

#[test]
fn invariant_tag_classifies_each_direction() {
    assert_eq!(invariant_tag(0, 2).0, "NEW");
    assert_eq!(invariant_tag(2, 0).0, "resolved");
    assert_eq!(invariant_tag(1, 3).0, "worse");
    assert_eq!(invariant_tag(3, 1).0, "better");
    assert_eq!(invariant_tag(2, 2).0, "same");
}

/// A gauge peak's displayed operands are rounded the same way its Δ is, so a
/// half-tenth value can't print a row whose `A → B  (Δ)` fails to add up.
#[test]
fn metric_delta_gauge_display_reconciles_with_its_delta() {
    use crate::diagnostics::registry::{GaugePoint, MetricSnapshot};
    let snap = |peak: f64| {
        EventPayload::MetricsSnapshot(Box::new(MetricSnapshot {
            at_secs: 1.0,
            gauges: vec![GaugePoint {
                name: "runtime.frame_time.ms".into(),
                last: peak,
            }],
            counters: Vec::new(),
            histograms: Vec::new(),
        }))
    };
    // 16.25 / 16.34: {:.1} (half-even) and round1 (half-away) would disagree.
    let a = ParsedLog {
        events: vec![
            ev(0.0, Severity::Info, startup_info(Some("x"))),
            ev(1.0, Severity::Trace, snap(16.25)),
        ],
        unparseable: 0,
    };
    let b = ParsedLog {
        events: vec![
            ev(0.0, Severity::Info, startup_info(Some("x"))),
            ev(1.0, Severity::Trace, snap(16.34)),
        ],
        unparseable: 0,
    };
    let r = diff_report("a.jsonl", &a, "b.jsonl", &b);
    // Both operands round1 → 16.3, so the row reads "16.3 → 16.3 (+0.0)".
    assert!(r.contains("16.3 → 16.3"), "operands must be round1'd: {r}");
    assert!(
        r.contains("(+0.0)"),
        "delta must reconcile with operands: {r}"
    );
}

/// A counter that fired in A but whose candidate B has *no metric snapshots
/// at all* (crashed before the first scrape) must read `N → —` (unknown),
/// never `N → 0` — a data-less run is not an improvement.
#[test]
fn metric_delta_absent_counter_without_snapshots_is_unknown_not_resolved() {
    let a = ParsedLog {
        events: vec![
            ev(0.0, Severity::Info, startup_info(Some("x"))),
            ev(
                1.0,
                Severity::Trace,
                metric_snapshot(1.0, 16.0, 1200.0, 256.0 * 1024.0 * 1024.0, 3),
            ),
        ],
        unparseable: 0,
    };
    let b = ParsedLog {
        events: vec![
            ev(0.0, Severity::Info, startup_info(Some("x"))),
            ev(
                1.0,
                Severity::Info,
                EventPayload::SessionEnd {
                    reason: "crash".into(),
                },
            ),
        ],
        unparseable: 0,
    };
    let r = diff_report("a.jsonl", &a, "b.jsonl", &b);
    assert!(r.contains("net.peer.connected_count"), "{r}");
    assert!(
        !r.contains("3 → 0"),
        "an absent-because-no-snapshots counter must not read as resolved: {r}"
    );
    assert!(r.contains("3 → —"), "it should read as unknown: {r}");
}

/// Snapshots that carry only histograms (no gauge/counter series this section
/// diffs) report as present-but-nothing-to-diff, not as absent.
#[test]
fn metric_delta_histograms_only_distinguishes_from_no_snapshots() {
    use crate::diagnostics::registry::{HistPoint, MetricSnapshot};
    let snap = || {
        EventPayload::MetricsSnapshot(Box::new(MetricSnapshot {
            at_secs: 1.0,
            gauges: Vec::new(),
            counters: Vec::new(),
            histograms: vec![HistPoint {
                name: "net.jitter.playout_latency_ms".into(),
                min: 8.0,
                p50: 12.0,
                p90: 24.0,
                max: 40.0,
                mean: 14.0,
                n: 50,
            }],
        }))
    };
    let mklog = || ParsedLog {
        events: vec![
            ev(0.0, Severity::Info, startup_info(Some("x"))),
            ev(1.0, Severity::Trace, snap()),
        ],
        unparseable: 0,
    };
    let r = diff_report("a.jsonl", &mklog(), "b.jsonl", &mklog());
    assert!(
        r.contains("metric snapshots present, but no gauge/counter series to diff"),
        "{r}"
    );
}

// --- B-5: analyzer filters ----------------------------------------------

fn mixed_log() -> ParsedLog {
    ParsedLog {
        events: vec![
            ev(0.0, Severity::Info, startup_info(Some("did:plc:me"))), // Session
            ev(1.0, Severity::Info, EventPayload::LoadingPhaseStarted), // Loading
            ev(
                5.0,
                Severity::Warn,
                EventPayload::PeerIdentitySpoofRejected {
                    peer: "p".into(),
                    claimed_did: "a".into(),
                    authenticated_did: "b".into(),
                },
            ), // Network / Warn / t=5
            ev(
                50.0,
                Severity::Error,
                EventPayload::OffloadJobFailed {
                    job: "heightmap".into(),
                    reason: "gone".into(),
                },
            ), // Offload / Error / t=50
            ev(
                90.0,
                Severity::Info,
                EventPayload::SessionEnd {
                    reason: "app_exit".into(),
                },
            ), // Session
        ],
        unparseable: 0,
    }
}

#[test]
fn filters_parse_names_case_insensitively_and_reject_unknowns() {
    let f = Filters::parse(
        Some("Network"),
        Some("peer"),
        Some("warn"),
        Some(1.0),
        Some(9.0),
    )
    .expect("valid");
    assert_eq!(f.subsystem, Some(Subsystem::Network));
    assert_eq!(f.category, Some(Category::Peer));
    assert_eq!(f.min_severity, Some(Severity::Warn));
    assert!(f.is_active());
    assert!(Filters::parse(Some("bogus"), None, None, None, None).is_err());
    assert!(Filters::parse(None, Some("bogus"), None, None, None).is_err());
    assert!(Filters::parse(None, None, Some("bogus"), None, None).is_err());
    // An all-None filter is an inactive passthrough.
    assert!(
        !Filters::parse(None, None, None, None, None)
            .unwrap()
            .is_active()
    );
}

#[test]
fn subsystem_filter_scopes_sections_but_not_the_header() {
    let log = mixed_log();
    let filters = Filters::parse(Some("network"), None, None, None, None).unwrap();
    let r = report_with("s.jsonl", &log, &filters);
    // Header still identifies the run (from the FULL log).
    assert!(r.contains("did: did:plc:me"), "header from full log: {r}");
    assert!(r.contains("v0.1.0 (deadbee)"), "{r}");
    // Filter line documents the lens + match count (1 Network event of 5).
    assert!(
        r.contains("[Filter]  subsystem=Network  —  1 of 5 events match"),
        "{r}"
    );
    // Verdict reflects only the Network subset: the spoof Warn, not the
    // Offload Error.
    assert!(r.contains("1 warning") && !r.contains("1 error"), "{r}");
}

#[test]
fn severity_filter_is_a_minimum_threshold() {
    let log = mixed_log();
    let filters = Filters::parse(None, None, Some("error"), None, None).unwrap();
    let r = report_with("s.jsonl", &log, &filters);
    // Only the Error (Offload) matches; the Warn spoof is below threshold.
    assert!(r.contains("severity≥Error  —  1 of 5 events match"), "{r}");
    assert!(r.contains("1 error"), "{r}");
}

#[test]
fn time_window_filter_bounds_both_ends_inclusive() {
    let log = mixed_log();
    // [4.0, 60.0] captures the t=5 spoof and t=50 offload fail, not t=0/1/90.
    let filters = Filters::parse(None, None, None, Some(4.0), Some(60.0)).unwrap();
    let r = report_with("s.jsonl", &log, &filters);
    assert!(r.contains("t∈[4.0s, 60.0s]  —  2 of 5 events match"), "{r}");
}

#[test]
fn filter_matching_nothing_reports_it_and_keeps_the_header() {
    let log = mixed_log();
    let filters = Filters::parse(Some("runtime"), None, None, None, None).unwrap();
    let r = report_with("s.jsonl", &log, &filters);
    assert!(r.contains("did: did:plc:me"), "header still present: {r}");
    assert!(r.contains("0 of 5 events match"), "{r}");
    assert!(r.contains("(no events match — nothing to analyze)"), "{r}");
    // No analysis sections emitted when nothing matches.
    assert!(!r.contains("[Verdict]"), "{r}");
}

#[test]
fn no_filter_report_equals_report_with_default() {
    // The unfiltered path must be byte-identical to report_with(default).
    let log = mixed_log();
    assert_eq!(
        report("s.jsonl", &log),
        report_with("s.jsonl", &log, &Filters::default())
    );
}

/// A record write (region save) is a timeline milestone (#624) — an in-game
/// save must be visible in the post-mortem, not just record reads.
#[test]
fn timeline_shows_record_writes() {
    use crate::diagnostics::event::RecordKind;
    let write = |t: f64, record| {
        ev(
            t,
            Severity::Info,
            EventPayload::RecordWriteCompleted {
                record,
                did: "did:plc:me".into(),
                duration_secs: 0.4,
            },
        )
    };
    let parsed = ParsedLog {
        events: vec![
            ev(0.0, Severity::Info, startup_info(Some("did:plc:me"))),
            write(30.0, RecordKind::Room),
            write(35.0, RecordKind::Avatar),
            write(38.0, RecordKind::Inventory),
            ev(
                40.0,
                Severity::Info,
                EventPayload::SessionEnd {
                    reason: "app_exit".into(),
                },
            ),
        ],
        unparseable: 0,
    };
    let r = report("s.jsonl", &parsed);
    // #624 + #626: room, avatar, and inventory saves all trace.
    assert!(r.contains("Room saved to PDS"), "{r}");
    assert!(r.contains("Avatar saved to PDS"), "{r}");
    assert!(r.contains("Inventory saved to PDS"), "{r}");
    // Writes land in the Loading/Fetch bucket (PDS record I/O) — 3 of them.
    assert!(
        r.contains("Fetch 3"),
        "record writes count under Fetch: {r}"
    );
}

/// #627: an avatar re-seed is a timeline milestone (Loading/Generation),
/// so an in-game avatar re-roll is visible rather than inferable only from
/// asset-handle churn.
#[test]
fn timeline_shows_avatar_reseed() {
    let parsed = ParsedLog {
        events: vec![
            ev(0.0, Severity::Info, startup_info(Some("did:plc:me"))),
            ev(
                50.0,
                Severity::Info,
                EventPayload::AvatarReseeded { seed: 7 },
            ),
            ev(
                60.0,
                Severity::Info,
                EventPayload::SessionEnd {
                    reason: "app_exit".into(),
                },
            ),
        ],
        unparseable: 0,
    };
    let r = report("s.jsonl", &parsed);
    assert!(r.contains("avatar reseeded (seed 7)"), "{r}");
    // Counts under Loading/Generation, alongside region-regen events.
    assert!(r.contains("Generation 1"), "{r}");
}

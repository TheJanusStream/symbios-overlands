//! Offline invariant replay harness (Pillar D-5) — the `[Invariant Violations]`
//! section of the `--analyze-session` post-mortem (Pillar B).
//!
//! It folds a captured event log two ways, giving the offline counterpart to
//! the live anomaly engine:
//!
//! 1. **Offline re-derived** — every [`Rule`](super::rule::Rule) with a replay
//!    body (the D-2 log-expressible rules) is replayed over the stream, so the
//!    analyzer independently re-derives those violations from the raw events.
//! 2. **Captured live-only** — the D-3 ECS-state rules have no replay body (a
//!    log can't re-derive a NaN transform or a missing collider), so their live
//!    fires are *surfaced* from the `InvariantViolation` events the running
//!    engine recorded. Captured fires for a replayable rule are dropped, since
//!    the re-derived section already covers them (no double counting).
//!
//! Both sides read the **same** [`default_registry`] the live plugin (D-4) uses,
//! so the rule set is byte-identical — the parity guarantee, exercised by the
//! module tests.

use std::collections::{BTreeMap, HashSet};
use std::fmt::Write;

use crate::diagnostics::anomaly::registry::default_registry;
use crate::diagnostics::anomaly::rule::Verdict;
use crate::diagnostics::event::{EventPayload, SessionEvent, Severity};

/// One rule's contribution to the post-mortem: how many times it fired, a
/// sample detail, and whether it was re-derived offline (`replayable`) or only
/// surfaced from a captured live fire.
#[derive(Clone, Debug, PartialEq)]
pub struct RuleFinding {
    pub id: String,
    pub severity: Severity,
    pub count: usize,
    pub sample: String,
    /// `true` = re-derived by replaying the rule over the stream; `false` =
    /// surfaced from a captured `InvariantViolation` (a live-only rule).
    pub replayable: bool,
}

/// Fold the captured log into per-rule findings: the replayed re-derivations
/// plus the surfaced live-only captures. Pure over its input.
pub fn replay_findings(events: &[SessionEvent]) -> Vec<RuleFinding> {
    let reg = default_registry();

    // The rule ids that can be re-derived offline; a captured fire for one of
    // these is redundant with the replayed section, so it is not surfaced twice.
    let replayable_ids: HashSet<&'static str> = reg
        .rules()
        .iter()
        .filter(|r| r.is_replayable())
        .map(|r| r.header().id)
        .collect();

    let mut findings = Vec::new();

    // 1. Offline re-derived: replay every replayable rule over the whole log.
    for rule in reg.rules() {
        if !rule.is_replayable() {
            continue;
        }
        let details: Vec<String> = rule
            .replay(events)
            .into_iter()
            .filter_map(|v| match v {
                Verdict::Violated { detail } => Some(detail),
                Verdict::Clear => None,
            })
            .collect();
        if let Some(sample) = details.first().cloned() {
            let h = rule.header();
            findings.push(RuleFinding {
                id: h.id.to_string(),
                severity: h.severity,
                count: details.len(),
                sample,
                replayable: true,
            });
        }
    }

    // 2. Captured live-only: group the recorded `InvariantViolation` events for
    //    rules with no replay body (D-3 ECS rules, or rules absent from this
    //    build) so their fires are surfaced even though they can't be re-derived.
    //    Keyed in a BTreeMap for deterministic output; keeps the worst severity
    //    seen and the first detail as the sample.
    let mut captured: BTreeMap<String, (Severity, usize, String)> = BTreeMap::new();
    for e in events {
        let EventPayload::InvariantViolation { rule, detail } = &e.payload else {
            continue;
        };
        if replayable_ids.contains(rule.as_str()) {
            continue; // already covered by the re-derived section
        }
        let entry = captured
            .entry(rule.clone())
            .or_insert((e.severity, 0, detail.clone()));
        entry.0 = entry.0.max(e.severity);
        entry.1 += 1;
    }
    for (id, (severity, count, sample)) in captured {
        findings.push(RuleFinding {
            id,
            severity,
            count,
            sample,
            replayable: false,
        });
    }

    findings
}

/// The five-way severity label, upper-cased for the report.
fn sev_label(s: Severity) -> &'static str {
    match s {
        Severity::Trace => "TRACE",
        Severity::Info => "INFO",
        Severity::Warn => "WARN",
        Severity::Error => "ERROR",
        Severity::Critical => "CRITICAL",
    }
}

fn finding_line(f: &RuleFinding) -> String {
    format!(
        "[{:<8}] {} ×{} — {}",
        sev_label(f.severity),
        f.id,
        f.count,
        f.sample
    )
}

/// Render the `[Invariant Violations]` report section for a captured log — the
/// offline counterpart to the live engine, wired into `--analyze-session` (B-1).
/// Findings are printed worst-severity first, split into the re-derived and the
/// captured-live subsections (each shown only when non-empty).
pub fn replay_invariants(events: &[SessionEvent]) -> String {
    let findings = replay_findings(events);
    let mut s = String::new();
    let _ = writeln!(s, "[Invariant Violations]");
    if findings.is_empty() {
        let _ = writeln!(s, "  none — no invariant violations detected");
        return s;
    }

    // Worst severity first, then by id for stable, deterministic output.
    let order = |a: &&RuleFinding, b: &&RuleFinding| {
        b.severity.cmp(&a.severity).then_with(|| a.id.cmp(&b.id))
    };
    let mut rederived: Vec<&RuleFinding> = findings.iter().filter(|f| f.replayable).collect();
    let mut captured: Vec<&RuleFinding> = findings.iter().filter(|f| !f.replayable).collect();
    rederived.sort_by(order);
    captured.sort_by(order);

    if !rederived.is_empty() {
        let _ = writeln!(s, "  offline re-derived (replayed from the event stream):");
        for f in &rederived {
            let _ = writeln!(s, "    {}", finding_line(f));
        }
    }
    if !captured.is_empty() {
        let _ = writeln!(
            s,
            "  captured live (recorded by the running engine; not offline-re-derivable):"
        );
        for f in &captured {
            let _ = writeln!(s, "    {}", finding_line(f));
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(t: f64, sev: Severity, p: EventPayload) -> SessionEvent {
        SessionEvent::new(0, t, None, sev, p)
    }

    fn spoof() -> EventPayload {
        EventPayload::PeerIdentitySpoofRejected {
            peer: "p".into(),
            claimed_did: "a".into(),
            authenticated_did: "b".into(),
        }
    }

    /// A log that trips several replayable (D-2) rules, carries one captured
    /// live-only (D-3) fire, and a captured fire for a REPLAYABLE rule (which
    /// must not double-count).
    fn crafted() -> Vec<SessionEvent> {
        vec![
            ev(0.0, Severity::Info, EventPayload::LoadingPhaseStarted),
            // 3 spoof rejections → net.identity_spoof_burst (replayable) fires.
            ev(1.0, Severity::Warn, spoof()),
            ev(2.0, Severity::Warn, spoof()),
            ev(3.0, Severity::Warn, spoof()),
            // A decode failure → net.silent_decode_failure (replayable) fires.
            ev(
                4.0,
                Severity::Error,
                EventPayload::RoomStateDecodeFailed {
                    sender_did: "did:x".into(),
                    error: "bad".into(),
                },
            ),
            // Captured live-only D-3 fire → surfaced from the recorded event.
            ev(
                5.0,
                Severity::Critical,
                EventPayload::InvariantViolation {
                    rule: "runtime.terrain_collider_missing".into(),
                    detail: "0 colliders in-game — terrain body missing".into(),
                },
            ),
            // Captured fire for a REPLAYABLE rule → must NOT surface again.
            ev(
                6.0,
                Severity::Critical,
                EventPayload::InvariantViolation {
                    rule: "loading.gate_stall".into(),
                    detail: "in loading gate 200s (> 120s)".into(),
                },
            ),
            // No InGame transition; log runs past budget → gate_stall replay fires.
            ev(200.0, Severity::Info, EventPayload::RoomStateApplied),
        ]
    }

    #[test]
    fn re_derives_replayable_and_surfaces_live_only_without_double_counting() {
        let findings = replay_findings(&crafted());
        let has = |id: &str, replayable: bool| {
            findings
                .iter()
                .any(|f| f.id.as_str() == id && f.replayable == replayable)
        };

        // Replayable rules re-derived directly from the stream.
        assert!(has("loading.gate_stall", true));
        assert!(has("net.identity_spoof_burst", true));
        assert!(has("net.silent_decode_failure", true));

        // Live-only D-3 fire surfaced from its captured event.
        assert!(has("runtime.terrain_collider_missing", false));

        // The captured fire for a REPLAYABLE rule is NOT re-surfaced as
        // live-only (that would double-count with the re-derived section).
        assert!(!has("loading.gate_stall", false));
    }

    /// The parity guarantee: the offline harness's re-derived counts equal what
    /// the shared `default_registry()` (the live plugin's rule set) produces,
    /// rule-for-rule, over the same log.
    #[test]
    fn parity_offline_harness_matches_shared_registry_replay() {
        let events = crafted();
        let findings = replay_findings(&events);
        let reg = default_registry();
        for rule in reg.rules() {
            if !rule.is_replayable() {
                continue;
            }
            let expected = rule
                .replay(&events)
                .iter()
                .filter(|v| v.is_violated())
                .count();
            let got = findings
                .iter()
                .find(|f| f.id.as_str() == rule.header().id && f.replayable)
                .map(|f| f.count)
                .unwrap_or(0);
            assert_eq!(
                got,
                expected,
                "offline harness disagrees with the shared registry for {}",
                rule.header().id
            );
        }
    }

    #[test]
    fn replay_invariants_formats_both_subsections() {
        let text = replay_invariants(&crafted());
        assert!(text.contains("[Invariant Violations]"));
        assert!(text.contains("offline re-derived"));
        assert!(text.contains("captured live"));
        assert!(text.contains("loading.gate_stall"));
        assert!(text.contains("runtime.terrain_collider_missing"));
        // Severity-labelled, worst-first.
        assert!(text.contains("[CRITICAL]"));
    }

    /// Pin the set of replayable rule ids. A new rule that adds a `replay`
    /// body must mark `is_replayable()` and update this list, or it is silently
    /// misclassified (its captured fires surface as live-only, and it is never
    /// re-derived) — this test forces the author to keep the flag in sync.
    #[test]
    fn replayable_rule_set_is_pinned() {
        let reg = default_registry();
        let mut ids: Vec<&str> = reg
            .rules()
            .iter()
            .filter(|r| r.is_replayable())
            .map(|r| r.header().id)
            .collect();
        ids.sort_unstable();
        assert_eq!(
            ids,
            vec![
                "loading.gate_stall",
                "loading.record_fetch_exhausted",
                "net.identity_spoof_burst",
                "net.offer_acceptance_anomaly",
                "net.peer_churn_spike",
                "net.relay_connection_rejected",
                "net.signal_glare_suspected",
                "net.silent_decode_failure",
                "offload.ambient_bake_stall",
                "offload.task_never_resolves",
            ]
        );
    }

    /// The other drift direction: a rule flagged NOT replayable must genuinely
    /// have no replay body. A diverse log that trips every replayable rule must
    /// leave every non-replayable rule silent — otherwise a `replay` impl was
    /// added without setting `is_replayable()`.
    #[test]
    fn non_replayable_rules_have_no_replay_body() {
        let diverse = vec![
            ev(0.0, Severity::Info, EventPayload::LoadingPhaseStarted),
            ev(
                1.0,
                Severity::Error,
                EventPayload::RecordFetchCompleted {
                    record: crate::diagnostics::event::RecordKind::Room,
                    did: "did:x".into(),
                    status: crate::diagnostics::event::FetchStatus::Exhausted,
                    duration_secs: 1.0,
                },
            ),
            ev(
                2.0,
                Severity::Info,
                EventPayload::AmbientBakeStarted {
                    variant: "v".into(),
                },
            ),
            ev(
                3.0,
                Severity::Info,
                EventPayload::OffloadJobStarted { job: "j".into() },
            ),
            ev(4.0, Severity::Warn, spoof()),
            ev(5.0, Severity::Warn, spoof()),
            ev(6.0, Severity::Warn, spoof()),
            ev(
                7.0,
                Severity::Error,
                EventPayload::AvatarStateDecodeFailed {
                    peer: "p".into(),
                    reason: "bad".into(),
                },
            ),
            ev(400.0, Severity::Info, EventPayload::RoomStateApplied),
        ];
        let reg = default_registry();
        for rule in reg.rules() {
            if rule.is_replayable() {
                continue;
            }
            assert!(
                rule.replay(&diverse).is_empty(),
                "rule {} is flagged live-only but produced replay verdicts — set is_replayable()",
                rule.header().id
            );
        }
    }

    #[test]
    fn clean_log_reports_no_violations() {
        let clean = vec![
            ev(0.0, Severity::Info, EventPayload::LoadingPhaseStarted),
            ev(
                3.0,
                Severity::Info,
                EventPayload::LoadingGateTransitionToInGame { elapsed_secs: 3.0 },
            ),
        ];
        let text = replay_invariants(&clean);
        assert!(text.contains("none — no invariant violations"));
    }
}

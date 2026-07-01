//! Built-in invariant rules.
//!
//! This slice (D-2) holds the **log-expressible** invariants: conditions fully
//! determined by the event stream, so each implements a REPLAY body (folded by
//! the offline analyzer) and, where an equivalent live signal exists, a LIVE
//! body too. The ECS-state rules (NaN transforms, terrain collider, asset
//! growth, …) are added by the D-3 slice, which extends [`register_builtins`].

use crate::diagnostics::anomaly::registry::InvariantRegistry;
use crate::diagnostics::anomaly::rule::{DebouncePolicy, LiveCtx, Rule, RuleHeader, Verdict};
use crate::diagnostics::event::{EventPayload, FetchStatus, SessionEvent, Severity, Subsystem};
use crate::diagnostics::names;
use crate::state::AppState;

/// Register every built-in rule. Shared by the live plugin (D-4) and the
/// offline analyzer (D-5) via [`super::registry::default_registry`].
pub fn register_builtins(reg: &mut InvariantRegistry) {
    reg.register(LoadingGateStall);
    reg.register(RecordFetchExhausted);
    reg.register(AmbientBakeStall);
    reg.register(TaskNeverResolves);
    reg.register(PeerChurnSpike);
    reg.register(OfferAcceptanceAnomaly);
    reg.register(IdentitySpoofBurst);
    reg.register(SilentDecodeFailure);
    // D-3 ECS-state (live-only) rules.
    super::rules_ecs::register_ecs_rules(reg);
}

/// The last event's timestamp (session end), for "never resolved" checks.
fn last_ts(events: &[SessionEvent]) -> f64 {
    events.last().map(|e| e.t_mono_secs).unwrap_or(0.0)
}

/// Durations of unresolved / over-budget start→end spans: for each event
/// matching `is_start`, find the first later event matching `is_end`; emit the
/// gap when it exceeds `budget`, or the elapsed-so-far when it never resolved
/// but the log continued past `budget`.
fn stall_durations(
    events: &[SessionEvent],
    is_start: impl Fn(&EventPayload) -> bool,
    is_end: impl Fn(&EventPayload) -> bool,
    budget: f64,
) -> Vec<f64> {
    let last = last_ts(events);
    let mut out = Vec::new();
    for (i, e) in events.iter().enumerate() {
        if !is_start(&e.payload) {
            continue;
        }
        let start = e.t_mono_secs;
        let end = events[i + 1..]
            .iter()
            .find(|f| is_end(&f.payload))
            .map(|f| f.t_mono_secs);
        match end {
            Some(t) if t - start > budget => out.push(t - start),
            None if last - start > budget => out.push(last - start),
            _ => {}
        }
    }
    out
}

// --- LoadingGateStall -------------------------------------------------------
const GATE_STALL_SECS: f64 = 120.0;

struct LoadingGateStall;
const LOADING_GATE_STALL: RuleHeader = RuleHeader {
    id: "loading.gate_stall",
    subsystem: Subsystem::Loading,
    severity: Severity::Critical,
    debounce: DebouncePolicy::OncePerCondition,
    description: "loading gate exceeded its time budget",
    when_state: Some(AppState::Loading),
};
impl Rule for LoadingGateStall {
    fn header(&self) -> &RuleHeader {
        &LOADING_GATE_STALL
    }
    fn eval(&self, cx: &LiveCtx) -> Option<Verdict> {
        let elapsed = cx.loading_elapsed_secs?;
        Some(if elapsed > GATE_STALL_SECS {
            Verdict::violated(format!(
                "in loading gate {elapsed:.0}s (> {GATE_STALL_SECS:.0}s)"
            ))
        } else {
            Verdict::Clear
        })
    }
    fn replay(&self, events: &[SessionEvent]) -> Vec<Verdict> {
        stall_durations(
            events,
            |p| matches!(p, EventPayload::LoadingPhaseStarted),
            |p| matches!(p, EventPayload::LoadingGateTransitionToInGame { .. }),
            GATE_STALL_SECS,
        )
        .into_iter()
        .map(|d| Verdict::violated(format!("loading gate took {d:.0}s")))
        .collect()
    }
}

// --- RecordFetchExhausted ---------------------------------------------------
struct RecordFetchExhausted;
const RECORD_FETCH_EXHAUSTED: RuleHeader = RuleHeader {
    id: "loading.record_fetch_exhausted",
    subsystem: Subsystem::Loading,
    severity: Severity::Error,
    debounce: DebouncePolicy::OncePerCondition,
    description: "a PDS record fetch exhausted its retry budget",
    when_state: None,
};
impl Rule for RecordFetchExhausted {
    fn header(&self) -> &RuleHeader {
        &RECORD_FETCH_EXHAUSTED
    }
    fn replay(&self, events: &[SessionEvent]) -> Vec<Verdict> {
        events
            .iter()
            .filter_map(|e| match &e.payload {
                EventPayload::RecordFetchCompleted {
                    record,
                    did,
                    status: FetchStatus::Exhausted,
                    ..
                } => Some(Verdict::violated(format!(
                    "{record:?} fetch for {did} exhausted retries"
                ))),
                _ => None,
            })
            .collect()
    }
}

// --- AmbientBakeStall -------------------------------------------------------
const AMBIENT_STALL_SECS: f64 = 30.0;

struct AmbientBakeStall;
const AMBIENT_BAKE_STALL: RuleHeader = RuleHeader {
    id: "offload.ambient_bake_stall",
    subsystem: Subsystem::Offload,
    severity: Severity::Error,
    debounce: DebouncePolicy::OncePerCondition,
    description: "ambient audio bake did not finish within its budget",
    when_state: None,
};
impl Rule for AmbientBakeStall {
    fn header(&self) -> &RuleHeader {
        &AMBIENT_BAKE_STALL
    }
    fn replay(&self, events: &[SessionEvent]) -> Vec<Verdict> {
        stall_durations(
            events,
            |p| matches!(p, EventPayload::AmbientBakeStarted { .. }),
            |p| {
                matches!(
                    p,
                    EventPayload::AmbientBakeCompleted { .. }
                        | EventPayload::AmbientBakeFallback { .. }
                )
            },
            AMBIENT_STALL_SECS,
        )
        .into_iter()
        .map(|d| Verdict::violated(format!("ambient bake took {d:.0}s")))
        .collect()
    }
}

// --- TaskNeverResolves (offload jobs, keyed by job name) --------------------
const TASK_TIMEOUT_SECS: f64 = 60.0;

struct TaskNeverResolves;
const TASK_NEVER_RESOLVES: RuleHeader = RuleHeader {
    id: "offload.task_never_resolves",
    subsystem: Subsystem::Offload,
    severity: Severity::Critical,
    debounce: DebouncePolicy::OncePerCondition,
    description: "an offloaded job never reported completion or failure",
    when_state: None,
};
impl Rule for TaskNeverResolves {
    fn header(&self) -> &RuleHeader {
        &TASK_NEVER_RESOLVES
    }
    fn replay(&self, events: &[SessionEvent]) -> Vec<Verdict> {
        let last = last_ts(events);
        let mut out = Vec::new();
        for (i, e) in events.iter().enumerate() {
            let EventPayload::OffloadJobStarted { job } = &e.payload else {
                continue;
            };
            let start = e.t_mono_secs;
            let end = events[i + 1..].iter().find_map(|f| match &f.payload {
                EventPayload::OffloadJobCompleted { job: j, .. }
                | EventPayload::OffloadJobFailed { job: j, .. }
                    if j == job =>
                {
                    Some(f.t_mono_secs)
                }
                _ => None,
            });
            match end {
                Some(t) if t - start > TASK_TIMEOUT_SECS => out.push(Verdict::violated(format!(
                    "job '{job}' took {:.0}s",
                    t - start
                ))),
                None if last - start > TASK_TIMEOUT_SECS => {
                    out.push(Verdict::violated(format!("job '{job}' never resolved")))
                }
                _ => {}
            }
        }
        out
    }
}

// --- PeerChurnSpike (windowed) ----------------------------------------------
const CHURN_WINDOW_SECS: f64 = 300.0;
const CHURN_LIMIT: usize = 10;

struct PeerChurnSpike;
const PEER_CHURN_SPIKE: RuleHeader = RuleHeader {
    id: "net.peer_churn_spike",
    subsystem: Subsystem::Network,
    severity: Severity::Warn,
    debounce: DebouncePolicy::OncePerCondition,
    description: "an unusual burst of peers leaving in a short window",
    when_state: None,
};
impl Rule for PeerChurnSpike {
    fn header(&self) -> &RuleHeader {
        &PEER_CHURN_SPIKE
    }
    fn replay(&self, events: &[SessionEvent]) -> Vec<Verdict> {
        let leaves: Vec<f64> = events
            .iter()
            .filter(|e| matches!(e.payload, EventPayload::PeerLeft { .. }))
            .map(|e| e.t_mono_secs)
            .collect();
        // Any window [t, t+W] containing more than the limit is a spike.
        for (i, &t0) in leaves.iter().enumerate() {
            let count = leaves[i..]
                .iter()
                .take_while(|&&t| t - t0 <= CHURN_WINDOW_SECS)
                .count();
            if count > CHURN_LIMIT {
                return vec![Verdict::violated(format!(
                    "{count} peers left within {CHURN_WINDOW_SECS:.0}s"
                ))];
            }
        }
        Vec::new()
    }
}

// --- OfferAcceptanceAnomaly -------------------------------------------------
const MIN_OFFERS_FOR_RATIO: usize = 10;

struct OfferAcceptanceAnomaly;
const OFFER_ACCEPTANCE_ANOMALY: RuleHeader = RuleHeader {
    id: "net.offer_acceptance_anomaly",
    subsystem: Subsystem::Network,
    severity: Severity::Warn,
    debounce: DebouncePolicy::OncePerCondition,
    description: "offer accept/decline ratio is extreme (possible automation)",
    when_state: None,
};
impl Rule for OfferAcceptanceAnomaly {
    fn header(&self) -> &RuleHeader {
        &OFFER_ACCEPTANCE_ANOMALY
    }
    fn replay(&self, events: &[SessionEvent]) -> Vec<Verdict> {
        let (mut acc, mut total) = (0usize, 0usize);
        for e in events {
            if let EventPayload::ItemOfferUserResponded { accepted, .. } = &e.payload {
                total += 1;
                acc += usize::from(*accepted);
            }
        }
        if total < MIN_OFFERS_FOR_RATIO {
            return Vec::new();
        }
        let ratio = acc as f64 / total as f64;
        if !(0.1..=0.9).contains(&ratio) {
            vec![Verdict::violated(format!(
                "accept ratio {ratio:.0} over {total} offers"
            ))]
        } else {
            Vec::new()
        }
    }
}

// --- IdentitySpoofBurst -----------------------------------------------------
const SPOOF_LIMIT: u64 = 3;

struct IdentitySpoofBurst;
const IDENTITY_SPOOF_BURST: RuleHeader = RuleHeader {
    id: "net.identity_spoof_burst",
    subsystem: Subsystem::Network,
    severity: Severity::Warn,
    debounce: DebouncePolicy::Interval(30.0),
    description: "repeated spoofed identity claims from peers",
    when_state: None,
};
impl Rule for IdentitySpoofBurst {
    fn header(&self) -> &RuleHeader {
        &IDENTITY_SPOOF_BURST
    }
    fn eval(&self, cx: &LiveCtx) -> Option<Verdict> {
        let n = cx
            .metrics
            .counter(names::NET_IDENTITY_SPOOFED_COUNT)?
            .value();
        Some(if n >= SPOOF_LIMIT {
            Verdict::violated(format!("{n} spoofed identity claims"))
        } else {
            Verdict::Clear
        })
    }
    fn replay(&self, events: &[SessionEvent]) -> Vec<Verdict> {
        let n = events
            .iter()
            .filter(|e| matches!(e.payload, EventPayload::PeerIdentitySpoofRejected { .. }))
            .count();
        if n as u64 >= SPOOF_LIMIT {
            vec![Verdict::violated(format!("{n} spoofed identity claims"))]
        } else {
            Vec::new()
        }
    }
}

// --- SilentDecodeFailure ----------------------------------------------------
struct SilentDecodeFailure;
const SILENT_DECODE_FAILURE: RuleHeader = RuleHeader {
    id: "net.silent_decode_failure",
    subsystem: Subsystem::Network,
    severity: Severity::Error,
    debounce: DebouncePolicy::OncePerCondition,
    description: "a peer payload failed to decode and was silently dropped",
    when_state: None,
};
impl Rule for SilentDecodeFailure {
    fn header(&self) -> &RuleHeader {
        &SILENT_DECODE_FAILURE
    }
    fn replay(&self, events: &[SessionEvent]) -> Vec<Verdict> {
        events
            .iter()
            .filter_map(|e| match &e.payload {
                EventPayload::AvatarStateDecodeFailed { .. } => {
                    Some(Verdict::violated("avatar-state decode failed"))
                }
                EventPayload::RoomStateDecodeFailed { .. } => {
                    Some(Verdict::violated("room-state decode failed"))
                }
                EventPayload::ItemOfferDecodeFailed { .. } => {
                    Some(Verdict::violated("item-offer decode failed"))
                }
                _ => None,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(t: f64, payload: EventPayload) -> SessionEvent {
        SessionEvent::new(0, t, None, Severity::Info, payload)
    }

    #[test]
    fn gate_stall_replay_flags_a_long_gate() {
        let over = vec![
            ev(0.0, EventPayload::LoadingPhaseStarted),
            ev(
                200.0,
                EventPayload::LoadingGateTransitionToInGame {
                    elapsed_secs: 200.0,
                },
            ),
        ];
        assert_eq!(LoadingGateStall.replay(&over).len(), 1);
        let ok = vec![
            ev(0.0, EventPayload::LoadingPhaseStarted),
            ev(
                5.0,
                EventPayload::LoadingGateTransitionToInGame { elapsed_secs: 5.0 },
            ),
        ];
        assert!(LoadingGateStall.replay(&ok).is_empty());
        // Never transitioned but log continued past budget → stall.
        let never = vec![
            ev(0.0, EventPayload::LoadingPhaseStarted),
            ev(200.0, EventPayload::RoomStateApplied),
        ];
        assert_eq!(LoadingGateStall.replay(&never).len(), 1);
    }

    #[test]
    fn task_never_resolves_pairs_by_job_name() {
        let events = vec![
            ev(
                0.0,
                EventPayload::OffloadJobStarted {
                    job: "heightmap".into(),
                },
            ),
            ev(
                1.0,
                EventPayload::OffloadJobStarted {
                    job: "ambient".into(),
                },
            ),
            ev(
                2.0,
                EventPayload::OffloadJobCompleted {
                    job: "heightmap".into(),
                    duration_secs: 2.0,
                },
            ),
            ev(100.0, EventPayload::RoomStateApplied),
        ];
        // ambient never resolved and the log ran 100s past its start.
        let v = TaskNeverResolves.replay(&events);
        assert_eq!(v.len(), 1);
        assert!(matches!(&v[0], Verdict::Violated { detail } if detail.contains("ambient")));
    }

    #[test]
    fn offer_ratio_flags_extremes_only_above_min_count() {
        // 12 offers, all accepted → ratio 1.0 → anomalous.
        let all_accept: Vec<_> = (0..12)
            .map(|i| {
                ev(
                    i as f64,
                    EventPayload::ItemOfferUserResponded {
                        offer_id: i,
                        accepted: true,
                    },
                )
            })
            .collect();
        assert_eq!(OfferAcceptanceAnomaly.replay(&all_accept).len(), 1);
        // Too few offers → no verdict even if all accepted.
        let few: Vec<_> = (0..3)
            .map(|i| {
                ev(
                    i as f64,
                    EventPayload::ItemOfferUserResponded {
                        offer_id: i,
                        accepted: true,
                    },
                )
            })
            .collect();
        assert!(OfferAcceptanceAnomaly.replay(&few).is_empty());
    }

    #[test]
    fn spoof_and_decode_rules_fire_from_events() {
        let spoofs: Vec<_> = (0..3)
            .map(|_| {
                ev(
                    0.0,
                    EventPayload::PeerIdentitySpoofRejected {
                        peer: "p".into(),
                        claimed_did: "a".into(),
                        authenticated_did: "b".into(),
                    },
                )
            })
            .collect();
        assert_eq!(IdentitySpoofBurst.replay(&spoofs).len(), 1);

        let decode = vec![ev(
            0.0,
            EventPayload::RoomStateDecodeFailed {
                sender_did: "d".into(),
                error: "bad".into(),
            },
        )];
        assert_eq!(SilentDecodeFailure.replay(&decode).len(), 1);
    }
}
